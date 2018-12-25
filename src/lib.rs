//! tokio-ping is an asynchronous ICMP pinging library.
//!
//! The repository is located at <https://github.com/knsd/tokio-ping/>.
//!
//! # Usage example
//!
//! Note, sending and receiving ICMP packets requires privileges.
//!
//! ```rust,no_run
//! use futures::{Future, Stream};
//!
//! fn main() {
//!     let addr = std::env::args().nth(1).unwrap().parse().unwrap();
//!
//!     let pinger = tokio_ping::Pinger::new();
//!     let stream = pinger.and_then(move |pinger| Ok(pinger.chain(addr).stream()));
//!     let future = stream.and_then(|stream| {
//!         stream.take(3).for_each(|mb_time| {
//!             match mb_time {
//!                 Some(time) => println!("time={:?}", time),
//!                 None => println!("timeout"),
//!             }
//!             Ok(())
//!         })
//!     });
//!
//!     tokio::run(future.map_err(|err| {
//!         eprintln!("Error: {}", err)
//!     }))
//! }
//! ```

mod errors;
mod packet;
mod ping;
mod socket;

pub use self::errors::Error;
pub use self::ping::{PingChain, PingChainStream, PingFuture, Pinger};
