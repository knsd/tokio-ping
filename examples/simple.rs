extern crate futures;
extern crate tokio_core;
extern crate tokio_ping;

use futures::Stream;

fn main() {
    let addr = std::env::args().nth(1).unwrap().parse().unwrap();

    let mut reactor = tokio_core::reactor::Core::new().unwrap();
    let pinger = tokio_ping::Pinger::new(&reactor.handle()).unwrap();
    let stream = pinger.chain(addr).stream();

    let future = stream.take(3).for_each(|mb_time| {
        match mb_time {
            Some(time) => println!("time={}", time),
            None => println!("timeout"),
        }
        Ok(())
    });

    reactor.run(future).unwrap_or_default();
}
