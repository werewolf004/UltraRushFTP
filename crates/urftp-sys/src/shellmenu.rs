//! Windows Explorer context menu of local files ("Explorer menu" of FTPRush): the menu the
//! shell gives for these items (`IContextMenu`), shown at the mouse pointer, and the
//! command chosen run by the shell.
//!
//! The menu is owned by a hidden window of this module that forwards the drawing messages
//! of the shell's own sub-menus ("Send to", "Open with"…) to `IContextMenu3`.

use std::cell::RefCell;
use std::path::Path;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    CMF_NORMAL, CMIC_MASK_PTINVOKE, CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX, IContextMenu, IContextMenu3, ILFindLastID, ILFree,
    IShellFolder, SHBindToParent, SHParseDisplayName,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, GetCursorPos, GetForegroundWindow, HMENU, PostMessageW,
    RegisterClassW, SW_SHOWNORMAL, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx, WINDOW_EX_STYLE, WM_DRAWITEM,
    WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR, WM_NULL, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{HSTRING, Interface, PCSTR, PCWSTR, w};

/// Errors of [`explorer_menu`].
#[derive(Debug, thiserror::Error)]
pub enum ShellMenuError {
    /// No item given.
    #[error("no item selected")]
    Empty,
    /// The items are not in one folder.
    #[error("the items are not in the same folder")]
    Folders,
    /// A shell call failed.
    #[error("Windows shell: {0}")]
    Shell(String),
}

impl From<windows::core::Error> for ShellMenuError {
    fn from(e: windows::core::Error) -> Self {
        ShellMenuError::Shell(e.message())
    }
}

/// `CMIC_MASK_UNICODE` (same value as `SEE_MASK_UNICODE`): `lpVerbW` is read.
const CMIC_MASK_UNICODE: u32 = 0x0000_4000;

thread_local! {
    /// Menu shown by this thread, for the messages of its sub-menus.
    static CURRENT: RefCell<Option<IContextMenu3>> = const { RefCell::new(None) };
}

/// Shows the Explorer menu of `paths` (items of one folder) at the mouse pointer and runs
/// the command chosen. Answers whether a command ran (the folder may have changed). Must
/// be called from the interface thread; it returns when the menu closes.
pub fn explorer_menu(paths: &[&Path]) -> Result<bool, ShellMenuError> {
    let first = paths.first().ok_or(ShellMenuError::Empty)?;
    if paths.iter().any(|p| p.parent() != first.parent()) {
        return Err(ShellMenuError::Folders);
    }
    // SAFETY: COM is started for this thread (an apartment already started by the window
    // library is reused); every success is balanced by `CoUninitialize` in the guard.
    let started = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let _com = ComGuard(started);

    let mut pidls = Pidls(Vec::with_capacity(paths.len()));
    for p in paths {
        let name = HSTRING::from(p.as_os_str());
        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        // SAFETY: `name` lives for the call; `pidl` receives an item list owned by `pidls`.
        unsafe { SHParseDisplayName(&name, None, &mut pidl, 0, None) }?;
        pidls.0.push(pidl);
    }
    // SAFETY: the first item list is valid (just parsed); the parent folder is returned
    // as a COM object released when dropped.
    let folder: IShellFolder = unsafe { SHBindToParent(pidls.0[0], None) }?;
    // SAFETY: each item list is valid; the last part stays inside it (not freed apart).
    let children: Vec<*const ITEMIDLIST> = pidls.0.iter().map(|&p| unsafe { ILFindLastID(p) }.cast_const()).collect();

    // SAFETY: plain query of the window that has the keyboard (the application).
    let owner = unsafe { GetForegroundWindow() };
    let helper = Helper::create()?;
    // SAFETY: `children` are relative item lists of `folder`, valid while `pidls` lives.
    let menu: IContextMenu = unsafe { folder.GetUIObjectOf(helper.0, &children, None) }?;
    // SAFETY: a new empty menu, destroyed by the guard.
    let popup = MenuGuard(unsafe { CreatePopupMenu() }?);
    // SAFETY: the shell fills our menu with command ids 1 to 0x7FFF.
    unsafe { menu.QueryContextMenu(popup.0, 0, 1, 0x7FFF, CMF_NORMAL) }.ok()?;

    let mut pt = POINT::default();
    // SAFETY: `pt` is written by the call.
    unsafe { GetCursorPos(&mut pt) }?;
    CURRENT.with(|c| *c.borrow_mut() = menu.cast::<IContextMenu3>().ok());
    // SAFETY: our hidden window takes the foreground so that the menu closes when the user
    // clicks elsewhere; the command id is returned instead of being posted.
    let chosen = unsafe {
        let _ = SetForegroundWindow(helper.0);
        let cmd = TrackPopupMenuEx(popup.0, (TPM_RETURNCMD | TPM_RIGHTBUTTON).0, pt.x, pt.y, helper.0, None);
        let _ = PostMessageW(Some(helper.0), WM_NULL, WPARAM(0), LPARAM(0));
        cmd.0
    };
    CURRENT.with(|c| *c.borrow_mut() = None);
    let Some(id) = usize::try_from(chosen).ok().and_then(|c| c.checked_sub(1)) else { return Ok(false) };

    let info = CMINVOKECOMMANDINFOEX {
        cbSize: u32::try_from(std::mem::size_of::<CMINVOKECOMMANDINFOEX>()).unwrap_or(0),
        fMask: CMIC_MASK_UNICODE | CMIC_MASK_PTINVOKE,
        hwnd: owner,
        // The verb is the command offset (MAKEINTRESOURCE), never read as text.
        lpVerb: PCSTR(id as *const u8),
        lpVerbW: PCWSTR(id as *const u16),
        nShow: SW_SHOWNORMAL.0,
        ptInvoke: pt,
        ..Default::default()
    };
    // SAFETY: `info` is a complete CMINVOKECOMMANDINFOEX (its size in `cbSize`), which
    // starts like CMINVOKECOMMANDINFO as the API expects.
    unsafe { menu.InvokeCommand(std::ptr::from_ref(&info).cast::<CMINVOKECOMMANDINFO>()) }?;
    Ok(true)
}

/// Window procedure of the hidden owner: the sub-menus of the shell draw themselves.
extern "system" fn helper_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if matches!(msg, WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR) {
        let handled = CURRENT.with(|c| {
            c.borrow().as_ref().and_then(|m| {
                let mut result = LRESULT(0);
                // SAFETY: a message of the menu shown, handed to the object that built it.
                unsafe { m.HandleMenuMsg2(msg, wparam, lparam, Some(&mut result)) }.ok().map(|()| result)
            })
        });
        if let Some(r) = handled {
            return r;
        }
    }
    // SAFETY: default handling of a message of our own window.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Hidden window owning the menu, destroyed when dropped.
struct Helper(HWND);

impl Helper {
    fn create() -> Result<Self, ShellMenuError> {
        // SAFETY: the module handle of this process; the class is registered once per
        // process (a second registration fails harmlessly) with a procedure of this module.
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(helper_proc),
                hInstance: instance.into(),
                lpszClassName: w!("UltraRushFTPShellMenu"),
                ..Default::default()
            };
            RegisterClassW(&class);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0),
                w!("UltraRushFTPShellMenu"),
                w!(""),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            Ok(Self(hwnd))
        }
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        // SAFETY: our window, on this thread.
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

struct MenuGuard(HMENU);

impl Drop for MenuGuard {
    fn drop(&mut self) {
        // SAFETY: the menu created by `explorer_menu`, no longer shown.
        unsafe {
            let _ = DestroyMenu(self.0);
        }
    }
}

/// Item lists returned by the shell, freed when dropped.
struct Pidls(Vec<*mut ITEMIDLIST>);

impl Drop for Pidls {
    fn drop(&mut self) {
        for &p in &self.0 {
            // SAFETY: each pointer came from `SHParseDisplayName` and is freed once.
            unsafe { ILFree(Some(p.cast_const())) };
        }
    }
}

struct ComGuard(bool);

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the successful `CoInitializeEx` of `explorer_menu`.
            unsafe { CoUninitialize() };
        }
    }
}
