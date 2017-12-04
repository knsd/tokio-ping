use std::io;

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::rc::Rc;
use std::time::Duration;

use futures::future;
use futures::sync::oneshot;
use futures::{Future, Poll, Async};
use lazy_socket::raw::{Family, Protocol, Type};
use rand::{Rng, StdRng};
use time::precise_time_s;
use tokio_core::reactor::{Handle, Timeout};

use errors::{Error, ErrorKind};
use packet::IcmpMessage;
use socket::Socket;

const DEFAULT_TIMEOUT: u64 = 2;
type OpaqueRef = [u8; 32];


#[derive(Clone)]
struct PingState {
    inner: Rc<RefCell<HashMap<OpaqueRef, oneshot::Sender<f64>>>>,
}

impl PingState {
    fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    fn insert(&self, key: OpaqueRef, value: oneshot::Sender<f64>) {
        self.inner.borrow_mut().insert(key, value);
    }

    fn remove(&self, key: &[u8]) -> Option<oneshot::Sender<f64>> {
        self.inner.borrow_mut().remove(key)
    }
}

pub struct PingFuture {
    start_time: f64,
    inner: Box<Future<Item=Option<f64>, Error=Error>>,
    state: PingState,
    opaque_ref: OpaqueRef,
}

impl PingFuture {
    fn new(future: Box<Future<Item=Option<f64>, Error=Error>>, state: PingState, opaque_ref: OpaqueRef) -> Self {
        PingFuture {
            start_time: precise_time_s(),
            inner: future,
            state: state,
            opaque_ref: opaque_ref
        }
    }
}

impl Future for PingFuture {
    type Item = Option<f64>;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.inner.poll() {
            Ok(Async::Ready(Some(stop_time))) => Ok(Async::Ready(Some(stop_time - self.start_time))),
            Ok(Async::Ready(None)) => Ok(Async::Ready(None)),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => Err(err)
        }
    }
}

impl Drop for PingFuture {
    fn drop(&mut self) {
        self.state.remove(&self.opaque_ref);
    }
}

pub struct PingChain<'a> {
    ping: &'a mut Ping,
    hostname: Ipv4Addr,
    ident: Option<u16>,
    seq_cnt: Option<u16>,
    timeout: Option<Duration>,
}

impl<'a> PingChain<'a> {
    fn new(ping: &'a mut Ping, hostname: Ipv4Addr) -> Self {
        Self {
            ping: ping,
            hostname: hostname,
            ident: None,
            seq_cnt: None,
            timeout: None,
        }
    }

    pub fn ident(&mut self, ident: u16) -> &mut Self {
        self.ident = Some(ident);
        self
    }

    pub fn seq_cnt(&mut self, seq_cnt: u16) -> &mut Self {
        self.seq_cnt = Some(seq_cnt);
        self
    }

    pub fn timeout(&mut self, timeout: Duration) -> &mut Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn send(&mut self) -> PingFuture {
        let timeout = self.timeout.unwrap_or_else(|| Duration::from_secs(DEFAULT_TIMEOUT));
        let ident = match self.ident {
            Some(ident) => ident,
            None => {
                let ident = self.ping.rng.gen();
                self.ident = Some(ident);
                ident
            }
        };

        let seq_cnt = match self.seq_cnt {
            Some(seq_cnt) => {
                self.seq_cnt = Some(seq_cnt + 1);
                seq_cnt
            },
            None => {
                self.seq_cnt = Some(0);
                0
            }
        };

        self.ping.ping(self.hostname, ident, seq_cnt, timeout)
    }
}

pub struct Ping {
    socket: Socket,
    state: PingState,
    handle: Handle,
    rng: StdRng,
    _finalize: oneshot::Sender<()>,
}

impl Ping {
    pub fn new(handle: &Handle) -> io::Result<Self> {
        let socket = Socket::new(Family::IPv4, Type::RAW, Protocol::ICMPv4, handle)?;

        let state = PingState::new();

        let (receiver, finalize) = Receiver::new(socket.clone(), state.clone());

        handle.spawn(receiver);

        Ok(Self {
            socket: socket,
            state: state,
            handle: handle.clone(),
            rng: StdRng::new()?,
            _finalize: finalize,
        })
    }

    pub fn chain<'a>(&'a mut self, hostname: Ipv4Addr) -> PingChain<'a> {
        PingChain::new(self, hostname)
    }

    pub fn ping(&mut self, hostname: Ipv4Addr, ident: u16, seq_cnt: u16, timeout: Duration) -> PingFuture {
        let (sender, receiver) = oneshot::channel();

        let timeout_future = future::result(Timeout::new(timeout, &self.handle))
            .flatten().map_err(From::from).map(|()| None);

        let send_future = receiver.and_then(|time| {
            Ok(Some(time))
        }).map_err(|_| ErrorKind::PingInternalError.into());

        let future = timeout_future.select(send_future)
            .map(|(item, _next)| item)
            .map_err(|(item, _next)| item);

        let opaque_ref_bytes: OpaqueRef = self.rng.gen();
        self.state.insert(opaque_ref_bytes, sender);

        let dest = SocketAddr::new(hostname.into(), 1);

        let socket = self.socket.clone();
        self.handle.spawn_fn(move ||{
            let packet = IcmpMessage::echo_request(ident, seq_cnt, &opaque_ref_bytes);
            socket.send_to(packet.encode(), &dest).then(|_| Ok(()))
        });

        PingFuture::new(Box::new(future), self.state.clone(), opaque_ref_bytes)
    }
}

struct Receiver {
    socket: Socket,
    finalize: oneshot::Receiver<()>,
    state: PingState,
    buffer: [u8; 2048]
}

impl Receiver {
    fn new(socket: Socket, state: PingState) -> (Self, oneshot::Sender<()>) {
        let (finalize_sender, finalize_receiver) = oneshot::channel();

        let receiver = Self {
            socket: socket,
            finalize: finalize_receiver,
            state: state,
            buffer: [0; 2048],
        };

        (receiver, finalize_sender)
    }
}

impl Future for Receiver {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.finalize.poll() {
            Ok(Async::NotReady) => (),
            _ => return Ok(Async::Ready(())),
        }

        match self.socket.recv(&mut self.buffer) {
            Ok(Async::Ready(bytes)) => {
                if bytes >= 20 {
                    if let Ok(IcmpMessage::EchoReply(reply)) = IcmpMessage::decode(&self.buffer[20 .. bytes]) {
                        let now = precise_time_s();
                        if let Some(sender) = self.state.remove(&reply.payload) {
                            sender.send(now).unwrap_or_default()
                        }
                    }
                }
                self.poll()
            },
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(_) => Err(()),
        }
    }
}
