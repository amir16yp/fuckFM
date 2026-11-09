mod effects;
mod palettes;

pub use effects::EffectSet;
pub use palettes::{theme, Theme, ThemeId};

use sdl2::pixels::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bevel {
    pub face: Color,
    pub highlight: Color,
    pub shadow: Color,
    pub dark_shadow: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TitleBarStyle {
    pub active_left: Color,
    pub active_right: Color,
    pub inactive_left: Color,
    pub inactive_right: Color,
    pub text: Color,
    pub inactive_text: Color,
    pub gradient: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontKind {
    ClassicBitmap,
    Sans,
}