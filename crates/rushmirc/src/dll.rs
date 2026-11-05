//! Implémentation Windows de la DLL (même protocole que l'original Delphi).

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use urftp_ipc::win::{SharedMem, decode_ansi, wide};
use urftp_ipc::{
    CALLBACK_RAWCOMMAND, CALLBACK_SUCCESS, COPYDATA_COMMAND, COPYDATA_SCRIPT, MIRC_MAP_NAME, SHARED_MAP_NAME, SHARED_MAP_SIZE,
    WM_EXCHANGE, WM_MCOMMAND,
};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, FindWindowW, HWND_MESSAGE, RegisterClassExW, SMTO_ABORTIFHUNG, SendMessageTimeoutW,
    WM_COPYDATA, WNDCLASSEXW,
};

/// `BOOL` Win32.
pub type Bool = i32;

/// Structure passée par mIRC à `LoadDll`. Les champs après `m_keep` n'existent qu'à partir
/// de mIRC 7 ; ils ne sont lus que si la version l'indique.
#[repr(C)]
pub struct LoadInfo {
    pub m_version: u32,
    pub m_hwnd: HWND,
    pub m_keep: Bool,
    pub m_unicode: Bool,
    pub m_beta: u32,
    pub m_bytes: u32,
}

/// Taille minimale garantie du tampon `data` de mIRC (versions anciennes).
const DEFAULT_BYTES: usize = 900;
const CALLBACK_CLASS: &str = "UltraRushFTP.RushMirc.Callback";
const TIMEOUT_MS: u32 = 15_000;

struct State {
    /// Mémoire `FTPRush` (requêtes `Invoke`).
    exchange: Arc<SharedMem>,
    /// Mémoire `mIRC` (commandes renvoyées à mIRC).
    mirc: Arc<SharedMem>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);
static CALLBACK_HWND: AtomicIsize = AtomicIsize::new(0);
static IRC_HWND: AtomicIsize = AtomicIsize::new(0);
static CALLBACK_ENABLED: AtomicBool = AtomicBool::new(true);
static BUFFER_BYTES: AtomicUsize = AtomicUsize::new(DEFAULT_BYTES);

/// Initialise les mémoires partagées et la fenêtre de rappel (sur le fil appelant, celui
/// de mIRC, dont la boucle de messages servira la fenêtre).
fn ensure_init() -> Option<(Arc<SharedMem>, Arc<SharedMem>)> {
    let mut g = STATE.lock().ok()?;
    if g.is_none() {
        let exchange = Arc::new(SharedMem::create(SHARED_MAP_NAME, SHARED_MAP_SIZE)?);
        let mirc = Arc::new(SharedMem::create(MIRC_MAP_NAME, SHARED_MAP_SIZE)?);
        *g = Some(State { exchange, mirc });
        drop(g);
        create_callback_window();
        g = STATE.lock().ok()?;
    }
    g.as_ref().map(|s| (s.exchange.clone(), s.mirc.clone()))
}

fn create_callback_window() {
    // SAFETY : enregistrement de classe et création d'une fenêtre « message-only ».
    unsafe {
        let class = wide(CALLBACK_CLASS);
        let hinst = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(callback_proc),
            hInstance: hinst,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            hinst,
            std::ptr::null(),
        );
        CALLBACK_HWND.store(hwnd as isize, Ordering::SeqCst);
    }
}

/// Fenêtre de rappel de la DLL (pour les tests et l'application).
pub fn callback_hwnd() -> isize {
    CALLBACK_HWND.load(Ordering::SeqCst)
}

fn target_window() -> Option<HWND> {
    let class = wide(&urftp_ipc::window_class());
    // SAFETY : chaîne UTF-16 valide.
    let h = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    (!h.is_null()).then_some(h)
}

/// Copie `s` dans le tampon de mIRC (tronqué, terminé par zéro).
unsafe fn write_data(data: *mut u8, s: &str) {
    let max = BUFFER_BYTES.load(Ordering::SeqCst).max(2);
    let bytes = s.as_bytes();
    let n = bytes.len().min(max - 1);
    // SAFETY : mIRC fournit un tampon d'au moins `max` octets.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, n);
        *data.add(n) = 0;
    }
}

unsafe fn read_data(data: *const u8) -> String {
    if data.is_null() {
        return String::new();
    }
    let max = BUFFER_BYTES.load(Ordering::SeqCst);
    // SAFETY : tampon de mIRC terminé par zéro, d'au plus `max` octets.
    let bytes = unsafe { std::slice::from_raw_parts(data, max) };
    decode_ansi(bytes)
}

fn send_copydata(target: HWND, kind: usize, wparam: WPARAM, text: &str) {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    let cds = COPYDATASTRUCT { dwData: kind, cbData: bytes.len() as u32, lpData: bytes.as_mut_ptr().cast() };
    let mut r = 0usize;
    // SAFETY : `cds` et `bytes` vivent pendant l'appel synchrone.
    unsafe { SendMessageTimeoutW(target, WM_COPYDATA, wparam, &cds as *const _ as LPARAM, SMTO_ABORTIFHUNG, TIMEOUT_MS, &mut r) };
}

// ====================================================================== exports mIRC

/// Appelée par mIRC au chargement : on demande à rester chargé.
///
/// # Safety
/// `load` doit pointer sur la structure `LOADINFO` fournie par mIRC.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LoadDll(load: *mut LoadInfo) {
    // SAFETY : pointeur fourni par mIRC.
    if let Some(info) = unsafe { load.as_mut() } {
        info.m_keep = 1;
        let (major, minor) = (info.m_version & 0xFFFF, info.m_version >> 16);
        // mBytes (taille du tampon) n'existe que dans les mIRC récents.
        if (major, minor) >= (7, 53) && (DEFAULT_BYTES as u32..=1 << 20).contains(&info.m_bytes) {
            BUFFER_BYTES.store(info.m_bytes as usize, Ordering::SeqCst);
        }
    }
    ensure_init();
}

/// Appelée par mIRC au déchargement. `timeout = 1` : déchargement pour inactivité, refusé.
///
/// # Safety
/// Appelée par mIRC sur son fil principal.
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
        // SAFETY : fenêtre créée sur ce même fil.
        unsafe { DestroyWindow(h as HWND) };
    }
    1
}

/// `$dll(rushmirc.dll, Invoke, RushApp.TotalDownloadSize)` : renvoie la valeur demandée.
///
/// # Safety
/// `data` est le tampon de mIRC (lecture de la requête, écriture de la réponse).
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Invoke(_mwnd: HWND, _awnd: HWND, data: *mut u8, _parms: *mut u8, _show: Bool, _nopause: Bool) -> i32 {
    let Some(target) = target_window() else { return 1 };
    let Some((exchange, _)) = ensure_init() else { return 1 };
    // SAFETY : tampon de mIRC.
    let request = unsafe { read_data(data) };
    exchange.write_string(&request);
    let mut r = 0usize;
    // Le verrou n'est pas tenu pendant l'envoi : un rappel peut arriver entre-temps.
    // SAFETY : message sans pointeur.
    unsafe { SendMessageTimeoutW(target, WM_EXCHANGE, 0, 0, SMTO_ABORTIFHUNG, TIMEOUT_MS, &mut r) };
    let answer = exchange.read_string();
    // SAFETY : tampon de mIRC.
    unsafe { write_data(data, &answer) };
    3
}

/// `/dll rushmirc.dll RushScript <Pascal Script>` : exécute un script dans UltraRushFTP.
///
/// # Safety
/// `data` est le tampon de mIRC.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn RushScript(mwnd: HWND, _awnd: HWND, data: *mut u8, _parms: *mut u8, _show: Bool, _nopause: Bool) -> i32 {
    let Some(target) = target_window() else { return 1 };
    ensure_init();
    IRC_HWND.store(mwnd as isize, Ordering::SeqCst);
    let wparam = if CALLBACK_ENABLED.load(Ordering::SeqCst) { callback_hwnd() as WPARAM } else { 0 };
    // SAFETY : tampon de mIRC.
    let script = unsafe { read_data(data) };
    send_copydata(target, COPYDATA_SCRIPT, wparam, &script);
    1
}

/// `/dll rushmirc.dll RushCommand <commande>`.
///
/// # Safety
/// `data` est le tampon de mIRC.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn RushCommand(_mwnd: HWND, _awnd: HWND, data: *mut u8, _parms: *mut u8, _show: Bool, _nopause: Bool) -> i32 {
    let Some(target) = target_window() else { return 1 };
    // SAFETY : tampon de mIRC.
    let cmd = unsafe { read_data(data) };
    send_copydata(target, COPYDATA_COMMAND, 0, &cmd);
    1
}

/// Active le renvoi des réponses `RAW` dans mIRC (par défaut).
///
/// # Safety
/// Signature mIRC ; aucun pointeur n'est utilisé.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn EnableCallback(_m: HWND, _a: HWND, _d: *mut u8, _p: *mut u8, _s: Bool, _n: Bool) -> i32 {
    CALLBACK_ENABLED.store(true, Ordering::SeqCst);
    1
}

/// Désactive le renvoi des réponses `RAW` dans mIRC.
///
/// # Safety
/// Signature mIRC ; aucun pointeur n'est utilisé.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DisableCallback(_m: HWND, _a: HWND, _d: *mut u8, _p: *mut u8, _s: Bool, _n: Bool) -> i32 {
    CALLBACK_ENABLED.store(false, Ordering::SeqCst);
    1
}

// ====================================================================== rappels vers mIRC

/// Fait exécuter une commande par mIRC (interface `WM_MCOMMAND` + mémoire `mIRC`).
fn mirc_command(cmd: &str) {
    let irc = IRC_HWND.load(Ordering::SeqCst);
    let Some((_, mirc)) = ensure_init() else { return };
    if irc == 0 {
        return;
    }
    mirc.write_string(cmd);
    let mut r = 0usize;
    // SAFETY : interface documentée de mIRC (wParam 1 = commande, lParam 0 = mémoire « mIRC »).
    unsafe { SendMessageTimeoutW(irc as HWND, WM_MCOMMAND, 1, 0, SMTO_ABORTIFHUNG, TIMEOUT_MS, &mut r) };
}

/// Lignes d'un rappel `RAW` à afficher dans mIRC (format de l'original).
pub fn echo_lines(ok: bool, text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(site) = lines.first() else { return Vec::new() };
    let command = lines.get(1).copied().unwrap_or_default();
    let mut out = vec![if ok { format!("/echo -a ({site}) \u{2}{command}") } else { format!("/echo -a ({site}) \u{2}{command}\u{2} : échec") }];
    out.extend(lines.iter().skip(2).map(|l| format!("/echo -a ({site}) {l}")));
    out
}

unsafe extern "system" fn callback_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_COPYDATA && wparam == CALLBACK_RAWCOMMAND {
        // SAFETY : COPYDATASTRUCT fourni par l'expéditeur pendant l'appel synchrone.
        let cds = unsafe { &*(lparam as *const COPYDATASTRUCT) };
        if !cds.lpData.is_null() {
            let bytes = unsafe { std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize) };
            for line in echo_lines(cds.dwData == CALLBACK_SUCCESS, &decode_ansi(bytes)) {
                mirc_command(&line);
            }
        }
        return 1;
    }
    // SAFETY : traitement par défaut.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn echo_format_matches_original() {
        let l = super::echo_lines(true, "DEV\r\nSITE WHO\r\n200- user1\r\n200 End\r\n");
        assert_eq!(l, ["/echo -a (DEV) \u{2}SITE WHO", "/echo -a (DEV) 200- user1", "/echo -a (DEV) 200 End"]);
        assert!(super::echo_lines(false, "DEV\nNOOP\n")[0].contains("échec"));
        assert!(super::echo_lines(true, "").is_empty());
    }
}
