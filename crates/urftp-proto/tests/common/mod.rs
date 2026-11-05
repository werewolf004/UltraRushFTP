//! Serveur FTP minimal en mémoire pour les tests d'intégration (localhost uniquement).
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone)]
pub struct MockConfig {
    pub user: String,
    pub pass: String,
    /// Refuse (550) tout argument plus long que cette limite, comme certains serveurs réels.
    pub max_arg_len: Option<usize>,
    pub epsv: bool,
    pub mlsd: bool,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self { user: "test".into(), pass: "secret".into(), max_arg_len: None, epsv: true, mlsd: true }
    }
}

pub struct Fs {
    pub dirs: BTreeSet<String>,
    pub files: BTreeMap<String, Vec<u8>>,
}

impl Fs {
    fn new() -> Self {
        Self { dirs: BTreeSet::from(["/".to_string()]), files: BTreeMap::new() }
    }

    pub fn mkdir_p(&mut self, path: &str) {
        let mut cur = String::new();
        for seg in path.split('/').filter(|s| !s.is_empty()) {
            cur.push('/');
            cur.push_str(seg);
            self.dirs.insert(cur.clone());
        }
    }

    fn children(&self, dir: &str) -> (Vec<String>, Vec<(String, usize)>) {
        let dirs = self.dirs.iter().filter(|d| *d != "/" && parent(d) == dir).map(|d| name(d).to_string()).collect();
        let files =
            self.files.iter().filter(|(f, _)| parent(f) == dir).map(|(f, c)| (name(f).to_string(), c.len())).collect();
        (dirs, files)
    }
}

fn parent(p: &str) -> &str {
    match p.rfind('/') {
        Some(0) | None => "/",
        Some(i) => &p[..i],
    }
}

fn name(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

fn resolve(cwd: &str, arg: &str) -> String {
    let full = if arg.starts_with('/') { arg.to_string() } else { format!("{cwd}/{arg}") };
    let mut parts: Vec<&str> = Vec::new();
    for seg in full.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    format!("/{}", parts.join("/"))
}

pub struct MockServer {
    pub addr: SocketAddr,
    pub fs: Arc<Mutex<Fs>>,
    /// Toutes les commandes reçues, toutes sessions confondues.
    pub commands: Arc<Mutex<Vec<String>>>,
}

impl MockServer {
    pub async fn start(cfg: MockConfig) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let fs = Arc::new(Mutex::new(Fs::new()));
        let commands = Arc::new(Mutex::new(Vec::new()));
        let (fs2, cmds2) = (fs.clone(), commands.clone());
        tokio::spawn(async move {
            while let Ok((s, _)) = listener.accept().await {
                let (fs, cmds, cfg) = (fs2.clone(), cmds2.clone(), cfg.clone());
                tokio::spawn(async move {
                    let _ = session(s, fs, cmds, cfg).await;
                });
            }
        });
        Self { addr, fs, commands }
    }

    pub fn received(&self) -> Vec<String> {
        self.commands.lock().unwrap().clone()
    }
}

struct State {
    cwd: String,
    user_ok: bool,
    logged: bool,
    rest: u64,
    pasv: Option<TcpListener>,
    rnfr: Option<String>,
}

async fn reply(w: &mut OwnedWriteHalf, s: &str) -> std::io::Result<()> {
    w.write_all(s.as_bytes()).await?;
    w.write_all(b"\r\n").await
}

async fn data_conn(st: &mut State) -> Option<TcpStream> {
    let l = st.pasv.take()?;
    l.accept().await.ok().map(|(s, _)| s)
}

async fn session(
    stream: TcpStream,
    fs: Arc<Mutex<Fs>>,
    cmds: Arc<Mutex<Vec<String>>>,
    cfg: MockConfig,
) -> std::io::Result<()> {
    let (rd, mut w) = stream.into_split();
    let mut lines = BufReader::new(rd).lines();
    reply(&mut w, "220 Serveur de test UltraRushFTP").await?;
    let mut st = State { cwd: "/".into(), user_ok: false, logged: false, rest: 0, pasv: None, rnfr: None };

    while let Some(line) = lines.next_line().await? {
        cmds.lock().unwrap().push(line.clone());
        let (verb, arg) = match line.split_once(' ') {
            Some((v, a)) => (v.to_ascii_uppercase(), a.to_string()),
            None => (line.to_ascii_uppercase(), String::new()),
        };
        if !st.logged && !matches!(verb.as_str(), "USER" | "PASS" | "QUIT" | "FEAT") {
            reply(&mut w, "530 Please login").await?;
            continue;
        }
        if cfg.max_arg_len.is_some_and(|m| arg.len() > m) {
            reply(&mut w, "550 Path too long").await?;
            continue;
        }
        let target = resolve(&st.cwd, &arg);
        match verb.as_str() {
            "USER" => {
                st.user_ok = arg == cfg.user;
                reply(&mut w, "331 Password required").await?;
            }
            "PASS" => {
                if st.user_ok && arg == cfg.pass {
                    st.logged = true;
                    reply(&mut w, "230 Logged in").await?;
                } else {
                    reply(&mut w, "530 Login incorrect").await?;
                }
            }
            "FEAT" => {
                let mut s = String::from("211-Features:\r\n SIZE\r\n REST STREAM\r\n UTF8\r\n");
                if cfg.epsv {
                    s.push_str(" EPSV\r\n");
                }
                if cfg.mlsd {
                    s.push_str(" MLST type*;size*;modify*;\r\n");
                }
                s.push_str("211 End");
                reply(&mut w, &s).await?;
            }
            "OPTS" | "NOOP" | "TYPE" | "PRET" => reply(&mut w, "200 OK").await?,
            "SYST" => reply(&mut w, "215 UNIX Type: L8").await?,
            "PWD" => reply(&mut w, &format!("257 \"{}\" is current directory", st.cwd)).await?,
            "CWD" | "CDUP" => {
                let t = if verb == "CDUP" { resolve(&st.cwd, "..") } else { target };
                if fs.lock().unwrap().dirs.contains(&t) {
                    st.cwd = t;
                    reply(&mut w, "250 OK").await?;
                } else {
                    reply(&mut w, "550 No such directory").await?;
                }
            }
            "EPSV" if cfg.epsv => {
                let l = TcpListener::bind("127.0.0.1:0").await?;
                let port = l.local_addr()?.port();
                st.pasv = Some(l);
                reply(&mut w, &format!("229 Entering Extended Passive Mode (|||{port}|)")).await?;
            }
            "PASV" => {
                let l = TcpListener::bind("127.0.0.1:0").await?;
                let port = l.local_addr()?.port();
                st.pasv = Some(l);
                reply(&mut w, &format!("227 Entering Passive Mode (127,0,0,1,{},{})", port >> 8, port & 0xff)).await?;
            }
            "LIST" | "MLSD" => {
                let dir = if arg.is_empty() || arg.starts_with('-') { st.cwd.clone() } else { target };
                let (dirs, files) = fs.lock().unwrap().children(&dir);
                let mut out = String::new();
                for d in dirs {
                    if verb == "MLSD" {
                        out += &format!("type=dir;modify=20240101120000; {d}\r\n");
                    } else {
                        out += &format!("drwxr-xr-x 1 owner group 0 Jan 01 12:00 {d}\r\n");
                    }
                }
                for (f, n) in files {
                    if verb == "MLSD" {
                        out += &format!("type=file;size={n};modify=20240101120000; {f}\r\n");
                    } else {
                        out += &format!("-rw-r--r-- 1 owner group {n} Jan 01 12:00 {f}\r\n");
                    }
                }
                reply(&mut w, "150 Opening data connection").await?;
                let Some(mut d) = data_conn(&mut st).await else {
                    reply(&mut w, "425 No data connection").await?;
                    continue;
                };
                d.write_all(out.as_bytes()).await?;
                d.shutdown().await?;
                reply(&mut w, "226 Transfer complete").await?;
            }
            "REST" => {
                st.rest = arg.parse().unwrap_or(0);
                reply(&mut w, "350 Restarting").await?;
            }
            "RETR" => {
                let content = fs.lock().unwrap().files.get(&target).cloned();
                let rest = std::mem::take(&mut st.rest) as usize;
                let Some(content) = content else {
                    reply(&mut w, "550 No such file").await?;
                    continue;
                };
                reply(&mut w, "150 Opening data connection").await?;
                let Some(mut d) = data_conn(&mut st).await else {
                    reply(&mut w, "425 No data connection").await?;
                    continue;
                };
                d.write_all(&content[rest.min(content.len())..]).await?;
                d.shutdown().await?;
                reply(&mut w, "226 Transfer complete").await?;
            }
            "STOR" | "APPE" => {
                if !fs.lock().unwrap().dirs.contains(parent(&target)) {
                    reply(&mut w, "550 No such directory").await?;
                    continue;
                }
                let rest = std::mem::take(&mut st.rest) as usize;
                reply(&mut w, "150 Opening data connection").await?;
                let Some(mut d) = data_conn(&mut st).await else {
                    reply(&mut w, "425 No data connection").await?;
                    continue;
                };
                let mut buf = Vec::new();
                d.read_to_end(&mut buf).await?;
                {
                    let mut g = fs.lock().unwrap();
                    let file = g.files.entry(target).or_default();
                    if verb != "APPE" {
                        file.truncate(rest);
                    }
                    file.extend_from_slice(&buf);
                }
                reply(&mut w, "226 Transfer complete").await?;
            }
            "SIZE" => {
                let n = fs.lock().unwrap().files.get(&target).map(Vec::len);
                match n {
                    Some(n) => reply(&mut w, &format!("213 {n}")).await?,
                    None => reply(&mut w, "550 No such file").await?,
                }
            }
            "MDTM" => {
                let exists = fs.lock().unwrap().files.contains_key(&target);
                reply(&mut w, if exists { "213 20240102030405" } else { "550 No such file" }).await?;
            }
            "MKD" => {
                let ok = {
                    let mut g = fs.lock().unwrap();
                    let ok = g.dirs.contains(parent(&target)) && !g.dirs.contains(&target);
                    if ok {
                        g.dirs.insert(target.clone());
                    }
                    ok
                };
                if ok {
                    reply(&mut w, &format!("257 \"{target}\" created")).await?;
                } else {
                    reply(&mut w, "550 Cannot create").await?;
                }
            }
            "RMD" => {
                let ok = {
                    let mut g = fs.lock().unwrap();
                    let (d, f) = g.children(&target);
                    let ok = target != "/" && g.dirs.contains(&target) && d.is_empty() && f.is_empty();
                    if ok {
                        g.dirs.remove(&target);
                    }
                    ok
                };
                reply(&mut w, if ok { "250 Removed" } else { "550 Cannot remove" }).await?;
            }
            "DELE" => {
                let ok = fs.lock().unwrap().files.remove(&target).is_some();
                reply(&mut w, if ok { "250 Deleted" } else { "550 No such file" }).await?;
            }
            "RNFR" => {
                let exists = fs.lock().unwrap().files.contains_key(&target);
                if exists {
                    st.rnfr = Some(target);
                    reply(&mut w, "350 Ready for RNTO").await?;
                } else {
                    reply(&mut w, "550 No such file").await?;
                }
            }
            "RNTO" => {
                let moved = st.rnfr.take().and_then(|from| {
                    let mut g = fs.lock().unwrap();
                    let c = g.files.remove(&from)?;
                    g.files.insert(target, c);
                    Some(())
                });
                reply(&mut w, if moved.is_some() { "250 Renamed" } else { "503 Bad sequence" }).await?;
            }
            "QUIT" => {
                reply(&mut w, "221 Bye").await?;
                break;
            }
            _ => reply(&mut w, "502 Not implemented").await?,
        }
    }
    Ok(())
}
