//! Copy/move progress dialog — bar styling follows the active ThemeId.

use crate::render::{lerp_color, Frame, Rect};
use crate::theme::ThemeId;
use crate::ui::Button;
use anyhow::Result;
use sdl2::pixels::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressHit {
    None,
    Cancel,
}

pub struct ProgressDialog {
    pub open: bool,
    pub rect: Rect,
    parent: Rect,
    title: String,
    detail: String,
    /// 0.0..=1.0
    pub fraction: f32,
    cancel: Button,
}

impl ProgressDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            rect: Rect::default(),
            parent: Rect::default(),
            title: String::new(),
            detail: String::new(),
            fraction: 0.0,
            cancel: Button::new(Rect::default(), "Cancel"),
        }
    }

    pub fn show(&mut self, parent: Rect, title: impl Into<String>) {
        self.open = true;
        self.parent = parent;
        self.title = title.into();
        self.detail.clear();
        self.fraction = 0.0;
        self.layout(parent);
    }

    pub fn update(&mut self, detail: impl Into<String>, done: usize, total: usize) {
        self.detail = detail.into();
        self.fraction = if total == 0 {
            0.0
        } else {
            (done as f32 / total as f32).clamp(0.0, 1.0)
        };
    }

    pub fn hide(&mut self) {
        self.open = false;
        self.fraction = 0.0;
        self.detail.clear();
    }

    fn layout(&mut self, parent: Rect) {
        self.rect = Rect::new(0, 0, 420, 150);
        self.rect.x = parent.x + (parent.w - self.rect.w) / 2;
        self.rect.y = parent.y + (parent.h - self.rect.h) / 2;
    }

    pub fn hit(&self, x: i32, y: i32) -> ProgressHit {
        if !self.open {
            return ProgressHit::None;
        }
        if self.cancel.hit(x, y) {
            ProgressHit::Cancel
        } else {
            ProgressHit::None
        }
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>) -> Result<()> {
        if !self.open {
            return Ok(());
        }
        self.layout(self.parent);
        let theme = frame.theme;
        frame.modal_dim(self.parent);
        frame.panel(self.rect, theme.chrome);
        let inner = self.rect.shrink(2);
        frame.fill(inner, theme.window);

        // Active title strip — same language as framed windows.
        let title_h = theme.title_height.max(18);
        let title_r = Rect::new(inner.x, inner.y, inner.w, title_h);
        frame.title_bar(title_r, true);
        let title_color = theme.title.text;
        frame.text(&self.title, title_r.x + 8, title_r.y + (title_h - theme.font_size as i32) / 2, title_color)?;

        let body_y = title_r.y + title_h + 10;
        frame.text(&self.detail, inner.x + 14, body_y, theme.text)?;

        let bar = Rect::new(
            inner.x + 14,
            body_y + theme.row_height + 10,
            inner.w - 28,
            progress_bar_height(theme.id),
        );
        draw_progress_bar(frame, bar, self.fraction)?;

        let btn_h = (theme.toolbar_height - 6).max(22);
        let btn_w = 88;
        let by = inner.y + inner.h - btn_h - 12;
        self.cancel.rect = Rect::new(inner.x + inner.w - btn_w - 12, by, btn_w, btn_h);
        self.cancel.label = "Cancel".into();
        self.cancel.draw(frame)?;
        Ok(())
    }
}

fn progress_bar_height(id: ThemeId) -> i32 {
    match id {
        ThemeId::Win95 | ThemeId::Win98 => 18,
        ThemeId::WinXpLuna => 16,
        ThemeId::Win7Aero => 16,
        ThemeId::ModernLight | ThemeId::ModernDark => 12,
    }
}

fn draw_progress_bar(frame: &mut Frame<'_>, bar: Rect, fraction: f32) -> Result<()> {
    let theme = frame.theme;
    let frac = fraction.clamp(0.0, 1.0);
    match theme.id {
        ThemeId::Win95 | ThemeId::Win98 => draw_classic_blocks(frame, bar, frac),
        ThemeId::WinXpLuna => draw_luna_bar(frame, bar, frac),
        ThemeId::Win7Aero => draw_aero_bar(frame, bar, frac),
        ThemeId::ModernLight | ThemeId::ModernDark => draw_modern_bar(frame, bar, frac),
    }
    Ok(())
}

/// Win95/98: sunken trough + discrete navy chunks (classic “blocks”).
fn draw_classic_blocks(frame: &mut Frame<'_>, bar: Rect, frac: f32) {
    let theme = frame.theme;
    frame.sunken(bar, theme.sunken);
    let inner = bar.shrink(2);
    if frac <= 0.0 || inner.w <= 0 {
        return;
    }
    let fill_w = ((inner.w as f32) * frac).round() as i32;
    let chunk = 8;
    let gap = 2;
    let mut x = inner.x;
    let end = inner.x + fill_w;
    while x < end {
        let w = chunk.min(end - x);
        if w > 0 {
            frame.fill(Rect::new(x, inner.y, w, inner.h), theme.selection);
        }
        x += chunk + gap;
    }
}

/// XP Luna: soft blue trough + continuous blue→cyan gradient fill.
fn draw_luna_bar(frame: &mut Frame<'_>, bar: Rect, frac: f32) {
    let theme = frame.theme;
    frame.sunken(bar, theme.sunken);
    let inner = bar.shrink(2);
    frame.fill(inner, Color::RGB(0xE6, 0xF0, 0xFA));
    if frac <= 0.0 || inner.w <= 0 {
        return;
    }
    let fill_w = ((inner.w as f32) * frac).round().max(1.0) as i32;
    let left = Color::RGB(0x2A, 0x6E, 0xBB);
    let right = Color::RGB(0x6C, 0xB3, 0xF0);
    for i in 0..fill_w {
        let t = i as f32 / fill_w.max(1) as f32;
        let c = lerp_color(left, right, t);
        frame.fill(Rect::new(inner.x + i, inner.y, 1, inner.h), c);
    }
    // Top highlight strip (Luna chrome shine) — solid mix, no alpha path.
    let shine_h = (inner.h / 3).max(1);
    for i in 0..fill_w {
        let t = i as f32 / fill_w.max(1) as f32;
        let base = lerp_color(left, right, t);
        let shine = lerp_color(base, Color::RGB(255, 255, 255), 0.35);
        frame.fill(Rect::new(inner.x + i, inner.y, 1, shine_h), shine);
    }
}

/// Win7 Aero: glossy accent fill with vertical sheen.
fn draw_aero_bar(frame: &mut Frame<'_>, bar: Rect, frac: f32) {
    let theme = frame.theme;
    frame.flat_border(bar, theme.border);
    let inner = bar.shrink(1);
    frame.fill(inner, Color::RGB(0xF0, 0xF4, 0xF8));
    if frac <= 0.0 || inner.w <= 0 {
        return;
    }
    let fill_w = ((inner.w as f32) * frac).round().max(1.0) as i32;
    let top = Color::RGB(0x6B, 0xB5, 0xF5);
    let mid = Color::RGB(0x2F, 0x7D, 0xD1);
    let bot = Color::RGB(0x1B, 0x5F, 0xB0);
    for y in 0..inner.h {
        let t = y as f32 / inner.h.max(1) as f32;
        let c = if t < 0.45 {
            lerp_color(top, mid, t / 0.45)
        } else {
            lerp_color(mid, bot, (t - 0.45) / 0.55)
        };
        frame.fill(Rect::new(inner.x, inner.y + y, fill_w, 1), c);
    }
    let shine_h = (inner.h / 4).max(1);
    for y in 0..shine_h {
        let t = y as f32 / shine_h.max(1) as f32;
        let base = lerp_color(top, mid, t * 0.5);
        let shine = lerp_color(base, Color::RGB(255, 255, 255), 0.45);
        frame.fill(Rect::new(inner.x, inner.y + y, fill_w, 1), shine);
    }
}

/// Modern: flat track + accent fill (selection color).
fn draw_modern_bar(frame: &mut Frame<'_>, bar: Rect, frac: f32) {
    let theme = frame.theme;
    let track = if theme.id == ThemeId::ModernDark {
        Color::RGB(0x3A, 0x3A, 0x3A)
    } else {
        Color::RGB(0xE0, 0xE0, 0xE0)
    };
    frame.fill(bar, track);
    if frac <= 0.0 || bar.w <= 0 {
        return;
    }
    let fill_w = ((bar.w as f32) * frac).round().max(1.0) as i32;
    frame.fill(Rect::new(bar.x, bar.y, fill_w, bar.h), theme.selection);
}
