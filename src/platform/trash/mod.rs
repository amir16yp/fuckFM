//! Trash / Recycle Bin: send, list, restore, empty, permanent delete.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use anyhow::Result;
use crate::browser::DirEntry;
use std::path::{Path, PathBuf};

/// Move `path` into the platform trash (XDG Trash / Recycle Bin).
/// Returns the path inside trash when known (XDG); `None` on Windows Recycle Bin.
pub fn trash_path(path: &Path) -> Result<Option<PathBuf>> {
    #[cfg(unix)]
    {
        Ok(Some(unix::trash_path(path)?))
    }
    #[cfg(windows)]
    {
        windows::trash_path(path)?;
        Ok(None)
    }
}

/// Restore a trashed item to its original location.
pub fn restore_from_trash(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        unix::restore_from_trash(path)
    }
    #[cfg(windows)]
    {
        windows::restore_from_trash(path)
    }
}

/// Permanently delete a path (bypass trash). Also cleans `.trashinfo` when under XDG trash.
pub fn delete_permanently(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        unix::delete_permanently(path)
    }
    #[cfg(windows)]
    {
        windows::delete_permanently(path)
    }
}

/// Empty the entire trash / Recycle Bin.
pub fn empty_trash() -> Result<()> {
    #[cfg(unix)]
    {
        unix::empty_trash()
    }
    #[cfg(windows)]
    {
        windows::empty_trash()
    }
}

/// Human label for the trash place ("Trash" / "Recycle Bin").
pub fn trash_label() -> &'static str {
    #[cfg(windows)]
    {
        "Recycle Bin"
    }
    #[cfg(not(windows))]
    {
        "Trash"
    }
}

/// List items currently in trash / Recycle Bin.
pub fn list_trash() -> Result<Vec<DirEntry>> {
    #[cfg(unix)]
    {
        unix::list_trash()
    }
    #[cfg(windows)]
    {
        windows::list_trash()
    }
}
