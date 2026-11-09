#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::{free_space, this_pc_entries};

#[cfg(windows)]
pub use windows::{free_space, this_pc_entries};

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
