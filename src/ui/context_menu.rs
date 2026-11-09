use crate::platform::ContextItem;
use crate::render::{Frame, Rect};
use anyhow::Result;

pub struct ContextMenu {
    pub rect: Rect,
    pub items: Vec<ContextItem>,
    pub hover: Option<usize>,
    row_height: i32,
}

impl ContextMenu {
    pub fn open(x: i32, y: i32, items: Vec<ContextItem>, row_height: i32) -> Self {
        let row_height = row_height.max(18);
        let width = 260;
        let height = items.len() as i32 * row_height + 6;
        Self {
            rect: Rect::new(x, y, width, height),
            items,
            hover: None,
            row_height,
        }
    }

    pub fn clamp_to(&mut self, bounds: Rect) {
        if self.rect.x + self.rect.w > bounds.x + bounds.w {
            self.rect.x = (bounds.x + bounds.w - self.rect.w).max(bounds.x);
        }
        if self.rect.y + self.rect.h > bounds.y + bounds.h {
            self.rect.y = (bounds.y + bounds.h - self.rect.h).max(bounds.y);
        }
        if self.rect.x < bounds.x {
            self.rect.x = bounds.x;
        }
        if self.rect.y < bounds.y {
            self.rect.y = bounds.y;
        }
    }

    pub fn index_at(&self, x: i32, y: i32) -> Option<usize> {
        if !self.rect.contains(x, y) {
            return None;
        }
        let rel = y - (self.rect.y + 3);
        if rel < 0 {
            return None;
        }
        let idx = (rel / self.row_height) as usize;
        if idx < self.items.len() {
            Some(idx)
        } else {
            None
        }
    }

    pub fn draw(&self, frame: &mut Frame<'_>) -> Result<()> {
        let theme = frame.theme;
        frame.panel(self.rect, theme.chrome);
        let inner = self.rect.shrink(2);
        frame.fill(inner, theme.window);

        for (i, item) in self.items.iter().enumerate() {
            let y = inner.y + 1 + i as i32 * self.row_height;
            let row = Rect::new(inner.x, y, inner.w, self.row_height);
            if item.separator_before && i > 0 {
                frame.fill(
                    Rect::new(inner.x + 4, y, inner.w - 8, 1),
                    theme.border,
                );
            }
            if self.hover == Some(i) {
                frame.fill(row, theme.selection);
            }
            let color = if self.hover == Some(i) {
                theme.selection_text
            } else {
                theme.text
            };
            frame.text(
                &item.label,
                row.x + 8,
                row.y + (self.row_height - theme.font_size as i32) / 2,
                color,
            )?;
        }
        Ok(())
    }
}