//! `rushmirc.dll` exports (same protocol as the original Delphi DLL). Compiled only with
//! the `mirc-dll` feature; the `rushmirc` crate re-exports them into the cdylib.
//!
//! Exports (standard mIRC signature, `stdcall`): `LoadDll`, `UnloadDll`, `Invoke`,
//! `RushScript`, `RushCommand`, `EnableCallback`, `DisableCallback`.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, FindWindowW, HWND_MESSAGE, RegisterClassExW, SMTO_ABORTIFHUNG, SendMessageTimeoutW,
    WM_COPYDATA, WNDCLASSEXW,
};

use crate::win::{SharedMem, decode_ansi, wide};
use crate::{
    CALLBACK_RAWCOMMAND, CALLBACK_SUCCESS, COPYDATA_COMMAND, COPYDATA_SCRIPT, MIRC_MAP_NAME, SHARED_MAP_NAME, SHARED_MAP_SIZE, WM_EXCHANGE,
    WM_MCOMMAND,
};

/// Win32 `BOOL`.
pub type Bool = i32;

/// Structure passed by mIRC to `LoadDll`. Fields after `m_keep` only exist since mIRC 7;
/// they are read only when the version says so.
#[repr(C)]
pub struct LoadInfo {
    /// mIRC version (low word major, high word minor).
    pub m_version: u32,
    /// Main mIRC window.
    pub m_hwnd: HWND,
    /// Set to 1 to stay loaded.
    pub m_keep: Bool,
    /// Unicode strings (mIRC 7+).
    pub m_unicode: Bool,
    /// Beta number.
    pub m_beta: u32,
    /// Size of the `data` buffer (mIRC 7.53+).
    pub m_bytes: u32,
}

/// Minimum guaranteed size of the mIRC `data` buffer (old versions).
const DEFAULT_BYTES: usize = 900;
const CALLBACK_CLASS: &str = "UltraRushFTP.RushMirc.Callback";
const TIMEOUT_MS: u32 = 15_000;
/// mIRC return code: continue processing (`/dll` command).
const RET_CONTINUE: i32 = 1;
/// mIRC return code: `data` holds the result (`$dll(...)`).
const RET_RESULT: i32 = 3;

struct State {
    /// `FTPRush` memory (`Invoke` requests).
    exchange: Arc<SharedMem>,
    /// `mIRC` memory (commands sent back to mIRC).
    mirc: Arc<SharedMem>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);
static CALLBACK_HWND: AtomicIsize = AtomicIsize::new(0);
static IRC_HWND: AtomicIsize = AtomicIsize::new(0);
static CALLBACK_ENABLED: AtomicBool = AtomicBool::new(true);
static BUFFER_BYTES: AtomicUsize = AtomicUsize::new(DEFAULT_BYTES);

/// Creates the shared memories and the callback window (on the calling thread, mIRC's,
/// whose message loop serves the window).
fn ensure_init() -> Option<(Arc<SharedMem>, Arc<SharedMem>)> {
    let mut g = STATE.lock().ok()?;
    if g.is_none() {
        let exchange = Arc::new(SharedMem::create(SHARED_MAP_NAME, SHARED_MAP_SIZE).ok()?);
        let mirc = Arc::new(SharedMem::create(MIRC_MAP_NAME, SHARED_MAP_SIZE).ok()?);
        *g = Some(State { exchange, mirc });
        drop(g);
        create_callback_window();
        g = STATE.lock().ok()?;
    }
    g.as_ref().map(|s| (s.exchange.clone(), s.mirc.clone()))
}

fn create_callback_window() {
    let class = wide(CALLBACK_CLASS);
    // SAFETY: a null module name returns the handle of the host executable.
    let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: Some(callback_proc),
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
    // SAFETY: `wc` is fully initialised and `class` outlives the call.
    unsafe { RegisterClassExW(&wc) };
    // SAFETY: message-only window with a registered class; no creation parameter.
    let hwnd = unsafe {
        CreateWindowExW(0, class.as_ptr(), class.as_ptr(), 0, 0, 0, 0, 0, HWND_MESSAGE, std::ptr::null_mut(), hinst, std::ptr::null())
    };
    CALLBACK_HWND.store(hwnd as isize, Ordering::SeqCst);
}

/// Callback window of the DLL (tests and application).
pub fn callback_hwnd() -> isize {
    CALLBACK_HWND.load(Ordering::SeqCst)
}

fn target_window() -> Option<HWND> {
    crate::dll_target_classes().iter().find_map(|c| {
        let class = wide(c);
        // SAFETY: `class` is a valid NUL-terminated UTF-16 string; a null title matches any.
        let h = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
        (!h.is_null()).then_some(h)
    })
}

/// Copies `s` into the mIRC buffer (truncated, NUL-terminated).
///
/// # Safety
/// `data` must be null or point to a writable buffer of at least `BUFFER_BYTES` bytes.
unsafe fn write_data(data: *mut u8, s: &str) {
    if data.is_null() {
        return;
    }
    let max = BUFFER_BYTES.load(Ordering::SeqCst).max(2);
    let bytes = s.as_bytes();
    let n = bytes.len().min(max - 1);
    // SAFETY: per the contract the buffer holds `max >= n + 1` bytes; `s` does not overlap it.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, n);
        *data.add(n) = 0;
    }
}

/// Reads the NUL-terminated request from the mIRC buffer.
///
/// # Safety
/// `data` must be null or point to a readable buffer of at least `BUFFER_BYTES` bytes.
unsafe fn read_data(data: *const u8) -> String {
    if data.is_null() {
        return String::new();
    }
    let max = BUFFER_BYTES.load(Ordering::SeqCst);
    // SAFETY: per the contract the buffer holds `max` readable bytes.
    let bytes = unsafe { std::slice::from_raw_parts(data, max) };
    decode_ansi(bytes)
}

fn send_copydata(target: HWND, kind: usize, wparam: WPARAM, text: &str) {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    let Ok(len) = u32::try_from(bytes.len()) else { return };
    let cds = COPYDATASTRUCT { dwData: kind, cbData: len, lpData: bytes.as_mut_ptr().cast() };
    let mut r = 0usize;
    // SAFETY: `cds` and `bytes` outlive this synchronous call.
    unsafe { SendMessageTimeoutW(target, WM_COPYDATA, wparam, &cds as *const _ as LPARAM, SMTO_ABORTIFHUNG, TIMEOUT_MS, &mut r) };
}

// ====================================================================== mIRC exports

/// Called by mIRC on load: asks to stay loaded and reads the buffer size.
///
/// # Safety
/// `load` must be null or point to the `LOADINFO` structure provided by mIRC.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LoadDll(load: *mut LoadInfo) {
    // SAFETY: per the contract the pointer is null or valid for the call.
    if let Some(info) = unsafe { load.as_mut() } {
        info.m_keep = 1;
        let (major, minor) = (info.m_version & 0xFFFF, info.m_version >> 16);
        // `m_bytes` only exists in recent mIRC versions.
        if (major, minor) >= (7, 53) && (DEFAULT_BYTES as u32..=1 << 20).contains(&info.m_bytes) {
            BUFFER_BYTES.store(info.m_bytes as usize, Ordering::SeqCst);
        }
    }
    ensure_init();
}

/// Called by mIRC on unload. `timeout = 1`: unload for inactivity, refused.
///
/// # Safety
/// Must be called by mIRC on the thread that called `LoadDll`.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn UnloadDll(timeout: i32) -> i32 {
    if timeout == 1 {
        return 0;
    }
    if let Ok(mut g) = STATE.lock() {
        *g = None;
    }
    let h = CALLBACK_HWND.swap(0, Ordering::SeqCst);
    if h != 0 {
        // SAFETY: the window was created on this same thread by `ensure_init`.
        unsafe { DestroyWindow(h as HWND) };
    }
    1
}

/// `$dll(rushmirc.dll, Invoke, RushApp.TotalDownloadSize)`: returns the requested value.
///
/// # Safety
/// `data` must be the mIRC buffer (read for the request, written with the answer).
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Invoke(_mwnd: HWND, _awnd: HWND, data: *mut u8, _parms: *mut u8, _show: Bool, _nopause: Bool) -> i32 {
    let Some(target) = target_window() else { return RET_CONTINUE };
    let Some((exchange, _)) = ensure_init() else { return RET_CONTINUE };
    // SAFETY: `data` is the mIRC buffer.
    let request = unsafe { read_data(data) };
    exchange.write_string(&request);
    let mut r = 0usize;
    // The lock is not held while sending: a callback may arrive meanwhile.
    // SAFETY: message without pointer arguments; an invalid target only fails the call.
    unsafe { SendMessageTimeoutW(target, WM_EXCHANGE, 0, 0, SMTO_ABORTIFHUNG, TIMEOUT_MS, &mut r) };
    let answer = exchange.read_string();
    // SAFETY: `data` is the mIRC buffer.
    unsafe { write_data(data, &answer) };
    RET_RESULT
}

/// `/dll rushmirc.dll RushScript <Pascal Script>`: runs a script in UltraRushFTP.
///
/// # Safety
/// `data` must be the mIRC buffer.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn RushScript(mwnd: HWND, _awnd: HWND, data: *mut u8, _parms: *mut u8, _show: Bool, _nopause: Bool) -> i32 {
    let Some(target) = target_window() else { return RET_CONTINUE };
    ensure_init();
    IRC_HWND.store(mwnd as isize, Ordering::SeqCst);
    let wparam = if CALLBACK_ENABLED.load(Ordering::SeqCst) { callback_hwnd() as WPARAM } else { 0 };
    // SAFETY: `data` is the mIRC buffer.
    let script = unsafe { read_data(data) };
    send_copydata(target, COPYDATA_SCRIPT, wparam, &script);
    RET_CONTINUE
}

/// `/dll rushmirc.dll RushCommand <command>`.
///
/// # Safety
/// `data` must be the mIRC buffer.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn RushCommand(_mwnd: HWND, _awnd: HWND, data: *mut u8, _parms: *mut u8, _show: Bool, _nopause: Bool) -> i32 {
    let Some(target) = target_window() else { return RET_CONTINUE };
    // SAFETY: `data` is the mIRC buffer.
    let cmd = unsafe { read_data(data) };
    send_copydata(target, COPYDATA_COMMAND, 0, &cmd);
    RET_CONTINUE
}

/// Enables sending `RAW` replies back to mIRC (default).
///
/// # Safety
/// mIRC signature; no pointer is dereferenced.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn EnableCallback(_m: HWND, _a: HWND, _d: *mut u8, _p: *mut u8, _s: Bool, _n: Bool) -> i32 {
    CALLBACK_ENABLED.store(true, Ordering::SeqCst);
    RET_CONTINUE
}

/// Disables sending `RAW` replies back to mIRC.
///
/// # Safety
/// mIRC signature; no pointer is dereferenced.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DisableCallback(_m: HWND, _a: HWND, _d: *mut u8, _p: *mut u8, _s: Bool, _n: Bool) -> i32 {
    CALLBACK_ENABLED.store(false, Ordering::SeqCst);
    RET_CONTINUE
}

// ====================================================================== callbacks to mIRC

/// Makes mIRC run a command (`WM_MCOMMAND` + `mIRC` shared memory).
fn mirc_command(cmd: &str) {
    let irc = IRC_HWND.load(Ordering::SeqCst);
    let Some((_, mirc)) = ensure_init() else { return };
    if irc == 0 {
        return;
    }
    mirc.write_string(cmd);
    let mut r = 0usize;
    // SAFETY: documented mIRC interface (wParam 1 = command, lParam 0 = "mIRC" memory);
    // no pointer is passed.
    unsafe { SendMessageTimeoutW(irc as HWND, WM_MCOMMAND, 1, 0, SMTO_ABORTIFHUNG, TIMEOUT_MS, &mut r) };
}

/// Lines of a `RAW` callback to display in mIRC (format of the original).
pub fn echo_lines(ok: bool, text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(site) = lines.first() else { return Vec::new() };
    let command = lines.get(1).copied().unwrap_or_default();
    let mut out =
        vec![if ok { format!("/echo -a ({site}) \u{2}{command}") } else { format!("/echo -a ({site}) \u{2}{command}\u{2} : failed") }];
    out.extend(lines.iter().skip(2).map(|l| format!("/echo -a ({site}) {l}")));
    out
}

unsafe extern "system" fn callback_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_COPYDATA && wparam == CALLBACK_RAWCOMMAND {
        // SAFETY: for `WM_COPYDATA` the system guarantees `lParam` points to a valid
        // `COPYDATASTRUCT` for the duration of the message.
        if let Some(cds) = unsafe { (lparam as *const COPYDATASTRUCT).as_ref() }
            && !cds.lpData.is_null()
        {
            // SAFETY: `cbData` bytes at `lpData` were copied into our process for the
            // duration of the message.
            let bytes = unsafe { std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize) };
            for line in echo_lines(cds.dwData == CALLBACK_SUCCESS, &decode_ansi(bytes)) {
                mirc_command(&line);
            }
        }
        return 1;
    }
    // SAFETY: default processing with the received arguments.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn echo_format_matches_original() {
        let l = super::echo_lines(true, "DEV\r\nSITE WHO\r\n200- user1\r\n200 End\r\n");
        assert_eq!(l, ["/echo -a (DEV) \u{2}SITE WHO", "/echo -a (DEV) 200- user1", "/echo -a (DEV) 200 End"]);
        assert!(super::echo_lines(false, "DEV\nNOOP\n")[0].contains("failed"));
        assert!(super::echo_lines(true, "").is_empty());
    }
}
