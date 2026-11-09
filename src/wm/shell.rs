use super::desktop::{open_desk_item, Desktop};
use super::start_menu::{self, StartAction, StartEntry, BRAND_W, ICON_SZ, MENU_W, SEP_H};
use super::taskbar::Taskbar;
use super::wallpaper::WallpaperCache;
use crate::iconpack::IconPack;
use crate::render::{FontAtlas, Frame, Rect};
use crate::settings::Settings;
use crate::theme::theme;
use anyhow::{Context, Result};
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::mouse::MouseButton;
use std::time::Duration;

struct StartMenu {
    open: bool,
    rect: Rect,
    entries: Vec<StartEntry>,
    hover: Option<usize>,
}

impl StartMenu {
    fn new() -> Self {
        Self {
            open: false,
            rect: Rect::default(),
            entries: start_menu::default_entries(),
            hover: None,
        }
    }

    fn layout(&mut self, taskbar: &Taskbar, theme_row: i32) {
        let row = theme_row.max(22);
        let h = start_menu::menu_height(&self.entries, row);
        self.rect = Rect::new(
            taskbar.rect.x + 4,
            taskbar.rect.y - h - 2,
            MENU_W,
            h,
        );
    }

    fn index_at(&self, x: i32, y: i32, row: i32) -> Option<usize> {
        if !self.open || !self.rect.contains(x, y) {
            return None;
        }
        // Brand strip is not clickable as an item.
        if x < self.rect.x + BRAND_W {
            return None;
        }
        start_menu::hit_index(&self.entries, y - self.rect.y, row)
    }

    fn draw(&self, frame: &mut Frame<'_>, pack: &IconPack, row: i32, brand: &str) -> Result<()> {
        if !self.open {
            return Ok(());
        }
        let theme = frame.theme;
        let row = row.max(22);
        frame.panel(self.rect, theme.chrome);

        // Brand strip (classic Start look).
        let brand_r = Rect::new(self.rect.x + 2, self.rect.y + 2, BRAND_W - 2, self.rect.h - 4);
        frame.fill(brand_r, theme.title.active_left);
        let mut by = brand_r.y + 10;
        for ch in brand.chars().take(10) {
            let s = ch.to_string();
            frame.text(&s, brand_r.x + 8, by, theme.title.text)?;
            by += theme.font_size as i32 + 2;
        }

        let inner = Rect::new(
            self.rect.x + BRAND_W,
            self.rect.y + 2,
            self.rect.w - BRAND_W - 2,
            self.rect.h - 4,
        );
        frame.fill(inner, theme.window);

        for (i, entry) in self.entries.iter().enumerate() {
            let y = self.rect.y + start_menu::entry_y(&self.entries, i, row);
            if entry.separator_before {
                let sy = y - SEP_H / 2 - 1;
                frame.fill(
                    Rect::new(inner.x + 6, sy, inner.w - 12, 1),
                    theme.border,
                );
            }
            let row_r = Rect::new(inner.x, y, inner.w, row);
            if self.hover == Some(i) {
                frame.fill(row_r, theme.selection);
            }
            let color = if self.hover == Some(i) {
                theme.selection_text
            } else {
                theme.text
            };
            let mut text_x = row_r.x + 8;
            if let Some(purpose) = entry.icon {
                if let Some(icon) = pack
                    .resolve(purpose, ICON_SZ as u32)
                    .or_else(|| pack.resolve(purpose, 32))
                {
                    let iy = row_r.y + (row - ICON_SZ) / 2;
                    let _ = frame.fonts.blit_rgba(
                        frame.canvas,
                        text_x,
                        iy,
                        icon.width,
                        icon.height,
                        ICON_SZ as u32,
                        ICON_SZ as u32,
                        &icon.rgba,
                    );
                    text_x += ICON_SZ + 6;
                }
            }
            frame.text(
                entry.label,
                text_x,
                row_r.y + (row - theme.font_size as i32) / 2,
                color,
            )?;
        }
        Ok(())
    }
}

pub fn boot(product: &str) -> Result<()> {
    let force_sdl = std::env::args().any(|a| a == "--sdl")
        || std::env::var("FUCKWM_BACKEND")
            .map(|v| v.eq_ignore_ascii_case("sdl"))
            .unwrap_or(false);

    #[cfg(unix)]
    let mut xephyr_child = None;
    #[cfg(unix)]
    if !force_sdl {
        match crate::wm::xephyr::maybe_spawn() {
            Ok(child) => xephyr_child = child,
            Err(err) => eprintln!("{product}: Xephyr helper: {err}"),
        }
    }

    let result = if force_sdl {
        boot_sdl(product)
    } else {
        match crate::wm::backend::run_native(product) {
            Ok(()) => Ok(()),
            Err(err) => {
                eprintln!(
                    "{product}: native backend unavailable ({err}); falling back to SDL preview"
                );
                eprintln!("  tip: cargo run --bin fuckwm -- --xephyr");
                eprintln!("  or:  cargo run --bin fuckwm -- --sdl");
                boot_sdl(product)
            }
        }
    };

    #[cfg(unix)]
    if let Some(mut child) = xephyr_child {
        let _ = child.kill();
        let _ = child.wait();
    }

    result
}

fn boot_sdl(product: &str) -> Result<()> {
    let sdl = sdl2::init().map_err(|e| anyhow::anyhow!(e))?;
    let video = sdl.video().map_err(|e| anyhow::anyhow!(e))?;

    let window = video
        .window(product, 1280, 800)
        .position_centered()
        .resizable()
        .maximized()
        .build()
        .context("create shell window")?;

    let mut canvas = window.into_canvas().present_vsync().build()?;
    let mut fonts = FontAtlas::new(&canvas)?;
    let mut events = sdl.event_pump().map_err(|e| anyhow::anyhow!(e))?;

    let mut settings = Settings::load_or_default();
    let mut icons = IconPack::load_for_theme(settings.theme_id, &settings.icon_pack);
    let mut wallpaper = WallpaperCache::new(settings.theme_id);
    let mut desktop = Desktop::load();
    let mut taskbar = Taskbar::new();
    let mut start = StartMenu::new();
    let mut status = format!("{product} SDL shell preview");
    let mut running = true;

    while running {
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. } => running = false,
                Event::Window {
                    win_event: WindowEvent::Resized(..) | WindowEvent::SizeChanged(..),
                    ..
                } => {}
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => {
                    if start.open {
                        start.open = false;
                        taskbar.start_open = false;
                    } else {
                        running = false;
                    }
                }
                Event::MouseMotion { x, y, .. } => {
                    let th = theme(settings.theme_id);
                    desktop.hover = desktop.index_at(x, y);
                    if start.open {
                        start.hover = start.index_at(x, y, th.row_height + 6);
                    }
                }
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    clicks,
                    ..
                } => {
                    let th = theme(settings.theme_id);
                    if start.open {
                        if let Some(i) = start.index_at(x, y, th.row_height + 6) {
                            match apply_start_action(
                                &start.entries[i].action,
                                &mut settings,
                                &mut icons,
                                &mut status,
                                &mut running,
                            ) {
                                Ok(()) => {}
                                Err(err) => status = format!("{err}"),
                            }
                            start.open = false;
                            taskbar.start_open = false;
                            continue;
                        } else if !taskbar.start_btn.hit(x, y) {
                            start.open = false;
                            taskbar.start_open = false;
                        }
                    }

                    if taskbar.start_btn.hit(x, y) {
                        start.open = !start.open;
                        taskbar.start_open = start.open;
                        continue;
                    }
                    if taskbar.fm_btn.hit(x, y) {
                        let _ = start_menu::launch_fm(None);
                        status = "Launched fuckFM".into();
                        continue;
                    }
                    if let Some(i) = desktop.index_at(x, y) {
                        desktop.selected = Some(i);
                        if clicks >= 2 {
                            match open_desk_item(&desktop.icons[i].path) {
                                Ok(()) => {
                                    status = format!("Opened {}", desktop.icons[i].name);
                                }
                                Err(err) => status = format!("Open failed: {err}"),
                            }
                        }
                    } else if !taskbar.rect.contains(x, y) {
                        desktop.selected = None;
                    }
                }
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Right,
                    x,
                    y,
                    ..
                } => {
                    if !taskbar.rect.contains(x, y) {
                        status = "Desktop menu — coming soon".into();
                    }
                }
                _ => {}
            }
        }

        let th = theme(settings.theme_id);
        let (w, h) = canvas.output_size().map_err(|e| anyhow::anyhow!(e))?;
        let window_r = Rect::new(0, 0, w as i32, h as i32);
        let taskbar_h = (th.toolbar_height + 8).max(36);
        taskbar.layout(window_r, taskbar_h);
        start.layout(&taskbar, th.row_height + 6);
        let desk_area = Rect::new(0, 0, window_r.w, window_r.h - taskbar_h);
        desktop.layout(desk_area, &th);
        wallpaper.ensure(
            settings.theme_id,
            settings.wallpaper.as_deref(),
            desk_area.w.max(1) as u32,
            desk_area.h.max(1) as u32,
        );

        let mut frame = Frame {
            canvas: &mut canvas,
            fonts: &mut fonts,
            theme: th,
            effects: th.effects.gated(settings.visual_effects),
        };
        // Solid fill under the wallpaper (letterbox / load failure safety).
        frame.clear_desktop();
        wallpaper.draw(&mut frame, desk_area)?;
        desktop.draw(&mut frame, &icons)?;
        taskbar.draw(&mut frame)?;
        start.draw(&mut frame, &icons, th.row_height + 6, product)?;

        // Status strip above taskbar (dev hint).
        let hint = Rect::new(8, taskbar.rect.y - th.row_height - 6, window_r.w - 16, th.row_height);
        frame.text(&status, hint.x, hint.y, th.text_disabled)?;

        frame.present();
        std::thread::sleep(Duration::from_millis(16));
    }
    Ok(())
}

fn apply_start_action(
    action: &StartAction,
    settings: &mut Settings,
    icons: &mut IconPack,
    status: &mut String,
    running: &mut bool,
) -> Result<()> {
    match action {
        StartAction::LaunchFm => {
            start_menu::launch_fm(None)?;
            *status = "Launched fuckFM".into();
        }
        StartAction::OpenThisPc => {
            start_menu::launch_fm_this_pc()?;
            *status = "Opened This PC".into();
        }
        StartAction::OpenDesktop => {
            if let Some(p) = start_menu::desktop_dir() {
                start_menu::launch_fm(Some(&p))?;
                *status = "Opened Desktop".into();
            } else {
                *status = "Desktop folder missing".into();
            }
        }
        StartAction::OpenDocuments => {
            if let Some(p) = start_menu::documents_dir() {
                start_menu::launch_fm(Some(&p))?;
                *status = "Opened Documents".into();
            } else {
                *status = "Documents folder missing".into();
            }
        }
        StartAction::OpenPictures => {
            if let Some(p) = start_menu::pictures_dir() {
                start_menu::launch_fm(Some(&p))?;
                *status = "Opened Pictures".into();
            } else {
                *status = "Pictures folder missing".into();
            }
        }
        StartAction::OpenCmd => {
            start_menu::open_command_prompt()?;
            *status = "Opened Command Prompt".into();
        }
        StartAction::CycleTheme => {
            settings.theme_id = settings.theme_id.next();
            *icons = IconPack::load_for_theme(settings.theme_id, &settings.icon_pack);
            let _ = settings.save();
            *status = format!("Theme: {}", settings.theme_id.label());
        }
        StartAction::LogOff => {
            start_menu::power_action("logoff");
            *status = "Log off requested".into();
        }
        StartAction::Restart => {
            start_menu::power_action("restart");
            *status = "Restart requested".into();
        }
        StartAction::Shutdown => {
            start_menu::power_action("shutdown");
            *status = "Shut down requested".into();
        }
        StartAction::QuitSession => *running = false,
    }
    Ok(())
}
