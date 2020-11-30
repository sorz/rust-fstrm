# rust-fstrm

A implementation of the *Frame Streams data transport protocol*
([fstrm](https://github.com/farsightsec/fstrm)) in Rust.

Only the **reader** is current implemented in this repo, whereas
[rust-framestream](https://github.com/jedisct1/rust-framestream)
is a **writer-only** implementation in Rust you may want to look into.

I wrote this library for parsing [dnstap](https://dnstap.info),
which is a DNS-encapsulated-in-ProtocolBuffers-send-over-FrameStreams protocol,
in [dnsnfset](https://github.com/sorz/dnsnfset/).
