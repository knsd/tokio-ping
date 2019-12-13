use std::io;

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Instant, Duration};
use std::task::{Poll, Context};
use std::pin::Pin;

use futures::future::{Future, FutureExt, select};
use futures::stream::Stream;
use futures::channel::oneshot;
use rand::random;
use parking_lot::Mutex;
use socket2::{Domain, Protocol, Type};

use tokio::time::{delay_until, Delay};

use crate::errors::{Error, ErrorKind};
use crate::packet::{IpV4Packet, IpV4Protocol};
use crate::packet::{ICMP_HEADER_SIZE, IcmpV4, IcmpV6, EchoRequest, EchoReply};
use crate::socket::{Socket, Send};

const DEFAULT_TIMEOUT: u64 = 2;
const TOKEN_SIZE: usize = 24;
const ECHO_REQUEST_BUFFER_SIZE: usize = ICMP_HEADER_SIZE + TOKEN_SIZE;
type Token = [u8; TOKEN_SIZE];
type EchoRequestBuffer = [u8; ECHO_REQUEST_BUFFER_SIZE];

#[derive(Clone)]
struct PingState {
    inner: Arc<Mutex<HashMap<Token, oneshot::Sender<Instant>>>>,
}

impl PingState {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn insert(&self, key: Token, value: oneshot::Sender<Instant>) {
        self.inner.lock().insert(key, value);
    }

    fn remove(&self, key: &[u8]) -> Option<oneshot::Sender<Instant>> {
        self.inner.lock().remove(key)
    }
}

/// Represent a future that resolves into ping response time, resolves into `None` if timed out.
#[must_use = "futures do nothing unless polled"]
pub struct PingFuture {
    inner: PingFutureKind
}

enum PingFutureKind {
    Normal(NormalPingFutureKind),
    PacketEncodeError,
    InvalidProtocol,
}

struct NormalPingFutureKind {
    start_time: Instant,
    state: PingState,
    token: Token,
    delay: Delay,
    send: Option<Send<EchoRequestBuffer>>,
    receiver: oneshot::Receiver<Instant>,
}

impl Future for PingFuture {
    type Output = Result<Option<Duration>, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.inner {
            PingFutureKind::Normal(ref mut normal) => {
                let mut swap_send = false;

                if let Some(ref mut send) = normal.send {
                    match Pin::new(send).poll(cx) {
                        Poll::Pending => (),
                        Poll::Ready(Ok(_)) => swap_send = true,
                        Poll::Ready(Err(_)) => return Poll::Ready(Err(ErrorKind::InternalError.into())),
                    }
                }


                if swap_send {
                    normal.send = None;
                }

                match Pin::new(&mut normal.receiver).poll(cx) {
                    Poll::Pending => (),
                    Poll::Ready(Ok(stop_time)) => {
                        return Poll::Ready(Ok(Some(stop_time - normal.start_time)))
                    },
                    Poll::Ready(Err(_)) => return Poll::Ready(Err(ErrorKind::InternalError.into())),
                }

                match Pin::new(&mut normal.delay).poll(cx) {
                    Poll::Pending => (),
                    Poll::Ready(_) => return Poll::Ready(Ok(None)),
                }
            }
            PingFutureKind::InvalidProtocol => {
                return Poll::Ready(Err(ErrorKind::InvalidProtocol.into()))
            }
            PingFutureKind::PacketEncodeError => {
                return Poll::Ready(Err(ErrorKind::InternalError.into()))
            }
        }
        Poll::Pending
    }
}

impl Drop for PingFuture {
    fn drop(&mut self) {
        match self.inner {
            PingFutureKind::Normal(ref normal) => {
                normal.state.remove(&normal.token);
            }
            | PingFutureKind::InvalidProtocol
            | PingFutureKind::PacketEncodeError => (),
        }
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
    future: Option<PingFuture>,
}

impl PingChainStream {
    fn new(chain: PingChain) -> Self {
        Self {
            chain: chain,
            future: None,
        }
    }
}

impl Stream for PingChainStream {
    type Item = Result<Option<Duration>, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut future = self.future.take().unwrap_or_else(|| self.chain.send());

        match Pin::new(&mut future).poll(cx) {
            Poll::Ready(result) => Poll::Ready(Some(result)),
            Poll::Pending => {
                self.future = Some(future);
                Poll::Pending
            },
        }
    }
}

/// ICMP packets sender and receiver.
#[derive(Clone)]
pub struct Pinger {
    inner: Arc<PingInner>,
}

struct PingInner {
    sockets: Sockets,
    state: PingState,
    _v4_finalize: Option<oneshot::Sender<()>>,
    _v6_finalize: Option<oneshot::Sender<()>>,
}

enum Sockets {
    V4(Socket),
    V6(Socket),
    Both { v4: Socket, v6: Socket },
}

impl Sockets {
    fn new() -> io::Result<Self> {
        let mb_v4socket = Socket::new(Domain::ipv4(), Type::raw(), Protocol::icmpv4());
        let mb_v6socket = Socket::new(Domain::ipv6(), Type::raw(), Protocol::icmpv6());
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
    pub async fn new() -> Result<Self, Error> {
        let sockets = Sockets::new()?;

        let state = PingState::new();

        let v4_finalize = if let Some(v4_socket) = sockets.v4() {
            let (s, r) = oneshot::channel();
            let receiver =
                Receiver::<IcmpV4>::new(v4_socket.clone(), state.clone());
            tokio::spawn(select(receiver, r).map(|_| ()));
            Some(s)
        } else {
            None
        };

        let v6_finalize = if let Some(v6_socket) = sockets.v6() {
            let (s, r) = oneshot::channel();
            let receiver =
                Receiver::<IcmpV6>::new(v6_socket.clone(), state.clone());
            tokio::spawn(select(receiver, r).map(|_| ()));
            Some(s)
        } else {
            None
        };

        let inner = PingInner {
            sockets: sockets,
            state: state,
            _v4_finalize: v4_finalize,
            _v6_finalize: v6_finalize,
        };

        Ok(Self {
            inner: Arc::new(inner),
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

        let deadline = Instant::now() + timeout;

        let token = random();
        self.inner.state.insert(token, sender);

        let dest = SocketAddr::new(hostname, 0);
        let mut buffer = [0; ECHO_REQUEST_BUFFER_SIZE];

        let request = EchoRequest {
            ident: ident,
            seq_cnt: seq_cnt,
            payload: &token,
        };

        let (encode_result, mb_socket) = {
            if dest.is_ipv4() {
                (
                    request.encode::<IcmpV4>(&mut buffer[..]),
                    self.inner.sockets.v4().cloned()
                )
            } else {
                (
                    request.encode::<IcmpV6>(&mut buffer[..]),
                    self.inner.sockets.v6().cloned(),
                )
            }
        };


        let socket = match mb_socket {
            Some(socket) => socket,
            None => {
                return PingFuture {
                    inner: PingFutureKind::InvalidProtocol
                }
            }
        };

        if let Err(_) = encode_result {
            return PingFuture {
                inner: PingFutureKind::PacketEncodeError
            }
        }

        let send_future = socket.send_to(buffer, &dest);

        PingFuture {
            inner: PingFutureKind::Normal(NormalPingFutureKind {
                start_time: Instant::now(),
                state: self.inner.state.clone(),
                token: token,
                delay: delay_until(deadline.into()),
                send: Some(send_future),
                receiver: receiver,
            })
        }
    }
}

struct Receiver<Message> {
    socket: Socket,
    state: PingState,
    _phantom: ::std::marker::PhantomData<Message>,
}

trait ParseReply {
    fn reply_payload(data: &[u8]) -> Option<&[u8]>;
}

impl ParseReply for IcmpV4 {
    fn reply_payload(data: &[u8]) -> Option<&[u8]> {
        if let Ok(ipv4_packet) = IpV4Packet::decode(data) {
            if ipv4_packet.protocol != IpV4Protocol::Icmp {
                return None;
            }

            if let Ok(reply) = EchoReply::decode::<IcmpV4>(ipv4_packet.data) {
                return Some(reply.payload);
            }
        }
        None
    }
}

impl ParseReply for IcmpV6 {
    fn reply_payload(data: &[u8]) -> Option<&[u8]> {
        if let Ok(reply) = EchoReply::decode::<IcmpV6>(data) {
            return Some(reply.payload);
        }
        None
    }
}

impl<Proto> Receiver<Proto> {
    fn new(socket: Socket, state: PingState) -> Self {
        Self {
            socket: socket,
            state: state,
            _phantom: ::std::marker::PhantomData,
        }
    }
}

impl<Message: ParseReply> Future for Receiver<Message> {
    type Output = Result<(), ()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut buffer = [0; 2048];
        match self.socket.recv(&mut buffer, cx) {
            Poll::Ready(Ok(bytes)) => {
                if let Some(payload) = Message::reply_payload(&buffer[..bytes]) {
                    let now = Instant::now();
                    if let Some(sender) = self.state.remove(payload) {
                        sender.send(now).unwrap_or_default()
                    }
                }
                self.poll(cx)
            }
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(_)) => Poll::Ready(Err(())),
        }
    }
}
