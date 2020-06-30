//! tokio-ping is an asynchronous ICMP pinging library.
//!
//! The repository is located at <https://github.com/knsd/tokio-ping/>.
//!
//! # Usage example
//!
//! Note, sending and receiving ICMP packets requires privileges.
//!
//! ```rust,no_run
//! use futures::{future, StreamExt};
//!
//! #[tokio::main]
//! async fn main() {
//!     let addr = std::env::args().nth(1).unwrap().parse().unwrap();
//!
//!     let pinger = tokio_ping::Pinger::new().await.unwrap();
//!     let stream = pinger.chain(addr).stream();
//!     stream.take(3).for_each(|mb_time| {
//!         match mb_time {
//!             Ok(Some(time)) => println!("time={:?}", time),
//!             Ok(None) => println!("timeout"),
//!             Err(err) => println!("error: {:?}", err)
//!         }
//!         future::ready(())
//!     }).await;
//! }
//! ```

#[macro_use] extern crate failure;

mod errors;
mod packet;
mod ping;
mod socket;

pub use errors::Error;
pub use ping::{Pinger, PingChain, PingChainStream, PingFuture};
