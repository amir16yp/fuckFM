//! Taskbar — Start, pinned fuckFM, clock.

use crate::render::{Frame, Rect};
use crate::ui::Button;
use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Taskbar {
    pub rect: Rect,
    pub start_btn: Button,
    pub fm_btn: Button,
    pub clock_rect: Rect,
    pub start_open: bool,
}

impl Taskbar {
    pub fn new() -> Self {
        Self {
            rect: Rect::default(),
            start_btn: Button::new(Rect::default(), "Start"),
            fm_btn: Button::new(Rect::default(), "fuckFM"),
            clock_rect: Rect::default(),
            start_open: false,
        }
    }

    pub fn layout(&mut self, window: Rect, height: i32) {
        self.rect = Rect::new(window.x, window.y + window.h - height, window.w, height);
        let pad = 3;
        let btn_h = height - pad * 2;
        self.start_btn.rect = Rect::new(self.rect.x + pad, self.rect.y + pad, 72, btn_h);
        self.start_btn.label = "Start".into();
        self.fm_btn.rect = Rect::new(
            self.start_btn.rect.x + self.start_btn.rect.w + 4,
            self.rect.y + pad,
            88,
            btn_h,
        );
        self.fm_btn.label = "fuckFM".into();
        let clock_w = 86;
        self.clock_rect = Rect::new(
            self.rect.x + self.rect.w - clock_w - pad,
            self.rect.y + pad,
            clock_w,
            btn_h,
        );
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>) -> Result<()> {
        let theme = frame.theme;
        frame.raised(self.rect, theme.chrome);
        frame.fill(self.rect.shrink(1), theme.chrome.face);

        self.start_btn.pressed = self.start_open;
        self.start_btn.draw(frame)?;
        self.fm_btn.pressed = false;
        self.fm_btn.draw(frame)?;

        frame.sunken(self.clock_rect, theme.sunken);
        let clock = current_clock();
        let tw = frame.text_width(&clock);
        frame.text(
            &clock,
            self.clock_rect.x + (self.clock_rect.w - tw) / 2,
            self.clock_rect.y + (self.clock_rect.h - theme.font_size as i32) / 2,
            theme.text,
        )?;
        Ok(())
    }
}

fn current_clock() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let offset = std::env::var("TZ_OFFSET_SECS")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    let local = secs as i64 + offset;
    let tod = ((local % 86_400) + 86_400) % 86_400;
    let h = tod / 3600;
    let m = (tod % 3600) / 60;
    format!("{h:02}:{m:02}")
}
