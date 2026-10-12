use std::ffi::c_int;
use std::io;
use std::net::Ipv4Addr;
use std::os::fd::{FromRawFd, OwnedFd};

#[cfg(any(target_os = "linux", target_os = "macos"))]
unsafe extern "C" {
    fn socket(domain: c_int, socket_type: c_int, protocol: c_int) -> c_int;
}

#[repr(i32)]
#[derive(Clone, Copy)]
enum AddressFamily {
    Unix = 1,
    Ipv4 = 2,

    #[cfg(target_os = "linux")]
    Ipv6 = 10,

    #[cfg(target_os = "macos")]
    Ipv6 = 30,
}

#[repr(i32)]
#[derive(Clone, Copy)]
enum SocketType {
    Stream = 1,
    Datagram = 2,
    Raw = 3,
}

#[repr(i32)]
#[derive(Clone, Copy)]
enum Protocol {
    Default = 0,
    Icmp = 1,
    Tcp = 6,
    Udp = 17,
    IcmpV6 = 58,
}

/// Algorithm: https://www.rfc-editor.org/rfc/rfc1071#section-1
/// ICMP specification: https://www.rfc-editor.org/rfc/rfc792
fn echo_request(identifier: u16, sequence: u16) -> [u8; 8] {
    let mut packet = [0u8; 8];
    packet[0] = 8; // Echo Request
    packet[1] = 0; // Code
    packet[4..6].copy_from_slice(&identifier.to_be_bytes());
    packet[6..8].copy_from_slice(&sequence.to_be_bytes());

    let check = checksum(&packet);
    packet[2..4].copy_from_slice(&check.to_be_bytes());

    packet
}

fn echo_request_with_body(identifier: u16, sequence: u16, body: &[u8]) -> Vec<u8> {
    let mut packet = vec![0u8; 8];
    packet.extend_from_slice(body);

    packet[0] = 8; // Echo Request
    packet[4..6].copy_from_slice(&identifier.to_be_bytes());
    packet[6..8].copy_from_slice(&sequence.to_be_bytes());

    let check = checksum(&packet);
    packet[2..4].copy_from_slice(&check.to_be_bytes());

    packet
}

fn checksum(bytes: &[u8]) -> u16 {
    // Used u32 to handle carry. checksum is u16 but
    // Because of Internet checksum uses one’s-complement addition need to avoid overflow
    let mut sum: u32 = 0;

    for pair in bytes.chunks(2) {
        let high = pair[0];
        let low = pair.get(1).copied().unwrap_or(0);
        let word = u16::from_be_bytes([high, low]);
        sum += u32::from(word);
        sum = (sum & 0xffff) + (sum >> 16);
    }

    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }

    !(sum as u16)
}

#[repr(C)]
struct SockAddrIn {
    family: u16, // Address family: AF_INET
    port: u16,   // Zero: ICMP does not use ports
    address: [u8; 4],
    padding: [u8; 8],
}

impl SockAddrIn {
    fn new(ip: Ipv4Addr) -> Self {
        Self {
            family: 2, // AF_INET on Linux
            port: 0,
            address: ip.octets(),
            padding: [0; 8],
        }
    }
}


fn main() -> std::io::Result<()> {
    let Some(input) = std::env::args().nth(1) else {
        eprintln!("input id");
        std::process::exit(2);
    };

    let address: Ipv4Addr = match input.parse() {
        Ok(address) => address,
        Err(_) => {
            eprintln!("Invalid IPv4 address: {input}");
            std::process::exit(2);
        }
    };

    // Socket open a file descriptor
    let file_descriptor = unsafe {
        socket(
            AddressFamily::Ipv4 as c_int,
            SocketType::Raw as c_int,
            Protocol::Icmp as c_int,
        )
    };

    if file_descriptor == -1 {
        return Err(io::Error::last_os_error());
    }

    // SAFETY: fd is a newly created, valid descriptor that we own.
    let socket = unsafe { OwnedFd::from_raw_fd(file_descriptor) };
    let dest = SockAddrIn::new(address);
    let identifier = std::process::id() as u16;
    let sequence: u16 = 1;
    let packet = echo_request(identifier, sequence);


    println!("Socket opened: {socket:?}");
    println!("Destination: {}", Ipv4Addr::from(dest.address));
    println!("Identifier: {identifier}, sequence: {sequence}");
    println!("Packet: {packet:02x?}");

    Ok(())
}
