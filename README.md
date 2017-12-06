# tokio-ping
[![Build Status](https://travis-ci.org/knsd/tokio-ping.svg?branch=master)](https://travis-ci.org/knsd/tokio-ping)
[![Latest Version](https://img.shields.io/crates/v/tokio-ping.svg)](https://crates.io/crates/tokio-ping/)
[![docs](https://docs.rs/tokio-ping/badge.svg)](https://docs.rs/tokio-ping)

tokio-ping is an asynchronous ICMP pinging library.

# Usage example

Note, sending and receiving ICMP packets requires privileges.

```rust
extern crate futures;
extern crate tokio_core;
extern crate tokio_ping;

use futures::{Future, Stream};

fn main() {
    let addr = std::env::args().nth(1).unwrap().parse().unwrap();

    let mut reactor = tokio_core::reactor::Core::new().unwrap();
    let pinger = tokio_ping::Pinger::new(&reactor.handle()).unwrap();
    let chain = pinger.chain(addr);

    let future = futures::stream::iter_ok((0..3))
        .and_then(|n| chain.send().and_then(move |res| Ok((n, res))))
        .for_each(|(n, mb_time)| {
            match mb_time {
                Some(time) => println!("{} time={}", n, time),
                None => println!("{} timeout", n),
            }
            Ok(())
    });

    reactor.run(future).unwrap_or_default();
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