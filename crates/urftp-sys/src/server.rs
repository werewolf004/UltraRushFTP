//! Application side: a hidden window, so that `rushmirc.dll` can drive UltraRushFTP. Its
//! class is `TfmUltraRush`, or `TfmRush` like FTPRush in compatibility mode (needed by the
//! original DLL of FTPRush, but FTPRush v2 then refuses to start).
//!
//! - `WM_EXCHANGE`: the DLL wrote an expression into the `FTPRush` shared memory; the
//!   answer is written back there before returning (synchronous request).
//! - `WM_COPYDATA` (`dwData` 1000 = `RushScript`, 1001 = `RushCommand`): a script to run;
//!   `wParam` = callback window of the DLL (or 0).
//! - [`send_raw_callback`] sends the reply of a raw command back to the DLL (`wParam` 888).

use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::Duration;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW, GWLP_USERDATA, GetMessageW,
    GetWindowLongPtrW, MSG, PostMessageW, PostQuitMessage, RegisterClassExW, SMTO_ABORTIFHUNG, SendMessageTimeoutW, SetWindowLongPtrW,
    TranslateMessage, WM_CLOSE, WM_COPYDATA, WM_DESTROY, WM_NCCREATE, WNDCLASSEXW, WS_OVERLAPPED,
};

use crate::win::{SharedMem, decode_ansi, wide};
use crate::{
    CALLBACK_FAILED, CALLBACK_RAWCOMMAND, CALLBACK_SUCCESS, COPYDATA_COMMAND, COPYDATA_SCRIPT, SHARED_MAP_NAME, SHARED_MAP_SIZE,
    WM_EXCHANGE,
};

/// Maximum time to answer an `Invoke` (mIRC is blocked meanwhile).
const INVOKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Maximum time for the DLL to accept a raw-command callback.
const CALLBACK_TIMEOUT_MS: u32 = 5000;

/// Request received from the DLL.
pub enum Request {
    /// Evaluate an expression; the answer must be sent on `reply`.
    Invoke {
        /// Expression such as `RushApp.Version`.
        expression: String,
        /// Where to send the answer.
        reply: SyncSender<String>,
    },
    /// Run a script; `callback` = DLL window for `RAW` replies.
    Script {
        /// Pascal Script source.
        source: String,
        /// DLL callback window (`None` when callbacks are disabled).
        callback: Option<isize>,
    },
    /// `RushCommand`: run like a script, without callback.
    Command {
        /// Pascal Script source.
        source: String,
    },
}

/// Failure to start the `TfmRush` window.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// The window thread could not be spawned.
    #[error("mIRC link window thread: {0}")]
    Thread(#[source] std::io::Error),
    /// `CreateWindowExW` failed.
    #[error("mIRC link window: {0}")]
    Window(#[source] std::io::Error),
    /// The window thread stopped before reporting.
    #[error("mIRC link window: the thread stopped unexpectedly")]
    ThreadStopped,
}

type Handler = Box<dyn Fn(Request) + Send>;

/// Running server; the window is destroyed when it is dropped.
pub struct Server {
    hwnd: isize,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// Is a top-level window of this class open (FTPRush, another UltraRushFTP...)?
pub fn window_exists(class: &str) -> bool {
    let class = wide(class);
    // SAFETY: `class` is a valid NUL-terminated UTF-16 string; a null title matches any.
    let h = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    !h.is_null()
}

/// Creates the window on the current thread, with `handler` stored in its user data.
fn create_window(handler: Handler) -> Result<HWND, std::io::Error> {
    let class = wide(&crate::window_class());
    let title = wide("UltraRushFTP");
    // SAFETY: a null module name returns the handle of the current executable.
    let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: Some(wndproc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: std::ptr::null_mut(),
        hCursor: std::ptr::null_mut(),
        hbrBackground: std::ptr::null_mut(),
        lpszMenuName: std::ptr::null(),
        lpszClassName: class.as_ptr(),
        hIconSm: std::ptr::null_mut(),
    };
    // SAFETY: `wc` is fully initialised and `class` outlives the call. Registering a class
    // that already exists fails harmlessly.
    unsafe { RegisterClassExW(&wc) };
    let state = Box::into_raw(Box::new(handler));
    // Top-level window that is never shown: `FindWindow` ignores message-only windows.
    // SAFETY: class and title are valid NUL-terminated strings; `state` is handed to
    // `WM_NCCREATE` through `lpCreateParams` and reclaimed in `WM_DESTROY`.
    let hwnd = unsafe {
        CreateWindowExW(
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
        )
    };
    if hwnd.is_null() {
        let err = std::io::Error::last_os_error();
        // SAFETY: creation failed, so `wndproc` never stored `state`; we still own it.
        drop(unsafe { Box::from_raw(state) });
        return Err(err);
    }
    Ok(hwnd)
}

fn message_loop() {
    // SAFETY: `MSG` is a plain C struct for which all-zero is a valid value.
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    // SAFETY: standard message loop on the thread owning the window; `msg` is valid.
    while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
        // SAFETY: `msg` was filled by `GetMessageW`.
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

impl Server {
    /// Starts the `TfmRush` window on its own thread. `on_request` is called from that
    /// thread: it should only forward the request (channel) and wake the UI up.
    pub fn start(on_request: impl Fn(Request) + Send + 'static) -> Result<Self, ServerError> {
        let (tx, rx) = sync_channel::<Result<isize, std::io::Error>>(1);
        let handler: Handler = Box::new(on_request);
        let thread = std::thread::Builder::new()
            .name("urftp-ipc".into())
            .spawn(move || match create_window(handler) {
                Ok(hwnd) => {
                    let _ = tx.send(Ok(hwnd as isize));
                    message_loop();
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                }
            })
            .map_err(ServerError::Thread)?;
        match rx.recv() {
            Ok(Ok(hwnd)) => Ok(Self { hwnd, thread: Some(thread) }),
            Ok(Err(e)) => Err(ServerError::Window(e)),
            Err(_) => Err(ServerError::ThreadStopped),
        }
    }

    /// Window handle (for tests).
    pub fn hwnd(&self) -> isize {
        self.hwnd
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // SAFETY: posting to a window owned by another thread is allowed; `WM_CLOSE`
        // destroys it there and ends its message loop.
        unsafe { PostMessageW(self.hwnd as HWND, WM_CLOSE, 0, 0) };
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Handler stored in the window user data.
///
/// # Safety
/// `hwnd` must be a window created by [`create_window`] on the calling thread, between
/// `WM_NCCREATE` and `WM_DESTROY`.
unsafe fn handler<'a>(hwnd: HWND) -> Option<&'a Handler> {
    // SAFETY: per the contract, the user data is null or the `Box<Handler>` pointer set in
    // `WM_NCCREATE`, valid until `WM_DESTROY` clears it.
    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Handler).as_ref() }
}

/// Bytes of a `COPYDATASTRUCT` received in `WM_COPYDATA`.
///
/// # Safety
/// `lparam` must be the `lParam` of a `WM_COPYDATA` message being processed.
unsafe fn copydata<'a>(lparam: LPARAM) -> Option<(usize, &'a [u8])> {
    // SAFETY: for `WM_COPYDATA` the system guarantees `lParam` points to a valid
    // `COPYDATASTRUCT` for the duration of the message.
    let cds = unsafe { (lparam as *const COPYDATASTRUCT).as_ref() }?;
    if cds.lpData.is_null() {
        return None;
    }
    // SAFETY: the system copied `cbData` bytes at `lpData` into our process for the
    // duration of the message.
    let bytes = unsafe { std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize) };
    Some((cds.dwData, bytes))
}

fn answer_invoke(h: &Handler) -> LRESULT {
    let Ok(mem) = SharedMem::open(SHARED_MAP_NAME, SHARED_MAP_SIZE) else { return 0 };
    let expression = mem.read_string();
    let (reply_tx, reply_rx) = sync_channel(1);
    h(Request::Invoke { expression, reply: reply_tx });
    let answer = reply_rx.recv_timeout(INVOKE_TIMEOUT).unwrap_or_default();
    mem.write_string(&answer);
    1
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            // SAFETY: for `WM_NCCREATE`, `lParam` points to the `CREATESTRUCTW` of the
            // window being created.
            if let Some(cs) = unsafe { (lparam as *const CREATESTRUCTW).as_ref() } {
                // SAFETY: `hwnd` is the window being created, owned by this thread.
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as _) };
            }
            // SAFETY: default processing with the received arguments.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_EXCHANGE => {
            // SAFETY: `hwnd` is our window, between WM_NCCREATE and WM_DESTROY.
            match unsafe { handler(hwnd) } {
                Some(h) => answer_invoke(h),
                None => 0,
            }
        }
        WM_COPYDATA => {
            // SAFETY: as above.
            let Some(h) = (unsafe { handler(hwnd) }) else { return 0 };
            // SAFETY: we are processing `WM_COPYDATA`.
            let Some((kind, bytes)) = (unsafe { copydata(lparam) }) else { return 0 };
            let source = decode_ansi(bytes);
            match kind {
                COPYDATA_SCRIPT => h(Request::Script { source, callback: (wparam != 0).then_some(wparam as isize) }),
                COPYDATA_COMMAND => h(Request::Command { source }),
                _ => return 0,
            }
            1
        }
        WM_CLOSE => {
            // SAFETY: `hwnd` belongs to this thread.
            unsafe { DestroyWindow(hwnd) };
            0
        }
        WM_DESTROY => {
            // SAFETY: `hwnd` is our window; clearing the user data hands the handler back.
            let p = unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) } as *mut Handler;
            if !p.is_null() {
                // SAFETY: `p` came from `Box::into_raw` in `create_window` and is freed once.
                drop(unsafe { Box::from_raw(p) });
            }
            // SAFETY: ends the message loop of the current thread.
            unsafe { PostQuitMessage(0) };
            0
        }
        // SAFETY: default processing with the received arguments.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Raw-command reply as sent to the DLL: "site", "command", then the reply lines.
pub fn raw_callback_text(site: &str, command: &str, lines: &[String]) -> String {
    let mut text = format!("{site}\r\n{command}\r\n");
    for l in lines {
        text.push_str(l);
        text.push_str("\r\n");
    }
    text
}

/// Sends the reply of a raw command back to the DLL (format of the original). Returns
/// `false` when the window is gone or did not answer in time.
pub fn send_raw_callback(callback: isize, ok: bool, site: &str, command: &str, lines: &[String]) -> bool {
    let mut bytes = raw_callback_text(site, command, lines).into_bytes();
    bytes.push(0);
    let Ok(len) = u32::try_from(bytes.len()) else { return false };
    let cds =
        COPYDATASTRUCT { dwData: if ok { CALLBACK_SUCCESS } else { CALLBACK_FAILED }, cbData: len, lpData: bytes.as_mut_ptr().cast() };
    let mut result = 0usize;
    // SAFETY: `cds` and `bytes` outlive this synchronous call; an invalid `callback`
    // handle only makes the call fail.
    let sent = unsafe {
        SendMessageTimeoutW(
            callback as HWND,
            WM_COPYDATA,
            CALLBACK_RAWCOMMAND,
            &cds as *const _ as LPARAM,
            SMTO_ABORTIFHUNG,
            CALLBACK_TIMEOUT_MS,
            &mut result,
        )
    };
    sent != 0
}

#[cfg(test)]
mod tests {
    #[test]
    fn callback_text_format() {
        let t = super::raw_callback_text("DEV", "SITE WHO", &["200- a".into(), "200 End".into()]);
        assert_eq!(t, "DEV\r\nSITE WHO\r\n200- a\r\n200 End\r\n");
    }
}
