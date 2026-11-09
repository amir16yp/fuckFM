use crate::ops::glob_match;
use crate::platform::network::{is_remote_uri, network_entries, normalize_uri};
use crate::platform::{free_space, human_bytes, list_trash, this_pc_entries, trash_label};
use anyhow::{Context, Result};
use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Virtual location marker used in history / side pane.
pub const THIS_PC: &str = "fuckfm:this-pc";
pub const TRASH: &str = "fuckfm:trash";
pub const NETWORK: &str = "fuckfm:network";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Folder,
    Drive { free_bytes: u64, total_bytes: u64 },
}

#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub kind: EntryKind,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Type,
    Size,
    Modified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    ThisPc,
    Trash,
    Network,
    /// Remote URI (`smb://…`) currently backed by a local mount path.
    Remote { uri: String, path: PathBuf },
    Path(PathBuf),
}

impl Location {
    pub fn display(&self) -> String {
        match self {
            Location::ThisPc => "This PC".into(),
            Location::Trash => trash_label().into(),
            Location::Network => "Network".into(),
            Location::Remote { uri, .. } => uri.clone(),
            Location::Path(p) => p.display().to_string(),
        }
    }

    pub fn session_key(&self) -> String {
        match self {
            Location::ThisPc => THIS_PC.into(),
            Location::Trash => TRASH.into(),
            Location::Network => NETWORK.into(),
            Location::Remote { uri, .. } => uri.clone(),
            Location::Path(p) => p.display().to_string(),
        }
    }
}

#[derive(Debug)]
pub struct Browser {
    pub location: Location,
    /// Visible entries (after name filter).
    pub entries: Vec<DirEntry>,
    /// Full listing before search filter.
    pub unfiltered: Vec<DirEntry>,
    /// Case-insensitive name filter (substring, or `*`/`?` glob when present).
    pub filter: String,
    /// When true, name filter also walks subfolders (Path locations only).
    pub recursive_search: bool,
    pub show_hidden: bool,
    pub sort_key: SortKey,
    pub sort_asc: bool,
    history: Vec<Location>,
    forward: Vec<Location>,
}

impl Browser {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let location = normalize_start(path.as_ref())?;
        let mut browser = Self {
            location,
            entries: Vec::new(),
            unfiltered: Vec::new(),
            filter: String::new(),
            recursive_search: false,
            show_hidden: false,
            sort_key: SortKey::Name,
            sort_asc: true,
            history: Vec::new(),
            forward: Vec::new(),
        };
        browser.refresh()?;
        Ok(browser)
    }

    pub fn open_this_pc() -> Result<Self> {
        let mut browser = Self {
            location: Location::ThisPc,
            entries: Vec::new(),
            unfiltered: Vec::new(),
            filter: String::new(),
            recursive_search: false,
            show_hidden: false,
            sort_key: SortKey::Name,
            sort_asc: true,
            history: Vec::new(),
            forward: Vec::new(),
        };
        browser.refresh()?;
        Ok(browser)
    }

    pub fn open_at(location: &Location) -> Result<Self> {
        match location {
            Location::ThisPc => Self::open_this_pc(),
            Location::Trash => {
                let mut browser = Self::open_this_pc()?;
                browser.goto_trash(false)?;
                Ok(browser)
            }
            Location::Network => {
                let mut browser = Self::open_this_pc()?;
                browser.goto_network(false, &[], &[])?;
                Ok(browser)
            }
            Location::Remote { uri, path } => {
                let mut browser = Self::open(path)?;
                browser.location = Location::Remote {
                    uri: uri.clone(),
                    path: path.clone(),
                };
                browser.refresh()?;
                Ok(browser)
            }
            Location::Path(path) => Self::open(path),
        }
    }

    pub fn cwd_path(&self) -> Option<&Path> {
        match &self.location {
            Location::Path(p) => Some(p.as_path()),
            Location::Remote { path, .. } => Some(path.as_path()),
            Location::ThisPc | Location::Trash | Location::Network => None,
        }
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.unfiltered = match &self.location {
            Location::ThisPc => this_pc_entries()?,
            Location::Trash => list_trash()?,
            Location::Network => network_entries(&[], &[])?,
            Location::Remote { path, .. } | Location::Path(path) => {
                read_dir_entries(path, self.show_hidden)?
            }
        };
        self.apply_filter_and_sort();
        Ok(())
    }

    /// Refresh Network view with recent + bookmark URIs from settings.
    pub fn refresh_network(&mut self, recent: &[String], bookmarks: &[String]) -> Result<()> {
        if matches!(self.location, Location::Network) {
            self.unfiltered = network_entries(recent, bookmarks)?;
            self.apply_filter_and_sort();
        } else {
            self.refresh()?;
        }
        Ok(())
    }

    pub fn set_filter(&mut self, query: impl Into<String>) {
        self.filter = query.into();
        self.apply_filter_and_sort();
    }

    pub fn clear_filter(&mut self) {
        if !self.filter.is_empty() {
            self.filter.clear();
            self.apply_filter_and_sort();
        }
    }

    pub fn set_recursive_search(&mut self, on: bool) {
        if self.recursive_search != on {
            self.recursive_search = on;
            self.apply_filter_and_sort();
        }
    }

    fn apply_filter_and_sort(&mut self) {
        if self.filter.is_empty() {
            self.entries = self.unfiltered.clone();
        } else if self.recursive_search {
            let cwd = match &self.location {
                Location::Path(p) => Some(p.clone()),
                Location::Remote { path, .. } => Some(path.clone()),
                _ => None,
            };
            if let Some(cwd) = cwd {
                let q = self.filter.clone();
                let hidden = self.show_hidden;
                self.entries = recursive_name_matches(&cwd, &q, hidden, 4_000);
            } else {
                self.entries = filter_entries(&self.unfiltered, &self.filter);
            }
        } else {
            self.entries = filter_entries(&self.unfiltered, &self.filter);
        }
        self.apply_sort();
    }

    pub fn toggle_hidden(&mut self) -> Result<()> {
        self.show_hidden = !self.show_hidden;
        self.refresh()
    }

    pub fn set_sort(&mut self, key: SortKey) {
        if self.sort_key == key {
            self.sort_asc = !self.sort_asc;
        } else {
            self.sort_key = key;
            self.sort_asc = true;
        }
        self.apply_sort();
    }

    fn apply_sort(&mut self) {
        let key = self.sort_key;
        let asc = self.sort_asc;
        self.entries.sort_by(|a, b| {
            // Folders first, except on This PC (all drives).
            let folder_ord = match (a.is_dir && !matches!(a.kind, EntryKind::Drive { .. }), b.is_dir && !matches!(b.kind, EntryKind::Drive { .. })) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => Ordering::Equal,
            };
            if folder_ord != Ordering::Equal && key == SortKey::Name {
                return if asc { folder_ord } else { folder_ord.reverse() };
            }
            let primary = match key {
                SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortKey::Type => entry_type_key(a).cmp(&entry_type_key(b)),
                SortKey::Size => a.size.unwrap_or(0).cmp(&b.size.unwrap_or(0)),
                SortKey::Modified => a.modified.cmp(&b.modified),
            };
            let primary = if asc { primary } else { primary.reverse() };
            primary.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
    }

    fn push_nav(&mut self, next: Location) {
        if next != self.location {
            self.history.push(self.location.clone());
            self.forward.clear();
            self.location = next;
        }
    }

    pub fn enter(&mut self, entry: &DirEntry) -> Result<bool> {
        let path_str = entry.path.to_string_lossy();
        if is_remote_uri(&path_str) || path_str.starts_with("smb://") {
            // Caller (app) should mount + goto_remote; treat as navigable marker.
            return Ok(false);
        }
        if !entry.is_dir {
            return Ok(false);
        }
        self.push_nav(Location::Path(entry.path.clone()));
        self.refresh()?;
        Ok(true)
    }

    pub fn up(&mut self) -> Result<bool> {
        match &self.location {
            Location::ThisPc => Ok(false),
            Location::Network => {
                self.push_nav(Location::ThisPc);
                self.refresh()?;
                Ok(true)
            }
            Location::Trash => {
                self.push_nav(Location::ThisPc);
                self.refresh()?;
                Ok(true)
            }
            Location::Remote { .. } => {
                self.push_nav(Location::Network);
                self.refresh()?;
                Ok(true)
            }
            Location::Path(cwd) => {
                if let Some(parent) = cwd.parent() {
                    let parent = parent.to_path_buf();
                    if parent != *cwd {
                        self.push_nav(Location::Path(parent));
                        self.refresh()?;
                        return Ok(true);
                    }
                }
                self.push_nav(Location::ThisPc);
                self.refresh()?;
                Ok(true)
            }
        }
    }

    pub fn back(&mut self) -> Result<bool> {
        if let Some(prev) = self.history.pop() {
            self.forward.push(self.location.clone());
            self.location = prev;
            self.refresh()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn forward(&mut self) -> Result<bool> {
        if let Some(next) = self.forward.pop() {
            self.history.push(self.location.clone());
            self.location = next;
            self.refresh()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn can_back(&self) -> bool {
        !self.history.is_empty()
    }

    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Back-stack labels, most recent first (index 0 = one Back).
    pub fn history_back_labels(&self) -> Vec<String> {
        self.history.iter().rev().map(|l| l.display()).collect()
    }

    /// Forward-stack labels, next destination first (index 0 = one Forward).
    pub fn history_forward_labels(&self) -> Vec<String> {
        self.forward.iter().rev().map(|l| l.display()).collect()
    }

    /// Jump `index` entries back (0 = previous location).
    pub fn jump_back(&mut self, index: usize) -> Result<bool> {
        if index >= self.history.len() {
            return Ok(false);
        }
        for _ in 0..=index {
            if !self.back()? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Jump `index` entries forward (0 = next location).
    pub fn jump_forward(&mut self, index: usize) -> Result<bool> {
        if index >= self.forward.len() {
            return Ok(false);
        }
        for _ in 0..=index {
            if !self.forward()? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn goto_this_pc(&mut self, push_history: bool) -> Result<()> {
        if push_history {
            self.push_nav(Location::ThisPc);
        } else {
            self.location = Location::ThisPc;
            self.forward.clear();
        }
        self.refresh()
    }

    pub fn goto_trash(&mut self, push_history: bool) -> Result<()> {
        if push_history {
            self.push_nav(Location::Trash);
        } else {
            self.location = Location::Trash;
            self.forward.clear();
        }
        self.refresh()
    }

    pub fn goto_network(
        &mut self,
        push_history: bool,
        recent: &[String],
        bookmarks: &[String],
    ) -> Result<()> {
        if push_history {
            self.push_nav(Location::Network);
        } else {
            self.location = Location::Network;
            self.forward.clear();
        }
        self.unfiltered = network_entries(recent, bookmarks)?;
        self.apply_filter_and_sort();
        Ok(())
    }

    pub fn goto_remote(&mut self, uri: &str, path: PathBuf, push_history: bool) -> Result<()> {
        let next = Location::Remote {
            uri: normalize_uri(uri),
            path,
        };
        if push_history {
            self.push_nav(next);
        } else {
            self.location = next;
            self.forward.clear();
        }
        self.refresh()
    }

    pub fn goto(&mut self, path: impl AsRef<Path>, push_history: bool) -> Result<()> {
        let path = path.as_ref();
        if path == Path::new(THIS_PC) {
            return self.goto_this_pc(push_history);
        }
        if path == Path::new(TRASH) {
            return self.goto_trash(push_history);
        }
        if path == Path::new(NETWORK) {
            return self.goto_network(push_history, &[], &[]);
        }
        let as_str = path.to_string_lossy();
        if is_remote_uri(&as_str) {
            anyhow::bail!("remote URI needs mount first: {as_str}");
        }
        let cwd = fs::canonicalize(path)
            .with_context(|| format!("canonicalize {}", path.display()))?;
        let next = Location::Path(cwd);
        if push_history {
            self.push_nav(next);
        } else {
            self.location = next;
            self.forward.clear();
        }
        self.refresh()
    }

    pub fn tab_title(&self) -> String {
        match &self.location {
            Location::ThisPc => "This PC".into(),
            Location::Trash => trash_label().into(),
            Location::Network => "Network".into(),
            Location::Remote { uri, .. } => uri.clone(),
            Location::Path(cwd) => cwd
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| cwd.display().to_string()),
        }
    }

    pub fn status_line(&self, selected: usize) -> String {
        self.status_line_with_size(selected, None)
    }

    /// `selected_bytes`: total size of the current selection when known.
    pub fn status_line_with_size(&self, selected: usize, selected_bytes: Option<u64>) -> String {
        let size_bit = selected_bytes
            .filter(|&_n| selected > 0)
            .map(|n| format!(" ({})", human_bytes(n)))
            .unwrap_or_default();
        match &self.location {
            Location::ThisPc => {
                let (free, total, drives) = drive_space_rollup(&self.entries);
                let mut s = if selected > 0 {
                    format!("{selected} selected{size_bit}  —  {drives} drive(s)")
                } else {
                    format!("{drives} drive(s) and devices")
                };
                if total > 0 {
                    let used = total.saturating_sub(free);
                    s.push_str(&format!(
                        "  —  {} used / {} total  —  {} free",
                        human_bytes(used),
                        human_bytes(total),
                        human_bytes(free)
                    ));
                }
                s
            }
            Location::Trash => {
                let dirs = self.entries.iter().filter(|e| e.is_dir).count();
                let files = self.entries.len() - dirs;
                if selected > 0 {
                    format!(
                        "{selected} selected{size_bit}  —  {} item(s) in trash",
                        self.entries.len()
                    )
                } else {
                    format!(
                        "{} item(s)  —  {} folder(s), {} file(s)",
                        self.entries.len(),
                        dirs,
                        files
                    )
                }
            }
            Location::Network => {
                if selected > 0 {
                    format!("{selected} selected  —  {} network place(s)", self.entries.len())
                } else {
                    format!("{} network place(s)", self.entries.len())
                }
            }
            Location::Remote { path, .. } | Location::Path(path) => {
                let dirs = self.entries.iter().filter(|e| e.is_dir).count();
                let files = self.entries.len() - dirs;
                let mut s = if selected > 0 {
                    format!(
                        "{selected} selected{size_bit}  —  {} item(s) in folder",
                        self.entries.len()
                    )
                } else {
                    format!(
                        "{} item(s)  —  {} folder(s), {} file(s)",
                        self.entries.len(),
                        dirs,
                        files
                    )
                };
                if let Some(free) = free_space(path) {
                    s.push_str(&format!("  —  {} free", human_bytes(free)));
                }
                s
            }
        }
    }
}

pub fn entry_type_label(entry: &DirEntry) -> String {
    match entry.kind {
        EntryKind::Drive { .. } => "Drive".into(),
        EntryKind::Folder => "Folder".into(),
        EntryKind::File => {
            let ext = Path::new(&entry.name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            if ext.is_empty() {
                "File".into()
            } else {
                format!("{} file", ext.to_ascii_uppercase())
            }
        }
    }
}

fn entry_type_key(entry: &DirEntry) -> String {
    entry_type_label(entry).to_lowercase()
}

fn normalize_start(path: &Path) -> Result<Location> {
    if path == Path::new(THIS_PC) {
        return Ok(Location::ThisPc);
    }
    if path == Path::new(TRASH) {
        return Ok(Location::Trash);
    }
    if path == Path::new(NETWORK) {
        return Ok(Location::Network);
    }
    let cwd = fs::canonicalize(path)
        .with_context(|| format!("canonicalize {}", path.display()))?;
    Ok(Location::Path(cwd))
}

fn read_dir_entries(cwd: &Path, show_hidden: bool) -> Result<Vec<DirEntry>> {
    let mut entries = Vec::new();
    let read = fs::read_dir(cwd).with_context(|| format!("read_dir {}", cwd.display()))?;
    for item in read {
        let item = item?;
        let path = item.path();
        let name = item.file_name().to_string_lossy().to_string();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let meta = item.metadata().ok();
        let is_dir = meta
            .as_ref()
            .map(|m| m.is_dir())
            .unwrap_or_else(|| item.file_type().map(|t| t.is_dir()).unwrap_or(false));
        let size = meta.as_ref().and_then(|m| {
            if m.is_file() {
                Some(m.len())
            } else {
                None
            }
        });
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

fn filter_entries(entries: &[DirEntry], filter: &str) -> Vec<DirEntry> {
    entries
        .iter()
        .filter(|e| entry_matches_query(e, filter))
        .cloned()
        .collect()
}

fn name_matches(filter: &str, name: &str) -> bool {
    if let Some((pat, insensitive)) = parse_slash_regex(filter) {
        return regex_name_matches(pat, insensitive, name);
    }
    if filter.contains('*') || filter.contains('?') {
        glob_match(filter, name)
    } else {
        name.to_lowercase().contains(&filter.to_lowercase())
    }
}

/// `/pattern/` or `/pattern/i` (case-insensitive). `/pattern/c` = case-sensitive.
fn parse_slash_regex(q: &str) -> Option<(&str, bool)> {
    let q = q.trim();
    if !q.starts_with('/') || q.len() < 2 {
        return None;
    }
    let rest = &q[1..];
    let end = rest.rfind('/')?;
    let pat = &rest[..end];
    let flags = &rest[end + 1..];
    if !flags
        .chars()
        .all(|c| matches!(c, 'i' | 'I' | 'c' | 'C'))
    {
        return None;
    }
    let lower = flags.to_ascii_lowercase();
    let insensitive = !lower.contains('c');
    Some((pat, insensitive))
}

fn regex_name_matches(pattern: &str, case_insensitive: bool, name: &str) -> bool {
    let Ok(re) = regex::RegexBuilder::new(pattern)
        .case_insensitive(case_insensitive)
        .build()
    else {
        return false;
    };
    re.is_match(name)
}

fn entry_matches_query(entry: &DirEntry, query: &str) -> bool {
    let q = query.trim();
    if q.is_empty() {
        return true;
    }
    // Whole-bar slash regex (may contain spaces inside `/…/`).
    if let Some((pat, insensitive)) = parse_slash_regex(q) {
        return regex_name_matches(pat, insensitive, &entry.name);
    }
    let advanced = q.contains("type:")
        || q.contains("ext:")
        || q.contains("size:")
        || q.contains("mtime:")
        || q.contains("modified:")
        || q.contains("regex:")
        || q.contains("re:");
    if !advanced {
        return name_matches(q, &entry.name);
    }

    let mut name_bits = Vec::new();
    let mut type_req: Option<&str> = None;
    let mut ext_req: Option<&str> = None;
    let mut size_req: Option<&str> = None;
    let mut mtime_req: Option<&str> = None;
    let mut regex_req: Option<(&str, bool)> = None;
    for token in q.split_whitespace() {
        if let Some(rest) = token
            .strip_prefix("regex:")
            .or_else(|| token.strip_prefix("re:"))
        {
            regex_req = Some((rest, true));
        } else if let Some(pair) = parse_slash_regex(token) {
            regex_req = Some(pair);
        } else if let Some(rest) = token.strip_prefix("type:") {
            type_req = Some(rest);
        } else if let Some(rest) = token.strip_prefix("ext:") {
            ext_req = Some(rest.trim_start_matches('.'));
        } else if let Some(rest) = token.strip_prefix("size:") {
            size_req = Some(rest);
        } else if let Some(rest) = token
            .strip_prefix("mtime:")
            .or_else(|| token.strip_prefix("modified:"))
        {
            mtime_req = Some(rest);
        } else {
            name_bits.push(token);
        }
    }
    if let Some(t) = type_req {
        if !type_matches(entry, t) {
            return false;
        }
    }
    if let Some(ext) = ext_req {
        let got = Path::new(&entry.name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !got.eq_ignore_ascii_case(ext) {
            return false;
        }
    }
    if let Some(sz) = size_req {
        if !size_matches(entry, sz) {
            return false;
        }
    }
    if let Some(mt) = mtime_req {
        if !mtime_matches(entry, mt) {
            return false;
        }
    }
    if let Some((pat, insensitive)) = regex_req {
        if !regex_name_matches(pat, insensitive, &entry.name) {
            return false;
        }
    }
    if name_bits.is_empty() {
        return true;
    }
    name_matches(&name_bits.join(" "), &entry.name)
}

/// Parse sizes like `12`, `12k`, `1.5M`, `2g` into bytes.
fn parse_size_token(raw: &str) -> Option<u64> {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    let (num, mult) = match s.chars().last() {
        Some('k') => (&s[..s.len() - 1], 1024u64),
        Some('m') => (&s[..s.len() - 1], 1024 * 1024),
        Some('g') => (&s[..s.len() - 1], 1024 * 1024 * 1024),
        Some('t') => (&s[..s.len() - 1], 1024u64.pow(4)),
        Some('b') => (&s[..s.len() - 1], 1),
        _ => (s.as_str(), 1),
    };
    if num.is_empty() {
        return None;
    }
    if let Ok(n) = num.parse::<u64>() {
        return Some(n.saturating_mul(mult));
    }
    if let Ok(f) = num.parse::<f64>() {
        return Some((f * mult as f64) as u64);
    }
    None
}

fn size_matches(entry: &DirEntry, spec: &str) -> bool {
    // Folders/drives: only match when explicitly empty size and looking for 0? Skip dirs for size.
    if entry.is_dir || matches!(entry.kind, EntryKind::Drive { .. }) {
        return false;
    }
    let Some(size) = entry.size else {
        return false;
    };
    let spec = spec.trim();
    if let Some(rest) = spec.strip_prefix(">=") {
        return parse_size_token(rest).is_some_and(|n| size >= n);
    }
    if let Some(rest) = spec.strip_prefix("<=") {
        return parse_size_token(rest).is_some_and(|n| size <= n);
    }
    if let Some(rest) = spec.strip_prefix('>') {
        return parse_size_token(rest).is_some_and(|n| size > n);
    }
    if let Some(rest) = spec.strip_prefix('<') {
        return parse_size_token(rest).is_some_and(|n| size < n);
    }
    if let Some((a, b)) = spec.split_once('-').or_else(|| spec.split_once("..")) {
        let (Some(lo), Some(hi)) = (parse_size_token(a), parse_size_token(b)) else {
            return false;
        };
        return size >= lo && size <= hi;
    }
    // Exact-ish: within 1% or exact for small values.
    parse_size_token(spec).is_some_and(|n| size == n)
}

fn mtime_matches(entry: &DirEntry, spec: &str) -> bool {
    use std::time::{Duration, SystemTime};
    let Some(mtime) = entry.modified else {
        return false;
    };
    let now = SystemTime::now();
    let spec = spec.trim().to_ascii_lowercase();
    let age_ok = |max_age: Duration| -> bool {
        match now.duration_since(mtime) {
            Ok(age) => age <= max_age,
            Err(_) => true, // future mtime counts as recent
        }
    };
    match spec.as_str() {
        "today" | "1d" => age_ok(Duration::from_secs(86_400)),
        "week" | "7d" => age_ok(Duration::from_secs(86_400 * 7)),
        "month" | "30d" => age_ok(Duration::from_secs(86_400 * 30)),
        "year" | "365d" => age_ok(Duration::from_secs(86_400 * 365)),
        _ => {
            if let Some(rest) = spec.strip_prefix(">=") {
                return mtime_cmp(mtime, rest, true, true);
            }
            if let Some(rest) = spec.strip_prefix("<=") {
                return mtime_cmp(mtime, rest, false, true);
            }
            if let Some(rest) = spec.strip_prefix('>') {
                return mtime_cmp(mtime, rest, true, false);
            }
            if let Some(rest) = spec.strip_prefix('<') {
                return mtime_cmp(mtime, rest, false, false);
            }
            // `mtime:7d` = within last 7 days
            if let Some(days) = spec.strip_suffix('d').and_then(|n| n.parse::<u64>().ok()) {
                return age_ok(Duration::from_secs(86_400 * days));
            }
            false
        }
    }
}

fn mtime_cmp(mtime: SystemTime, raw: &str, newer: bool, eq: bool) -> bool {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    // Relative ages: `7d`, `24h`
    let threshold = if let Some(days) = raw
        .trim()
        .strip_suffix('d')
        .and_then(|n| n.parse::<u64>().ok())
    {
        SystemTime::now()
            .checked_sub(Duration::from_secs(86_400 * days))
            .unwrap_or(UNIX_EPOCH)
    } else if let Some(hours) = raw
        .trim()
        .strip_suffix('h')
        .and_then(|n| n.parse::<u64>().ok())
    {
        SystemTime::now()
            .checked_sub(Duration::from_secs(3600 * hours))
            .unwrap_or(UNIX_EPOCH)
    } else {
        return false;
    };
    match mtime.duration_since(threshold) {
        Ok(d) => {
            if newer {
                eq || d.as_secs() > 0
            } else {
                false
            }
        }
        Err(err) => {
            if newer {
                false
            } else {
                eq || err.duration().as_secs() > 0
            }
        }
    }
}

fn type_matches(entry: &DirEntry, ty: &str) -> bool {
    let ty = ty.to_ascii_lowercase();
    match ty.as_str() {
        "folder" | "dir" | "directory" => entry.is_dir,
        "file" => !entry.is_dir && !matches!(entry.kind, EntryKind::Drive { .. }),
        "drive" => matches!(entry.kind, EntryKind::Drive { .. }),
        "image" | "img" | "photo" => {
            !entry.is_dir && ext_in(&entry.name, &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"])
        }
        "audio" | "music" => {
            !entry.is_dir && ext_in(&entry.name, &["mp3", "ogg", "flac", "wav", "m4a", "aac", "opus"])
        }
        "video" | "movie" => {
            !entry.is_dir && ext_in(&entry.name, &["mp4", "mkv", "webm", "avi", "mov", "wmv"])
        }
        "text" | "txt" | "code" => {
            !entry.is_dir
                && ext_in(
                    &entry.name,
                    &[
                        "txt", "md", "rs", "py", "js", "ts", "toml", "json", "yml", "yaml", "c",
                        "h", "cpp", "hpp", "go", "java", "sh", "css", "html",
                    ],
                )
        }
        other => {
            // `type:png` treated as extension shorthand.
            !entry.is_dir
                && Path::new(&entry.name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case(other))
        }
    }
}

fn ext_in(name: &str, exts: &[&str]) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Depth-first name matches under `root`, capped at `limit` results.
fn recursive_name_matches(
    root: &Path,
    filter: &str,
    show_hidden: bool,
    limit: usize,
) -> Vec<DirEntry> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if out.len() >= limit {
            break;
        }
        let Ok(read) = fs::read_dir(&dir) else {
            continue;
        };
        for item in read.flatten() {
            if out.len() >= limit {
                break;
            }
            let path = item.path();
            let name = item.file_name().to_string_lossy().to_string();
            if !show_hidden && name.starts_with('.') {
                continue;
            }
            let meta = item.metadata().ok();
            let is_dir = meta
                .as_ref()
                .map(|m| m.is_dir())
                .unwrap_or_else(|| path.is_dir());
            // Build a temp entry so type:/ext: queries work recursively too.
            let probe = DirEntry {
                name: name.clone(),
                path: path.clone(),
                is_dir,
                kind: if is_dir {
                    EntryKind::Folder
                } else {
                    EntryKind::File
                },
                size: None,
                modified: None,
            };
            if entry_matches_query(&probe, filter) {
                let rel = path
                    .strip_prefix(root)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| name.clone());
                let size = meta.as_ref().and_then(|m| m.is_file().then(|| m.len()));
                let modified = meta.and_then(|m| m.modified().ok());
                out.push(DirEntry {
                    name: rel,
                    path: path.clone(),
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
            if is_dir {
                // Skip noisy / huge trees.
                let s = path.to_string_lossy();
                if !(s.contains("/.git/")
                    || s.ends_with("/.git")
                    || s.contains("/node_modules/")
                    || s.ends_with("/node_modules")
                    || s.contains("/target/debug/")
                    || s.contains("/target/release/"))
                {
                    stack.push(path);
                }
            }
        }
    }
    out
}

/// Sum free/total across drives, skipping nested mounts that share the same capacity
/// fingerprint as a listed parent (avoids double-counting `/` + bind mounts).
fn drive_space_rollup(entries: &[DirEntry]) -> (u64, u64, usize) {
    let drives: Vec<(&Path, u64, u64)> = entries
        .iter()
        .filter_map(|e| match e.kind {
            EntryKind::Drive {
                free_bytes,
                total_bytes,
            } => Some((e.path.as_path(), free_bytes, total_bytes)),
            _ => None,
        })
        .collect();
    let mut free = 0u64;
    let mut total = 0u64;
    let mut count = 0usize;
    for (path, f, t) in &drives {
        let nested = drives.iter().any(|(other, of, ot)| {
            *other != *path
                && path.starts_with(other)
                && *ot == *t
                && *of == *f
                && other.components().count() < path.components().count()
        });
        if nested {
            continue;
        }
        free = free.saturating_add(*f);
        total = total.saturating_add(*t);
        count += 1;
    }
    (free, total, count)
}