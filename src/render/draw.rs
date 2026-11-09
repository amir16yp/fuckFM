use crate::theme::{Bevel, EffectSet, Theme};
use sdl2::pixels::Color;
use sdl2::rect::Rect as SdlRect;
use sdl2::render::Canvas;
use sdl2::video::Window;

#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }

    pub fn shrink(self, amount: i32) -> Self {
        Self {
            x: self.x + amount,
            y: self.y + amount,
            w: (self.w - amount * 2).max(0),
            h: (self.h - amount * 2).max(0),
        }
    }

    pub fn to_sdl(self) -> SdlRect {
        SdlRect::new(self.x, self.y, self.w.max(0) as u32, self.h.max(0) as u32)
    }
}

pub fn fill_rect(canvas: &mut Canvas<Window>, rect: Rect, color: Color) {
    canvas.set_draw_color(color);
    let _ = canvas.fill_rect(rect.to_sdl());
}

pub fn outline_rect(canvas: &mut Canvas<Window>, rect: Rect, color: Color) {
    canvas.set_draw_color(color);
    let _ = canvas.draw_rect(rect.to_sdl());
}

pub fn hline(canvas: &mut Canvas<Window>, x1: i32, x2: i32, y: i32, color: Color) {
    canvas.set_draw_color(color);
    let _ = canvas.draw_line((x1, y), (x2, y));
}

pub fn vline(canvas: &mut Canvas<Window>, x: i32, y1: i32, y2: i32, color: Color) {
    canvas.set_draw_color(color);
    let _ = canvas.draw_line((x, y1), (x, y2));
}

pub fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    Color::RGB(
        (a.r as f32 * inv + b.r as f32 * t) as u8,
        (a.g as f32 * inv + b.g as f32 * t) as u8,
        (a.b as f32 * inv + b.b as f32 * t) as u8,
    )
}

pub fn raised_bevel(canvas: &mut Canvas<Window>, rect: Rect, bevel: Bevel) {
    fill_rect(canvas, rect, bevel.face);
    let x2 = rect.x + rect.w - 1;
    let y2 = rect.y + rect.h - 1;

    // outer highlight
    hline(canvas, rect.x, x2 - 1, rect.y, bevel.highlight);
    vline(canvas, rect.x, rect.y, y2 - 1, bevel.highlight);
    // outer shadow
    hline(canvas, rect.x, x2, y2, bevel.dark_shadow);
    vline(canvas, x2, rect.y, y2, bevel.dark_shadow);
    // inner shadow
    hline(canvas, rect.x + 1, x2 - 1, y2 - 1, bevel.shadow);
    vline(canvas, x2 - 1, rect.y + 1, y2 - 1, bevel.shadow);
}

pub fn sunken_bevel(canvas: &mut Canvas<Window>, rect: Rect, bevel: Bevel) {
    fill_rect(canvas, rect, bevel.face);
    let x2 = rect.x + rect.w - 1;
    let y2 = rect.y + rect.h - 1;

    hline(canvas, rect.x, x2, rect.y, bevel.shadow);
    vline(canvas, rect.x, rect.y, y2, bevel.shadow);
    hline(canvas, rect.x + 1, x2, rect.y + 1, bevel.dark_shadow);
    vline(canvas, rect.x + 1, rect.y + 1, y2, bevel.dark_shadow);

    hline(canvas, rect.x, x2, y2, bevel.highlight);
    vline(canvas, x2, rect.y, y2, bevel.highlight);
    hline(canvas, rect.x + 1, x2 - 1, y2 - 1, bevel.face);
    vline(canvas, x2 - 1, rect.y + 1, y2 - 1, bevel.face);
}

pub fn title_bar(
    canvas: &mut Canvas<Window>,
    rect: Rect,
    theme: &Theme,
    active: bool,
    effects: EffectSet,
) {
    let (left, right) = if active {
        (theme.title.active_left, theme.title.active_right)
    } else {
        (theme.title.inactive_left, theme.title.inactive_right)
    };

    let use_gradient = effects.title_gradient && theme.title.gradient && left != right && rect.w > 0;
    if use_gradient {
        for i in 0..rect.w {
            let t = i as f32 / rect.w.max(1) as f32;
            vline(
                canvas,
                rect.x + i,
                rect.y,
                rect.y + rect.h - 1,
                lerp_color(left, right, t),
            );
        }
    } else {
        fill_rect(canvas, rect, left);
    }

    if effects.title_gloss {
        super::fx::title_gloss(canvas, rect);
    }
}