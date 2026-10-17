//! One probe: successful send, one absolute deadline, then a matching reply or timeout.

use crate::{icmp, linux::PingSocket};
use std::io;
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

pub fn probe(target: Ipv4Addr, sequence: u16, timeout: Duration) -> io::Result<Option<Duration>> {
    if timeout.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "timeout must be positive",
        ));
    }
    let socket = PingSocket::open()?;
    let identifier = socket.identifier();
    let payload = b"hello";
    let packet = icmp::echo_request(identifier, sequence, payload);
    let mut buffer = [0u8; 65535];
    let started = Instant::now();
    let deadline = started
        .checked_add(timeout)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "timeout is too large"))?;
    socket.send_to(&packet, target)?;

    while let Some((length, source)) = socket.receive_until(deadline, &mut buffer)? {
        let observed = Instant::now();
        if observed >= deadline {
            return Ok(None);
        }
        if icmp::parse_reply(&buffer[..length]).is_some_and(|reply| {
            matches_request(&reply, source, target, identifier, sequence, payload)
        }) {
            return Ok(Some(observed - started));
        }
    }
    Ok(None)
}

fn matches_request(
    reply: &icmp::EchoReply<'_>,
    source: Ipv4Addr,
    target: Ipv4Addr,
    identifier: u16,
    sequence: u16,
    payload: &[u8],
) -> bool {
    source == target
        && reply.identifier == identifier
        && reply.sequence == sequence
        && reply.payload == payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_must_match_every_request_field() {
        let target = Ipv4Addr::LOCALHOST;
        let reply = icmp::EchoReply {
            identifier: 42,
            sequence: 1,
            payload: b"hello",
        };
        assert!(matches_request(&reply, target, target, 42, 1, b"hello"));
        assert!(!matches_request(
            &reply,
            Ipv4Addr::UNSPECIFIED,
            target,
            42,
            1,
            b"hello"
        ));
        assert!(!matches_request(&reply, target, target, 43, 1, b"hello"));
        assert!(!matches_request(&reply, target, target, 42, 2, b"hello"));
        assert!(!matches_request(&reply, target, target, 42, 1, b"other"));
    }

    #[test]
    #[ignore = "requires Linux ping socket permission and loopback networking"]
    fn loopback_probe_receives_matching_reply() -> io::Result<()> {
        assert!(probe(Ipv4Addr::LOCALHOST, 1, Duration::from_secs(1))?.is_some());
        Ok(())
    }
}
