//! tokio-ping is an asynchronous ICMP pinging library.
//!
//! The repository is located at https://github.com/knsd/tokio-ping/.
//!
//! # Usage example
//!
//! Note, sending and receiving ICMP packets requires privileges.
//!
//! ```rust,no_run
//! extern crate futures;
//! extern crate tokio_core;
//! extern crate tokio_ping;
//!
//! use futures::Stream;
//!
//! fn main() {
//!     let addr = std::env::args().nth(1).unwrap().parse().unwrap();
//!
//!     let mut reactor = tokio_core::reactor::Core::new().unwrap();
//!     let pinger = tokio_ping::Pinger::new(&reactor.handle()).unwrap();
//!     let stream = pinger.chain(addr).stream();
//!
//!     let future = stream.take(3).for_each(|mb_time| {
//!         match mb_time {
//!             Some(time) => println!("time={}", time),
//!             None => println!("timeout"),
//!         }
//!         Ok(())
//!     });
//!
//!     reactor.run(future).unwrap_or_default();
//! }
//!
//! ```

#[macro_use] extern crate error_chain;
extern crate futures;
extern crate lazy_socket;
extern crate libc;
extern crate mio;
extern crate rand;
extern crate time;
#[macro_use] extern crate tokio_core;

mod errors;
mod packet;
mod ping;
mod socket;

pub use errors::{Error, ErrorKind};
pub use ping::{Pinger, PingChain, PingChainStream, PingFuture};
