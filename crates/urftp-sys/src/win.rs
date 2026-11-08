//! Small Win32 helpers shared by the application and the DLL: UTF-16 strings and named
//! shared memory.

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, OpenFileMappingW, PAGE_READWRITE, UnmapViewOfFile,
};

pub use crate::decode_ansi;

/// NUL-terminated UTF-16 string for the `…W` APIs.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Free space for this user on the volume of `dir` (an existing folder), in bytes.
pub fn free_space(dir: &std::path::Path) -> std::io::Result<u64> {
    let path = wide(&dir.to_string_lossy());
    let mut free: u64 = 0;
    // SAFETY: `path` is NUL-terminated UTF-16; `free` is a valid out pointer; the other
    // totals are not asked for (null pointers are allowed).
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(path.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut())
    };
    if ok == 0 { Err(std::io::Error::last_os_error()) } else { Ok(free) }
}

/// Files and folders copied in Explorer (`CF_HDROP` on the clipboard); empty when the
/// clipboard holds something else or cannot be opened.
pub fn clipboard_files() -> Vec<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard};
    use windows_sys::Win32::System::Ole::CF_HDROP;
    use windows_sys::Win32::UI::Shell::{DragQueryFileW, HDROP};

    let format = u32::from(CF_HDROP);
    // SAFETY: plain query of the clipboard formats, no pointer involved.
    if unsafe { IsClipboardFormatAvailable(format) } == 0 {
        return Vec::new();
    }
    // SAFETY: a null owner window is allowed; the clipboard is closed below.
    if unsafe { OpenClipboard(std::ptr::null_mut()) } == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    // SAFETY: the clipboard is open; the handle stays owned by the clipboard and is only
    // read until `CloseClipboard`.
    let handle = unsafe { GetClipboardData(format) };
    if !handle.is_null() {
        let drop: HDROP = handle;
        // SAFETY: index 0xFFFFFFFF with a null buffer returns the number of files.
        let count = unsafe { DragQueryFileW(drop, u32::MAX, std::ptr::null_mut(), 0) };
        for i in 0..count {
            // SAFETY: a null buffer returns the length of name `i` (without the NUL).
            let len = unsafe { DragQueryFileW(drop, i, std::ptr::null_mut(), 0) };
            let mut buf = vec![0u16; len as usize + 1];
            let cap = u32::try_from(buf.len()).unwrap_or(u32::MAX);
            // SAFETY: `buf` has room for `cap` UTF-16 units, the NUL included.
            let got = unsafe { DragQueryFileW(drop, i, buf.as_mut_ptr(), cap) } as usize;
            if got > 0 {
                out.push(std::path::PathBuf::from(std::ffi::OsString::from_wide(&buf[..got.min(buf.len())])));
            }
        }
    }
    // SAFETY: the clipboard was opened above by this thread.
    unsafe { CloseClipboard() };
    out
}

/// `Ctrl+V` is held down now (the window framework does not report it when the
/// clipboard holds files rather than text).
pub fn ctrl_v_down() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL};
    // SAFETY: plain query of the keyboard state.
    let down = |vk: i32| unsafe { GetAsyncKeyState(vk) } < 0;
    down(i32::from(VK_CONTROL)) && down(0x56)
}

/// Local date and time of the computer: (year, month, day, hour, minute, second).
pub fn local_time() -> (u16, u16, u16, u16, u16, u16) {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: `GetLocalTime` only fills the structure it is given.
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    (t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

/// Errors of [`SharedMem`].
#[derive(Debug, thiserror::Error)]
pub enum SharedMemError {
    /// `size` must be between 1 byte and 4 GiB.
    #[error("invalid shared memory size: {0}")]
    InvalidSize(usize),
    /// `CreateFileMappingW` / `OpenFileMappingW` / `MapViewOfFile` failed.
    #[error("shared memory {name}: {source}")]
    Os {
        /// Name of the mapping.
        name: String,
        /// Error reported by Windows.
        source: std::io::Error,
    },
}

/// View on a named shared memory (`FTPRush`, `mIRC`).
pub struct SharedMem {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    size: usize,
}

// SAFETY: the view is plain memory owned by this value and the handle is only closed in
// `Drop`; both may move to another thread.
unsafe impl Send for SharedMem {}
// SAFETY: `read_string` / `write_string` only copy bytes within the `size` bytes of the
// view. Concurrent use of one region is serialised by the protocol (one synchronous
// `SendMessage` exchange at a time); a race can garble text but never touch memory
// outside the view.
unsafe impl Sync for SharedMem {}

impl SharedMem {
    /// Creates (or opens when it already exists) the shared memory `name`.
    pub fn create(name: &str, size: usize) -> Result<Self, SharedMemError> {
        let size32 = u32::try_from(size).ok().filter(|&s| s > 0).ok_or(SharedMemError::InvalidSize(size))?;
        let n = wide(name);
        // SAFETY: `n` is a valid NUL-terminated UTF-16 string living across the call; null
        // security attributes are allowed; the returned handle is owned by `map`.
        let handle = unsafe { CreateFileMappingW(INVALID_HANDLE_VALUE, std::ptr::null(), PAGE_READWRITE, 0, size32, n.as_ptr()) };
        Self::map(name, handle, size)
    }

    /// Opens an existing shared memory.
    pub fn open(name: &str, size: usize) -> Result<Self, SharedMemError> {
        if size == 0 {
            return Err(SharedMemError::InvalidSize(size));
        }
        let n = wide(name);
        // SAFETY: `n` is a valid NUL-terminated UTF-16 string living across the call.
        let handle = unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, n.as_ptr()) };
        Self::map(name, handle, size)
    }

    fn map(name: &str, handle: HANDLE, size: usize) -> Result<Self, SharedMemError> {
        let os = || SharedMemError::Os { name: name.to_string(), source: std::io::Error::last_os_error() };
        if handle.is_null() {
            return Err(os());
        }
        // SAFETY: `handle` is a valid file-mapping handle obtained just above.
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, size) };
        if view.Value.is_null() {
            let err = os();
            // SAFETY: `handle` is valid and not used after this point.
            unsafe { CloseHandle(handle) };
            return Err(err);
        }
        Ok(Self { handle, view, size })
    }

    fn ptr(&self) -> *mut u8 {
        self.view.Value.cast()
    }

    /// Reads the NUL-terminated string at the start of the region.
    pub fn read_string(&self) -> String {
        // SAFETY: the view maps at least `size` readable bytes for the lifetime of `self`.
        let bytes = unsafe { std::slice::from_raw_parts(self.ptr(), self.size) };
        decode_ansi(bytes)
    }

    /// Writes a string (truncated to the region size) followed by a NUL.
    pub fn write_string(&self, s: &str) {
        let bytes = s.as_bytes();
        let n = bytes.len().min(self.size - 1);
        // SAFETY: `n + 1 <= size` bytes are written inside the writable view; the source
        // slice does not overlap the mapping.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.ptr(), n);
            *self.ptr().add(n) = 0;
        }
    }
}

impl Drop for SharedMem {
    fn drop(&mut self) {
        // SAFETY: view and handle were obtained in `map` and are released exactly once.
        unsafe {
            UnmapViewOfFile(self.view);
            CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_memory_round_trip() {
        let name = format!("urftp-test-{}", std::process::id());
        let a = SharedMem::create(&name, 64).unwrap();
        let b = SharedMem::open(&name, 64).unwrap();
        a.write_string("bonjour");
        assert_eq!(b.read_string(), "bonjour");
        b.write_string(&"x".repeat(200));
        assert_eq!(a.read_string().len(), 63, "truncated to the region size");
        assert!(matches!(SharedMem::create("x", 0), Err(SharedMemError::InvalidSize(0))));
        assert!(matches!(SharedMem::open("urftp-test-missing-mapping", 8), Err(SharedMemError::Os { .. })));
    }
}
