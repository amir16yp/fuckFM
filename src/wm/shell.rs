use super::alt_tab::{AltTab, SwitchEntry};
use super::desktop::{
    desk_action_from_index, open_desk_item, DeskAction, DeskArrange, Desktop,
};
use super::start_menu::{self, StartAction, StartEntry, BRAND_W, ICON_SZ, MENU_W, SEP_H};
use super::taskbar::{self, Flyout, Taskbar, TaskbarHit};
use super::wallpaper::WallpaperCache;
use crate::iconpack::IconPack;
use crate::render::{FontAtlas, Frame, Rect};
use crate::settings::Settings;
use crate::theme::theme;
use anyhow::{Context, Result};
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::{Keycode, Mod};
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

        let brand_r = Rect::new(self.rect.x + 2, self.rect.y + 2, BRAND_W - 2, self.rect.h - 4);
        // Brand strip follows caption colors (gradient when the theme has one).
        if theme.title.gradient && theme.title.active_left != theme.title.active_right {
            for y in 0..brand_r.h {
                let t = y as f32 / brand_r.h.max(1) as f32;
                frame.fill(
                    Rect::new(brand_r.x, brand_r.y + y, brand_r.w, 1),
                    crate::render::lerp_color(
                        theme.title.active_left,
                        theme.title.active_right,
                        t,
                    ),
                );
            }
        } else {
            frame.fill(brand_r, theme.title.active_left);
        }
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

/// Simple desktop / shell context menu (labels only; actions resolved by index).
struct DeskMenu {
    open: bool,
    rect: Rect,
    labels: Vec<String>,
    hover: Option<usize>,
    row: i32,
}

impl DeskMenu {
    fn closed() -> Self {
        Self {
            open: false,
            rect: Rect::default(),
            labels: Vec::new(),
            hover: None,
            row: 22,
        }
    }

    fn open_at(&mut self, x: i32, y: i32, labels: Vec<String>, row: i32, bounds: Rect) {
        let row = row.max(18);
        let w = 220;
        let h = labels.len() as i32 * row + 6;
        let mut rect = Rect::new(x, y, w, h);
        if rect.x + rect.w > bounds.x + bounds.w {
            rect.x = (bounds.x + bounds.w - rect.w).max(bounds.x);
        }
        if rect.y + rect.h > bounds.y + bounds.h {
            rect.y = (bounds.y + bounds.h - rect.h).max(bounds.y);
        }
        self.open = true;
        self.rect = rect;
        self.labels = labels;
        self.hover = None;
        self.row = row;
    }

    fn close(&mut self) {
        self.open = false;
        self.labels.clear();
        self.hover = None;
    }

    fn index_at(&self, x: i32, y: i32) -> Option<usize> {
        if !self.open || !self.rect.contains(x, y) {
            return None;
        }
        let rel = y - (self.rect.y + 3);
        if rel < 0 {
            return None;
        }
        let idx = (rel / self.row) as usize;
        if idx < self.labels.len() {
            Some(idx)
        } else {
            None
        }
    }

    fn draw(&self, frame: &mut Frame<'_>) -> Result<()> {
        if !self.open {
            return Ok(());
        }
        let theme = frame.theme;
        frame.panel(self.rect, theme.chrome);
        let inner = self.rect.shrink(2);
        frame.fill(inner, theme.window);
        for (i, label) in self.labels.iter().enumerate() {
            let y = inner.y + 1 + i as i32 * self.row;
            let row = Rect::new(inner.x, y, inner.w, self.row);
            if self.hover == Some(i) {
                frame.fill(row, theme.selection);
            }
            let color = if self.hover == Some(i) {
                theme.selection_text
            } else {
                theme.text
            };
            frame.text(
                label,
                row.x + 8,
                row.y + (self.row - theme.font_size as i32) / 2,
                color,
            )?;
        }
        Ok(())
    }
}

fn jump_items_for(taskbar: &Taskbar) -> Vec<(String, JumpAction)> {
    let mut items = vec![
        ("Open fuckFM".into(), JumpAction::LaunchFm),
        ("This PC".into(), JumpAction::ThisPc),
        ("Documents".into(), JumpAction::Documents),
        ("Desktop".into(), JumpAction::Desktop),
    ];
    if taskbar.jump_for.is_some() {
        items.push(("Close window".into(), JumpAction::Close));
    }
    items
}

#[derive(Clone, Copy)]
enum JumpAction {
    LaunchFm,
    ThisPc,
    Documents,
    Desktop,
    Close,
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
    taskbar.refresh_tray_status();
    // Seed a demo running window so Alt-Tab / task buttons are exercisable offline.
    let _ = taskbar.spawn_running("Shell Preview");
    let mut start = StartMenu::new();
    let mut desk_menu = DeskMenu::closed();
    let mut alt_tab = AltTab::default();
    let mut status = format!("{product} SDL shell preview — Alt+Tab · right-click desktop");
    let mut running = true;
    let mut show_desktop = false;
    let mut icon_drag: Option<(usize, i32, i32)> = None;
    let mut tray_ticks: u32 = 0;

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
                    if alt_tab.open {
                        alt_tab.cancel();
                        taskbar.flyout = Flyout::None;
                    } else if desk_menu.open {
                        desk_menu.close();
                    } else if taskbar.flyout != Flyout::None {
                        taskbar.flyout = Flyout::None;
                    } else if start.open {
                        start.open = false;
                        taskbar.start_open = false;
                    } else {
                        running = false;
                    }
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Return) | Some(Keycode::KpEnter),
                    ..
                } if alt_tab.open => {
                    if let Some(sel) = alt_tab.close() {
                        apply_switch(
                            &sel,
                            &mut taskbar,
                            &mut desktop,
                            &mut show_desktop,
                            &mut status,
                        );
                        taskbar.flyout = Flyout::None;
                    }
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Tab),
                    keymod,
                    ..
                } => {
                    let alt = keymod.intersects(Mod::LALTMOD | Mod::RALTMOD);
                    let shift = keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
                    if alt || alt_tab.open {
                        if !alt_tab.open {
                            alt_tab.begin(switch_entries(&taskbar));
                        } else if shift {
                            alt_tab.retreat();
                        } else {
                            alt_tab.advance();
                        }
                        close_overlays(&mut start, &mut taskbar, &mut desk_menu);
                    }
                }
                Event::KeyUp {
                    keycode: Some(Keycode::LAlt) | Some(Keycode::RAlt),
                    ..
                } => {
                    if alt_tab.open {
                        if let Some(sel) = alt_tab.close() {
                            apply_switch(
                                &sel,
                                &mut taskbar,
                                &mut desktop,
                                &mut show_desktop,
                                &mut status,
                            );
                        }
                    }
                }
                Event::KeyDown {
                    keycode: Some(Keycode::D),
                    keymod,
                    ..
                } if keymod.intersects(Mod::LGUIMOD | Mod::RGUIMOD) => {
                    toggle_show_desktop(
                        &mut taskbar,
                        &mut desktop,
                        &mut show_desktop,
                        &mut status,
                    );
                }
                Event::MouseMotion { x, y, .. } => {
                    let th = theme(settings.theme_id);
                    if let Some((idx, ox, oy)) = icon_drag {
                        if desktop.arrange != DeskArrange::Auto {
                            if let Some(icon) = desktop.icons.get_mut(idx) {
                                icon.rect.x = x - ox;
                                icon.rect.y = y - oy;
                                let cols = ((desktop.area.w - 16) / desktop.cell_w.max(1)).max(1);
                                let cx = icon.rect.x + icon.rect.w / 2 - desktop.area.x - 12;
                                let cy = icon.rect.y + icon.rect.h / 2 - desktop.area.y - 12;
                                icon.col = (cx / desktop.cell_w.max(1)).clamp(0, cols - 1);
                                icon.row = (cy / desktop.cell_h.max(1)).max(0);
                            }
                        }
                    } else {
                        desktop.hover = desktop.index_at(x, y);
                    }
                    if start.open {
                        start.hover = start.index_at(x, y, th.row_height + 6);
                    }
                    if desk_menu.open {
                        desk_menu.hover = desk_menu.index_at(x, y);
                    }
                }
                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    ..
                } => {
                    if icon_drag.take().is_some() && desktop.arrange == DeskArrange::Free {
                        // Keep free placement; next layout pass uses stored col/row.
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
                    let (w, h) = canvas.output_size().map_err(|e| anyhow::anyhow!(e))?;
                    let window_r = Rect::new(0, 0, w as i32, h as i32);

                    if alt_tab.open {
                        if let Some(i) = alt_tab.index_at(x, y) {
                            alt_tab.index = i;
                            if let Some(sel) = alt_tab.close() {
                                apply_switch(
                                    &sel,
                                    &mut taskbar,
                                    &mut desktop,
                                    &mut show_desktop,
                                    &mut status,
                                );
                                taskbar.flyout = Flyout::None;
                            }
                        } else {
                            alt_tab.cancel();
                            taskbar.flyout = Flyout::None;
                        }
                        continue;
                    }

                    if desk_menu.open {
                        if let Some(i) = desk_menu.index_at(x, y) {
                            if let Some(action) = desk_action_from_index(&desktop, i) {
                                apply_desk_action(
                                    action,
                                    &mut desktop,
                                    &mut settings,
                                    &mut icons,
                                    &mut taskbar,
                                    &mut show_desktop,
                                    &mut status,
                                    &mut running,
                                )?;
                            }
                            desk_menu.close();
                            continue;
                        }
                        desk_menu.close();
                    }

                    if taskbar.flyout != Flyout::None {
                        let jumps = jump_items_for(&taskbar);
                        if let Some(i) = taskbar::jump_list_hit(&taskbar, x, y, jumps.len()) {
                            apply_jump(jumps[i].1, &mut taskbar, &mut status)?;
                            taskbar.flyout = Flyout::None;
                            continue;
                        }
                        let fr = taskbar.flyout_rect(taskbar.flyout);
                        if !fr.contains(x, y) && taskbar.hit(x, y).is_none() {
                            taskbar.flyout = Flyout::None;
                        }
                    }

                    if start.open {
                        if let Some(i) = start.index_at(x, y, th.row_height + 6) {
                            match apply_start_action(
                                &start.entries[i].action,
                                &mut settings,
                                &mut icons,
                                &mut taskbar,
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

                    if let Some(hit) = taskbar.hit(x, y) {
                        handle_taskbar_hit(
                            hit,
                            &mut taskbar,
                            &mut start,
                            &mut desktop,
                            &mut show_desktop,
                            &mut alt_tab,
                            &mut status,
                        )?;
                        continue;
                    }

                    if let Some(i) = desktop.index_at(x, y) {
                        desktop.selected = Some(i);
                        if desktop.arrange != DeskArrange::Auto {
                            let r = desktop.icons[i].rect;
                            icon_drag = Some((i, x - r.x, y - r.y));
                            if desktop.arrange == DeskArrange::AlignGrid {
                                desktop.arrange = DeskArrange::Free;
                            }
                        }
                        if clicks >= 2 {
                            icon_drag = None;
                            match open_desk_item(&desktop.icons[i].path) {
                                Ok(()) => {
                                    let name = desktop.icons[i].name.clone();
                                    note_running(&mut taskbar, &name);
                                    status = format!("Opened {name}");
                                }
                                Err(err) => status = format!("Open failed: {err}"),
                            }
                        }
                    } else if !taskbar.rect.contains(x, y) {
                        desktop.selected = None;
                        // Click empty desktop dismisses flyouts.
                        taskbar.flyout = Flyout::None;
                        let _ = window_r;
                    }
                }
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Right,
                    x,
                    y,
                    ..
                } => {
                    let th = theme(settings.theme_id);
                    let (w, h) = canvas.output_size().map_err(|e| anyhow::anyhow!(e))?;
                    let bounds = Rect::new(0, 0, w as i32, h as i32);

                    if taskbar.fm_btn.hit(x, y) {
                        taskbar.flyout = Flyout::JumpList;
                        taskbar.jump_for = Some(0);
                        start.open = false;
                        taskbar.start_open = false;
                        desk_menu.close();
                        status = "Jump list".into();
                        continue;
                    }
                    if let Some(TaskbarHit::Running(i)) = taskbar.hit(x, y) {
                        if let Some(app) = taskbar.running.get(i) {
                            taskbar.flyout = Flyout::JumpList;
                            taskbar.jump_for = Some(app.id);
                            status = format!("Jump list — {}", app.title);
                        }
                        continue;
                    }
                    if !taskbar.rect.contains(x, y) {
                        start.open = false;
                        taskbar.start_open = false;
                        taskbar.flyout = Flyout::None;
                        desk_menu.open_at(
                            x,
                            y,
                            desktop.context_menu_labels(),
                            th.row_height + 4,
                            bounds,
                        );
                    }
                }
                _ => {}
            }
        }

        tray_ticks = tray_ticks.wrapping_add(1);
        if tray_ticks % 120 == 0 {
            taskbar.refresh_tray_status();
        }

        let th = theme(settings.theme_id);
        let (w, h) = canvas.output_size().map_err(|e| anyhow::anyhow!(e))?;
        let window_r = Rect::new(0, 0, w as i32, h as i32);
        let taskbar_h = (th.toolbar_height + 8).max(36);
        taskbar.layout(window_r, taskbar_h);
        start.layout(&taskbar, th.row_height + 6);
        let desk_area = Rect::new(0, 0, window_r.w, window_r.h - taskbar_h);
        // Don't clobber free-drag rects mid-drag.
        if icon_drag.is_none() {
            desktop.layout(desk_area, &th);
        } else {
            desktop.area = desk_area;
        }
        wallpaper.ensure(
            settings.theme_id,
            settings.wallpaper.as_deref(),
            desk_area.w.max(1) as u32,
            desk_area.h.max(1) as u32,
        );
        if alt_tab.open {
            alt_tab.layout(window_r);
        }

        let mut frame = Frame {
            canvas: &mut canvas,
            fonts: &mut fonts,
            theme: th,
            effects: th.effects.gated(settings.visual_effects),
        };
        frame.clear_desktop();
        wallpaper.draw(&mut frame, desk_area)?;
        if show_desktop || desktop.peek {
            // Peek / show-desktop: dim client chrome hint over wallpaper.
            frame.modal_dim(desk_area);
        }
        desktop.draw(&mut frame, &icons)?;
        taskbar.draw(&mut frame)?;
        let jump_owned = jump_items_for(&taskbar);
        let jump_labels: Vec<(&str, bool)> = jump_owned
            .iter()
            .map(|(l, _)| (l.as_str(), false))
            .collect();
        taskbar::draw_flyout(&mut frame, &taskbar, &jump_labels)?;
        start.draw(&mut frame, &icons, th.row_height + 6, product)?;
        desk_menu.draw(&mut frame)?;
        alt_tab.draw(&mut frame, window_r)?;

        let hint = Rect::new(8, taskbar.rect.y - th.row_height - 6, window_r.w - 16, th.row_height);
        frame.text(&status, hint.x, hint.y, th.text_disabled)?;

        frame.present();
        std::thread::sleep(Duration::from_millis(16));
    }
    Ok(())
}

fn close_overlays(start: &mut StartMenu, taskbar: &mut Taskbar, desk_menu: &mut DeskMenu) {
    start.open = false;
    taskbar.start_open = false;
    taskbar.flyout = Flyout::None;
    desk_menu.close();
}

fn switch_entries(taskbar: &Taskbar) -> Vec<SwitchEntry> {
    let mut entries = vec![SwitchEntry {
        id: 0,
        title: "Desktop".into(),
        is_desktop: true,
    }];
    for app in &taskbar.running {
        entries.push(SwitchEntry {
            id: app.id,
            title: app.title.clone(),
            is_desktop: false,
        });
    }
    entries
}

fn apply_switch(
    sel: &SwitchEntry,
    taskbar: &mut Taskbar,
    desktop: &mut Desktop,
    show_desktop: &mut bool,
    status: &mut String,
) {
    if sel.is_desktop {
        taskbar.minimize_all();
        *show_desktop = true;
        desktop.peek = true;
        *status = "Switched to Desktop".into();
        return;
    }
    taskbar.restore_all();
    *show_desktop = false;
    desktop.peek = false;
    for app in &mut taskbar.running {
        app.minimized = app.id != sel.id;
    }
    *status = format!("Switched to {}", sel.title);
}

fn toggle_show_desktop(
    taskbar: &mut Taskbar,
    desktop: &mut Desktop,
    show_desktop: &mut bool,
    status: &mut String,
) {
    if *show_desktop {
        taskbar.restore_all();
        *show_desktop = false;
        desktop.peek = false;
        *status = "Restored windows".into();
    } else {
        taskbar.minimize_all();
        *show_desktop = true;
        desktop.peek = true;
        *status = "Show Desktop".into();
    }
}

fn handle_taskbar_hit(
    hit: TaskbarHit,
    taskbar: &mut Taskbar,
    start: &mut StartMenu,
    desktop: &mut Desktop,
    show_desktop: &mut bool,
    alt_tab: &mut AltTab,
    status: &mut String,
) -> Result<()> {
    match hit {
        TaskbarHit::Start => {
            start.open = !start.open;
            taskbar.start_open = start.open;
            taskbar.flyout = Flyout::None;
        }
        TaskbarHit::PinFm => {
            start_menu::launch_fm(None)?;
            note_running(taskbar, "fuckFM");
            *status = "Launched fuckFM".into();
            taskbar.flyout = Flyout::None;
        }
        TaskbarHit::Running(i) => {
            if let Some(app) = taskbar.running.get_mut(i) {
                app.minimized = !app.minimized;
                *status = if app.minimized {
                    format!("Minimized {}", app.title)
                } else {
                    format!("Restored {}", app.title)
                };
                if !app.minimized {
                    *show_desktop = false;
                    desktop.peek = false;
                }
            }
            taskbar.flyout = Flyout::None;
        }
        TaskbarHit::TaskView => {
            if taskbar.flyout == Flyout::TaskView || alt_tab.open {
                taskbar.flyout = Flyout::None;
                alt_tab.cancel();
                *status = "Task View closed".into();
            } else {
                taskbar.flyout = Flyout::TaskView;
                alt_tab.begin(switch_entries(taskbar));
                *status = "Task View — click / Enter to switch, Esc cancels".into();
            }
        }
        TaskbarHit::Widgets => {
            taskbar.flyout = if taskbar.flyout == Flyout::Widgets {
                Flyout::None
            } else {
                Flyout::Widgets
            };
            *status = "Widgets".into();
        }
        TaskbarHit::TrayChevron | TaskbarHit::TrayNet | TaskbarHit::TrayVol | TaskbarHit::TrayBat => {
            taskbar.flyout = if taskbar.flyout == Flyout::QuickSettings {
                Flyout::None
            } else {
                Flyout::QuickSettings
            };
            taskbar.refresh_tray_status();
            *status = "Quick Settings".into();
        }
        TaskbarHit::Clock => {
            taskbar.flyout = if taskbar.flyout == Flyout::Calendar {
                Flyout::None
            } else {
                Flyout::Calendar
            };
            *status = "Calendar".into();
        }
        TaskbarHit::ShowDesktop => {
            toggle_show_desktop(taskbar, desktop, show_desktop, status);
            taskbar.flyout = Flyout::None;
        }
    }
    start.open = matches!(hit, TaskbarHit::Start) && start.open;
    taskbar.start_open = start.open;
    Ok(())
}

fn note_running(taskbar: &mut Taskbar, title: &str) {
    if let Some(app) = taskbar.running.iter().find(|a| a.title == title) {
        let id = app.id;
        taskbar.upsert_running(id, title);
        taskbar.set_minimized(id, false);
    } else {
        let _ = taskbar.spawn_running(title);
    }
}

fn apply_jump(action: JumpAction, taskbar: &mut Taskbar, status: &mut String) -> Result<()> {
    match action {
        JumpAction::LaunchFm => {
            start_menu::launch_fm(None)?;
            note_running(taskbar, "fuckFM");
            *status = "Launched fuckFM".into();
        }
        JumpAction::ThisPc => {
            start_menu::launch_fm_this_pc()?;
            note_running(taskbar, "This PC");
            *status = "Opened This PC".into();
        }
        JumpAction::Documents => {
            if let Some(p) = start_menu::documents_dir() {
                start_menu::launch_fm(Some(&p))?;
                note_running(taskbar, "Documents");
                *status = "Opened Documents".into();
            } else {
                *status = "Documents folder missing".into();
            }
        }
        JumpAction::Desktop => {
            if let Some(p) = start_menu::desktop_dir() {
                start_menu::launch_fm(Some(&p))?;
                note_running(taskbar, "Desktop");
                *status = "Opened Desktop".into();
            } else {
                *status = "Desktop folder missing".into();
            }
        }
        JumpAction::Close => {
            if let Some(id) = taskbar.jump_for.take() {
                taskbar.remove_running(id);
                *status = "Closed window".into();
            }
        }
    }
    Ok(())
}

fn apply_desk_action(
    action: DeskAction,
    desktop: &mut Desktop,
    settings: &mut Settings,
    icons: &mut IconPack,
    taskbar: &mut Taskbar,
    show_desktop: &mut bool,
    status: &mut String,
    running: &mut bool,
) -> Result<()> {
    match action {
        DeskAction::Refresh => {
            desktop.reload_icons();
            *status = "Desktop refreshed".into();
        }
        DeskAction::OpenFm => {
            start_menu::launch_fm(None)?;
            note_running(taskbar, "fuckFM");
            *status = "Launched fuckFM".into();
        }
        DeskAction::NewFolder => match desktop.new_folder() {
            Ok(path) => *status = format!("Created {}", path.display()),
            Err(err) => *status = format!("New Folder failed: {err}"),
        },
        DeskAction::ToggleAutoArrange => {
            desktop.toggle_auto_arrange();
            *status = match desktop.arrange {
                DeskArrange::Auto => "Auto arrange on".into(),
                _ => "Auto arrange off — drag icons freely".into(),
            };
        }
        DeskAction::AlignToGrid => {
            desktop.align_to_grid();
            *status = "Icons aligned to grid".into();
        }
        DeskAction::Personalize => {
            settings.theme_id = settings.theme_id.next();
            *icons = IconPack::load_for_theme(settings.theme_id, &settings.icon_pack);
            let _ = settings.save();
            *status = format!("Theme: {}", settings.theme_id.label());
        }
        DeskAction::ShowDesktop => {
            toggle_show_desktop(taskbar, desktop, show_desktop, status);
        }
        DeskAction::Exit => *running = false,
    }
    Ok(())
}

fn apply_start_action(
    action: &StartAction,
    settings: &mut Settings,
    icons: &mut IconPack,
    taskbar: &mut Taskbar,
    status: &mut String,
    running: &mut bool,
) -> Result<()> {
    match action {
        StartAction::LaunchFm => {
            start_menu::launch_fm(None)?;
            note_running(taskbar, "fuckFM");
            *status = "Launched fuckFM".into();
        }
        StartAction::OpenThisPc => {
            start_menu::launch_fm_this_pc()?;
            note_running(taskbar, "This PC");
            *status = "Opened This PC".into();
        }
        StartAction::OpenDesktop => {
            if let Some(p) = start_menu::desktop_dir() {
                start_menu::launch_fm(Some(&p))?;
                note_running(taskbar, "Desktop");
                *status = "Opened Desktop".into();
            } else {
                *status = "Desktop folder missing".into();
            }
        }
        StartAction::OpenDocuments => {
            if let Some(p) = start_menu::documents_dir() {
                start_menu::launch_fm(Some(&p))?;
                note_running(taskbar, "Documents");
                *status = "Opened Documents".into();
            } else {
                *status = "Documents folder missing".into();
            }
        }
        StartAction::OpenPictures => {
            if let Some(p) = start_menu::pictures_dir() {
                start_menu::launch_fm(Some(&p))?;
                note_running(taskbar, "Pictures");
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
