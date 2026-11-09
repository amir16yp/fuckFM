//! File attributes (Windows) and permissions / ownership (Linux).

use anyhow::{bail, Context, Result};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WinAttrs {
    pub readonly: bool,
    pub hidden: bool,
}

/// Read Windows file attributes (no-op metadata on non-Windows).
pub fn get_win_attrs(path: &Path) -> Result<WinAttrs> {
    #[cfg(windows)]
    {
        get_win_attrs_impl(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Ok(WinAttrs::default())
    }
}

/// Toggle or set Windows Read-only / Hidden. Elevates via UAC on access denied.
pub fn set_win_attrs(path: &Path, attrs: WinAttrs) -> Result<()> {
    #[cfg(windows)]
    {
        set_win_attrs_impl(path, attrs)
    }
    #[cfg(not(windows))]
    {
        let _ = (path, attrs);
        bail!("Windows attributes are only available on Windows")
    }
}

/// Unix mode bits (e.g. 0o644). Returns None on non-Unix.
pub fn get_unix_mode(path: &Path) -> Result<Option<u32>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(path)
            .with_context(|| format!("stat {}", path.display()))?;
        Ok(Some(meta.permissions().mode() & 0o7777))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(None)
    }
}

/// Set Unix mode. On EPERM/EACCES, retries via pkexec `chmod`.
pub fn set_unix_mode(path: &Path, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        set_unix_mode_impl(path, mode)
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        bail!("Unix permissions are only available on Unix")
    }
}

/// Take ownership of `path` (Windows: takeown+icacls elevated; Unix: pkexec chown).
pub fn take_ownership(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        take_ownership_windows(path)
    }
    #[cfg(unix)]
    {
        take_ownership_unix(path)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        bail!("take ownership unsupported on this platform")
    }
}

#[cfg(windows)]
fn get_win_attrs_impl(path: &Path) -> Result<WinAttrs> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileAttributesW, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_READONLY,
        INVALID_FILE_ATTRIBUTES,
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let attr = unsafe { GetFileAttributesW(wide.as_ptr()) };
    if attr == INVALID_FILE_ATTRIBUTES {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("GetFileAttributesW {}", path.display()));
    }
    Ok(WinAttrs {
        readonly: attr & FILE_ATTRIBUTE_READONLY != 0,
        hidden: attr & FILE_ATTRIBUTE_HIDDEN != 0,
    })
}

#[cfg(windows)]
fn set_win_attrs_impl(path: &Path, want: WinAttrs) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_NORMAL,
        FILE_ATTRIBUTE_READONLY, INVALID_FILE_ATTRIBUTES,
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let cur = unsafe { GetFileAttributesW(wide.as_ptr()) };
    if cur == INVALID_FILE_ATTRIBUTES {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("GetFileAttributesW {}", path.display()));
    }
    let mut next = cur & !(FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_HIDDEN);
    if want.readonly {
        next |= FILE_ATTRIBUTE_READONLY;
    }
    if want.hidden {
        next |= FILE_ATTRIBUTE_HIDDEN;
    }
    if next == 0 {
        next = FILE_ATTRIBUTE_NORMAL;
    }
    let ok = unsafe { SetFileAttributesW(wide.as_ptr(), next) };
    if ok == 0 {
        let err = std::io::Error::last_os_error();
        if super::elevate::is_access_denied(&err) {
            return set_win_attrs_elevated(path, want);
        }
        return Err(err).with_context(|| format!("SetFileAttributesW {}", path.display()));
    }
    Ok(())
}

#[cfg(windows)]
fn set_win_attrs_elevated(path: &Path, want: WinAttrs) -> Result<()> {
    let p = path.display().to_string().replace('\'', "''");
    let mut flags = Vec::new();
    if want.readonly {
        flags.push("ReadOnly");
    }
    if want.hidden {
        flags.push("Hidden");
    }
    let script = if flags.is_empty() {
        format!(
            "$i=Get-Item -LiteralPath '{p}' -Force; $i.Attributes = 'Normal'"
        )
    } else {
        format!(
            "$i=Get-Item -LiteralPath '{p}' -Force; $i.Attributes = '{}'",
            flags.join(",")
        )
    };
    super::elevate::run_elevated_powershell(&script)
}

#[cfg(windows)]
fn take_ownership_windows(path: &Path) -> Result<()> {
    let p = path.display().to_string().replace('\'', "''");
    let script = format!(
        "takeown /F '{p}' /R /D Y; icacls '{p}' /grant \"${{env:USERNAME}}:(F)\" /T /C"
    );
    super::elevate::run_elevated_powershell(&script)
}

#[cfg(unix)]
fn set_unix_mode_impl(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(mode);
    match std::fs::set_permissions(path, perms) {
        Ok(()) => Ok(()),
        Err(err) if super::elevate::is_access_denied(&err) => {
            if !super::elevate::has_pkexec() {
                return Err(err).context("chmod (install pkexec for elevation)");
            }
            let mode_s = format!("{mode:o}");
            super::elevate::run_pkexec(&["chmod", &mode_s, &path.display().to_string()])
        }
        Err(err) => Err(err).with_context(|| format!("chmod {}", path.display())),
    }
}

#[cfg(unix)]
fn take_ownership_unix(path: &Path) -> Result<()> {
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    if !super::elevate::has_pkexec() {
        bail!("pkexec not found — install policykit to take ownership");
    }
    super::elevate::run_pkexec(&[
        "chown",
        &format!("{uid}:{gid}"),
        &path.display().to_string(),
    ])
}
