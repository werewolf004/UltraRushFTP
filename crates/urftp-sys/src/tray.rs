//! Notification area icon (tray), its pop-up messages, the flashing of the main window
//! and the event sounds.
//!
//! The icon lives on a hidden message window of its own thread; a click on the icon
//! shows the main window again directly (it may be hidden, and then draws nothing) and is
//! reported to the application.

use std::sync::mpsc::sync_channel;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetMessageW, GetSystemMetrics,
    GetWindowLongPtrW, HICON, HWND_MESSAGE, IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTCOLOR, LR_SHARED, LoadIconW, LoadImageW, MSG,
    PostMessageW, PostQuitMessage, RegisterClassExW, SM_CXSMICON, SM_CYSMICON, SW_RESTORE, SW_SHOW, SetForegroundWindow, SetWindowLongPtrW,
    ShowWindow, TranslateMessage, WM_APP, WM_CLOSE, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NCCREATE, WM_RBUTTONUP, WNDCLASSEXW,
};

use crate::win::wide;

/// Icon of the program (resource 1 of the executable) at the small-icon size of the
/// notification area; the stock application icon when the executable has none (tests).
fn app_icon(hinst: windows_sys::Win32::Foundation::HINSTANCE) -> HICON {
    // SAFETY: plain metric queries.
    let (cx, cy) = unsafe { (GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON)) };
    // SAFETY: `hinst` is the module of this process; resource 1 is an integer resource
    // name (MAKEINTRESOURCE), never dereferenced as a string. A shared icon is not owned.
    let own = unsafe { LoadImageW(hinst, 1 as windows_sys::core::PCWSTR, IMAGE_ICON, cx, cy, LR_DEFAULTCOLOR | LR_SHARED) };
    if !own.is_null() {
        return own;
    }
    // SAFETY: a null instance with `IDI_APPLICATION` loads the stock application icon.
    unsafe { LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) }
}

/// Message the icon sends to its window.
const WM_TRAY: u32 = WM_APP + 41;
/// Identifier of the icon.
const ICON_ID: u32 = 1;

/// Errors of the tray icon.
#[derive(Debug, thiserror::Error)]
pub enum TrayError {
    /// The thread of the icon could not be started.
    #[error("tray icon thread: {0}")]
    Thread(#[source] std::io::Error),
    /// The window or the icon could not be created.
    #[error("tray icon: {0}")]
    Os(#[source] std::io::Error),
    /// The thread stopped before reporting.
    #[error("tray icon: the thread stopped unexpectedly")]
    ThreadStopped,
}

/// What the user did on the icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// Click or double-click: the main window was shown again.
    Activate,
}

struct State {
    on_event: Box<dyn Fn(TrayEvent) + Send>,
    /// Main window to show again.
    main: isize,
}

/// Running icon; removed when dropped.
pub struct Tray {
    hwnd: isize,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// `text` as a fixed UTF-16 field (cut to fit, NUL-terminated). Returned by value: the
/// fields of `NOTIFYICONDATAW` may not be borrowed (the structure is packed on 32 bits).
fn field<const N: usize>(text: &str) -> [u16; N] {
    let mut out = [0u16; N];
    for (slot, c) in out.iter_mut().zip(text.encode_utf16().take(N - 1)) {
        *slot = c;
    }
    out
}

fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    // SAFETY: `NOTIFYICONDATAW` is a plain C struct for which all-zero is a valid value.
    let mut d: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    d.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    d.hWnd = hwnd;
    d.uID = ICON_ID;
    d
}

impl Tray {
    /// Shows the icon with `tip`; `main` is the main window (shown again on a click),
    /// `on_event` is called from the icon thread.
    pub fn start(tip: &str, main: isize, on_event: impl Fn(TrayEvent) + Send + 'static) -> Result<Self, TrayError> {
        let (tx, rx) = sync_channel::<Result<isize, std::io::Error>>(1);
        let state = State { on_event: Box::new(on_event), main };
        let tip = tip.to_string();
        let thread = std::thread::Builder::new()
            .name("urftp-tray".into())
            .spawn(move || match create(state, &tip) {
                Ok(hwnd) => {
                    let _ = tx.send(Ok(hwnd as isize));
                    message_loop();
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                }
            })
            .map_err(TrayError::Thread)?;
        match rx.recv() {
            Ok(Ok(hwnd)) => Ok(Self { hwnd, thread: Some(thread) }),
            Ok(Err(e)) => Err(TrayError::Os(e)),
            Err(_) => Err(TrayError::ThreadStopped),
        }
    }

    /// Pop-up message above the icon.
    pub fn notify(&self, title: &str, text: &str) {
        let mut d = icon_data(self.hwnd as HWND);
        d.uFlags = NIF_INFO;
        d.dwInfoFlags = NIIF_INFO;
        d.szInfoTitle = field(title);
        d.szInfo = field(text);
        // SAFETY: `d` is fully initialised and names the icon of our window.
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) };
    }

    /// Text shown when the pointer is over the icon.
    pub fn set_tip(&self, tip: &str) {
        let mut d = icon_data(self.hwnd as HWND);
        d.uFlags = NIF_TIP;
        d.szTip = field(tip);
        // SAFETY: as in `notify`.
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) };
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        // SAFETY: posting to a window of another thread is allowed; `WM_CLOSE` removes the
        // icon and destroys the window there, which ends its message loop.
        unsafe { PostMessageW(self.hwnd as HWND, WM_CLOSE, 0, 0) };
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn create(state: State, tip: &str) -> Result<HWND, std::io::Error> {
    let class = wide("UltraRushFTPTray");
    // SAFETY: a null module name returns the handle of the current executable.
    let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
    // SAFETY: `WNDCLASSEXW` is a plain C struct; the fields used are set below.
    let mut wc: WNDCLASSEXW = unsafe { std::mem::zeroed() };
    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    wc.lpfnWndProc = Some(wndproc);
    wc.hInstance = hinst;
    wc.lpszClassName = class.as_ptr();
    // SAFETY: `wc` is initialised and `class` outlives the call; registering a class that
    // already exists fails harmlessly.
    unsafe { RegisterClassExW(&wc) };
    let state = Box::into_raw(Box::new(state));
    // SAFETY: `class` is a valid string; `state` is handed to `WM_NCCREATE` and reclaimed
    // in `WM_DESTROY`. `HWND_MESSAGE`: a message-only window.
    let hwnd = unsafe {
        CreateWindowExW(0, class.as_ptr(), class.as_ptr(), 0, 0, 0, 0, 0, HWND_MESSAGE, std::ptr::null_mut(), hinst, state.cast())
    };
    if hwnd.is_null() {
        let err = std::io::Error::last_os_error();
        // SAFETY: creation failed, so `wndproc` never stored `state`; we still own it.
        drop(unsafe { Box::from_raw(state) });
        return Err(err);
    }
    let mut d = icon_data(hwnd);
    d.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    d.uCallbackMessage = WM_TRAY;
    d.hIcon = app_icon(hinst);
    d.szTip = field(tip);
    // SAFETY: `d` is fully initialised for our window.
    if unsafe { Shell_NotifyIconW(NIM_ADD, &d) } == 0 {
        let err = std::io::Error::last_os_error();
        // SAFETY: our window, on this thread; `WM_DESTROY` frees the state.
        unsafe { DestroyWindow(hwnd) };
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

/// Shows a window again and brings it to the front.
pub fn show_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    // SAFETY: plain calls on a window handle; an invalid one makes them fail harmlessly.
    unsafe {
        ShowWindow(hwnd as HWND, SW_SHOW);
        ShowWindow(hwnd as HWND, SW_RESTORE);
        SetForegroundWindow(hwnd as HWND);
    }
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
            // SAFETY: default processing of the message.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_TRAY => {
            let what = (lparam & 0xFFFF) as u32;
            if matches!(what, WM_LBUTTONUP | WM_LBUTTONDBLCLK | WM_RBUTTONUP) {
                // SAFETY: the user data is the `State` set in `WM_NCCREATE`, valid until
                // `WM_DESTROY` clears it, and only used on this thread.
                if let Some(s) = unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const State).as_ref() } {
                    show_window(s.main);
                    (s.on_event)(TrayEvent::Activate);
                }
            }
            0
        }
        WM_CLOSE => {
            let d = icon_data(hwnd);
            // SAFETY: removes our icon, then destroys our window on its own thread.
            unsafe {
                Shell_NotifyIconW(NIM_DELETE, &d);
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            // SAFETY: the user data is the `Box<State>` pointer set in `WM_NCCREATE`; it is
            // cleared before being freed, so it is freed once.
            unsafe {
                let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                if !p.is_null() {
                    drop(Box::from_raw(p));
                }
                PostQuitMessage(0);
            }
            0
        }
        // SAFETY: default processing of the other messages.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Window of this thread that has the keyboard (the main window, called from the
/// interface thread while it is focused).
pub fn active_window() -> isize {
    // SAFETY: plain query.
    unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow() as isize }
}

/// Makes the taskbar button of a window flash until it gets the keyboard.
pub fn flash(hwnd: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FLASHW_TIMERNOFG, FLASHW_TRAY, FLASHWINFO, FlashWindowEx};
    if hwnd == 0 {
        return;
    }
    let info = FLASHWINFO {
        cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
        hwnd: hwnd as HWND,
        dwFlags: FLASHW_TRAY | FLASHW_TIMERNOFG,
        uCount: 0,
        dwTimeout: 0,
    };
    // SAFETY: `info` is fully initialised.
    unsafe { FlashWindowEx(&info) };
}

/// Stops the flashing of the taskbar button of a window.
pub fn flash_stop(hwnd: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FLASHW_STOP, FLASHWINFO, FlashWindowEx};
    if hwnd == 0 {
        return;
    }
    let info =
        FLASHWINFO { cbSize: std::mem::size_of::<FLASHWINFO>() as u32, hwnd: hwnd as HWND, dwFlags: FLASHW_STOP, uCount: 0, dwTimeout: 0 };
    // SAFETY: `info` is fully initialised.
    unsafe { FlashWindowEx(&info) };
}

/// Plays a sound: a `.wav` file, or else the Windows "notification" sound.
pub fn play_sound(file: Option<&std::path::Path>) {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};
    match file {
        Some(f) => {
            let w = wide(&f.to_string_lossy());
            // SAFETY: `w` is a NUL-terminated path; asynchronous play copies what it needs.
            unsafe { PlaySoundW(w.as_ptr(), std::ptr::null_mut(), SND_FILENAME | SND_ASYNC | SND_NODEFAULT) };
        }
        None => {
            let w = wide("SystemAsterisk");
            // SAFETY: `w` is a NUL-terminated system sound alias.
            unsafe { PlaySoundW(w.as_ptr(), std::ptr::null_mut(), SND_ALIAS | SND_ASYNC) };
        }
    }
}
