use anyhow::{bail, Context, Result};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipMode {
    Copy,
    Cut,
}

#[derive(Clone, Debug, Default)]
pub struct Clipboard {
    pub mode: Option<ClipMode>,
    pub paths: Vec<PathBuf>,
}

impl Clipboard {
    pub fn set(&mut self, mode: ClipMode, paths: Vec<PathBuf>) {
        self.mode = Some(mode);
        self.paths = paths;
    }

    pub fn clear(&mut self) {
        self.mode = None;
        self.paths.clear();
    }

}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictChoice {
    /// Keep both — write as `name (N).ext`.
    Rename,
    /// Overwrite the existing destination.
    Replace,
    /// Leave the destination alone.
    Skip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictPolicy {
    /// Always auto-rename (legacy default).
    Rename,
    Replace,
    Skip,
    /// Prompt per conflict (handled by UI).
    Ask,
}

impl ConflictPolicy {
    pub fn key(self) -> &'static str {
        match self {
            ConflictPolicy::Rename => "rename",
            ConflictPolicy::Replace => "replace",
            ConflictPolicy::Skip => "skip",
            ConflictPolicy::Ask => "ask",
        }
    }

    pub fn from_key(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "rename" => Some(ConflictPolicy::Rename),
            "replace" => Some(ConflictPolicy::Replace),
            "skip" => Some(ConflictPolicy::Skip),
            "ask" => Some(ConflictPolicy::Ask),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ConflictPolicy::Rename => "Auto-rename",
            ConflictPolicy::Replace => "Replace",
            ConflictPolicy::Skip => "Skip",
            ConflictPolicy::Ask => "Ask",
        }
    }

    pub fn next(self) -> Self {
        match self {
            ConflictPolicy::Rename => ConflictPolicy::Replace,
            ConflictPolicy::Replace => ConflictPolicy::Skip,
            ConflictPolicy::Skip => ConflictPolicy::Ask,
            ConflictPolicy::Ask => ConflictPolicy::Rename,
        }
    }

    pub fn as_choice(self) -> Option<ConflictChoice> {
        match self {
            ConflictPolicy::Rename => Some(ConflictChoice::Rename),
            ConflictPolicy::Replace => Some(ConflictChoice::Replace),
            ConflictPolicy::Skip => Some(ConflictChoice::Skip),
            ConflictPolicy::Ask => None,
        }
    }
}

/// Destination path if copied/moved with the original file name (may already exist).
pub fn intended_dest(src: &Path, dest_dir: &Path) -> Result<PathBuf> {
    let name = src
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("source has no name"))?;
    Ok(dest_dir.join(name))
}

pub fn copy_path(src: &Path, dest_dir: &Path) -> Result<PathBuf> {
    copy_path_with(src, dest_dir, ConflictChoice::Rename)?
        .ok_or_else(|| anyhow::anyhow!("copy skipped"))
}

pub fn move_path(src: &Path, dest_dir: &Path) -> Result<PathBuf> {
    move_path_with(src, dest_dir, ConflictChoice::Rename)?
        .ok_or_else(|| anyhow::anyhow!("move skipped"))
}

/// Copy with an explicit conflict choice. Returns `Ok(None)` when skipped.
pub fn copy_path_with(
    src: &Path,
    dest_dir: &Path,
    choice: ConflictChoice,
) -> Result<Option<PathBuf>> {
    let dest = resolve_dest(src, dest_dir, choice)?;
    let Some(dest) = dest else {
        return Ok(None);
    };
    if src.is_dir() {
        copy_dir_all(src, &dest)?;
    } else {
        fs::copy(src, &dest)
            .with_context(|| format!("copy {} -> {}", src.display(), dest.display()))?;
    }
    Ok(Some(dest))
}

/// Move with an explicit conflict choice. Returns `Ok(None)` when skipped.
pub fn move_path_with(
    src: &Path,
    dest_dir: &Path,
    choice: ConflictChoice,
) -> Result<Option<PathBuf>> {
    let dest = resolve_dest(src, dest_dir, choice)?;
    let Some(dest) = dest else {
        return Ok(None);
    };
    match fs::rename(src, &dest) {
        Ok(()) => Ok(Some(dest)),
        Err(err) if err.kind() == io::ErrorKind::CrossesDevices => {
            if src.is_dir() {
                copy_dir_all(src, &dest)?;
                remove_path(src)?;
            } else {
                fs::copy(src, &dest)?;
                fs::remove_file(src)?;
            }
            Ok(Some(dest))
        }
        Err(err) => Err(err).with_context(|| {
            format!("move {} -> {}", src.display(), dest.display())
        }),
    }
}

fn resolve_dest(src: &Path, dest_dir: &Path, choice: ConflictChoice) -> Result<Option<PathBuf>> {
    let intended = intended_dest(src, dest_dir)?;
    if !intended.exists() {
        return Ok(Some(intended));
    }
    // Same path (move onto itself) — treat as skip.
    if same_path_loose(src, &intended) {
        return Ok(None);
    }
    match choice {
        ConflictChoice::Skip => Ok(None),
        ConflictChoice::Rename => Ok(Some(unique_path(intended))),
        ConflictChoice::Replace => {
            remove_path(&intended)?;
            Ok(Some(intended))
        }
    }
}

fn same_path_loose(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

pub fn remove_path(path: &Path) -> Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
            .with_context(|| format!("remove_dir_all {}", path.display()))?;
    } else {
        fs::remove_file(path).with_context(|| format!("remove_file {}", path.display()))?;
    }
    Ok(())
}

pub fn create_folder(parent: &Path, name: &str) -> Result<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        bail!("folder name cannot be empty");
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        bail!("folder name cannot contain path separators");
    }
    let dest = unique_path(parent.join(name));
    fs::create_dir(&dest).with_context(|| format!("create_dir {}", dest.display()))?;
    Ok(dest)
}

pub fn create_empty_file(parent: &Path, name: &str) -> Result<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        bail!("file name cannot be empty");
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        bail!("file name cannot contain path separators");
    }
    let dest = unique_path(parent.join(name));
    fs::File::create(&dest).with_context(|| format!("create {}", dest.display()))?;
    Ok(dest)
}

/// Copy `src` into its parent as `name (N).ext` (or recurse for directories).
pub fn duplicate_path(src: &Path) -> Result<PathBuf> {
    let parent = src
        .parent()
        .ok_or_else(|| anyhow::anyhow!("no parent directory"))?;
    copy_path(src, parent)
}

/// Create a symlink (Unix) / directory-or-file symlink (Windows) next to `target`.
pub fn create_symlink(target: &Path) -> Result<PathBuf> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("no parent directory"))?;
    let name = target
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("target has no name"))?
        .to_string_lossy();
    let dest = unique_path(parent.join(format!("{name} - link")));
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, &dest)
            .with_context(|| format!("symlink {} -> {}", dest.display(), target.display()))?;
    }
    #[cfg(windows)]
    {
        if target.is_dir() {
            std::os::windows::fs::symlink_dir(target, &dest).with_context(|| {
                format!(
                    "symlink_dir {} -> {} (may need Developer Mode)",
                    dest.display(),
                    target.display()
                )
            })?;
        } else {
            std::os::windows::fs::symlink_file(target, &dest).with_context(|| {
                format!(
                    "symlink_file {} -> {} (may need Developer Mode)",
                    dest.display(),
                    target.display()
                )
            })?;
        }
    }
    Ok(dest)
}

/// Case-insensitive `*` / `?` glob match (Thunar-style select by pattern).
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let pat: Vec<char> = pattern.to_lowercase().chars().collect();
    let text: Vec<char> = name.to_lowercase().chars().collect();
    glob_match_rec(&pat, &text)
}

fn glob_match_rec(pat: &[char], text: &[char]) -> bool {
    let mut pi = 0usize;
    let mut ti = 0usize;
    let mut star_p = None;
    let mut star_t = 0usize;
    while ti < text.len() {
        if pi < pat.len() && (pat[pi] == '?' || pat[pi] == text[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < pat.len() && pat[pi] == '*' {
            star_p = Some(pi);
            star_t = ti;
            pi += 1;
        } else if let Some(sp) = star_p {
            pi = sp + 1;
            star_t += 1;
            ti = star_t;
        } else {
            return false;
        }
    }
    while pi < pat.len() && pat[pi] == '*' {
        pi += 1;
    }
    pi == pat.len()
}

pub fn rename_path(path: &Path, new_name: &str) -> Result<PathBuf> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        bail!("name cannot be empty");
    }
    if new_name.contains('/') || new_name.contains('\\') || new_name.contains('\0') {
        bail!("name cannot contain path separators");
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("no parent directory"))?;
    let dest = parent.join(new_name);
    if dest == path {
        return Ok(path.to_path_buf());
    }
    if dest.exists() {
        bail!("“{new_name}” already exists");
    }
    fs::rename(path, &dest)
        .with_context(|| format!("rename {} -> {}", path.display(), dest.display()))?;
    Ok(dest)
}

#[derive(Clone, Debug, Default)]
pub struct PasteResult {
    pub count: usize,
    pub mode: Option<ClipMode>,
    /// Destinations written (copy) or final locations (cut).
    pub destinations: Vec<PathBuf>,
    /// Original sources for cut (paired with `destinations`).
    pub sources: Vec<PathBuf>,
}

pub fn paste_clipboard(clip: &mut Clipboard, dest_dir: &Path) -> Result<PasteResult> {
    let mode = clip.mode.ok_or_else(|| anyhow::anyhow!("clipboard empty"))?;
    if clip.paths.is_empty() {
        bail!("clipboard empty");
    }
    let mut result = PasteResult {
        count: 0,
        mode: Some(mode),
        destinations: Vec::new(),
        sources: Vec::new(),
    };
    for src in clip.paths.clone() {
        if !src.exists() {
            continue;
        }
        match mode {
            ClipMode::Copy => {
                let dest = copy_path(&src, dest_dir)?;
                result.destinations.push(dest);
            }
            ClipMode::Cut => {
                let dest = move_path(&src, dest_dir)?;
                result.sources.push(src);
                result.destinations.push(dest);
            }
        }
        result.count += 1;
    }
    if mode == ClipMode::Cut {
        clip.clear();
    }
    Ok(result)
}

/// Build a `text/uri-list` body (one `file://` URI per line).
pub fn paths_as_uri_list(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| path_to_file_uri(p))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn path_to_file_uri(path: &Path) -> String {
    let abs = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let raw = abs.to_string_lossy();
    #[cfg(windows)]
    {
        let normalized = raw.replace('\\', "/");
        if normalized.starts_with("//") {
            format!("file:{normalized}")
        } else {
            format!("file:///{normalized}")
        }
    }
    #[cfg(not(windows))]
    {
        format!("file://{raw}")
    }
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
    parent.join(format!("{stem}-{}.{}", std::process::id(), ext.trim_start_matches('.')))
}

pub fn home_dir() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\"))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"))
    }
}

pub fn desktop_dir() -> PathBuf {
    let home = home_dir();
    let desktop = home.join("Desktop");
    if desktop.is_dir() {
        desktop
    } else {
        home
    }
}