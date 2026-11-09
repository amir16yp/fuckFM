//! Privilege elevation helpers (Windows UAC / Linux pkexec).

use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

/// True when an I/O error looks like a permission / access-denied failure.
pub fn is_access_denied(err: &std::io::Error) -> bool {
    match err.kind() {
        std::io::ErrorKind::PermissionDenied => true,
        _ => {
            #[cfg(windows)]
            {
                err.raw_os_error() == Some(5) // ERROR_ACCESS_DENIED
            }
            #[cfg(not(windows))]
            {
                err.raw_os_error() == Some(1) /* EPERM */ || err.raw_os_error() == Some(13)
                /* EACCES */
            }
        }
    }
}

/// Run a PowerShell script elevated (UAC prompt). Returns after the elevated
/// process is *spawned* (not necessarily finished).
#[cfg(windows)]
pub fn run_elevated_powershell(script: &str) -> Result<()> {
    // Encode as Base64 UTF-16LE so quoting stays safe across UAC.
    let wide: Vec<u8> = script
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    let b64 = base64_encode(&wide);
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Start-Process -FilePath powershell.exe -Verb RunAs -Wait -ArgumentList '-NoProfile','-EncodedCommand','{b64}'"
            ),
        ])
        .stdin(Stdio::null())
        .status()
        .context("elevate via UAC")
        .and_then(|st| {
            if st.success() {
                Ok(())
            } else {
                bail!("elevated PowerShell exited with {st}")
            }
        })
}

#[cfg(windows)]
fn base64_encode(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push(T[(n & 63) as usize] as char);
        i += 3;
    }
    match bytes.len() - i {
        1 => {
            let n = (bytes[i] as u32) << 16;
            out.push(T[((n >> 18) & 63) as usize] as char);
            out.push(T[((n >> 12) & 63) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
            out.push(T[((n >> 18) & 63) as usize] as char);
            out.push(T[((n >> 12) & 63) as usize] as char);
            out.push(T[((n >> 6) & 63) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

/// Run `argv` under pkexec (graphical auth). Blocks until finished.
#[cfg(unix)]
pub fn run_pkexec(argv: &[&str]) -> Result<()> {
    if argv.is_empty() {
        bail!("empty pkexec argv");
    }
    let status = Command::new("pkexec")
        .args(argv)
        .stdin(Stdio::null())
        .status()
        .context("pkexec")?;
    if status.success() {
        Ok(())
    } else {
        bail!("pkexec failed: {status}")
    }
}

#[cfg(unix)]
pub fn has_pkexec() -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|p| p.join("pkexec").is_file())
        })
        .unwrap_or(false)
}
