//! Known / special folders for the side pane and This PC.

use std::path::PathBuf;

/// Built-in navigation places beyond Home/Desktop (platform-specific).
pub fn extra_places() -> Vec<(String, PathBuf)> {
    #[cfg(windows)]
    {
        windows_side_places()
    }
    #[cfg(not(windows))]
    {
        unix_side_places()
    }
}

/// Folders shown at the top of This PC (Windows Explorer-style).
#[cfg(windows)]
pub fn windows_this_pc_folders() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for (label, name) in [
        ("Desktop", "Desktop"),
        ("Documents", "Personal"),
        ("Downloads", "{374DE290-123F-4565-9164-39C4925E467B}"),
        ("Music", "My Music"),
        ("Pictures", "My Pictures"),
        ("Videos", "My Video"),
    ] {
        if let Some(path) = shell_folder(name) {
            if path.is_dir() {
                out.push((label.into(), path));
            }
        }
    }
    out
}

#[cfg(windows)]
fn windows_side_places() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for (label, name) in [
        ("Documents", "Personal"),
        ("Downloads", "{374DE290-123F-4565-9164-39C4925E467B}"),
        ("Pictures", "My Pictures"),
        ("Music", "My Music"),
        ("Videos", "My Video"),
    ] {
        if let Some(path) = shell_folder(name) {
            if path.is_dir() {
                out.push((label.into(), path));
            }
        }
    }
    out
}

#[cfg(not(windows))]
fn unix_side_places() -> Vec<(String, PathBuf)> {
    let home = crate::ops::home_dir();
    let mut out = Vec::new();
    for (label, rel) in [
        ("Documents", "Documents"),
        ("Downloads", "Downloads"),
        ("Pictures", "Pictures"),
        ("Music", "Music"),
        ("Videos", "Videos"),
    ] {
        let path = xdg_user_dir(rel).unwrap_or_else(|| home.join(rel));
        if path.is_dir() {
            out.push((label.into(), path));
        }
    }
    out
}

#[cfg(not(windows))]
fn xdg_user_dir(name: &str) -> Option<PathBuf> {
    // Prefer `xdg-user-dir` when available.
    if let Ok(out) = std::process::Command::new("xdg-user-dir").arg(name).output() {
        if out.status.success() {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() {
                let path = PathBuf::from(p);
                if path.is_dir() {
                    return Some(path);
                }
            }
        }
    }
    None
}

/// Read an expanded path from HKCU Explorer Shell Folders.
#[cfg(windows)]
fn shell_folder(name: &str) -> Option<PathBuf> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    // Shell Folders = already-expanded paths.
    if let Ok(key) =
        hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders")
    {
        if let Ok(path) = key.get_value::<String, _>(name) {
            let p = PathBuf::from(path);
            if !p.as_os_str().is_empty() {
                return Some(p);
            }
        }
    }
    // User Shell Folders may contain %USERPROFILE% — expand env.
    if let Ok(key) =
        hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders")
    {
        if let Ok(raw) = key.get_value::<String, _>(name) {
            let expanded = expand_env(&raw);
            let p = PathBuf::from(expanded);
            if !p.as_os_str().is_empty() {
                return Some(p);
            }
        }
    }
    // Fallbacks for common layouts.
    let home = crate::ops::home_dir();
    let fallback = match name {
        "Desktop" => home.join("Desktop"),
        "Personal" => home.join("Documents"),
        "My Music" => home.join("Music"),
        "My Pictures" => home.join("Pictures"),
        "My Video" => home.join("Videos"),
        "{374DE290-123F-4565-9164-39C4925E467B}" => home.join("Downloads"),
        _ => return None,
    };
    fallback.is_dir().then_some(fallback)
}

#[cfg(windows)]
fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' {
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == '%') {
                let name: String = chars[i + 1..i + 1 + end].iter().collect();
                if let Ok(val) = std::env::var(&name) {
                    out.push_str(&val);
                    i += end + 2;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}
