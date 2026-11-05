//! Chaîne complète dans un seul processus : fausse fenêtre mIRC ↔ DLL (vraies fonctions
//! exportées) ↔ serveur `TfmRush` de l'application.
//!
//! Si FTPRush ou UltraRushFTP est déjà ouvert, `FindWindow("TfmRush")` peut le trouver à la
//! place du serveur de test : fermez-les avant de lancer ce test.
#![cfg(windows)]

use std::sync::Mutex;
use std::sync::mpsc::channel;
use std::time::Duration;

use rushmirc::{Invoke, LoadDll, LoadInfo, RushCommand, RushScript, callback_hwnd};
use urftp_ipc::server::{Request, Server, send_raw_callback};
use urftp_ipc::win::{SharedMem, wide};
use urftp_ipc::{MIRC_MAP_NAME, SHARED_MAP_SIZE, WM_MCOMMAND};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DefWindowProcW, HWND_MESSAGE, RegisterClassExW, WNDCLASSEXW};

/// Commandes reçues par la fausse fenêtre mIRC.
static MIRC_RECEIVED: Mutex<Vec<String>> = Mutex::new(Vec::new());

unsafe extern "system" fn fake_mirc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_MCOMMAND {
        let mem = SharedMem::open(MIRC_MAP_NAME, SHARED_MAP_SIZE).expect("mémoire mIRC");
        MIRC_RECEIVED.lock().unwrap().push(mem.read_string());
        return 1;
    }
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

fn fake_mirc_window() -> HWND {
    unsafe {
        let class = wide("UltraRushFTP.Test.FakeMirc");
        let hinst = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(fake_mirc),
            hInstance: hinst,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassExW(&wc);
        CreateWindowExW(0, class.as_ptr(), class.as_ptr(), 0, 0, 0, 0, 0, HWND_MESSAGE, std::ptr::null_mut(), hinst, std::ptr::null())
    }
}

/// Tampon façon mIRC (900 octets) contenant `s`.
fn buffer(s: &str) -> Vec<u8> {
    let mut b = vec![0u8; 900];
    b[..s.len()].copy_from_slice(s.as_bytes());
    b
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b.split(|&c| c == 0).next().unwrap()).into_owned()
}

fn other_tfmrush_window() -> bool {
    let class = wide(&urftp_ipc::window_class());
    !unsafe { windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(class.as_ptr(), std::ptr::null()) }.is_null()
}

#[test]
fn invoke_script_and_raw_callback_round_trip() {
    // Classe de fenêtre propre au test : un vrai FTPRush ou UltraRushFTP ouvert (classe
    // « TfmRush ») ne reçoit ainsi jamais les commandes du test.
    // SAFETY : seul test de ce binaire ; aucun autre fil ne lit l'environnement en même temps.
    unsafe { std::env::set_var("URUSHFTP_IPC_CLASS", format!("TfmRush.UltraRushFTP.Test.{}", std::process::id())) };
    assert!(!other_tfmrush_window());
    // Côté application : répond aux Invoke, transmet les scripts.
    let (tx, rx) = channel::<(String, Option<isize>, bool)>();
    let server = Server::start(move |req| match req {
        Request::Invoke { expression, reply } => {
            let _ = reply.send(format!("réponse à {expression}"));
        }
        Request::Script { source, callback } => {
            let _ = tx.send((source, callback, false));
        }
        Request::Command { source } => {
            let _ = tx.send((source, None, true));
        }
    })
    .expect("serveur TfmRush");

    // Côté mIRC : chargement de la DLL.
    let mut info = LoadInfo { m_version: 7 | (81 << 16), m_hwnd: std::ptr::null_mut(), m_keep: 0, m_unicode: 0, m_beta: 0, m_bytes: 4096 };
    unsafe { LoadDll(&mut info) };
    assert_eq!(info.m_keep, 1, "la DLL demande à rester chargée");
    assert_ne!(callback_hwnd(), 0, "fenêtre de rappel créée");

    // Invoke : requête synchrone, réponse écrite dans le tampon de mIRC.
    let mut data = buffer("RushApp.TotalDownloadSize");
    let r = unsafe { Invoke(std::ptr::null_mut(), std::ptr::null_mut(), data.as_mut_ptr(), std::ptr::null_mut(), 0, 0) };
    assert_eq!(r, 3, "3 = la valeur est renvoyée à mIRC");
    assert_eq!(text(&data), "réponse à RushApp.TotalDownloadSize");

    // RushScript : le script arrive dans l'application avec la fenêtre de rappel de la DLL.
    let mirc = fake_mirc_window();
    let mut data = buffer("RushApp.FTP.RAW('DEV', 'SITE WHO', RS_LOGIN);");
    let r = unsafe { RushScript(mirc, std::ptr::null_mut(), data.as_mut_ptr(), std::ptr::null_mut(), 0, 0) };
    assert_eq!(r, 1);
    let (src, cb, is_command) = rx.recv_timeout(Duration::from_secs(5)).expect("script reçu");
    assert_eq!(src, "RushApp.FTP.RAW('DEV', 'SITE WHO', RS_LOGIN);");
    assert_eq!(cb, Some(callback_hwnd()));
    assert!(!is_command);

    // L'application renvoie la réponse du RAW : elle doit s'afficher dans mIRC.
    let lines = vec!["200- user1".to_string(), "200 End".to_string()];
    assert!(send_raw_callback(cb.unwrap(), true, "DEV", "SITE WHO", &lines));
    assert_eq!(
        MIRC_RECEIVED.lock().unwrap().as_slice(),
        ["/echo -a (DEV) \u{2}SITE WHO", "/echo -a (DEV) 200- user1", "/echo -a (DEV) 200 End"]
    );

    // RushCommand : transmis sans rappel.
    let mut data = buffer("RushApp.FTP.Logout('', 0);");
    unsafe { RushCommand(mirc, std::ptr::null_mut(), data.as_mut_ptr(), std::ptr::null_mut(), 0, 0) };
    let (src, cb, is_command) = rx.recv_timeout(Duration::from_secs(5)).expect("commande reçue");
    assert_eq!((src.as_str(), cb, is_command), ("RushApp.FTP.Logout('', 0);", None, true));

    // Sans application : Invoke rend la main sans rien écrire (comme l'original).
    drop(server);
    std::thread::sleep(Duration::from_millis(200));
    if other_tfmrush_window() {
        return; // une application a été ouverte entre-temps : ne rien lui envoyer
    }
    let mut data = buffer("RushApp.Version");
    let r = unsafe { Invoke(std::ptr::null_mut(), std::ptr::null_mut(), data.as_mut_ptr(), std::ptr::null_mut(), 0, 0) };
    assert_eq!(r, 1);
}
