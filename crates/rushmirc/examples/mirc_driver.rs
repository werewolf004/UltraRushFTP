//! Simule mIRC : appelle les fonctions de la DLL contre une instance d'UltraRushFTP ouverte,
//! puis affiche ce que mIRC aurait reçu. Développement uniquement.
//!
//! ```text
//! cargo run -p rushmirc --example mirc_driver -- "RushApp.Version" "RushApp.FTP.RAW('DEV','SYST',0);"
//! ```
//! Respecte `URUSHFTP_IPC_CLASS` (même valeur que l'application visée).
#![cfg_attr(not(windows), allow(unused))]

#[cfg(windows)]
fn main() {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use rushmirc::{Invoke, LoadDll, LoadInfo, RushScript};
    use urftp_ipc::win::{SharedMem, wide};
    use urftp_ipc::{MIRC_MAP_NAME, SHARED_MAP_SIZE, WM_MCOMMAND};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, HWND_MESSAGE, MSG, PM_REMOVE, PeekMessageW, RegisterClassExW, WNDCLASSEXW,
    };

    static RECEIVED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    unsafe extern "system" fn fake_mirc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        if msg == WM_MCOMMAND {
            if let Some(mem) = SharedMem::open(MIRC_MAP_NAME, SHARED_MAP_SIZE) {
                RECEIVED.lock().unwrap().push(mem.read_string());
            }
            return 1;
        }
        unsafe { DefWindowProcW(hwnd, msg, w, l) }
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (invoke, script) = (args.first().cloned().unwrap_or_else(|| "RushApp.Version".into()), args.get(1).cloned());

    let mirc = unsafe {
        let class = wide("UltraRushFTP.Driver.FakeMirc");
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
    };
    let mut info = LoadInfo { m_version: 7 | (81 << 16), m_hwnd: mirc, m_keep: 0, m_unicode: 0, m_beta: 0, m_bytes: 4096 };
    unsafe { LoadDll(&mut info) };

    let mut data = vec![0u8; 4096];
    data[..invoke.len()].copy_from_slice(invoke.as_bytes());
    let r = unsafe { Invoke(mirc, std::ptr::null_mut(), data.as_mut_ptr(), std::ptr::null_mut(), 0, 0) };
    let answer = String::from_utf8_lossy(data.split(|&c| c == 0).next().unwrap()).into_owned();
    println!("Invoke({invoke}) -> code {r}, réponse « {answer} »");

    if let Some(script) = script {
        let mut data = vec![0u8; 4096];
        data[..script.len()].copy_from_slice(script.as_bytes());
        unsafe { RushScript(mirc, std::ptr::null_mut(), data.as_mut_ptr(), std::ptr::null_mut(), 0, 0) };
        println!("RushScript envoyé, attente des réponses…");
        // Boucle de messages : la fenêtre de rappel de la DLL reçoit les réponses RAW.
        let end = Instant::now() + Duration::from_secs(6);
        while Instant::now() < end {
            unsafe {
                let mut msg: MSG = std::mem::zeroed();
                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    DispatchMessageW(&msg);
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        for line in RECEIVED.lock().unwrap().iter() {
            println!("mIRC reçoit : {line}");
        }
    }
}

#[cfg(not(windows))]
fn main() {}
