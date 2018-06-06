# tokio-ping
[![Build Status](https://travis-ci.org/knsd/tokio-ping.svg?branch=master)](https://travis-ci.org/knsd/tokio-ping)
[![Latest Version](https://img.shields.io/crates/v/tokio-ping.svg)](https://crates.io/crates/tokio-ping/)
[![docs](https://docs.rs/tokio-ping/badge.svg)](https://docs.rs/tokio-ping)

tokio-ping is an asynchronous ICMP pinging library.

# Usage example

Note, sending and receiving ICMP packets requires privileges.

```rust
extern crate futures;
extern crate tokio;

extern crate tokio_ping;

use futures::{Future, Stream};

fn main() {
    let addr = std::env::args().nth(1).unwrap().parse().unwrap();

    let pinger = tokio_ping::Pinger::new();
    let stream = pinger.and_then(move |pinger| Ok(pinger.chain(addr).stream()));
    let future = stream.and_then(|stream| {
        stream.take(3).for_each(|mb_time| {
            match mb_time {
                Some(time) => println!("time={}", time),
                None => println!("timeout"),
            }
            Ok(())
        })
    });

    tokio::run(future.map_err(|err| {
        eprintln!("Error: {}", err)
    }))
}

```

# License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you shall be dual licensed as above, without any additional terms or conditions.