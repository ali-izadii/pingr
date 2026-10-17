//! ICMPv4 wire format. Native sockets and IPv4 headers do not belong here.

pub struct EchoReply<'a> {
    pub identifier: u16,
    pub sequence: u16,
    pub payload: &'a [u8],
}

pub fn echo_request(identifier: u16, sequence: u16, body: &[u8]) -> Vec<u8> {
    let mut packet = vec![0u8; 8];
    packet.extend_from_slice(body);
    packet[0] = 8;
    packet[4..6].copy_from_slice(&identifier.to_be_bytes());
    packet[6..8].copy_from_slice(&sequence.to_be_bytes());
    let check = checksum(&packet);
    packet[2..4].copy_from_slice(&check.to_be_bytes());
    packet
}

pub fn parse_reply(bytes: &[u8]) -> Option<EchoReply<'_>> {
    if bytes.len() < 8 || bytes[0] != 0 || bytes[1] != 0 || checksum(bytes) != 0 {
        return None;
    }
    Some(EchoReply {
        identifier: u16::from_be_bytes([bytes[4], bytes[5]]),
        sequence: u16::from_be_bytes([bytes[6], bytes[7]]),
        payload: &bytes[8..],
    })
}

/// Internet checksum: RFC 1071. An odd final byte occupies the high position.
fn checksum(bytes: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    for pair in bytes.chunks(2) {
        let word = u16::from_be_bytes([pair[0], pair.get(1).copied().unwrap_or(0)]);
        sum += u32::from(word);
        sum = (sum & 0xffff) + (sum >> 16);
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_vectors_and_request_layout() {
        assert_eq!(checksum(&[]), 0xffff);
        assert_eq!(checksum(&[1, 2, 3]), 0xfbfd);
        assert_eq!(checksum(&[0, 1, 0xf2, 3, 0xf4, 0xf5, 0xf6, 0xf7]), 0x220d);
        let packet = echo_request(0x1234, 7, b"hello");
        assert_eq!(&packet[..2], &[8, 0]);
        assert_eq!(&packet[4..8], &[0x12, 0x34, 0, 7]);
        assert_eq!(&packet[8..], b"hello");
        assert_eq!(checksum(&packet), 0);
    }

    fn reply_packet() -> Vec<u8> {
        let mut packet = echo_request(42, 1, b"hello");
        packet[0] = 0;
        packet[2..4].fill(0);
        let check = checksum(&packet);
        packet[2..4].copy_from_slice(&check.to_be_bytes());
        packet
    }

    #[test]
    fn decodes_echo_reply_without_an_ipv4_header() {
        let packet = reply_packet();
        let reply = parse_reply(&packet).unwrap();
        assert_eq!(reply.identifier, 42);
        assert_eq!(reply.sequence, 1);
        assert_eq!(reply.payload, b"hello");
    }

    #[test]
    fn rejects_short_corrupt_and_non_reply_packets() {
        let packet = reply_packet();
        for length in 0..8 {
            assert!(parse_reply(&packet[..length]).is_none());
        }
        let mut corrupt = packet.clone();
        corrupt[8] ^= 1;
        assert!(parse_reply(&corrupt).is_none());
        assert!(parse_reply(&echo_request(42, 1, b"hello")).is_none());
        let mut bad_code = packet;
        bad_code[1] = 1;
        bad_code[2..4].fill(0);
        let check = checksum(&bad_code);
        bad_code[2..4].copy_from_slice(&check.to_be_bytes());
        assert!(parse_reply(&bad_code).is_none());
    }
}
