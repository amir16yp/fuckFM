//! Network neighborhood + remote URI mounts (`smb` / `ftp` / `sftp` / `mtp`).

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use anyhow::Result;
use std::path::PathBuf;

#[cfg(unix)]
pub use unix::{list_network_hosts, mount_remote, unmount_remote};

#[cfg(windows)]
pub use windows::{list_network_hosts, mount_remote, unmount_remote};

#[derive(Clone, Debug)]
pub struct NetworkHost {
    pub name: String,
    /// Suggested browse URI, e.g. `smb://hostname/` or `\\hostname`.
    pub uri: String,
}

#[derive(Clone, Debug)]
pub struct RemoteMount {
    pub uri: String,
    pub path: PathBuf,
}

pub fn is_remote_uri(s: &str) -> bool {
    let lower = s.trim().to_ascii_lowercase();
    lower.starts_with("smb://")
        || lower.starts_with("ftp://")
        || lower.starts_with("sftp://")
        || lower.starts_with("mtp://")
        || lower.starts_with("\\\\")
        || lower.starts_with("//")
}

pub fn normalize_uri(raw: &str) -> String {
    let s = raw.trim();
    if s.starts_with("\\\\") {
        // UNC → smb://host/share
        let rest = s.trim_start_matches('\\');
        let parts: Vec<&str> = rest.split('\\').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            return s.to_string();
        }
        if parts.len() == 1 {
            return format!("smb://{}/", parts[0]);
        }
        return format!("smb://{}/{}/", parts[0], parts[1..].join("/"));
    }
    if s.starts_with("//") && !s.starts_with("///") {
        return format!("smb:{}", s);
    }
    s.to_string()
}

pub fn network_entries(
    recent: &[String],
    bookmarks: &[String],
) -> Result<Vec<crate::browser::DirEntry>> {
    use crate::browser::{DirEntry, EntryKind};
    let mut out = Vec::new();
    out.push(DirEntry {
        name: "Connect to server…".into(),
        path: PathBuf::from("fuckfm:connect"),
        is_dir: false,
        kind: EntryKind::File,
        size: None,
        modified: None,
    });
    let mut seen = std::collections::BTreeSet::new();
    for uri in bookmarks.iter().chain(recent.iter()) {
        let uri = normalize_uri(uri);
        if !seen.insert(uri.clone()) {
            continue;
        }
        out.push(DirEntry {
            name: format!("★ {uri}"),
            path: PathBuf::from(&uri),
            is_dir: true,
            kind: EntryKind::Folder,
            size: None,
            modified: None,
        });
    }
    match list_network_hosts() {
        Ok(hosts) => {
            for host in hosts {
                if !seen.insert(host.uri.clone()) {
                    continue;
                }
                out.push(DirEntry {
                    name: host.name,
                    path: PathBuf::from(host.uri),
                    is_dir: true,
                    kind: EntryKind::Folder,
                    size: None,
                    modified: None,
                });
            }
        }
        Err(_) => {
            out.push(DirEntry {
                name: "(network discovery unavailable)".into(),
                path: PathBuf::from("fuckfm:network-unavailable"),
                is_dir: false,
                kind: EntryKind::File,
                size: None,
                modified: None,
            });
        }
    }
    Ok(out)
}

/// Open a remote URI; returns local mount path when successful.
pub fn open_remote_uri(uri: &str, user: Option<&str>, password: Option<&str>) -> Result<RemoteMount> {
    let uri = normalize_uri(uri);
    mount_remote(&uri, user, password)
}
