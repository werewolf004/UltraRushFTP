//! TCP connection to a host name with "happy eyeballs" (RFC 8305): when the name has both
//! IPv6 and IPv4 addresses, the families are tried alternately, a new attempt starting
//! every [`ATTEMPT_DELAY`] without waiting for the previous one to fail. The first
//! connection established wins, the others are dropped. A dead IPv6 route therefore costs
//! a quarter of a second instead of a full connection timeout.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use tokio::net::{TcpSocket, TcpStream, lookup_host};
use tokio::task::JoinSet;

/// Wait before starting the next attempt ("Connection Attempt Delay" of RFC 8305).
pub const ATTEMPT_DELAY: Duration = Duration::from_millis(250);

/// Connects to `host:port` (name or IP address).
pub async fn connect(host: &str, port: u16) -> io::Result<TcpStream> {
    connect_from(host, port, None).await
}

/// Connects to `host:port` from the local address `bind` (several network cards); only
/// the addresses of the same family as `bind` are then tried.
pub async fn connect_from(host: &str, port: u16, bind: Option<IpAddr>) -> io::Result<TcpStream> {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host.parse::<IpAddr>() {
        return connect_addr(SocketAddr::new(ip, port), bind).await;
    }
    let mut addrs: Vec<SocketAddr> = lookup_host((host, port)).await?.collect();
    if let Some(b) = bind {
        addrs.retain(|a| a.is_ipv6() == b.is_ipv6());
        if addrs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                format!("the host name has no {} address for the local address {b}", if b.is_ipv6() { "IPv6" } else { "IPv4" }),
            ));
        }
    }
    connect_any_from(interleave(addrs), ATTEMPT_DELAY, bind).await
}

/// Send and receive buffers of a data connection (`None`: left to the system).
pub fn set_buffers(tcp: &TcpStream, size: Option<usize>) -> io::Result<()> {
    if let Some(n) = size {
        let s = socket2::SockRef::from(tcp);
        s.set_recv_buffer_size(n)?;
        s.set_send_buffer_size(n)?;
    }
    Ok(())
}

/// Connects to one address, from the local address `bind` when given.
pub async fn connect_addr(addr: SocketAddr, bind: Option<IpAddr>) -> io::Result<TcpStream> {
    let Some(local) = bind else { return TcpStream::connect(addr).await };
    if local.is_ipv6() != addr.is_ipv6() {
        return Err(io::Error::new(io::ErrorKind::AddrNotAvailable, format!("{addr} cannot be reached from the local address {local}")));
    }
    let socket = if local.is_ipv6() { TcpSocket::new_v6()? } else { TcpSocket::new_v4()? };
    socket.bind(SocketAddr::new(local, 0))?;
    socket.connect(addr).await
}

/// Orders the addresses by alternating the families, starting with the family of the
/// first address returned by the resolver (the system's preference).
pub fn interleave(addrs: Vec<SocketAddr>) -> Vec<SocketAddr> {
    let Some(first) = addrs.first() else { return addrs };
    let first_v6 = first.is_ipv6();
    let (mut preferred, mut other): (Vec<_>, Vec<_>) = addrs.into_iter().partition(|a| a.is_ipv6() == first_v6);
    preferred.reverse();
    other.reverse();
    let mut out = Vec::with_capacity(preferred.len() + other.len());
    loop {
        match (preferred.pop(), other.pop()) {
            (None, None) => return out,
            (a, b) => out.extend(a.into_iter().chain(b)),
        }
    }
}

/// Tries the addresses in order, starting a new attempt every `delay` or as soon as the
/// previous one fails; returns the first connection established, or the last error.
pub async fn connect_any(addrs: Vec<SocketAddr>, delay: Duration) -> io::Result<TcpStream> {
    connect_any_from(addrs, delay, None).await
}

async fn connect_any_from(addrs: Vec<SocketAddr>, delay: Duration, bind: Option<IpAddr>) -> io::Result<TcpStream> {
    let mut pending = addrs.into_iter();
    let mut running: JoinSet<io::Result<TcpStream>> = JoinSet::new();
    let mut last_error = io::Error::new(io::ErrorKind::NotFound, "the host name has no address");
    let Some(first) = pending.next() else { return Err(last_error) };
    running.spawn(connect_addr(first, bind));
    loop {
        tokio::select! {
            done = running.join_next() => {
                match done {
                    Some(Ok(Ok(stream))) => {
                        running.abort_all();
                        return Ok(stream);
                    }
                    Some(Ok(Err(e))) => last_error = e,
                    Some(Err(e)) => last_error = io::Error::other(e),
                    None => {}
                }
                // A failure starts the next attempt at once.
                match pending.next() {
                    Some(next) => {
                        running.spawn(connect_addr(next, bind));
                    }
                    None if running.is_empty() => return Err(last_error),
                    None => {}
                }
            }
            _ = tokio::time::sleep(delay), if pending.len() > 0 => {
                if let Some(next) = pending.next() {
                    running.spawn(connect_addr(next, bind));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};
    use std::time::Instant;
    use tokio::net::TcpListener;

    fn v4(p: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), p)
    }
    fn v6(p: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), p)
    }

    #[test]
    fn families_alternate_from_the_first_one() {
        assert_eq!(interleave(vec![v6(1), v6(2), v4(3), v4(4), v4(5)]), [v6(1), v4(3), v6(2), v4(4), v4(5)]);
        assert_eq!(interleave(vec![v4(1), v6(2), v6(3)]), [v4(1), v6(2), v6(3)]);
        assert!(interleave(Vec::new()).is_empty());
    }

    /// Address that accepts nothing: a port that was just freed (connection refused).
    async fn refused() -> SocketAddr {
        let l = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let a = l.local_addr().expect("addr");
        drop(l);
        a
    }

    #[tokio::test]
    async fn a_refused_address_moves_to_the_next_at_once() {
        let l = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let good = l.local_addr().expect("addr");
        let t0 = Instant::now();
        let s = connect_any(vec![refused().await, good], Duration::from_secs(10)).await.expect("second address");
        assert_eq!(s.peer_addr().expect("peer"), good);
        assert!(t0.elapsed() < Duration::from_secs(5), "no wait for the delay after a failure");
    }

    #[tokio::test]
    async fn a_silent_address_does_not_block_the_next() {
        // 192.0.2.1 (TEST-NET-1) is never routed: the attempt hangs or fails, the second
        // one starts after the delay anyway.
        let l = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let good = l.local_addr().expect("addr");
        let t0 = Instant::now();
        let s = connect_any(vec![v4(9), good], Duration::from_millis(100)).await.expect("second address");
        assert_eq!(s.peer_addr().expect("peer"), good);
        assert!(t0.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn every_address_failing_gives_the_last_error() {
        let e = connect_any(vec![refused().await, refused().await], ATTEMPT_DELAY).await.expect_err("all refused");
        assert_eq!(e.kind(), io::ErrorKind::ConnectionRefused);
        assert_eq!(connect_any(Vec::new(), ATTEMPT_DELAY).await.expect_err("empty").kind(), io::ErrorKind::NotFound);
    }

    #[tokio::test]
    async fn names_and_literals() {
        let l = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = l.local_addr().expect("addr").port();
        connect("127.0.0.1", port).await.expect("literal");
        connect("localhost", port).await.expect("name with IPv6 and IPv4 addresses");
    }
}
