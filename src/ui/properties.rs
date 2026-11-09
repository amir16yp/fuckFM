use crate::browser::{entry_type_label, DirEntry, EntryKind};
use crate::platform::human_bytes;
use crate::render::{Frame, Rect};
use anyhow::Result;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Docked details / properties pane (selection summary).
pub struct DetailsPane {
    pub rect: Rect,
    pub visible: bool,
    title: String,
    lines: Vec<(String, String)>,
}

impl DetailsPane {
    pub fn new() -> Self {
        Self {
            rect: Rect::default(),
            visible: false,
            title: String::new(),
            lines: Vec::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            self.lines.clear();
            self.title.clear();
        }
    }

    pub fn sync(&mut self, entries: &[DirEntry], folder_label: &str) {
        if !self.visible {
            return;
        }
        if entries.is_empty() {
            self.title = "Details".into();
            self.lines = vec![
                ("Selection".into(), "None".into()),
                ("Folder".into(), folder_label.to_string()),
            ];
        } else {
            let (title, lines) = build_property_lines(entries);
            self.title = title;
            self.lines = lines;
        }
    }

    pub fn draw(&self, frame: &mut Frame<'_>) -> Result<()> {
        if !self.visible || self.rect.w <= 0 || self.rect.h <= 0 {
            return Ok(());
        }
        let theme = frame.theme;
        frame.sunken(self.rect, theme.sunken);
        let inner = self.rect.shrink(2);
        frame.fill(inner, theme.window);

        let header_h = (theme.font_size as i32 + 8).max(20);
        frame.fill(
            Rect::new(inner.x, inner.y, inner.w, header_h),
            theme.status_bar,
        );
        let title = truncate_end(frame, &self.title, inner.w - 12);
        frame.text(
            &title,
            inner.x + 6,
            inner.y + (header_h - theme.font_size as i32) / 2,
            theme.text,
        )?;

        let label_w = 88;
        let line_h = theme.row_height.max(18);
        let cols = if inner.w >= 520 { 2 } else { 1 };
        let col_w = if cols == 2 {
            (inner.w - 12) / 2
        } else {
            inner.w - 8
        };
        for (i, (label, value)) in self.lines.iter().enumerate() {
            let col = (i % cols) as i32;
            let row = (i / cols) as i32;
            let x = inner.x + 6 + col * (col_w + 4);
            let y = inner.y + header_h + 4 + row * line_h;
            if y + line_h > inner.y + inner.h - 2 {
                break;
            }
            frame.text(label, x, y, theme.text_disabled)?;
            let val = truncate_end(frame, value, (col_w - label_w - 4).max(20));
            frame.text(&val, x + label_w, y, theme.text)?;
        }
        Ok(())
    }
}

/// Modal properties sheet for one or more selected items.
pub struct PropertiesDialog {
    pub open: bool,
    pub rect: Rect,
    title: String,
    lines: Vec<(String, String)>,
}

impl PropertiesDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            rect: Rect::default(),
            title: String::new(),
            lines: Vec::new(),
        }
    }

    pub fn show_entries(&mut self, entries: &[DirEntry], parent: Rect) {
        if entries.is_empty() {
            self.hide();
            return;
        }
        self.open = true;
        self.rect = Rect::new(0, 0, 420, 260);
        self.rect.x = parent.x + (parent.w - self.rect.w) / 2;
        self.rect.y = parent.y + (parent.h - self.rect.h) / 2;

        let (title, lines) = build_property_lines(entries);
        self.title = title;
        self.lines = lines;
        let line_h = 22;
        let needed = 48 + self.lines.len() as i32 * line_h + 40;
        self.rect.h = needed.max(200).min(parent.h - 20);
        self.rect.y = parent.y + (parent.h - self.rect.h) / 2;
    }

    pub fn hide(&mut self) {
        self.open = false;
        self.lines.clear();
    }

    pub fn hit_close(&self, x: i32, y: i32, title_h: i32) -> bool {
        if !self.open {
            return false;
        }
        close_btn_rect(self.rect, title_h).contains(x, y)
    }

    pub fn draw(&self, frame: &mut Frame<'_>) -> Result<()> {
        if !self.open {
            return Ok(());
        }
        let theme = frame.theme;
        frame.modal_dim(Rect::new(
            self.rect.x - 2000,
            self.rect.y - 2000,
            4000,
            4000,
        ));
        frame.panel(self.rect, theme.chrome);
        let inner = self.rect.shrink(2);
        frame.fill(inner, theme.window);

        let title = Rect::new(inner.x + 2, inner.y + 2, inner.w - 4, theme.title_height);
        frame.title_bar(title, true);
        let btn_s = theme.title_height - 4;
        let close = Rect::new(title.x + title.w - btn_s - 2, title.y + 2, btn_s, btn_s);
        frame.raised(close, theme.button);
        let x_w = frame.text_width("X");
        frame.text(
            "X",
            close.x + (close.w - x_w) / 2,
            close.y + (close.h - theme.font_size as i32) / 2,
            theme.text,
        )?;
        let title_text = truncate_end(frame, &self.title, title.w - btn_s - 20);
        frame.text(
            &title_text,
            title.x + 8,
            title.y + (theme.title_height - theme.font_size as i32) / 2,
            theme.title.text,
        )?;

        let mut y = title.y + theme.title_height + 12;
        let label_w = 90;
        let line_h = (theme.row_height + 4).max(20);
        for (label, value) in &self.lines {
            if y + line_h > inner.y + inner.h - 12 {
                break;
            }
            frame.text(label, inner.x + 14, y, theme.text_disabled)?;
            let val = truncate_end(frame, value, inner.w - label_w - 28);
            frame.text(&val, inner.x + 14 + label_w, y, theme.text)?;
            y += line_h;
        }
        Ok(())
    }
}

pub fn build_property_lines(entries: &[DirEntry]) -> (String, Vec<(String, String)>) {
    if entries.len() == 1 {
        let entry = &entries[0];
        let title = format!("Properties — {}", entry.name);
        let ty = entry_type_label(entry);
        let location = entry
            .path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "—".into());
        let size = size_line(entry);
        let modified = format_stamp(entry.modified);
        let (created, accessed) = extra_times(&entry.path);

        let mut lines = vec![
            ("Name".into(), entry.name.clone()),
            ("Type".into(), ty),
            ("Location".into(), location),
            ("Size".into(), size),
            ("Modified".into(), modified),
        ];
        if let Some(c) = created {
            lines.push(("Created".into(), c));
        }
        if let Some(a) = accessed {
            lines.push(("Accessed".into(), a));
        }
        lines.push(("Path".into(), entry.path.display().to_string()));
        (title, lines)
    } else {
        let title = format!("Properties — {} items", entries.len());
        let folders = entries.iter().filter(|e| e.is_dir).count();
        let files = entries.len() - folders;
        let mut total = 0u64;
        let mut sized = 0usize;
        for e in entries {
            if let Some(s) = e.size {
                total += s;
                sized += 1;
            }
        }
        let size = if sized == 0 {
            "—".into()
        } else if sized < entries.len() {
            format!(
                "{} ({} of {} with size)",
                human_bytes(total),
                sized,
                entries.len()
            )
        } else {
            human_bytes(total)
        };
        (
            title,
            vec![
                ("Items".into(), format!("{}", entries.len())),
                ("Folders".into(), format!("{folders}")),
                ("Files".into(), format!("{files}")),
                ("Total size".into(), size),
            ],
        )
    }
}

fn close_btn_rect(dialog: Rect, title_h: i32) -> Rect {
    let inner = dialog.shrink(2);
    let title = Rect::new(inner.x + 2, inner.y + 2, inner.w - 4, title_h);
    let btn_s = title_h - 4;
    Rect::new(title.x + title.w - btn_s - 2, title.y + 2, btn_s, btn_s)
}

fn size_line(entry: &DirEntry) -> String {
    match entry.kind {
        EntryKind::Drive {
            free_bytes,
            total_bytes,
        } => {
            if total_bytes == 0 {
                "—".into()
            } else {
                format!(
                    "{} free of {}",
                    human_bytes(free_bytes),
                    human_bytes(total_bytes)
                )
            }
        }
        EntryKind::Folder => match folder_byte_count(&entry.path, 2_000) {
            Some(n) => format!("{} (sampled)", human_bytes(n)),
            None => "Folder".into(),
        },
        EntryKind::File => entry.size.map(human_bytes).unwrap_or_else(|| "—".into()),
    }
}

/// Shallow recursive size with a hard entry cap (keeps UI snappy).
fn folder_byte_count(path: &Path, max_entries: usize) -> Option<u64> {
    let mut budget = max_entries;
    folder_byte_count_inner(path, &mut budget)
}

fn folder_byte_count_inner(path: &Path, budget: &mut usize) -> Option<u64> {
    if *budget == 0 {
        return Some(0);
    }
    let rd = fs::read_dir(path).ok()?;
    let mut total = 0u64;
    for item in rd.flatten() {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let p = item.path();
        let meta = match item.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_dir() {
            if let Some(n) = folder_byte_count_inner(&p, budget) {
                total += n;
            }
        } else {
            total += meta.len();
        }
    }
    Some(total)
}

fn extra_times(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(meta) = fs::metadata(path) else {
        return (None, None);
    };
    let created = meta.created().ok().map(format_stamp_val);
    let accessed = meta.accessed().ok().map(format_stamp_val);
    (created, accessed)
}

fn format_stamp(t: Option<SystemTime>) -> String {
    t.map(format_stamp_val).unwrap_or_else(|| "—".into())
}

fn format_stamp_val(t: SystemTime) -> String {
    let Ok(dur) = t.duration_since(UNIX_EPOCH) else {
        return "—".into();
    };
    let secs = dur.as_secs();
    let days = secs / 86400;
    let year = 1970 + (days / 365) as i32;
    let rem = days % 365;
    let month = (rem / 30 + 1).min(12);
    let day = (rem % 30 + 1).min(31);
    let tod = secs % 86400;
    let hour = tod / 3600;
    let min = (tod % 3600) / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{min:02}")
}

fn truncate_end(frame: &mut Frame<'_>, text: &str, max_w: i32) -> String {
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
