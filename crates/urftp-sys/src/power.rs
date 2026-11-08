//! Power actions run when a queue is finished ("On transfer completed" of FTPRush v2):
//! sleep, hibernate, shut down. The application always asks first, with a countdown.

use std::path::PathBuf;
use std::process::Command;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LUID};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES,
    TOKEN_QUERY,
};
use windows_sys::Win32::System::Power::SetSuspendState;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use crate::win::wide;

/// Errors of the power actions.
#[derive(Debug, thiserror::Error)]
pub enum PowerError {
    /// The shutdown privilege could not be enabled, or Windows refused to sleep.
    #[error("Windows refused the request: {0}")]
    Os(#[source] std::io::Error),
    /// `shutdown.exe` could not be started.
    #[error("cannot start {program}: {source}")]
    Program {
        /// Program started.
        program: String,
        /// Error reported by Windows.
        source: std::io::Error,
    },
}

/// Enables `SeShutdownPrivilege` for this process (needed by `SetSuspendState`).
fn enable_shutdown_privilege() -> Result<(), PowerError> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: `GetCurrentProcess` returns a pseudo-handle; `token` is a valid out pointer.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) } == 0 {
        return Err(PowerError::Os(std::io::Error::last_os_error()));
    }
    let name = wide("SeShutdownPrivilege");
    let mut luid = LUID { LowPart: 0, HighPart: 0 };
    // SAFETY: `name` is NUL-terminated UTF-16 and `luid` a valid out pointer.
    let found = unsafe { LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) } != 0;
    let privileges =
        TOKEN_PRIVILEGES { PrivilegeCount: 1, Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }] };
    // SAFETY: `token` was opened above with TOKEN_ADJUST_PRIVILEGES; `privileges` lives for
    // the call; the previous state is not asked for (null pointers, size 0).
    let adjusted = found && unsafe { AdjustTokenPrivileges(token, 0, &privileges, 0, std::ptr::null_mut(), std::ptr::null_mut()) } != 0;
    let error = std::io::Error::last_os_error();
    // SAFETY: `token` is a handle opened above, closed once.
    unsafe { CloseHandle(token) };
    // AdjustTokenPrivileges succeeds without granting a privilege the account lacks.
    if !adjusted || error.raw_os_error() == Some(1300) {
        return Err(PowerError::Os(error));
    }
    Ok(())
}

/// Puts the computer to sleep (or hibernates it with `hibernate`).
fn suspend(hibernate: bool) -> Result<(), PowerError> {
    enable_shutdown_privilege()?;
    // SAFETY: plain call with boolean arguments; it returns once the computer wakes up.
    if !unsafe { SetSuspendState(hibernate, false, false) } {
        return Err(PowerError::Os(std::io::Error::last_os_error()));
    }
    Ok(())
}

/// Puts the computer to sleep.
pub fn sleep() -> Result<(), PowerError> {
    suspend(false)
}

/// Hibernates the computer (Windows sleeps instead when hibernation is off).
pub fn hibernate() -> Result<(), PowerError> {
    suspend(true)
}

/// Shuts the computer down now (`shutdown.exe /s /t 0` from the Windows folder, never from
/// the `PATH`).
pub fn shut_down() -> Result<(), PowerError> {
    let windows = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let program = windows.join("System32").join("shutdown.exe");
    Command::new(&program)
        .args(["/s", "/t", "0"])
        .spawn()
        .map(|_| ())
        .map_err(|source| PowerError::Program { program: program.display().to_string(), source })
}
