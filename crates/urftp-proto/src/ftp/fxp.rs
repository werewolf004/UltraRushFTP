//! FXP : transfert direct de serveur à serveur (RFC 959 § 5.2, « third party transfer »).
//!
//! Le client pilote deux connexions de contrôle : un serveur se met en écoute (`PASV`),
//! l'autre s'y connecte (`PORT`), puis `STOR` sur la destination et `RETR` sur la source.
//! Les données ne passent jamais par le client.
//!
//! FXP chiffré : les deux serveurs sont normalement serveurs TLS ; il faut que l'un des deux
//! se comporte en client TLS. C'est le rôle de `SSCN ON` ou de `CPSV` (à la place de `PASV`),
//! envoyés au côté passif.

use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use super::client::{needs_fix, parse_pasv};
use super::{FtpClient, FtpError, Result, TransferType};

/// Qui se met en écoute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FxpMode {
    /// Ordre normal : la source fait `PASV`, la destination `PORT`.
    SourcePassive,
    /// Ordre inverse (« Use PASV when acts as target FXP FTP » de la v2).
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

/// Méthode de chiffrement du canal de données entre les deux serveurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FxpSecurity {
    /// `SSCN` si le côté passif l'annonce, sinon `CPSV`.
    Auto,
    Sscn,
    Cpsv,
    /// Canal de données en clair (`PROT C` sur les deux serveurs le temps du transfert).
    Clear,
}

/// Réglages d'un transfert FXP.
#[derive(Debug, Clone)]
pub struct FxpOptions {
    pub mode: FxpMode,
    /// Si le côté actif refuse `PORT` (FXP interdit vers une IP étrangère), réessayer en
    /// inversant les rôles.
    pub auto_reverse: bool,
    pub security: FxpSecurity,
    /// Si aucune méthode de FXP chiffré ne marche, transférer en clair plutôt qu'échouer.
    pub allow_clear_fallback: bool,
    /// Reprise : octet de départ sur la source et sur la destination.
    pub offset: u64,
    /// Délai maximal d'attente de la fin du transfert (`None` : illimité).
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

/// Résultat d'un FXP réussi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FxpOutcome {
    /// Rôles effectivement utilisés.
    pub mode: FxpMode,
    /// Rôles inversés automatiquement après un refus de `PORT`.
    pub reversed: bool,
    /// Canal de données chiffré entre les deux serveurs.
    pub encrypted: bool,
    pub elapsed: Duration,
}

/// Méthode retenue pour cette tentative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Method {
    Plain,
    Sscn,
    Cpsv,
}

/// Transfère `src_path` (sur `src`) vers `dst_path` (sur `dst`) en FXP.
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
                let msg = format!("FXP : PORT refusé ({e}), inversion des rôles");
                src.status(msg.clone());
                dst.status(msg);
            }
            Err(Attempt::PortRefused(e) | Attempt::Failed(e)) => return Err(e),
        }
    }
}

enum Attempt {
    /// Le côté actif refuse `PORT` : inverser les rôles peut résoudre le problème.
    PortRefused(FtpError),
    Failed(FtpError),
}

impl From<FtpError> for Attempt {
    fn from(e: FtpError) -> Self {
        Attempt::Failed(e)
    }
}

/// Une tentative ; renvoie « chiffré ? ». Les réglages PROT / SSCN modifiés sont restaurés.
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

    // Remise en état, même après une erreur, pour que les transferts normaux suivants marchent.
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
    // ----- Choix de la protection du canal de données.
    let want = opts.security != FxpSecurity::Clear && (src.is_data_protected() || dst.is_data_protected());
    let both_tls = src.is_secure() && dst.is_secure();
    let mut method = if want && both_tls {
        let passive = if mode == FxpMode::SourcePassive { &*src } else { &*dst };
        match opts.security {
            FxpSecurity::Sscn => Method::Sscn,
            FxpSecurity::Cpsv => Method::Cpsv,
            _ if passive.features().has("SSCN") => Method::Sscn,
            _ => Method::Cpsv,
        }
    } else {
        if want {
            let msg = "FXP chiffré impossible : un des serveurs n'est pas en TLS, transfert en clair";
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
            r => {
                passive.status(format!("SSCN refusé ({r}) : essai de CPSV"));
                method = Method::Cpsv;
            }
        }
    }

    // ----- Côté passif : PRET, puis PASV ou CPSV.
    if passive.features().has("PRET") {
        passive.ok(&format!("PRET {passive_cmd}")).await?;
    }
    let pasv_cmd = if method == Method::Cpsv { "CPSV" } else { "PASV" };
    let mut r = passive.raw(pasv_cmd).await?;
    if r.code != 227 && method == Method::Cpsv {
        if !opts.allow_clear_fallback {
            return Err(Attempt::Failed(FtpError::Unexpected { command: "CPSV".into(), reply: r }));
        }
        passive.status("Ni SSCN ni CPSV : FXP en clair (repli autorisé)");
        method = Method::Plain;
        passive.set_data_protection(false).await?;
        active.set_data_protection(false).await?;
        r = passive.raw("PASV").await?;
    }
    if r.code != 227 {
        return Err(Attempt::Failed(FtpError::Unexpected { command: pasv_cmd.into(), reply: r }));
    }
    let (ip, port) = parse_pasv(r.text()).ok_or_else(|| FtpError::Protocol(format!("réponse {pasv_cmd} illisible : {r}")))?;
    let mut ip = IpAddr::V4(ip);
    if let Some(server) = passive.server_ip()
        && needs_fix(ip, Some(server))
    {
        passive.status(format!("{pasv_cmd} renvoie {ip} : utilisation de {server}"));
        ip = server;
    }
    let addr = SocketAddr::new(ip, port);

    // ----- Côté actif : PORT vers l'adresse du côté passif.
    let IpAddr::V4(v4) = addr.ip() else {
        return Err(Attempt::Failed(FtpError::Protocol("FXP IPv6 non géré (PORT)".into())));
    };
    let o = v4.octets();
    let port_cmd = format!("PORT {},{},{},{},{},{}", o[0], o[1], o[2], o[3], port >> 8, port & 0xff);
    let r = active.raw(&port_cmd).await?;
    if !r.is_completion() {
        let e = FtpError::Unexpected { command: port_cmd, reply: r.clone() };
        return Err(if r.is_permanent_negative() { Attempt::PortRefused(e) } else { Attempt::Failed(e) });
    }

    // ----- Reprise.
    if opts.offset > 0 {
        let rest = format!("REST {}", opts.offset);
        src.expect(&rest, |r| r.code == 350).await?;
        dst.expect(&rest, |r| r.code == 350).await?;
    }

    // ----- STOR sur la destination, puis RETR sur la source.
    let r = dst.raw(&transfer_dst).await?;
    if !r.is_preliminary() {
        return Err(Attempt::Failed(FtpError::Unexpected { command: transfer_dst, reply: r }));
    }
    let r = src.raw(&transfer_src).await?;
    if !r.is_preliminary() {
        let _ = dst.abort().await;
        return Err(Attempt::Failed(FtpError::Unexpected { command: transfer_src, reply: r }));
    }

    // ----- Fin : les deux serveurs répondent quand le transfert est terminé.
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
