use crate::ffini::{Document, Value};
use crate::ops::ConflictPolicy;
use crate::theme::ThemeId;
use crate::ui::ViewMode;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// User preferences — persisted as `#.ffini` (binary INI).
#[derive(Clone, Debug)]
pub struct Settings {
    pub theme_id: ThemeId,
    /// 1..=4 — multiplies icon tile size in Icons view.
    pub icon_scale: u8,
    /// Optional `.fficons` pack path (empty = auto-discover).
    pub icon_pack: Option<PathBuf>,
    /// Optional wallpaper override. `None` = OS desktop wallpaper (Windows registry) when
    /// available, otherwise the procedural default for the active theme.
    pub wallpaper: Option<PathBuf>,
    /// When false, theme EffectSet bits (gloss, shadows, dim, sheen) are skipped.
    pub visual_effects: bool,
    /// Denser list/details row spacing (and slightly smaller UI font metrics).
    pub compact_view: bool,
    /// When true, draw classic framed chrome (title bar + outer bevel) inside the OS window.
    /// When false, fill the OS client area flatly so it does not look like a window-in-a-window.
    pub classic_window_chrome: bool,
    pub show_hidden_default: bool,
    pub preview_default: bool,
    /// Details / properties pane visible under the file list.
    pub details_pane_visible: bool,
    /// Side / navigation pane visible.
    pub side_pane_visible: bool,
    /// Restore previous tabs on startup (ignored when a path CLI arg is given).
    pub restore_tabs: bool,
    /// Startup location when session restore is off/empty: `this-pc` | `home` | `cwd`.
    pub startup_location: String,
    /// Open files/folders on a single click (Explorer “single-click to open”).
    pub single_click_open: bool,
    /// Ask before permanent delete / empty trash.
    pub confirm_delete: bool,
    /// Ask before completing a drag-and-drop move/copy/trash.
    pub confirm_drag: bool,
    /// How to handle name collisions on copy/move.
    pub conflict_policy: ConflictPolicy,
    /// Last session tab locations (`fuckfm:this-pc` / `fuckfm:trash` / absolute paths).
    pub session_tabs: Vec<String>,
    pub session_active: usize,
    /// Remembered view mode per location key (`fuckfm:this-pc` / path).
    pub folder_views: BTreeMap<String, ViewMode>,
    /// User-pinned bookmark folders (absolute paths).
    pub bookmarks: Vec<PathBuf>,
    /// Optional display labels for bookmarks (keyed by path string).
    pub bookmark_labels: BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme_id: ThemeId::Win98,
            icon_scale: 1,
            icon_pack: None,
            wallpaper: None,
            visual_effects: true,
            compact_view: false,
            classic_window_chrome: true,
            show_hidden_default: false,
            preview_default: false,
            details_pane_visible: false,
            side_pane_visible: true,
            restore_tabs: true,
            startup_location: "cwd".into(),
            single_click_open: false,
            confirm_delete: true,
            confirm_drag: false,
            conflict_policy: ConflictPolicy::Rename,
            session_tabs: Vec::new(),
            session_active: 0,
            folder_views: BTreeMap::new(),
            bookmarks: Vec::new(),
            bookmark_labels: BTreeMap::new(),
        }
    }
}

impl Settings {
    pub fn clamp_icon_scale(&mut self) {
        self.icon_scale = self.icon_scale.clamp(1, 4);
    }

    pub fn icon_scale_label(&self) -> String {
        format!("{}×", self.icon_scale)
    }

    /// Load from the platform config path, or defaults if missing/unreadable.
    pub fn load_or_default() -> Self {
        match Self::config_path().and_then(|p| Self::load_from(&p)) {
            Ok(s) => s,
            Err(_) => Self::default(),
        }
    }

    pub fn load_from(path: &std::path::Path) -> Result<Self> {
        let doc = Document::read_file(path)?;
        Ok(Self::from_document(&doc))
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        self.save_to(&path)
    }

    pub fn save_to(&self, path: &std::path::Path) -> Result<()> {
        self.to_document()
            .write_file(path)
            .with_context(|| format!("save settings {}", path.display()))
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(config_dir()?.join("settings.ffini"))
    }

    pub fn from_document(doc: &Document) -> Self {
        let mut s = Self::default();
        if let Some(name) = doc.get_str("appearance", "theme") {
            if let Some(id) = ThemeId::from_key(name) {
                s.theme_id = id;
            }
        }
        if let Some(scale) = doc.get_u64("appearance", "icon_scale") {
            s.icon_scale = scale.clamp(1, 4) as u8;
        }
        if let Some(path) = doc.get_str("appearance", "icon_pack") {
            if !path.is_empty() {
                s.icon_pack = Some(PathBuf::from(path));
            }
        }
        if let Some(path) = doc.get_str("appearance", "wallpaper") {
            if !path.is_empty() {
                s.wallpaper = Some(PathBuf::from(path));
            }
        }
        if let Some(v) = doc.get_bool("appearance", "visual_effects") {
            s.visual_effects = v;
        }
        if let Some(v) = doc.get_bool("appearance", "compact_view") {
            s.compact_view = v;
        }
        if let Some(v) = doc.get_bool("appearance", "classic_window_chrome") {
            s.classic_window_chrome = v;
        }
        if let Some(v) = doc.get_bool("defaults", "show_hidden") {
            s.show_hidden_default = v;
        }
        if let Some(v) = doc.get_bool("defaults", "preview_pane") {
            s.preview_default = v;
        }
        if let Some(v) = doc.get_bool("defaults", "details_pane") {
            s.details_pane_visible = v;
        }
        if let Some(v) = doc.get_bool("defaults", "side_pane") {
            s.side_pane_visible = v;
        }
        if let Some(v) = doc.get_bool("session", "restore_tabs") {
            s.restore_tabs = v;
        }
        if let Some(loc) = doc.get_str("defaults", "startup_location") {
            let loc = loc.trim();
            if matches!(loc, "this-pc" | "home" | "cwd") {
                s.startup_location = loc.to_string();
            }
        }
        if let Some(v) = doc.get_bool("defaults", "single_click_open") {
            s.single_click_open = v;
        }
        if let Some(v) = doc.get_bool("defaults", "confirm_delete") {
            s.confirm_delete = v;
        }
        if let Some(v) = doc.get_bool("defaults", "confirm_drag") {
            s.confirm_drag = v;
        }
        if let Some(raw) = doc.get_str("defaults", "conflict_policy") {
            if let Some(p) = ConflictPolicy::from_key(raw) {
                s.conflict_policy = p;
            }
        }
        if let Some(raw) = doc.get_str("session", "tabs") {
            s.session_tabs = raw
                .lines()
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(str::to_string)
                .collect();
        }
        if let Some(n) = doc.get_u64("session", "active") {
            s.session_active = n as usize;
        }
        if let Some(raw) = doc.get_str("views", "folders") {
            for line in raw.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Some((path, view)) = line.rsplit_once('\t') {
                    if let Some(mode) = ViewMode::from_key(view) {
                        s.folder_views.insert(path.to_string(), mode);
                    }
                }
            }
        }
        if let Some(raw) = doc.get_str("bookmarks", "paths") {
            for line in raw.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Some((path, label)) = line.split_once('\t') {
                    let path = PathBuf::from(path);
                    if !label.is_empty() {
                        s.bookmark_labels
                            .insert(path.display().to_string(), label.to_string());
                    }
                    s.bookmarks.push(path);
                } else {
                    s.bookmarks.push(PathBuf::from(line));
                }
            }
        }
        s.clamp_icon_scale();
        s
    }

    pub fn to_document(&self) -> Document {
        let mut doc = Document::new();
        doc.set(
            "appearance",
            "theme",
            Value::Str(self.theme_id.key().into()),
        );
        doc.set(
            "appearance",
            "icon_scale",
            Value::U64(self.icon_scale as u64),
        );
        if let Some(path) = &self.icon_pack {
            doc.set(
                "appearance",
                "icon_pack",
                Value::Str(path.display().to_string()),
            );
        }
        if let Some(path) = &self.wallpaper {
            doc.set(
                "appearance",
                "wallpaper",
                Value::Str(path.display().to_string()),
            );
        }
        doc.set(
            "appearance",
            "visual_effects",
            Value::Bool(self.visual_effects),
        );
        doc.set(
            "appearance",
            "compact_view",
            Value::Bool(self.compact_view),
        );
        doc.set(
            "appearance",
            "classic_window_chrome",
            Value::Bool(self.classic_window_chrome),
        );
        doc.set(
            "defaults",
            "show_hidden",
            Value::Bool(self.show_hidden_default),
        );
        doc.set(
            "defaults",
            "preview_pane",
            Value::Bool(self.preview_default),
        );
        doc.set(
            "defaults",
            "details_pane",
            Value::Bool(self.details_pane_visible),
        );
        doc.set(
            "defaults",
            "side_pane",
            Value::Bool(self.side_pane_visible),
        );
        doc.set(
            "session",
            "restore_tabs",
            Value::Bool(self.restore_tabs),
        );
        doc.set(
            "defaults",
            "startup_location",
            Value::Str(self.startup_location.clone()),
        );
        doc.set(
            "defaults",
            "single_click_open",
            Value::Bool(self.single_click_open),
        );
        doc.set(
            "defaults",
            "confirm_delete",
            Value::Bool(self.confirm_delete),
        );
        doc.set(
            "defaults",
            "confirm_drag",
            Value::Bool(self.confirm_drag),
        );
        doc.set(
            "defaults",
            "conflict_policy",
            Value::Str(self.conflict_policy.key().into()),
        );
        doc.set(
            "session",
            "tabs",
            Value::Str(self.session_tabs.join("\n")),
        );
        doc.set(
            "session",
            "active",
            Value::U64(self.session_active as u64),
        );
        let views = self
            .folder_views
            .iter()
            .map(|(p, v)| format!("{p}\t{}", v.key()))
            .collect::<Vec<_>>()
            .join("\n");
        doc.set("views", "folders", Value::Str(views));
        let paths = self
            .bookmarks
            .iter()
            .map(|p| {
                let key = p.display().to_string();
                if let Some(label) = self.bookmark_labels.get(&key) {
                    format!("{key}\t{label}")
                } else {
                    key
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        doc.set("bookmarks", "paths", Value::Str(paths));
        doc
    }

    pub fn view_for(&self, location_key: &str) -> Option<ViewMode> {
        self.folder_views.get(location_key).copied()
    }

    pub fn set_view_for(&mut self, location_key: String, view: ViewMode) {
        self.folder_views.insert(location_key, view);
    }

    pub fn startup_location_label(&self) -> &'static str {
        match self.startup_location.as_str() {
            "this-pc" => "This PC",
            "home" => "Home",
            _ => "Current folder",
        }
    }

    pub fn cycle_startup_location(&mut self) {
        self.startup_location = match self.startup_location.as_str() {
            "cwd" => "this-pc".into(),
            "this-pc" => "home".into(),
            _ => "cwd".into(),
        };
    }

    pub fn add_bookmark(&mut self, path: PathBuf) -> bool {
        if !path.is_absolute() {
            return false;
        }
        if self
            .bookmarks
            .iter()
            .any(|p| same_bookmark(p, &path))
        {
            return false;
        }
        self.bookmarks.push(path);
        true
    }

    pub fn remove_bookmark(&mut self, path: &std::path::Path) -> bool {
        let before = self.bookmarks.len();
        self.bookmarks.retain(|p| !same_bookmark(p, path));
        self.bookmark_labels
            .retain(|k, _| !same_bookmark(std::path::Path::new(k), path));
        self.bookmarks.len() != before
    }

    pub fn set_bookmark_label(&mut self, path: &std::path::Path, label: String) {
        let key = path.display().to_string();
        let trimmed = label.trim().to_string();
        if trimmed.is_empty() {
            self.bookmark_labels.remove(&key);
        } else {
            self.bookmark_labels.insert(key, trimmed);
        }
    }

    pub fn bookmark_label(&self, path: &std::path::Path) -> Option<&str> {
        self.bookmark_labels
            .get(&path.display().to_string())
            .map(|s| s.as_str())
    }

    pub fn move_bookmark(&mut self, from: usize, to: usize) -> bool {
        if from >= self.bookmarks.len() || to >= self.bookmarks.len() || from == to {
            return false;
        }
        let item = self.bookmarks.remove(from);
        self.bookmarks.insert(to, item);
        true
    }

    pub fn bookmark_index(&self, path: &std::path::Path) -> Option<usize> {
        self.bookmarks
            .iter()
            .position(|p| same_bookmark(p, path))
    }
}

fn same_bookmark(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Public for icon pack / theme discovery next to settings.
pub fn config_dir_public() -> Result<PathBuf> {
    config_dir()
}

fn config_dir() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(|p| PathBuf::from(p).join("AppData/Roaming"))
            })
            .context("APPDATA / USERPROFILE not set")?;
        Ok(base.join("fuckFM"))
    }
    #[cfg(not(windows))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .context("HOME / XDG_CONFIG_HOME not set")?;
        Ok(base.join("fuckfm"))
    }
}
