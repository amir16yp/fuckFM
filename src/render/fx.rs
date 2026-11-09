//! Draw helpers for [`crate::theme::EffectSet`] bits.

use super::draw::{fill_rect, hline, lerp_color, Rect};
use sdl2::pixels::Color;
use sdl2::render::{BlendMode, Canvas};
use sdl2::video::Window;

/// Soft multi-offset shadow under `rect` (draw *before* the panel).
pub fn soft_shadow(canvas: &mut Canvas<Window>, rect: Rect) {
    let _ = canvas.set_blend_mode(BlendMode::Blend);
    let layers = [
        (2, Color::RGBA(0, 0, 0, 50)),
        (4, Color::RGBA(0, 0, 0, 30)),
        (6, Color::RGBA(0, 0, 0, 14)),
    ];
    for (pad, color) in layers {
        fill_rect(
            canvas,
            Rect::new(rect.x + pad, rect.y + pad, rect.w, rect.h),
            color,
        );
    }
    let _ = canvas.set_blend_mode(BlendMode::None);
}

/// Vertical specular gloss over an already-painted title bar.
pub fn title_gloss(canvas: &mut Canvas<Window>, rect: Rect) {
    if rect.h < 4 || rect.w < 2 {
        return;
    }
    let _ = canvas.set_blend_mode(BlendMode::Blend);
    let band = (rect.h / 2).max(2);
    for i in 0..band {
        let t = 1.0 - (i as f32 / band as f32);
        let a = (90.0 * t * t) as u8;
        hline(
            canvas,
            rect.x,
            rect.x + rect.w - 1,
            rect.y + i,
            Color::RGBA(255, 255, 255, a),
        );
    }
    let _ = canvas.set_blend_mode(BlendMode::None);
}

/// Light vertical sheen on a raised button face (inner rect, past bevel).
pub fn button_sheen(canvas: &mut Canvas<Window>, rect: Rect) {
    let inner = rect.shrink(2);
    if inner.h < 4 || inner.w < 2 {
        return;
    }
    let _ = canvas.set_blend_mode(BlendMode::Blend);
    let band = (inner.h / 2).max(2);
    for i in 0..band {
        let t = 1.0 - (i as f32 / band as f32);
        let a = (55.0 * t) as u8;
        hline(
            canvas,
            inner.x,
            inner.x + inner.w - 1,
            inner.y + i,
            Color::RGBA(255, 255, 255, a),
        );
    }
    let _ = canvas.set_blend_mode(BlendMode::None);
}

/// Full-window dim behind a modal.
pub fn modal_dim(canvas: &mut Canvas<Window>, bounds: Rect) {
    let _ = canvas.set_blend_mode(BlendMode::Blend);
    fill_rect(canvas, bounds, Color::RGBA(0, 0, 0, 90));
    let _ = canvas.set_blend_mode(BlendMode::None);
}

/// Subtle top-edge glass highlight for Aero chrome faces.
pub fn glass_face_highlight(canvas: &mut Canvas<Window>, rect: Rect, face: Color) {
    if rect.h < 3 {
        return;
    }
    let shine = lerp_color(face, Color::RGB(255, 255, 255), 0.45);
    hline(canvas, rect.x + 1, rect.x + rect.w - 2, rect.y + 1, shine);
    let soft = lerp_color(face, Color::RGB(255, 255, 255), 0.2);
    hline(canvas, rect.x + 1, rect.x + rect.w - 2, rect.y + 2, soft);
}
