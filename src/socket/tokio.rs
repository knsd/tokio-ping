use std::io;
use std::sync::Arc;

use futures;
use std::net::SocketAddr;
use mio::Ready;
use tokio_reactor::{Handle, PollEvented};
use socket2::{Domain, Protocol, SockAddr, Type};

use socket::mio;

#[derive(Clone)]
pub struct Socket {
    socket: Arc<PollEvented<mio::Socket>>,
}

impl Socket {
    pub fn new(
        domain: Domain,
        type_: Type,
        protocol: Protocol,
        handle: &Handle,
    ) -> io::Result<Self> {
        let socket = mio::Socket::new(domain, type_, protocol)?;
        let socket = PollEvented::new_with_handle(socket, handle)?;
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
                addr: (*target).into(),
                buf,
            },
        }
    }

    pub fn recv(&self, buffer: &mut [u8]) -> futures::Poll<usize, io::Error> {
        try_ready!(self.socket.poll_read_ready(Ready::readable()));

        match self.socket.get_ref().recv(buffer) {
            Ok(n) => Ok(n.into()),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.socket.clear_read_ready(Ready::readable())?;
                Ok(futures::Async::NotReady)
            }
            Err(e) => Err(e),
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
) -> futures::Poll<usize, io::Error> {
    try_ready!(socket.poll_write_ready());

    match socket.get_ref().send_to(buf, target) {
        Ok(n) => Ok(n.into()),
        Err(e) => {
            if e.kind() == io::ErrorKind::WouldBlock {
                socket.clear_write_ready()?;
                Ok(futures::Async::NotReady)
            } else {
                Err(e)
            }
        }
    }
}

impl<T> futures::Future for Send<T>
where
    T: AsRef<[u8]>,
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> futures::Poll<(), io::Error> {
        match self.state {
            SendState::Writing {
                ref socket,
                ref buf,
                ref addr,
            } => {
                let n = try_ready!(send_to(socket, buf.as_ref(), addr));
                if n != buf.as_ref().len() {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "failed to send entire packet",
                    ));
                }
            }
            SendState::Empty => panic!("poll a Send after it's done"),
        }

        match ::std::mem::replace(&mut self.state, SendState::Empty) {
            SendState::Writing { .. } => Ok(futures::Async::Ready(())),
            SendState::Empty => unreachable!(),
        }
    }
}
