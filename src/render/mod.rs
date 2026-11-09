mod draw;
mod font;
mod fx;

pub use draw::{fill_rect, lerp_color, Rect};
pub use font::FontAtlas;

use crate::theme::{Bevel, EffectSet, Theme};
use anyhow::Result;
use sdl2::render::Canvas;
use sdl2::video::Window;

pub struct Frame<'a> {
    pub canvas: &'a mut Canvas<Window>,
    pub fonts: &'a mut FontAtlas,
    pub theme: Theme,
    /// Theme effects already gated by `Settings::visual_effects`.
    pub effects: EffectSet,
}

impl Frame<'_> {
    pub fn clear_desktop(&mut self) {
        self.canvas.set_draw_color(self.theme.desktop);
        self.canvas.clear();
    }

    pub fn present(&mut self) {
        self.canvas.present();
    }

    pub fn fill(&mut self, rect: Rect, color: sdl2::pixels::Color) {
        fill_rect(self.canvas, rect, color);
    }

    pub fn raised(&mut self, rect: Rect, bevel: Bevel) {
        draw::raised_bevel(self.canvas, rect, bevel);
        if self.effects.glass_chrome {
            fx::glass_face_highlight(self.canvas, rect, bevel.face);
        }
    }

    /// Raised panel with optional soft drop shadow (dialogs, menus, start).
    pub fn panel(&mut self, rect: Rect, bevel: Bevel) {
        if self.effects.soft_shadow {
            fx::soft_shadow(self.canvas, rect);
        }
        self.raised(rect, bevel);
    }

    pub fn sunken(&mut self, rect: Rect, bevel: Bevel) {
        draw::sunken_bevel(self.canvas, rect, bevel);
    }

    pub fn flat_border(&mut self, rect: Rect, color: sdl2::pixels::Color) {
        draw::outline_rect(self.canvas, rect, color);
    }

    pub fn title_bar(&mut self, rect: Rect, active: bool) {
        draw::title_bar(self.canvas, rect, &self.theme, active, self.effects);
    }

    pub fn modal_dim(&mut self, bounds: Rect) {
        if self.effects.modal_dim {
            fx::modal_dim(self.canvas, bounds);
        }
    }

    pub fn button_sheen(&mut self, rect: Rect) {
        if self.effects.button_sheen {
            fx::button_sheen(self.canvas, rect);
        }
    }

    pub fn text(
        &mut self,
        text: &str,
        x: i32,
        y: i32,
        color: sdl2::pixels::Color,
    ) -> Result<(i32, i32)> {
        self.fonts.draw_text(
            self.canvas,
            self.theme.font,
            self.theme.font_size,
            text,
            x,
            y,
            color,
        )
    }

    pub fn text_width(&mut self, text: &str) -> i32 {
        self.fonts
            .measure(self.theme.font, self.theme.font_size, text)
            .0
    }
}