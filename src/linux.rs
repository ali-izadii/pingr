//! Linux x86_64 ping datagram sockets. All native ABI and descriptor ownership live here.
//!
//! Declarations match Linux sys/socket.h, netinet/in.h, and sys/poll.h.

use std::ffi::{c_int, c_short, c_ulong, c_void};
use std::io;
use std::mem::size_of;
use std::net::Ipv4Addr;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::{Duration, Instant};

const AF_INET: c_int = 2;
const SOCK_DGRAM: c_int = 0x00002;
const SOCK_NONBLOCK: c_int = 0x00800;
const SOCK_CLOEXEC: c_int = 0x80000;
const IPPROTO_ICMP: c_int = 1;
const POLLIN: c_short = 0x001;
const POLLERR: c_short = 0x008;
const POLLHUP: c_short = 0x010;
const POLLNVAL: c_short = 0x020;

#[repr(C)]
struct SockAddrIn {
    family: u16,
    port: u16,
    address: u32,
    padding: [u8; 8],
}

impl SockAddrIn {
    fn new(address: Ipv4Addr) -> Self {
        Self {
            family: AF_INET as u16,
            port: 0,
            address: u32::from_ne_bytes(address.octets()),
            padding: [0; 8],
        }
    }

    fn validate(&self, length: u32) -> io::Result<()> {
        if length as usize != size_of::<Self>() || self.family != AF_INET as u16 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid IPv4 socket address",
            ));
        }
        Ok(())
    }
}

#[repr(C)]
struct PollFd {
    fd: c_int,
    events: c_short,
    revents: c_short,
}

// C prototypes below omit libc-specific macros and attributes.
// On Linux x86_64: socklen_t = u32, size_t = usize, ssize_t = isize,
// and nfds_t = unsigned long (c_ulong).
// C uses generic sockaddr pointers; these declarations specialize them to the
// IPv4 sockaddr_in layout represented by SockAddrIn. PollFd represents pollfd.
unsafe extern "C" {
    // Create a socket; returns its descriptor, or -1 on error.
    // C: int socket(int domain, int type, int protocol);
    fn socket(domain: c_int, kind: c_int, protocol: c_int) -> c_int;

    // Assign a local address; returns 0 on success, or -1 on error.
    // C: int bind(int sockfd, const struct sockaddr *addr, socklen_t addrlen);
    fn bind(fd: c_int, address: *const SockAddrIn, length: u32) -> c_int;

    // Read the local address; addrlen is both buffer capacity and returned length.
    // Returns 0 on success, or -1 on error.
    // C: int getsockname(int sockfd, struct sockaddr *restrict addr,
    //                    socklen_t *restrict addrlen);
    fn getsockname(fd: c_int, address: *mut SockAddrIn, length: *mut u32) -> c_int;

    // Send bytes to a destination; returns bytes sent, or -1 on error.
    // C: ssize_t sendto(int sockfd, const void *buf, size_t len, int flags,
    //                   const struct sockaddr *dest_addr, socklen_t addrlen);
    fn sendto(
        fd: c_int,
        buffer: *const c_void,
        length: usize,
        flags: c_int,
        address: *const SockAddrIn,
        address_length: u32,
    ) -> isize;

    // Receive bytes and the sender's address; returns bytes received, or -1 on error.
    // C: ssize_t recvfrom(int sockfd, void *restrict buf, size_t len, int flags,
    //                     struct sockaddr *restrict src_addr,
    //                     socklen_t *restrict addrlen);
    fn recvfrom(
        fd: c_int,
        buffer: *mut c_void,
        length: usize,
        flags: c_int,
        address: *mut SockAddrIn,
        address_length: *mut u32,
    ) -> isize;

    // Wait up to timeout milliseconds for descriptor events. Returns the number
    // of ready entries, 0 on timeout, or -1 on error.
    // C: int poll(struct pollfd *fds, nfds_t nfds, int timeout);
    fn poll(fds: *mut PollFd, count: c_ulong, timeout: c_int) -> c_int;
}

pub struct PingSocket {
    socket: OwnedFd,
    identifier: u16,
}

impl PingSocket {
    pub fn open() -> io::Result<Self> {
        // SAFETY: these constants select a Linux ICMPv4 datagram socket.
        let fd = unsafe {
            socket(
                AF_INET,
                // `|` is bitwise OR: combine the socket type and flags into one
                // c_int (i32 on Linux x86_64), not three separate arguments.
                //   SOCK_DGRAM    = 0x00002: datagram socket type
                //   SOCK_NONBLOCK = 0x00800: return WouldBlock instead of waiting
                //   SOCK_CLOEXEC  = 0x80000: close the socket when executing a new program
                //   combined     = 0x80802 (526338 in decimal)
                // Linux reads the low bits as the type and the flag bits as options.
                SOCK_DGRAM | SOCK_NONBLOCK | SOCK_CLOEXEC,
                IPPROTO_ICMP,
            )
        };
        if fd == -1 {
            let error = io::Error::last_os_error();
            return Err(if error.kind() == io::ErrorKind::PermissionDenied {
                io::Error::new(
                    error.kind(),
                    format!(
                        "{error}; ask your administrator to allow your group in net.ipv4.ping_group_range"
                    ),
                )
            } else {
                error
            });
        }
        // SAFETY: socket returned a new descriptor; ownership transfers exactly once.
        let socket = unsafe { OwnedFd::from_raw_fd(fd) };
        let mut address = SockAddrIn::new(Ipv4Addr::UNSPECIFIED);
        let mut length = size_of::<SockAddrIn>() as u32;
        // SAFETY: socket is live and address points to an initialized sockaddr_in.
        if unsafe { bind(socket.as_raw_fd(), &address, length) } == -1 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: address and length are writable and describe the full address buffer.
        if unsafe { getsockname(socket.as_raw_fd(), &mut address, &mut length) } == -1 {
            return Err(io::Error::last_os_error());
        }
        address.validate(length)?;
        // Ping sockets use the local port as the kernel-assigned ICMP identifier.
        Ok(Self {
            socket,
            identifier: u16::from_be(address.port),
        })
    }

    pub fn identifier(&self) -> u16 {
        self.identifier
    }

    pub fn send_to(&self, packet: &[u8], target: Ipv4Addr) -> io::Result<()> {
        let address = SockAddrIn::new(target);
        loop {
            // SAFETY: the owned descriptor is live; both input buffers remain valid
            // for the call and their lengths match the supplied allocations.
            let sent = unsafe {
                sendto(
                    self.socket.as_raw_fd(),
                    packet.as_ptr().cast(),
                    packet.len(),
                    0,
                    &address,
                    size_of::<SockAddrIn>() as u32,
                )
            };
            if sent == -1 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if sent as usize != packet.len() {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "incomplete ICMP datagram send",
                ));
            }
            return Ok(());
        }
    }

    /// Returns ICMP bytes (no IPv4 header) and the sender, or None at the deadline.
    pub fn receive_until(
        &self,
        deadline: Instant,
        buffer: &mut [u8; 65535],
    ) -> io::Result<Option<(usize, Ipv4Addr)>> {
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            let mut descriptor = PollFd {
                fd: self.socket.as_raw_fd(),
                events: POLLIN,
                revents: 0,
            };
            // SAFETY: descriptor points to one initialized, writable pollfd.
            let ready = unsafe { poll(&mut descriptor, 1, poll_timeout(deadline - now)) };
            if ready == -1 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            if ready == 0 {
                continue;
            }
            if descriptor.revents & (POLLHUP | POLLNVAL) != 0 {
                return Err(io::Error::other("ping socket closed or invalid"));
            }
            if descriptor.revents & (POLLIN | POLLERR) == 0 {
                return Err(io::Error::other("unexpected ping socket readiness"));
            }
            let mut source = SockAddrIn::new(Ipv4Addr::UNSPECIFIED);
            let mut length = size_of::<SockAddrIn>() as u32;
            // SAFETY: buffer can hold a full IPv4 datagram; source and length are
            // initialized writable buffers. The socket is owned and nonblocking.
            let received = unsafe {
                recvfrom(
                    self.socket.as_raw_fd(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    0,
                    &mut source,
                    &mut length,
                )
            };
            if received == -1 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                if error.kind() == io::ErrorKind::WouldBlock && descriptor.revents & POLLERR == 0 {
                    continue;
                }
                return Err(error);
            }
            source.validate(length)?;
            return Ok(Some((
                received as usize,
                Ipv4Addr::from(source.address.to_ne_bytes()),
            )));
        }
    }
}

fn poll_timeout(remaining: Duration) -> c_int {
    remaining
        .as_nanos()
        .div_ceil(1_000_000)
        .min(c_int::MAX as u128) as c_int
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::align_of;

    #[test]
    fn native_layout_and_network_byte_order() {
        assert_eq!((size_of::<SockAddrIn>(), align_of::<SockAddrIn>()), (16, 4));
        assert_eq!((size_of::<PollFd>(), align_of::<PollFd>()), (8, 4));
        let address = SockAddrIn::new(Ipv4Addr::new(1, 2, 3, 4));
        assert_eq!(address.address.to_ne_bytes(), [1, 2, 3, 4]);
        assert_eq!(address.family, AF_INET as u16);
    }

    #[test]
    fn poll_rounds_up_and_bounds_long_waits() {
        assert_eq!(poll_timeout(Duration::from_nanos(1)), 1);
        assert_eq!(poll_timeout(Duration::from_millis(1)), 1);
        assert_eq!(poll_timeout(Duration::from_micros(1001)), 2);
        assert_eq!(poll_timeout(Duration::MAX), c_int::MAX);
    }

    #[test]
    #[ignore = "requires local networking"]
    fn idle_receive_stops_at_deadline() -> io::Result<()> {
        let udp = std::net::UdpSocket::bind("127.0.0.1:0")?;
        udp.set_nonblocking(true)?;
        let socket = PingSocket {
            socket: udp.into(),
            identifier: 0,
        };
        let mut buffer = [0; 65535];
        let started = Instant::now();
        let deadline = started + Duration::from_millis(30);
        assert!(socket.receive_until(deadline, &mut buffer)?.is_none());
        assert!(Instant::now() >= deadline);
        assert!(started.elapsed() < Duration::from_secs(2));
        Ok(())
    }
}
