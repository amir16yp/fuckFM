use crate::browser::{Browser, Location, SortKey};
use crate::browser::{NETWORK, THIS_PC, TRASH};
use crate::iconpack::IconPack;
use crate::ops::{
    copy_path_with, create_empty_file, create_folder, create_hardlink, create_symlink,
    duplicate_path, glob_match, home_dir, intended_dest, move_path_with, paths_as_uri_list,
    rename_path, ClipMode, Clipboard, ConflictChoice, ConflictPolicy,
};
use crate::undo::{UndoOp, UndoStack};
use crate::platform::{
    attrs, build_context_menu, delete_permanently, detail_column_menu, empty_trash, open_default,
    open_with, open_with_dialog, restore_from_trash, run_action, set_default_app, tab_context_menu,
    trash_label, trash_path, ContextAction, ContextKind,
};
use crate::render::{FontAtlas, Frame, Rect};
use crate::settings::Settings;
use crate::theme::{theme, Theme};
use crate::ui::{
    ChromeHit, ConfirmAction, ConfirmDialog, ConfirmHit, ConflictDialog, ConflictHit, ContextMenu,
    CrumbTarget, DetailsPane, FileList, PlaceTarget, PreviewPane, ProgressDialog, ProgressHit,
    PromptDialog, PropertiesDialog, RenameDialog, SettingsDialog, SettingsHit, SidePane, TabHit,
    TabStrip, ThumbCache, WindowChrome,
};
use anyhow::{Context, Result};
use sdl2::clipboard::ClipboardUtil;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::{Keycode, Mod, TextInputUtil};
use sdl2::mouse::MouseButton;
use sdl2::EventPump;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Tab {
    browser: Browser,
    list: FileList,
    /// Inactive pane when dual-pane mode is on (swap with primary on focus change).
    other: Option<Box<(Browser, FileList)>>,
    /// When dual: focused (primary) pane is drawn on the right.
    focus_right: bool,
}

#[derive(Clone, Debug)]
enum DropTarget {
    None,
    Dir(PathBuf),
    Trash,
    /// Pin dragged folder(s) as side-pane bookmarks.
    Bookmark,
    /// Drop into another tab's folder, then switch to that tab.
    TabDir { index: usize, path: PathBuf },
}

struct DragState {
    paths: Vec<PathBuf>,
    origin: (i32, i32),
    active: bool,
    target: DropTarget,
}

struct TabReorderState {
    from: usize,
    origin: (i32, i32),
    active: bool,
}

struct BookmarkReorderState {
    /// Index into `settings.bookmarks`.
    from: usize,
    /// Path of the dragged bookmark (for identity if list rebuilds).
    path: PathBuf,
    hover: Option<usize>,
    origin: (i32, i32),
    active: bool,
}

struct ColumnResizeState {
    /// 0=Name|Type, 1=Type|Size, 2=Size|Modified
    which: usize,
    last_x: i32,
}

/// Stashed drag result while the confirm dialog is open.
struct PendingDrag {
    paths: Vec<PathBuf>,
    target: DropTarget,
    copy: bool,
}

struct TransferJob {
    remaining: Vec<PathBuf>,
    /// Top-level items at start (for progress denominator).
    total: usize,
    dest: PathBuf,
    copy: bool,
    apply_all: Option<ConflictChoice>,
    ok: usize,
    fail: usize,
    skip: usize,
    created: Vec<PathBuf>,
    moved: Vec<(PathBuf, PathBuf)>,
}

struct MarqueeState {
    origin: (i32, i32),
    current: (i32, i32),
    additive: bool,
    /// Selection before marquee started (for Ctrl additive).
    baseline: std::collections::BTreeSet<usize>,
}

impl MarqueeState {
    fn band(&self) -> Rect {
        let x0 = self.origin.0.min(self.current.0);
        let y0 = self.origin.1.min(self.current.1);
        let x1 = self.origin.0.max(self.current.0);
        let y1 = self.origin.1.max(self.current.1);
        Rect::new(x0, y0, (x1 - x0).max(1), (y1 - y0).max(1))
    }
}

impl Tab {
    fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_browser(Browser::open(path)?)
    }

    fn from_browser(browser: Browser) -> Result<Self> {
        Ok(Self {
            browser,
            list: FileList::new(Rect::default()),
            other: None,
            focus_right: false,
        })
    }

    fn after_nav(&mut self) {
        self.list.clear_selection();
        self.list.hover = None;
        self.list.scroll = 0;
    }

    fn dual_on(&self) -> bool {
        self.other.is_some()
    }

    fn toggle_dual(&mut self) -> Result<bool> {
        if self.other.take().is_some() {
            self.focus_right = false;
            return Ok(false);
        }
        let mut browser = Browser::open_at(&self.browser.location)?;
        browser.show_hidden = self.browser.show_hidden;
        browser.sort_key = self.browser.sort_key;
        browser.sort_asc = self.browser.sort_asc;
        browser.refresh()?;
        let mut list = FileList::new(Rect::default());
        list.view = self.list.view;
        list.icon_scale = self.list.icon_scale;
        list.col_name = self.list.col_name;
        list.col_type = self.list.col_type;
        list.col_size = self.list.col_size;
        list.col_modified = self.list.col_modified;
        list.show_type = self.list.show_type;
        list.show_size = self.list.show_size;
        list.show_modified = self.list.show_modified;
        list.detail_order = self.list.detail_order;
        self.other = Some(Box::new((browser, list)));
        self.focus_right = false;
        Ok(true)
    }

    fn focus_pane(&mut self, right: bool) {
        if self.other.is_none() || self.focus_right == right {
            return;
        }
        let mut other = self.other.take().expect("dual pane");
        std::mem::swap(&mut self.browser, &mut other.0);
        std::mem::swap(&mut self.list, &mut other.1);
        self.other = Some(other);
        self.focus_right = right;
    }
}

pub struct App {
    settings: Settings,
    tabs: Vec<Tab>,
    active: usize,
    chrome: WindowChrome,
    side: SidePane,
    strip: TabStrip,
    clipboard: Clipboard,
    undo: UndoStack,
    sys_clip: ClipboardUtil,
    context_menu: Option<ContextMenu>,
    rename: Option<RenameDialog>,
    pattern: Option<PromptDialog>,
    properties: PropertiesDialog,
    details: DetailsPane,
    confirm: ConfirmDialog,
    conflict: ConflictDialog,
    progress: ProgressDialog,
    transfer: Option<TransferJob>,
    pending_drag: Option<PendingDrag>,
    bookmark_label_edit: Option<PathBuf>,
    /// When set, `pattern` prompt commits a Unix chmod for this path.
    perms_edit: Option<PathBuf>,
    /// When set, `pattern` prompt renames a volume label.
    volume_label_edit: Option<PathBuf>,
    /// When true, `pattern` prompt is a remote URI connect dialog.
    connect_remote_prompt: bool,
    /// When set, `pattern` prompt expects `user:password` for this URI.
    remote_creds_uri: Option<String>,
    pending_format: Option<PathBuf>,
    pending_media_open: Option<PathBuf>,
    volumes_seeded: bool,
    seen_volume_keys: std::collections::BTreeSet<String>,
    mounted_volume_paths: std::collections::BTreeSet<String>,
    last_volume_poll: Instant,
    settings_dialog: SettingsDialog,
    preview: PreviewPane,
    thumbs: ThumbCache,
    icons: IconPack,
    last_click: Option<(usize, Instant)>,
    drag: Option<DragState>,
    marquee: Option<MarqueeState>,
    tab_reorder: Option<TabReorderState>,
    bookmark_reorder: Option<BookmarkReorderState>,
    column_resize: Option<ColumnResizeState>,
    typeahead: String,
    typeahead_at: Instant,
    last_view_loc: Option<String>,
    status_override: Option<String>,
    ctrl: bool,
    shift: bool,
    alt: bool,
    running: bool,
}

impl App {
    /// `cli_start`: optional path from argv. When absent and restore is enabled, reopen session tabs.
    pub fn new(window: Rect, cli_start: Option<PathBuf>, sys_clip: ClipboardUtil) -> Result<Self> {
        let settings = Settings::load_or_default();
        let icons = IconPack::load_for_theme(settings.theme_id, &settings.icon_pack);
        let (tabs, active) = open_startup_tabs(&settings, cli_start)?;
        let active = active.min(tabs.len().saturating_sub(1));
        let mut chrome = WindowChrome::new(window, "fuckFM");
        chrome.classic_framed = settings.classic_window_chrome;
        chrome.address = tabs[active].browser.location.display();
        chrome.status = tabs[active].browser.status_line(0);
        chrome.preview_visible = settings.preview_default;
        chrome.details_visible = settings.details_pane_visible;
        chrome.side_visible = settings.side_pane_visible;
        let mut preview = PreviewPane::new();
        preview.visible = settings.preview_default;
        let mut details = DetailsPane::new();
        details.visible = settings.details_pane_visible;
        let mut side = SidePane::new(Rect::default());
        side.visible = settings.side_pane_visible;
        side.rebuild(&settings.bookmarks, &settings.bookmark_labels);
        Ok(Self {
            settings,
            tabs,
            active,
            chrome,
            side,
            strip: TabStrip::new(Rect::default()),
            clipboard: Clipboard::default(),
            undo: UndoStack::default(),
            sys_clip,
            context_menu: None,
            rename: None,
            pattern: None,
            properties: PropertiesDialog::new(),
            details,
            confirm: ConfirmDialog::new(),
            conflict: ConflictDialog::new(),
            progress: ProgressDialog::new(),
            transfer: None,
            pending_drag: None,
            bookmark_label_edit: None,
            perms_edit: None,
            volume_label_edit: None,
            connect_remote_prompt: false,
            remote_creds_uri: None,
            pending_format: None,
            pending_media_open: None,
            volumes_seeded: false,
            seen_volume_keys: std::collections::BTreeSet::new(),
            mounted_volume_paths: std::collections::BTreeSet::new(),
            last_volume_poll: Instant::now(),
            settings_dialog: SettingsDialog::new(),
            preview,
            thumbs: ThumbCache::new(256),
            icons,
            last_click: None,
            drag: None,
            marquee: None,
            tab_reorder: None,
            bookmark_reorder: None,
            column_resize: None,
            typeahead: String::new(),
            typeahead_at: Instant::now(),
            last_view_loc: None,
            status_override: None,
            ctrl: false,
            shift: false,
            alt: false,
            running: true,
        })
    }

    fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    pub fn run(
        &mut self,
        canvas: &mut sdl2::render::Canvas<sdl2::video::Window>,
        fonts: &mut FontAtlas,
        events: &mut EventPump,
        text_input: &mut TextInputUtil,
    ) -> Result<()> {
        while self.running {
            // Keep SDL text input armed for type-ahead when no field owns focus.
            if self.rename.is_none()
                && self.pattern.is_none()
                && !self.chrome.address_focused
                && !self.chrome.search_focused
            {
                text_input.start();
            }
            for event in events.poll_iter() {
                self.handle_event(event, text_input)?;
            }
            if !self.running {
                self.save_session();
            }

            let theme = self.active_theme();
            let (w, h) = canvas.output_size().map_err(|e| anyhow::anyhow!(e))?;
            self.chrome.classic_framed = self.settings.classic_window_chrome;
            if self.settings.classic_window_chrome {
                self.chrome
                    .set_window(Rect::new(8, 8, w as i32 - 16, h as i32 - 16));
            } else {
                self.chrome.set_window(Rect::new(0, 0, w as i32, h as i32));
            }
            self.chrome.preview_visible = self.preview.visible;
            self.side.visible = self.settings.side_pane_visible;
            self.chrome.side_visible = self.side.visible;
            let location = self.tab().browser.location.clone();
            self.chrome
                .sync_path_crumbs(&location, theme.row_height);
            let layout = self.chrome.layout(
                theme.title_height,
                theme.toolbar_height,
                theme.status_height,
                theme.border_width,
            );
            self.strip.rect = layout.tabs;
            self.side.rect = layout.side;
            self.layout_dual_panes(layout.list);
            self.preview.rect = layout.preview;

            let titles: Vec<String> = self.tabs.iter().map(|t| t.browser.tab_title()).collect();
            self.strip.set_titles(titles, self.active);

            let loc_key = location_key(&location);
            if self.last_view_loc.as_deref() != Some(loc_key.as_str()) {
                self.apply_folder_view_for(&loc_key);
                self.last_view_loc = Some(loc_key);
            }
            let entry_count = self.tab().browser.entries.len();
            let selected_count = self.tab().list.selected.len();
            let selected_bytes = if selected_count == 0 {
                None
            } else {
                let mut total = 0u64;
                let mut any = false;
                for i in &self.tab().list.selected {
                    if let Some(sz) = self.tab().browser.entries.get(*i).and_then(|e| e.size) {
                        total += sz;
                        any = true;
                    }
                }
                any.then_some(total)
            };
            let status = self
                .status_override
                .clone()
                .unwrap_or_else(|| {
                    let mut s = self
                        .tab()
                        .browser
                        .status_line_with_size(selected_count, selected_bytes);
                    if !self.tab().browser.filter.is_empty() {
                        if self.tab().browser.recursive_search {
                            s.push_str(&format!(
                                "  —  recursive “{}” ({} matches)",
                                self.tab().browser.filter,
                                entry_count
                            ));
                        } else {
                            s.push_str(&format!(
                                "  —  filter “{}” ({}/{})",
                                self.tab().browser.filter,
                                entry_count,
                                self.tab().browser.unfiltered.len()
                            ));
                        }
                    }
                    if self.tab().browser.show_hidden {
                        s.push_str("  —  hidden on");
                    }
                    if self.tab().dual_on() {
                        s.push_str("  —  dual");
                    }
                    s
                });
            let address = location.display();
            let search = self.tab().browser.filter.clone();
            let entries = self.tab().browser.entries.clone();
            let sort_key = self.tab().browser.sort_key;
            let sort_asc = self.tab().browser.sort_asc;
            let other_entries = self
                .tab()
                .other
                .as_ref()
                .map(|o| (o.0.entries.clone(), o.0.sort_key, o.0.sort_asc));
            let preview_path = self
                .tab()
                .list
                .primary()
                .and_then(|i| self.tab().browser.entries.get(i))
                .map(|e| e.path.clone());

            self.side.sync_selection(&location);
            self.tab_mut()
                .list
                .clamp_scroll(entry_count, theme.row_height);
            if let Some(other) = self.tab_mut().other.as_mut() {
                other
                    .1
                    .clamp_scroll(other.0.entries.len(), theme.row_height);
            }
            self.preview.show_path(preview_path.as_deref());
            let selected_entries: Vec<_> = self
                .tab()
                .list
                .selected
                .iter()
                .filter_map(|i| self.tab().browser.entries.get(*i).cloned())
                .collect();
            let folder_label = location.display();
            self.details.sync(&selected_entries, &folder_label);

            self.chrome.title = format!("fuckFM — {}", self.settings.theme_id.label());
            if !self.chrome.address_focused {
                self.chrome.address = address;
            }
            if !self.chrome.search_focused {
                self.chrome.search = search;
            }
            self.chrome.status = status;
            self.chrome.view_label = self.tab().list.view.label().into();
            self.chrome.back_btn.enabled = self.tab().browser.can_back();
            self.chrome.forward_btn.enabled = self.tab().browser.can_forward();

            let mut frame = Frame {
                canvas,
                fonts,
                theme,
                effects: theme.effects.gated(self.settings.visual_effects),
            };
            if self.settings.classic_window_chrome {
                frame.clear_desktop();
            } else {
                frame.fill(
                    Rect::new(0, 0, w as i32, h as i32),
                    theme.window,
                );
            }
            self.chrome.draw(&mut frame)?;
            self.strip.draw(&mut frame)?;
            self.side.draw(&mut frame)?;
            if let (Some(other), Some((o_entries, o_key, o_asc))) =
                (self.tabs[self.active].other.as_mut(), other_entries.as_ref())
            {
                other.1.draw(
                    &mut frame,
                    o_entries,
                    *o_key,
                    *o_asc,
                    &mut self.thumbs,
                    &self.icons,
                )?;
            }
            self.tabs[self.active].list.draw(
                &mut frame,
                &entries,
                sort_key,
                sort_asc,
                &mut self.thumbs,
                &self.icons,
            )?;
            if self.tabs[self.active].dual_on() {
                frame.flat_border(self.tabs[self.active].list.rect, theme.selection);
            }
            self.preview.draw(&mut frame)?;
            self.details.draw(&mut frame)?;
            if let Some(m) = &self.marquee {
                let band = m.band();
                frame.fill(
                    band,
                    sdl2::pixels::Color::RGBA(
                        theme.selection.r,
                        theme.selection.g,
                        theme.selection.b,
                        60,
                    ),
                );
                frame.flat_border(band, theme.selection);
            }
            if let Some(menu) = &self.context_menu {
                menu.draw(&mut frame)?;
            }
            if let Some(rename) = &self.rename {
                rename.draw(&mut frame)?;
            }
            if let Some(pattern) = &self.pattern {
                pattern.draw(&mut frame)?;
            }
            let _ = self.properties.poll_size();
            self.properties.draw(&mut frame)?;
            self.confirm.draw(&mut frame)?;
            self.progress.draw(&mut frame)?;
            self.conflict.draw(&mut frame)?;
            self.settings_dialog.draw(&mut frame, &self.settings)?;
            frame.present();

            // Advance at most one transfer item per frame so the themed progress bar can paint.
            if self.transfer.is_some() && !self.conflict.open {
                let _ = self.pump_transfer_step();
            }
            let _ = self.poll_volumes();

            std::thread::sleep(Duration::from_millis(16));
        }
        Ok(())
    }

    fn handle_event(&mut self, event: Event, text_input: &mut TextInputUtil) -> Result<()> {
        match event {
            Event::Quit { .. } => self.running = false,
            Event::Window {
                win_event: WindowEvent::Resized(..) | WindowEvent::SizeChanged(..),
                ..
            } => {}
            Event::TextInput { text, .. } if self.rename.is_some() => {
                if let Some(rename) = &mut self.rename {
                    rename.insert(&text);
                }
            }
            Event::TextInput { text, .. } if self.pattern.is_some() => {
                if let Some(prompt) = &mut self.pattern {
                    prompt.insert(&text);
                }
            }
            Event::TextInput { text, .. } if self.chrome.address_focused => {
                self.insert_address(&text);
            }
            Event::TextInput { text, .. } if self.chrome.search_focused => {
                self.insert_search(&text);
            }
            Event::TextInput { text, .. }
                if self.rename.is_none()
                    && self.pattern.is_none()
                    && !self.chrome.address_focused
                    && !self.chrome.search_focused
                    && !self.settings_dialog.open
                    && !self.properties.open
                    && !self.confirm.open
                    && !self.progress.open
                    && !self.conflict.open =>
            {
                self.typeahead_append(&text);
            }
            Event::KeyDown {
                keycode: Some(key),
                keymod,
                ..
            } => {
                self.sync_mods(keymod);
                self.on_key(key, text_input)?;
            }
            Event::KeyUp {
                keycode: Some(_),
                keymod,
                ..
            } => self.sync_mods(keymod),
            Event::MouseMotion { x, y, .. } => {
                if self.rename.is_some()
                    || self.settings_dialog.open
                    || self.properties.open
                {
                    return Ok(());
                }
                let theme = self.active_theme();
                if let Some(menu) = &mut self.context_menu {
                    menu.hover = menu.index_at(x, y);
                    return Ok(());
                }
                if self.marquee.is_some() {
                    self.update_marquee(x, y);
                    return Ok(());
                }
                if self.tab_reorder.is_some() {
                    self.update_tab_reorder(x, y);
                    return Ok(());
                }
                if self.bookmark_reorder.is_some() {
                    self.update_bookmark_reorder(x, y);
                    return Ok(());
                }
                if let Some(resize) = self.column_resize.as_mut() {
                    let dx = x - resize.last_x;
                    if dx != 0 {
                        let which = resize.which;
                        resize.last_x = x;
                        let inner_w = self.tab().list.rect.shrink(2).w;
                        self.tab_mut().list.resize_column(which, dx, inner_w);
                    }
                    return Ok(());
                }
                self.strip.hover_at(x, y);
                self.side.hover = if self.side.rect.contains(x, y) {
                    self.side.index_at(y, theme.row_height + 2)
                } else {
                    None
                };
                let len = self.tab().browser.entries.len();
                self.tab_mut().list.hover = self
                    .tab()
                    .list
                    .index_at(x, y, theme.row_height)
                    .filter(|i| *i < len);
                self.update_drag(x, y);
            }
            Event::MouseWheel { y, .. } => {
                if self.settings_dialog.open {
                    return Ok(());
                }
                let theme = self.active_theme();
                let len = self.tab().browser.entries.len();
                let list = &mut self.tab_mut().list;
                list.scroll -= y;
                list.clamp_scroll(len, theme.row_height);
            }
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if self.conflict.open {
                    match self.conflict.hit(x, y) {
                        ConflictHit::ToggleAll => {
                            self.conflict.apply_all = !self.conflict.apply_all;
                        }
                        hit => {
                            if let Some(choice) = ConflictDialog::choice_from_hit(hit) {
                                self.resolve_conflict(choice)?;
                            } else if !self.conflict.rect.contains(x, y) {
                                self.cancel_transfer();
                            }
                        }
                    }
                } else if self.progress.open {
                    if self.progress.hit(x, y) == ProgressHit::Cancel
                        || !self.progress.rect.contains(x, y)
                    {
                        self.cancel_transfer();
                    }
                } else if self.confirm.open {
                    match self.confirm.hit(x, y) {
                        ConfirmHit::Yes => self.resolve_confirm(true)?,
                        ConfirmHit::No => self.resolve_confirm(false)?,
                        ConfirmHit::None => {
                            if !self.confirm.rect.contains(x, y) {
                                self.resolve_confirm(false)?;
                            }
                        }
                    }
                } else if self.properties.open {
                    let theme = self.active_theme();
                    if self.properties.hit_close(x, y, theme.title_height)
                        || !self.properties.rect.contains(x, y)
                    {
                        self.properties.hide();
                    }
                } else if self.settings_dialog.open {
                    self.on_settings_click(x, y);
                } else if self.rename.is_some() {
                    self.cancel_rename(text_input);
                } else if self.pattern.is_some() {
                    self.cancel_pattern(text_input);
                } else {
                    self.on_left_down(x, y, text_input)?;
                }
            }
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Right,
                x,
                y,
                ..
            } => {
                if self.rename.is_none() {
                    self.on_right_down(x, y)?;
                }
            }
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Middle,
                x,
                y,
                ..
            } => {
                self.context_menu = None;
                if let TabHit::Tab(i) = self.strip.hit(x, y) {
                    self.close_tab(i)?;
                } else {
                    self.middle_click_open_tab(x, y)?;
                }
            }
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => {
                self.chrome.back_btn.pressed = false;
                self.chrome.forward_btn.pressed = false;
                self.chrome.up_btn.pressed = false;
                self.chrome.settings_btn.pressed = false;
                self.chrome.view_btn.pressed = false;
                self.chrome.preview_btn.pressed = false;
                self.chrome.details_btn.pressed = false;
                self.chrome.close.pressed = false;
                self.chrome.new_tab_btn.pressed = false;
                if let Some(reorder) = self.tab_reorder.take() {
                    self.strip.drag_from = None;
                    self.strip.drop_hover = None;
                    if reorder.active {
                        self.finish_tab_reorder(reorder, x);
                    }
                } else if let Some(reorder) = self.bookmark_reorder.take() {
                    self.finish_bookmark_reorder(reorder, x, y)?;
                } else if self.column_resize.take().is_some() {
                    self.status_override = Some("Columns resized".into());
                } else if self.marquee.take().is_some() {
                    self.status_override = Some(format!(
                        "{} selected",
                        self.tab().list.selected.len()
                    ));
                } else if let Some(drag) = self.drag.take() {
                    self.strip.drop_hover = None;
                    if drag.active {
                        self.finish_drag(drag, x, y)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn sync_mods(&mut self, keymod: Mod) {
        self.ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
        self.shift = keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
        self.alt = keymod.intersects(Mod::LALTMOD | Mod::RALTMOD);
    }

    fn on_key(&mut self, key: Keycode, text_input: &mut TextInputUtil) -> Result<()> {
        if let Some(rename) = &mut self.rename {
            match key {
                Keycode::Escape => self.cancel_rename(text_input),
                Keycode::Return | Keycode::KpEnter => self.commit_rename(text_input)?,
                Keycode::Backspace => rename.backspace(),
                Keycode::Delete => rename.delete_forward(),
                Keycode::Left => rename.left(),
                Keycode::Right => rename.right(),
                _ => {}
            }
            return Ok(());
        }
        if let Some(prompt) = &mut self.pattern {
            match key {
                Keycode::Escape => self.cancel_pattern(text_input),
                Keycode::Return | Keycode::KpEnter => self.commit_pattern(text_input),
                Keycode::Backspace => prompt.backspace(),
                Keycode::Left => prompt.left(),
                Keycode::Right => prompt.right(),
                _ => {}
            }
            return Ok(());
        }
        if self.conflict.open {
            match key {
                Keycode::Escape => self.cancel_transfer(),
                Keycode::S => self.resolve_conflict(ConflictChoice::Skip)?,
                Keycode::R => self.resolve_conflict(ConflictChoice::Replace)?,
                Keycode::A | Keycode::N => self.resolve_conflict(ConflictChoice::Rename)?,
                Keycode::Tab => {
                    self.conflict.apply_all = !self.conflict.apply_all;
                }
                _ => {}
            }
            return Ok(());
        }
        if self.progress.open {
            if matches!(key, Keycode::Escape) {
                self.cancel_transfer();
            }
            return Ok(());
        }
        if self.confirm.open {
            match key {
                Keycode::Escape | Keycode::N => self.resolve_confirm(false)?,
                Keycode::Return | Keycode::KpEnter | Keycode::Y => self.resolve_confirm(true)?,
                _ => {}
            }
            return Ok(());
        }
        if self.settings_dialog.open {
            if key == Keycode::Escape {
                self.settings_dialog.hide();
            }
            return Ok(());
        }
        if self.chrome.address_focused {
            match key {
                Keycode::Escape => self.cancel_address(text_input),
                Keycode::Return | Keycode::KpEnter => self.commit_address(text_input)?,
                Keycode::Backspace => self.backspace_address(),
                _ => {}
            }
            return Ok(());
        }
        if self.chrome.search_focused {
            match key {
                Keycode::Escape => {
                    if self.tab().browser.filter.is_empty() {
                        self.blur_search(text_input);
                    } else {
                        self.clear_search(text_input);
                    }
                }
                Keycode::Return | Keycode::KpEnter => self.blur_search(text_input),
                Keycode::Backspace => self.backspace_search(),
                _ => {}
            }
            return Ok(());
        }
        if key == Keycode::Escape && self.properties.open {
            self.properties.hide();
            return Ok(());
        }
        if key == Keycode::Escape && self.context_menu.is_some() {
            self.context_menu = None;
            return Ok(());
        }
        if self.properties.open {
            return Ok(());
        }
        match (self.ctrl, key) {
            (true, Keycode::T) => self.new_tab(None)?,
            (true, Keycode::W) => self.close_tab(self.active)?,
            (true, Keycode::Tab) if self.shift => self.cycle_tab(-1),
            (true, Keycode::Tab) => self.cycle_tab(1),
            (true, Keycode::C) if self.shift => self.copy_paths_text(false),
            (true, Keycode::C) => self.clipboard_set(ClipMode::Copy),
            (true, Keycode::U) if self.shift => self.copy_paths_text(true),
            (true, Keycode::X) => self.clipboard_set(ClipMode::Cut),
            (true, Keycode::V) => self.clipboard_paste()?,
            (true, Keycode::Z) if self.shift => self.redo_op()?,
            (true, Keycode::Z) => self.undo_op()?,
            (true, Keycode::Y) => self.redo_op()?,
            (true, Keycode::L) => self.focus_address(text_input),
            (true, Keycode::F) if self.shift => self.focus_recursive_search(text_input),
            (true, Keycode::F) => self.focus_search(text_input),
            (true, Keycode::A) => {
                let len = self.tab().browser.entries.len();
                let list = &mut self.tab_mut().list;
                list.selected = (0..len).collect();
                list.anchor = if len > 0 { Some(0) } else { None };
            }
            (true, Keycode::I) => {
                let len = self.tab().browser.entries.len();
                self.tab_mut().list.invert_selection(len);
                self.status_override = Some(format!(
                    "Inverted — {} selected",
                    self.tab().list.selected.len()
                ));
            }
            (true, Keycode::D) => self.duplicate_selected()?,
            (true, Keycode::B) => self.add_bookmark_from_selection(),
            (true, Keycode::S) => self.begin_pattern_select(text_input),
            (true, Keycode::R) => self.refresh_view()?,
            (true, Keycode::N) if self.shift => self.new_folder(text_input)?,
            (true, Keycode::N) => self.open_new_window()?,
            (true, Keycode::H) => {
                self.tab_mut().browser.toggle_hidden()?;
                self.tab_mut().list.clear_selection();
                self.status_override = Some(if self.tab().browser.show_hidden {
                    "Hidden files shown".into()
                } else {
                    "Hidden files hidden".into()
                });
            }
            (true, Keycode::Num1)
            | (true, Keycode::Num2)
            | (true, Keycode::Num3)
            | (true, Keycode::Num4)
            | (true, Keycode::Num5)
            | (true, Keycode::Num6)
            | (true, Keycode::Num7)
            | (true, Keycode::Num8)
            | (true, Keycode::Num9) => {
                if let Some(n) = tab_number_from_key(key) {
                    self.jump_tab(n - 1);
                }
            }
            _ => match key {
                Keycode::Escape => self.running = false,
                Keycode::Backspace if !self.ctrl => {
                    self.tab_mut().browser.up()?;
                    self.tab_mut().after_nav();
                    self.status_override = None;
                }
                Keycode::F2 if !self.ctrl => self.begin_rename(text_input)?,
                Keycode::F3 if !self.ctrl => self.cycle_view(),
                Keycode::F4 if !self.ctrl => self.toggle_dual_pane()?,
                Keycode::F5 if !self.ctrl => self.refresh_view()?,
                Keycode::F9 if !self.ctrl => self.toggle_side_pane(),
                Keycode::P if self.alt && self.shift && !self.ctrl => self.toggle_details_pane(),
                Keycode::P if self.alt && !self.ctrl && !self.shift => self.toggle_preview(),
                Keycode::Up if self.alt => {
                    self.tab_mut().browser.up()?;
                    self.tab_mut().after_nav();
                    self.status_override = None;
                }
                Keycode::Left if self.alt => {
                    if self.tab_mut().browser.back()? {
                        self.tab_mut().after_nav();
                        self.status_override = None;
                    }
                }
                Keycode::Right if self.alt => {
                    if self.tab_mut().browser.forward()? {
                        self.tab_mut().after_nav();
                        self.status_override = None;
                    }
                }
                Keycode::Home if self.alt => {
                    let home = home_dir();
                    let _ = std::fs::create_dir_all(&home);
                    self.tab_mut().browser.goto(&home, true)?;
                    self.tab_mut().after_nav();
                    self.status_override = Some("Home".into());
                }
                Keycode::Up => self.move_selection_grid(0, -1, self.shift),
                Keycode::Down => self.move_selection_grid(0, 1, self.shift),
                Keycode::Left if !self.ctrl => self.move_selection_grid(-1, 0, self.shift),
                Keycode::Right if !self.ctrl => self.move_selection_grid(1, 0, self.shift),
                Keycode::Home => self.move_selection_to(0, self.shift),
                Keycode::End => {
                    let last = self.tab().browser.entries.len().saturating_sub(1);
                    self.move_selection_to(last, self.shift);
                }
                Keycode::PageUp => {
                    let theme = self.active_theme();
                    let page = self.tab().list.visible_rows(theme.row_height).max(1);
                    self.move_selection_grid(0, -page, self.shift);
                }
                Keycode::PageDown => {
                    let theme = self.active_theme();
                    let page = self.tab().list.visible_rows(theme.row_height).max(1);
                    self.move_selection_grid(0, page, self.shift);
                }
                Keycode::Space if !self.ctrl => self.toggle_focus_selection(),
                Keycode::Return if self.alt => self.show_properties(),
                Keycode::Return => self.activate_selected(text_input)?,
                Keycode::Delete if self.shift || self.in_trash() => {
                    self.delete_selected_permanently()?
                }
                Keycode::Delete => self.delete_selected()?,
                _ => {}
            },
        }
        Ok(())
    }

    fn in_trash(&self) -> bool {
        matches!(self.tab().browser.location, Location::Trash)
    }

    fn active_theme(&self) -> Theme {
        let mut t = theme(self.settings.theme_id);
        if self.settings.compact_view {
            t.row_height = (t.row_height - 4).max(14);
            t.font_size = (t.font_size - 1.0).max(11.0);
        }
        t
    }

    fn focus_address(&mut self, text_input: &mut TextInputUtil) {
        self.context_menu = None;
        self.drag = None;
        if self.chrome.search_focused {
            self.chrome.search_focused = false;
        }
        self.chrome.address_focused = true;
        self.chrome.address = self.tab().browser.location.display();
        text_input.start();
        self.status_override = Some("Address: Enter goes, Esc cancels".into());
    }

    fn cancel_address(&mut self, text_input: &mut TextInputUtil) {
        self.chrome.address_focused = false;
        self.chrome.address = self.tab().browser.location.display();
        text_input.stop();
        if self
            .status_override
            .as_deref()
            .is_some_and(|s| s.starts_with("Address:"))
        {
            self.status_override = None;
        }
    }

    fn insert_address(&mut self, text: &str) {
        let filtered: String = text.chars().filter(|c| *c != '\0').collect();
        if filtered.is_empty() {
            return;
        }
        self.chrome.address.push_str(&filtered);
    }

    fn backspace_address(&mut self) {
        let _ = self.chrome.address.pop();
    }

    fn commit_address(&mut self, text_input: &mut TextInputUtil) -> Result<()> {
        let raw = self.chrome.address.trim().to_string();
        self.chrome.address_focused = false;
        text_input.stop();
        if raw.is_empty() {
            self.chrome.address = self.tab().browser.location.display();
            self.status_override = Some("Empty address".into());
            return Ok(());
        }
        match resolve_address(&raw, self.tab().browser.cwd_path()) {
            Ok(target) => match target {
                AddressTarget::ThisPc => {
                    self.tab_mut().browser.goto_this_pc(true)?;
                    self.tab_mut().after_nav();
                    self.status_override = None;
                }
                AddressTarget::Trash => {
                    self.tab_mut().browser.goto_trash(true)?;
                    self.tab_mut().after_nav();
                    self.status_override = None;
                }
                AddressTarget::Network => {
                    self.goto_network_view()?;
                }
                AddressTarget::Remote(uri) => {
                    self.open_remote_uri(&uri, None, None, text_input)?;
                }
                AddressTarget::Path(path) => match self.tab_mut().browser.goto(&path, true) {
                    Ok(()) => {
                        self.tab_mut().after_nav();
                        self.status_override = None;
                    }
                    Err(err) => {
                        self.chrome.address = self.tab().browser.location.display();
                        self.status_override = Some(format!("Go failed: {err}"));
                    }
                },
            },
            Err(err) => {
                self.chrome.address = self.tab().browser.location.display();
                self.status_override = Some(format!("Bad path: {err}"));
            }
        }
        Ok(())
    }

    fn focus_search(&mut self, text_input: &mut TextInputUtil) {
        self.context_menu = None;
        self.drag = None;
        if self.chrome.address_focused {
            self.chrome.address_focused = false;
        }
        self.tab_mut().browser.set_recursive_search(false);
        self.chrome.search_focused = true;
        self.chrome.search = self.tab().browser.filter.clone();
        text_input.start();
        self.status_override =
            Some("Search current folder (Esc clears; Ctrl+Shift+F = subfolders)".into());
    }

    fn focus_recursive_search(&mut self, text_input: &mut TextInputUtil) {
        if self.tab().browser.cwd_path().is_none() {
            self.status_override = Some("Recursive search needs a folder".into());
            return;
        }
        self.context_menu = None;
        self.drag = None;
        if self.chrome.address_focused {
            self.chrome.address_focused = false;
        }
        self.tab_mut().browser.set_recursive_search(true);
        self.chrome.search_focused = true;
        self.chrome.search = self.tab().browser.filter.clone();
        text_input.start();
        self.status_override = Some("Search this folder + subfolders (Esc clears)".into());
    }

    fn undo_op(&mut self) -> Result<()> {
        match self.undo.undo() {
            Ok(Some(label)) => {
                self.tab_mut().browser.refresh()?;
                self.tab_mut().list.clear_selection();
                self.status_override = Some(format!("Undo {label}"));
            }
            Ok(None) => self.status_override = Some("Nothing to undo".into()),
            Err(err) => self.status_override = Some(format!("Undo failed: {err}")),
        }
        Ok(())
    }

    fn redo_op(&mut self) -> Result<()> {
        match self.undo.redo() {
            Ok(Some(label)) => {
                self.tab_mut().browser.refresh()?;
                self.tab_mut().list.clear_selection();
                self.status_override = Some(format!("Redo {label}"));
            }
            Ok(None) => self.status_override = Some("Nothing to redo".into()),
            Err(err) => self.status_override = Some(format!("Redo failed: {err}")),
        }
        Ok(())
    }

    fn search_in_folder(
        &mut self,
        text_input: &mut TextInputUtil,
        selected: &[PathBuf],
    ) -> Result<()> {
        if selected.len() == 1 && selected[0].is_dir() {
            self.tab_mut().browser.goto(&selected[0], true)?;
            self.tab_mut().after_nav();
        }
        self.tab_mut().browser.clear_filter();
        self.chrome.search.clear();
        self.focus_search(text_input);
        Ok(())
    }

    fn blur_search(&mut self, text_input: &mut TextInputUtil) {
        self.chrome.search_focused = false;
        text_input.stop();
        if self.status_override
            .as_deref()
            .is_some_and(|s| s.starts_with("Search "))
        {
            self.status_override = None;
        }
    }

    fn clear_search(&mut self, text_input: &mut TextInputUtil) {
        self.tab_mut().browser.clear_filter();
        self.tab_mut().browser.set_recursive_search(false);
        self.chrome.search.clear();
        self.tab_mut().list.clear_selection();
        self.tab_mut().list.scroll = 0;
        self.blur_search(text_input);
        self.status_override = Some("Search cleared".into());
    }

    fn insert_search(&mut self, text: &str) {
        let filtered: String = text.chars().filter(|c| *c != '\0').collect();
        if filtered.is_empty() {
            return;
        }
        let mut q = self.tab().browser.filter.clone();
        q.push_str(&filtered);
        self.tab_mut().browser.set_filter(q);
        self.chrome.search = self.tab().browser.filter.clone();
        self.tab_mut().list.clear_selection();
        self.tab_mut().list.scroll = 0;
    }

    fn backspace_search(&mut self) {
        let mut q = self.tab().browser.filter.clone();
        if q.pop().is_none() {
            return;
        }
        self.tab_mut().browser.set_filter(q);
        self.chrome.search = self.tab().browser.filter.clone();
        self.tab_mut().list.clear_selection();
        self.tab_mut().list.scroll = 0;
    }

    fn update_marquee(&mut self, x: i32, y: i32) {
        let Some(m) = self.marquee.as_mut() else {
            return;
        };
        m.current = (x, y);
        let band = m.band();
        let additive = m.additive;
        let baseline = m.baseline.clone();
        let theme = self.active_theme();
        let len = self.tab().browser.entries.len();
        let hit = self
            .tab()
            .list
            .indices_in_rect(band, len, theme.row_height);
        let list = &mut self.tab_mut().list;
        if additive {
            list.selected = baseline;
            for i in hit {
                list.selected.insert(i);
            }
        } else {
            list.selected = hit.into_iter().collect();
        }
        list.anchor = list.selected.iter().next().copied();
    }

    fn show_properties(&mut self) {
        let entries: Vec<_> = self
            .tab()
            .list
            .selected
            .iter()
            .filter_map(|i| self.tab().browser.entries.get(*i).cloned())
            .collect();
        if entries.is_empty() {
            self.status_override = Some("Nothing selected".into());
            return;
        }
        self.properties
            .show_entries(&entries, self.chrome.window);
        self.status_override = Some("Properties".into());
    }

    fn update_bookmark_reorder(&mut self, x: i32, y: i32) {
        let Some(state) = self.bookmark_reorder.as_mut() else {
            return;
        };
        if !state.active {
            let dx = x - state.origin.0;
            let dy = y - state.origin.1;
            if dx * dx + dy * dy < 25 {
                return;
            }
            state.active = true;
        }
        let theme = self.active_theme();
        let hover = self
            .side
            .index_at(y, theme.row_height + 2)
            .and_then(|place_i| {
                let place = self.side.places.get(place_i)?;
                if !place.bookmark {
                    return None;
                }
                match &place.target {
                    PlaceTarget::Path(p) => self.settings.bookmark_index(p),
                    _ => None,
                }
            });
        if let Some(state) = self.bookmark_reorder.as_mut() {
            state.hover = hover.or(Some(state.from));
            if state.active {
                self.status_override = Some("Reorder bookmark".into());
            }
        }
        let _ = x;
    }

    fn finish_bookmark_reorder(
        &mut self,
        state: BookmarkReorderState,
        _x: i32,
        y: i32,
    ) -> Result<()> {
        if !state.active {
            // Click without drag → navigate.
            let _ = std::fs::create_dir_all(&state.path);
            self.tab_mut().browser.goto(&state.path, true)?;
            self.tab_mut().after_nav();
            self.status_override = None;
            return Ok(());
        }
        let theme = self.active_theme();
        let to = self
            .side
            .index_at(y, theme.row_height + 2)
            .and_then(|place_i| {
                let place = self.side.places.get(place_i)?;
                if !place.bookmark {
                    return None;
                }
                match &place.target {
                    PlaceTarget::Path(p) => self.settings.bookmark_index(p),
                    _ => None,
                }
            })
            .or(state.hover)
            .unwrap_or(state.from);
        if self.settings.move_bookmark(state.from, to) {
            self.side.rebuild(&self.settings.bookmarks, &self.settings.bookmark_labels);
            self.persist_settings();
            self.status_override = Some("Bookmark reordered".into());
        } else {
            self.status_override = None;
        }
        Ok(())
    }

    fn send_selected_to(&mut self, dest: &Path) -> Result<()> {
        let paths = self.selected_paths();
        if paths.is_empty() {
            self.status_override = Some("Nothing selected".into());
            return Ok(());
        }
        if !dest.is_dir() {
            self.status_override = Some(format!("Send to failed: {} missing", dest.display()));
            return Ok(());
        }
        for path in &paths {
            if path == dest || dest.starts_with(path) {
                self.status_override = Some("Cannot send into itself".into());
                return Ok(());
            }
        }
        self.begin_transfer(paths, dest.to_path_buf(), false)
    }

    fn update_tab_reorder(&mut self, x: i32, y: i32) {
        let Some(state) = self.tab_reorder.as_mut() else {
            return;
        };
        if !state.active {
            let dx = x - state.origin.0;
            let dy = y - state.origin.1;
            if dx * dx + dy * dy < 36 {
                return;
            }
            state.active = true;
        }
        let to = self.strip.reorder_index_at(x);
        self.strip.drop_hover = Some(to);
        self.status_override = Some(format!("Move tab to position {}", to + 1));
        let _ = y;
    }

    fn finish_tab_reorder(&mut self, state: TabReorderState, x: i32) {
        let to = self.strip.reorder_index_at(x);
        let from = state.from;
        if from == to || from >= self.tabs.len() || to >= self.tabs.len() {
            self.status_override = None;
            return;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.active = to;
        self.status_override = Some(format!("Tab moved to position {}", to + 1));
        self.save_session();
    }

    fn update_drag(&mut self, x: i32, y: i32) {
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        if !drag.active {
            let dx = x - drag.origin.0;
            let dy = y - drag.origin.1;
            if dx * dx + dy * dy < 36 {
                return;
            }
            drag.active = true;
            self.last_click = None;
        }
        let paths = drag.paths.clone();
        let target = self.resolve_drop_target(x, y, &paths);
        self.strip.drop_hover = match &target {
            DropTarget::TabDir { index, .. } => Some(*index),
            _ => None,
        };
        if let Some(drag) = self.drag.as_mut() {
            drag.target = target.clone();
        }
        let verb = if self.ctrl { "Copy" } else { "Move" };
        self.status_override = Some(match &target {
            DropTarget::None => format!("Drag {verb}… (Ctrl=copy)"),
            DropTarget::Dir(p) => format!("{verb} to {}", p.display()),
            DropTarget::Trash => format!("Move to {}", trash_label()),
            DropTarget::Bookmark => "Add bookmark".into(),
            DropTarget::TabDir { index, path } => {
                format!("{verb} to tab {} ({})", index + 1, path.display())
            }
        });
    }

    fn resolve_drop_target(&self, x: i32, y: i32, paths: &[PathBuf]) -> DropTarget {
        let theme = self.active_theme();

        if let TabHit::Tab(index) = self.strip.hit(x, y) {
            match &self.tabs[index].browser.location {
                Location::Trash => return DropTarget::Trash,
                Location::Path(path) | Location::Remote { path, .. } => {
                    if path.is_dir() && !is_invalid_drop_dest(path, paths) {
                        return DropTarget::TabDir {
                            index,
                            path: path.clone(),
                        };
                    }
                }
                Location::ThisPc | Location::Network => {}
            }
        }

        if self.side.visible && self.side.rect.contains(x, y) {
            if let Some(index) = self.side.index_at(y, theme.row_height + 2) {
                match &self.side.places[index].target {
                    PlaceTarget::Trash => return DropTarget::Trash,
                    PlaceTarget::Path(path) => {
                        if path.is_dir() && !is_invalid_drop_dest(path, paths) {
                            return DropTarget::Dir(path.clone());
                        }
                    }
                    PlaceTarget::ThisPc | PlaceTarget::Network => {}
                }
            } else if paths.iter().any(|p| p.is_dir()) {
                // Empty side-pane space: pin folders as bookmarks.
                return DropTarget::Bookmark;
            }
        }

        let len = self.tab().browser.entries.len();
        if let Some(index) = self
            .tab()
            .list
            .index_at(x, y, theme.row_height)
            .filter(|i| *i < len)
        {
            if let Some(entry) = self.tab().browser.entries.get(index) {
                if entry.is_dir
                    && !paths.iter().any(|p| same_path_loose(p, &entry.path))
                    && !is_invalid_drop_dest(&entry.path, paths)
                {
                    return DropTarget::Dir(entry.path.clone());
                }
            }
        }
        DropTarget::None
    }

    fn finish_drag(&mut self, mut drag: DragState, x: i32, y: i32) -> Result<()> {
        drag.target = self.resolve_drop_target(x, y, &drag.paths);
        let copy = self.ctrl;
        if matches!(drag.target, DropTarget::None) {
            self.status_override = Some("Drop cancelled".into());
            return Ok(());
        }
        // Bookmarks are lightweight pins — never confirm.
        if matches!(drag.target, DropTarget::Bookmark) {
            return self.apply_drag_drop(drag.paths, drag.target, copy);
        }
        if self.settings.confirm_drag {
            let n = drag.paths.len();
            let msg = match &drag.target {
                DropTarget::Trash => {
                    format!("Send {n} item(s) to {}?", trash_label())
                }
                DropTarget::Dir(dest) | DropTarget::TabDir { path: dest, .. } => {
                    let verb = if copy { "Copy" } else { "Move" };
                    let name = dest
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| dest.display().to_string());
                    format!("{verb} {n} item(s) to “{name}”?")
                }
                DropTarget::None | DropTarget::Bookmark => unreachable!(),
            };
            self.pending_drag = Some(PendingDrag {
                paths: drag.paths,
                target: drag.target,
                copy,
            });
            self.confirm
                .show(self.chrome.window, ConfirmAction::DragDrop, msg);
            return Ok(());
        }
        self.apply_drag_drop(drag.paths, drag.target, copy)
    }

    fn apply_drag_drop(
        &mut self,
        paths: Vec<PathBuf>,
        target: DropTarget,
        copy: bool,
    ) -> Result<()> {
        match target {
            DropTarget::None => {
                self.status_override = Some("Drop cancelled".into());
            }
            DropTarget::Trash => {
                let mut ok = 0usize;
                let mut fail = 0usize;
                let mut items = Vec::new();
                for path in &paths {
                    match trash_path(path) {
                        Ok(Some(trashed)) => {
                            items.push((path.clone(), trashed));
                            ok += 1;
                        }
                        Ok(None) => ok += 1,
                        Err(_) => fail += 1,
                    }
                }
                self.undo.push(UndoOp::Trash { items });
                self.tab_mut().browser.refresh()?;
                self.tab_mut().after_nav();
                let bin = trash_label();
                self.status_override = Some(if fail == 0 {
                    format!("Moved {ok} item(s) to {bin}")
                } else {
                    format!("Sent {ok} to {bin}, failed {fail}")
                });
            }
            DropTarget::Bookmark => {
                let mut ok = 0usize;
                for path in &paths {
                    if path.is_dir() {
                        let path = path.canonicalize().unwrap_or_else(|_| path.clone());
                        if self.settings.add_bookmark(path) {
                            ok += 1;
                        }
                    }
                }
                self.side
                    .rebuild(&self.settings.bookmarks, &self.settings.bookmark_labels);
                self.persist_settings();
                self.status_override = Some(if ok == 0 {
                    "No new bookmarks".into()
                } else {
                    format!("Bookmarked {ok} folder(s)")
                });
            }
            DropTarget::Dir(dest) => {
                self.finish_copy_move(&paths, &dest, copy)?;
            }
            DropTarget::TabDir { index, path } => {
                self.finish_copy_move(&paths, &path, copy)?;
                if index < self.tabs.len() {
                    self.active = index;
                    self.tab_mut().browser.refresh()?;
                    self.tab_mut().after_nav();
                }
            }
        }
        Ok(())
    }

    fn finish_copy_move(&mut self, paths: &[PathBuf], dest: &Path, copy: bool) -> Result<()> {
        self.begin_transfer(paths.to_vec(), dest.to_path_buf(), copy)
    }

    fn on_left_down(
        &mut self,
        x: i32,
        y: i32,
        text_input: &mut TextInputUtil,
    ) -> Result<()> {
        if let Some(menu) = &self.context_menu {
            if let Some(idx) = menu.index_at(x, y) {
                let action = menu.items[idx].action.clone();
                self.context_menu = None;
                self.run_context_action(action, text_input)?;
                return Ok(());
            }
            self.context_menu = None;
            // fall through so click still acts underneath
        }

        let ctrl = self.ctrl;
        let shift = self.shift;

        match self.chrome.hit(x, y) {
            ChromeHit::Close => {
                self.chrome.close.pressed = true;
                self.running = false;
                return Ok(());
            }
            ChromeHit::Settings => {
                self.chrome.settings_btn.pressed = true;
                self.settings_dialog.show(self.chrome.window);
                return Ok(());
            }
            ChromeHit::ViewCycle => {
                self.chrome.view_btn.pressed = true;
                self.cycle_view();
                return Ok(());
            }
            ChromeHit::PreviewToggle => {
                self.chrome.preview_btn.pressed = true;
                self.toggle_preview();
                return Ok(());
            }
            ChromeHit::DetailsToggle => {
                self.chrome.details_btn.pressed = true;
                self.toggle_details_pane();
                return Ok(());
            }
            ChromeHit::Up => {
                self.chrome.up_btn.pressed = true;
                self.tab_mut().browser.up()?;
                self.tab_mut().after_nav();
                self.status_override = None;
                return Ok(());
            }
            ChromeHit::Back => {
                self.chrome.back_btn.pressed = true;
                self.tab_mut().browser.back()?;
                self.tab_mut().after_nav();
                self.status_override = None;
                return Ok(());
            }
            ChromeHit::BackMenu => {
                self.chrome.back_menu_btn.pressed = true;
                self.open_nav_history_menu(true, x, y);
                return Ok(());
            }
            ChromeHit::Forward => {
                self.chrome.forward_btn.pressed = true;
                self.tab_mut().browser.forward()?;
                self.tab_mut().after_nav();
                self.status_override = None;
                return Ok(());
            }
            ChromeHit::ForwardMenu => {
                self.chrome.forward_menu_btn.pressed = true;
                self.open_nav_history_menu(false, x, y);
                return Ok(());
            }
            ChromeHit::NewTab => {
                self.chrome.new_tab_btn.pressed = true;
                self.new_tab(None)?;
                return Ok(());
            }
            ChromeHit::Address => {
                self.focus_address(text_input);
                return Ok(());
            }
            ChromeHit::Search => {
                self.focus_search(text_input);
                return Ok(());
            }
            ChromeHit::PathCrumb(i) => {
                if let Some(crumb) = self.chrome.crumbs.get(i).cloned() {
                    match crumb.target {
                        CrumbTarget::ThisPc => {
                            self.tab_mut().browser.goto_this_pc(true)?;
                        }
                        CrumbTarget::Trash => {
                            self.tab_mut().browser.goto_trash(true)?;
                        }
                        CrumbTarget::Network => {
                            self.goto_network_view()?;
                            return Ok(());
                        }
                        CrumbTarget::Path(path) => {
                            self.tab_mut().browser.goto(&path, true)?;
                        }
                    }
                    self.tab_mut().after_nav();
                    self.status_override = None;
                }
                return Ok(());
            }
            ChromeHit::None => {
                if self.chrome.address_focused {
                    self.cancel_address(text_input);
                }
                if self.chrome.search_focused {
                    self.blur_search(text_input);
                }
            }
        }

        match self.strip.hit(x, y) {
            TabHit::New => {
                self.new_tab(None)?;
                return Ok(());
            }
            TabHit::Tab(i) => {
                self.active = i;
                self.tab_reorder = Some(TabReorderState {
                    from: i,
                    origin: (x, y),
                    active: false,
                });
                self.strip.drag_from = Some(i);
                self.status_override = None;
                return Ok(());
            }
            TabHit::None => {}
        }

        let theme = self.active_theme();
        if let Some(index) = self.side.index_at(y, theme.row_height + 2) {
            if self.side.rect.contains(x, y) {
                let place = &self.side.places[index];
                if place.bookmark {
                    if let PlaceTarget::Path(path) = &place.target {
                        if let Some(bm) = self.settings.bookmark_index(path) {
                            self.bookmark_reorder = Some(BookmarkReorderState {
                                from: bm,
                                path: path.clone(),
                                hover: Some(bm),
                                origin: (x, y),
                                active: false,
                            });
                            self.status_override = None;
                            return Ok(());
                        }
                    }
                }
                match place.target.clone() {
                    PlaceTarget::ThisPc => {
                        self.tab_mut().browser.goto_this_pc(true)?;
                    }
                    PlaceTarget::Trash => {
                        self.tab_mut().browser.goto_trash(true)?;
                    }
                    PlaceTarget::Network => {
                        self.goto_network_view()?;
                        return Ok(());
                    }
                    PlaceTarget::Path(path) => {
                        let _ = std::fs::create_dir_all(&path);
                        self.tab_mut().browser.goto(&path, true)?;
                    }
                }
                self.tab_mut().after_nav();
                self.status_override = None;
                return Ok(());
            }
        }

        self.focus_dual_pane_at(x, y);

        if let Some(which) = self.tab().list.column_divider_hit(x, y, theme.row_height) {
            self.column_resize = Some(ColumnResizeState { which, last_x: x });
            self.status_override = Some("Drag to resize column".into());
            return Ok(());
        }
        if let Some(key) = self.tab().list.header_hit(x, y, theme.row_height) {
            self.tab_mut().browser.set_sort(key);
            self.tab_mut().list.clear_selection();
            self.status_override = Some(format!(
                "Sorted by {} ({})",
                match key {
                    SortKey::Name => "name",
                    SortKey::Type => "type",
                    SortKey::Size => "size",
                    SortKey::Modified => "modified",
                },
                if self.tab().browser.sort_asc {
                    "asc"
                } else {
                    "desc"
                }
            ));
            return Ok(());
        }

        let len = self.tab().browser.entries.len();
        if let Some(index) = self
            .tab()
            .list
            .index_at(x, y, theme.row_height)
            .filter(|i| *i < len)
        {
            let now = Instant::now();
            let double = !ctrl
                && !shift
                && matches!(
                    self.last_click,
                    Some((prev, t)) if prev == index && now.duration_since(t) < Duration::from_millis(400)
                );
            let single_open = self.settings.single_click_open && !ctrl && !shift;

            if shift {
                self.tab_mut().list.select_range(index);
            } else if ctrl {
                self.tab_mut().list.toggle(index);
            } else {
                self.tab_mut().list.select_only(index);
            }
            self.tab_mut()
                .list
                .ensure_visible(index, theme.row_height);

            if single_open || double {
                self.activate_selected(text_input)?;
                self.last_click = None;
                self.drag = None;
            } else {
                self.last_click = Some((index, now));
                if !self.in_trash() && !self.tab().list.selected.is_empty() {
                    self.drag = Some(DragState {
                        paths: self.selected_paths(),
                        origin: (x, y),
                        active: false,
                        target: DropTarget::None,
                    });
                }
            }
            self.status_override = None;
        } else if self.tab().list.rect.contains(x, y) {
            // Start rubber-band select on empty list space.
            let baseline = if ctrl {
                self.tab().list.selected.clone()
            } else {
                self.tab_mut().list.clear_selection();
                std::collections::BTreeSet::new()
            };
            self.drag = None;
            self.marquee = Some(MarqueeState {
                origin: (x, y),
                current: (x, y),
                additive: ctrl,
                baseline,
            });
            self.status_override = None;
        }

        Ok(())
    }

    fn open_selected_in_new_tab(&mut self) -> Result<()> {
        let paths = self.selected_paths();
        let Some(path) = paths.into_iter().find(|p| p.is_dir()) else {
            self.status_override = Some("Select a folder to open in a new tab".into());
            return Ok(());
        };
        self.new_tab(Some(path))?;
        self.status_override = Some("Opened in new tab".into());
        Ok(())
    }

    fn middle_click_open_tab(&mut self, x: i32, y: i32) -> Result<()> {
        let theme = self.active_theme();
        let len = self.tab().browser.entries.len();
        let Some(index) = self
            .tab()
            .list
            .index_at(x, y, theme.row_height)
            .filter(|i| *i < len)
        else {
            return Ok(());
        };
        let Some(entry) = self.tab().browser.entries.get(index).cloned() else {
            return Ok(());
        };
        if entry.is_dir {
            self.new_tab(Some(entry.path))?;
            self.status_override = Some("Opened in new tab".into());
        }
        Ok(())
    }

    fn new_tab(&mut self, path: Option<PathBuf>) -> Result<()> {
        let tab = match path {
            Some(path) => Tab::open(path)?,
            None => match &self.tab().browser.location {
                Location::ThisPc => {
                    let mut t = Tab::from_browser(Browser::open_this_pc()?)?;
                    t.after_nav();
                    t
                }
                Location::Trash => {
                    let mut browser = Browser::open_this_pc()?;
                    browser.goto_trash(false)?;
                    let mut t = Tab::from_browser(browser)?;
                    t.after_nav();
                    t
                }
                Location::Network => {
                    let recent = self.settings.remote_recent.clone();
                    let bookmarks = self.settings.remote_bookmarks.clone();
                    let mut browser = Browser::open_this_pc()?;
                    browser.goto_network(false, &recent, &bookmarks)?;
                    let mut t = Tab::from_browser(browser)?;
                    t.after_nav();
                    t
                }
                Location::Remote { path, .. } | Location::Path(path) => {
                    Tab::open(path.clone())?
                }
            },
        };
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        self.status_override = Some("New tab".into());
        self.save_session();
        Ok(())
    }

    fn close_tab(&mut self, index: usize) -> Result<()> {
        if self.tabs.len() == 1 {
            self.running = false;
            return Ok(());
        }
        if index >= self.tabs.len() {
            return Ok(());
        }
        self.tabs.remove(index);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if index < self.active {
            self.active -= 1;
        }
        self.status_override = None;
        self.save_session();
        Ok(())
    }

    fn close_other_tabs(&mut self, index: usize) -> Result<()> {
        if index >= self.tabs.len() {
            return Ok(());
        }
        let keep = self.tabs.remove(index);
        self.tabs.clear();
        self.tabs.push(keep);
        self.active = 0;
        self.status_override = Some("Closed other tabs".into());
        Ok(())
    }

    fn close_tabs_to_right(&mut self, index: usize) -> Result<()> {
        if index + 1 >= self.tabs.len() {
            return Ok(());
        }
        self.tabs.truncate(index + 1);
        if self.active > index {
            self.active = index;
        }
        self.status_override = Some("Closed tabs to the right".into());
        Ok(())
    }

    fn jump_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
            self.status_override = None;
        }
    }

    fn cycle_tab(&mut self, delta: i32) {
        let len = self.tabs.len() as i32;
        if len == 0 {
            return;
        }
        self.active = ((self.active as i32 + delta).rem_euclid(len)) as usize;
        self.status_override = None;
    }

    fn move_selection(&mut self, delta: i32, extend: bool) {
        if self.tab().browser.entries.is_empty() {
            return;
        }
        let len = self.tab().browser.entries.len() as i32;
        let cur = self.tab().list.primary().map(|i| i as i32).unwrap_or(-1);
        let next = (cur + delta).clamp(0, len - 1) as usize;
        self.move_selection_to(next, extend);
    }

    fn move_selection_grid(&mut self, dx: i32, dy: i32, extend: bool) {
        use crate::ui::ViewMode;
        if self.tab().browser.entries.is_empty() {
            return;
        }
        let theme = self.active_theme();
        let cols = if self.tab().list.view == ViewMode::Icons {
            self.tab().list.cols(theme.row_height).max(1)
        } else {
            1
        };
        // List/Details: Left/Right do nothing useful — treat as ±1 like Up/Down neighbors.
        let delta = if self.tab().list.view == ViewMode::Icons {
            dy * cols + dx
        } else if dx != 0 {
            dx
        } else {
            dy
        };
        self.move_selection(delta, extend);
    }

    fn move_selection_to(&mut self, next: usize, extend: bool) {
        let len = self.tab().browser.entries.len();
        if len == 0 {
            return;
        }
        let next = next.min(len - 1);
        if extend {
            self.tab_mut().list.select_range(next);
            self.tab_mut().list.anchor = self.tab().list.anchor.or(Some(next));
        } else {
            self.tab_mut().list.select_only(next);
        }
        let theme = self.active_theme();
        self.tab_mut().list.ensure_visible(next, theme.row_height);
        self.status_override = None;
    }

    fn toggle_focus_selection(&mut self) {
        let Some(i) = self.tab().list.primary() else {
            if !self.tab().browser.entries.is_empty() {
                self.move_selection_to(0, false);
            }
            return;
        };
        self.tab_mut().list.toggle(i);
        self.status_override = Some(format!(
            "{} selected",
            self.tab().list.selected.len()
        ));
    }

    fn typeahead_append(&mut self, text: &str) {
        let now = Instant::now();
        if now.duration_since(self.typeahead_at) > Duration::from_millis(800) {
            self.typeahead.clear();
        }
        self.typeahead_at = now;
        let filtered: String = text
            .chars()
            .filter(|c| !c.is_control() && !c.is_whitespace())
            .collect();
        if filtered.is_empty() {
            return;
        }
        self.typeahead.push_str(&filtered);
        let q = self.typeahead.to_lowercase();
        let Some(idx) = self
            .tab()
            .browser
            .entries
            .iter()
            .position(|e| e.name.to_lowercase().starts_with(&q))
        else {
            self.status_override = Some(format!("No match for “{}”", self.typeahead));
            return;
        };
        self.move_selection_to(idx, false);
        self.status_override = Some(format!("Go to “{}”", self.typeahead));
    }

    fn apply_folder_view_for(&mut self, key: &str) {
        if let Some(view) = self.settings.view_for(key) {
            if self.tab().list.view != view {
                self.tab_mut().list.view = view;
                self.tab_mut().list.scroll = 0;
            }
        }
    }

    fn refresh_view(&mut self) -> Result<()> {
        let recent = self.settings.remote_recent.clone();
        let bookmarks = self.settings.remote_bookmarks.clone();
        {
            let browser = &mut self.tab_mut().browser;
            if matches!(browser.location, Location::Network) {
                browser.refresh_network(&recent, &bookmarks)?;
            } else {
                browser.refresh()?;
            }
        }
        if let Some(other) = self.tab_mut().other.as_mut() {
            other.0.refresh()?;
        }
        self.side.rebuild(&self.settings.bookmarks, &self.settings.bookmark_labels);
        self.status_override = Some("Refreshed".into());
        Ok(())
    }

    fn poll_volumes(&mut self) -> Result<()> {
        if self.last_volume_poll.elapsed() < Duration::from_secs(2) {
            return Ok(());
        }
        self.last_volume_poll = Instant::now();
        let Ok(vols) = crate::platform::volumes::list_volumes() else {
            return Ok(());
        };
        let mut now_keys = std::collections::BTreeSet::new();
        let mut now_mounted = std::collections::BTreeSet::new();
        for v in &vols {
            let key = v
                .device
                .as_ref()
                .or(v.mount_path.as_ref())
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| v.label.clone());
            now_keys.insert(key);
            if v.mounted {
                if let Some(mp) = &v.mount_path {
                    now_mounted.insert(mp.display().to_string());
                }
            }
        }
        if !self.volumes_seeded {
            self.seen_volume_keys = now_keys;
            self.mounted_volume_paths = now_mounted;
            self.volumes_seeded = true;
            return Ok(());
        }

        // Auto-mount newly appeared removable / optical devices.
        if self.settings.auto_mount {
            for v in &vols {
                let key = v
                    .device
                    .as_ref()
                    .or(v.mount_path.as_ref())
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| v.label.clone());
                let is_new = !self.seen_volume_keys.contains(&key);
                if is_new
                    && !v.mounted
                    && matches!(
                        v.kind,
                        crate::platform::volumes::VolumeKind::Removable
                            | crate::platform::volumes::VolumeKind::Optical
                    )
                {
                    if let Some(target) = v.action_target() {
                        if let Ok(mount) = crate::platform::volumes::mount_volume(&target) {
                            now_mounted.insert(mount.display().to_string());
                            self.on_media_appeared(mount);
                        }
                    }
                }
            }
        }

        // Newly mounted paths (system auto-mount or ours).
        let appeared: Vec<String> = now_mounted
            .difference(&self.mounted_volume_paths)
            .cloned()
            .collect();
        for mp in appeared {
            self.on_media_appeared(PathBuf::from(mp));
        }

        let refresh_this_pc = matches!(self.tab().browser.location, Location::ThisPc)
            && (now_keys != self.seen_volume_keys || now_mounted != self.mounted_volume_paths);
        self.seen_volume_keys = now_keys;
        self.mounted_volume_paths = now_mounted;
        if refresh_this_pc {
            let _ = self.tab_mut().browser.refresh();
        }
        Ok(())
    }

    fn on_media_appeared(&mut self, mount: PathBuf) {
        use crate::settings::MediaAutoOpen;
        match self.settings.media_auto_open_policy() {
            MediaAutoOpen::Off => {
                self.status_override =
                    Some(format!("Media mounted: {}", mount.display()));
            }
            MediaAutoOpen::Open => {
                if let Ok(()) = self.tab_mut().browser.goto(&mount, true) {
                    self.tab_mut().after_nav();
                }
                self.status_override =
                    Some(format!("Opened media: {}", mount.display()));
            }
            MediaAutoOpen::Ask => {
                self.pending_media_open = Some(mount.clone());
                self.confirm.show(
                    self.chrome.window,
                    ConfirmAction::OpenMedia,
                    format!("Open newly mounted media?\n{}", mount.display()),
                );
            }
        }
    }

    fn activate_selected(&mut self, text_input: &mut TextInputUtil) -> Result<()> {
        let Some(index) = self.tab().list.primary() else {
            return Ok(());
        };
        let Some(entry) = self.tab().browser.entries.get(index).cloned() else {
            return Ok(());
        };
        let path_s = entry.path.to_string_lossy();
        if path_s == "fuckfm:connect" {
            return self.begin_connect_remote(text_input);
        }
        if crate::platform::network::is_remote_uri(&path_s) {
            return self.open_remote_uri(&path_s, None, None, text_input);
        }
        // Unmounted volume device: try mount then open.
        if !entry.is_dir && entry.path.starts_with("/dev") {
            match crate::platform::volumes::mount_volume(&entry.path) {
                Ok(mount) => {
                    self.tab_mut().browser.goto(&mount, true)?;
                    self.tab_mut().after_nav();
                    self.status_override = Some(format!("Mounted {}", mount.display()));
                }
                Err(err) => {
                    self.status_override = Some(format!("Mount failed: {err}"));
                }
            }
            return Ok(());
        }
        if entry.is_dir {
            self.tab_mut().browser.enter(&entry)?;
            self.tab_mut().after_nav();
            self.status_override = None;
        } else {
            match open_default(&entry.path) {
                Ok(()) => {
                    self.status_override =
                        Some(format!("Opened “{}”", entry.name));
                }
                Err(err) => {
                    self.status_override = Some(format!("Open failed: {err}"));
                }
            }
        }
        Ok(())
    }

    fn selected_paths(&self) -> Vec<PathBuf> {
        self.tab()
            .list
            .selected
            .iter()
            .filter_map(|i| self.tab().browser.entries.get(*i).map(|e| e.path.clone()))
            .collect()
    }

    fn copy_paths_text(&mut self, as_uri: bool) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            self.status_override = Some("Nothing selected".into());
            return;
        }
        let text = if as_uri {
            paths_as_uri_list(&paths)
        } else {
            paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("\n")
        };
        match self.sys_clip.set_clipboard_text(&text) {
            Ok(()) => {
                self.status_override = Some(if as_uri {
                    format!("Copied {} URI(s)", paths.len())
                } else {
                    format!("Copied {} path(s)", paths.len())
                });
            }
            Err(err) => {
                self.status_override = Some(format!("Clipboard failed: {err}"));
            }
        }
    }

    fn open_new_window(&mut self) -> Result<()> {
        let exe = std::env::current_exe().context("current_exe")?;
        let mut cmd = Command::new(exe);
        match &self.tab().browser.location {
            Location::Path(p) | Location::Remote { path: p, .. } => {
                cmd.arg(p);
            }
            Location::ThisPc => {
                cmd.arg(THIS_PC);
            }
            Location::Trash => {
                cmd.arg(TRASH);
            }
            Location::Network => {
                cmd.arg(NETWORK);
            }
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("spawn new fuckFM window")?;
        self.status_override = Some("Opened new window".into());
        Ok(())
    }

    fn clipboard_set(&mut self, mode: ClipMode) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            self.status_override = Some("Nothing selected".into());
            return;
        }
        let n = paths.len();
        // Keep a URI-list mirror on the system clipboard for interop / paste-as-text.
        let _ = self
            .sys_clip
            .set_clipboard_text(&paths_as_uri_list(&paths));
        self.clipboard.set(mode, paths);
        self.status_override = Some(format!(
            "{} {} item(s)",
            match mode {
                ClipMode::Copy => "Copied",
                ClipMode::Cut => "Cut",
            },
            n
        ));
    }

    fn clipboard_paste(&mut self) -> Result<()> {
        let Some(dest) = self.tab().browser.cwd_path().map(|p| p.to_path_buf()) else {
            self.status_override = Some("Can't paste on This PC".into());
            return Ok(());
        };
        let mode = self.clipboard.mode;
        let paths = self.clipboard.paths.clone();
        if paths.is_empty() || mode.is_none() {
            self.status_override = Some("Clipboard empty".into());
            return Ok(());
        }
        let copy = mode == Some(ClipMode::Copy);
        if !copy {
            self.clipboard.clear();
        }
        self.begin_transfer(paths, dest, copy)
    }

    fn begin_transfer(&mut self, paths: Vec<PathBuf>, dest: PathBuf, copy: bool) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let total = paths.len();
        let title = if copy {
            "Copying…"
        } else {
            "Moving…"
        };
        self.progress.show(self.chrome.window, title);
        self.sync_progress_ui();
        self.transfer = Some(TransferJob {
            remaining: paths,
            total,
            dest,
            copy,
            apply_all: self.settings.conflict_policy.as_choice(),
            ok: 0,
            fail: 0,
            skip: 0,
            created: Vec::new(),
            moved: Vec::new(),
        });
        self.sync_progress_ui();
        Ok(())
    }

    fn cancel_transfer(&mut self) {
        self.conflict.hide();
        self.progress.hide();
        if let Some(job) = self.transfer.take() {
            if job.ok > 0 || job.skip > 0 || job.fail > 0 {
                if job.copy {
                    self.undo.push(UndoOp::Create {
                        paths: job.created.clone(),
                    });
                } else if !job.moved.is_empty() {
                    self.undo.push(UndoOp::Move {
                        pairs: job.moved.clone(),
                    });
                }
                let _ = self.tab_mut().browser.refresh();
                self.tab_mut().after_nav();
            }
            self.finish_transfer_status(&job, true);
        }
    }

    fn resolve_conflict(&mut self, choice: ConflictChoice) -> Result<()> {
        let apply_all = self.conflict.apply_all;
        self.conflict.hide();
        if let Some(job) = self.transfer.as_mut() {
            if apply_all {
                job.apply_all = Some(choice);
            }
            // Re-process current head with explicit choice once.
            if let Some(src) = job.remaining.first().cloned() {
                self.apply_one_transfer(src, choice)?;
            }
        }
        self.sync_progress_ui();
        Ok(())
    }

    fn sync_progress_ui(&mut self) {
        let Some(job) = self.transfer.as_ref() else {
            self.progress.hide();
            return;
        };
        let done = job.ok + job.fail + job.skip;
        let name = job
            .remaining
            .first()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| {
                if done >= job.total {
                    "Finishing…".into()
                } else {
                    String::new()
                }
            });
        let detail = if name.is_empty() {
            format!("{done} of {}", job.total)
        } else {
            format!("{name}  —  {done} of {}", job.total)
        };
        self.progress.update(detail, done, job.total.max(1));
    }

    fn apply_one_transfer(&mut self, src: PathBuf, choice: ConflictChoice) -> Result<()> {
        let Some(job) = self.transfer.as_mut() else {
            return Ok(());
        };
        job.remaining.retain(|p| p != &src);
        let dest = job.dest.clone();
        let copy = job.copy;
        if !src.exists() {
            job.fail += 1;
            return Ok(());
        }
        if same_path_loose(&src, &dest) || is_invalid_drop_dest(&dest, &[src.clone()]) {
            job.fail += 1;
            return Ok(());
        }
        let result = if copy {
            copy_path_with(&src, &dest, choice)
        } else {
            move_path_with(&src, &dest, choice)
        };
        match result {
            Ok(Some(to)) => {
                job.ok += 1;
                if copy {
                    job.created.push(to);
                } else {
                    job.moved.push((src, to));
                }
            }
            Ok(None) => job.skip += 1,
            Err(_) => job.fail += 1,
        }
        Ok(())
    }

    /// Process a single remaining item (or finish). Returns after conflict pause.
    fn pump_transfer_step(&mut self) -> Result<()> {
        let (src, dest_dir, apply_all) = {
            let Some(job) = self.transfer.as_ref() else {
                self.progress.hide();
                return Ok(());
            };
            let Some(src) = job.remaining.first().cloned() else {
                let job = self.transfer.take().unwrap();
                if job.copy {
                    self.undo.push(UndoOp::Create {
                        paths: job.created.clone(),
                    });
                } else {
                    self.undo.push(UndoOp::Move {
                        pairs: job.moved.clone(),
                    });
                }
                self.tab_mut().browser.refresh()?;
                self.tab_mut().after_nav();
                self.progress.hide();
                self.finish_transfer_status(&job, false);
                return Ok(());
            };
            (src, job.dest.clone(), job.apply_all)
        };
        self.sync_progress_ui();
        let intended = match intended_dest(&src, &dest_dir) {
            Ok(p) => p,
            Err(_) => {
                if let Some(job) = self.transfer.as_mut() {
                    job.remaining.remove(0);
                    job.fail += 1;
                }
                self.sync_progress_ui();
                return Ok(());
            }
        };
        let choice = if intended.exists() && !same_path_loose(&src, &intended) {
            if let Some(c) = apply_all {
                c
            } else if self.settings.conflict_policy == ConflictPolicy::Ask {
                self.conflict
                    .show(self.chrome.window, src.clone(), intended);
                self.status_override = Some("Resolve file conflict".into());
                return Ok(());
            } else {
                self.settings
                    .conflict_policy
                    .as_choice()
                    .unwrap_or(ConflictChoice::Rename)
            }
        } else {
            ConflictChoice::Rename // unused when no conflict; resolve_dest handles
        };
        self.apply_one_transfer(src, choice)?;
        self.sync_progress_ui();
        Ok(())
    }

    fn finish_transfer_status(&mut self, job: &TransferJob, cancelled: bool) {
        let verb = if job.copy { "Copied" } else { "Moved" };
        let mut parts = vec![format!("{verb} {}", job.ok)];
        if job.skip > 0 {
            parts.push(format!("skipped {}", job.skip));
        }
        if job.fail > 0 {
            parts.push(format!("failed {}", job.fail));
        }
        if cancelled {
            parts.push("cancelled".into());
        }
        self.status_override = Some(parts.join(", "));
    }

    fn open_nav_history_menu(&mut self, back: bool, x: i32, y: i32) {
        let theme = self.active_theme();
        let labels = if back {
            self.tab().browser.history_back_labels()
        } else {
            self.tab().browser.history_forward_labels()
        };
        if labels.is_empty() {
            self.status_override = Some(if back {
                "No back history".into()
            } else {
                "No forward history".into()
            });
            self.context_menu = None;
            return;
        }
        let items: Vec<crate::platform::ContextItem> = labels
            .into_iter()
            .enumerate()
            .map(|(index, label)| crate::platform::ContextItem {
                label,
                action: if back {
                    ContextAction::HistoryBack { index }
                } else {
                    ContextAction::HistoryForward { index }
                },
                separator_before: false,
            })
            .collect();
        let mut menu = ContextMenu::open(x, y, items, theme.row_height + 4);
        let bounds = Rect::new(
            self.chrome.window.x,
            self.chrome.window.y,
            self.chrome.window.w,
            self.chrome.window.h,
        );
        menu.clamp_to(bounds);
        self.context_menu = Some(menu);
    }

    fn on_right_down(&mut self, x: i32, y: i32) -> Result<()> {
        let theme = self.active_theme();

        match self.chrome.hit(x, y) {
            ChromeHit::Back | ChromeHit::BackMenu => {
                self.open_nav_history_menu(true, x, y);
                return Ok(());
            }
            ChromeHit::Forward | ChromeHit::ForwardMenu => {
                self.open_nav_history_menu(false, x, y);
                return Ok(());
            }
            _ => {}
        }

        if let TabHit::Tab(index) = self.strip.hit(x, y) {
            let items = tab_context_menu(index, self.tabs.len());
            let mut menu = ContextMenu::open(x, y, items, theme.row_height + 4);
            let (w, h) = (
                self.chrome.window.x + self.chrome.window.w,
                self.chrome.window.y + self.chrome.window.h,
            );
            menu.clamp_to(Rect::new(0, 0, w, h));
            self.context_menu = Some(menu);
            return Ok(());
        }

        if self.side.visible && self.side.rect.contains(x, y) {
            if let Some(index) = self.side.index_at(y, theme.row_height + 2) {
                let place = &self.side.places[index];
                if place.bookmark {
                    if let PlaceTarget::Path(path) = &place.target {
                        let items = vec![
                            crate::platform::ContextItem {
                                label: "Rename Bookmark".into(),
                                action: ContextAction::RenameBookmark { path: path.clone() },
                                separator_before: false,
                            },
                            crate::platform::ContextItem {
                                label: "Remove Bookmark".into(),
                                action: ContextAction::RemoveBookmark { path: path.clone() },
                                separator_before: false,
                            },
                        ];
                        let mut menu = ContextMenu::open(x, y, items, theme.row_height + 4);
                        let (w, h) = (
                            self.chrome.window.x + self.chrome.window.w,
                            self.chrome.window.y + self.chrome.window.h,
                        );
                        menu.clamp_to(Rect::new(0, 0, w, h));
                        self.context_menu = Some(menu);
                        return Ok(());
                    }
                }
            }
            self.context_menu = None;
            return Ok(());
        }

        self.focus_dual_pane_at(x, y);

        if self
            .tab()
            .list
            .header_strip_hit(x, y, theme.row_height)
        {
            let list = &self.tab().list;
            let under = list.header_hit(x, y, theme.row_height);
            let items = detail_column_menu(
                list.show_type,
                list.show_size,
                list.show_modified,
                under,
            );
            let mut menu = ContextMenu::open(x, y, items, theme.row_height + 4);
            let (w, h) = (
                self.chrome.window.x + self.chrome.window.w,
                self.chrome.window.y + self.chrome.window.h,
            );
            menu.clamp_to(Rect::new(0, 0, w, h));
            self.context_menu = Some(menu);
            return Ok(());
        }

        let len = self.tab().browser.entries.len();

        // Prefer selection under cursor in the file list.
        let clicked = self
            .tab()
            .list
            .index_at(x, y, theme.row_height)
            .filter(|i| *i < len);

        if let Some(index) = clicked {
            if !self.tab().list.selected.contains(&index) {
                self.tab_mut().list.select_only(index);
            }
        } else if self.tab().list.rect.contains(x, y) {
            self.tab_mut().list.clear_selection();
        } else {
            self.context_menu = None;
            return Ok(());
        }

        let selected_paths = self.selected_paths();
        let folder = self.context_folder(&selected_paths);
        let kind = if selected_paths.is_empty() {
            ContextKind::Background
        } else {
            ContextKind::Selection
        };
        let items =
            build_context_menu(kind, folder.as_deref(), &selected_paths, self.in_trash())?;
        if items.is_empty() {
            self.context_menu = None;
            return Ok(());
        }

        let mut menu = ContextMenu::open(x, y, items, theme.row_height + 4);
        let (w, h) = (
            self.chrome.window.x + self.chrome.window.w,
            self.chrome.window.y + self.chrome.window.h,
        );
        menu.clamp_to(Rect::new(0, 0, w, h));
        self.context_menu = Some(menu);
        Ok(())
    }

    fn context_folder(&self, selected: &[PathBuf]) -> Option<PathBuf> {
        if let Some(path) = selected.first() {
            if path.is_dir() {
                return Some(path.clone());
            }
            return path.parent().map(|p| p.to_path_buf());
        }
        self.tab().browser.cwd_path().map(|p| p.to_path_buf())
    }

    fn run_context_action(
        &mut self,
        action: ContextAction,
        text_input: &mut TextInputUtil,
    ) -> Result<()> {
        let selected = self.selected_paths();
        let folder = self.context_folder(&selected);
        match action {
            ContextAction::Open => self.activate_selected(text_input)?,
            ContextAction::OpenInNewTab => self.open_selected_in_new_tab()?,
            ContextAction::OpenWith { app_id } => {
                if let Some(path) = selected.first() {
                    match open_with(path, &app_id) {
                        Ok(()) => {
                            self.status_override = Some(format!("Opened with {app_id}"));
                        }
                        Err(err) => {
                            self.status_override =
                                Some(format!("Open with failed: {err}"));
                        }
                    }
                }
            }
            ContextAction::OpenWithOther => {
                if let Some(path) = selected.first() {
                    match open_with_dialog(path) {
                        Ok(()) => {
                            self.status_override = Some("Open with…".into());
                        }
                        Err(err) => {
                            self.status_override =
                                Some(format!("Open with failed: {err}"));
                        }
                    }
                }
            }
            ContextAction::SetDefaultApp { app_id } => {
                if let Some(path) = selected.first() {
                    match set_default_app(path, &app_id) {
                        Ok(()) => {
                            self.status_override =
                                Some(format!("Default set: {app_id}"));
                        }
                        Err(err) => {
                            self.status_override =
                                Some(format!("Set default failed: {err}"));
                        }
                    }
                }
            }
            ContextAction::ToggleAttrReadOnly => {
                if let Some(path) = selected.first() {
                    match toggle_readonly(path) {
                        Ok(on) => {
                            self.status_override = Some(format!(
                                "Read-only {}",
                                if on { "on" } else { "off" }
                            ));
                            self.refresh_view()?;
                        }
                        Err(err) => {
                            self.status_override =
                                Some(format!("Attribute failed: {err}"));
                        }
                    }
                }
            }
            ContextAction::ToggleAttrHidden => {
                if let Some(path) = selected.first() {
                    match toggle_hidden(path) {
                        Ok(on) => {
                            self.status_override = Some(format!(
                                "Hidden {}",
                                if on { "on" } else { "off" }
                            ));
                            self.refresh_view()?;
                        }
                        Err(err) => {
                            self.status_override =
                                Some(format!("Attribute failed: {err}"));
                        }
                    }
                }
            }
            ContextAction::EditPermissions => {
                if let Some(path) = selected.first() {
                    self.begin_perms_edit(path.clone(), text_input);
                }
            }
            ContextAction::TakeOwnership => {
                if let Some(path) = selected.first() {
                    match attrs::take_ownership(path) {
                        Ok(()) => {
                            self.status_override = Some("Ownership updated".into());
                            self.refresh_view()?;
                        }
                        Err(err) => {
                            self.status_override =
                                Some(format!("Take ownership failed: {err}"));
                        }
                    }
                }
            }
            ContextAction::Rename => self.begin_rename(text_input)?,
            ContextAction::NewFolder => self.new_folder(text_input)?,
            ContextAction::NewFile => self.new_file(text_input)?,
            ContextAction::Duplicate => self.duplicate_selected()?,
            ContextAction::Properties => self.show_properties(),
            ContextAction::CreateLink => self.create_link_selected()?,
            ContextAction::CreateHardLink => self.create_hardlink_selected()?,
            ContextAction::MountVolume => self.mount_selected_volume()?,
            ContextAction::UnmountVolume => self.unmount_selected_volume()?,
            ContextAction::EjectVolume => self.eject_selected_volume()?,
            ContextAction::RenameVolumeLabel => self.begin_rename_volume_label(text_input)?,
            ContextAction::FormatVolume => self.format_selected_volume()?,
            ContextAction::ConnectRemote => self.begin_connect_remote(text_input)?,
            ContextAction::AddRemoteBookmark => self.bookmark_selected_remote()?,
            ContextAction::DisconnectRemote => self.disconnect_selected_remote()?,
            ContextAction::ToggleDetailColumn { key } => {
                self.tab_mut().list.toggle_column(key);
                let on = self.tab().list.column_visible(key);
                self.status_override = Some(format!(
                    "{} column {}",
                    match key {
                        SortKey::Name => "Name",
                        SortKey::Type => "Type",
                        SortKey::Size => "Size",
                        SortKey::Modified => "Modified",
                    },
                    if on { "shown" } else { "hidden" }
                ));
            }
            ContextAction::MoveDetailColumn { key, left } => {
                if self.tab_mut().list.move_column(key, left) {
                    self.status_override = Some(format!(
                        "Moved {} {}",
                        match key {
                            SortKey::Name => "Name",
                            SortKey::Type => "Type",
                            SortKey::Size => "Size",
                            SortKey::Modified => "Modified",
                        },
                        if left { "left" } else { "right" }
                    ));
                } else {
                    self.status_override = Some("Can't move that column further".into());
                }
            }
            ContextAction::CloseTab { index } => self.close_tab(index)?,
            ContextAction::CloseOtherTabs { index } => self.close_other_tabs(index)?,
            ContextAction::CloseTabsToRight { index } => self.close_tabs_to_right(index)?,
            ContextAction::Cut => self.clipboard_set(ClipMode::Cut),
            ContextAction::Copy => self.clipboard_set(ClipMode::Copy),
            ContextAction::CopyPath => self.copy_paths_text(false),
            ContextAction::CopyUri => self.copy_paths_text(true),
            ContextAction::Paste => self.clipboard_paste()?,
            ContextAction::SendTo { dest } => self.send_selected_to(&dest)?,
            ContextAction::SendToTrash => self.delete_selected()?,
            ContextAction::RestoreFromTrash => self.restore_selected()?,
            ContextAction::DeletePermanently => self.delete_selected_permanently()?,
            ContextAction::EmptyTrash => self.empty_trash_bin()?,
            ContextAction::SearchInFolder => self.search_in_folder(text_input, &selected)?,
            ContextAction::HistoryBack { index } => {
                if self.tab_mut().browser.jump_back(index)? {
                    self.tab_mut().after_nav();
                    self.status_override = None;
                }
            }
            ContextAction::HistoryForward { index } => {
                if self.tab_mut().browser.jump_forward(index)? {
                    self.tab_mut().after_nav();
                    self.status_override = None;
                }
            }
            ContextAction::AddBookmark => self.add_bookmark_from_selection(),
            ContextAction::RemoveBookmark { path } => {
                if self.settings.remove_bookmark(&path) {
                    self.side.rebuild(&self.settings.bookmarks, &self.settings.bookmark_labels);
                    self.persist_settings();
                    self.status_override = Some("Bookmark removed".into());
                }
            }
            ContextAction::RenameBookmark { path } => {
                self.begin_bookmark_label_edit(path, text_input);
            }
            ContextAction::OpenTerminal => {
                let dir = if selected.len() == 1 && selected[0].is_dir() {
                    selected[0].clone()
                } else {
                    folder.clone().ok_or_else(|| anyhow::anyhow!("no folder"))?
                };
                match run_action(&ContextAction::OpenTerminal, Some(&dir)) {
                    Ok(()) => self.status_override = Some("Opened terminal".into()),
                    Err(err) => self.status_override = Some(format!("Terminal: {err}")),
                }
            }
            #[cfg(windows)]
            other => match run_action(&other, folder.as_deref()) {
                Ok(()) => self.status_override = Some("Ran shell action".into()),
                Err(err) => self.status_override = Some(format!("Action failed: {err}")),
            },
        }
        Ok(())
    }

    fn cycle_view(&mut self) {
        let view = self.tab().list.view.next();
        self.tab_mut().list.view = view;
        self.tab_mut().list.scroll = 0;
        let key = location_key(&self.tab().browser.location);
        self.settings.set_view_for(key.clone(), view);
        self.last_view_loc = Some(key);
        self.persist_settings();
        self.status_override = Some(format!("View: {}", view.label()));
    }

    fn layout_dual_panes(&mut self, area: Rect) {
        let scale = self.settings.icon_scale;
        let tab = self.tab_mut();
        if let Some(other) = tab.other.as_mut() {
            let gap = 4;
            let half = ((area.w - gap) / 2).max(0);
            let left = Rect::new(area.x, area.y, half, area.h);
            let right = Rect::new(
                area.x + half + gap,
                area.y,
                (area.w - half - gap).max(0),
                area.h,
            );
            if tab.focus_right {
                other.1.rect = left;
                tab.list.rect = right;
            } else {
                tab.list.rect = left;
                other.1.rect = right;
            }
            other.1.icon_scale = scale;
        } else {
            tab.list.rect = area;
            tab.focus_right = false;
        }
        tab.list.icon_scale = scale;
    }

    fn focus_dual_pane_at(&mut self, x: i32, y: i32) {
        let want_right = {
            let tab = self.tab();
            let Some(other) = &tab.other else {
                return;
            };
            if other.1.rect.contains(x, y) {
                !tab.focus_right
            } else if tab.list.rect.contains(x, y) {
                tab.focus_right
            } else {
                return;
            }
        };
        self.tab_mut().focus_pane(want_right);
    }

    fn toggle_dual_pane(&mut self) -> Result<()> {
        let on = self.tab_mut().toggle_dual()?;
        self.status_override = Some(if on {
            "Dual pane on (F4)".into()
        } else {
            "Dual pane off".into()
        });
        Ok(())
    }

    fn toggle_preview(&mut self) {
        self.preview.toggle();
        self.chrome.preview_visible = self.preview.visible;
        self.status_override = Some(if self.preview.visible {
            "Preview on".into()
        } else {
            "Preview off".into()
        });
    }

    fn toggle_details_pane(&mut self) {
        self.details.toggle();
        self.chrome.details_visible = self.details.visible;
        self.settings.details_pane_visible = self.details.visible;
        self.persist_settings();
        self.status_override = Some(if self.details.visible {
            "Details pane on (Alt+Shift+P)".into()
        } else {
            "Details pane off".into()
        });
    }

    fn toggle_side_pane(&mut self) {
        self.settings.side_pane_visible = !self.settings.side_pane_visible;
        self.side.visible = self.settings.side_pane_visible;
        self.chrome.side_visible = self.side.visible;
        self.persist_settings();
        self.status_override = Some(if self.side.visible {
            "Navigation pane on".into()
        } else {
            "Navigation pane off".into()
        });
    }

    fn add_bookmark_from_selection(&mut self) {
        let mut candidates: Vec<PathBuf> = self
            .selected_paths()
            .into_iter()
            .filter(|p| p.is_dir())
            .collect();
        if candidates.is_empty() {
            if let Some(cwd) = self.tab().browser.cwd_path() {
                candidates.push(cwd.to_path_buf());
            }
        }
        let mut ok = 0usize;
        for path in candidates {
            let path = path.canonicalize().unwrap_or(path);
            if self.settings.add_bookmark(path) {
                ok += 1;
            }
        }
        if ok > 0 {
            self.side.rebuild(&self.settings.bookmarks, &self.settings.bookmark_labels);
            self.persist_settings();
            self.status_override = Some(format!("Bookmarked {ok} folder(s)"));
        } else {
            self.status_override = Some("Nothing to bookmark".into());
        }
    }

    fn on_settings_click(&mut self, x: i32, y: i32) {
        let mut dirty = false;
        match self.settings_dialog.hit(x, y) {
            SettingsHit::Close => {
                self.settings_dialog.hide();
            }
            SettingsHit::Theme(id) => {
                self.settings.theme_id = id;
                self.icons = IconPack::load_for_theme(id, &self.settings.icon_pack);
                self.status_override = Some(format!("Theme: {}", id.label()));
                dirty = true;
            }
            SettingsHit::IconScale(scale) => {
                self.settings.icon_scale = scale.clamp(1, 4);
                for tab in &mut self.tabs {
                    tab.list.icon_scale = self.settings.icon_scale;
                    tab.list.scroll = 0;
                }
                self.status_override =
                    Some(format!("Icon size: {}", self.settings.icon_scale_label()));
                dirty = true;
            }
            SettingsHit::ToggleClassicChrome => {
                self.settings.classic_window_chrome = !self.settings.classic_window_chrome;
                self.chrome.classic_framed = self.settings.classic_window_chrome;
                self.status_override = Some(if self.settings.classic_window_chrome {
                    "Classic framed chrome on".into()
                } else {
                    "OS frame only (no nested window)".into()
                });
                dirty = true;
            }
            SettingsHit::ToggleVisualEffects => {
                self.settings.visual_effects = !self.settings.visual_effects;
                self.status_override = Some(if self.settings.visual_effects {
                    "Visual effects on".into()
                } else {
                    "Visual effects off".into()
                });
                dirty = true;
            }
            SettingsHit::ToggleCompactView => {
                self.settings.compact_view = !self.settings.compact_view;
                self.status_override = Some(if self.settings.compact_view {
                    "Compact view on".into()
                } else {
                    "Compact view off".into()
                });
                dirty = true;
            }
            SettingsHit::ToggleHidden => {
                self.settings.show_hidden_default = !self.settings.show_hidden_default;
                dirty = true;
            }
            SettingsHit::TogglePreview => {
                self.settings.preview_default = !self.settings.preview_default;
                dirty = true;
            }
            SettingsHit::ToggleDetailsPane => {
                self.settings.details_pane_visible = !self.settings.details_pane_visible;
                self.details.visible = self.settings.details_pane_visible;
                self.chrome.details_visible = self.details.visible;
                dirty = true;
            }
            SettingsHit::ToggleSidePane => {
                self.toggle_side_pane();
                dirty = false; // toggle_side_pane already persists
            }
            SettingsHit::ToggleRestoreTabs => {
                self.settings.restore_tabs = !self.settings.restore_tabs;
                self.status_override = Some(if self.settings.restore_tabs {
                    "Restore session tabs on".into()
                } else {
                    "Restore session tabs off".into()
                });
                dirty = true;
            }
            SettingsHit::CycleStartupLocation => {
                self.settings.cycle_startup_location();
                self.status_override = Some(format!(
                    "Startup: {}",
                    self.settings.startup_location_label()
                ));
                dirty = true;
            }
            SettingsHit::ToggleSingleClick => {
                self.settings.single_click_open = !self.settings.single_click_open;
                self.status_override = Some(if self.settings.single_click_open {
                    "Single-click open on".into()
                } else {
                    "Double-click open on".into()
                });
                dirty = true;
            }
            SettingsHit::ToggleConfirmDelete => {
                self.settings.confirm_delete = !self.settings.confirm_delete;
                self.status_override = Some(if self.settings.confirm_delete {
                    "Confirm delete on".into()
                } else {
                    "Confirm delete off".into()
                });
                dirty = true;
            }
            SettingsHit::ToggleConfirmDrag => {
                self.settings.confirm_drag = !self.settings.confirm_drag;
                self.status_override = Some(if self.settings.confirm_drag {
                    "Confirm drag-and-drop on".into()
                } else {
                    "Confirm drag-and-drop off".into()
                });
                dirty = true;
            }
            SettingsHit::ToggleAutoMount => {
                self.settings.auto_mount = !self.settings.auto_mount;
                self.status_override = Some(if self.settings.auto_mount {
                    "Auto-mount on".into()
                } else {
                    "Auto-mount off".into()
                });
                dirty = true;
            }
            SettingsHit::CycleMediaAutoOpen => {
                let next = self.settings.media_auto_open_policy().next();
                self.settings.media_auto_open = next.key().into();
                self.status_override = Some(format!("Media auto-open: {}", next.label()));
                dirty = true;
            }
            SettingsHit::CycleConflictPolicy => {
                self.settings.conflict_policy = self.settings.conflict_policy.next();
                self.status_override = Some(format!(
                    "Conflicts: {}",
                    self.settings.conflict_policy.label()
                ));
                dirty = true;
            }
            SettingsHit::None => {
                // Click outside panel closes.
                if !self.settings_dialog.rect.contains(x, y) {
                    self.settings_dialog.hide();
                }
            }
        }
        if dirty {
            self.persist_settings();
        }
    }

    fn persist_settings(&mut self) {
        if let Err(err) = self.settings.save() {
            self.status_override = Some(format!("Settings save failed: {err}"));
        }
    }

    fn save_session(&mut self) {
        self.settings.session_tabs = self
            .tabs
            .iter()
            .map(|t| location_key(&t.browser.location))
            .collect();
        self.settings.session_active = self.active;
        let _ = self.settings.save();
    }

    fn begin_pattern_select(&mut self, text_input: &mut TextInputUtil) {
        self.bookmark_label_edit = None;
        self.perms_edit = None;
        let (w, h) = (self.chrome.window.w, self.chrome.window.h);
        let rect = Rect::new(
            self.chrome.window.x + (w - 360) / 2,
            self.chrome.window.y + (h - 110) / 2,
            360,
            110,
        );
        self.pattern = Some(PromptDialog::new(
            "Select by pattern",
            "Enter = select   Esc = cancel   * ? wildcards",
            rect,
        ));
        text_input.start();
        self.status_override = Some("Pattern select…".into());
    }

    fn begin_perms_edit(&mut self, path: PathBuf, text_input: &mut TextInputUtil) {
        self.bookmark_label_edit = None;
        let (w, h) = (self.chrome.window.w, self.chrome.window.h);
        let rect = Rect::new(
            self.chrome.window.x + (w - 360) / 2,
            self.chrome.window.y + (h - 110) / 2,
            360,
            110,
        );
        let mut prompt = PromptDialog::new(
            "Permissions (octal)",
            "e.g. 755 or 0644   Enter = apply   Esc = cancel",
            rect,
        );
        if let Ok(Some(mode)) = attrs::get_unix_mode(&path) {
            let s = format!("{mode:o}");
            prompt.cursor = s.len();
            prompt.text = s;
        }
        self.perms_edit = Some(path);
        self.pattern = Some(prompt);
        text_input.start();
        self.status_override = Some("Edit permissions…".into());
    }

    fn cancel_pattern(&mut self, text_input: &mut TextInputUtil) {
        self.pattern = None;
        self.bookmark_label_edit = None;
        self.perms_edit = None;
        self.volume_label_edit = None;
        self.connect_remote_prompt = false;
        self.remote_creds_uri = None;
        text_input.stop();
        self.status_override = None;
    }

    fn begin_bookmark_label_edit(&mut self, path: PathBuf, text_input: &mut TextInputUtil) {
        let (w, h) = (self.chrome.window.w, self.chrome.window.h);
        let rect = Rect::new(
            self.chrome.window.x + (w - 360) / 2,
            self.chrome.window.y + (h - 110) / 2,
            360,
            110,
        );
        let mut prompt = PromptDialog::new(
            "Rename bookmark",
            "Enter = save   Esc = cancel   empty = default name",
            rect,
        );
        let initial = self
            .settings
            .bookmark_label(&path)
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                path.file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string())
            });
        prompt.cursor = initial.len();
        prompt.text = initial;
        self.bookmark_label_edit = Some(path);
        self.pattern = Some(prompt);
        text_input.start();
        self.status_override = Some("Rename bookmark…".into());
    }

    fn commit_pattern(&mut self, text_input: &mut TextInputUtil) {
        let Some(prompt) = self.pattern.take() else {
            return;
        };
        text_input.stop();
        if let Some(path) = self.bookmark_label_edit.take() {
            self.settings
                .set_bookmark_label(&path, prompt.text.trim().to_string());
            self.side
                .rebuild(&self.settings.bookmarks, &self.settings.bookmark_labels);
            self.persist_settings();
            self.status_override = Some("Bookmark renamed".into());
            return;
        }
        if self.connect_remote_prompt {
            self.connect_remote_prompt = false;
            let uri = prompt.text.trim().to_string();
            if uri.is_empty() {
                self.status_override = Some("Empty URI".into());
            } else {
                let _ = self.open_remote_uri(&uri, None, None, text_input);
            }
            return;
        }
        if let Some(uri) = self.remote_creds_uri.take() {
            let raw = prompt.text.trim();
            let (user, pass) = if let Some((u, p)) = raw.split_once(':') {
                (Some(u.trim()), Some(p))
            } else if raw.is_empty() {
                (None, None)
            } else {
                (Some(raw), None)
            };
            let _ = self.open_remote_uri(&uri, user, pass, text_input);
            return;
        }
        if let Some(path) = self.volume_label_edit.take() {
            let label = prompt.text.trim();
            match crate::platform::volumes::set_volume_label(&path, label) {
                Ok(()) => {
                    let _ = self.refresh_view();
                    self.status_override = Some("Volume label updated".into());
                }
                Err(err) => {
                    self.status_override = Some(format!("Label failed: {err}"));
                }
            }
            return;
        }
        if let Some(path) = self.perms_edit.take() {
            match parse_octal_mode(prompt.text.trim()) {
                Ok(mode) => match attrs::set_unix_mode(&path, mode) {
                    Ok(()) => {
                        self.status_override =
                            Some(format!("Permissions set to {mode:04o}"));
                        let _ = self.refresh_view();
                    }
                    Err(err) => {
                        self.status_override =
                            Some(format!("chmod failed: {err}"));
                    }
                },
                Err(err) => {
                    self.status_override = Some(err);
                }
            }
            return;
        }
        let mut pat = prompt.text.trim().to_string();
        if pat.is_empty() {
            self.status_override = Some("Empty pattern".into());
            return;
        }
        if !pat.contains('*') && !pat.contains('?') {
            pat = format!("*{pat}*");
        }
        let mut selected = std::collections::BTreeSet::new();
        for (i, entry) in self.tab().browser.entries.iter().enumerate() {
            if glob_match(&pat, &entry.name) {
                selected.insert(i);
            }
        }
        let n = selected.len();
        self.tab_mut().list.selected = selected;
        self.tab_mut().list.anchor = self.tab().list.selected.iter().next().copied();
        self.status_override = Some(format!("Selected {n} matching “{pat}”"));
    }

    fn create_link_selected(&mut self) -> Result<()> {
        let Some(path) = self.selected_paths().into_iter().next() else {
            self.status_override = Some("Nothing selected".into());
            return Ok(());
        };
        match create_symlink(&path) {
            Ok(link) => {
                self.tab_mut().browser.refresh()?;
                if let Some(idx) = self
                    .tab()
                    .browser
                    .entries
                    .iter()
                    .position(|e| e.path == link)
                {
                    self.tab_mut().list.select_only(idx);
                }
                self.status_override = Some(format!("Link: {}", link.display()));
            }
            Err(err) => {
                self.status_override = Some(format!("Link failed: {err}"));
            }
        }
        Ok(())
    }

    fn create_hardlink_selected(&mut self) -> Result<()> {
        let Some(path) = self.selected_paths().into_iter().next() else {
            self.status_override = Some("Nothing selected".into());
            return Ok(());
        };
        match create_hardlink(&path) {
            Ok(link) => {
                self.tab_mut().browser.refresh()?;
                if let Some(idx) = self
                    .tab()
                    .browser
                    .entries
                    .iter()
                    .position(|e| e.path == link)
                {
                    self.tab_mut().list.select_only(idx);
                }
                self.status_override = Some(format!("Hard link: {}", link.display()));
            }
            Err(err) => {
                self.status_override = Some(format!("Hard link failed: {err}"));
            }
        }
        Ok(())
    }

    fn goto_network_view(&mut self) -> Result<()> {
        let recent = self.settings.remote_recent.clone();
        let bookmarks = self.settings.remote_bookmarks.clone();
        self.tab_mut()
            .browser
            .goto_network(true, &recent, &bookmarks)?;
        self.tab_mut().after_nav();
        self.status_override = Some("Network".into());
        Ok(())
    }

    fn open_remote_uri(
        &mut self,
        uri: &str,
        user: Option<&str>,
        password: Option<&str>,
        text_input: &mut TextInputUtil,
    ) -> Result<()> {
        match crate::platform::network::open_remote_uri(uri, user, password) {
            Ok(mount) => {
                self.settings.remember_remote(&mount.uri);
                self.persist_settings();
                self.tab_mut()
                    .browser
                    .goto_remote(&mount.uri, mount.path, true)?;
                self.tab_mut().after_nav();
                self.status_override = Some(format!("Connected {}", mount.uri));
            }
            Err(err) => {
                let msg = err.to_string();
                let needs_creds = user.is_none()
                    && (msg.to_ascii_lowercase().contains("credential")
                        || msg.to_ascii_lowercase().contains("password")
                        || msg.to_ascii_lowercase().contains("auth")
                        || msg.contains("failed"));
                if needs_creds {
                    self.begin_remote_creds(uri, text_input);
                    self.status_override =
                        Some(format!("Credentials required for {uri}"));
                } else {
                    self.status_override = Some(format!("Connect failed: {err}"));
                }
            }
        }
        Ok(())
    }

    fn begin_remote_creds(&mut self, uri: &str, text_input: &mut TextInputUtil) {
        self.bookmark_label_edit = None;
        self.perms_edit = None;
        self.volume_label_edit = None;
        self.connect_remote_prompt = false;
        let (w, h) = (self.chrome.window.w, self.chrome.window.h);
        let rect = Rect::new(
            self.chrome.window.x + (w - 420) / 2,
            self.chrome.window.y + (h - 110) / 2,
            420,
            110,
        );
        let prompt = PromptDialog::new(
            format!("Credentials for {uri}"),
            "user:password   Enter = connect   Esc = cancel",
            rect,
        );
        self.remote_creds_uri = Some(uri.to_string());
        self.pattern = Some(prompt);
        text_input.start();
    }

    fn begin_connect_remote(&mut self, text_input: &mut TextInputUtil) -> Result<()> {
        self.bookmark_label_edit = None;
        self.perms_edit = None;
        self.volume_label_edit = None;
        self.remote_creds_uri = None;
        let (w, h) = (self.chrome.window.w, self.chrome.window.h);
        let rect = Rect::new(
            self.chrome.window.x + (w - 420) / 2,
            self.chrome.window.y + (h - 110) / 2,
            420,
            110,
        );
        let mut prompt = PromptDialog::new(
            "Connect to server",
            "smb://host/share   ftp://…   sftp://…   mtp://…   Enter = connect",
            rect,
        );
        prompt.text = "smb://".into();
        prompt.cursor = prompt.text.len();
        self.connect_remote_prompt = true;
        self.pattern = Some(prompt);
        text_input.start();
        self.status_override = Some("Connect remote…".into());
        Ok(())
    }

    fn bookmark_selected_remote(&mut self) -> Result<()> {
        let uri = match &self.tab().browser.location {
            Location::Remote { uri, .. } => uri.clone(),
            _ => {
                if let Some(p) = self.selected_paths().first() {
                    p.display().to_string()
                } else {
                    self.status_override = Some("No remote location".into());
                    return Ok(());
                }
            }
        };
        self.settings.add_remote_bookmark(&uri);
        self.persist_settings();
        self.status_override = Some("Remote bookmarked".into());
        Ok(())
    }

    fn disconnect_selected_remote(&mut self) -> Result<()> {
        let (uri, path) = match &self.tab().browser.location {
            Location::Remote { uri, path } => (uri.clone(), path.clone()),
            _ => {
                self.status_override = Some("Not on a remote mount".into());
                return Ok(());
            }
        };
        let _ = path;
        match crate::platform::network::unmount_remote(&uri) {
            Ok(()) => {
                self.goto_network_view()?;
                self.status_override = Some("Disconnected".into());
            }
            Err(err) => {
                self.status_override = Some(format!("Disconnect failed: {err}"));
            }
        }
        Ok(())
    }

    fn selected_volume_target(&self) -> Option<PathBuf> {
        self.selected_paths().into_iter().next().or_else(|| {
            self.tab().browser.cwd_path().map(|p| p.to_path_buf())
        })
    }

    fn mount_selected_volume(&mut self) -> Result<()> {
        let Some(path) = self.selected_volume_target() else {
            self.status_override = Some("No volume selected".into());
            return Ok(());
        };
        match crate::platform::volumes::mount_volume(&path) {
            Ok(mounted) => {
                self.tab_mut().browser.goto(&mounted, true)?;
                self.tab_mut().after_nav();
                self.status_override = Some(format!("Mounted {}", mounted.display()));
            }
            Err(err) => {
                self.status_override = Some(format!("Mount failed: {err}"));
            }
        }
        Ok(())
    }

    fn unmount_selected_volume(&mut self) -> Result<()> {
        let Some(path) = self.selected_volume_target() else {
            self.status_override = Some("No volume selected".into());
            return Ok(());
        };
        match crate::platform::volumes::unmount_volume(&path) {
            Ok(()) => {
                self.tab_mut().browser.goto_this_pc(true)?;
                self.tab_mut().after_nav();
                self.status_override = Some("Unmounted".into());
            }
            Err(err) => {
                self.status_override = Some(format!("Unmount failed: {err}"));
            }
        }
        Ok(())
    }

    fn eject_selected_volume(&mut self) -> Result<()> {
        let Some(path) = self.selected_volume_target() else {
            self.status_override = Some("No volume selected".into());
            return Ok(());
        };
        match crate::platform::volumes::eject_volume(&path) {
            Ok(()) => {
                self.tab_mut().browser.goto_this_pc(true)?;
                self.tab_mut().after_nav();
                self.status_override = Some("Ejected".into());
            }
            Err(err) => {
                self.status_override = Some(format!("Eject failed: {err}"));
            }
        }
        Ok(())
    }

    fn begin_rename_volume_label(&mut self, text_input: &mut TextInputUtil) -> Result<()> {
        let Some(path) = self.selected_volume_target() else {
            self.status_override = Some("No volume selected".into());
            return Ok(());
        };
        self.bookmark_label_edit = None;
        self.perms_edit = None;
        self.connect_remote_prompt = false;
        self.remote_creds_uri = None;
        let (w, h) = (self.chrome.window.w, self.chrome.window.h);
        let rect = Rect::new(
            self.chrome.window.x + (w - 360) / 2,
            self.chrome.window.y + (h - 110) / 2,
            360,
            110,
        );
        let mut prompt = PromptDialog::new(
            "Volume label",
            "Enter = apply   Esc = cancel",
            rect,
        );
        let label = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        prompt.cursor = label.len();
        prompt.text = label;
        self.volume_label_edit = Some(path);
        self.pattern = Some(prompt);
        text_input.start();
        self.status_override = Some("Rename volume…".into());
        Ok(())
    }

    fn format_selected_volume(&mut self) -> Result<()> {
        let Some(path) = self.selected_volume_target() else {
            self.status_override = Some("No volume selected".into());
            return Ok(());
        };
        self.pending_format = Some(path.clone());
        self.confirm.show(
            self.chrome.window,
            ConfirmAction::FormatVolume,
            format!(
                "Format {} as exFAT?\nThis erases all data on the volume.",
                path.display()
            ),
        );
        Ok(())
    }

    fn execute_format_volume(&mut self) -> Result<()> {
        let Some(path) = self.pending_format.take() else {
            self.status_override = Some("Nothing to format".into());
            return Ok(());
        };
        match crate::platform::volumes::format_volume(&path, "exfat") {
            Ok(()) => {
                self.tab_mut().browser.goto_this_pc(true)?;
                self.tab_mut().after_nav();
                self.status_override = Some(format!("Formatted {}", path.display()));
            }
            Err(err) => {
                self.status_override = Some(format!("Format failed: {err}"));
            }
        }
        Ok(())
    }

    fn new_folder(&mut self, text_input: &mut TextInputUtil) -> Result<()> {
        let Some(parent) = self.tab().browser.cwd_path().map(|p| p.to_path_buf()) else {
            self.status_override = Some("Can't create folder on This PC".into());
            return Ok(());
        };
        match create_folder(&parent, "New Folder") {
            Ok(path) => {
                self.tab_mut().browser.refresh()?;
                if let Some(idx) = self
                    .tab()
                    .browser
                    .entries
                    .iter()
                    .position(|e| e.path == path)
                {
                    self.tab_mut().list.select_only(idx);
                    let theme = self.active_theme();
                    self.tab_mut()
                        .list
                        .ensure_visible(idx, theme.row_height);
                }
                self.begin_rename(text_input)?;
            }
            Err(err) => {
                self.status_override = Some(format!("New folder failed: {err}"));
            }
        }
        Ok(())
    }

    fn new_file(&mut self, text_input: &mut TextInputUtil) -> Result<()> {
        let Some(parent) = self.tab().browser.cwd_path().map(|p| p.to_path_buf()) else {
            self.status_override = Some("Can't create file on This PC".into());
            return Ok(());
        };
        match create_empty_file(&parent, "New File") {
            Ok(path) => {
                self.tab_mut().browser.refresh()?;
                if let Some(idx) = self
                    .tab()
                    .browser
                    .entries
                    .iter()
                    .position(|e| e.path == path)
                {
                    self.tab_mut().list.select_only(idx);
                    let theme = self.active_theme();
                    self.tab_mut()
                        .list
                        .ensure_visible(idx, theme.row_height);
                }
                self.begin_rename(text_input)?;
            }
            Err(err) => {
                self.status_override = Some(format!("New file failed: {err}"));
            }
        }
        Ok(())
    }

    fn duplicate_selected(&mut self) -> Result<()> {
        let paths = self.selected_paths();
        if paths.is_empty() {
            self.status_override = Some("Nothing selected".into());
            return Ok(());
        }
        let mut ok = 0usize;
        let mut fail = 0usize;
        let mut created = Vec::new();
        let mut last = None;
        for path in paths {
            match duplicate_path(&path) {
                Ok(p) => {
                    ok += 1;
                    last = Some(p.clone());
                    created.push(p);
                }
                Err(_) => fail += 1,
            }
        }
        self.undo.push(UndoOp::Create { paths: created });
        self.tab_mut().browser.refresh()?;
        if let Some(path) = last {
            if let Some(idx) = self
                .tab()
                .browser
                .entries
                .iter()
                .position(|e| e.path == path)
            {
                self.tab_mut().list.select_only(idx);
            }
        }
        self.status_override = Some(if fail == 0 {
            format!("Duplicated {ok} item(s)")
        } else {
            format!("Duplicated {ok}, failed {fail}")
        });
        Ok(())
    }

    fn begin_rename(&mut self, text_input: &mut TextInputUtil) -> Result<()> {
        let Some(index) = self.tab().list.primary() else {
            self.status_override = Some("Nothing selected".into());
            return Ok(());
        };
        let Some(entry) = self.tab().browser.entries.get(index).cloned() else {
            return Ok(());
        };
        let (w, h) = (self.chrome.window.w, self.chrome.window.h);
        let rect = Rect::new(
            self.chrome.window.x + w / 2 - 180,
            self.chrome.window.y + h / 2 - 50,
            360,
            100,
        );
        self.context_menu = None;
        self.rename = Some(RenameDialog::new(entry.path, rect));
        text_input.start();
        self.status_override = Some("Renaming…".into());
        Ok(())
    }

    fn cancel_rename(&mut self, text_input: &mut TextInputUtil) {
        self.rename = None;
        text_input.stop();
        self.status_override = None;
    }

    fn commit_rename(&mut self, text_input: &mut TextInputUtil) -> Result<()> {
        let Some(rename) = self.rename.take() else {
            return Ok(());
        };
        text_input.stop();
        let new_name = rename.text.trim().to_string();
        let from = rename.path.clone();
        match rename_path(&rename.path, &new_name) {
            Ok(to) => {
                self.undo.push(UndoOp::Rename { from, to: to.clone() });
                self.tab_mut().browser.refresh()?;
                // Reselect renamed item if still visible.
                if let Some(idx) = self
                    .tab()
                    .browser
                    .entries
                    .iter()
                    .position(|e| e.path == to || e.name == new_name)
                {
                    self.tab_mut().list.select_only(idx);
                } else {
                    self.tab_mut().list.clear_selection();
                }
                self.status_override = Some(format!("Renamed to “{new_name}”"));
            }
            Err(err) => {
                self.status_override = Some(format!("Rename failed: {err}"));
            }
        }
        Ok(())
    }

    fn delete_selected(&mut self) -> Result<()> {
        if self.in_trash() {
            return self.delete_selected_permanently();
        }
        let paths = self.selected_paths();
        if paths.is_empty() {
            self.status_override = Some("Nothing selected".into());
            return Ok(());
        }
        let mut ok = 0usize;
        let mut fail = 0usize;
        let mut items = Vec::new();
        for path in paths {
            match trash_path(&path) {
                Ok(Some(trashed)) => {
                    items.push((path, trashed));
                    ok += 1;
                }
                Ok(None) => ok += 1,
                Err(_) => fail += 1,
            }
        }
        self.undo.push(UndoOp::Trash { items });
        self.tab_mut().browser.refresh()?;
        self.tab_mut().after_nav();
        let bin = trash_label();
        self.status_override = Some(if fail == 0 {
            format!("Moved {ok} item(s) to {bin}")
        } else {
            format!("Sent {ok} to {bin}, failed {fail}")
        });
        Ok(())
    }

    fn delete_selected_permanently(&mut self) -> Result<()> {
        let paths = self.selected_paths();
        if paths.is_empty() {
            self.status_override = Some("Nothing selected".into());
            return Ok(());
        }
        if self.settings.confirm_delete {
            let n = paths.len();
            self.confirm.show(
                self.chrome.window,
                ConfirmAction::DeletePermanent,
                format!("Permanently delete {n} item(s)? This cannot be undone."),
            );
            return Ok(());
        }
        self.execute_delete_permanent()
    }

    fn execute_delete_permanent(&mut self) -> Result<()> {
        let paths = self.selected_paths();
        if paths.is_empty() {
            self.status_override = Some("Nothing selected".into());
            return Ok(());
        }
        let mut ok = 0usize;
        let mut fail = 0usize;
        for path in paths {
            match delete_permanently(&path) {
                Ok(()) => ok += 1,
                Err(_) => fail += 1,
            }
        }
        self.tab_mut().browser.refresh()?;
        self.tab_mut().after_nav();
        self.status_override = Some(if fail == 0 {
            format!("Permanently deleted {ok} item(s)")
        } else {
            format!("Deleted {ok}, failed {fail}")
        });
        Ok(())
    }

    fn restore_selected(&mut self) -> Result<()> {
        let paths = self.selected_paths();
        if paths.is_empty() {
            self.status_override = Some("Nothing selected".into());
            return Ok(());
        }
        let mut ok = 0usize;
        let mut fail = 0usize;
        for path in paths {
            match restore_from_trash(&path) {
                Ok(()) => ok += 1,
                Err(_) => fail += 1,
            }
        }
        self.tab_mut().browser.refresh()?;
        self.tab_mut().after_nav();
        self.status_override = Some(if fail == 0 {
            format!("Restored {ok} item(s)")
        } else {
            format!("Restored {ok}, failed {fail}")
        });
        Ok(())
    }

    fn empty_trash_bin(&mut self) -> Result<()> {
        if self.settings.confirm_delete {
            self.confirm.show(
                self.chrome.window,
                ConfirmAction::EmptyTrash,
                format!("Empty {}? All items will be permanently deleted.", trash_label()),
            );
            return Ok(());
        }
        self.execute_empty_trash()
    }

    fn execute_empty_trash(&mut self) -> Result<()> {
        match empty_trash() {
            Ok(()) => {
                if self.in_trash() {
                    self.tab_mut().browser.refresh()?;
                    self.tab_mut().after_nav();
                }
                self.status_override = Some(format!("Emptied {}", trash_label()));
            }
            Err(err) => {
                self.status_override = Some(format!("Empty failed: {err}"));
            }
        }
        Ok(())
    }

    fn resolve_confirm(&mut self, yes: bool) -> Result<()> {
        if !self.confirm.open {
            return Ok(());
        }
        let action = self.confirm.action;
        self.confirm.hide();
        if !yes {
            self.pending_drag = None;
            self.pending_format = None;
            self.pending_media_open = None;
            self.status_override = Some("Cancelled".into());
            return Ok(());
        }
        match action {
            ConfirmAction::DeletePermanent => self.execute_delete_permanent(),
            ConfirmAction::EmptyTrash => self.execute_empty_trash(),
            ConfirmAction::DragDrop => {
                if let Some(pending) = self.pending_drag.take() {
                    self.apply_drag_drop(pending.paths, pending.target, pending.copy)
                } else {
                    self.status_override = Some("Drop cancelled".into());
                    Ok(())
                }
            }
            ConfirmAction::FormatVolume => self.execute_format_volume(),
            ConfirmAction::OpenMedia => {
                if let Some(path) = self.pending_media_open.take() {
                    match self.tab_mut().browser.goto(&path, true) {
                        Ok(()) => {
                            self.tab_mut().after_nav();
                            self.status_override =
                                Some(format!("Opened {}", path.display()));
                        }
                        Err(err) => {
                            self.status_override =
                                Some(format!("Open media failed: {err}"));
                        }
                    }
                } else {
                    self.status_override = Some("No media path".into());
                }
                Ok(())
            }
        }
    }
}

enum AddressTarget {
    ThisPc,
    Trash,
    Network,
    Remote(String),
    Path(PathBuf),
}

fn resolve_address(raw: &str, cwd: Option<&Path>) -> Result<AddressTarget> {
    let s = raw.trim();
    let lower = s.to_ascii_lowercase();
    if lower == "this pc" || s == THIS_PC {
        return Ok(AddressTarget::ThisPc);
    }
    if lower == "trash"
        || lower == "recycle bin"
        || s == TRASH
        || lower == trash_label().to_ascii_lowercase()
    {
        return Ok(AddressTarget::Trash);
    }
    if lower == "network" || s == NETWORK {
        return Ok(AddressTarget::Network);
    }
    if crate::platform::network::is_remote_uri(s) {
        return Ok(AddressTarget::Remote(
            crate::platform::network::normalize_uri(s),
        ));
    }

    let path = if s == "~" {
        home_dir()
    } else if let Some(rest) = s.strip_prefix("~/") {
        home_dir().join(rest)
    } else if let Some(rest) = s.strip_prefix("~\\") {
        home_dir().join(rest)
    } else {
        let p = PathBuf::from(s);
        if p.is_absolute() {
            p
        } else if let Some(cwd) = cwd {
            cwd.join(p)
        } else {
            home_dir().join(p)
        }
    };

    if path.is_dir() {
        Ok(AddressTarget::Path(path))
    } else if path.is_file() {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("file has no parent"))?
            .to_path_buf();
        Ok(AddressTarget::Path(parent))
    } else if path.exists() {
        Ok(AddressTarget::Path(path))
    } else {
        Err(anyhow::anyhow!("{} not found", path.display()))
    }
}

fn tab_number_from_key(key: Keycode) -> Option<usize> {
    Some(match key {
        Keycode::Num1 => 1,
        Keycode::Num2 => 2,
        Keycode::Num3 => 3,
        Keycode::Num4 => 4,
        Keycode::Num5 => 5,
        Keycode::Num6 => 6,
        Keycode::Num7 => 7,
        Keycode::Num8 => 8,
        Keycode::Num9 => 9,
        _ => return None,
    })
}

fn same_path_loose(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// True if `dest` is one of `paths`, or lies inside a selected directory (can't move into self).
fn is_invalid_drop_dest(dest: &Path, paths: &[PathBuf]) -> bool {
    for src in paths {
        if same_path_loose(src, dest) {
            return true;
        }
        if src.is_dir() {
            let Ok(src_c) = src.canonicalize() else {
                if dest.starts_with(src) {
                    return true;
                }
                continue;
            };
            let Ok(dest_c) = dest.canonicalize() else {
                continue;
            };
            if dest_c.starts_with(&src_c) {
                return true;
            }
        }
    }
    false
}

pub fn boot() -> Result<()> {
    // CLI utilities that must run before SDL (folder-handler registration, etc.).
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = handle_cli_flags(&args)? {
        std::process::exit(code);
    }

    let sdl = sdl2::init().map_err(|e| anyhow::anyhow!(e))?;
    let video = sdl.video().map_err(|e| anyhow::anyhow!(e))?;

    let window = video
        .window("fuckFM", 1024, 680)
        .position_centered()
        .resizable()
        .build()
        .context("create window")?;

    let mut canvas = window.into_canvas().present_vsync().build()?;
    let mut fonts = FontAtlas::new(&canvas)?;
    let mut events = sdl.event_pump().map_err(|e| anyhow::anyhow!(e))?;

    let cli = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map(PathBuf::from);

    let (w, h) = canvas.output_size().map_err(|e| anyhow::anyhow!(e))?;
    let mut text_input = video.text_input();
    let sys_clip = video.clipboard();
    let mut app = App::new(Rect::new(8, 8, w as i32 - 16, h as i32 - 16), cli, sys_clip)?;
    app.run(&mut canvas, &mut fonts, &mut events, &mut text_input)
}

/// Returns `Some(exit_code)` when a CLI utility flag was handled.
fn handle_cli_flags(args: &[String]) -> Result<Option<i32>> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!(
            "fuckFM — file manager\n\
             Usage: fuckfm [path]\n\
             \n\
               --register-folder-handler    set as default folder opener (Windows HKCU)\n\
               --unregister-folder-handler  restore previous folder opener\n\
               --folder-handler-status      print whether fuckFM owns Directory open"
        );
        return Ok(Some(0));
    }
    if args.iter().any(|a| a == "--register-folder-handler") {
        let exe = crate::platform::assoc::default_fm_exe()?;
        crate::platform::assoc::register_folder_handler(&exe)?;
        println!("Registered folder handler: {}", exe.display());
        return Ok(Some(0));
    }
    if args.iter().any(|a| a == "--unregister-folder-handler") {
        crate::platform::assoc::unregister_folder_handler()?;
        println!("Unregistered folder handler (restored backups where present)");
        return Ok(Some(0));
    }
    if args.iter().any(|a| a == "--folder-handler-status") {
        let on = crate::platform::assoc::is_folder_handler_registered();
        println!("{}", if on { "registered" } else { "not registered" });
        return Ok(Some(0));
    }
    Ok(None)
}

fn location_key(loc: &Location) -> String {
    loc.session_key()
}

fn open_tab_from_key(key: &str, icon_scale: u8, show_hidden: bool) -> Result<Tab> {
    let mut tab = if key == THIS_PC {
        Tab::from_browser(Browser::open_this_pc()?)?
    } else if key == TRASH {
        let mut browser = Browser::open_this_pc()?;
        browser.goto_trash(false)?;
        Tab::from_browser(browser)?
    } else if key == NETWORK {
        let mut browser = Browser::open_this_pc()?;
        browser.goto_network(false, &[], &[])?;
        Tab::from_browser(browser)?
    } else if crate::platform::network::is_remote_uri(key) {
        match crate::platform::network::open_remote_uri(key, None, None) {
            Ok(m) => {
                let mut browser = Browser::open(&m.path)?;
                browser.location = Location::Remote {
                    uri: m.uri,
                    path: m.path,
                };
                Tab::from_browser(browser)?
            }
            Err(_) => Tab::open(PathBuf::from(key))?,
        }
    } else {
        Tab::open(PathBuf::from(key))?
    };
    tab.list.icon_scale = icon_scale;
    if show_hidden && !tab.browser.show_hidden {
        let _ = tab.browser.toggle_hidden();
    }
    tab.after_nav();
    Ok(tab)
}

fn toggle_readonly(path: &Path) -> Result<bool> {
    let mut attrs = attrs::get_win_attrs(path)?;
    attrs.readonly = !attrs.readonly;
    attrs::set_win_attrs(path, attrs)?;
    Ok(attrs.readonly)
}

fn toggle_hidden(path: &Path) -> Result<bool> {
    let mut attrs = attrs::get_win_attrs(path)?;
    attrs.hidden = !attrs.hidden;
    attrs::set_win_attrs(path, attrs)?;
    Ok(attrs.hidden)
}

fn parse_octal_mode(s: &str) -> std::result::Result<u32, String> {
    let t = s.trim().trim_start_matches('0');
    let t = if s.trim().starts_with('0') && s.trim().len() > 1 {
        s.trim()
    } else {
        t
    };
    let raw = if t.is_empty() { "0" } else { s.trim() };
    u32::from_str_radix(raw, 8).map_err(|_| format!("invalid octal mode: {s}"))
}

fn open_startup_tabs(
    settings: &Settings,
    cli_start: Option<PathBuf>,
) -> Result<(Vec<Tab>, usize)> {
    if let Some(path) = cli_start {
        let tab = open_tab_from_key(
            &path.display().to_string(),
            settings.icon_scale,
            settings.show_hidden_default,
        )?;
        return Ok((vec![tab], 0));
    }

    if settings.restore_tabs && !settings.session_tabs.is_empty() {
        let mut tabs = Vec::new();
        for key in &settings.session_tabs {
            match open_tab_from_key(key, settings.icon_scale, settings.show_hidden_default) {
                Ok(t) => tabs.push(t),
                Err(_) => continue,
            }
        }
        if !tabs.is_empty() {
            let active = settings.session_active.min(tabs.len() - 1);
            return Ok((tabs, active));
        }
    }

    let key = match settings.startup_location.as_str() {
        "this-pc" => THIS_PC.to_string(),
        "home" => home_dir().display().to_string(),
        _ => std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .display()
            .to_string(),
    };
    let tab = open_tab_from_key(&key, settings.icon_scale, settings.show_hidden_default)?;
    Ok((vec![tab], 0))
}