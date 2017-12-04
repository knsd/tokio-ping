use std::io;
use std::rc::Rc;
use std::net::SocketAddr;

use futures;
use libc::c_int;
use tokio_core::reactor::{Handle, PollEvented};

use socket::mio;

#[derive(Clone)]
pub struct Socket {
    socket: Rc<PollEvented<mio::Socket>>,
}

impl Socket {
    pub fn new (family: c_int, type_: c_int, protocol: c_int, handle: &Handle) -> io::Result<Self> {
        let socket = mio::Socket::new(family, type_, protocol)?;
        let socket = PollEvented::new(socket, handle)?;
        Ok(Self {
            socket: Rc::new(socket),
        })
    }

    pub fn send_to<T>(&self, buf: T, target: &SocketAddr) -> Send<T> where T: AsRef<[u8]> {
        Send {
            state: SendState::Writing {
                socket: self.socket.clone(),
                addr: target.clone(),
                buf: buf,
            },
        }
    }

    pub fn recv(&self, buffer: &mut [u8]) -> futures::Poll<usize, io::Error> {
        Ok(futures::Async::Ready(try_nb!(recv(self.socket.clone(), buffer))))
    }
}

pub struct Send<T> {
    state: SendState<T>
}

enum SendState<T> {
    Writing {
        socket: Rc<PollEvented<mio::Socket>>,
        buf: T,
        addr: SocketAddr,
    },
    Empty,
}

fn send_to(socket: Rc<PollEvented<mio::Socket>>, buf: &[u8], target: &SocketAddr) -> io::Result<usize> {
    if let futures::Async::NotReady = socket.poll_write() {
        return Err(io::ErrorKind::WouldBlock.into())
    }
    match socket.get_ref().send_to(buf, target) {
        Ok(n) => Ok(n),
        Err(e) => {
            if e.kind() == io::ErrorKind::WouldBlock {
                socket.need_write();
            }
            Err(e)
        }
    }
}

fn recv(socket: Rc<PollEvented<mio::Socket>>, buf: &mut [u8]) -> io::Result<usize> {
    if let futures::Async::NotReady = socket.poll_read() {
        return Err(io::ErrorKind::WouldBlock.into())
    }
    match socket.get_ref().recv(buf) {
        Ok(n) => Ok(n),
        Err(e) => {
            if e.kind() == io::ErrorKind::WouldBlock {
                socket.need_read();
            }
            Err(e)
        }
    }
}


impl<T> futures::Future for Send<T> where T: AsRef<[u8]> {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> futures::Poll<(), io::Error> {
        match self.state {
            SendState::Writing { ref socket, ref buf, ref addr } => {
                let n = try_nb!(send_to(socket.clone(), buf.as_ref(), addr));
                if n != buf.as_ref().len() {
                    return Err(io::Error::new(io::ErrorKind::Other,
                                              "failed to send entire packet"))
                }
            }
            SendState::Empty => panic!("poll a Send after it's done"),
        }

        match ::std::mem::replace(&mut self.state, SendState::Empty) {
            SendState::Writing { .. } => {
                Ok(futures::Async::Ready(()))
            }
            SendState::Empty => unreachable!(),
        }
    }
}
