//! Côté application : fenêtre cachée de classe `TfmRush`, comme FTPRush, pour que
//! `rushmirc.dll` (l'originale ou la nôtre) puisse piloter UltraRushFTP.
//!
//! - `WM_EXCHANGE` : la DLL a écrit une expression dans la mémoire `FTPRush` ; on y écrit
//!   la réponse avant de rendre la main (requête synchrone).
//! - `WM_COPYDATA` (`dwData` 1000 = `RushScript`, 1001 = `RushCommand`) : un script à
//!   exécuter ; `wParam` = fenêtre de rappel de la DLL (ou 0).
//! - [`send_raw_callback`] renvoie à la DLL la réponse d'une commande brute (`wParam` 888).

use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::Duration;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetMessageW, GetWindowLongPtrW, MSG,
    PostMessageW, PostQuitMessage, RegisterClassExW, SMTO_ABORTIFHUNG, SendMessageTimeoutW, SetWindowLongPtrW, TranslateMessage, WM_CLOSE,
    WM_COPYDATA, WM_DESTROY, WM_NCCREATE, WNDCLASSEXW, WS_OVERLAPPED,
};

use crate::win::{SharedMem, decode_ansi, wide};
use crate::{CALLBACK_FAILED, CALLBACK_RAWCOMMAND, CALLBACK_SUCCESS, COPYDATA_COMMAND, COPYDATA_SCRIPT, SHARED_MAP_NAME, SHARED_MAP_SIZE, WM_EXCHANGE};

/// Délai maximal pour répondre à un `Invoke` (mIRC attend pendant ce temps).
const INVOKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Demande reçue de la DLL.
pub enum Request {
    /// Évaluer une expression ; la réponse doit être envoyée sur `reply`.
    Invoke { expression: String, reply: SyncSender<String> },
    /// Exécuter un script ; `callback` = fenêtre de la DLL pour les réponses `RAW`.
    Script { source: String, callback: Option<isize> },
    /// `RushCommand` : exécuté comme un script, sans rappel.
    Command { source: String },
}

type Handler = Box<dyn Fn(Request) + Send>;

/// Serveur en cours ; la fenêtre est détruite quand il est libéré.
pub struct Server {
    hwnd: isize,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    /// Démarre la fenêtre `TfmRush` sur son propre fil. `on_request` est appelé depuis ce
    /// fil : il doit seulement transmettre la demande (canal) et réveiller l'interface.
    pub fn start(on_request: impl Fn(Request) + Send + 'static) -> std::io::Result<Self> {
        let (tx, rx) = sync_channel::<Result<isize, String>>(1);
        let handler: Handler = Box::new(on_request);
        let thread = std::thread::Builder::new().name("urftp-ipc".into()).spawn(move || {
            // SAFETY : création et boucle de messages classiques ; `handler` vit dans la
            // fenêtre (GWLP_USERDATA) et est libéré à WM_DESTROY.
            unsafe {
                let class = wide(&crate::window_class());
                let hinst = GetModuleHandleW(std::ptr::null());
                let wc = WNDCLASSEXW {
                    cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                    lpfnWndProc: Some(wndproc),
                    hInstance: hinst,
                    lpszClassName: class.as_ptr(),
                    ..std::mem::zeroed()
                };
                RegisterClassExW(&wc); // déjà enregistrée : sans importance
                let title = wide("UltraRushFTP");
                let state = Box::into_raw(Box::new(handler));
                // Fenêtre de premier niveau jamais affichée : FindWindow ne voit pas les
                // fenêtres « message-only ».
                let hwnd = CreateWindowExW(
                    0,
                    class.as_ptr(),
                    title.as_ptr(),
                    WS_OVERLAPPED,
                    0,
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    hinst,
                    state.cast(),
                );
                if hwnd.is_null() {
                    drop(Box::from_raw(state));
                    let _ = tx.send(Err(std::io::Error::last_os_error().to_string()));
                    return;
                }
                let _ = tx.send(Ok(hwnd as isize));
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        })?;
        match rx.recv() {
            Ok(Ok(hwnd)) => Ok(Self { hwnd, thread: Some(thread) }),
            Ok(Err(e)) => Err(std::io::Error::other(format!("fenêtre TfmRush : {e}"))),
            Err(_) => Err(std::io::Error::other("fenêtre TfmRush : le fil s'est arrêté")),
        }
    }

    pub fn hwnd(&self) -> isize {
        self.hwnd
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // SAFETY : la fenêtre appartient au fil du serveur ; WM_CLOSE la détruit là-bas.
        unsafe { PostMessageW(self.hwnd as HWND, WM_CLOSE, 0, 0) };
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

unsafe fn handler<'a>(hwnd: HWND) -> Option<&'a Handler> {
    // SAFETY : pointeur posé à WM_NCCREATE, valide jusqu'à WM_DESTROY.
    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as isize as *const Handler).as_ref() }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY : messages Win32 standard ; les pointeurs viennent du système ou de la DLL.
    unsafe {
        match msg {
            WM_NCCREATE => {
                let cs = &*(lparam as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as _);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_EXCHANGE => {
                let Some(h) = handler(hwnd) else { return 0 };
                let Some(mem) = SharedMem::open(SHARED_MAP_NAME, SHARED_MAP_SIZE) else { return 0 };
                let expression = mem.read_string();
                let (reply_tx, reply_rx) = sync_channel(1);
                h(Request::Invoke { expression, reply: reply_tx });
                let answer = reply_rx.recv_timeout(INVOKE_TIMEOUT).unwrap_or_default();
                mem.write_string(&answer);
                1
            }
            WM_COPYDATA => {
                let Some(h) = handler(hwnd) else { return 0 };
                let cds = &*(lparam as *const COPYDATASTRUCT);
                if cds.lpData.is_null() {
                    return 0;
                }
                let bytes = std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize);
                let source = decode_ansi(bytes);
                match cds.dwData {
                    COPYDATA_SCRIPT => h(Request::Script { source, callback: (wparam != 0).then_some(wparam as isize) }),
                    COPYDATA_COMMAND => h(Request::Command { source }),
                    _ => return 0,
                }
                1
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                let p = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as isize as *mut Handler;
                if !p.is_null() {
                    drop(Box::from_raw(p));
                }
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

/// Renvoie à la DLL la réponse d'une commande brute : lignes « site », « commande », puis
/// les lignes de la réponse (format de l'original). Renvoie `false` si la fenêtre a disparu.
pub fn send_raw_callback(callback: isize, ok: bool, site: &str, command: &str, lines: &[String]) -> bool {
    let mut text = format!("{site}\r\n{command}\r\n");
    for l in lines {
        text.push_str(l);
        text.push_str("\r\n");
    }
    let mut bytes = text.into_bytes();
    bytes.push(0);
    let cds = COPYDATASTRUCT {
        dwData: if ok { CALLBACK_SUCCESS } else { CALLBACK_FAILED },
        cbData: bytes.len() as u32,
        lpData: bytes.as_mut_ptr().cast(),
    };
    let mut result = 0usize;
    // SAFETY : `cds` et `bytes` vivent pendant l'appel synchrone.
    let sent = unsafe {
        SendMessageTimeoutW(callback as HWND, WM_COPYDATA, CALLBACK_RAWCOMMAND, &cds as *const _ as LPARAM, SMTO_ABORTIFHUNG, 5000, &mut result)
    };
    sent != 0
}
