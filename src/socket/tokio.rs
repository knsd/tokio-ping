use std::io;
use std::sync::Arc;
use std::task::{Poll, Context};
use std::pin::Pin;

use std::future::Future;
use std::net::SocketAddr;
use ::mio::Ready;
use tokio::io::PollEvented;
use socket2::{Domain, Protocol, SockAddr, Type};

use super::mio;

#[derive(Clone)]
pub struct Socket {
    socket: Arc<PollEvented<mio::Socket>>,
}

impl Socket {
    pub fn new(
        domain: Domain,
        type_: Type,
        protocol: Protocol,
    ) -> io::Result<Self> {
        let socket = mio::Socket::new(domain, type_, protocol)?;
        let socket = PollEvented::new(socket)?;
        Ok(Self {
            socket: Arc::new(socket),
        })
    }

    pub fn send_to<T>(&self, buf: T, target: &SocketAddr) -> Send<T>
    where
        T: AsRef<[u8]>,
    {
        Send {
            state: SendState::Writing {
                socket: self.socket.clone(),
                addr: target.clone().into(),
                buf: buf,
            },
        }
    }

    pub fn recv(&self, buffer: &mut [u8], cx: &mut Context<'_>) -> Poll<Result<usize, io::Error>> {

        match self.socket.poll_read_ready(cx, Ready::readable()) {
            Poll::Ready(Ok(_)) => (),
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        }

        match self.socket.get_ref().recv(buffer) {
            Ok(n) => Poll::Ready(Ok(n.into())),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.socket.clear_read_ready(cx, Ready::readable())?;
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

pub struct Send<T> {
    state: SendState<T>,
}

enum SendState<T> {
    Writing {
        socket: Arc<PollEvented<mio::Socket>>,
        buf: T,
        addr: SockAddr,
    },
    Empty,
}

fn send_to(
    socket: &Arc<PollEvented<mio::Socket>>,
    buf: &[u8],
    target: &SockAddr,
    cx: &mut Context<'_>
) -> Poll<Result<usize, io::Error>> {
    match socket.poll_write_ready(cx) {
        Poll::Ready(Ok(_)) => (),
        Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
        Poll::Pending => return Poll::Pending
    }

    match socket.get_ref().send_to(buf, target) {
        Ok(n) => Poll::Ready(Ok(n.into())),
        Err(e) => {
            if e.kind() == io::ErrorKind::WouldBlock {
                socket.clear_write_ready(cx)?;
                Poll::Pending
            } else {
                Poll::Ready(Err(e))
            }
        }
    }
}

impl<T> Future for Send<T>
where
    T: AsRef<[u8]> + Unpin,
{
    type Output = Result<(), io::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.state {
            SendState::Writing {
                ref socket,
                ref buf,
                ref addr,
            } => {
                let n = match send_to(socket, buf.as_ref(), addr, cx) {
                    Poll::Ready(Ok(n)) => n,
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                    Poll::Pending => return Poll::Pending,
                };
                if n != buf.as_ref().len() {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::Other,
                        "failed to send entire packet",
                    )));
                }
            }
            SendState::Empty => panic!("poll a Send after it's done"),
        }

        match ::std::mem::replace(&mut self.state, SendState::Empty) {
            SendState::Writing { .. } => Poll::Ready(Ok(())),
            SendState::Empty => unreachable!(),
        }
    }
}
