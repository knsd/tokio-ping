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
