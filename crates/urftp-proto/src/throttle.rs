//! Limitation de débit (« Limit download/upload speed » de la v2).
//!
//! Un [`RateLimiter`] est partagé par tous les transferts concernés (limite globale ou par
//! site). [`Throttled`] enveloppe un lecteur ou un écrivain local : après chaque bloc, il
//! attend le temps nécessaire pour respecter le débit.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use parking_lot::Mutex;
use std::task::{Context, Poll, ready};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::time::{Instant, Sleep};

/// Rafale autorisée : un débit moyen sur une fenêtre de cette durée.
const BURST: Duration = Duration::from_millis(100);

/// Seau à jetons partagé. Débit en octets par seconde ; 0 = illimité.
#[derive(Debug)]
pub struct RateLimiter {
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    rate: u64,
    /// Instant à partir duquel le « crédit » est épuisé.
    next: Instant,
}

impl RateLimiter {
    pub fn new(bytes_per_sec: u64) -> Arc<Self> {
        Arc::new(Self { state: Mutex::new(State { rate: bytes_per_sec, next: Instant::now() }) })
    }

    /// Change le débit en cours de route (0 = illimité).
    pub fn set_rate(&self, bytes_per_sec: u64) {
        let mut s = self.state.lock();
        s.rate = bytes_per_sec;
        s.next = Instant::now();
    }

    pub fn rate(&self) -> u64 {
        self.state.lock().rate
    }

    /// Compte `n` octets et renvoie l'attente nécessaire avant le prochain bloc.
    pub fn consume(&self, n: usize) -> Duration {
        let mut s = self.state.lock();
        if s.rate == 0 || n == 0 {
            return Duration::ZERO;
        }
        let now = Instant::now();
        let start = s.next.max(now.checked_sub(BURST).unwrap_or(now));
        s.next = start + Duration::from_secs_f64(n as f64 / s.rate as f64);
        s.next.saturating_duration_since(now)
    }
}

/// Lecteur ou écrivain soumis à un ou plusieurs limiteurs (global, par site…).
pub struct Throttled<T> {
    inner: T,
    limiters: Vec<Arc<RateLimiter>>,
    sleep: Option<Pin<Box<Sleep>>>,
}

impl<T> Throttled<T> {
    pub fn new(inner: T, limiters: Vec<Arc<RateLimiter>>) -> Self {
        Self { inner, limiters, sleep: None }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    fn account(&mut self, n: usize) {
        let wait = self.limiters.iter().map(|l| l.consume(n)).max().unwrap_or_default();
        if !wait.is_zero() {
            self.sleep = Some(Box::pin(tokio::time::sleep(wait)));
        }
    }

    fn poll_wait(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        if let Some(s) = self.sleep.as_mut() {
            ready!(s.as_mut().poll(cx));
            self.sleep = None;
        }
        Poll::Ready(())
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for Throttled<T> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        let this = &mut *self;
        ready!(this.poll_wait(cx));
        let before = buf.filled().len();
        ready!(Pin::new(&mut this.inner).poll_read(cx, buf))?;
        let n = buf.filled().len() - before;
        this.account(n);
        Poll::Ready(Ok(()))
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for Throttled<T> {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, data: &[u8]) -> Poll<std::io::Result<usize>> {
        let this = &mut *self;
        ready!(this.poll_wait(cx));
        let n = ready!(Pin::new(&mut this.inner).poll_write(cx, data))?;
        this.account(n);
        Poll::Ready(Ok(n))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn limits_read_speed() {
        let data = vec![1u8; 300_000];
        let limiter = RateLimiter::new(300_000); // 300 Ko/s → ~1 s
        let mut r = Throttled::new(data.as_slice(), vec![limiter.clone()]);
        let start = std::time::Instant::now();
        let mut out = Vec::new();
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            let n = r.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        let t = start.elapsed();
        assert_eq!(out.len(), 300_000);
        assert!(t >= Duration::from_millis(700), "trop rapide : {t:?}");
        assert!(t < Duration::from_secs(3), "trop lent : {t:?}");
    }

    #[test]
    fn unlimited_never_waits() {
        let l = RateLimiter::new(0);
        assert_eq!(l.consume(10_000_000), Duration::ZERO);
        l.set_rate(1000);
        assert!(l.consume(10_000) > Duration::from_secs(5));
    }
}
