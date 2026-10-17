//! Command-line input and output. Packet rules and native networking live in their modules.

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
compile_error!("pingr currently supports Linux x86_64 only");

mod icmp;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod ping;

use std::io::{self, Write};
use std::net::Ipv4Addr;
use std::process::ExitCode;
use std::time::Duration;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "pingr: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn run() -> io::Result<bool> {
    let mut args = std::env::args_os().skip(1);
    let target = args
        .next()
        .and_then(|input| input.to_str()?.parse::<Ipv4Addr>().ok());

    let target = match (target, args.next()) {
        (Some(target), None) => target,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Usage: pingr <IPv4 address>",
            ));
        }
    };

    let mut output = io::stdout().lock();
    writeln!(output, "Destination: {target}")?;

    let mut replied = false;
    for sequence in 1..=10 {
        match ping::probe(target, sequence, Duration::from_secs(1))? {
            Some(rtt) => {
                writeln!(
                    output,
                    "Reply from {target}: sequence={sequence} time={:.3} ms",
                    rtt.as_secs_f64() * 1000.0
                )?;
                replied = true;
            }
            None => {
                writeln!(output, "Request timed out: {target} sequence={sequence}")?;
            }
        }
        output.flush()?;
    }
    Ok(replied)
}

// Keep unsupported builds focused on the explicit diagnostic above.
#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn main() {}
