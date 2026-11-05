// Test harness (mock servers, fixtures): a failed unwrap/expect is a test failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Serveur FTP de développement (localhost) réutilisant le serveur des tests d'intégration.
//! Identifiants : `test` / `secret`. Affiche le port choisi puis tourne jusqu'à Ctrl+C.
//!
//! ```text
//! cargo run -p urftp-proto --example dev_server
//! ```

#[path = "../tests/common/mod.rs"]
#[allow(unused_imports)]
mod common;

use common::{MockConfig, MockServer};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let srv = MockServer::start(MockConfig::default()).await;
    {
        let mut fs = srv.fs.lock().unwrap();
        fs.mkdir_p("/pub/Releases/UltraRush.Demo-2026/Sample");
        fs.mkdir_p("/pub/Docs");
        fs.mkdir_p("/incoming");
        let rel = "/pub/Releases/UltraRush.Demo-2026";
        fs.files.insert(format!("{rel}/ultrarush.demo-2026.sfv"), b"; sfv\r\n".to_vec());
        fs.files.insert(format!("{rel}/ultrarush.demo-2026.nfo"), vec![b'#'; 4_096]);
        for i in 0..6 {
            fs.files.insert(format!("{rel}/ultrarush.demo-2026.r{i:02}"), vec![i as u8; 2_500_000]);
        }
        fs.files.insert(format!("{rel}/Sample/sample.mkv"), vec![9; 800_000]);
        fs.files.insert("/pub/Docs/LISEZMOI.txt".into(), "Serveur de développement UltraRushFTP\r\n".into());
        fs.files.insert("/pub/bienvenue.txt".into(), b"Bonjour !".to_vec());
    }
    println!("PORT={}", srv.addr.port());
    tokio::signal::ctrl_c().await.ok();
}
