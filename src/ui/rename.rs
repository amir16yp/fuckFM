use crate::render::{Frame, Rect};
use anyhow::Result;

/// Modal rename field shown over the file list.
pub struct RenameDialog {
    pub path: std::path::PathBuf,
    pub text: String,
    pub cursor: usize,
    pub rect: Rect,
}

impl RenameDialog {
    pub fn new(path: std::path::PathBuf, rect: Rect) -> Self {
        let text = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let cursor = text.len();
        Self {
            path,
            text,
            cursor,
            rect,
        }
    }

    pub fn insert(&mut self, s: &str) {
        let filtered: String = s
            .chars()
            .filter(|c| *c != '/' && *c != '\\' && *c != '\0')
            .collect();
        if filtered.is_empty() {
            return;
        }
        self.text.insert_str(self.cursor, &filtered);
        self.cursor += filtered.len();
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 || self.text.is_empty() {
            return;
        }
        let prev = prev_char_boundary(&self.text, self.cursor);
        self.text.replace_range(prev..self.cursor, "");
        self.cursor = prev;
    }

    pub fn delete_forward(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let next = next_char_boundary(&self.text, self.cursor);
        self.text.replace_range(self.cursor..next, "");
    }

    pub fn left(&mut self) {
        self.cursor = prev_char_boundary(&self.text, self.cursor);
    }

    pub fn right(&mut self) {
        self.cursor = next_char_boundary(&self.text, self.cursor);
    }

    pub fn draw(&self, frame: &mut Frame<'_>) -> Result<()> {
        let theme = frame.theme;
        frame.fill(self.rect, theme.window);
        frame.panel(self.rect, theme.chrome);

        frame.text( "Rename:", self.rect.x + 10, self.rect.y + 10, theme.text)?;

        let field = Rect::new(
            self.rect.x + 10,
            self.rect.y + 32,
            self.rect.w - 20,
            theme.row_height + 8,
        );
        frame.sunken(field, theme.sunken);
        frame.fill(field.shrink(2), theme.address_bar);

        let shown = truncate_end(frame, &self.text, field.w - 16);
        frame.text(
            &shown,
            field.x + 6,
            field.y + (field.h - theme.font_size as i32) / 2,
            theme.text,
        )?;

        let prefix: String = self
            .text
            .chars()
            .take(byte_to_char_index(&self.text, self.cursor))
            .collect();
        let caret_x = field.x + 6 + frame.text_width(&prefix).min(field.w - 12);
        frame.fill(
            Rect::new(caret_x, field.y + 4, 1, field.h - 8),
            theme.text,
        );

        frame.text(
            "Enter = OK   Esc = Cancel",
            self.rect.x + 10,
            self.rect.y + self.rect.h - 22,
            theme.text_disabled,
        )?;
        Ok(())
    }
}

fn prev_char_boundary(s: &str, idx: usize) -> usize {
    if idx == 0 {
        return 0;
    }
    let mut i = idx - 1;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn next_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let mut i = idx + 1;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn byte_to_char_index(s: &str, byte_idx: usize) -> usize {
    s.char_indices()
        .take_while(|(i, _)| *i < byte_idx)
        .count()
}

fn truncate_end(frame: &mut Frame<'_>, text: &str, max_w: i32) -> String {
    if frame.text_width(text) <= max_w {
        return text.to_string();
    }
    let mut out = text.to_string();
    while out.len() > 1 && frame.text_width(&format!("…{out}")) > max_w {
        let mut chars = out.chars();
        chars.next();
        out = chars.collect();
    }
    format!("…{out}")
}