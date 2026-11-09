//! Register / unregister fuckFM as the default folder handler (HKCU).

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Register `exe` as the open handler for directories / drives / folder background (current user).
#[cfg(windows)]
pub fn register_folder_handler(exe: &Path) -> Result<()> {
    let exe = std::fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());
    let exe_s = exe.to_string_lossy();
    let exe_s = exe_s.strip_prefix(r"\\?\").unwrap_or(&exe_s);
    if !Path::new(exe_s).is_file() {
        bail!("exe not found: {exe_s}");
    }
    let cmd = format!("\"{exe_s}\" \"%1\"");
    let bg_cmd = format!("\"{exe_s}\" \"%V\"");

    set_open_command(
        r"Software\Classes\Directory\shell\open\command",
        &cmd,
    )?;
    set_open_command(r"Software\Classes\Drive\shell\open\command", &cmd)?;
    set_open_command(
        r"Software\Classes\Directory\Background\shell\open\command",
        &bg_cmd,
    )?;

    // Friendly “Open with fuckFM” verb (does not replace default alone).
    ensure_verb(
        r"Software\Classes\Directory\shell\openfuckfm",
        "Open with fuckFM",
        &cmd,
    )?;
    ensure_verb(
        r"Software\Classes\Directory\Background\shell\openfuckfm",
        "Open with fuckFM",
        &bg_cmd,
    )?;
    ensure_verb(
        r"Software\Classes\Drive\shell\openfuckfm",
        "Open with fuckFM",
        &cmd,
    )?;

    Ok(())
}

/// Restore backed-up open commands and remove fuckFM verbs.
#[cfg(windows)]
pub fn unregister_folder_handler() -> Result<()> {
    restore_open_command(r"Software\Classes\Directory\shell\open\command")?;
    restore_open_command(r"Software\Classes\Drive\shell\open\command")?;
    restore_open_command(r"Software\Classes\Directory\Background\shell\open\command")?;
    let _ = delete_tree(r"Software\Classes\Directory\shell\openfuckfm");
    let _ = delete_tree(r"Software\Classes\Directory\Background\shell\openfuckfm");
    let _ = delete_tree(r"Software\Classes\Drive\shell\openfuckfm");
    Ok(())
}

#[cfg(windows)]
pub fn is_folder_handler_registered() -> bool {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey(r"Software\Classes\Directory\shell\open\command") else {
        return false;
    };
    let Ok(cmd) = key.get_value::<String, _>("") else {
        return false;
    };
    let lower = cmd.to_ascii_lowercase();
    lower.contains("fuckfm.exe") || lower.contains("fuckfm\"")
}

#[cfg(windows)]
fn set_open_command(subkey: &str, command: &str) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(subkey)
        .with_context(|| format!("create {subkey}"))?;
    // Backup once.
    if key.get_value::<String, _>("fuckfm.bak").is_err() {
        if let Ok(prev) = key.get_value::<String, _>("") {
            let _ = key.set_value("fuckfm.bak", &prev);
        } else {
            let _ = key.set_value("fuckfm.bak", &"");
        }
    }
    key.set_value("", &command)
        .with_context(|| format!("set {subkey}"))?;
    Ok(())
}

#[cfg(windows)]
fn restore_open_command(subkey: &str) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(subkey, KEY_READ | KEY_WRITE) else {
        return Ok(());
    };
    match key.get_value::<String, _>("fuckfm.bak") {
        Ok(prev) if !prev.is_empty() => {
            key.set_value("", &prev)?;
            let _ = key.delete_value("fuckfm.bak");
        }
        Ok(_) => {
            // Empty backup → delete our override by removing the value if possible.
            let _ = key.delete_value("");
            let _ = key.delete_value("fuckfm.bak");
        }
        Err(_) => {}
    }
    Ok(())
}

#[cfg(windows)]
fn ensure_verb(subkey: &str, title: &str, command: &str) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(subkey)?;
    key.set_value("", &title)?;
    let (cmd_key, _) = key.create_subkey("command")?;
    cmd_key.set_value("", &command)?;
    Ok(())
}

#[cfg(windows)]
fn delete_tree(subkey: &str) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    // Best-effort: delete command then verb.
    let _ = hkcu.delete_subkey_all(subkey);
    Ok(())
}

/// Path used when registering from scripts / installers.
pub fn default_fm_exe() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("current_exe")?;
    let fm = exe.with_file_name(if cfg!(windows) {
        "fuckfm.exe"
    } else {
        "fuckfm"
    });
    if fm.is_file() {
        Ok(fm)
    } else {
        Ok(exe)
    }
}

#[cfg(not(windows))]
pub fn register_folder_handler(_exe: &Path) -> Result<()> {
    bail!("folder-handler registration is Windows-only")
}

#[cfg(not(windows))]
pub fn unregister_folder_handler() -> Result<()> {
    bail!("folder-handler registration is Windows-only")
}

#[cfg(not(windows))]
pub fn is_folder_handler_registered() -> bool {
    false
}
