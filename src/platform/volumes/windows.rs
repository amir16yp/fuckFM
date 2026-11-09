use crate::browser::{DirEntry, EntryKind};
use crate::platform::volumes::human_bytes;
use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Volume {
    pub label: String,
    pub path: PathBuf,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub fs_type: String,
    pub drive_kind: DriveKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriveKind {
    Fixed,
    Removable,
    Remote,
    Optical,
    RamDisk,
    Unknown,
}

impl Volume {
    pub fn display_name(&self) -> String {
        let kind = match self.drive_kind {
            DriveKind::Removable => "Removable Disk",
            DriveKind::Remote => "Network Drive",
            DriveKind::Optical => "CD Drive",
            DriveKind::RamDisk => "RAM Disk",
            DriveKind::Fixed | DriveKind::Unknown => "Local Disk",
        };
        let letter = self.path.display();
        let title = if self.label.is_empty() || self.label.eq_ignore_ascii_case(kind) {
            format!("{kind} ({letter})")
        } else {
            format!("{} ({letter})", self.label)
        };
        if self.total_bytes == 0 {
            return title;
        }
        format!(
            "{title} — {} free of {}",
            human_bytes(self.free_bytes),
            human_bytes(self.total_bytes)
        )
    }
}

pub fn this_pc_entries() -> Result<Vec<DirEntry>> {
    let mut entries = Vec::new();

    // Known folders first (Explorer-style This PC).
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

    for v in mounted_volumes()? {
        entries.push(DirEntry {
            name: v.display_name(),
            path: v.path.clone(),
            is_dir: true,
            kind: EntryKind::Drive {
                free_bytes: v.free_bytes,
                total_bytes: v.total_bytes,
            },
            size: Some(v.total_bytes),
            modified: None,
        });
    }
    Ok(entries)
}

fn mounted_volumes() -> Result<Vec<Volume>> {
    let mut out = Vec::new();
    for letter in b'A'..=b'Z' {
        let root = PathBuf::from(format!("{}:\\", letter as char));
        let kind = drive_kind(&root);
        if matches!(kind, DriveKind::Unknown) && !root.exists() {
            continue;
        }
        // Skip purely unknown absent roots; keep optical even if empty.
        if !root.exists() && !matches!(kind, DriveKind::Optical | DriveKind::Removable) {
            continue;
        }
        if matches!(kind, DriveKind::Unknown) {
            continue;
        }
        let (total, free) = disk_usage(&root).unwrap_or((0, 0));
        let (label, fs) = volume_info(&root);
        out.push(Volume {
            label,
            path: root,
            total_bytes: total,
            free_bytes: free,
            fs_type: fs,
            drive_kind: kind,
        });
    }
    Ok(out)
}

fn drive_kind(path: &Path) -> DriveKind {
    // GetDriveTypeW return codes (winbase.h / WindowsProgramming).
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;
    const DRIVE_RAMDISK: u32 = 6;
    let wide = wide_path(path);
    let ty = unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(wide.as_ptr()) };
    match ty {
        DRIVE_REMOVABLE => DriveKind::Removable,
        DRIVE_FIXED => DriveKind::Fixed,
        DRIVE_REMOTE => DriveKind::Remote,
        DRIVE_CDROM => DriveKind::Optical,
        DRIVE_RAMDISK => DriveKind::RamDisk,
        _ => DriveKind::Unknown,
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
        anyhow::bail!("GetDiskFreeSpaceExW failed for {}", path.display());
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

pub fn free_space(path: &Path) -> Option<u64> {
    let probe = drive_root(path).unwrap_or_else(|| path.to_path_buf());
    disk_usage(&probe)
        .or_else(|_| disk_usage(path))
        .ok()
        .map(|(_, free)| free)
        .filter(|&n| n > 0)
}

fn drive_root(path: &Path) -> Option<PathBuf> {
    let s = path.to_string_lossy();
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return Some(PathBuf::from(format!("{}:\\", bytes[0] as char)));
    }
    None
}

fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    let mut s: Vec<u16> = path.as_os_str().encode_wide().collect();
    if !s.ends_with(&[0]) {
        // Ensure trailing backslash for roots like C:
        if let Some(&last) = s.last() {
            if last != b'\\' as u16 && last != b'/' as u16 {
                // GetDiskFreeSpaceEx accepts "C:\" form.
            }
        }
        s.push(0);
    }
    s
}

fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}
