//! Lecture des réponses du serveur FTP (RFC 959 § 4.2), y compris multi-lignes.

use std::fmt;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};

use super::FtpError;

/// Nombre maximal de lignes acceptées dans une réponse multi-lignes.
const MAX_REPLY_LINES: usize = 10_000;

/// Réponse complète du serveur : code à trois chiffres et lignes brutes (sans fin de ligne).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub code: u16,
    pub lines: Vec<String>,
}

impl Reply {
    /// 1xx : réponse préliminaire positive (le transfert commence).
    pub fn is_preliminary(&self) -> bool {
        (100..200).contains(&self.code)
    }
    /// 2xx : commande terminée avec succès.
    pub fn is_completion(&self) -> bool {
        (200..300).contains(&self.code)
    }
    /// 3xx : la commande attend une suite (ex. `331` après `USER`).
    pub fn is_intermediate(&self) -> bool {
        (300..400).contains(&self.code)
    }
    /// 4xx : échec temporaire.
    pub fn is_transient_negative(&self) -> bool {
        (400..500).contains(&self.code)
    }
    /// 5xx : échec définitif.
    pub fn is_permanent_negative(&self) -> bool {
        (500..600).contains(&self.code)
    }

    /// Texte de la dernière ligne, sans le code.
    pub fn text(&self) -> &str {
        self.lines.last().map(|l| strip_code(l)).unwrap_or("")
    }

    /// Texte complet, une ligne par ligne de la réponse, sans les codes.
    pub fn message(&self) -> String {
        self.lines.iter().map(|l| strip_code(l)).collect::<Vec<_>>().join("\n")
    }
}

impl fmt::Display for Reply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.code, self.text())
    }
}

fn strip_code(line: &str) -> &str {
    if parse_code(line).is_some() { line.get(4..).unwrap_or("") } else { line.trim_start() }
}

/// Code en tête de ligne si celle-ci a la forme `ddd`, `ddd ` ou `ddd-`.
fn parse_code(line: &str) -> Option<u16> {
    let b = line.as_bytes();
    if b.len() < 3 || !b[..3].iter().all(u8::is_ascii_digit) {
        return None;
    }
    if b.len() > 3 && b[3] != b' ' && b[3] != b'-' {
        return None;
    }
    line[..3].parse().ok()
}

/// Lit une ligne terminée par `\n` (au plus `max_len` octets), sans `\r\n`.
pub(crate) async fn read_line<R>(r: &mut R, max_len: usize) -> Result<String, FtpError>
where
    R: AsyncBufRead + Unpin + ?Sized,
{
    let mut buf = Vec::new();
    let n = (&mut *r).take(max_len as u64).read_until(b'\n', &mut buf).await?;
    if n == 0 {
        return Err(FtpError::Closed);
    }
    if buf.last() != Some(&b'\n') {
        return Err(if buf.len() >= max_len {
            FtpError::Protocol(format!("ligne de réponse trop longue (> {max_len} octets)"))
        } else {
            FtpError::Closed
        });
    }
    while matches!(buf.last(), Some(b'\n' | b'\r')) {
        buf.pop();
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Lit une réponse complète. Une réponse multi-lignes commence par `ddd-` et se termine
/// par une ligne `ddd ` portant le même code.
pub(crate) async fn read_reply<R>(r: &mut R, max_line: usize) -> Result<Reply, FtpError>
where
    R: AsyncBufRead + Unpin + ?Sized,
{
    let first = read_line(r, max_line).await?;
    let code = parse_code(&first).ok_or_else(|| FtpError::Protocol(format!("réponse invalide : {first:?}")))?;
    let multi = first.as_bytes().get(3) == Some(&b'-');
    let mut lines = vec![first];
    if multi {
        loop {
            let line = read_line(r, max_line).await?;
            let b = line.as_bytes();
            let end = parse_code(&line) == Some(code) && (b.len() == 3 || b[3] == b' ');
            lines.push(line);
            if end {
                break;
            }
            if lines.len() > MAX_REPLY_LINES {
                return Err(FtpError::Protocol("réponse multi-lignes trop longue".into()));
            }
        }
    }
    Ok(Reply { code, lines })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn parse(input: &str) -> Result<Reply, FtpError> {
        let mut r = input.as_bytes();
        read_reply(&mut r, 1024).await
    }

    #[tokio::test]
    async fn single_line() {
        let r = parse("220 Bienvenue\r\n").await.unwrap();
        assert_eq!(r.code, 220);
        assert_eq!(r.text(), "Bienvenue");
        assert!(r.is_completion());
    }

    #[tokio::test]
    async fn multi_line_with_inner_codes() {
        let r = parse("211-Features:\r\n MDTM\r\n211-ceci n'est pas la fin\r\n PRET\r\n211 End\r\n").await.unwrap();
        assert_eq!(r.code, 211);
        assert_eq!(r.lines.len(), 5);
        assert_eq!(r.text(), "End");
    }

    #[tokio::test]
    async fn bare_lf_and_code_only() {
        let r = parse("200\n").await.unwrap();
        assert_eq!(r.code, 200);
        assert_eq!(r.text(), "");
    }

    #[tokio::test]
    async fn errors() {
        assert!(matches!(parse("").await, Err(FtpError::Closed)));
        assert!(matches!(parse("220 incomplet").await, Err(FtpError::Closed)));
        assert!(matches!(parse("abc\r\n").await, Err(FtpError::Protocol(_))));
        let long = format!("220 {}\r\n", "x".repeat(2000));
        assert!(matches!(parse(&long).await, Err(FtpError::Protocol(_))));
    }
}
