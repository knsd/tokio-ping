use std::io;

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::rc::{Rc, Weak};
use std::time::Duration;

use futures::future;
use futures::{Async, Future, Poll, Stream};
use futures::sync::oneshot;
use rand::random;
use socket2::{Domain, Protocol, Type};
use time::precise_time_s;
use tokio_core::reactor::{Handle, Timeout};

use errors::{Error, ErrorKind};
use packet::{IcmpV4Message, IcmpV6Message, IpV4Packet, IpV4Protocol};
use socket::Socket;

const DEFAULT_TIMEOUT: u64 = 2;
type Token = [u8; 32];

#[derive(Clone)]
struct PingState {
    inner: Rc<RefCell<HashMap<Token, oneshot::Sender<f64>>>>,
}

impl PingState {
    fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    fn insert(&self, key: Token, value: oneshot::Sender<f64>) {
        self.inner.borrow_mut().insert(key, value);
    }

    fn remove(&self, key: &[u8]) -> Option<oneshot::Sender<f64>> {
        self.inner.borrow_mut().remove(key)
    }
}

/// Represent a future that resolves into ping response time, resolves into `None` if timed out.
#[must_use = "futures do nothing unless polled"]
pub struct PingFuture {
    start_time: f64,
    inner: Box<Future<Item = Option<f64>, Error = Error>>,
    pinger: Pinger,
    token: Token,
}

impl PingFuture {
    fn new(
        future: Box<Future<Item = Option<f64>, Error = Error>>,
        pinger: Pinger,
        token: Token,
    ) -> Self {
        PingFuture {
            start_time: precise_time_s(),
            inner: future,
            pinger: pinger,
            token: token,
        }
    }
}

impl Future for PingFuture {
    type Item = Option<f64>;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.inner.poll() {
            Ok(Async::Ready(Some(stop_time))) => {
                Ok(Async::Ready(Some(stop_time - self.start_time)))
            }
            Ok(Async::Ready(None)) => Ok(Async::Ready(None)),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => Err(err),
        }
    }
}

impl Drop for PingFuture {
    fn drop(&mut self) {
        self.pinger.inner.state.remove(&self.token);
    }
}

/// Ping the same host several times.
pub struct PingChain {
    pinger: Pinger,
    hostname: IpAddr,
    ident: Option<u16>,
    seq_cnt: Option<u16>,
    timeout: Option<Duration>,
}

impl PingChain {
    fn new(pinger: Pinger, hostname: IpAddr) -> Self {
        Self {
            pinger: pinger,
            hostname: hostname,
            ident: None,
            seq_cnt: None,
            timeout: None,
        }
    }

    /// Set ICMP ident. Default value is randomized.
    pub fn ident(mut self, ident: u16) -> Self {
        self.ident = Some(ident);
        self
    }

    /// Set ICMP seq_cnt, this value will be incremented by one for every `send`.
    /// Default value is 0.
    pub fn seq_cnt(mut self, seq_cnt: u16) -> Self {
        self.seq_cnt = Some(seq_cnt);
        self
    }

    /// Set ping timeout. Default timeout is two seconds.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Send ICMP request and wait for response.
    pub fn send(&mut self) -> PingFuture {
        let ident = match self.ident {
            Some(ident) => ident,
            None => {
                let ident = random();
                self.ident = Some(ident);
                ident
            }
        };

        let seq_cnt = match self.seq_cnt {
            Some(seq_cnt) => {
                self.seq_cnt = Some(seq_cnt.wrapping_add(1));
                seq_cnt
            }
            None => {
                self.seq_cnt = Some(1);
                0
            }
        };

        let timeout = match self.timeout {
            Some(timeout) => timeout,
            None => {
                let timeout = Duration::from_secs(DEFAULT_TIMEOUT);
                self.timeout = Some(timeout);
                timeout
            }
        };

        self.pinger.ping(self.hostname, ident, seq_cnt, timeout)
    }

    /// Create infinite stream of ping response times.
    pub fn stream(self) -> PingChainStream {
        PingChainStream::new(self)
    }
}

/// Stream of sequential ping response times, iterates `None` if timed out.
pub struct PingChainStream {
    chain: PingChain,
    future: PingFuture,
}

impl PingChainStream {
    fn new(mut chain: PingChain) -> Self {
        let future = chain.send();
        Self {
            chain: chain,
            future: future,
        }
    }
}

impl Stream for PingChainStream {
    type Item = Option<f64>;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.future.poll() {
            Ok(Async::Ready(item)) => {
                self.future = self.chain.send();
                Ok(Async::Ready(Some(item)))
            }
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => Err(err),
        }
    }
}

struct Finalize {
    inner: Rc<()>,
}

impl Finalize {
    fn new() -> Self {
        Self { inner: Rc::new(()) }
    }

    fn handle(&self) -> FinalizeHandle {
        FinalizeHandle {
            inner: Rc::downgrade(&self.inner),
        }
    }
}

struct FinalizeHandle {
    inner: Weak<()>,
}

impl FinalizeHandle {
    fn is_alive(&self) -> bool {
        self.inner.upgrade().is_some()
    }
}

/// ICMP packets sender and receiver.
#[derive(Clone)]
pub struct Pinger {
    inner: Rc<PingInner>,
}

struct PingInner {
    sockets: Sockets,
    state: PingState,
    handle: Handle,
    _finalize: Finalize,
}

enum Sockets {
    V4(Socket),
    V6(Socket),
    Both { v4: Socket, v6: Socket },
}

impl Sockets {
    fn new(handle: &Handle) -> io::Result<Self> {
        let mb_v4socket = Socket::new(Domain::ipv4(), Type::raw(), Protocol::icmpv4(), handle);
        let mb_v6socket = Socket::new(Domain::ipv6(), Type::raw(), Protocol::icmpv6(), handle);
        match (mb_v4socket, mb_v6socket) {
            (Ok(v4_socket), Ok(v6_socket)) => Ok(Sockets::Both {
                v4: v4_socket,
                v6: v6_socket,
            }),
            (Ok(v4_socket), Err(_)) => Ok(Sockets::V4(v4_socket)),
            (Err(_), Ok(v6_socket)) => Ok(Sockets::V6(v6_socket)),
            (Err(err), Err(_)) => Err(err),
        }
    }

    fn v4(&self) -> Option<&Socket> {
        match *self {
            Sockets::V4(ref socket) => Some(socket),
            Sockets::Both { ref v4, .. } => Some(v4),
            Sockets::V6(_) => None,
        }
    }

    fn v6(&self) -> Option<&Socket> {
        match *self {
            Sockets::V4(_) => None,
            Sockets::Both { ref v6, .. } => Some(v6),
            Sockets::V6(ref socket) => Some(socket),
        }
    }
}

impl Pinger {
    /// Create new `Pinger` instance, will fail if unable to create both IPv4 and IPv6 sockets.
    pub fn new(handle: &Handle) -> io::Result<Self> {
        let sockets = Sockets::new(handle)?;

        let state = PingState::new();
        let finalize = Finalize::new();

        if let Some(v4_socket) = sockets.v4() {
            let receiver =
                Receiver::<IcmpV4Message>::new(v4_socket.clone(), state.clone(), finalize.handle());
            handle.spawn(receiver);
        }

        if let Some(v6_socket) = sockets.v6() {
            let receiver =
                Receiver::<IcmpV6Message>::new(v6_socket.clone(), state.clone(), finalize.handle());
            handle.spawn(receiver);
        }

        let inner = PingInner {
            sockets: sockets,
            state: state,
            handle: handle.clone(),
            _finalize: finalize,
        };

        Ok(Self {
            inner: Rc::new(inner),
        })
    }

    /// Ping the same host several times.
    pub fn chain(&self, hostname: IpAddr) -> PingChain {
        PingChain::new(self.clone(), hostname)
    }

    /// Send ICMP request and wait for response.
    pub fn ping(
        &self,
        hostname: IpAddr,
        ident: u16,
        seq_cnt: u16,
        timeout: Duration,
    ) -> PingFuture {
        let (sender, receiver) = oneshot::channel();

        let timeout_future = future::result(Timeout::new(timeout, &self.inner.handle))
            .flatten()
            .map_err(From::from)
            .map(|()| None);

        let send_future = receiver
            .and_then(|time| Ok(Some(time)))
            .map_err(|_| ErrorKind::PingInternalError.into());

        let future = timeout_future
            .select(send_future)
            .map(|(item, _next)| item)
            .map_err(|(item, _next)| item);

        let token = random();
        self.inner.state.insert(token, sender);

        let dest = SocketAddr::new(hostname, 0);

        let (mb_socket, packet) = {
            if dest.is_ipv4() {
                (
                    self.inner.sockets.v4().cloned(),
                    IcmpV4Message::echo_request(ident, seq_cnt, &token).encode(),
                )
            } else {
                (
                    self.inner.sockets.v6().cloned(),
                    IcmpV6Message::echo_request(ident, seq_cnt, &token).encode(),
                )
            }
        };

        let socket = match mb_socket {
            Some(socket) => socket,
            None => {
                return PingFuture::new(
                    Box::new(future::err(ErrorKind::InvalidProtocol.into())),
                    self.clone(),
                    token,
                )
            }
        };

        self.inner
            .handle
            .spawn_fn(move || socket.send_to(packet, &dest).then(|_| Ok(())));

        PingFuture::new(Box::new(future), self.clone(), token)
    }
}

struct Receiver<Message> {
    socket: Socket,
    finalize: FinalizeHandle,
    state: PingState,
    buffer: [u8; 2048],
    _phantom: ::std::marker::PhantomData<Message>,
}

trait ParseReply {
    fn reply_payload(data: &[u8]) -> Option<&[u8]>;
}

impl<'b> ParseReply for IcmpV4Message<'b> {
    fn reply_payload(data: &[u8]) -> Option<&[u8]> {
        if let Ok(ipv4_packet) = IpV4Packet::decode(data) {
            if ipv4_packet.protocol != IpV4Protocol::Icmp {
                return None;
            }

            if let Ok(IcmpV4Message::EchoReply(reply)) = IcmpV4Message::decode(ipv4_packet.data) {
                return Some(reply.payload);
            }
        }
        None
    }
}

impl<'b> ParseReply for IcmpV6Message<'b> {
    fn reply_payload(data: &[u8]) -> Option<&[u8]> {
        if let Ok(IcmpV6Message::EchoReply(reply)) = IcmpV6Message::decode(data) {
            return Some(reply.payload);
        }
        None
    }
}

impl<Proto> Receiver<Proto> {
    fn new(socket: Socket, state: PingState, finalize: FinalizeHandle) -> Self {
        Self {
            socket: socket,
            finalize: finalize,
            state: state,
            buffer: [0; 2048],
            _phantom: ::std::marker::PhantomData,
        }
    }
}

impl<Message: ParseReply> Future for Receiver<Message> {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if !self.finalize.is_alive() {
            return Ok(Async::Ready(()));
        }

        match self.socket.recv(&mut self.buffer) {
            Ok(Async::Ready(bytes)) => {
                if let Some(payload) = Message::reply_payload(&self.buffer[..bytes]) {
                    let now = precise_time_s();
                    if let Some(sender) = self.state.remove(payload) {
                        sender.send(now).unwrap_or_default()
                    }
                }
                self.poll()
            }
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(_) => Err(()),
        }
    }
}
