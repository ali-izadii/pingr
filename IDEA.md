# Pingr: project ideas and learning path

Turn Pingr into a network troubleshooting tool for investigating intermittent
connection problems while learning Rust, Linux, networking, and testing.

This document contains ideas, not implemented features. See [README.md](README.md)
for current behavior and [ROADMAP.md](ROADMAP.md) for the existing milestones and
constraints.

## Current foundation

Pingr currently sends 10 sequential ICMPv4 probes to one literal IPv4 address on
Linux x86_64. It implements packet encoding, checksum validation, reply matching,
absolute deadlines, and an owned Linux socket wrapper without third-party crates.

Keep the roadmap's dependency-free constraint. External development tools can
help investigate the program without becoming application dependencies.

## Recommended learning path

### 1. Make the CLI useful

Add configurable probe count, interval, and timeout; hostname resolution; help;
and a final summary with packet loss and minimum/average/maximum RTT.

Learn argument validation, DNS resolution, scheduling, and statistics.

Acceptance conditions:

- Invalid and overflowing arguments fail clearly.
- Zero replies produce unavailable latency, without division by zero.
- Scheduling never sends catch-up bursts after a slow probe.
- Resolution failures are distinct from probe timeouts.
- Unique replies determine packet loss; duplicates cannot inflate success.

### 2. Monitor connection quality over time

Add continuous mode, a final summary on Ctrl+C, outage/recovery events, loss
streaks, and jitter. Define jitter as the mean absolute difference between
consecutive successful probes' RTTs, resetting across loss gaps.

Learn state machines, safe signal handling, monotonic clocks, and streaming
calculations. Keep formatting and cleanup outside signal handlers.

Acceptance conditions:

- Deterministic reply/timeout sequences produce the expected transitions.
- Ctrl+C prints one final summary during waiting or receiving.
- Loss gaps do not create misleading jitter samples.
- Missing ICMP replies are not presented as proof that the Internet is down.

### 3. Build a reproducible network lab

Create isolated scenarios for delay, packet loss, duplication, and reordering
using Linux network namespaces, veth pairs, and `tc netem`.

Learn Linux networking, integration testing, and evidence-based debugging.
Lab setup requires privileges; isolate experiments from normal host traffic and
document cleanup.

Acceptance conditions:

- Scenarios can be repeated with documented setup and cleanup.
- Controlled delay is reflected in measured RTT within reasonable tolerance.
- Duplicate packets never inflate successful probe counts.
- Unrelated or delayed packets never extend an outstanding probe's deadline.
- Network-dependent tests stay separate from ordinary offline tests.

### 4. Compare multiple destinations

Monitor a gateway and several external destinations with independent statistics
and bounded concurrency.

Learn polling, fairness, resource ownership, and concurrent scheduling. Start
with the existing Linux socket approach; introduce an async runtime only if a
later requirement justifies changing the dependency constraint.

Acceptance conditions:

- A slow destination does not delay every other target.
- Each destination retains independent identity, deadlines, and statistics.
- Resource use is bounded.
- Comparisons describe observations without claiming a definitive failure
  location from ICMP alone.

### 5. Produce scriptable reports

Add a final JSON report with documented fields, timing units, counts, and nullable
latency values. Keep diagnostics on stderr and structured output on stdout.

Learn output contracts, escaping, and compatibility. Under the no-crates
constraint, keep the schema small and test serialization carefully.

Acceptance conditions:

- An independent JSON parser accepts successful and zero-reply reports.
- Strings with quotes, backslashes, control characters, and Unicode work.
- Reports contain no non-finite numbers or terminal formatting.
- Existing exit-code meanings remain stable.

### 6. Explore advanced networking

Add IPv6 first, then consider a separate traceroute-style feature.

Learn ICMPv6, hop limits, network error messages, and protocol differences.
Traceroute needs its own design for probe/error correlation and socket behavior;
it is not simply an extra display mode.

Acceptance conditions:

- Packet captures and controlled topologies validate behavior.
- IPv4 behavior remains covered by tests.
- Unsupported socket behavior or permissions produce clear errors.

## Development tools

| Tool | Learning purpose |
| --- | --- |
| Wireshark or tcpdump | Compare actual ICMP fields and payloads with the bytes produced by the code. |
| strace | Observe socket creation, binding, sending, polling, receiving, and descriptor cleanup. |
| Network namespaces, veth, and tc netem | Reproduce delay, loss, duplication, and reordering in an isolated lab. |
| rust-gdb | Step through Rust execution and inspect the FFI boundary. |
| jq | Independently validate and explore future JSON reports. |
| GitHub Actions | Automate formatting, offline tests, and Clippy; keep network checks explicit. |

Start with packet capture and strace before expanding the application.

## Understand the existing implementation

Follow one packet through these files:

1. [src/main.rs](src/main.rs): arguments, destination, probe sequence, output, and
   exit status.
2. [src/ping.rs](src/ping.rs): request identity, absolute deadline, reply matching,
   and RTT measurement.
3. [src/icmp.rs](src/icmp.rs): wire format, byte order, checksum, and safe parsing.
4. [src/linux.rs](src/linux.rs): Linux calls, nonblocking I/O, polling, and socket
   ownership.

For each part, predict what happens, observe it with a tool, and explain any
difference. Useful questions include:

- Why must unrelated packets never restart the timeout?
- Why does Linux assign the ping socket's ICMP identifier?
- Why use a monotonic clock for RTT and deadlines?
- How does OwnedFd prevent descriptor leaks on error paths?
- Why are packets decoded from byte slices instead of cast to native structs?

Currently, `probe()` opens a new socket for each request. Continuous monitoring
and meaningful late/duplicate-reply tracking will benefit from a session that
owns a socket across probes. Design that ownership when implementing those
features, including request identity across sequence-number wraparound.

## Suggested first deliverable

Implement configurable probing and a final statistics summary, then demonstrate
the behavior with a packet capture. Follow with the controlled network lab.

Use the existing roadmap for detailed first-release requirements. Run focused
tests for each change and the documented formatting, test, and Clippy checks.

Defer dashboards, databases, distributed monitoring, and new runtimes until a
concrete requirement needs them. Reliable measurements, reproducible failure
tests, and a clear explanation of the internals are the core portfolio value.
