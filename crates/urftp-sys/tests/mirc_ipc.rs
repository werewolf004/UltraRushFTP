// Test code: a failed unwrap/expect is a test failure.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Complete chain in a single process: fake mIRC window ↔ DLL (real exported functions)
//! ↔ `TfmRush` server of the application.
//!
//! If FTPRush or UltraRushFTP is already running, `FindWindow("TfmRush")` may find it
//! instead of the test server: close them before running this test.
#![cfg(windows)]

use std::sync::Mutex;
use std::sync::mpsc::channel;
use std::time::Duration;

use urftp_sys::mirc::{Invoke, LoadDll, LoadInfo, RushCommand, RushScript, callback_hwnd};
use urftp_sys::server::{Request, Server, send_raw_callback};
use urftp_sys::win::{SharedMem, wide};
use urftp_sys::{MIRC_MAP_NAME, SHARED_MAP_SIZE, WM_MCOMMAND};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DefWindowProcW, HWND_MESSAGE, RegisterClassExW, WNDCLASSEXW};

/// Commands received by the fake mIRC window.
static MIRC_RECEIVED: Mutex<Vec<String>> = Mutex::new(Vec::new());

unsafe extern "system" fn fake_mirc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_MCOMMAND {
        let mem = SharedMem::open(MIRC_MAP_NAME, SHARED_MAP_SIZE).expect("mIRC shared memory");
        MIRC_RECEIVED.lock().unwrap().push(mem.read_string());
        return 1;
    }
    // SAFETY: default processing with the received arguments.
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

fn fake_mirc_window() -> HWND {
    // SAFETY: class registration and message-only window creation with valid NUL-terminated strings.
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

/// mIRC-like buffer (900 bytes) containing `s`.
fn buffer(s: &str) -> Vec<u8> {
    let mut b = vec![0u8; 900];
    b[..s.len()].copy_from_slice(s.as_bytes());
    b
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b.split(|&c| c == 0).next().unwrap()).into_owned()
}

fn other_tfmrush_window() -> bool {
    let class = wide(&urftp_sys::window_class());
    // SAFETY: valid NUL-terminated class name; null title matches any window.
    !unsafe { windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(class.as_ptr(), std::ptr::null()) }.is_null()
}

#[test]
fn invoke_script_and_raw_callback_round_trip() {
    // Window class private to the test: a real FTPRush or UltraRushFTP ("TfmRush" class)
    // thus never receives the commands of the test.
    // SAFETY: only test of this binary; no other thread reads the environment concurrently.
    unsafe { std::env::set_var("URUSHFTP_IPC_CLASS", format!("TfmRush.UltraRushFTP.Test.{}", std::process::id())) };
    assert!(!other_tfmrush_window());
    // Application side: answers Invoke, forwards the scripts.
    let (tx, rx) = channel::<(String, Option<isize>, bool)>();
    let server = Server::start(move |req| match req {
        Request::Invoke { expression, reply } => {
            let _ = reply.send(format!("answer to {expression}"));
        }
        Request::Script { source, callback } => {
            let _ = tx.send((source, callback, false));
        }
        Request::Command { source } => {
            let _ = tx.send((source, None, true));
        }
        Request::Show => {}
    })
    .expect("serveur TfmRush");

    // mIRC side: loading the DLL.
    let mut info = LoadInfo { m_version: 7 | (81 << 16), m_hwnd: std::ptr::null_mut(), m_keep: 0, m_unicode: 0, m_beta: 0, m_bytes: 4096 };
    // SAFETY: `info` is a valid LOADINFO for the duration of the call.
    unsafe { LoadDll(&mut info) };
    assert_eq!(info.m_keep, 1, "the DLL asks to stay loaded");
    assert_ne!(callback_hwnd(), 0, "callback window created");

    // Invoke: synchronous request, answer written into the mIRC buffer.
    let mut data = buffer("RushApp.TotalDownloadSize");
    // SAFETY: `data` is a writable 900-byte buffer, like the mIRC one.
    let r = unsafe { Invoke(std::ptr::null_mut(), std::ptr::null_mut(), data.as_mut_ptr(), std::ptr::null_mut(), 0, 0) };
    assert_eq!(r, 3, "3 = the value is returned to mIRC");
    assert_eq!(text(&data), "answer to RushApp.TotalDownloadSize");

    // RushScript: the script reaches the application with the DLL callback window.
    let mirc = fake_mirc_window();
    let mut data = buffer("RushApp.FTP.RAW('DEV', 'SITE WHO', RS_LOGIN);");
    // SAFETY: `data` is a writable 900-byte buffer, like the mIRC one.
    let r = unsafe { RushScript(mirc, std::ptr::null_mut(), data.as_mut_ptr(), std::ptr::null_mut(), 0, 0) };
    assert_eq!(r, 1);
    let (src, cb, is_command) = rx.recv_timeout(Duration::from_secs(5)).expect("script received");
    assert_eq!(src, "RushApp.FTP.RAW('DEV', 'SITE WHO', RS_LOGIN);");
    assert_eq!(cb, Some(callback_hwnd()));
    assert!(!is_command);

    // The application sends back the RAW reply: it must show up in mIRC.
    let lines = vec!["200- user1".to_string(), "200 End".to_string()];
    assert!(send_raw_callback(cb.unwrap(), true, "DEV", "SITE WHO", &lines));
    assert_eq!(
        MIRC_RECEIVED.lock().unwrap().as_slice(),
        ["/echo -a (DEV) \u{2}SITE WHO", "/echo -a (DEV) 200- user1", "/echo -a (DEV) 200 End"]
    );

    // RushCommand: forwarded without callback.
    let mut data = buffer("RushApp.FTP.Logout('', 0);");
    // SAFETY: `data` is a writable 900-byte buffer, like the mIRC one.
    unsafe { RushCommand(mirc, std::ptr::null_mut(), data.as_mut_ptr(), std::ptr::null_mut(), 0, 0) };
    let (src, cb, is_command) = rx.recv_timeout(Duration::from_secs(5)).expect("command received");
    assert_eq!((src.as_str(), cb, is_command), ("RushApp.FTP.Logout('', 0);", None, true));

    // Without the application: Invoke returns without writing anything (like the original).
    drop(server);
    std::thread::sleep(Duration::from_millis(200));
    if other_tfmrush_window() {
        return; // une application a été ouverte entre-temps : ne rien lui envoyer
    }
    let mut data = buffer("RushApp.Version");
    // SAFETY: `data` is a writable 900-byte buffer, like the mIRC one.
    let r = unsafe { Invoke(std::ptr::null_mut(), std::ptr::null_mut(), data.as_mut_ptr(), std::ptr::null_mut(), 0, 0) };
    assert_eq!(r, 1);
}
