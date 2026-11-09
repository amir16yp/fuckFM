//! Alt-Tab / Task View window switcher overlay — chrome follows ThemeId.

use crate::render::{lerp_color, Frame, Rect};
use crate::theme::ThemeId;
use anyhow::Result;
use sdl2::pixels::Color;

#[derive(Clone, Debug)]
pub struct SwitchEntry {
    pub id: u64,
    pub title: String,
    pub is_desktop: bool,
}

#[derive(Default)]
pub struct AltTab {
    pub open: bool,
    pub entries: Vec<SwitchEntry>,
    pub index: usize,
    pub rect: Rect,
}

impl AltTab {
    pub fn begin(&mut self, entries: Vec<SwitchEntry>) {
        self.entries = entries;
        if self.entries.is_empty() {
            self.open = false;
            return;
        }
        self.open = true;
        self.index = if self.entries.len() > 1 { 1 } else { 0 };
    }

    pub fn advance(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.index = (self.index + 1) % self.entries.len();
    }

    pub fn retreat(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.index = if self.index == 0 {
            self.entries.len() - 1
        } else {
            self.index - 1
        };
    }

    pub fn selected(&self) -> Option<&SwitchEntry> {
        self.entries.get(self.index)
    }

    pub fn close(&mut self) -> Option<SwitchEntry> {
        if !self.open {
            return None;
        }
        let sel = self.selected().cloned();
        self.open = false;
        self.entries.clear();
        sel
    }

    /// Dismiss without applying a switch.
    pub fn cancel(&mut self) {
        self.open = false;
        self.entries.clear();
    }

    pub fn index_at(&self, x: i32, y: i32) -> Option<usize> {
        if !self.open || self.entries.is_empty() {
            return None;
        }
        let inner = self.rect.shrink(2);
        let title_h = 22;
        let n = self.entries.len().max(1) as i32;
        let cell_w = ((inner.w - 16) / n).clamp(80, 140);
        let cell_h = 72;
        let y0 = inner.y + title_h + 10;
        for i in 0..self.entries.len() {
            let cx = inner.x + 8 + i as i32 * cell_w;
            let cell = Rect::new(cx, y0, cell_w - 6, cell_h);
            if cell.contains(x, y) {
                return Some(i);
            }
        }
        None
    }

    pub fn layout(&mut self, window: Rect) {
        let n = self.entries.len().max(1) as i32;
        let cell_w = 140;
        let pad = 16;
        let w = (n * cell_w + pad * 2).min(window.w - 40).max(200);
        let h = 140;
        self.rect = Rect::new(
            window.x + (window.w - w) / 2,
            window.y + (window.h - h) / 3,
            w,
            h,
        );
    }

    pub fn draw(&self, frame: &mut Frame<'_>, screen: Rect) -> Result<()> {
        if !self.open {
            return Ok(());
        }
        let theme = frame.theme;
        frame.modal_dim(screen);
        frame.panel(self.rect, theme.chrome);
        let inner = self.rect.shrink(2);
        frame.fill(inner, theme.window);

        let title_h = theme.title_height.max(18);
        let title_r = Rect::new(inner.x, inner.y, inner.w, title_h);
        frame.title_bar(title_r, true);
        frame.text(
            "Alt+Tab",
            title_r.x + 8,
            title_r.y + (title_h - theme.font_size as i32) / 2,
            theme.title.text,
        )?;

        let n = self.entries.len().max(1) as i32;
        let cell_w = ((inner.w - 16) / n).clamp(80, 140);
        let cell_h = 72;
        let y = title_r.y + title_h + 10;
        for (i, entry) in self.entries.iter().enumerate() {
            let x = inner.x + 8 + i as i32 * cell_w;
            let cell = Rect::new(x, y, cell_w - 6, cell_h);
            paint_switch_cell(frame, cell, i == self.index)?;
            let color = cell_text_color(theme.id, i == self.index);
            let label = if entry.is_desktop {
                "Desktop".into()
            } else {
                truncate(frame, &entry.title, cell.w - 10)
            };
            let tw = frame.text_width(&label);
            frame.text(
                &label,
                cell.x + (cell.w - tw) / 2,
                cell.y + (cell.h - theme.font_size as i32) / 2,
                color,
            )?;
        }
        Ok(())
    }
}

fn paint_switch_cell(frame: &mut Frame<'_>, cell: Rect, selected: bool) -> Result<()> {
    let theme = frame.theme;
    match theme.id {
        ThemeId::Win95 | ThemeId::Win98 => {
            if selected {
                frame.fill(cell, theme.selection);
            } else {
                frame.sunken(cell, theme.sunken);
                frame.fill(cell.shrink(2), theme.client);
            }
        }
        ThemeId::WinXpLuna => {
            if selected {
                for x in 0..cell.w {
                    let t = x as f32 / cell.w.max(1) as f32;
                    frame.fill(
                        Rect::new(cell.x + x, cell.y, 1, cell.h),
                        lerp_color(
                            Color::RGB(0x3A, 0x8A, 0xF0),
                            Color::RGB(0x1E, 0x58, 0xC0),
                            t,
                        ),
                    );
                }
                frame.flat_border(cell, Color::RGB(0xFF, 0xD0, 0x40));
            } else {
                frame.fill(cell, Color::RGB(0xEC, 0xE9, 0xD8));
                frame.flat_border(cell, theme.border);
            }
        }
        ThemeId::Win7Aero => {
            if selected {
                for y in 0..cell.h {
                    let t = y as f32 / cell.h.max(1) as f32;
                    frame.fill(
                        Rect::new(cell.x, cell.y + y, cell.w, 1),
                        lerp_color(
                            Color::RGB(0xC8, 0xE4, 0xF8),
                            Color::RGB(0x6A, 0xB0, 0xE0),
                            t,
                        ),
                    );
                }
                frame.flat_border(cell, Color::RGB(0x33, 0x99, 0xFF));
                frame.button_sheen(cell);
            } else {
                frame.fill(cell, Color::RGB(0xF4, 0xF8, 0xFC));
                frame.flat_border(cell, theme.border);
            }
        }
        ThemeId::ModernLight | ThemeId::ModernDark => {
            if selected {
                frame.fill(cell, theme.selection);
            } else {
                frame.fill(cell, theme.client);
                frame.flat_border(cell, theme.border);
            }
        }
    }
    Ok(())
}

fn cell_text_color(id: ThemeId, selected: bool) -> Color {
    match id {
        ThemeId::Win95 | ThemeId::Win98 => {
            if selected {
                Color::RGB(0xFF, 0xFF, 0xFF)
            } else {
                Color::RGB(0, 0, 0)
            }
        }
        ThemeId::WinXpLuna => {
            if selected {
                Color::RGB(0xFF, 0xFF, 0xFF)
            } else {
                Color::RGB(0, 0, 0)
            }
        }
        ThemeId::Win7Aero => Color::RGB(0x1A, 0x33, 0x55),
        ThemeId::ModernLight => {
            if selected {
                Color::RGB(0xFF, 0xFF, 0xFF)
            } else {
                Color::RGB(0x1F, 0x1F, 0x1F)
            }
        }
        ThemeId::ModernDark => Color::RGB(0xF0, 0xF0, 0xF0),
    }
}

fn truncate(frame: &mut Frame<'_>, text: &str, max_w: i32) -> String {
    if frame.text_width(text) <= max_w {
        return text.to_string();
    }
    let mut out = text.to_string();
    while out.len() > 1 && frame.text_width(&format!("{out}…")) > max_w {
        out.pop();
    }
    format!("{out}…")
}
