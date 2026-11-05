//! Parsing of directory listings: `LIST` (Unix, DOS/IIS) and `MLSD` (RFC 3659).

/// Kind of a remote entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryKind {
    File,
    Dir,
    Link,
}

/// Date as given by the server. Unix listings omit the year for recent files
/// (`year = None`) and do not give seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FtpTime {
    pub year: Option<i32>,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// One entry of a remote listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    pub modified: Option<FtpTime>,
    pub permissions: Option<String>,
    pub owner: Option<String>,
    pub group: Option<String>,
    pub link_target: Option<String>,
}

impl RemoteEntry {
    pub fn is_dir(&self) -> bool {
        self.kind == EntryKind::Dir
    }
}

/// Parses a complete `LIST` listing; skips `total N`, `.`, `..` and unknown lines.
pub fn parse_list(text: &str) -> Vec<RemoteEntry> {
    text.lines().filter_map(parse_list_line).filter(|e| !is_dot(&e.name)).collect()
}

/// Parses a complete `MLSD` listing; skips `cdir`, `pdir`, `.` and `..`.
pub fn parse_mlsd(text: &str) -> Vec<RemoteEntry> {
    text.lines().filter_map(parse_mlsd_line).filter(|e| !is_dot(&e.name)).collect()
}

fn is_dot(name: &str) -> bool {
    name == "." || name == ".."
}

/// Parses a `LIST` line in Unix or DOS/IIS format.
pub fn parse_list_line(line: &str) -> Option<RemoteEntry> {
    let line = line.trim_end_matches(['\r', '\n']);
    parse_unix(line).or_else(|| parse_dos(line))
}

/// Splits into words, keeping the start position of each one in the line.
fn tokens(line: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in line.char_indices() {
        match (c.is_whitespace(), start) {
            (true, Some(s)) => {
                out.push((s, &line[s..i]));
                start = None;
            }
            (false, None) => start = Some(i),
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, &line[s..]));
    }
    out
}

fn month(s: &str) -> Option<u8> {
    const M: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let s = s.to_ascii_lowercase();
    M.iter().position(|m| *m == s).map(|i| i as u8 + 1)
}

/// `drwxr-xr-x  2 owner group  4096 Jan  1 12:00 name with spaces`
/// Tolerates a missing group or link count (glftpd, embedded servers...).
fn parse_unix(line: &str) -> Option<RemoteEntry> {
    let kind = match line.chars().next()? {
        'd' => EntryKind::Dir,
        '-' => EntryKind::File,
        'l' => EntryKind::Link,
        _ => return None,
    };
    let t = tokens(line);
    if t.len() < 7 {
        return None;
    }
    // Look for the month: preceded by the size, followed by the day and the time or year.
    let i = (3..=t.len() - 4).find(|&i| {
        month(t[i].1).is_some() && t[i - 1].1.parse::<u64>().is_ok() && t[i + 1].1.parse::<u8>().is_ok_and(|d| (1..=31).contains(&d))
    })?;
    let mut time = FtpTime { month: month(t[i].1)?, day: t[i + 1].1.parse().ok()?, ..FtpTime::default() };
    let ty = t[i + 2].1;
    if let Some((h, m)) = ty.split_once(':') {
        time.hour = h.parse().ok()?;
        time.minute = m.parse().ok()?;
    } else {
        time.year = Some(ty.parse().ok()?);
    }
    let mut name = line[t[i + 3].0..].to_string();
    let mut link_target = None;
    if kind == EntryKind::Link
        && let Some((n, target)) = name.split_once(" -> ")
    {
        link_target = Some(target.to_string());
        name = n.to_string();
    }
    let (owner, group) = match i {
        5.. => (Some(t[2].1.to_string()), Some(t[3].1.to_string())),
        4 => (Some(t[2].1.to_string()), None),
        _ => (None, None),
    };
    Some(RemoteEntry {
        name,
        kind,
        size: t[i - 1].1.parse().ok()?,
        modified: Some(time),
        permissions: Some(t[0].1.to_string()),
        owner,
        group,
        link_target,
    })
}

/// `01-02-24  10:30AM       <DIR>          name` or with a size instead of `<DIR>`.
fn parse_dos(line: &str) -> Option<RemoteEntry> {
    let t = tokens(line);
    if t.len() < 4 {
        return None;
    }
    let d: Vec<&str> = t[0].1.split(['-', '/']).collect();
    if d.len() != 3 {
        return None;
    }
    let (year, month, day) = if d[0].len() == 4 {
        (d[0].parse::<i32>().ok()?, d[1].parse::<u8>().ok()?, d[2].parse::<u8>().ok()?)
    } else {
        let y: i32 = d[2].parse().ok()?;
        let y = match (d[2].len(), y) {
            (2, 0..70) => 2000 + y,
            (2, _) => 1900 + y,
            _ => y,
        };
        (y, d[0].parse().ok()?, d[1].parse().ok()?)
    };
    let tm = t[1].1.to_ascii_uppercase();
    let (clock, pm) = match tm.strip_suffix("PM") {
        Some(c) => (c, Some(true)),
        None => match tm.strip_suffix("AM") {
            Some(c) => (c, Some(false)),
            None => (tm.as_str(), None),
        },
    };
    let (h, m) = clock.split_once(':')?;
    let mut hour: u8 = h.parse().ok()?;
    let minute: u8 = m.parse().ok()?;
    match pm {
        Some(true) if hour < 12 => hour += 12,
        Some(false) if hour == 12 => hour = 0,
        _ => {}
    }
    let (kind, size) =
        if t[2].1.eq_ignore_ascii_case("<DIR>") { (EntryKind::Dir, 0) } else { (EntryKind::File, t[2].1.replace(',', "").parse().ok()?) };
    Some(RemoteEntry {
        name: line[t[3].0..].to_string(),
        kind,
        size,
        modified: Some(FtpTime { year: Some(year), month, day, hour, minute, second: 0 }),
        permissions: None,
        owner: None,
        group: None,
        link_target: None,
    })
}

/// `type=file;size=123;modify=20240101120000; name`
pub fn parse_mlsd_line(line: &str) -> Option<RemoteEntry> {
    let line = line.trim_end_matches(['\r', '\n']);
    let (facts, name) = line.split_once(' ')?;
    let mut kind = None;
    let mut size = 0;
    let mut modified = None;
    let mut permissions = None;
    let mut owner = None;
    let mut group = None;
    for fact in facts.split(';').filter(|f| !f.is_empty()) {
        let (k, v) = fact.split_once('=')?;
        match k.to_ascii_lowercase().as_str() {
            "type" => {
                let v = v.to_ascii_lowercase();
                kind = Some(match v.as_str() {
                    "file" => EntryKind::File,
                    "dir" => EntryKind::Dir,
                    "cdir" | "pdir" => return None,
                    _ if v.contains("link") => EntryKind::Link,
                    _ => EntryKind::File,
                });
            }
            "size" | "sizd" => size = v.parse().unwrap_or(0),
            "modify" => modified = parse_mlsd_time(v),
            "unix.mode" => permissions = Some(v.to_string()),
            "unix.owner" | "unix.uid" => owner = Some(v.to_string()),
            "unix.group" | "unix.gid" => group = Some(v.to_string()),
            _ => {}
        }
    }
    Some(RemoteEntry { name: name.to_string(), kind: kind?, size, modified, permissions, owner, group, link_target: None })
}

/// `YYYYMMDDHHMMSS[.sss]` (UTC).
pub fn parse_mlsd_time(v: &str) -> Option<FtpTime> {
    let v = v.split('.').next()?;
    if v.len() != 14 || !v.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = |a: usize, b: usize| v[a..b].parse::<u8>().ok();
    Some(FtpTime { year: v[0..4].parse().ok(), month: n(4, 6)?, day: n(6, 8)?, hour: n(8, 10)?, minute: n(10, 12)?, second: n(12, 14)? })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_file_dir_link() {
        let e = parse_list_line("-rw-r--r--   1 user group  123456 Jan  5 12:34 mon fichier.rar").unwrap();
        assert_eq!((e.kind, e.size, e.name.as_str()), (EntryKind::File, 123456, "mon fichier.rar"));
        assert_eq!(e.owner.as_deref(), Some("user"));
        let t = e.modified.unwrap();
        assert_eq!((t.year, t.month, t.day, t.hour, t.minute), (None, 1, 5, 12, 34));

        let d = parse_list_line("drwxr-xr-x 2 root root 4096 Dec 31 2023 Releases").unwrap();
        assert_eq!((d.kind, d.modified.unwrap().year), (EntryKind::Dir, Some(2023)));

        let l = parse_list_line("lrwxrwxrwx 1 a b 7 Mar 3 01:02 latest -> /pub/v2").unwrap();
        assert_eq!(l.kind, EntryKind::Link);
        assert_eq!((l.name.as_str(), l.link_target.as_deref()), ("latest", Some("/pub/v2")));
    }

    #[test]
    fn unix_without_group_and_numeric_name() {
        let e = parse_list_line("-rw-r--r-- 1 glftpd 1024 Feb 10 2024 2024 report.txt").unwrap();
        assert_eq!((e.size, e.name.as_str()), (1024, "2024 report.txt"));
        assert_eq!(e.group, None);
    }

    #[test]
    fn dos_lines() {
        let d = parse_list_line("01-02-24  10:30PM       <DIR>          Mes Dossiers").unwrap();
        assert_eq!((d.kind, d.name.as_str()), (EntryKind::Dir, "Mes Dossiers"));
        let t = d.modified.unwrap();
        assert_eq!((t.year, t.hour), (Some(2024), 22));
        let f = parse_list_line("2023-11-30  12:05AM            1,048,576 big.bin").unwrap();
        assert_eq!((f.size, f.modified.unwrap().hour), (1_048_576, 0));
    }

    #[test]
    fn list_skips_noise() {
        let v = parse_list(
            "total 8\r\ndrwxr-xr-x 2 a b 0 Jan 1 00:00 .\r\ndrwxr-xr-x 2 a b 0 Jan 1 00:00 ..\r\n-rw-r--r-- 1 a b 5 Jan 1 00:00 ok\r\n",
        );
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name, "ok");
    }

    #[test]
    fn mlsd_lines() {
        let v = parse_mlsd(
            "type=cdir;modify=20240101000000; .\r\ntype=file;size=42;modify=20240102030405.123;UNIX.mode=0644; a b.txt\r\ntype=dir;modify=20240101000000; sub\r\n",
        );
        assert_eq!(v.len(), 2);
        assert_eq!((v[0].name.as_str(), v[0].size), ("a b.txt", 42));
        assert_eq!(v[0].modified.unwrap().second, 5);
        assert_eq!(v[0].permissions.as_deref(), Some("0644"));
        assert!(v[1].is_dir());
    }
}
