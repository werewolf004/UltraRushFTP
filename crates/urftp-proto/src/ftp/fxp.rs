//! FXP: direct server-to-server transfer (RFC 959 § 5.2, "third party transfer").
//!
//! The client drives two control connections: one server listens (`PASV`), the other
//! connects to it (`PORT`), then `STOR` on the destination and `RETR` on the source.
//! The data never goes through the client.
//!
//! Encrypted FXP: both servers are normally TLS servers; one of them has to act as the
//! TLS client. That is what `SSCN ON` or `CPSV` (instead of `PASV`) are for, sent to the
//! passive side.

use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use super::client::{needs_fix, parse_epsv, parse_pasv};
use super::{FtpClient, FtpError, Result, TransferType};

/// Which side listens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FxpMode {
    /// Normal order: the source does `PASV`, the destination `PORT`.
    SourcePassive,
    /// Reverse order ("Use PASV when acts as target FXP FTP" in v2).
    DestinationPassive,
}

impl FxpMode {
    pub fn reversed(self) -> Self {
        match self {
            FxpMode::SourcePassive => FxpMode::DestinationPassive,
            FxpMode::DestinationPassive => FxpMode::SourcePassive,
        }
    }
}

/// Encryption method of the data channel between the two servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FxpSecurity {
    /// `SSCN` when the passive side announces it, `CPSV` otherwise.
    Auto,
    Sscn,
    Cpsv,
    /// Clear data channel (`PROT C` on both servers during the transfer).
    Clear,
}

/// Settings of an FXP transfer.
#[derive(Debug, Clone)]
pub struct FxpOptions {
    pub mode: FxpMode,
    /// When the active side refuses `PORT` (FXP to a foreign IP forbidden), try again
    /// with the roles reversed.
    pub auto_reverse: bool,
    pub security: FxpSecurity,
    /// When no encrypted FXP method works, transfer in clear instead of failing.
    pub allow_clear_fallback: bool,
    /// Resume: start byte on the source and on the destination.
    pub offset: u64,
    /// Maximum wait for the end of the transfer (`None`: unlimited).
    pub completion_timeout: Option<Duration>,
}

impl Default for FxpOptions {
    fn default() -> Self {
        Self {
            mode: FxpMode::SourcePassive,
            auto_reverse: true,
            security: FxpSecurity::Auto,
            allow_clear_fallback: false,
            offset: 0,
            completion_timeout: None,
        }
    }
}

/// Result of a successful FXP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FxpOutcome {
    /// Roles actually used.
    pub mode: FxpMode,
    /// Roles reversed automatically after a `PORT` refusal.
    pub reversed: bool,
    /// Data channel encrypted between the two servers.
    pub encrypted: bool,
    pub elapsed: Duration,
}

/// Method chosen for this attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Method {
    Plain,
    Sscn,
    Cpsv,
}

/// Transfers `src_path` (on `src`) to `dst_path` (on `dst`) with FXP.
pub async fn fxp(src: &mut FtpClient, src_path: &str, dst: &mut FtpClient, dst_path: &str, opts: &FxpOptions) -> Result<FxpOutcome> {
    let start = Instant::now();
    let src_name = src.enter_parent(src_path).await?.to_string();
    let dst_name = dst.enter_parent(dst_path).await?.to_string();
    src.set_type(TransferType::Binary).await?;
    dst.set_type(TransferType::Binary).await?;

    let mut mode = opts.mode;
    let mut reversed = false;
    loop {
        match attempt(src, &src_name, dst, &dst_name, opts, mode).await {
            Ok(encrypted) => return Ok(FxpOutcome { mode, reversed, encrypted, elapsed: start.elapsed() }),
            Err(Attempt::PortRefused(e)) if opts.auto_reverse && !reversed => {
                reversed = true;
                mode = mode.reversed();
                let msg = format!("FXP: PORT refused ({e}), reversing roles");
                src.status(msg.clone());
                dst.status(msg);
            }
            Err(Attempt::PortRefused(e) | Attempt::Failed(e)) => return Err(e),
        }
    }
}

enum Attempt {
    /// The active side refuses `PORT`: reversing the roles may solve it.
    PortRefused(FtpError),
    Failed(FtpError),
}

impl From<FtpError> for Attempt {
    fn from(e: FtpError) -> Self {
        Attempt::Failed(e)
    }
}

/// One attempt; returns "encrypted?". The PROT / SSCN settings changed are restored.
async fn attempt(
    src: &mut FtpClient,
    src_name: &str,
    dst: &mut FtpClient,
    dst_name: &str,
    opts: &FxpOptions,
    mode: FxpMode,
) -> std::result::Result<bool, Attempt> {
    let saved = (src.is_data_protected(), dst.is_data_protected());
    let mut sscn_on = false;
    let result = run(src, src_name, dst, dst_name, opts, mode, &mut sscn_on).await;

    // Restore, even after an error, so that the following normal transfers work.
    let (passive, _) = sides(src, dst, mode);
    if sscn_on {
        let _ = passive.ok("SSCN OFF").await;
    }
    let restore_src = src.set_data_protection(saved.0).await;
    let restore_dst = dst.set_data_protection(saved.1).await;
    let encrypted = result?;
    restore_src?;
    restore_dst?;
    Ok(encrypted)
}

fn sides<'a>(src: &'a mut FtpClient, dst: &'a mut FtpClient, mode: FxpMode) -> (&'a mut FtpClient, &'a mut FtpClient) {
    match mode {
        FxpMode::SourcePassive => (src, dst),
        FxpMode::DestinationPassive => (dst, src),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run(
    src: &mut FtpClient,
    src_name: &str,
    dst: &mut FtpClient,
    dst_name: &str,
    opts: &FxpOptions,
    mode: FxpMode,
    sscn_on: &mut bool,
) -> std::result::Result<bool, Attempt> {
    // ----- Protection of the data channel.
    let want = opts.security != FxpSecurity::Clear && (src.is_data_protected() || dst.is_data_protected());
    let both_tls = src.is_secure() && dst.is_secure();
    let mut method = if want && both_tls {
        let passive = if mode == FxpMode::SourcePassive { &*src } else { &*dst };
        match opts.security {
            FxpSecurity::Sscn => Method::Sscn,
            FxpSecurity::Cpsv => Method::Cpsv,
            _ if passive.features().has("SSCN") => Method::Sscn,
            _ if passive.allows("CPSV") => Method::Cpsv,
            _ => {
                let msg = "encrypted FXP impossible: SSCN is not announced and CPSV is switched off, transferring in clear";
                src.status(msg);
                dst.status(msg);
                Method::Plain
            }
        }
    } else {
        if want {
            let msg = "encrypted FXP impossible: one of the servers is not using TLS, transferring in clear";
            src.status(msg);
            dst.status(msg);
        }
        Method::Plain
    };
    let protect = method != Method::Plain;
    src.set_data_protection(protect).await?;
    dst.set_data_protection(protect).await?;

    let transfer_src = format!("RETR {src_name}");
    let transfer_dst = format!("STOR {dst_name}");
    let (passive, active) = sides(src, dst, mode);
    let passive_cmd = if mode == FxpMode::SourcePassive { &transfer_src } else { &transfer_dst };

    if method == Method::Sscn {
        match passive.raw("SSCN ON").await? {
            r if r.is_completion() => *sscn_on = true,
            r if passive.allows("CPSV") => {
                passive.status(format!("SSCN refused ({r}): trying CPSV"));
                method = Method::Cpsv;
            }
            r => return Err(Attempt::Failed(FtpError::Unexpected { command: "SSCN ON".into(), reply: r })),
        }
    }

    // ----- Passive side: PRET, then PASV or CPSV (EPSV when it is reached over IPv6:
    // PASV can only announce an IPv4 address).
    let ipv6 = passive.server_ip().is_some_and(|ip| ip.to_canonical().is_ipv6());
    if ipv6 && method == Method::Cpsv {
        // CPSV has no extended form: encryption needs SSCN between IPv6 servers.
        if !opts.allow_clear_fallback {
            return Err(Attempt::Failed(FtpError::Protocol("encrypted FXP between IPv6 servers needs SSCN (CPSV is IPv4 only)".into())));
        }
        passive.status("CPSV is IPv4 only: FXP in clear (fallback allowed)");
        method = Method::Plain;
        passive.set_data_protection(false).await?;
        active.set_data_protection(false).await?;
    }
    if passive.features().has("PRET") {
        passive.ok(&format!("PRET {passive_cmd}")).await?;
    }
    let addr = if ipv6 { extended_passive(passive).await? } else { classic_passive(passive, active, opts, &mut method).await? };

    // ----- Active side: PORT (IPv4) or EPRT (IPv6) to the address of the passive side.
    let port_cmd = match addr.ip() {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            format!("PORT {},{},{},{},{},{}", o[0], o[1], o[2], o[3], addr.port() >> 8, addr.port() & 0xff)
        }
        IpAddr::V6(v6) => format!("EPRT |2|{v6}|{}|", addr.port()),
    };
    let r = active.raw(&port_cmd).await?;
    if !r.is_completion() {
        let e = FtpError::Unexpected { command: port_cmd, reply: r.clone() };
        return Err(if r.is_permanent_negative() { Attempt::PortRefused(e) } else { Attempt::Failed(e) });
    }

    // ----- Resume.
    if opts.offset > 0 {
        let rest = format!("REST {}", opts.offset);
        src.expect(&rest, |r| r.code == 350).await?;
        dst.expect(&rest, |r| r.code == 350).await?;
    }

    // ----- STOR on the destination, then RETR on the source.
    let r = dst.raw(&transfer_dst).await?;
    if !r.is_preliminary() {
        return Err(Attempt::Failed(FtpError::Unexpected { command: transfer_dst, reply: r }));
    }
    let r = src.raw(&transfer_src).await?;
    if !r.is_preliminary() {
        let _ = dst.abort().await;
        return Err(Attempt::Failed(FtpError::Unexpected { command: transfer_src, reply: r }));
    }

    // ----- End: both servers reply when the transfer is over.
    let limit = opts.completion_timeout;
    let (rd, rs) = tokio::join!(dst.read_reply_within(limit), src.read_reply_within(limit));
    let (rd, rs) = (rd?, rs?);
    if !rs.is_completion() {
        return Err(Attempt::Failed(FtpError::Unexpected { command: transfer_src, reply: rs }));
    }
    if !rd.is_completion() {
        return Err(Attempt::Failed(FtpError::Unexpected { command: transfer_dst, reply: rd }));
    }
    Ok(method != Method::Plain)
}

/// `EPSV`: the passive side only gives a port, on the address of its control connection.
async fn extended_passive(passive: &mut FtpClient) -> std::result::Result<SocketAddr, Attempt> {
    let r = passive.raw("EPSV").await?;
    if r.code != 229 {
        return Err(Attempt::Failed(FtpError::Unexpected { command: "EPSV".into(), reply: r }));
    }
    let port = parse_epsv(r.text()).ok_or_else(|| FtpError::Protocol(format!("unreadable EPSV reply: {r}")))?;
    let ip = passive.server_ip().ok_or_else(|| FtpError::Protocol("EPSV: address of the server unknown".into()))?;
    Ok(SocketAddr::new(ip, port))
}

/// `PASV` or `CPSV` (IPv4), with the clear fallback when `CPSV` is refused.
async fn classic_passive(
    passive: &mut FtpClient,
    active: &mut FtpClient,
    opts: &FxpOptions,
    method: &mut Method,
) -> std::result::Result<SocketAddr, Attempt> {
    let pasv_cmd = if *method == Method::Cpsv { "CPSV" } else { "PASV" };
    let mut r = passive.raw(pasv_cmd).await?;
    if r.code != 227 && *method == Method::Cpsv {
        if !opts.allow_clear_fallback {
            return Err(Attempt::Failed(FtpError::Unexpected { command: "CPSV".into(), reply: r }));
        }
        passive.status("Neither SSCN nor CPSV: FXP in clear (fallback allowed)");
        *method = Method::Plain;
        passive.set_data_protection(false).await?;
        active.set_data_protection(false).await?;
        r = passive.raw("PASV").await?;
    }
    if r.code != 227 {
        return Err(Attempt::Failed(FtpError::Unexpected { command: pasv_cmd.into(), reply: r }));
    }
    let (ip, port) = parse_pasv(r.text()).ok_or_else(|| FtpError::Protocol(format!("unreadable {pasv_cmd} reply: {r}")))?;
    let mut ip = IpAddr::V4(ip);
    if let Some(server) = passive.server_ip()
        && needs_fix(ip, Some(server))
    {
        passive.status(format!("{pasv_cmd} returns {ip}: using {server}"));
        ip = server;
    }
    Ok(SocketAddr::new(ip, port))
}
