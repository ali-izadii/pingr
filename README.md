# pingr

A small Rust ICMPv4 experiment. Sends **10** echo requests to a literal IPv4
address, one at a time, waiting up to one second for each matching reply. The
next request starts after a reply or timeout, with no additional delay.

Supports Linux x86_64. Requires Rust 1.85 or newer; has no third-party crates.

```sh
cargo run -- 127.0.0.1
```

Exits after 10 attempts: `0` if any matching reply arrived, `1` if every attempt
timed out, `2` for invalid arguments or an operational error. Operational errors
stop the run immediately. A timeout alone does not prove the target is down.
Hostnames, IPv6, configurable probe counts, and other operating systems are future work.

Uses Linux ICMP datagram sockets rather than privileged raw sockets. Your group
must be allowed by `net.ipv4.ping_group_range`; permission failures explain this
requirement. pingr does not change system permissions or fall back to raw sockets.

## Code map

- `src/main.rs`: parse one destination, display the outcome, select the exit code.
- `src/ping.rs`: send one probe, enforce its deadline, accept only a matching reply.
- `src/icmp.rs`: request encoding, checksum, and safe reply decoding.
- `src/linux.rs`: native declarations, owned socket, assigned identifier, and bounded I/O.

The Linux module returns ICMP bytes without an IPv4 header. Reply matching uses
the socket's kernel-assigned identifier, source address, sequence, and payload.
Discarding unrelated packets never restarts the deadline.

## Checks

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Networking tests are explicit and ignored by default. On a Linux host that allows
ping sockets and local networking, run:

```sh
cargo test -- --ignored --test-threads=1
```

The roadmap and first-release plan describe future behavior; this prototype does
not yet implement their multi-probe command or statistics.
