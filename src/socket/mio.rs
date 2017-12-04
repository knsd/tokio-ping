use std::io;

use std::net::SocketAddr;
use std::os::unix::io::{RawFd, AsRawFd};

use lazy_socket::raw::{Socket as LazySocket};
use libc::c_int;
use mio::{Evented, Poll, Token, Ready, PollOpt};
use mio::unix::EventedFd;


pub struct Socket {
    socket: LazySocket,
}

impl Socket {
    pub fn new(family: c_int, type_: c_int, protocol: c_int) -> io::Result<Self> {
        let socket = LazySocket::new(family, type_, protocol)?;
        socket.set_blocking(false)?;

        Ok(Self {
            socket: socket
        })
    }

    pub fn send_to(&self, buf: &[u8], target: &SocketAddr) -> io::Result<usize> {
        self.socket.send_to(buf, target, 0)
    }

    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.socket.recv(buf, 0)
    }
}

impl AsRawFd for Socket {
    fn as_raw_fd(&self) -> RawFd {
        self.socket.as_raw_fd()
    }
}

impl Evented for Socket {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).deregister(poll)
    }
}
