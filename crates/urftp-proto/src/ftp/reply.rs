//! Reading FTP server replies (RFC 959 § 4.2), multi-line ones included.

use std::fmt;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};

use super::FtpError;

/// Maximum number of lines accepted in a multi-line reply.
const MAX_REPLY_LINES: usize = 10_000;

/// Complete server reply: three-digit code and raw lines (without line ends).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub code: u16,
    pub lines: Vec<String>,
}

impl Reply {
    /// 1xx: positive preliminary reply (the transfer starts).
    pub fn is_preliminary(&self) -> bool {
        (100..200).contains(&self.code)
    }
    /// 2xx: command completed successfully.
    pub fn is_completion(&self) -> bool {
        (200..300).contains(&self.code)
    }
    /// 3xx: the command expects more (e.g. `331` after `USER`).
    pub fn is_intermediate(&self) -> bool {
        (300..400).contains(&self.code)
    }
    /// 4xx: transient failure.
    pub fn is_transient_negative(&self) -> bool {
        (400..500).contains(&self.code)
    }
    /// 5xx: permanent failure.
    pub fn is_permanent_negative(&self) -> bool {
        (500..600).contains(&self.code)
    }

    /// Text of the last line, without the code.
    pub fn text(&self) -> &str {
        self.lines.last().map(|l| strip_code(l)).unwrap_or("")
    }

    /// Full text, one line per reply line, without the codes.
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

/// Code at the start of the line when it has the form `ddd`, `ddd ` or `ddd-`.
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

/// Reads a line ending with `\n` (at most `max_len` bytes), without `\r\n`.
pub(crate) async fn read_line<R>(r: &mut R, max_len: usize, enc: &'static encoding_rs::Encoding) -> Result<String, FtpError>
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
            FtpError::Protocol(format!("reply line too long (> {max_len} bytes)"))
        } else {
            FtpError::Closed
        });
    }
    while matches!(buf.last(), Some(b'\n' | b'\r')) {
        buf.pop();
    }
    Ok(enc.decode_without_bom_handling(&buf).0.into_owned())
}

/// Reads a complete reply. A multi-line reply starts with `ddd-` and ends with a
/// `ddd ` line carrying the same code.
pub(crate) async fn read_reply<R>(r: &mut R, max_line: usize, enc: &'static encoding_rs::Encoding) -> Result<Reply, FtpError>
where
    R: AsyncBufRead + Unpin + ?Sized,
{
    let first = read_line(r, max_line, enc).await?;
    let code = parse_code(&first).ok_or_else(|| FtpError::Protocol(format!("invalid reply: {first:?}")))?;
    let multi = first.as_bytes().get(3) == Some(&b'-');
    let mut lines = vec![first];
    if multi {
        loop {
            let line = read_line(r, max_line, enc).await?;
            let b = line.as_bytes();
            let end = parse_code(&line) == Some(code) && (b.len() == 3 || b[3] == b' ');
            lines.push(line);
            if end {
                break;
            }
            if lines.len() > MAX_REPLY_LINES {
                return Err(FtpError::Protocol("multi-line reply too long".into()));
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
        read_reply(&mut r, 1024, encoding_rs::UTF_8).await
    }

    #[tokio::test]
    async fn other_character_set() {
        let mut r: &[u8] = b"250 R\xe9pertoire chang\xe9\r\n";
        let reply = read_reply(&mut r, 1024, encoding_rs::WINDOWS_1252).await.unwrap();
        assert_eq!(reply.text(), "Répertoire changé");
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
        let r = parse("211-Features:\r\n MDTM\r\n211-this is not the end\r\n PRET\r\n211 End\r\n").await.unwrap();
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
