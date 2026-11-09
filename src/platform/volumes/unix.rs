use crate::browser::{DirEntry, EntryKind};
use crate::platform::volumes::human_bytes;
use anyhow::Result;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Volume {
    pub label: String,
    pub path: PathBuf,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub fs_type: String,
}

impl Volume {
    pub fn display_name(&self) -> String {
        if self.total_bytes == 0 {
            return format!("{} ({})", self.label, self.path.display());
        }
        format!(
            "{} — {} free of {}",
            self.label,
            human_bytes(self.free_bytes),
            human_bytes(self.total_bytes)
        )
    }
}

pub fn mounted_volumes() -> Result<Vec<Volume>> {
    let mut by_path: BTreeMap<PathBuf, Volume> = BTreeMap::new();

    if let Ok(mut v) = volume_at(Path::new("/"), "Linux System", "root") {
        v.label = "Linux System (/)".into();
        by_path.insert(v.path.clone(), v);
    }

    if let Ok(text) = fs::read_to_string("/proc/mounts") {
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let _src = parts.next();
            let Some(mount) = parts.next() else {
                continue;
            };
            let fs = parts.next().unwrap_or("unknown");
            if !keep_fs(fs) {
                continue;
            }
            let path = PathBuf::from(unescape_mount(mount));
            if !path.is_absolute() || !path.exists() || is_noisy_mount(&path) {
                continue;
            }
            let label = volume_label(&path, fs);
            if let Ok(vol) = volume_at(&path, &label, fs) {
                by_path.entry(vol.path.clone()).or_insert(vol);
            }
        }
    }

    for base in [
        PathBuf::from("/mnt"),
        PathBuf::from("/media"),
        PathBuf::from("/run/media"),
    ] {
        collect_under(&base, &mut by_path);
    }

    let mut out: Vec<Volume> = by_path.into_values().collect();
    out.sort_by(|a, b| match (a.path.as_path() == Path::new("/"), b.path.as_path() == Path::new("/")) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.path.cmp(&b.path),
    });
    Ok(out)
}

pub fn this_pc_entries() -> Result<Vec<DirEntry>> {
    Ok(mounted_volumes()?
        .into_iter()
        .map(|v| DirEntry {
            name: v.display_name(),
            path: v.path.clone(),
            is_dir: true,
            kind: EntryKind::Drive {
                free_bytes: v.free_bytes,
                total_bytes: v.total_bytes,
            },
            size: Some(v.total_bytes),
            modified: None,
        })
        .collect())
}

fn collect_under(base: &Path, out: &mut BTreeMap<PathBuf, Volume>) {
    let Ok(read) = fs::read_dir(base) else {
        return;
    };
    for item in read.flatten() {
        let path = item.path();
        if !path.is_dir() {
            continue;
        }

        // /run/media/<user>/<label>
        if base == Path::new("/run/media") {
            if let Ok(nested) = fs::read_dir(&path) {
                for child in nested.flatten() {
                    let child_path = child.path();
                    if child_path.is_dir() {
                        let name = child.file_name().to_string_lossy().to_string();
                        if let Ok(vol) =
                            volume_at(&child_path, &format!("Removable ({name})"), "removable")
                        {
                            out.entry(vol.path.clone()).or_insert(vol);
                        }
                    }
                }
            }
            continue;
        }

        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Volume")
            .to_string();
        if let Ok(vol) = volume_at(&path, &name, "mount") {
            out.entry(vol.path.clone()).or_insert(vol);
        }
    }
}

fn volume_at(path: &Path, label: &str, fs_type: &str) -> Result<Volume> {
    let (total, free) = disk_usage(path).unwrap_or((0, 0));
    Ok(Volume {
        label: label.to_string(),
        path: path.to_path_buf(),
        total_bytes: total,
        free_bytes: free,
        fs_type: fs_type.to_string(),
    })
}

fn disk_usage(path: &Path) -> Result<(u64, u64)> {
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|e| anyhow::anyhow!(e))?;
    unsafe {
        let mut s: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut s) != 0 {
            anyhow::bail!("statvfs failed for {}", path.display());
        }
        let frsize = s.f_frsize as u64;
        let total = s.f_blocks as u64 * frsize;
        let free = s.f_bavail as u64 * frsize;
        Ok((total, free))
    }
}

fn keep_fs(fs: &str) -> bool {
    matches!(
        fs,
        "ext2"
            | "ext3"
            | "ext4"
            | "btrfs"
            | "xfs"
            | "f2fs"
            | "zfs"
            | "vfat"
            | "exfat"
            | "ntfs"
            | "ntfs3"
            | "fuseblk"
            | "fuse"
            | "sshfs"
            | "nfs"
            | "nfs4"
            | "cifs"
            | "smb3"
            | "udf"
            | "iso9660"
            | "tmpfs"
    )
}

fn is_noisy_mount(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.starts_with("/proc")
        || s.starts_with("/sys")
        || s.starts_with("/dev")
        || s.starts_with("/run/user")
        || s.starts_with("/snap")
        || s.starts_with("/var/lib/docker")
        || s.starts_with("/var/lib/containers")
}

fn volume_label(path: &Path, fs: &str) -> String {
    if path == Path::new("/") {
        return "Linux System (/)".into();
    }
    if path == Path::new("/home") {
        return "Home (/home)".into();
    }
    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
        return format!("{name} ({fs})");
    }
    format!("{} ({fs})", path.display())
}

fn unescape_mount(s: &str) -> String {
    s.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

pub fn free_space(path: &Path) -> Option<u64> {
    disk_usage(path).ok().map(|(_, free)| free)
}