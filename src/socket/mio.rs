use std::io;

use std::os::unix::io::{AsRawFd, RawFd};

use mio::{Evented, Poll, PollOpt, Ready, Token};
use mio::unix::EventedFd;
use socket2::{Domain, Protocol, SockAddr, Socket as Socket2, Type};

pub struct Socket {
    socket: Socket2,
}

impl Socket {
    pub fn new(domain: Domain, type_: Type, protocol: Protocol) -> io::Result<Self> {
        let socket = Socket2::new(domain, type_, Some(protocol))?;
        socket.set_nonblocking(true)?;

        Ok(Self { socket: socket })
    }

    pub fn send_to(&self, buf: &[u8], target: &SockAddr) -> io::Result<usize> {
        self.socket.send_to(buf, target)
    }

    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.socket.recv(buf)
    }
}

impl AsRawFd for Socket {
    fn as_raw_fd(&self) -> RawFd {
        self.socket.as_raw_fd()
    }
}

impl Evented for Socket {
    fn register(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).register(poll, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).deregister(poll)
    }
}
