//! Startup options, with the FTPRush v2 syntax:
//!
//! | Option | Effect |
//! |---|---|
//! | `-d` | debug mode |
//! | `-w<n>` | open n tabs |
//! | `-c<url>` | connect (`ftp://user:pass@host:port`, `ftps://`, `ftpsi://`, `sftp://`, site name or host only) |
//! | `-f"folder"` | settings in this folder; `-f` alone: program folder (portable) |
//! | `-r[t][x]` | start the queues at startup; `t`: minimise the window; `x`: quit when nothing is left to transfer |
//! | `-s"file"` | run this script (FTPRush Pascal Script) at startup — UltraRushFTP addition |
//! | `-b` | v2 "BetaUser" licence: not applicable, ignored |

use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartupOptions {
    pub debug: bool,
    pub windows: Option<usize>,
    pub connect: Option<String>,
    /// `Some(None)`: `-f` alone (program folder).
    pub data_dir: Option<Option<PathBuf>>,
    pub run_queues: bool,
    pub minimize: bool,
    pub exit_when_done: bool,
    pub script_file: Option<PathBuf>,
    /// Unrecognised arguments (reported in the log).
    pub unknown: Vec<String>,
}

fn unquote(s: &str) -> &str {
    s.trim().trim_matches('"')
}

pub fn parse(args: impl IntoIterator<Item = String>) -> StartupOptions {
    let mut o = StartupOptions::default();
    for arg in args {
        let a = arg.trim();
        let Some(rest) = a.strip_prefix('-').or_else(|| a.strip_prefix('/')) else {
            o.unknown.push(arg);
            continue;
        };
        let mut chars = rest.chars();
        let Some(flag) = chars.next() else {
            o.unknown.push(arg);
            continue;
        };
        let value = chars.as_str();
        match flag.to_ascii_lowercase() {
            'd' if value.is_empty() => o.debug = true,
            'b' if value.is_empty() => {}
            'w' => match value.parse::<usize>() {
                Ok(n) if (1..=50).contains(&n) => o.windows = Some(n),
                _ => o.unknown.push(arg),
            },
            'c' if !value.is_empty() => o.connect = Some(unquote(value).to_string()),
            'f' => o.data_dir = Some((!unquote(value).is_empty()).then(|| PathBuf::from(unquote(value)))),
            's' if !value.is_empty() => o.script_file = Some(PathBuf::from(unquote(value))),
            'r' => {
                o.run_queues = true;
                for c in value.chars() {
                    match c.to_ascii_lowercase() {
                        't' => o.minimize = true,
                        'x' => o.exit_when_done = true,
                        _ => {}
                    }
                }
            }
            _ => o.unknown.push(arg),
        }
    }
    o
}

/// Settings folder requested by `-f` (relative: from the current folder).
pub fn resolve_data_dir(d: &Option<PathBuf>) -> Option<PathBuf> {
    match d {
        Some(p) => Some(p.clone()),
        None => std::env::current_exe().ok()?.parent().map(PathBuf::from),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> StartupOptions {
        parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn v2_syntax() {
        assert!(p(&["-d"]).debug);
        assert_eq!(p(&["-w10"]).windows, Some(10));
        assert_eq!(p(&["-c127.0.0.1"]).connect.as_deref(), Some("127.0.0.1"));
        assert_eq!(p(&["-fc:\\bak1"]).data_dir, Some(Some(PathBuf::from("c:\\bak1"))));
        assert_eq!(p(&["-f\"c:\\bak 1\""]).data_dir, Some(Some(PathBuf::from("c:\\bak 1"))));
        assert_eq!(p(&["-f"]).data_dir, Some(None));
        let r = p(&["-rxt"]);
        assert!(r.run_queues && r.minimize && r.exit_when_done);
        let r = p(&["-r"]);
        assert!(r.run_queues && !r.minimize && !r.exit_when_done);
        assert_eq!(p(&["-s\"C:\\jobs\\nuit.pas\""]).script_file, Some(PathBuf::from("C:\\jobs\\nuit.pas")));
        assert!(p(&["-b"]).unknown.is_empty());
    }

    #[test]
    fn combined_and_invalid() {
        let o = p(&["-w2", "-cftp://u:p@h:21", "-rx", "/d"]);
        assert_eq!(
            (o.windows, o.connect.as_deref(), o.run_queues, o.exit_when_done, o.debug),
            (Some(2), Some("ftp://u:p@h:21"), true, true, true)
        );
        assert_eq!(p(&["-w0", "-wx", "-z", "fichier.txt"]).unknown.len(), 4);
    }
}
