use crate::browser::{DirEntry, EntryKind, SortKey};
use crate::iconpack::{purpose, purpose_for_file, IconPack};
use crate::platform::human_bytes;
use crate::render::{Frame, Rect};
use anyhow::Result;
use std::collections::BTreeSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    List,
    Details,
    Icons,
}

impl ViewMode {
    pub fn next(self) -> Self {
        match self {
            ViewMode::List => ViewMode::Details,
            ViewMode::Details => ViewMode::Icons,
            ViewMode::Icons => ViewMode::List,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ViewMode::List => "List",
            ViewMode::Details => "Details",
            ViewMode::Icons => "Icons",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            ViewMode::List => "list",
            ViewMode::Details => "details",
            ViewMode::Icons => "icons",
        }
    }

    pub fn from_key(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "list" => Some(ViewMode::List),
            "details" => Some(ViewMode::Details),
            "icons" => Some(ViewMode::Icons),
            _ => None,
        }
    }
}

pub struct FileList {
    pub rect: Rect,
    pub scroll: i32,
    pub selected: BTreeSet<usize>,
    pub anchor: Option<usize>,
    pub hover: Option<usize>,
    pub view: ViewMode,
    /// 1..=4 — Icons view tile multiplier.
    pub icon_scale: u8,
    /// Details column width fractions (Name / Type / Size); Modified gets the remainder.
    pub col_name: f32,
    pub col_type: f32,
    pub col_size: f32,
}

impl FileList {
    pub fn new(rect: Rect) -> Self {
        Self {
            rect,
            scroll: 0,
            selected: BTreeSet::new(),
            anchor: None,
            hover: None,
            view: ViewMode::List,
            icon_scale: 1,
            col_name: 0.42,
            col_type: 0.16,
            col_size: 0.16,
        }
    }

    fn details_widths(&self, inner_w: i32) -> (i32, i32, i32, i32) {
        let name_w = ((inner_w as f32) * self.col_name).round() as i32;
        let type_w = ((inner_w as f32) * self.col_type).round() as i32;
        let size_w = ((inner_w as f32) * self.col_size).round() as i32;
        let used = name_w + type_w + size_w;
        let date_w = (inner_w - used).max(40);
        (name_w.max(60), type_w.max(40), size_w.max(40), date_w)
    }

    /// Hit-test Details column dividers (0=Name|Type, 1=Type|Size, 2=Size|Modified).
    pub fn column_divider_hit(&self, x: i32, y: i32, base_row: i32) -> Option<usize> {
        if self.view != ViewMode::Details || base_row <= 0 {
            return None;
        }
        let inner = self.rect.shrink(2);
        let header = Rect::new(inner.x, inner.y, inner.w, base_row);
        if !header.contains(x, y) {
            return None;
        }
        let (name_w, type_w, size_w, _) = self.details_widths(inner.w);
        let edges = [inner.x + name_w, inner.x + name_w + type_w, inner.x + name_w + type_w + size_w];
        for (i, edge) in edges.iter().enumerate() {
            if (x - edge).abs() <= 4 {
                return Some(i);
            }
        }
        None
    }

    /// Drag divider `which` by `delta_px` pixels.
    pub fn resize_column(&mut self, which: usize, delta_px: i32, inner_w: i32) {
        if inner_w <= 0 {
            return;
        }
        let d = delta_px as f32 / inner_w as f32;
        const MIN: f32 = 0.08;
        match which {
            0 => {
                let next = (self.col_name + d).clamp(MIN, 0.70);
                let grow = next - self.col_name;
                if self.col_type - grow >= MIN {
                    self.col_name = next;
                    self.col_type -= grow;
                }
            }
            1 => {
                let next = (self.col_type + d).clamp(MIN, 0.50);
                let grow = next - self.col_type;
                if self.col_size - grow >= MIN {
                    self.col_type = next;
                    self.col_size -= grow;
                }
            }
            2 => {
                let next = (self.col_size + d).clamp(MIN, 0.50);
                let rest = 1.0 - self.col_name - self.col_type - next;
                if rest >= MIN {
                    self.col_size = next;
                }
            }
            _ => {}
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }

    pub fn primary(&self) -> Option<usize> {
        self.anchor
            .filter(|i| self.selected.contains(i))
            .or_else(|| self.selected.iter().next().copied())
    }

    pub fn select_only(&mut self, index: usize) {
        self.selected.clear();
        self.selected.insert(index);
        self.anchor = Some(index);
    }

    pub fn toggle(&mut self, index: usize) {
        if !self.selected.remove(&index) {
            self.selected.insert(index);
        }
        self.anchor = Some(index);
    }

    pub fn select_range(&mut self, index: usize) {
        let start = self.anchor.unwrap_or(index);
        let (a, b) = if start <= index {
            (start, index)
        } else {
            (index, start)
        };
        self.selected.clear();
        for i in a..=b {
            self.selected.insert(i);
        }
        if self.anchor.is_none() {
            self.anchor = Some(start);
        }
    }

    pub fn invert_selection(&mut self, len: usize) {
        let mut next = BTreeSet::new();
        for i in 0..len {
            if !self.selected.contains(&i) {
                next.insert(i);
            }
        }
        self.selected = next;
        self.anchor = self.selected.iter().next().copied();
    }

    fn row_height(&self, base: i32) -> i32 {
        match self.view {
            ViewMode::List | ViewMode::Details => base,
            ViewMode::Icons => {
                let (_, cell_h, _) = self.icon_metrics(base);
                cell_h
            }
        }
    }

    fn icon_metrics(&self, base: i32) -> (i32, i32, i32) {
        // cell_w, cell_h, icon_size
        match self.view {
            ViewMode::Icons => {
                let scale = self.icon_scale.clamp(1, 4) as i32;
                let icon = (base * 2 * scale).max(32);
                let label_h = base + 4;
                (icon + 24 + 8 * scale, icon + label_h + 12, icon)
            }
            _ => (self.rect.w, base, base - 4),
        }
    }

    pub fn cols(&self, base: i32) -> i32 {
        if self.view != ViewMode::Icons {
            return 1;
        }
        let (cell_w, _, _) = self.icon_metrics(base);
        (self.rect.w / cell_w.max(1)).max(1)
    }

    pub fn visible_rows(&self, base_row: i32) -> i32 {
        let row_h = self.row_height(base_row);
        if row_h <= 0 {
            0
        } else if self.view == ViewMode::Icons {
            let (_, cell_h, _) = self.icon_metrics(base_row);
            self.rect.h / cell_h.max(1)
        } else {
            self.rect.h / row_h
        }
    }

    /// Click on Details column headers.
    pub fn header_hit(&self, x: i32, y: i32, base_row: i32) -> Option<SortKey> {
        if self.view != ViewMode::Details || base_row <= 0 {
            return None;
        }
        let inner = self.rect.shrink(2);
        let header = Rect::new(inner.x, inner.y, inner.w, base_row);
        if !header.contains(x, y) {
            return None;
        }
        let (name_w, type_w, size_w, _) = self.details_widths(inner.w);
        let rel = x - inner.x;
        if rel < name_w {
            Some(SortKey::Name)
        } else if rel < name_w + type_w {
            Some(SortKey::Type)
        } else if rel < name_w + type_w + size_w {
            Some(SortKey::Size)
        } else {
            Some(SortKey::Modified)
        }
    }

    pub fn index_at(&self, x: i32, y: i32, base_row: i32) -> Option<usize> {
        if !self.rect.contains(x, y) || base_row <= 0 {
            return None;
        }
        let inner = self.rect.shrink(2);
        if self.view == ViewMode::Icons {
            let (cell_w, cell_h, _) = self.icon_metrics(base_row);
            let cols = self.cols(base_row).max(1);
            let rel_x = x - inner.x;
            let rel_y = y - inner.y;
            if rel_x < 0 || rel_y < 0 {
                return None;
            }
            let col = rel_x / cell_w;
            let row = rel_y / cell_h + self.scroll;
            if col >= cols {
                return None;
            }
            Some((row * cols + col) as usize)
        } else {
            let header = if self.view == ViewMode::Details {
                base_row
            } else {
                0
            };
            let rel = y - inner.y - header;
            if rel < 0 {
                return None;
            }
            Some((rel / base_row + self.scroll) as usize)
        }
    }

    /// Indices whose item cells intersect `band` (screen coords).
    pub fn indices_in_rect(&self, band: Rect, len: usize, base_row: i32) -> Vec<usize> {
        let mut out = Vec::new();
        if len == 0 || base_row <= 0 || band.w <= 0 || band.h <= 0 {
            return out;
        }
        for i in 0..len {
            if let Some(r) = self.item_rect(i, base_row) {
                if rects_intersect(r, band) {
                    out.push(i);
                }
            }
        }
        out
    }

    pub fn item_rect(&self, index: usize, base_row: i32) -> Option<Rect> {
        if base_row <= 0 {
            return None;
        }
        let inner = self.rect.shrink(2);
        if self.view == ViewMode::Icons {
            let (cell_w, cell_h, _) = self.icon_metrics(base_row);
            let cols = self.cols(base_row).max(1) as usize;
            let row = (index / cols) as i32;
            let col = (index % cols) as i32;
            let y = inner.y + (row - self.scroll) * cell_h;
            if y + cell_h < inner.y || y > inner.y + inner.h {
                return None;
            }
            Some(Rect::new(
                inner.x + col * cell_w,
                y,
                cell_w - 4,
                cell_h - 4,
            ))
        } else {
            let header = if self.view == ViewMode::Details {
                base_row
            } else {
                0
            };
            let y = inner.y + header + (index as i32 - self.scroll) * base_row;
            if y + base_row < inner.y + header || y > inner.y + inner.h {
                return None;
            }
            Some(Rect::new(inner.x, y, inner.w, base_row))
        }
    }

    pub fn ensure_visible(&mut self, index: usize, base_row: i32) {
        let cols = self.cols(base_row).max(1) as usize;
        let row = if self.view == ViewMode::Icons {
            index / cols
        } else {
            index
        } as i32;
        let visible = self.visible_rows(base_row).max(1);
        if row < self.scroll {
            self.scroll = row;
        } else if row >= self.scroll + visible {
            self.scroll = row - visible + 1;
        }
        self.scroll = self.scroll.max(0);
    }

    pub fn clamp_scroll(&mut self, len: usize, base_row: i32) {
        let cols = self.cols(base_row).max(1) as usize;
        let rows = if self.view == ViewMode::Icons {
            len.div_ceil(cols)
        } else {
            len
        };
        let visible = self.visible_rows(base_row).max(1) as usize;
        let max_scroll = rows.saturating_sub(visible) as i32;
        self.scroll = self.scroll.clamp(0, max_scroll);
        self.selected.retain(|i| *i < len);
        if let Some(a) = self.anchor {
            if a >= len {
                self.anchor = self.primary();
            }
        }
    }

    pub fn draw(
        &self,
        frame: &mut Frame<'_>,
        entries: &[DirEntry],
        sort_key: SortKey,
        sort_asc: bool,
        thumbs: &mut crate::ui::thumbs::ThumbCache,
        icons: &IconPack,
    ) -> Result<()> {
        let theme = frame.theme;
        frame.sunken(self.rect, theme.sunken);
        let inner = self.rect.shrink(2);
        frame.fill(inner, theme.client);
        match self.view {
            ViewMode::List => {
                self.draw_list(frame, entries, inner, false, sort_key, sort_asc, icons)?
            }
            ViewMode::Details => {
                self.draw_list(frame, entries, inner, true, sort_key, sort_asc, icons)?
            }
            ViewMode::Icons => self.draw_icons(frame, entries, inner, thumbs, icons)?,
        }
        Ok(())
    }

    fn draw_list(
        &self,
        frame: &mut Frame<'_>,
        entries: &[DirEntry],
        inner: Rect,
        details: bool,
        sort_key: SortKey,
        sort_asc: bool,
        icons: &IconPack,
    ) -> Result<()> {
        let theme = frame.theme;
        let row_h = theme.row_height;
        let mut y0 = inner.y;
        let (name_w, type_w, size_w, _) = if details {
            self.details_widths(inner.w)
        } else {
            (inner.w, 0, 0, 0)
        };
        let type_x = inner.x + name_w;
        let size_x = type_x + type_w;
        let date_x = size_x + size_w;

        if details {
            let header = Rect::new(inner.x, inner.y, inner.w, row_h);
            frame.fill(header, theme.window);
            let marker = if sort_asc { " ▲" } else { " ▼" };
            let name = format!(
                "Name{}",
                if sort_key == SortKey::Name { marker } else { "" }
            );
            let ty = format!(
                "Type{}",
                if sort_key == SortKey::Type { marker } else { "" }
            );
            let size = format!(
                "Size{}",
                if sort_key == SortKey::Size { marker } else { "" }
            );
            let modified = format!(
                "Modified{}",
                if sort_key == SortKey::Modified {
                    marker
                } else {
                    ""
                }
            );
            frame.text(&name, inner.x + 8, inner.y + 2, theme.text)?;
            frame.text(&ty, type_x + 4, inner.y + 2, theme.text)?;
            frame.text(&size, size_x + 4, inner.y + 2, theme.text)?;
            frame.text(&modified, date_x + 4, inner.y + 2, theme.text)?;
            y0 += row_h;
        }

        let visible = ((inner.y + inner.h - y0) / row_h).max(0) as usize;
        let start = self.scroll as usize;
        let end = (start + visible + 1).min(entries.len());

        for (row, index) in (start..end).enumerate() {
            let entry = &entries[index];
            let y = y0 + row as i32 * row_h;
            let row_rect = Rect::new(inner.x, y, inner.w, row_h);
            paint_row(frame, row_rect, self.selected.contains(&index), self.hover == Some(index))?;

            let text_color = if self.selected.contains(&index) {
                theme.selection_text
            } else {
                theme.text
            };
            draw_icon(
                frame,
                inner.x + 4,
                y + 2,
                row_h - 4,
                entry.kind,
                &entry.path,
                theme,
                icons,
            )?;
            let label = entry_label(entry);
            let clipped = clip_text(frame, &label, name_w - row_h - 12);
            frame.text(
                &clipped,
                inner.x + row_h + 4,
                y + (row_h - theme.font_size as i32) / 2,
                text_color,
            )?;
            if details {
                let ty = clip_text(
                    frame,
                    &crate::browser::entry_type_label(entry),
                    type_w - 8,
                );
                frame.text(
                    &ty,
                    type_x + 4,
                    y + (row_h - theme.font_size as i32) / 2,
                    text_color,
                )?;
                let size = format_size(entry);
                let size_text_y = if matches!(entry.kind, EntryKind::Drive { .. }) {
                    y + 1
                } else {
                    y + (row_h - theme.font_size as i32) / 2
                };
                frame.text(&size, size_x + 4, size_text_y, text_color)?;
                draw_drive_usage_bar(
                    frame,
                    entry,
                    Rect::new(size_x + 4, y + row_h - 6, (size_w - 8).max(8), 3),
                )?;
                let modified = format_mtime(entry.modified);
                frame.text(
                    &modified,
                    date_x + 4,
                    y + (row_h - theme.font_size as i32) / 2,
                    text_color,
                )?;
            }
        }
        Ok(())
    }

    fn draw_icons(
        &self,
        frame: &mut Frame<'_>,
        entries: &[DirEntry],
        inner: Rect,
        thumbs: &mut crate::ui::thumbs::ThumbCache,
        icons: &IconPack,
    ) -> Result<()> {
        let theme = frame.theme;
        let (cell_w, cell_h, icon) = self.icon_metrics(theme.row_height);
        let cols = self.cols(theme.row_height).max(1);
        let start_row = self.scroll;
        let rows_vis = (inner.h / cell_h.max(1)) + 1;

        for row in start_row..start_row + rows_vis {
            for col in 0..cols {
                let index = (row * cols + col) as usize;
                if index >= entries.len() {
                    return Ok(());
                }
                let entry = &entries[index];
                let x = inner.x + col * cell_w;
                let y = inner.y + (row - start_row) * cell_h;
                let cell = Rect::new(x, y, cell_w - 4, cell_h - 4);
                paint_row(
                    frame,
                    cell,
                    self.selected.contains(&index),
                    self.hover == Some(index),
                )?;
                let ix = x + (cell_w - 4 - icon) / 2;
                let iy = y + 4;
                let drew_thumb = if matches!(entry.kind, EntryKind::File) {
                    if let Some(thumb) = thumbs.get_or_load(&entry.path, icon as u32) {
                        let scale = (icon as f32 / thumb.width.max(1) as f32)
                            .min(icon as f32 / thumb.height.max(1) as f32);
                        let dw = (thumb.width as f32 * scale).max(1.0) as u32;
                        let dh = (thumb.height as f32 * scale).max(1.0) as u32;
                        let dx = ix + (icon - dw as i32) / 2;
                        let dy = iy + (icon - dh as i32) / 2;
                        // checker / frame behind image
                        frame.fill(Rect::new(ix, iy, icon, icon), theme.client);
                        frame.flat_border(Rect::new(ix, iy, icon, icon), theme.border);
                        let _ = frame.fonts.blit_rgba(
                            frame.canvas,
                            dx,
                            dy,
                            thumb.width,
                            thumb.height,
                            dw,
                            dh,
                            &thumb.rgba,
                        );
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if !drew_thumb {
                    draw_icon(frame, ix, iy, icon, entry.kind, &entry.path, theme, icons)?;
                }
                let text_color = if self.selected.contains(&index) {
                    theme.selection_text
                } else {
                    theme.text
                };
                let label = clip_text(frame, &entry.name, cell_w - 10);
                let tw = frame.text_width(&label);
                let label_y = y + 8 + icon;
                frame.text(
                    &label,
                    x + (cell_w - 4 - tw) / 2,
                    label_y,
                    text_color,
                )?;
                if matches!(entry.kind, EntryKind::Drive { .. }) {
                    let bar_w = (cell_w - 16).min(icon + 8).max(24);
                    draw_drive_usage_bar(
                        frame,
                        entry,
                        Rect::new(
                            x + (cell_w - 4 - bar_w) / 2,
                            label_y + theme.font_size as i32 + 2,
                            bar_w,
                            4,
                        ),
                    )?;
                }
            }
        }
        Ok(())
    }
}

fn draw_drive_usage_bar(frame: &mut Frame<'_>, entry: &DirEntry, bar: Rect) -> Result<()> {
    let EntryKind::Drive {
        free_bytes,
        total_bytes,
    } = entry.kind
    else {
        return Ok(());
    };
    if total_bytes == 0 || bar.w <= 0 || bar.h <= 0 {
        return Ok(());
    }
    let theme = frame.theme;
    let used_frac = 1.0 - (free_bytes as f32 / total_bytes as f32);
    frame.fill(bar, theme.sunken.shadow);
    frame.flat_border(bar, theme.border);
    let fill_w = ((bar.w as f32 * used_frac).round() as i32).clamp(0, bar.w);
    if fill_w > 0 {
        let fill = if used_frac >= 0.92 {
            sdl2::pixels::Color::RGB(180, 40, 40)
        } else if used_frac >= 0.78 {
            sdl2::pixels::Color::RGB(180, 140, 30)
        } else {
            theme.selection
        };
        frame.fill(Rect::new(bar.x, bar.y, fill_w, bar.h), fill);
    }
    Ok(())
}

fn paint_row(
    frame: &mut Frame<'_>,
    row_rect: Rect,
    selected: bool,
    hover: bool,
) -> Result<()> {
    let theme = frame.theme;
    if selected {
        frame.fill(row_rect, theme.selection);
    } else if hover {
        frame.fill(
            row_rect,
            sdl2::pixels::Color::RGB(
                ((theme.client.r as u16 + theme.selection.r as u16) / 2) as u8,
                ((theme.client.g as u16 + theme.selection.g as u16) / 2) as u8,
                ((theme.client.b as u16 + theme.selection.b as u16) / 2) as u8,
            ),
        );
    }
    Ok(())
}

fn entry_label(entry: &DirEntry) -> String {
    match entry.kind {
        EntryKind::Drive { .. } => entry.name.clone(),
        EntryKind::Folder => format!("{}/", entry.name),
        EntryKind::File => entry.name.clone(),
    }
}

fn format_size(entry: &DirEntry) -> String {
    match entry.kind {
        EntryKind::Drive {
            free_bytes,
            total_bytes,
        } => {
            if total_bytes == 0 {
                String::new()
            } else {
                format!("{} free", human_bytes(free_bytes))
            }
        }
        EntryKind::Folder => String::new(),
        EntryKind::File => entry
            .size
            .map(human_bytes)
            .unwrap_or_default(),
    }
}

fn format_mtime(t: Option<SystemTime>) -> String {
    let Some(t) = t else {
        return String::new();
    };
    let Ok(dur) = t.duration_since(UNIX_EPOCH) else {
        return String::new();
    };
    // Compact UTC-ish stamp without chrono dependency.
    let secs = dur.as_secs();
    let days = secs / 86400;
    // 1970-01-01 + days — good enough preview; not a full calendar.
    let year = 1970 + (days / 365) as i32;
    let rem = days % 365;
    let month = (rem / 30 + 1).min(12);
    let day = (rem % 30 + 1).min(31);
    let tod = secs % 86400;
    let hour = tod / 3600;
    let min = (tod % 3600) / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{min:02}")
}

fn clip_text(frame: &mut Frame<'_>, text: &str, max_w: i32) -> String {
    if max_w <= 0 || frame.text_width(text) <= max_w {
        return text.to_string();
    }
    let mut out = text.to_string();
    while out.len() > 1 && frame.text_width(&format!("{out}…")) > max_w {
        out.pop();
        while !out.is_empty() && !out.is_char_boundary(out.len()) {
            out.pop();
        }
    }
    format!("{out}…")
}

fn purpose_for(kind: EntryKind, path: &Path) -> &'static str {
    match kind {
        EntryKind::Folder => purpose::FOLDER,
        EntryKind::Drive { .. } => purpose::DRIVE,
        EntryKind::File => purpose_for_file(path),
    }
}

fn blit_pack_icon(
    frame: &mut Frame<'_>,
    x: i32,
    y: i32,
    size: i32,
    purpose: &str,
    icons: &IconPack,
) -> bool {
    let Some(icon) = icons.resolve(purpose, size.max(1) as u32) else {
        return false;
    };
    let scale = (size as f32 / icon.width.max(1) as f32)
        .min(size as f32 / icon.height.max(1) as f32);
    let dw = (icon.width as f32 * scale).max(1.0) as u32;
    let dh = (icon.height as f32 * scale).max(1.0) as u32;
    let dx = x + (size - dw as i32) / 2;
    let dy = y + (size - dh as i32) / 2;
    frame
        .fonts
        .blit_rgba(
            frame.canvas,
            dx,
            dy,
            icon.width,
            icon.height,
            dw,
            dh,
            &icon.rgba,
        )
        .is_ok()
}

fn draw_icon(
    frame: &mut Frame<'_>,
    x: i32,
    y: i32,
    size: i32,
    kind: EntryKind,
    path: &Path,
    theme: crate::theme::Theme,
    icons: &IconPack,
) -> Result<()> {
    let rect = Rect::new(x, y, size, size);
    if blit_pack_icon(frame, x, y, size, purpose_for(kind, path), icons) {
        return Ok(());
    }
    match kind {
        EntryKind::Drive { .. } => {
            frame.fill(rect.shrink(1), theme.selection);
            frame.fill(
                Rect::new(x + 2, y + size / 2, size - 4, size / 2 - 2),
                theme.icon_file,
            );
            frame.flat_border(rect, theme.border);
        }
        EntryKind::Folder => {
            frame.fill(Rect::new(x, y + 2, size / 2, size / 3), theme.icon_folder);
            frame.fill(
                Rect::new(x, y + size / 3, size, size - size / 3),
                theme.icon_folder,
            );
            frame.flat_border(rect, theme.border);
        }
        EntryKind::File => {
            frame.fill(rect, theme.icon_file);
            frame.flat_border(rect, theme.border);
            frame.fill(
                Rect::new(x + size - size / 3, y, size / 3, size / 3),
                theme.client,
            );
        }
    }
    Ok(())
}

fn rects_intersect(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
}