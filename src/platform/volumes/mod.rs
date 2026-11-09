#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::{
    eject_volume, format_volume, free_space, list_volumes, mount_volume, set_volume_label,
    this_pc_entries, unmount_volume, Volume, VolumeKind,
};

#[cfg(windows)]
pub use windows::{
    eject_volume, format_volume, free_space, list_volumes, mount_volume, set_volume_label,
    this_pc_entries, unmount_volume, Volume, VolumeKind,
};

use crate::browser::{DirEntry, EntryKind};

/// Format a byte count for UI (1024-based, Explorer-style KB/MB/… labels).
pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut val = n as f64;
    let mut idx = 0usize;
    while val >= 1024.0 && idx < UNITS.len() - 1 {
        val /= 1024.0;
        idx += 1;
    }
    let text = if val >= 100.0 {
        format!("{val:.0}")
    } else if val >= 10.0 {
        let t = format!("{val:.1}");
        t.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        let t = format!("{val:.2}");
        t.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    format!("{text} {}", UNITS[idx])
}

/// This PC rows for both mounted and unmounted volumes.
pub fn volume_dir_entries(include_unmounted: bool) -> anyhow::Result<Vec<DirEntry>> {
    let mut out = Vec::new();
    for v in list_volumes()? {
        if !include_unmounted && !v.mounted {
            continue;
        }
        let path = v
            .mount_path
            .clone()
            .or_else(|| v.device.clone())
            .unwrap_or_else(|| std::path::PathBuf::from(&v.label));
        out.push(DirEntry {
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
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::human_bytes;

    #[test]
    fn human_bytes_scales() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1 KB");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(1024 * 1024), "1 MB");
        assert_eq!(human_bytes(5 * 1024 * 1024 * 1024), "5 GB");
    }
}
