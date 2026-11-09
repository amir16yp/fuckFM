use crate::browser::{DirEntry, EntryKind};
use crate::platform::volumes::human_bytes;
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeKind {
    Fixed,
    Removable,
    Optical,
    Network,
    Ram,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct Volume {
    pub label: String,
    pub mount_path: Option<PathBuf>,
    pub device: Option<PathBuf>,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub fs_type: String,
    pub kind: VolumeKind,
    pub mounted: bool,
}

impl Volume {
    pub fn display_name(&self) -> String {
        let state = if self.mounted { "" } else { " [not mounted]" };
        let kind = match self.kind {
            VolumeKind::Removable => "Removable",
            VolumeKind::Optical => "Optical",
            VolumeKind::Network => "Network",
            VolumeKind::Ram => "RAM",
            VolumeKind::Fixed | VolumeKind::Unknown => "Volume",
        };
        let where_ = self
            .mount_path
            .as_ref()
            .map(|p| p.display().to_string())
            .or_else(|| self.device.as_ref().map(|p| p.display().to_string()))
            .unwrap_or_default();
        let title = if self.label.is_empty() {
            format!("{kind} ({where_})")
        } else if where_.is_empty() {
            format!("{} ({kind})", self.label)
        } else {
            format!("{} ({where_})", self.label)
        };
        if self.mounted && self.total_bytes > 0 {
            format!(
                "{title} — {} free of {}{state}",
                human_bytes(self.free_bytes),
                human_bytes(self.total_bytes),
                state = state
            )
        } else {
            format!("{title}{state}")
        }
    }

    pub fn action_target(&self) -> Option<PathBuf> {
        self.device
            .clone()
            .or_else(|| self.mount_path.clone())
    }
}

pub fn list_volumes() -> Result<Vec<Volume>> {
    let mut by_key: BTreeMap<String, Volume> = BTreeMap::new();
    let mut mounted_devs: BTreeSet<String> = BTreeSet::new();

    if let Ok(mut v) = volume_at(Path::new("/"), "Linux System", "root", VolumeKind::Fixed) {
        v.label = "Linux System (/)".into();
        by_key.insert("mnt:/".into(), v);
    }

    if let Ok(text) = fs::read_to_string("/proc/mounts") {
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let Some(src) = parts.next() else { continue };
            let Some(mount) = parts.next() else { continue };
            let fs = parts.next().unwrap_or("unknown");
            if !keep_fs(fs) {
                continue;
            }
            let path = PathBuf::from(unescape_mount(mount));
            if !path.is_absolute() || is_noisy_mount(&path) {
                continue;
            }
            let kind = classify_mount(&path, fs, src);
            let label = volume_label(&path, fs);
            if let Ok(mut vol) = volume_at(&path, &label, fs, kind) {
                if src.starts_with('/') {
                    vol.device = Some(PathBuf::from(src));
                    mounted_devs.insert(src.to_string());
                }
                by_key.insert(format!("mnt:{}", path.display()), vol);
            }
        }
    }

    for base in [
        PathBuf::from("/mnt"),
        PathBuf::from("/media"),
        PathBuf::from("/run/media"),
    ] {
        collect_under(&base, &mut by_key);
    }

    // Unmounted block devices via lsblk when available.
    for vol in lsblk_unmounted(&mounted_devs) {
        let key = vol
            .device
            .as_ref()
            .map(|d| format!("dev:{}", d.display()))
            .unwrap_or_else(|| format!("label:{}", vol.label));
        by_key.entry(key).or_insert(vol);
    }

    let mut out: Vec<Volume> = by_key.into_values().collect();
    out.sort_by(|a, b| {
        match (
            a.mount_path.as_deref() == Some(Path::new("/")),
            b.mount_path.as_deref() == Some(Path::new("/")),
        ) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => match (a.mounted, b.mounted) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.label.cmp(&b.label),
            },
        }
    });
    Ok(out)
}

pub fn this_pc_entries() -> Result<Vec<DirEntry>> {
    Ok(list_volumes()?
        .into_iter()
        .map(|v| {
            let path = v
                .mount_path
                .clone()
                .or_else(|| v.device.clone())
                .unwrap_or_else(|| PathBuf::from(&v.label));
            DirEntry {
                name: v.display_name(),
                path,
                is_dir: v.mounted,
                kind: EntryKind::Drive {
                    free_bytes: v.free_bytes,
                    total_bytes: v.total_bytes,
                },
                size: if v.total_bytes > 0 {
                    Some(v.total_bytes)
                } else {
                    None
                },
                modified: None,
            }
        })
        .collect())
}

pub fn mount_volume(target: &Path) -> Result<PathBuf> {
    // Prefer udisksctl; fall back to gio.
    if let Ok(out) = run_capture("udisksctl", &["mount", "-b", &target.display().to_string()]) {
        if let Some(p) = parse_udisks_mount_path(&out) {
            return Ok(p);
        }
    }
    let _ = run_ok("gio", &["mount", "-d", &target.display().to_string()])?;
    // Best-effort: find a new /media or /run/media mount containing the device name.
    find_mount_for_device(target).ok_or_else(|| anyhow::anyhow!("mounted, but path unknown"))
}

pub fn unmount_volume(target: &Path) -> Result<()> {
    if target.is_dir() {
        if run_ok("udisksctl", &["unmount", "-b", &device_for_mount(target)?]).is_ok() {
            return Ok(());
        }
        if run_ok("gio", &["mount", "-u", &target.display().to_string()]).is_ok() {
            return Ok(());
        }
        run_ok("umount", &[&target.display().to_string()])?;
        return Ok(());
    }
    if run_ok("udisksctl", &["unmount", "-b", &target.display().to_string()]).is_ok() {
        return Ok(());
    }
    run_ok("umount", &[&target.display().to_string()])?;
    Ok(())
}

pub fn eject_volume(target: &Path) -> Result<()> {
    let _ = unmount_volume(target);
    let dev = if target.is_dir() {
        device_for_mount(target).unwrap_or_else(|_| target.display().to_string())
    } else {
        target.display().to_string()
    };
    if run_ok("udisksctl", &["power-off", "-b", &dev]).is_ok() {
        return Ok(());
    }
    if run_ok("eject", &[&dev]).is_ok() {
        return Ok(());
    }
    run_ok("eject", &[&target.display().to_string()])?;
    Ok(())
}

pub fn set_volume_label(target: &Path, label: &str) -> Result<()> {
    let label = label.trim();
    if label.is_empty() {
        bail!("label cannot be empty");
    }
    let mount = if target.is_dir() {
        target.to_path_buf()
    } else {
        find_mount_for_device(target).unwrap_or_else(|| target.to_path_buf())
    };
    let fs = fs_type_of(&mount).unwrap_or_default();
    let dev = if target.starts_with("/dev") {
        target.display().to_string()
    } else {
        device_for_mount(&mount)?
    };
    match fs.as_str() {
        "vfat" | "msdos" | "fat" | "fat32" | "exfat" => {
            if run_ok("fatlabel", &[&dev, label]).is_ok() || run_ok("exfatlabel", &[&dev, label]).is_ok()
            {
                return Ok(());
            }
        }
        "ntfs" | "ntfs3" => {
            if run_ok("ntfslabel", &[&dev, label]).is_ok() {
                return Ok(());
            }
        }
        "ext2" | "ext3" | "ext4" => {
            if run_ok("tune2fs", &["-L", label, &dev]).is_ok() {
                return Ok(());
            }
        }
        _ => {}
    }
    // Last resort: udisksctl
    run_ok(
        "udisksctl",
        &["set-label", "-b", &dev, "--label", label],
    )?;
    Ok(())
}

/// Destructive — caller must confirm. Formats with udisksctl when available.
pub fn format_volume(target: &Path, fs: &str) -> Result<()> {
    let fs = match fs {
        "vfat" | "fat32" | "exfat" | "ext4" | "ntfs" => fs,
        _ => "exfat",
    };
    let dev = if target.starts_with("/dev") {
        target.display().to_string()
    } else if target.is_dir() {
        device_for_mount(target)?
    } else {
        target.display().to_string()
    };
    let _ = unmount_volume(Path::new(&dev));
    run_ok(
        "udisksctl",
        &["format", "-b", &dev, "--no-request-interaction", "-t", fs],
    )
    .with_context(|| format!("format {dev} as {fs}"))?;
    Ok(())
}

pub fn free_space(path: &Path) -> Option<u64> {
    disk_usage(path).ok().map(|(_, free)| free)
}

fn lsblk_unmounted(mounted_devs: &BTreeSet<String>) -> Vec<Volume> {
    let Ok(out) = run_capture(
        "lsblk",
        &[
            "-J",
            "-o",
            "NAME,PATH,TYPE,FSTYPE,LABEL,SIZE,MOUNTPOINT,RM,ROTA",
        ],
    ) else {
        return Vec::new();
    };
    let mut vols = Vec::new();
    parse_lsblk_devices(&out, mounted_devs, &mut vols);
    vols
}

fn parse_lsblk_devices(json: &str, mounted: &BTreeSet<String>, out: &mut Vec<Volume>) {
    // Minimal JSON walk without serde — look for block entries with PATH and empty MOUNTPOINT.
    // Prefer line-oriented fallback if JSON parse is too brittle.
    if let Ok(out_text) = run_capture(
        "lsblk",
        &[
            "-n",
            "-p",
            "-o",
            "PATH,TYPE,FSTYPE,LABEL,MOUNTPOINT,RM",
        ],
    ) {
        for line in out_text.lines() {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 3 {
                continue;
            }
            let path = cols[0];
            let ty = cols[1];
            if ty != "part" && ty != "disk" {
                continue;
            }
            // PATH TYPE FSTYPE LABEL MOUNTPOINT RM — LABEL/MOUNTPOINT may be empty.
            // Use lsblk -P for safer parsing.
            let _ = json;
        }
    }
    if let Ok(text) = run_capture(
        "lsblk",
        &["-n", "-p", "-P", "-o", "PATH,TYPE,FSTYPE,LABEL,MOUNTPOINT,RM"],
    ) {
        for line in text.lines() {
            let path = lsblk_field(line, "PATH");
            let ty = lsblk_field(line, "TYPE");
            let fstype = lsblk_field(line, "FSTYPE");
            let label = lsblk_field(line, "LABEL");
            let mount = lsblk_field(line, "MOUNTPOINT");
            let rm = lsblk_field(line, "RM");
            if path.is_empty() {
                continue;
            }
            if ty != "part" && !(ty == "disk" && !fstype.is_empty()) {
                continue;
            }
            if !mount.is_empty() || mounted.contains(&path) {
                continue;
            }
            if fstype.is_empty() && ty == "disk" {
                continue;
            }
            let kind = if rm == "1" {
                VolumeKind::Removable
            } else if fstype == "iso9660" || fstype == "udf" {
                VolumeKind::Optical
            } else {
                VolumeKind::Fixed
            };
            out.push(Volume {
                label: if label.is_empty() {
                    Path::new(&path)
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.clone())
                } else {
                    label
                },
                mount_path: None,
                device: Some(PathBuf::from(&path)),
                total_bytes: 0,
                free_bytes: 0,
                fs_type: if fstype.is_empty() {
                    "unknown".into()
                } else {
                    fstype
                },
                kind,
                mounted: false,
            });
        }
    }
}

fn lsblk_field(line: &str, key: &str) -> String {
    let needle = format!("{key}=\"");
    let Some(rest) = line.split(&needle).nth(1) else {
        return String::new();
    };
    rest.split('"').next().unwrap_or("").to_string()
}

fn collect_under(base: &Path, out: &mut BTreeMap<String, Volume>) {
    let Ok(read) = fs::read_dir(base) else {
        return;
    };
    for item in read.flatten() {
        let path = item.path();
        if !path.is_dir() {
            continue;
        }
        if base == Path::new("/run/media") {
            if let Ok(nested) = fs::read_dir(&path) {
                for child in nested.flatten() {
                    let child_path = child.path();
                    if child_path.is_dir() {
                        let name = child.file_name().to_string_lossy().to_string();
                        if let Ok(vol) = volume_at(
                            &child_path,
                            &format!("Removable ({name})"),
                            "removable",
                            VolumeKind::Removable,
                        ) {
                            out.entry(format!("mnt:{}", child_path.display()))
                                .or_insert(vol);
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
        if let Ok(vol) = volume_at(&path, &name, "mount", VolumeKind::Unknown) {
            out.entry(format!("mnt:{}", path.display())).or_insert(vol);
        }
    }
}

fn volume_at(path: &Path, label: &str, fs_type: &str, kind: VolumeKind) -> Result<Volume> {
    let (total, free) = disk_usage(path).unwrap_or((0, 0));
    Ok(Volume {
        label: label.to_string(),
        mount_path: Some(path.to_path_buf()),
        device: None,
        total_bytes: total,
        free_bytes: free,
        fs_type: fs_type.to_string(),
        kind,
        mounted: true,
    })
}

fn disk_usage(path: &Path) -> Result<(u64, u64)> {
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|e| anyhow::anyhow!(e))?;
    unsafe {
        let mut s: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut s) != 0 {
            bail!("statvfs failed for {}", path.display());
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

fn classify_mount(path: &Path, fs: &str, src: &str) -> VolumeKind {
    if fs == "cifs" || fs == "smb3" || fs == "nfs" || fs == "nfs4" || fs == "sshfs" {
        return VolumeKind::Network;
    }
    if fs == "iso9660" || fs == "udf" || src.contains("sr") {
        return VolumeKind::Optical;
    }
    if fs == "tmpfs" {
        return VolumeKind::Ram;
    }
    let s = path.to_string_lossy();
    if s.starts_with("/media") || s.starts_with("/run/media") || s.starts_with("/mnt") {
        return VolumeKind::Removable;
    }
    VolumeKind::Fixed
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

fn run_ok(bin: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("spawn {bin}"))?;
    if !status.success() {
        bail!("{bin} {} failed", args.join(" "));
    }
    Ok(())
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

fn parse_udisks_mount_path(out: &str) -> Option<PathBuf> {
    // "Mounted /dev/sdb1 at /run/media/user/LABEL."
    let lower = out.to_ascii_lowercase();
    let idx = lower.find(" at ")?;
    let rest = out[idx + 4..].trim();
    let path = rest.trim_end_matches('.').trim();
    if path.starts_with('/') {
        Some(PathBuf::from(path))
    } else {
        None
    }
}

fn find_mount_for_device(dev: &Path) -> Option<PathBuf> {
    let dev_s = dev.to_string_lossy();
    let text = fs::read_to_string("/proc/mounts").ok()?;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(src) = parts.next() else { continue };
        let Some(mount) = parts.next() else { continue };
        if src == dev_s || src.ends_with(dev_s.trim_start_matches("/dev/")) {
            return Some(PathBuf::from(unescape_mount(mount)));
        }
    }
    None
}

fn device_for_mount(mount: &Path) -> Result<String> {
    let text = fs::read_to_string("/proc/mounts")?;
    let mount_s = mount.to_string_lossy();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(src) = parts.next() else { continue };
        let Some(mnt) = parts.next() else { continue };
        if unescape_mount(mnt) == mount_s {
            return Ok(src.to_string());
        }
    }
    bail!("no device for mount {}", mount.display())
}

fn fs_type_of(mount: &Path) -> Option<String> {
    let text = fs::read_to_string("/proc/mounts").ok()?;
    let mount_s = mount.to_string_lossy();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let _src = parts.next()?;
        let mnt = parts.next()?;
        let fs = parts.next()?;
        if unescape_mount(mnt) == mount_s {
            return Some(fs.to_string());
        }
    }
    None
}
