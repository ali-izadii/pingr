# pingr First Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Linux IPv4 ping CLI with no third-party crates that sends a finite number of probes and reports accurate latency and packet loss.

**Architecture:** Keep ICMP encoding, reply classification, and statistics independent of sockets. A small Linux module owns all FFI and file descriptors; a synchronous probe loop owns timing. The CLI resolves one destination and prints results.

**Tech Stack:** Rust 2024, Rust standard library, Linux ICMP datagram sockets through system libc FFI. No custom C, Cargo dependencies, async runtime, or external `ping` process.

**Spec:** [ROADMAP.md](../../../ROADMAP.md), milestones 1–2. This guide is intended for you to implement manually; no application code has been implemented by writing it.

## Global constraints

- Keep Cargo dependencies empty, including build and development dependencies.
- Do not invoke the system `ping` command or compile custom C code.
- First release: one destination, IPv4, terminal output. No GUI, background service, configuration files, plugin system, or full compatibility with every system `ping` option.
- Default: four probes, one-second interval, one-second reply timeout.
- One outstanding probe at a time; no catch-up bursts.
- Exit codes: `0` for at least one successful reply, `1` for no successful replies, `2` for usage or operational failure. Operational failure takes precedence over earlier success.
- Prefer Linux ICMP datagram sockets. On permission denial, explain `net.ipv4.ping_group_range`; never change it automatically or silently switch to privileged raw sockets.

Concrete first-release choices, refining the roadmap:

- Initially support **Linux x86_64** only. Reject unsupported targets explicitly at build time; extend the target list only after checking ABI definitions and running network tests there.
- Require Rust 1.85 or later, the first stable version supporting edition 2024.
- Accept `-c` / `--count` from `1` through `65535`, with no sequence wrap. Accept positive integer seconds for `--interval` and `--timeout`, each at most `86400`. These limits bound state and arithmetic; fractional durations can be considered later.
- Send a fixed 24-byte ICMP message: 8-byte header, 8-byte session nonce, 8-byte probe ordinal. Generate the nonce once from `/dev/urandom` using standard-library file I/O; randomness failure is an operational error.
- A probe succeeds only if its matching reply is observed before its deadline. Late packets never reverse a timeout. Duplicate and late counters count observed packets, not distinct probes; do not wait after the last probe merely to collect them.
- Ctrl+C uses ordinary process termination for this release; graceful final summaries belong to roadmap milestone 3.

## Review focus

1. Malformed packets and forged lengths must not panic or read out of bounds — task 1.
2. Foreign replies, identifier rewriting, and stale replies must not count as success — tasks 2 and 3.
3. Continuous irrelevant traffic and interrupted waits must not extend a probe deadline — tasks 3 and 5.
4. Invalid numeric input, unsupported IPv6, and missing IPv4 DNS results must produce actionable errors — task 6.
5. Zero replies, duplicate replies, and output failures must not produce misleading statistics or panic — tasks 4 and 7.

## How to work through this guide

Implement tasks in order. For each task, write the named tests, run them and observe the expected failure, then implement the smallest behavior that passes. Compile the binary after each task. Keep networking tests explicitly ignored so ordinary `cargo test` remains offline and unprivileged.

Commit each working task if you use Git. Stage only the files belonging to that task; the repository currently has untracked starter files, so avoid indiscriminate `git add .`.

Commands below follow this repository's `rtk` convention.

## File map and data flow

| File | Responsibility |
| --- | --- |
| `src/main.rs` | Module declarations, top-level errors, process exit |
| `src/icmp.rs` | Checksums, fixed request encoding, safe reply parsing |
| `src/linux.rs` | Linux ABI, owned socket, send/receive and polling |
| `src/probe.rs` | Request identity, reply classification, serial probe loop |
| `src/stats.rs` | Counts and latency aggregation |
| `src/cli.rs` | Argument parsing and IPv4 resolution |
| `src/output.rs` | Human-readable output through `std::io::Write` |
| `README.md` | Installation, usage, permissions, supported platform |

Flow: arguments → resolved IPv4 → open socket → send request → wait within deadline → validate/classify reply → update statistics → display outcome → final summary and exit status.

Use private modules in the existing binary, with unit tests beside the code. A library crate and a transport trait are unnecessary for this release. Introduce each file only in the task that uses it.

## Task 1 — ICMP bytes without networking

**Files:** Create `src/icmp.rs`; add `mod icmp;` in `src/main.rs`.

**Interfaces produced:**

```rust
pub const PACKET_LEN: usize = 24;
pub struct EchoReply<'a> {
    pub identifier: u16,
    pub sequence: u16,
    pub payload: &'a [u8],
}
pub fn checksum(bytes: &[u8]) -> u16;
pub fn encode_request(identifier: u16, sequence: u16,
                      nonce: [u8; 8], ordinal: u64) -> [u8; PACKET_LEN];
pub fn parse_reply(bytes: &[u8]) -> Option<EchoReply<'_>>;
```

- [ ] Write `checksum_known_vectors`: assert empty input gives `0xffff`, `[0x01, 0x02, 0x03]` gives `0xfbfd`, and `[0x00, 0x01, 0xf2, 0x03, 0xf4, 0xf5, 0xf6, 0xf7]` gives `0x220d`.
- [ ] Run `rtk cargo test icmp::tests::checksum_known_vectors`; expect a missing-function compile failure before implementation.
- [ ] Implement the checksum: sum big-endian 16-bit words in a wide accumulator, pad an odd final byte in the high position, fold carry bits into the low 16 bits, then complement. No unsafe code.
- [ ] Write `request_layout`: assert type/code are `8/0`, identifier and sequence occupy bytes `4..8` in big-endian order, nonce occupies `8..16`, ordinal occupies `16..24` in big-endian order, and `checksum(&packet) == 0`.
- [ ] Implement `encode_request`, calculating the checksum with bytes `2..4` initially zero.
- [ ] Write `reply_validation`: transform a request into type `0`, recompute its checksum, and assert parsed fields match; separately assert every prefix shorter than 8 bytes, bad code, bad type, and corrupted checksum return `None`.
- [ ] Implement `parse_reply` with length checks before indexing. It validates the ICMP envelope; exact application payload length and identity belong to task 3. Linux ping datagram sockets provide ICMP bytes, not an IPv4 header.
- [ ] Run `rtk cargo test icmp::tests`; all tests pass. Commit the codec task.

## Task 2 — A safe owner for the Linux socket

**Files:** Create `src/linux.rs`; register it in `src/main.rs`; set `rust-version = "1.85"` in `Cargo.toml`.

**Interfaces produced:**

```rust
pub struct PingSocket { /* OwnedFd and assigned identifier */ }
impl PingSocket {
    pub fn open() -> std::io::Result<Self>;
    pub fn identifier(&self) -> u16;
    pub fn send_to(&self, packet: &[u8], target: std::net::Ipv4Addr)
        -> std::io::Result<()>;
    pub fn receive_until(&self, deadline: std::time::Instant,
                         buffer: &mut [u8])
        -> std::io::Result<Option<(usize, std::net::Ipv4Addr)>>;
}
```

- [ ] Add the Linux/x86_64 build guard. Gate the Linux implementation so unsupported targets get a clear `compile_error!` rather than unrelated missing-import errors.
- [ ] Define only the FFI used here: `socket`, `bind`, `getsockname`, `sendto`, `recvfrom`, and `poll`. Use Rust 2024 `unsafe extern "C"` and `std::ffi` C integer types. Check declarations against the installed Linux headers/man pages before writing calls.
- [ ] Define `#[repr(C)]` ABI structs for IPv4 socket addresses and `pollfd`; use Linux x86_64 `socklen_t = u32` and `nfds_t = c_ulong`. Keep all definitions private. Test IPv4 socket-address size/alignment as `16/4` and poll descriptor size/alignment as `8/4`.
- [ ] Test address conversion: `1.2.3.4` must have in-memory address bytes `[1, 2, 3, 4]`; port/identifier conversion must round-trip through network byte order. Use `u32::from_ne_bytes(ip.octets())` for the address storage, not an accidental second endian conversion.
- [ ] Implement `open`: call `socket(AF_INET, SOCK_DGRAM | SOCK_CLOEXEC | SOCK_NONBLOCK, IPPROTO_ICMP)`, check for `-1`, immediately wrap the successful descriptor in `OwnedFd`, then bind to `0.0.0.0` with identifier/port zero. Use `getsockname` to retrieve the assigned identifier with `u16::from_be`. Validate returned address length and family.
- [ ] Implement `send_to`: construct the destination address with port zero, retry `Interrupted`, require a full datagram send, and propagate other errors. A short send is an operational error, never a successful probe.
- [ ] Implement `receive_until`: recompute remaining time before each `poll`, round positive sub-millisecond waits upward, clamp to `c_int::MAX`, and return `None` once the absolute deadline is reached. On readiness, use nonblocking `recvfrom`; validate source family and length. Retry `Interrupted` and raced `WouldBlock` only after rechecking the deadline. Propagate terminal socket/poll errors rather than spinning.
- [ ] Allocate the receive buffer in the caller at 65535 bytes so a datagram cannot masquerade as a valid short packet through truncation. Capture `last_os_error()` immediately after a failed FFI call. Document why every unsafe call's pointers, lengths, ABI, and descriptor ownership are valid.
- [ ] Add ignored tests named `linux::tests::socket_open_close` and `linux::tests::idle_receive_deadline`. The first repeatedly opens/drops sockets and confirms `/proc/self/fd` counts stabilize; the second verifies an idle socket returns `None` after a short deadline with a generous scheduling tolerance.
- [ ] Run `rtk cargo test linux::tests` for offline ABI/address checks; then `rtk cargo test linux::tests -- --ignored --test-threads=1` on a Linux host allowing ping sockets. Permission failure is reported, never silently considered a pass. Commit the socket task.

Linux ping sockets may replace the request identifier and recompute the checksum. Always match replies to the identifier returned by the bound socket, never merely to the process ID. System libc remains an operating-system dependency; this is not a promise of a fully static binary.

## Task 3 — Match packets to probes

**Files:** Create `src/probe.rs`; register it in `src/main.rs`.

**Interfaces consumed:** `icmp::EchoReply` and `icmp::parse_reply`.

**Interfaces produced:**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeState { Waiting, Replied, TimedOut }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyKind { Current, Duplicate, Late, Ignore }
pub fn classify_reply(reply: &crate::icmp::EchoReply<'_>,
    source: std::net::Ipv4Addr, target: std::net::Ipv4Addr,
    identifier: u16, nonce: [u8; 8], states: &[ProbeState]) -> ReplyKind;
```

- [ ] Write classification tests: a matching reply for the last `Waiting` entry is `Current`; a matching `Replied` entry is `Duplicate`; a matching `TimedOut` entry is `Late`.
- [ ] Add tests returning `Ignore` for wrong source, identifier, nonce, payload length, sequence/ordinal mismatch, ordinal zero, and an ordinal beyond the number of sent probes. Use a payload containing a huge `u64` ordinal to prove bounds are checked before indexing or narrowing.
- [ ] Run `rtk cargo test probe::tests`; expect failure before implementing the classifier.
- [ ] Implement classification: payload must be exactly 16 bytes; nonce must match; decode the ordinal, verify it is within `1..=states.len()`, then compare the sequence. Probe ordinal `n` uses state index `n - 1`. Only the last state may be `Waiting`.
- [ ] Add a test showing malformed packets are rejected by the codec before classification. The classifier is pure and never updates state or statistics.
- [ ] Run `rtk cargo test probe::tests`; all pass. Commit the matching task.

## Task 4 — Accurate statistics

**Files:** Create `src/stats.rs`; register it in `src/main.rs`.

**Interfaces produced:**

```rust
#[derive(Default)]
pub struct Stats { /* private counts, duration sum, min, max */ }
pub struct Summary {
    pub sent: u64, pub received: u64,
    pub duplicates: u64, pub late: u64,
    pub loss_percent: f64,
    pub min: Option<std::time::Duration>,
    pub avg: Option<std::time::Duration>,
    pub max: Option<std::time::Duration>,
}
impl Stats {
    pub fn record_sent(&mut self);
    pub fn record_reply(&mut self, rtt: std::time::Duration);
    pub fn record_duplicate(&mut self);
    pub fn record_late(&mut self);
    pub fn summary(&self) -> Summary;
}
```

- [ ] Write `mixed_results`: record four sends, replies of 10 ms and 30 ms, one duplicate and one late packet. Assert sent `4`, received `2`, loss `50.0`, min/avg/max `10/20/30 ms`, and separate counters of `1` each.
- [ ] Write `no_replies`: four sends give `100.0` loss and `None` for every latency. Write `empty_stats`: zero sends give `0.0` loss and no latency, with no division by zero.
- [ ] Run `rtk cargo test stats::tests`; expect failure before implementation.
- [ ] Implement counters and duration aggregation. Only `record_reply` affects latency and received count; call it once per successful probe. Compute average using integer duration arithmetic and convert to milliseconds only for display. Use saturating increments for observational duplicate/late counters.
- [ ] Run `rtk cargo test stats::tests`; all pass. Commit the statistics task.

## Task 5 — Complete the serial probe loop

**Files:** Extend `src/probe.rs`; use `src/linux.rs` and `src/stats.rs`.

**Interfaces produced:**

```rust
pub struct ProbeConfig {
    pub target: std::net::Ipv4Addr,
    pub count: u16,
    pub interval: std::time::Duration,
    pub timeout: std::time::Duration,
}
pub enum ProbeEvent {
    Reply { sequence: u16, rtt: std::time::Duration },
    Timeout { sequence: u16 },
}
pub fn run(config: &ProbeConfig,
    on_event: impl FnMut(ProbeEvent) -> std::io::Result<()>)
    -> std::io::Result<crate::stats::Summary>;
```

- [ ] Write a private scheduling helper test: `next_send_at(start, interval, finished)` returns `max(start + interval, finished)`. Verify a quick reply preserves the interval and a long timeout schedules no catch-up burst. Use `Instant::checked_add` and return `InvalidInput` on overflow.
- [ ] Test a private `before_deadline(now, deadline)` helper: strictly earlier is true; equal and later are false. Use it on every received packet before classifying it as a success.
- [ ] Implement `run` with validation of nonzero count/durations and the one-day duration maximum, including callers that bypass the CLI. Open one socket, read an 8-byte nonce, allocate one receive buffer, and start with an empty state vector and statistics.
- [ ] For each ordinal from `1` to `count`, encode the packet, record `Instant::now()` immediately before sending, and send. Only after a successful send append `Waiting` and record the send. Derive the absolute deadline from the recorded instant with checked arithmetic.
- [ ] Receive until that deadline. Recheck the deadline after receive; if expired, mark the current probe `TimedOut`. Otherwise parse and classify: `Ignore` continues waiting, `Duplicate`/`Late` update their own counters and continue, and `Current` transitions once to `Replied`, records RTT, and emits one `Reply` event.
- [ ] When the receive deadline expires without success, transition to `TimedOut` and emit one `Timeout` event. Never restart a full timeout after a discarded packet. Between probes, sleep until the scheduling helper's result; do not sleep after the last probe. Queued earlier replies are classified when receiving for the next probe.
- [ ] Propagate send/receive/output errors immediately. Do not turn an operational error into packet loss or return a successful final summary after it.
- [ ] Add ignored `probe::tests::loopback_two_probes`: send two requests to `127.0.0.1`, assert two sends, two unique successes, zero loss, and two reply events. Inspect actual packet bytes while diagnosing failures; never disable identity validation to make the test pass.
- [ ] Add ignored `probe::tests::irrelevant_traffic_deadline`: use a local UDP responder to deliver checksum-valid but nonce-mismatched ICMP-shaped datagrams continuously. In a test-only constructor inside the Linux module, adopt a nonblocking UDP `OwnedFd` so the real `receive_until` path is exercised. The test receive/validation loop must stop at its original deadline despite traffic. Keep test-only construction out of the production API and join the responder thread.
- [ ] Add ignored Linux test `interrupted_poll_deadline`: in a serialized test process, install a temporary non-restarting no-op signal handler and use a helper thread to interrupt the waiting thread repeatedly. Verify the same absolute deadline holds and restore the handler afterward. Keep the signal FFI and setup test-only; do not add production signal handling yet.
- [ ] Run `rtk cargo test` followed by `rtk cargo test -- --ignored --test-threads=1` on the supported host. Commit the probe-loop task.

The callback is only an output hook: it lets tests collect events and lets the terminal writer report errors. It is not a new service or plugin abstraction. Keep time arithmetic and packet classification separately testable instead of designing a fake networking framework.

## Task 6 — Arguments and IPv4 resolution

**Files:** Create `src/cli.rs`; register it in `src/main.rs`.

**Interfaces produced:**

```rust
pub struct Options {
    pub destination: String,
    pub count: u16,
    pub interval: std::time::Duration,
    pub timeout: std::time::Duration,
}
pub enum Command { Help, Version, Run(Options) }
pub fn parse_args(args: impl IntoIterator<Item = std::ffi::OsString>)
    -> Result<Command, String>;
pub fn resolve_ipv4(destination: &str) -> Result<std::net::Ipv4Addr, String>;
```

- [ ] Write parsing tests for defaults (`4/1s/1s`), `-c 10`, `--count 10`, independent interval/timeout overrides, `--help`, and `--version`. The iterator excludes the executable name.
- [ ] Test rejection of no destination, two destinations, unknown flags, missing values, repeated options, zero, negative values, fractional seconds, `NaN`, overflow, count `65536`, seconds `86401`, and non-UTF-8 arguments. Accept boundary count `65535` and duration `86400`.
- [ ] Run `rtk cargo test cli::tests`; expect failure before implementation.
- [ ] Implement a small argument iterator. Support `--` to end option parsing. Accept help/version only as standalone commands; other combinations get a usage error. Do not introduce a generic argument-parser framework.
- [ ] Test literal `127.0.0.1` resolution and explicit rejection of `::1`. Extract a private `select_ipv4` iterator helper and test that it selects the first IPv4 result and errors on empty or IPv6-only results without invoking DNS.
- [ ] Implement resolution: first parse a literal `IpAddr`, rejecting IPv6 clearly; otherwise use `(destination, 0).to_socket_addrs()` and select the first IPv4 address. Resolve once per run, never once per probe. Add the original hostname to errors. Explain that system DNS resolution is outside the ICMP timeout.
- [ ] Run `rtk cargo test cli::tests`; all pass. Commit the CLI task.

## Task 7 — Wire the application and document it

**Files:** Create `src/output.rs` and `README.md`; replace the starter body in `src/main.rs`.

**Interfaces produced:**

```rust
pub fn write_event(out: &mut impl std::io::Write,
    event: &crate::probe::ProbeEvent) -> std::io::Result<()>;
pub fn write_summary(out: &mut impl std::io::Write,
    summary: &crate::stats::Summary) -> std::io::Result<()>;
```

- [ ] Test output into `Vec<u8>`: replies contain sequence and milliseconds, timeouts contain sequence, summary contains sent/received/loss, and no-reply summary prints `latency: unavailable`. Specify RTT formatting as three decimal milliseconds and loss as one decimal percent.
- [ ] Test a custom writer that returns `BrokenPipe`: both output functions return the error without panicking.
- [ ] Implement the writers using `write!`/`writeln!` with propagated errors. Use `Duration::as_secs_f64() * 1000.0` only for display. Include observed duplicate/late counts in the summary.
- [ ] Implement `main() -> std::process::ExitCode`: parse, handle help/version, resolve, print the original and selected destination, build `ProbeConfig`, run using the output callback, print summary, and choose the documented exit code. Use `env!("CARGO_PKG_VERSION")` for version output. Propagate output errors including final flush errors; avoid `println!` on fallible output paths.
- [ ] Send errors to stderr with context (`argument`, `resolution`, `socket/probe`, or `output`). On socket `PermissionDenied`, mention `net.ipv4.ping_group_range` and asking the system administrator to allow the user's group. A diagnostic write failure must not panic or change exit code `2`.
- [ ] Write `README.md`: Linux x86_64 and Rust 1.85+, `cargo build --release`, binary location, examples, flags and limits, exit codes, ping-socket permissions, system-library reliance, finite default count, DNS timeout limitation, and ordinary Ctrl+C behavior. Explain that absent ICMP replies do not prove a host is down.
- [ ] Run `rtk cargo test` and `rtk cargo run -- --help`; verify flags/defaults match this plan. Commit the usable CLI task.

## Task 8 — First-release acceptance

**Files:** Update `README.md` with reproducible network-test instructions; mark implemented milestones in `ROADMAP.md` only after verification.

- [ ] Run `rtk cargo fmt --check` and fix formatting.
- [ ] Run `rtk cargo test`; every offline test passes without network access or elevated privileges.
- [ ] Run `rtk cargo clippy --all-targets -- -D warnings`; resolve warnings, including undocumented or needlessly broad unsafe operations.
- [ ] Run `rtk cargo build --release` and `rtk cargo tree`; the tree contains only `pingr` and no dependencies.
- [ ] Run `rtk cargo test -- --ignored --test-threads=1` on the supported Linux host; record the kernel, target, and outcome in your release notes. Do not report ignored tests as passed if they were not run.
- [ ] Run `rtk ./target/release/pingr -c 2 127.0.0.1`; verify two replies, zero loss, and exit `0`.
- [ ] Run `rtk ./target/release/pingr -c 0 127.0.0.1`; verify a usage error and exit `2`.
- [ ] Run against a DNS name under `.invalid`; verify a contextual resolution error and exit `2`. Do not expect the probe timeout to bound DNS.
- [ ] In a disposable Linux network namespace or VM, configure a reachable peer that deliberately drops echo requests while keeping the route valid. Run two probes with one-second timeouts; verify two timeouts, unavailable latency, 100% loss, and exit `1`. Do not use a random public address as a deterministic timeout test.
- [ ] In that isolated environment, exclude the test user's group from the ping-socket range and run as that unprivileged user. Verify actionable permission output and exit `2`. Change no host-wide permission or firewall setting merely to run tests.
- [ ] Review every unsafe block, file-descriptor error path, receive deadline, and reply identity check. Confirm no raw-socket fallback or external command has slipped in.
- [ ] Update the roadmap's first two milestone statuses based on actual results, document any blocked environment checks honestly, and commit the release documentation.

For commands whose exit codes are part of acceptance, inspect the shell/tool's reported exit status; terminal wording alone is insufficient.

## What to do afterward

Use the first release before expanding it. Next implement roadmap milestone 3 as a separate plan: safe Ctrl+C handling, continuous mode, outage transitions, then jitter. Add JSON output in another small change. IPv6, multiple destinations, and portability remain future decisions, so this plan does not create abstractions or dependencies for them.

**Your starting point:** implement task 1's checksum tests in `src/icmp.rs`. You can complete the entire first task without network permissions or unsafe Rust.
