use crate::browser::{DirEntry, EntryKind};
use crate::platform::volumes::human_bytes;
use anyhow::{bail, Context, Result};
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
        let kind = match self.kind {
            VolumeKind::Removable => "Removable Disk",
            VolumeKind::Network => "Network Drive",
            VolumeKind::Optical => "CD Drive",
            VolumeKind::Ram => "RAM Disk",
            VolumeKind::Fixed | VolumeKind::Unknown => "Local Disk",
        };
        let letter = self
            .mount_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let state = if self.mounted { "" } else { " [not mounted]" };
        let title = if self.label.is_empty() || self.label.eq_ignore_ascii_case(kind) {
            format!("{kind} ({letter})")
        } else {
            format!("{} ({letter})", self.label)
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
        self.mount_path.clone().or_else(|| self.device.clone())
    }
}

pub fn list_volumes() -> Result<Vec<Volume>> {
    let mut out = Vec::new();
    for letter in b'A'..=b'Z' {
        let root = PathBuf::from(format!("{}:\\", letter as char));
        let kind = drive_kind(&root);
        if matches!(kind, VolumeKind::Unknown) {
            continue;
        }
        let exists = root.exists();
        // Keep optical/removable letters even when empty/unmounted media.
        if !exists && !matches!(kind, VolumeKind::Optical | VolumeKind::Removable) {
            continue;
        }
        let (total, free) = if exists {
            disk_usage(&root).unwrap_or((0, 0))
        } else {
            (0, 0)
        };
        let (label, fs) = if exists {
            volume_info(&root)
        } else {
            (String::new(), "unknown".into())
        };
        out.push(Volume {
            label,
            mount_path: Some(root.clone()),
            device: Some(root.clone()),
            total_bytes: total,
            free_bytes: free,
            fs_type: fs,
            kind,
            mounted: exists,
        });
    }
    Ok(out)
}

pub fn this_pc_entries() -> Result<Vec<DirEntry>> {
    let mut entries = Vec::new();
    for (label, path) in crate::platform::folders::windows_this_pc_folders() {
        if path.is_dir() {
            entries.push(DirEntry {
                name: label,
                path,
                is_dir: true,
                kind: EntryKind::Folder,
                size: None,
                modified: None,
            });
        }
    }
    for v in list_volumes()? {
        let path = v
            .mount_path
            .clone()
            .unwrap_or_else(|| PathBuf::from(&v.label));
        entries.push(DirEntry {
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
        });
    }
    Ok(entries)
}

pub fn mount_volume(target: &Path) -> Result<PathBuf> {
    // Optical / removable letters “mount” by existing; try to refresh.
    if target.exists() {
        return Ok(target.to_path_buf());
    }
    // Network shares may need `net use`.
    let s = target.to_string_lossy();
    if s.starts_with("\\\\") {
        run_ok("net", &["use", &s])?;
        return Ok(target.to_path_buf());
    }
    bail!("nothing to mount for {}", target.display())
}

pub fn unmount_volume(target: &Path) -> Result<()> {
    let s = target.to_string_lossy();
    if s.starts_with("\\\\") {
        let _ = run_ok("net", &["use", &s, "/delete", "/y"]);
        return Ok(());
    }
    // Detach removable drive letter via PowerShell when possible.
    let letter = drive_letter(target).unwrap_or_else(|| s.chars().next().unwrap_or('?'));
    let script = format!(
        "$vol = Get-Volume -DriveLetter {letter} -ErrorAction SilentlyContinue; if ($vol) {{ $vol | Get-Disk | Get-Partition | Dismount-Partition -Confirm:$false }}"
    );
    let _ = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(())
}

pub fn eject_volume(target: &Path) -> Result<()> {
    let _ = unmount_volume(target);
    let letter = drive_letter(target).context("drive letter")?;
    let script = format!(
        "$vol = Get-Volume -DriveLetter {letter} -ErrorAction SilentlyContinue; if ($vol) {{ $disk = $vol | Get-Disk; $disk | ForEach-Object {{ $_.Number }} | ForEach-Object {{ (New-Object -ComObject Shell.Application).Namespace(17).ParseName(\"{letter}:\").InvokeVerb(\"Eject\") }} }}"
    );
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("eject")?;
    if !status.success() {
        // Fallback: mountvol /p (dismount + offline)
        let root = format!("{letter}:\\");
        let _ = run_ok("mountvol", &[&root, "/p"]);
    }
    Ok(())
}

pub fn set_volume_label(target: &Path, label: &str) -> Result<()> {
    let label = label.trim();
    if label.is_empty() {
        bail!("label cannot be empty");
    }
    let wide_root = wide_path(target);
    let wide_label: Vec<u16> = label
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::SetVolumeLabelW(
            wide_root.as_ptr(),
            wide_label.as_ptr(),
        )
    };
    if ok == 0 {
        bail!("SetVolumeLabelW failed for {}", target.display());
    }
    Ok(())
}

pub fn format_volume(target: &Path, fs: &str) -> Result<()> {
    let letter = drive_letter(target).context("drive letter for format")?;
    let fs = match fs.to_ascii_lowercase().as_str() {
        "ntfs" => "ntfs",
        "exfat" => "exfat",
        "fat32" | "vfat" => "fat32",
        _ => "exfat",
    };
    // format.com /FS:exfat /Q /Y X:
    run_ok(
        "cmd.exe",
        &[
            "/C",
            &format!("format {letter}: /FS:{fs} /Q /Y"),
        ],
    )
    .with_context(|| format!("format {letter}: as {fs}"))?;
    Ok(())
}

pub fn free_space(path: &Path) -> Option<u64> {
    let probe = drive_root(path).unwrap_or_else(|| path.to_path_buf());
    disk_usage(&probe)
        .or_else(|_| disk_usage(path))
        .ok()
        .map(|(_, free)| free)
        .filter(|&n| n > 0)
}

fn drive_kind(path: &Path) -> VolumeKind {
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;
    const DRIVE_RAMDISK: u32 = 6;
    let wide = wide_path(path);
    let ty = unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(wide.as_ptr()) };
    match ty {
        DRIVE_REMOVABLE => VolumeKind::Removable,
        DRIVE_FIXED => VolumeKind::Fixed,
        DRIVE_REMOTE => VolumeKind::Network,
        DRIVE_CDROM => VolumeKind::Optical,
        DRIVE_RAMDISK => VolumeKind::Ram,
        _ => VolumeKind::Unknown,
    }
}

fn disk_usage(path: &Path) -> Result<(u64, u64)> {
    let wide = wide_path(path);
    let mut free_to_caller: u64 = 0;
    let mut total: u64 = 0;
    let mut free_total: u64 = 0;
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_to_caller,
            &mut total,
            &mut free_total,
        )
    };
    if ok == 0 {
        bail!("GetDiskFreeSpaceExW failed for {}", path.display());
    }
    Ok((total, free_to_caller))
}

fn volume_info(path: &Path) -> (String, String) {
    let wide = wide_path(path);
    let mut label = vec![0u16; 261];
    let mut fs = vec![0u16; 64];
    let mut serial = 0u32;
    let mut max_comp = 0u32;
    let mut flags = 0u32;
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW(
            wide.as_ptr(),
            label.as_mut_ptr(),
            label.len() as u32,
            &mut serial,
            &mut max_comp,
            &mut flags,
            fs.as_mut_ptr(),
            fs.len() as u32,
        )
    };
    if ok == 0 {
        return (String::new(), "unknown".into());
    }
    (from_wide(&label), from_wide(&fs).to_ascii_lowercase())
}

fn drive_root(path: &Path) -> Option<PathBuf> {
    let s = path.to_string_lossy();
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return Some(PathBuf::from(format!("{}:\\", bytes[0] as char)));
    }
    None
}

fn drive_letter(path: &Path) -> Option<char> {
    path.to_string_lossy().chars().next().filter(|c| c.is_ascii_alphabetic())
}

fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    let mut s: Vec<u16> = path.as_os_str().encode_wide().collect();
    if !s.ends_with(&[0]) {
        s.push(0);
    }
    s
}

fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
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
