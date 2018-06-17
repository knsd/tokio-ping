Here you can see the full list of changes between each tokio-ping release.

### Version 0.2.0 (2018-06-17)

#### Refactorings
* Use tokio instead of tokio-core
* Use failure instead of error-chain
* Simplify ICMP packets encoding and parsing

### Version 0.1.2 (2018-03-18)

#### Fixes
* Still EINVAL on ICMPv6 ([#5](https://github.com/knsd/tokio-ping/pull/5))
* Panic in debug builds ([#4](https://github.com/knsd/tokio-ping/issues/4))

### Version 0.1.1 (2018-02-17)

#### Fixes
* EINVAL error on ICMPv6 ([#1](https://github.com/knsd/tokio-ping/issues/1), [#2](https://github.com/knsd/tokio-ping/pull/2))

#### Refactorings
* Use socket2 instead of lazy_socket ([#3](https://github.com/knsd/tokio-ping/pull/3))


### Version 0.1.0 (2017-12-06)

Initial release.
