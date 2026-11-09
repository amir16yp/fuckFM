use super::{NetworkHost, RemoteMount};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn list_network_hosts() -> Result<Vec<NetworkHost>> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    // Avahi / mdns browse (optional).
    if let Ok(text) = run_capture("avahi-browse", &["-atrk"]) {
        for line in text.lines() {
            // = eth0 IPv4 MyNAS ...
            if !line.starts_with('=') {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                continue;
            }
            let name = parts[3].replace("\\032", " ");
            if name.is_empty() || !seen.insert(name.clone()) {
                continue;
            }
            out.push(NetworkHost {
                uri: format!("smb://{}/", name.replace(' ', "-")),
                name: format!("{name} (mDNS)"),
            });
        }
    }

    // smbtree short listing (may need workgroup).
    if let Ok(text) = run_capture("smbtree", &["-N"]) {
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with("\\\\") || t.starts_with("//") {
                let host = t
                    .trim_start_matches('\\')
                    .trim_start_matches('/')
                    .split(['\\', '/'])
                    .find(|s| !s.is_empty())
                    .unwrap_or("");
                if host.is_empty() || !seen.insert(host.to_string()) {
                    continue;
                }
                out.push(NetworkHost {
                    name: host.to_string(),
                    uri: format!("smb://{host}/"),
                });
            }
        }
    }

    Ok(out)
}

pub fn mount_remote(
    uri: &str,
    user: Option<&str>,
    password: Option<&str>,
) -> Result<RemoteMount> {
    let uri = uri.trim();
    // Already a local GVFS mount?
    if let Some(existing) = find_gvfs_mount(uri) {
        return Ok(RemoteMount {
            uri: uri.to_string(),
            path: existing,
        });
    }

    let before = gvfs_roots();

    // gio mount supports smb/ftp/sftp/mtp.
    let mut args = vec!["mount".to_string(), uri.to_string()];
    if let (Some(u), Some(p)) = (user, password) {
        // Prefer anonymous then ask — gio uses ask-password helper; pass via env when possible.
        std::env::set_var("GIO_PASSWORD", p);
        let _ = u;
    }
    let status = Command::new("gio")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .context("gio mount")?;
    let _ = std::env::remove_var("GIO_PASSWORD");

    if !status.success() {
        // Fallback: smbclient / curlftpfs not attempted — surface clear error.
        bail!("gio mount failed for {uri} (credentials may be required)");
    }

    // Wait briefly for GVFS path to appear.
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Some(path) = find_new_gvfs(&before).or_else(|| find_gvfs_mount(uri)) {
            return Ok(RemoteMount {
                uri: uri.to_string(),
                path,
            });
        }
    }
    if let Some(path) = find_gvfs_mount(uri) {
        return Ok(RemoteMount {
            uri: uri.to_string(),
            path,
        });
    }
    bail!("mounted {uri} but could not locate GVFS path")
}

pub fn unmount_remote(uri_or_path: &str) -> Result<()> {
    let target = if Path::new(uri_or_path).is_dir() {
        uri_or_path.to_string()
    } else {
        find_gvfs_mount(uri_or_path)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| uri_or_path.to_string())
    };
    let status = Command::new("gio")
        .args(["mount", "-u", &target])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("gio unmount")?;
    if !status.success() {
        bail!("gio unmount failed for {target}");
    }
    Ok(())
}

fn gvfs_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let uid = unsafe { libc::getuid() };
    let base = PathBuf::from(format!("/run/user/{uid}/gvfs"));
    if let Ok(rd) = fs::read_dir(&base) {
        for ent in rd.flatten() {
            roots.push(ent.path());
        }
    }
    roots
}

fn find_new_gvfs(before: &[PathBuf]) -> Option<PathBuf> {
    for p in gvfs_roots() {
        if !before.iter().any(|b| b == &p) {
            return Some(p);
        }
    }
    None
}

fn find_gvfs_mount(uri: &str) -> Option<PathBuf> {
    let needle = uri_to_gvfs_needle(uri);
    let uid = unsafe { libc::getuid() };
    let base = PathBuf::from(format!("/run/user/{uid}/gvfs"));
    let rd = fs::read_dir(&base).ok()?;
    for ent in rd.flatten() {
        let name = ent.file_name().to_string_lossy().into_owned();
        if name.contains(&needle) {
            return Some(ent.path());
        }
    }
    // Also match recent mtime mounts loosely.
    let mut best: Option<(SystemTime, PathBuf)> = None;
    let rd = fs::read_dir(&base).ok()?;
    for ent in rd.flatten() {
        let meta = ent.metadata().ok()?;
        let m = meta.modified().unwrap_or(UNIX_EPOCH);
        if best.as_ref().map(|(t, _)| m > *t).unwrap_or(true) {
            best = Some((m, ent.path()));
        }
    }
    // Only return recent if needle empty-ish.
    if needle.is_empty() {
        best.map(|(_, p)| p)
    } else {
        None
    }
}

fn uri_to_gvfs_needle(uri: &str) -> String {
    let u = uri.trim().trim_end_matches('/');
    if let Some(rest) = u
        .strip_prefix("smb://")
        .or_else(|| u.strip_prefix("ftp://"))
        .or_else(|| u.strip_prefix("sftp://"))
        .or_else(|| u.strip_prefix("mtp://"))
    {
        let host = rest.split('/').next().unwrap_or(rest);
        let host = host.split('@').last().unwrap_or(host);
        return host.to_string();
    }
    u.to_string()
}

fn run_capture(bin: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("spawn {bin}"))?;
    if !out.status.success() {
        bail!("{bin} failed");
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
