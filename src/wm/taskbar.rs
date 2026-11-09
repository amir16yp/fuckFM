//! Taskbar — Start, pins, running apps, Task View, tray, clock, show-desktop.
//! Visual chrome follows the active `ThemeId` (classic / Luna / Aero / modern).

use crate::render::{lerp_color, Frame, Rect};
use crate::theme::ThemeId;
use crate::ui::Button;
use anyhow::Result;
use sdl2::pixels::Color;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskbarHit {
    Start,
    PinFm,
    Running(usize),
    TaskView,
    Widgets,
    TrayChevron,
    TrayNet,
    TrayVol,
    TrayBat,
    Clock,
    ShowDesktop,
}

#[derive(Clone, Debug)]
pub struct RunningApp {
    pub id: u64,
    pub title: String,
    pub minimized: bool,
    pub badge: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flyout {
    None,
    JumpList,
    Calendar,
    QuickSettings,
    Widgets,
    TaskView,
}

pub struct Taskbar {
    pub rect: Rect,
    pub start_btn: Button,
    pub fm_btn: Button,
    pub task_view_btn: Button,
    pub widgets_btn: Button,
    pub running: Vec<RunningApp>,
    pub running_rects: Vec<Rect>,
    pub tray_chevron: Rect,
    pub tray_net: Rect,
    pub tray_vol: Rect,
    pub tray_bat: Rect,
    pub clock_rect: Rect,
    pub show_desktop: Rect,
    pub start_open: bool,
    pub flyout: Flyout,
    pub jump_for: Option<u64>,
    pub net_label: String,
    pub vol_label: String,
    pub bat_label: String,
    pub next_id: u64,
    pub active_running: Option<u64>,
}

impl Taskbar {
    pub fn new() -> Self {
        Self {
            rect: Rect::default(),
            start_btn: Button::new(Rect::default(), "Start"),
            fm_btn: Button::new(Rect::default(), "fuckFM"),
            task_view_btn: Button::new(Rect::default(), "TV"),
            widgets_btn: Button::new(Rect::default(), "Wid"),
            running: Vec::new(),
            running_rects: Vec::new(),
            tray_chevron: Rect::default(),
            tray_net: Rect::default(),
            tray_vol: Rect::default(),
            tray_bat: Rect::default(),
            clock_rect: Rect::default(),
            show_desktop: Rect::default(),
            start_open: false,
            flyout: Flyout::None,
            jump_for: None,
            net_label: "Net".into(),
            vol_label: "Vol".into(),
            bat_label: "Bat".into(),
            next_id: 1,
            active_running: None,
        }
    }

    pub fn upsert_running(&mut self, id: u64, title: impl Into<String>) {
        if let Some(app) = self.running.iter_mut().find(|a| a.id == id) {
            app.title = title.into();
            app.minimized = false;
            return;
        }
        self.running.push(RunningApp {
            id,
            title: title.into(),
            minimized: false,
            badge: 0,
        });
    }

    pub fn spawn_running(&mut self, title: impl Into<String>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.running.push(RunningApp {
            id,
            title: title.into(),
            minimized: false,
            badge: 0,
        });
        self.active_running = Some(id);
        id
    }

    pub fn remove_running(&mut self, id: u64) {
        self.running.retain(|a| a.id != id);
        if self.active_running == Some(id) {
            self.active_running = self.running.last().map(|a| a.id);
        }
    }

    pub fn set_minimized(&mut self, id: u64, minimized: bool) {
        if let Some(app) = self.running.iter_mut().find(|a| a.id == id) {
            app.minimized = minimized;
            if !minimized {
                self.active_running = Some(id);
            }
        }
    }

    pub fn minimize_all(&mut self) {
        for app in &mut self.running {
            app.minimized = true;
        }
        self.active_running = None;
    }

    pub fn restore_all(&mut self) {
        for app in &mut self.running {
            app.minimized = false;
        }
        self.active_running = self.running.last().map(|a| a.id);
    }

    pub fn refresh_tray_status(&mut self) {
        self.net_label = probe_network();
        self.vol_label = "Vol".into();
        self.bat_label = probe_battery();
    }

    pub fn layout(&mut self, window: Rect, height: i32) {
        self.rect = Rect::new(window.x, window.y + window.h - height, window.w, height);
        let pad = 3;
        let btn_h = height - pad * 2;
        let start_w = match /* filled at draw via theme; layout uses generous */ 0 {
            _ => 72,
        };
        self.start_btn.rect = Rect::new(self.rect.x + pad, self.rect.y + pad, start_w, btn_h);
        self.start_btn.label = "Start".into();
        self.fm_btn.rect = Rect::new(
            self.start_btn.rect.x + self.start_btn.rect.w + 4,
            self.rect.y + pad,
            88,
            btn_h,
        );
        self.fm_btn.label = "fuckFM".into();
        self.task_view_btn.rect = Rect::new(
            self.fm_btn.rect.x + self.fm_btn.rect.w + 4,
            self.rect.y + pad,
            36,
            btn_h,
        );
        self.task_view_btn.label = "TV".into();
        self.widgets_btn.rect = Rect::new(
            self.task_view_btn.rect.x + self.task_view_btn.rect.w + 4,
            self.rect.y + pad,
            40,
            btn_h,
        );
        self.widgets_btn.label = "Wid".into();

        let show_w = match height {
            h if h >= 40 => 8,
            _ => 6,
        };
        self.show_desktop = Rect::new(
            self.rect.x + self.rect.w - show_w,
            self.rect.y,
            show_w,
            height,
        );
        let clock_w = 86;
        self.clock_rect = Rect::new(
            self.show_desktop.x - clock_w - 4,
            self.rect.y + pad,
            clock_w,
            btn_h,
        );
        let tray_w = 36;
        self.tray_bat = Rect::new(
            self.clock_rect.x - tray_w - 2,
            self.rect.y + pad,
            tray_w,
            btn_h,
        );
        self.tray_vol = Rect::new(
            self.tray_bat.x - tray_w - 2,
            self.rect.y + pad,
            tray_w,
            btn_h,
        );
        self.tray_net = Rect::new(
            self.tray_vol.x - tray_w - 2,
            self.rect.y + pad,
            tray_w,
            btn_h,
        );
        self.tray_chevron = Rect::new(self.tray_net.x - 22, self.rect.y + pad, 20, btn_h);

        let run_left = self.widgets_btn.rect.x + self.widgets_btn.rect.w + 8;
        let run_right = self.tray_chevron.x - 8;
        let avail = (run_right - run_left).max(0);
        let n = self.running.len().max(1);
        let each = (avail / n as i32).clamp(56, 120);
        self.running_rects.clear();
        let mut x = run_left;
        for _ in &self.running {
            if x + each > run_right {
                break;
            }
            self.running_rects
                .push(Rect::new(x, self.rect.y + pad, each - 4, btn_h));
            x += each;
        }
    }

    pub fn hit(&self, x: i32, y: i32) -> Option<TaskbarHit> {
        if !self.rect.contains(x, y) {
            return None;
        }
        if self.start_btn.hit(x, y) {
            return Some(TaskbarHit::Start);
        }
        if self.fm_btn.hit(x, y) {
            return Some(TaskbarHit::PinFm);
        }
        if self.task_view_btn.hit(x, y) {
            return Some(TaskbarHit::TaskView);
        }
        if self.widgets_btn.hit(x, y) {
            return Some(TaskbarHit::Widgets);
        }
        for (i, r) in self.running_rects.iter().enumerate() {
            if r.contains(x, y) {
                return Some(TaskbarHit::Running(i));
            }
        }
        if self.tray_chevron.contains(x, y) {
            return Some(TaskbarHit::TrayChevron);
        }
        if self.tray_net.contains(x, y) {
            return Some(TaskbarHit::TrayNet);
        }
        if self.tray_vol.contains(x, y) {
            return Some(TaskbarHit::TrayVol);
        }
        if self.tray_bat.contains(x, y) {
            return Some(TaskbarHit::TrayBat);
        }
        if self.clock_rect.contains(x, y) {
            return Some(TaskbarHit::Clock);
        }
        if self.show_desktop.contains(x, y) {
            return Some(TaskbarHit::ShowDesktop);
        }
        None
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>) -> Result<()> {
        let theme = frame.theme;
        paint_taskbar_face(frame, self.rect)?;

        draw_start_button(frame, self.start_btn.rect, self.start_open)?;
        draw_pin_button(frame, self.fm_btn.rect, &self.fm_btn.label, false)?;
        draw_tool_button(
            frame,
            self.task_view_btn.rect,
            "TV",
            self.flyout == Flyout::TaskView,
        )?;
        draw_tool_button(
            frame,
            self.widgets_btn.rect,
            "Wid",
            self.flyout == Flyout::Widgets,
        )?;

        for (i, app) in self.running.iter().enumerate() {
            let Some(r) = self.running_rects.get(i).copied() else {
                break;
            };
            let active = self.active_running == Some(app.id) && !app.minimized;
            draw_running_button(frame, r, &app.title, active, app.minimized, app.badge)?;
        }

        draw_tray_chip(frame, self.tray_chevron, "^")?;
        draw_tray_chip(frame, self.tray_net, &self.net_label)?;
        draw_tray_chip(frame, self.tray_vol, &self.vol_label)?;
        draw_tray_chip(frame, self.tray_bat, &self.bat_label)?;
        draw_clock(frame, self.clock_rect, &current_clock())?;
        draw_show_desktop_strip(frame, self.show_desktop)?;
        let _ = theme;
        Ok(())
    }

    pub fn flyout_rect(&self, kind: Flyout) -> Rect {
        let w = match kind {
            Flyout::Calendar => 268,
            Flyout::QuickSettings => 300,
            Flyout::Widgets => 300,
            Flyout::JumpList => 220,
            Flyout::TaskView => 360,
            Flyout::None => 0,
        };
        let h = match kind {
            Flyout::Calendar => 248,
            Flyout::QuickSettings => 220,
            Flyout::Widgets => 200,
            Flyout::JumpList => 150,
            Flyout::TaskView => 180,
            Flyout::None => 0,
        };
        let x = match kind {
            Flyout::JumpList => self.fm_btn.rect.x,
            Flyout::Widgets => self.widgets_btn.rect.x,
            Flyout::TaskView => self.task_view_btn.rect.x,
            _ => (self.clock_rect.x + self.clock_rect.w - w).max(self.rect.x + 4),
        };
        Rect::new(x, self.rect.y - h - 4, w, h)
    }
}

fn paint_taskbar_face(frame: &mut Frame<'_>, rect: Rect) -> Result<()> {
    let theme = frame.theme;
    match theme.id {
        ThemeId::Win95 | ThemeId::Win98 => {
            frame.raised(rect, theme.chrome);
            frame.fill(rect.shrink(1), theme.chrome.face);
        }
        ThemeId::WinXpLuna => {
            // Classic Luna blue taskbar: vertical blue→lighter blue.
            let top = Color::RGB(0x24, 0x5E, 0xDB);
            let bot = Color::RGB(0x19, 0x3E, 0x9E);
            for y in 0..rect.h {
                let t = y as f32 / rect.h.max(1) as f32;
                frame.fill(
                    Rect::new(rect.x, rect.y + y, rect.w, 1),
                    lerp_color(top, bot, t),
                );
            }
            // Top shine edge
            frame.fill(
                Rect::new(rect.x, rect.y, rect.w, 1),
                Color::RGB(0x6A, 0x9A, 0xF0),
            );
        }
        ThemeId::Win7Aero => {
            let top = Color::RGB(0xE8, 0xF0, 0xF8);
            let mid = Color::RGB(0xC0, 0xD4, 0xE8);
            let bot = Color::RGB(0x8A, 0xB0, 0xD0);
            for y in 0..rect.h {
                let t = y as f32 / rect.h.max(1) as f32;
                let c = if t < 0.4 {
                    lerp_color(top, mid, t / 0.4)
                } else {
                    lerp_color(mid, bot, (t - 0.4) / 0.6)
                };
                frame.fill(Rect::new(rect.x, rect.y + y, rect.w, 1), c);
            }
            frame.fill(
                Rect::new(rect.x, rect.y, rect.w, 1),
                Color::RGB(0xFF, 0xFF, 0xFF),
            );
            if frame.effects.glass_chrome {
                frame.button_sheen(rect);
            }
        }
        ThemeId::ModernLight => {
            frame.fill(rect, Color::RGB(0xF3, 0xF3, 0xF3));
            frame.fill(
                Rect::new(rect.x, rect.y, rect.w, 1),
                Color::RGB(0xD0, 0xD0, 0xD0),
            );
        }
        ThemeId::ModernDark => {
            frame.fill(rect, Color::RGB(0x1F, 0x1F, 0x1F));
            frame.fill(
                Rect::new(rect.x, rect.y, rect.w, 1),
                Color::RGB(0x3A, 0x3A, 0x3A),
            );
        }
    }
    Ok(())
}

fn draw_start_button(frame: &mut Frame<'_>, rect: Rect, pressed: bool) -> Result<()> {
    let theme = frame.theme;
    match theme.id {
        ThemeId::Win95 | ThemeId::Win98 => {
            if pressed {
                frame.sunken(rect, theme.button);
            } else {
                frame.raised(rect, theme.button);
            }
            // Classic Start: bold label, optional Win flag block.
            let flag = Rect::new(rect.x + 5, rect.y + (rect.h - 12) / 2, 12, 12);
            frame.fill(flag, Color::RGB(0x00, 0x80, 0x00));
            frame.fill(
                Rect::new(flag.x + 1, flag.y + 1, 5, 5),
                Color::RGB(0xFF, 0x00, 0x00),
            );
            frame.fill(
                Rect::new(flag.x + 6, flag.y + 1, 5, 5),
                Color::RGB(0x00, 0x00, 0xFF),
            );
            frame.fill(
                Rect::new(flag.x + 1, flag.y + 6, 5, 5),
                Color::RGB(0xFF, 0xFF, 0x00),
            );
            frame.fill(
                Rect::new(flag.x + 6, flag.y + 6, 5, 5),
                Color::RGB(0x00, 0xFF, 0x00),
            );
            frame.text(
                "Start",
                rect.x + 22,
                rect.y + (rect.h - theme.font_size as i32) / 2,
                theme.text,
            )?;
        }
        ThemeId::WinXpLuna => {
            // Green Luna Start pill.
            let left = if pressed {
                Color::RGB(0x2A, 0x7A, 0x2A)
            } else {
                Color::RGB(0x3C, 0xB3, 0x3C)
            };
            let right = if pressed {
                Color::RGB(0x1E, 0x5A, 0x1E)
            } else {
                Color::RGB(0x2A, 0x8A, 0x2A)
            };
            fill_hgrad(frame, rect, left, right);
            let shine_h = (rect.h / 3).max(1);
            for y in 0..shine_h {
                let t = y as f32 / shine_h as f32;
                let c = lerp_color(Color::RGB(0xA0, 0xE8, 0xA0), left, t);
                frame.fill(Rect::new(rect.x + 1, rect.y + y, rect.w - 2, 1), c);
            }
            frame.flat_border(rect, Color::RGB(0x1A, 0x5A, 0x1A));
            frame.text(
                "start",
                rect.x + 14,
                rect.y + (rect.h - theme.font_size as i32) / 2,
                Color::RGB(0xFF, 0xFF, 0xFF),
            )?;
        }
        ThemeId::Win7Aero => {
            // Pearl orb-ish Start region.
            let face = if pressed {
                Color::RGB(0xB0, 0xC8, 0xE0)
            } else {
                Color::RGB(0xE8, 0xF2, 0xFA)
            };
            frame.fill(rect, face);
            frame.flat_border(rect, theme.border);
            frame.button_sheen(rect);
            frame.text(
                "Start",
                rect.x + 14,
                rect.y + (rect.h - theme.font_size as i32) / 2,
                theme.title.text,
            )?;
        }
        ThemeId::ModernLight | ThemeId::ModernDark => {
            let face = if pressed {
                theme.selection
            } else if theme.id == ThemeId::ModernDark {
                Color::RGB(0x2D, 0x2D, 0x2D)
            } else {
                Color::RGB(0xE8, 0xE8, 0xE8)
            };
            frame.fill(rect, face);
            let color = if pressed {
                theme.selection_text
            } else {
                theme.text
            };
            frame.text(
                "Start",
                rect.x + 16,
                rect.y + (rect.h - theme.font_size as i32) / 2,
                color,
            )?;
        }
    }
    Ok(())
}

fn draw_pin_button(frame: &mut Frame<'_>, rect: Rect, label: &str, pressed: bool) -> Result<()> {
    draw_tool_button(frame, rect, label, pressed)
}

fn draw_tool_button(frame: &mut Frame<'_>, rect: Rect, label: &str, pressed: bool) -> Result<()> {
    let theme = frame.theme;
    match theme.id {
        ThemeId::Win95 | ThemeId::Win98 => {
            if pressed {
                frame.sunken(rect, theme.button);
            } else {
                frame.raised(rect, theme.button);
            }
            let tw = frame.text_width(label);
            frame.text(
                label,
                rect.x + (rect.w - tw) / 2,
                rect.y + (rect.h - theme.font_size as i32) / 2,
                theme.text,
            )?;
        }
        ThemeId::WinXpLuna => {
            if pressed {
                fill_hgrad(
                    frame,
                    rect,
                    Color::RGB(0x1E, 0x4A, 0xA8),
                    Color::RGB(0x3A, 0x7A, 0xD8),
                );
            } else {
                fill_hgrad(
                    frame,
                    rect,
                    Color::RGB(0x3A, 0x78, 0xD8),
                    Color::RGB(0x2A, 0x58, 0xB8),
                );
            }
            frame.flat_border(rect, Color::RGB(0x14, 0x38, 0x88));
            let tw = frame.text_width(label);
            frame.text(
                label,
                rect.x + (rect.w - tw) / 2,
                rect.y + (rect.h - theme.font_size as i32) / 2,
                Color::RGB(0xFF, 0xFF, 0xFF),
            )?;
        }
        ThemeId::Win7Aero => {
            if pressed {
                frame.fill(rect, Color::RGB(0xB8, 0xD0, 0xE8));
            } else {
                frame.fill(rect, Color::RGB(0xF0, 0xF6, 0xFC));
            }
            frame.flat_border(rect, theme.border);
            frame.button_sheen(rect);
            let tw = frame.text_width(label);
            frame.text(
                label,
                rect.x + (rect.w - tw) / 2,
                rect.y + (rect.h - theme.font_size as i32) / 2,
                theme.text,
            )?;
        }
        ThemeId::ModernLight | ThemeId::ModernDark => {
            if pressed {
                frame.fill(rect, theme.selection);
            } else {
                frame.fill(rect, theme.button.face);
            }
            let color = if pressed {
                theme.selection_text
            } else {
                theme.text
            };
            let tw = frame.text_width(label);
            frame.text(
                label,
                rect.x + (rect.w - tw) / 2,
                rect.y + (rect.h - theme.font_size as i32) / 2,
                color,
            )?;
        }
    }
    Ok(())
}

fn draw_running_button(
    frame: &mut Frame<'_>,
    rect: Rect,
    title: &str,
    active: bool,
    minimized: bool,
    badge: u32,
) -> Result<()> {
    let theme = frame.theme;
    let label = truncate(frame, title, rect.w - 12);
    match theme.id {
        ThemeId::Win95 | ThemeId::Win98 => {
            if active && !minimized {
                frame.sunken(rect, theme.button);
            } else {
                frame.raised(rect, theme.button);
            }
            let tw = frame.text_width(&label);
            frame.text(
                &label,
                rect.x + (rect.w - tw) / 2,
                rect.y + (rect.h - theme.font_size as i32) / 2,
                theme.text,
            )?;
        }
        ThemeId::WinXpLuna => {
            if active && !minimized {
                fill_hgrad(
                    frame,
                    rect,
                    Color::RGB(0x3A, 0x8A, 0xF0),
                    Color::RGB(0x1E, 0x58, 0xC0),
                );
            } else if minimized {
                fill_hgrad(
                    frame,
                    rect,
                    Color::RGB(0x4A, 0x6A, 0xB0),
                    Color::RGB(0x2A, 0x48, 0x88),
                );
            } else {
                fill_hgrad(
                    frame,
                    rect,
                    Color::RGB(0x4A, 0x88, 0xE0),
                    Color::RGB(0x2E, 0x60, 0xB8),
                );
            }
            frame.flat_border(rect, Color::RGB(0x10, 0x30, 0x78));
            let tw = frame.text_width(&label);
            frame.text(
                &label,
                rect.x + (rect.w - tw) / 2,
                rect.y + (rect.h - theme.font_size as i32) / 2,
                Color::RGB(0xFF, 0xFF, 0xFF),
            )?;
        }
        ThemeId::Win7Aero => {
            if active && !minimized {
                fill_vgrad(
                    frame,
                    rect,
                    Color::RGB(0xB8, 0xD8, 0xF8),
                    Color::RGB(0x70, 0xA8, 0xD8),
                );
                frame.fill(
                    Rect::new(rect.x + 2, rect.y + rect.h - 3, rect.w - 4, 2),
                    Color::RGB(0x33, 0x99, 0xFF),
                );
            } else {
                frame.fill(rect, Color::RGB(0xE8, 0xF0, 0xF8));
                frame.flat_border(rect, theme.border);
            }
            frame.button_sheen(rect);
            let tw = frame.text_width(&label);
            frame.text(
                &label,
                rect.x + (rect.w - tw) / 2,
                rect.y + (rect.h - theme.font_size as i32) / 2,
                theme.text,
            )?;
        }
        ThemeId::ModernLight | ThemeId::ModernDark => {
            let face = if active && !minimized {
                theme.selection
            } else if theme.id == ThemeId::ModernDark {
                Color::RGB(0x2A, 0x2A, 0x2A)
            } else {
                Color::RGB(0xE8, 0xE8, 0xE8)
            };
            frame.fill(rect, face);
            if active && !minimized {
                frame.fill(
                    Rect::new(rect.x + 4, rect.y + rect.h - 3, rect.w - 8, 2),
                    theme.selection_text,
                );
            }
            let color = if active && !minimized {
                theme.selection_text
            } else {
                theme.text
            };
            let tw = frame.text_width(&label);
            frame.text(
                &label,
                rect.x + (rect.w - tw) / 2,
                rect.y + (rect.h - theme.font_size as i32) / 2,
                color,
            )?;
        }
    }
    if badge > 0 {
        let b = format!("{}", badge.min(99));
        let bw = frame.text_width(&b) + 6;
        let badge_r = Rect::new(rect.x + rect.w - bw - 2, rect.y + 2, bw, theme.font_size as i32 + 4);
        frame.fill(badge_r, Color::RGB(0xE8, 0x20, 0x20));
        frame.text(&b, badge_r.x + 3, badge_r.y + 1, Color::RGB(0xFF, 0xFF, 0xFF))?;
    }
    Ok(())
}

fn draw_tray_chip(frame: &mut Frame<'_>, rect: Rect, label: &str) -> Result<()> {
    let theme = frame.theme;
    match theme.id {
        ThemeId::Win95 | ThemeId::Win98 => {
            frame.sunken(rect, theme.sunken);
        }
        ThemeId::WinXpLuna => {
            frame.fill(rect, Color::RGB(0x1A, 0x48, 0xA8));
            frame.flat_border(rect, Color::RGB(0x0C, 0x28, 0x70));
        }
        ThemeId::Win7Aero => {
            frame.fill(rect, Color::RGB(0xD8, 0xE8, 0xF4));
            frame.flat_border(rect, theme.border);
        }
        ThemeId::ModernLight | ThemeId::ModernDark => {
            frame.fill(rect, theme.button.face);
        }
    }
    let shown = truncate(frame, label, rect.w - 4);
    let tw = frame.text_width(&shown);
    let color = match theme.id {
        ThemeId::WinXpLuna => Color::RGB(0xFF, 0xFF, 0xFF),
        _ => theme.text,
    };
    frame.text(
        &shown,
        rect.x + (rect.w - tw) / 2,
        rect.y + (rect.h - theme.font_size as i32) / 2,
        color,
    )?;
    Ok(())
}

fn draw_clock(frame: &mut Frame<'_>, rect: Rect, clock: &str) -> Result<()> {
    let theme = frame.theme;
    match theme.id {
        ThemeId::Win95 | ThemeId::Win98 => frame.sunken(rect, theme.sunken),
        ThemeId::WinXpLuna => {
            frame.fill(rect, Color::RGB(0x16, 0x40, 0x98));
            frame.flat_border(rect, Color::RGB(0x0A, 0x24, 0x68));
        }
        ThemeId::Win7Aero => {
            frame.fill(rect, Color::RGB(0xD0, 0xE4, 0xF4));
            frame.flat_border(rect, theme.border);
        }
        ThemeId::ModernLight | ThemeId::ModernDark => {
            frame.fill(rect, theme.button.face);
        }
    }
    let tw = frame.text_width(clock);
    let color = match theme.id {
        ThemeId::WinXpLuna => Color::RGB(0xFF, 0xFF, 0xFF),
        _ => theme.text,
    };
    frame.text(
        clock,
        rect.x + (rect.w - tw) / 2,
        rect.y + (rect.h - theme.font_size as i32) / 2,
        color,
    )?;
    Ok(())
}

fn draw_show_desktop_strip(frame: &mut Frame<'_>, rect: Rect) -> Result<()> {
    let theme = frame.theme;
    match theme.id {
        ThemeId::Win95 | ThemeId::Win98 => {
            frame.raised(rect, theme.chrome);
        }
        ThemeId::WinXpLuna => {
            fill_vgrad(
                frame,
                rect,
                Color::RGB(0x6A, 0x9A, 0xF0),
                Color::RGB(0x1A, 0x3E, 0xA0),
            );
        }
        ThemeId::Win7Aero => {
            frame.fill(rect, Color::RGB(0xA8, 0xC8, 0xE0));
            frame.fill(
                Rect::new(rect.x, rect.y, 1, rect.h),
                Color::RGB(0xFF, 0xFF, 0xFF),
            );
        }
        ThemeId::ModernLight => {
            frame.fill(rect, Color::RGB(0xC8, 0xC8, 0xC8));
        }
        ThemeId::ModernDark => {
            frame.fill(rect, Color::RGB(0x55, 0x55, 0x55));
        }
    }
    let _ = theme;
    Ok(())
}

fn fill_hgrad(frame: &mut Frame<'_>, rect: Rect, left: Color, right: Color) {
    for x in 0..rect.w {
        let t = x as f32 / rect.w.max(1) as f32;
        frame.fill(
            Rect::new(rect.x + x, rect.y, 1, rect.h),
            lerp_color(left, right, t),
        );
    }
}

fn fill_vgrad(frame: &mut Frame<'_>, rect: Rect, top: Color, bot: Color) {
    for y in 0..rect.h {
        let t = y as f32 / rect.h.max(1) as f32;
        frame.fill(
            Rect::new(rect.x, rect.y + y, rect.w, 1),
            lerp_color(top, bot, t),
        );
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

fn probe_network() -> String {
    #[cfg(unix)]
    {
        if std::path::Path::new("/sys/class/net").is_dir() {
            if let Ok(rd) = std::fs::read_dir("/sys/class/net") {
                for ent in rd.flatten() {
                    let name = ent.file_name().to_string_lossy().into_owned();
                    if name == "lo" {
                        continue;
                    }
                    let oper = ent.path().join("operstate");
                    if let Ok(s) = std::fs::read_to_string(oper) {
                        if s.trim() == "up" {
                            return "Net".into();
                        }
                    }
                }
            }
        }
        "Off".into()
    }
    #[cfg(not(unix))]
    {
        "Net".into()
    }
}

fn probe_battery() -> String {
    #[cfg(unix)]
    {
        let base = std::path::Path::new("/sys/class/power_supply");
        if let Ok(rd) = std::fs::read_dir(base) {
            for ent in rd.flatten() {
                let path = ent.path();
                let typ = std::fs::read_to_string(path.join("type")).unwrap_or_default();
                if !typ.to_ascii_lowercase().contains("battery") {
                    continue;
                }
                if let Ok(cap) = std::fs::read_to_string(path.join("capacity")) {
                    let n = cap.trim().to_string();
                    if !n.is_empty() {
                        return format!("{n}%");
                    }
                }
            }
        }
        "AC".into()
    }
    #[cfg(not(unix))]
    {
        "Bat".into()
    }
}

/// Draw calendar / quick-settings / widgets / jump-list flyouts above the taskbar.
pub fn draw_flyout(
    frame: &mut Frame<'_>,
    taskbar: &Taskbar,
    jump_items: &[(&str, bool)],
) -> Result<()> {
    let kind = taskbar.flyout;
    if kind == Flyout::None {
        return Ok(());
    }
    let theme = frame.theme;
    let rect = taskbar.flyout_rect(kind);
    frame.panel(rect, theme.chrome);
    let inner = rect.shrink(2);
    frame.fill(inner, theme.window);

    let title = match kind {
        Flyout::Calendar => "Calendar",
        Flyout::QuickSettings => "Quick Settings",
        Flyout::Widgets => "Widgets",
        Flyout::JumpList => "Jump List",
        Flyout::TaskView => "Task View",
        Flyout::None => "",
    };
    let title_h = theme.title_height.max(18);
    let title_r = Rect::new(inner.x, inner.y, inner.w, title_h);
    frame.title_bar(title_r, true);
    frame.text(
        title,
        title_r.x + 8,
        title_r.y + (title_h - theme.font_size as i32) / 2,
        theme.title.text,
    )?;

    let body_y = title_r.y + title_h + 8;
    match kind {
        Flyout::Calendar => {
            let (y, m, d) = civil_ymd();
            frame.text(
                &format!("{y}-{m:02}-{d:02}"),
                inner.x + 10,
                body_y,
                theme.text_disabled,
            )?;
            draw_month_grid(frame, inner.x + 10, body_y + theme.row_height + 4, m, d)?;
        }
        Flyout::QuickSettings => {
            let lines = [
                format!("Network: {}", taskbar.net_label),
                format!("Volume: {}", taskbar.vol_label),
                format!("Battery: {}", taskbar.bat_label),
                "Theme / effects: Settings in fuckFM".into(),
                "Notifications: none".into(),
            ];
            for (i, line) in lines.iter().enumerate() {
                frame.text(
                    line,
                    inner.x + 10,
                    body_y + i as i32 * theme.row_height,
                    theme.text,
                )?;
            }
        }
        Flyout::Widgets => {
            frame.text(
                &format!("Clock  {}", current_clock()),
                inner.x + 10,
                body_y,
                theme.text,
            )?;
            frame.text(
                &format!("Network  {}", taskbar.net_label),
                inner.x + 10,
                body_y + theme.row_height,
                theme.text,
            )?;
            frame.text(
                &format!("Battery  {}", taskbar.bat_label),
                inner.x + 10,
                body_y + theme.row_height * 2,
                theme.text,
            )?;
            frame.text(
                "Pin more widgets later",
                inner.x + 10,
                body_y + theme.row_height * 3,
                theme.text_disabled,
            )?;
        }
        Flyout::JumpList => {
            for (i, (label, _)) in jump_items.iter().enumerate() {
                let row = Rect::new(
                    inner.x + 4,
                    body_y + i as i32 * theme.row_height,
                    inner.w - 8,
                    theme.row_height,
                );
                if matches!(theme.id, ThemeId::WinXpLuna | ThemeId::Win7Aero) && i == 0 {
                    frame.fill(row, theme.selection);
                    frame.text(
                        label,
                        row.x + 6,
                        row.y + (row.h - theme.font_size as i32) / 2,
                        theme.selection_text,
                    )?;
                } else {
                    frame.text(
                        label,
                        row.x + 6,
                        row.y + (row.h - theme.font_size as i32) / 2,
                        theme.text,
                    )?;
                }
            }
        }
        Flyout::TaskView => {
            frame.text(
                "Virtual desktop 1 (active)",
                inner.x + 10,
                body_y,
                theme.text,
            )?;
            frame.text(
                "Use Alt+Tab to switch windows",
                inner.x + 10,
                body_y + theme.row_height,
                theme.text_disabled,
            )?;
            frame.text(
                "Show Desktop from the right strip",
                inner.x + 10,
                body_y + theme.row_height * 2,
                theme.text_disabled,
            )?;
        }
        Flyout::None => {}
    }
    Ok(())
}

pub fn jump_list_hit(taskbar: &Taskbar, x: i32, y: i32, count: usize) -> Option<usize> {
    if taskbar.flyout != Flyout::JumpList {
        return None;
    }
    let rect = taskbar.flyout_rect(Flyout::JumpList);
    if !rect.contains(x, y) {
        return None;
    }
    let theme_row = 22;
    let title_h = 22;
    let inner = rect.shrink(2);
    let rel = y - (inner.y + title_h + 8);
    if rel < 0 {
        return None;
    }
    let idx = (rel / theme_row) as usize;
    if idx < count {
        Some(idx)
    } else {
        None
    }
}

fn draw_month_grid(frame: &mut Frame<'_>, x: i32, y: i32, month: u32, today: u32) -> Result<()> {
    let theme = frame.theme;
    let cell = 28;
    let days = days_in_month(month);
    for d in 1..=days {
        let col = ((d - 1) % 7) as i32;
        let row = ((d - 1) / 7) as i32;
        let r = Rect::new(x + col * cell, y + row * cell, cell - 2, cell - 2);
        if d == today {
            match theme.id {
                ThemeId::WinXpLuna => {
                    fill_hgrad(
                        frame,
                        r,
                        Color::RGB(0x3A, 0x8A, 0xF0),
                        Color::RGB(0x1E, 0x58, 0xC0),
                    );
                    frame.text(
                        &format!("{d}"),
                        r.x + 6,
                        r.y + 4,
                        Color::RGB(0xFF, 0xFF, 0xFF),
                    )?;
                }
                _ => {
                    frame.fill(r, theme.selection);
                    frame.text(
                        &format!("{d}"),
                        r.x + 6,
                        r.y + 4,
                        theme.selection_text,
                    )?;
                }
            }
        } else {
            frame.text(&format!("{d}"), r.x + 6, r.y + 4, theme.text)?;
        }
    }
    Ok(())
}

fn civil_ymd() -> (i32, u32, u32) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let offset = std::env::var("TZ_OFFSET_SECS")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    let days = ((secs as i64 + offset) / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

fn days_in_month(month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => 28,
        _ => 30,
    }
}
