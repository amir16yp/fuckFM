use crate::browser::{DirEntry, EntryKind};
use anyhow::{bail, Context, Result};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn trash_files_dir() -> PathBuf {
    home().join(".local/share/Trash/files")
}

fn trash_info_dir() -> PathBuf {
    home().join(".local/share/Trash/info")
}

pub fn trash_path(path: &Path) -> Result<PathBuf> {
    let files = trash_files_dir();
    let info = trash_info_dir();
    fs::create_dir_all(&files)?;
    fs::create_dir_all(&info)?;

    let name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("trash target has no name"))?
        .to_string_lossy()
        .to_string();
    let dest = unique_path(files.join(&name));
    let info_name = format!(
        "{}.trashinfo",
        dest.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&name)
    );

    let deletion_date = format_trash_date(SystemTime::now());
    let abs = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let body = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        escape_trash_path(&abs),
        deletion_date
    );
    fs::write(info.join(info_name), body)?;

    match fs::rename(path, &dest) {
        Ok(()) => Ok(dest),
        Err(err) if err.kind() == io::ErrorKind::CrossesDevices => {
            if path.is_dir() {
                copy_dir_all(path, &dest)?;
                remove_path(path)?;
            } else {
                fs::copy(path, &dest)?;
                fs::remove_file(path)?;
            }
            Ok(dest)
        }
        Err(err) => Err(err).with_context(|| format!("trash {}", path.display())),
    }
}

pub fn restore_from_trash(path: &Path) -> Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("trash item has no name"))?
        .to_string_lossy();
    let info_path = trash_info_dir().join(format!("{name}.trashinfo"));
    let original = if info_path.is_file() {
        parse_trashinfo_path(&info_path)?
    } else {
        // No trashinfo — restore beside Home/Desktop as last resort.
        home().join(name.as_ref())
    };

    if let Some(parent) = original.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create parent {}", parent.display()))?;
    }
    let dest = if original.exists() {
        unique_path(original)
    } else {
        original
    };

    match fs::rename(path, &dest) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::CrossesDevices => {
            if path.is_dir() {
                copy_dir_all(path, &dest)?;
                remove_path(path)?;
            } else {
                fs::copy(path, &dest)?;
                fs::remove_file(path)?;
            }
        }
        Err(err) => {
            return Err(err).with_context(|| {
                format!("restore {} -> {}", path.display(), dest.display())
            })
        }
    }
    let _ = fs::remove_file(info_path);
    Ok(())
}

pub fn delete_permanently(path: &Path) -> Result<()> {
    if let Some(name) = path.file_name() {
        let files = trash_files_dir();
        if path.starts_with(&files) {
            let info = trash_info_dir().join(format!("{}.trashinfo", name.to_string_lossy()));
            let _ = fs::remove_file(info);
        }
    }
    remove_path(path)
}

pub fn empty_trash() -> Result<()> {
    clear_dir_contents(&trash_files_dir())?;
    clear_dir_contents(&trash_info_dir())?;
    Ok(())
}

pub fn list_trash() -> Result<Vec<DirEntry>> {
    let files = trash_files_dir();
    if !files.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for item in fs::read_dir(&files).with_context(|| format!("read_dir {}", files.display()))? {
        let item = item?;
        let path = item.path();
        let name = item.file_name().to_string_lossy().to_string();
        let meta = item.metadata().ok();
        let is_dir = meta
            .as_ref()
            .map(|m| m.is_dir())
            .unwrap_or_else(|| item.file_type().map(|t| t.is_dir()).unwrap_or(false));
        let size = meta
            .as_ref()
            .and_then(|m| if m.is_file() { Some(m.len()) } else { None });
        let modified = meta.and_then(|m| m.modified().ok());
        entries.push(DirEntry {
            name,
            path,
            is_dir,
            kind: if is_dir {
                EntryKind::Folder
            } else {
                EntryKind::File
            },
            size,
            modified,
        });
    }
    Ok(entries)
}

fn parse_trashinfo_path(info_path: &Path) -> Result<PathBuf> {
    let body = fs::read_to_string(info_path)
        .with_context(|| format!("read {}", info_path.display()))?;
    for line in body.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Path=") {
            return Ok(PathBuf::from(unescape_trash_path(rest)));
        }
    }
    bail!("no Path= in {}", info_path.display())
}

fn clear_dir_contents(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for item in fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))? {
        let path = item?.path();
        remove_path(&path)?;
    }
    Ok(())
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn remove_path(path: &Path) -> Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
            .with_context(|| format!("remove_dir_all {}", path.display()))?;
    } else {
        fs::remove_file(path).with_context(|| format!("remove_file {}", path.display()))?;
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for item in fs::read_dir(src)? {
        let item = item?;
        let from = item.path();
        let to = dest.join(item.file_name());
        if from.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn unique_path(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "item".into());
    let ext = path
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    for i in 1..10_000 {
        let candidate = parent.join(format!("{stem} ({i}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!(
        "{stem}-{}.{}",
        std::process::id(),
        ext.trim_start_matches('.')
    ))
}

fn format_trash_date(time: SystemTime) -> String {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

fn escape_trash_path(path: &Path) -> String {
    path.to_string_lossy().replace(' ', "%20")
}

fn unescape_trash_path(s: &str) -> String {
    s.replace("%20", " ")
}
