# pingr roadmap

Build a small, dependable command-line tool for everyday network troubleshooting, implemented in Rust with no third-party crates. Start with Linux and IPv4, then add features that help explain connection quality.

This is a roadmap for future implementation, not a claim that these features exist. The current prototype sends 10 serial IPv4 probes on Linux x86_64, gives each a one-second deadline, and validates replies. See [README.md](README.md) for current behavior and checks. All milestones below remain pending until their full acceptance conditions are verified.

For step-by-step instructions covering milestones 1–2, see the [first-release implementation guide](docs/superpowers/plans/2026-10-11-pingr-first-release.md).

## Constraints and scope

- Keep Cargo dependencies empty, including build and development dependencies.
- Do not invoke the system `ping` command or compile custom C code.
- Use Rust's standard library plus a small, documented `unsafe` FFI boundary to operating-system APIs. Zero crates does not mean zero operating-system dependencies.
- Initially support Linux; document and verify the supported CPU architectures before release rather than assuming FFI layouts are portable.
- Prefer Linux ICMP datagram sockets, which can work without root when the user's group is allowed by `ping_group_range`. Give an actionable permission error when unavailable. Do not silently escalate privileges or add a raw-socket fallback initially.
- First release: one destination, IPv4, terminal output. No GUI, background service, configuration files, plugin system, or full compatibility with every system `ping` option.

## Milestone 1 — Prove the networking foundation

**Outcome:** Send one IPv4 ICMP echo request to a literal address and recognize its reply on Linux.

- Establish the smallest socket wrapper, with explicit ABI definitions and documented safety requirements. Use standard-library file-descriptor ownership to close sockets reliably.
- Encode and decode ICMP packets using byte slices and explicit network byte order; avoid casting network data to native structs.
- Implement and test the Internet checksum, including odd-length payloads.
- Match replies to the destination and outstanding request using sequence and payload identity. Account for Linux ping sockets assigning the ICMP identifier.
- Measure elapsed time with a monotonic clock and enforce an absolute receive deadline. Unrelated packets must not extend that deadline.
- Handle short packets, interruptions, socket errors, and permission failures without panics.

**Done when:** Offline packet tests pass, a loopback request succeeds, a controlled nonresponsive target times out, and repeated runs do not leak file descriptors. Tests requiring networking are explicit and separate from ordinary offline tests.

## Milestone 2 — Deliver a useful first release

**Outcome:** A user can run a finite sequence of probes and understand whether the destination answered.

Illustrative interface:

```text
pingr example.com
pingr -c 10 --interval 1 --timeout 2 1.1.1.1
pingr --help
pingr --version
```

- Default to four probes, one second between probe starts, and a one-second reply timeout. Accept positive count, interval, and timeout values; reject invalid or overflowing inputs.
- Keep one probe outstanding at a time. If a probe takes longer than the interval, send the next after it completes without a catch-up burst.
- Resolve hostnames through the system resolver, choose an IPv4 address, and print the selected address. Clearly distinguish resolution failures from reply timeouts. The probe timeout does not promise to bound system DNS resolution.
- Display each reply's sequence and round-trip time, and report each timeout.
- Summarize requests sent, unique replies received, packet loss, and minimum/average/maximum round-trip time. With no replies, show latency as unavailable.
- Count duplicate and late replies without letting them inflate successful probes or corrupt loss statistics.
- Use stable exit codes: `0` if at least one valid reply arrives, `1` if none arrive, and `2` for invalid arguments or operational failures.
- Document installation, permission requirements, examples, and the supported platform.

**Done when:** Argument parsing and statistics pass offline tests; loopback, DNS failure, timeout, malformed arguments, and permission failure have verified behavior; a release build has no Cargo dependencies.

## Milestone 3 — Make troubleshooting clearer

**Outcome:** Long-running checks reveal intermittent connection problems.

- Add an explicit continuous mode and graceful Ctrl+C handling that prints the final summary. Keep signal handling minimal and safe; perform formatting and cleanup in normal control flow.
- Report outage and recovery transitions with timestamps. Describe these as missing ICMP replies, not proof that the Internet or host is down.
- Show the longest consecutive loss streak and longest observed interval without successful replies.
- Add latency variation: define jitter as the mean absolute difference between consecutive successful probes' RTTs, resetting across loss gaps. Show unavailable when there are no qualifying pairs.
- Report recognizable ICMP/network errors separately from ordinary timeouts when the Linux socket interface provides them, matching errors to the current probe.
- Optionally include a compact, bounded recent-history display with an ASCII fallback. Preserve readable output when redirected to a file.

**Done when:** Deterministic event sequences verify outage transitions, duplicates, late replies, loss gaps, and jitter; Ctrl+C prints one final summary and exits cleanly during both waiting and receiving.

## Milestone 4 — Support scripts

**Outcome:** Shell scripts can consume results without scraping terminal formatting.

- Add `--json` for one final structured result with destination, resolved address, counts, timing units, and nullable latency values.
- Keep diagnostics on stderr and machine output on stdout.
- Specify the output schema and escaping rules. With no serialization crate, keep the schema small and test control characters, quotes, backslashes, and non-ASCII text.
- Preserve the documented exit-code meanings.

**Done when:** Output parses in an independent JSON parser for successful runs and zero-reply runs, contains no non-finite numbers, and remains free of terminal formatting.

## Later candidates — choose from actual use

These are ideas, not first-release commitments:

1. **IPv6:** Add explicit address-family selection, ICMPv6 handling, and verified checksum/socket behavior; retain IPv4 tests.
2. **Multiple destinations:** Compare gateway and external-host observations side by side. Bound concurrency and keep per-target statistics independent; avoid claiming a definitive failure location from ICMP alone.
3. **Payload-size checks:** Explore size-related reachability problems only after defining fragmentation behavior. Do not claim path-MTU discovery from echo timeouts alone.
4. **Other operating systems:** Add native implementations after selecting a concrete target and checking its permissions and API constraints. Reuse protocol and statistics code where behavior truly matches.

## Implementation boundaries

Keep responsibilities clear without introducing a framework:

- **CLI:** Parse arguments, resolve the selected destination, render output, and choose the process exit status.
- **Probe loop:** Own scheduling, outstanding-request identity, deadlines, and classification of replies and timeouts.
- **ICMP codec:** Encode and validate packets independently of live sockets.
- **Linux socket boundary:** Own FFI calls, descriptors, and translation of operating-system failures into Rust errors.
- **Statistics:** Consume classified probe outcomes and compute summaries without networking or terminal dependencies.

Introduce modules as these responsibilities appear. Avoid provider interfaces, async runtimes, or generalized platform abstractions before they are needed.

## Verification and release order

Use standard Rust unit tests for pure logic and explicit integration checks for operating-system behavior. Run `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, and `cargo build --release` before a release. Confirm the dependency tree remains empty apart from `pingr` itself.

Ship milestones 1–2 as the first usable release. Add milestones 3 and 4 separately, guided by real troubleshooting needs. Schedule dates only when implementation begins.
