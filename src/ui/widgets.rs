use crate::render::{Frame, Rect};
use anyhow::Result;
use sdl2::pixels::Color;

#[derive(Clone, Debug)]
pub enum ButtonIcon {
    None,
    BackArrow,
    ForwardArrow,
    UpArrow,
}

#[derive(Clone, Debug)]
pub struct Button {
    pub rect: Rect,
    pub label: String,
    pub icon: ButtonIcon,
    pub pressed: bool,
    pub enabled: bool,
}

impl Button {
    pub fn new(rect: Rect, label: impl Into<String>) -> Self {
        Self {
            rect,
            label: label.into(),
            icon: ButtonIcon::None,
            pressed: false,
            enabled: true,
        }
    }

    pub fn with_icon(rect: Rect, icon: ButtonIcon) -> Self {
        Self {
            rect,
            label: String::new(),
            icon,
            pressed: false,
            enabled: true,
        }
    }

    pub fn hit(&self, x: i32, y: i32) -> bool {
        self.enabled && self.rect.contains(x, y)
    }

    pub fn draw(&self, frame: &mut Frame<'_>) -> Result<()> {
        let theme = frame.theme;
        if self.pressed {
            frame.sunken(self.rect, theme.button);
        } else {
            frame.raised(self.rect, theme.button);
            frame.button_sheen(self.rect);
        }

        let color = if self.enabled {
            theme.text
        } else {
            theme.text_disabled
        };
        let ox = if self.pressed { 1 } else { 0 };
        let oy = if self.pressed { 1 } else { 0 };

        match self.icon {
            ButtonIcon::BackArrow => draw_back_arrow(frame, self.rect, color, ox, oy),
            ButtonIcon::ForwardArrow => draw_forward_arrow(frame, self.rect, color, ox, oy),
            ButtonIcon::UpArrow => draw_up_arrow(frame, self.rect, color, ox, oy),
            ButtonIcon::None => {
                let text_w = frame.text_width(&self.label);
                let tx = self.rect.x + (self.rect.w - text_w) / 2 + ox;
                let ty =
                    self.rect.y + (self.rect.h - theme.font_size as i32) / 2 + oy;
                frame.text(&self.label, tx, ty, color)?;
            }
        }
        Ok(())
    }
}

fn draw_back_arrow(frame: &mut Frame<'_>, rect: Rect, color: Color, ox: i32, oy: i32) {
    let cx = rect.x + rect.w / 2 + ox;
    let cy = rect.y + rect.h / 2 + oy;
    // shaft
    for x in (cx - 6)..=(cx + 6) {
        frame.fill(Rect::new(x, cy - 1, 1, 3), color);
    }
    // head ◄
    for i in 0..6 {
        let x = cx - 6 + i;
        let h = i * 2 + 1;
        frame.fill(Rect::new(x, cy - h / 2, 1, h), color);
    }
}

fn draw_forward_arrow(frame: &mut Frame<'_>, rect: Rect, color: Color, ox: i32, oy: i32) {
    let cx = rect.x + rect.w / 2 + ox;
    let cy = rect.y + rect.h / 2 + oy;
    for x in (cx - 6)..=(cx + 6) {
        frame.fill(Rect::new(x, cy - 1, 1, 3), color);
    }
    for i in 0..6 {
        let x = cx + 6 - i;
        let h = i * 2 + 1;
        frame.fill(Rect::new(x, cy - h / 2, 1, h), color);
    }
}

fn draw_up_arrow(frame: &mut Frame<'_>, rect: Rect, color: Color, ox: i32, oy: i32) {
    let cx = rect.x + rect.w / 2 + ox;
    let cy = rect.y + rect.h / 2 + oy;
    // shaft
    for y in (cy - 6)..=(cy + 6) {
        frame.fill(Rect::new(cx - 1, y, 3, 1), color);
    }
    // head ▲
    for i in 0..6 {
        let y = cy - 6 + i;
        let w = i * 2 + 1;
        frame.fill(Rect::new(cx - w / 2, y, w, 1), color);
    }
}