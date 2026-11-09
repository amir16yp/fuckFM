//! X11 reparenting window manager (fuckWM) with optional EGL/GLES accel.
//!
//! ```text
//! cargo run --bin fuckwm -- --xephyr          # nested + Glamor when available
//! Xephyr :1 -glamor -screen 1280x800 &
//! DISPLAY=:1 cargo run --bin fuckwm
//! ```

use crate::settings::Settings;
use crate::theme::{theme, Theme};
use crate::wm::gpu::{open_shared_display, Gpu, GpuColor, SharedX11, SurfaceId};
use crate::wm::wallpaper::WallpaperCache;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::process::{Command, Stdio};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use x11rb::xcb_ffi::XCBConnection;
use x11rb::COPY_DEPTH_FROM_PARENT;

const TITLE_H: u16 = 28;
const TASKBAR_H: u16 = 36;
const BORDER: u16 = 2;

struct Client {
    frame: Window,
    #[allow(dead_code)]
    win: Window,
    frame_gpu: Option<SurfaceId>,
}

struct Wm {
    shared: SharedX11,
    root: Window,
    root_w: u16,
    root_h: u16,
    clients: HashMap<Window, Client>,
    frames: HashMap<Window, Window>,
    taskbar: Window,
    desktop: Window,
    theme: Theme,
    settings: Settings,
    wallpaper: WallpaperCache,
    drag: Option<Drag>,
    running: bool,
    gpu: Option<Gpu>,
    taskbar_gpu: Option<SurfaceId>,
    visual: Option<Visualid>,
    depth: u8,
    colormap: Option<Colormap>,
    /// Root visual masks for PutImage wallpaper packing.
    red_mask: u32,
    green_mask: u32,
    blue_mask: u32,
    bits_per_pixel: u8,
}

struct Drag {
    frame: Window,
    ox: i16,
    oy: i16,
    fx: i32,
    fy: i32,
}

pub fn run(product: &str) -> Result<()> {
    let settings = Settings::load_or_default();
    let th = theme(settings.theme_id);

    // Shared Xlib+xcb connection so EGL window surfaces match WM windows.
    let shared = open_shared_display().context("open shared X11 display")?;
    let mut gpu = Gpu::from_shared(&shared).ok();
    let screen_num = shared.screen_num;
    let root = shared.conn.setup().roots[screen_num].root;
    let root_w = shared.conn.setup().roots[screen_num].width_in_pixels;
    let root_h = shared.conn.setup().roots[screen_num].height_in_pixels;
    let root_depth = shared.conn.setup().roots[screen_num].root_depth;

    let mask = EventMask::SUBSTRUCTURE_REDIRECT
        | EventMask::SUBSTRUCTURE_NOTIFY
        | EventMask::BUTTON_PRESS
        | EventMask::BUTTON_RELEASE
        | EventMask::POINTER_MOTION
        | EventMask::KEY_PRESS
        | EventMask::ENTER_WINDOW;
    let aux = ChangeWindowAttributesAux::new().event_mask(mask);
    match shared.conn.change_window_attributes(root, &aux) {
        Ok(cookie) => {
            if let Err(err) = cookie.check() {
                bail!("another WM is running (SubstructureRedirect denied): {err}");
            }
        }
        Err(err) => bail!("cannot become WM: {err}"),
    }
    shared.conn.flush()?;

    let (visual, depth, colormap) = if let Some(g) = gpu.as_ref() {
        let vid = g.visual_id().unwrap_or(0);
        let screen = &shared.conn.setup().roots[screen_num];
        if let Some((d, _)) = depth_for_visual(screen, vid) {
            let cmap = shared.conn.generate_id()?;
            shared
                .conn
                .create_colormap(ColormapAlloc::NONE, cmap, root, vid)?;
            (Some(vid), d, Some(cmap))
        } else {
            eprintln!("fuckWM: EGL visual {vid:#x} missing on screen; GPU chrome disabled");
            gpu = None;
            (None, root_depth, None)
        }
    } else {
        (None, root_depth, None)
    };

    let screen = &shared.conn.setup().roots[screen_num];
    let (red_mask, green_mask, blue_mask, bits_per_pixel) = visual_pack_info(screen, visual);

    let mut wm = Wm {
        shared,
        root,
        root_w,
        root_h,
        clients: HashMap::new(),
        frames: HashMap::new(),
        taskbar: 0,
        desktop: 0,
        theme: th,
        wallpaper: WallpaperCache::new(settings.theme_id),
        settings,
        drag: None,
        running: true,
        gpu,
        taskbar_gpu: None,
        visual,
        depth,
        colormap,
        red_mask,
        green_mask,
        blue_mask,
        bits_per_pixel,
    };

    wm.paint_root()?;
    wm.desktop = wm.create_desktop()?;
    wm.taskbar = wm.create_taskbar(product)?;
    wm.adopt_existing()?;
    let accel = if wm.gpu.is_some() { "EGL/GLES2" } else { "software" };
    eprintln!(
        "{product}: X11 WM on {}×{} ({accel}; taskbar {}, desktop {})",
        root_w, root_h, wm.taskbar, wm.desktop
    );

    while wm.running {
        wm.shared.conn.flush()?;
        let event = wm.shared.conn.wait_for_event().context("X11 event")?;
        wm.handle(event)?;
    }
    Ok(())
}

impl Wm {
    fn conn(&self) -> &XCBConnection {
        &self.shared.conn
    }

    fn paint_root(&self) -> Result<()> {
        let pixel = rgb_pixel(self.theme.desktop);
        let aux = ChangeWindowAttributesAux::new().background_pixel(pixel);
        self.conn().change_window_attributes(self.root, &aux)?;
        self.conn().clear_area(false, self.root, 0, 0, 0, 0)?;
        self.conn().flush()?;
        Ok(())
    }

    fn create_shell_window(
        &self,
        x: i16,
        y: i16,
        w: u16,
        h: u16,
        bg: u32,
        events: EventMask,
    ) -> Result<Window> {
        let win = self.conn().generate_id()?;
        let mut aux = CreateWindowAux::new()
            .background_pixel(bg)
            .border_pixel(bg)
            .event_mask(events)
            .override_redirect(1);
        let (depth, visual) = if let (Some(v), Some(cmap)) = (self.visual, self.colormap) {
            aux = aux.colormap(cmap);
            (self.depth, v)
        } else {
            (COPY_DEPTH_FROM_PARENT, 0)
        };
        self.conn().create_window(
            depth,
            win,
            self.root,
            x,
            y,
            w,
            h,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &aux,
        )?;
        Ok(win)
    }

    fn create_desktop(&mut self) -> Result<Window> {
        let h = self.root_h.saturating_sub(TASKBAR_H);
        let pixel = rgb_pixel(self.theme.desktop);
        let win = self.create_shell_window(
            0,
            0,
            self.root_w,
            h,
            pixel,
            EventMask::BUTTON_PRESS | EventMask::EXPOSURE,
        )?;
        self.conn().map_window(win)?;
        self.conn().configure_window(
            win,
            &ConfigureWindowAux::new().stack_mode(StackMode::BELOW),
        )?;
        // Desktop stays on X PutImage (wallpaper) — no EGL surface so the bitmap isn't overwritten.
        self.desktop = win;
        let _ = self.paint_desktop_wallpaper();
        Ok(win)
    }

    fn paint_desktop_wallpaper(&mut self) -> Result<()> {
        let win = self.desktop;
        if win == 0 {
            return Ok(());
        }
        let h = self.root_h.saturating_sub(TASKBAR_H);
        let w = self.root_w;
        self.wallpaper.ensure(
            self.settings.theme_id,
            self.settings.wallpaper.as_deref(),
            w as u32,
            h as u32,
        );
        let (src_w, src_h, rgba) = {
            let body = self.wallpaper.body();
            if body.width == 0 || body.height == 0 || body.rgba.len() < 4 {
                return Ok(());
            }
            (body.width, body.height, body.rgba.clone())
        };

        let gc = self.conn().generate_id()?;
        self.conn()
            .create_gc(gc, win, &CreateGCAux::new().graphics_exposures(0))?;

        // PutImage in strips so we stay under the max request size.
        const STRIP: u32 = 32;
        let red = self.red_mask;
        let green = self.green_mask;
        let blue = self.blue_mask;
        let bpp = self.bits_per_pixel;
        let depth = self.depth;
        for y0 in (0..h).step_by(STRIP as usize) {
            let rows = (STRIP as u16).min(h - y0);
            let data = pack_strip(
                &rgba,
                src_w,
                src_h,
                w as u32,
                h as u32,
                y0 as u32,
                rows as u32,
                red,
                green,
                blue,
                bpp,
            );
            self.conn().put_image(
                ImageFormat::Z_PIXMAP,
                win,
                gc,
                w,
                rows,
                0,
                y0 as i16,
                0,
                depth,
                &data,
            )?;
        }
        self.conn().free_gc(gc)?;
        self.conn().flush()?;
        Ok(())
    }

    fn create_taskbar(&mut self, product: &str) -> Result<Window> {
        let y = self.root_h.saturating_sub(TASKBAR_H) as i16;
        let pixel = rgb_pixel(self.theme.chrome.face);
        let win = self.create_shell_window(
            0,
            y,
            self.root_w,
            TASKBAR_H,
            pixel,
            EventMask::BUTTON_PRESS | EventMask::EXPOSURE | EventMask::ENTER_WINDOW,
        )?;
        self.conn().map_window(win)?;
        self.conn().configure_window(
            win,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )?;
        if let Some(gpu) = self.gpu.as_mut() {
            match gpu.create_window_surface(win, self.root_w as u32, TASKBAR_H as u32) {
                Ok(id) => {
                    self.taskbar_gpu = Some(id);
                }
                Err(err) => eprintln!("fuckWM: taskbar EGL surface failed: {err}"),
            }
        }
        self.draw_taskbar(win, product)?;
        Ok(win)
    }

    fn draw_taskbar(&mut self, win: Window, product: &str) -> Result<()> {
        if let (Some(gpu), Some(id)) = (self.gpu.as_mut(), self.taskbar_gpu) {
            let w = self.root_w as f32;
            let h = TASKBAR_H as f32;
            let btn = GpuColor::from_sdl(self.theme.button.face);
            let rects = [
                (4.0 / w, 4.0 / h, 72.0 / w, (h - 8.0) / h, btn),
                (80.0 / w, 4.0 / h, 80.0 / w, (h - 8.0) / h, btn),
            ];
            gpu.clear_and_rects(id, GpuColor::from_sdl(self.theme.chrome.face), &rects)?;
            let _ = product;
            return Ok(());
        }
        // Software fallback.
        let gc = self.conn().generate_id()?;
        let face = rgb_pixel(self.theme.chrome.face);
        let text = rgb_pixel(self.theme.text);
        let btn = rgb_pixel(self.theme.button.face);
        self.conn().create_gc(
            gc,
            win,
            &CreateGCAux::new()
                .foreground(face)
                .background(face)
                .line_width(1),
        )?;
        self.conn().poly_fill_rectangle(
            win,
            gc,
            &[Rectangle {
                x: 0,
                y: 0,
                width: self.root_w,
                height: TASKBAR_H,
            }],
        )?;
        self.conn()
            .change_gc(gc, &ChangeGCAux::new().foreground(btn))?;
        self.conn().poly_fill_rectangle(
            win,
            gc,
            &[
                Rectangle {
                    x: 4,
                    y: 4,
                    width: 72,
                    height: TASKBAR_H - 8,
                },
                Rectangle {
                    x: 80,
                    y: 4,
                    width: 80,
                    height: TASKBAR_H - 8,
                },
            ],
        )?;
        let _ = self.draw_string(win, gc, 14, (TASKBAR_H as i16 / 2) + 4, "Start", text);
        let _ = self.draw_string(win, gc, 90, (TASKBAR_H as i16 / 2) + 4, "fuckFM", text);
        let _ = self.draw_string(
            win,
            gc,
            (self.root_w as i16) - 120,
            (TASKBAR_H as i16 / 2) + 4,
            product,
            text,
        );
        self.conn().free_gc(gc)?;
        Ok(())
    }

    fn draw_string(
        &self,
        win: Window,
        gc: Gcontext,
        x: i16,
        y: i16,
        text: &str,
        pixel: u32,
    ) -> Result<()> {
        self.conn()
            .change_gc(gc, &ChangeGCAux::new().foreground(pixel))?;
        let font = self.conn().generate_id()?;
        if self.conn().open_font(font, b"fixed").is_ok() {
            let _ = self.conn().change_gc(gc, &ChangeGCAux::new().font(font));
            let _ = self.conn().image_text8(win, gc, x, y, text.as_bytes());
            let _ = self.conn().close_font(font);
        }
        Ok(())
    }

    fn adopt_existing(&mut self) -> Result<()> {
        let tree = self.conn().query_tree(self.root)?.reply()?;
        for win in tree.children {
            if win == self.desktop || win == self.taskbar {
                continue;
            }
            let attrs = match self.conn().get_window_attributes(win)?.reply() {
                Ok(a) => a,
                Err(_) => continue,
            };
            if attrs.override_redirect || attrs.map_state != MapState::VIEWABLE {
                continue;
            }
            let _ = self.manage(win);
        }
        Ok(())
    }

    fn manage(&mut self, win: Window) -> Result<()> {
        if self.clients.contains_key(&win) || win == self.desktop || win == self.taskbar {
            return Ok(());
        }
        let attrs = self.conn().get_window_attributes(win)?.reply()?;
        if attrs.override_redirect {
            self.conn().map_window(win)?;
            return Ok(());
        }
        let geom = self.conn().get_geometry(win)?.reply()?;
        let usable_h = self.root_h.saturating_sub(TASKBAR_H);
        let mut bw = geom.width.max(120);
        let mut bh = geom.height.max(80);
        let mut bx = geom.x.max(0);
        let mut by = geom.y.max(0);
        if bx as u16 + bw > self.root_w {
            bx = 0;
        }
        if by as u16 + bh + TITLE_H > usable_h {
            by = 0;
        }
        if bh + TITLE_H > usable_h {
            bh = usable_h.saturating_sub(TITLE_H).max(80);
        }
        if bw > self.root_w {
            bw = self.root_w;
        }

        let frame = self.conn().generate_id()?;
        let face = rgb_pixel(self.theme.chrome.face);
        let mut aux = CreateWindowAux::new()
            .background_pixel(face)
            .border_pixel(face)
            .event_mask(
                EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | EventMask::POINTER_MOTION
                    | EventMask::SUBSTRUCTURE_NOTIFY
                    | EventMask::SUBSTRUCTURE_REDIRECT
                    | EventMask::EXPOSURE
                    | EventMask::ENTER_WINDOW,
            );
        let (depth, visual) = if let (Some(v), Some(cmap)) = (self.visual, self.colormap) {
            aux = aux.colormap(cmap);
            (self.depth, v)
        } else {
            (COPY_DEPTH_FROM_PARENT, 0)
        };
        let fw = bw + BORDER * 2;
        let fh = bh + TITLE_H + BORDER;
        self.conn().create_window(
            depth,
            frame,
            self.root,
            bx,
            by,
            fw,
            fh,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &aux,
        )?;

        self.conn()
            .reparent_window(win, frame, BORDER as i16, TITLE_H as i16)?;
        self.conn().configure_window(
            win,
            &ConfigureWindowAux::new()
                .x(BORDER as i32)
                .y(TITLE_H as i32)
                .width(bw as u32)
                .height(bh as u32)
                .border_width(0),
        )?;
        self.conn().grab_button(
            false,
            frame,
            EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            0u32,
            0u32,
            ButtonIndex::M1,
            ModMask::ANY,
        )?;
        self.conn().map_window(win)?;
        self.conn().map_window(frame)?;

        let mut frame_gpu = None;
        if let Some(gpu) = self.gpu.as_mut() {
            if let Ok(id) = gpu.create_window_surface(frame, fw as u32, fh as u32) {
                frame_gpu = Some(id);
            }
        }
        self.frames.insert(frame, win);
        self.clients.insert(
            win,
            Client {
                frame,
                win,
                frame_gpu,
            },
        );
        self.draw_frame(frame, win)?;
        self.conn().configure_window(
            self.taskbar,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )?;
        Ok(())
    }

    fn draw_frame(&mut self, frame: Window, client: Window) -> Result<()> {
        let geom = self.conn().get_geometry(frame)?.reply()?;
        let frame_gpu = self
            .clients
            .get(&client)
            .and_then(|c| c.frame_gpu)
            .or_else(|| {
                // Lookup via frame map if client key path missed.
                self.frames
                    .get(&frame)
                    .and_then(|w| self.clients.get(w))
                    .and_then(|c| c.frame_gpu)
            });

        if let (Some(gpu), Some(id)) = (self.gpu.as_mut(), frame_gpu) {
            let _ = gpu.resize(id, geom.width as u32, geom.height as u32);
            let w = geom.width as f32;
            let h = geom.height as f32;
            let title = GpuColor::from_sdl(self.theme.title.active_left);
            let close = GpuColor::from_sdl(self.theme.button.face);
            let face = GpuColor::from_sdl(self.theme.chrome.face);
            let th = TITLE_H as f32 / h;
            let close_x = (w - TITLE_H as f32 + 4.0) / w;
            let rects = [
                (0.0, 0.0, 1.0, th, title),
                (
                    close_x,
                    4.0 / h,
                    (TITLE_H as f32 - 8.0) / w,
                    (TITLE_H as f32 - 8.0) / h,
                    close,
                ),
            ];
            gpu.clear_and_rects(id, face, &rects)?;
            return Ok(());
        }

        let title_px = rgb_pixel(self.theme.title.active_left);
        let text_px = rgb_pixel(self.theme.title.text);
        let gc = self.conn().generate_id()?;
        self.conn()
            .create_gc(gc, frame, &CreateGCAux::new().foreground(title_px))?;
        self.conn().poly_fill_rectangle(
            frame,
            gc,
            &[Rectangle {
                x: 0,
                y: 0,
                width: geom.width,
                height: TITLE_H,
            }],
        )?;
        let name = window_name(self.conn(), client).unwrap_or_else(|| "Window".into());
        let _ = self.draw_string(frame, gc, 8, (TITLE_H as i16 / 2) + 4, &name, text_px);
        let close_px = rgb_pixel(self.theme.button.face);
        self.conn()
            .change_gc(gc, &ChangeGCAux::new().foreground(close_px))?;
        let cx = geom.width.saturating_sub(TITLE_H - 4) as i16;
        self.conn().poly_fill_rectangle(
            frame,
            gc,
            &[Rectangle {
                x: cx,
                y: 4,
                width: TITLE_H - 8,
                height: TITLE_H - 8,
            }],
        )?;
        self.conn().free_gc(gc)?;
        Ok(())
    }

    fn unmanage(&mut self, win: Window) -> Result<()> {
        if let Some(client) = self.clients.remove(&win) {
            self.frames.remove(&client.frame);
            if let (Some(gpu), Some(id)) = (self.gpu.as_mut(), client.frame_gpu) {
                gpu.destroy_surface(id);
            }
            let _ = self.conn().destroy_window(client.frame);
        }
        Ok(())
    }

    fn handle(&mut self, event: Event) -> Result<()> {
        match event {
            Event::MapRequest(ev) => self.manage(ev.window)?,
            Event::ConfigureRequest(ev) => self.on_configure_request(&ev)?,
            Event::DestroyNotify(ev) => self.unmanage(ev.window)?,
            Event::UnmapNotify(_) => {}
            Event::Expose(ev) => {
                if ev.window == self.taskbar && ev.count == 0 {
                    self.draw_taskbar(self.taskbar, "fuckWM")?;
                } else if ev.window == self.desktop && ev.count == 0 {
                    let _ = self.paint_desktop_wallpaper();
                } else if let Some(&client) = self.frames.get(&ev.window) {
                    if ev.count == 0 {
                        self.draw_frame(ev.window, client)?;
                    }
                }
            }
            Event::ButtonPress(ev) => self.on_button_press(&ev)?,
            Event::ButtonRelease(ev) => {
                if ev.detail == 1 {
                    self.drag = None;
                }
            }
            Event::MotionNotify(ev) => self.on_motion(&ev)?,
            Event::EnterNotify(ev) => {
                if let Some(&client) = self.frames.get(&ev.event) {
                    let _ = self.conn().set_input_focus(InputFocus::POINTER_ROOT, client, 0u32);
                    self.conn().configure_window(
                        ev.event,
                        &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
                    )?;
                    self.conn().configure_window(
                        self.taskbar,
                        &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
                    )?;
                }
            }
            Event::KeyPress(ev) => {
                if ev.detail == 9 || ev.detail == 24 {
                    self.running = false;
                }
            }
            Event::Error(err) => eprintln!("X11 error: {err:?}"),
            _ => {}
        }
        Ok(())
    }

    fn on_configure_request(&mut self, ev: &ConfigureRequestEvent) -> Result<()> {
        if let Some(client) = self.clients.get(&ev.window).map(|c| c.frame) {
            let mut aux = ConfigureWindowAux::new();
            if ev.value_mask.contains(ConfigWindow::WIDTH) {
                aux = aux.width(ev.width as u32);
            }
            if ev.value_mask.contains(ConfigWindow::HEIGHT) {
                aux = aux.height(ev.height as u32);
            }
            self.conn().configure_window(ev.window, &aux)?;
            let gw = self.conn().get_geometry(ev.window)?.reply()?;
            self.conn().configure_window(
                client,
                &ConfigureWindowAux::new()
                    .width((gw.width + BORDER * 2) as u32)
                    .height((gw.height + TITLE_H + BORDER) as u32),
            )?;
        } else {
            let aux = ConfigureWindowAux::from_configure_request(ev);
            self.conn().configure_window(ev.window, &aux)?;
        }
        Ok(())
    }

    fn on_button_press(&mut self, ev: &ButtonPressEvent) -> Result<()> {
        if ev.event == self.taskbar {
            if (4..160).contains(&ev.event_x) {
                let _ = launch_fm(None);
            }
            return Ok(());
        }
        if ev.event == self.desktop {
            return Ok(());
        }
        if let Some(&client) = self.frames.get(&ev.event) {
            let geom = self.conn().get_geometry(ev.event)?.reply()?;
            if ev.event_y < TITLE_H as i16
                && ev.event_x >= geom.width.saturating_sub(TITLE_H - 4) as i16
            {
                self.conn().kill_client(client)?;
                return Ok(());
            }
            if ev.detail == 1 && ev.event_y < TITLE_H as i16 {
                self.drag = Some(Drag {
                    frame: ev.event,
                    ox: ev.root_x,
                    oy: ev.root_y,
                    fx: geom.x as i32,
                    fy: geom.y as i32,
                });
                self.conn().configure_window(
                    ev.event,
                    &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
                )?;
                self.conn().configure_window(
                    self.taskbar,
                    &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
                )?;
                let _ = self.conn().set_input_focus(InputFocus::POINTER_ROOT, client, 0u32);
            }
        }
        Ok(())
    }

    fn on_motion(&mut self, ev: &MotionNotifyEvent) -> Result<()> {
        let Some(drag) = &self.drag else {
            return Ok(());
        };
        let frame = drag.frame;
        let nx = drag.fx + (ev.root_x - drag.ox) as i32;
        let ny = drag.fy + (ev.root_y - drag.oy) as i32;
        let max_y = self.root_h.saturating_sub(TASKBAR_H) as i32 - 8;
        self.conn().configure_window(
            frame,
            &ConfigureWindowAux::new()
                .x(nx.max(0))
                .y(ny.clamp(0, max_y)),
        )?;
        Ok(())
    }
}

fn depth_for_visual(screen: &Screen, visual: Visualid) -> Option<(u8, Visualtype)> {
    for depth in &screen.allowed_depths {
        for vis in &depth.visuals {
            if vis.visual_id == visual {
                return Some((depth.depth, *vis));
            }
        }
    }
    None
}

fn visual_pack_info(screen: &Screen, visual: Option<Visualid>) -> (u32, u32, u32, u8) {
    let vid = visual.unwrap_or(screen.root_visual);
    if let Some((_, vis)) = depth_for_visual(screen, vid) {
        // TrueColor / DirectColor: pack via masks. Depth-24 screens almost always use 32 bpp.
        return (vis.red_mask, vis.green_mask, vis.blue_mask, 32);
    }
    // Fallback: 8:8:8 in low bytes (Xorg little-endian default-ish).
    (0xFF0000, 0x00FF00, 0x0000FF, 32)
}

fn shift_into_mask(val: u8, mask: u32) -> u32 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let bits = mask.count_ones().min(8);
    let v = if bits >= 8 {
        val as u32
    } else {
        (val as u32) >> (8 - bits)
    };
    (v << shift) & mask
}

/// Scale-sample wallpaper RGBA into a horizontal strip of native ZPixmap bytes.
fn pack_strip(
    rgba: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
    y0: u32,
    rows: u32,
    red_mask: u32,
    green_mask: u32,
    blue_mask: u32,
    bits_per_pixel: u8,
) -> Vec<u8> {
    let bpp = (bits_per_pixel / 8).max(3) as usize;
    // X bitmap pad is typically 32-bit — stride in bytes.
    let stride = ((dst_w as usize * bpp + 3) / 4) * 4;
    let mut out = vec![0u8; stride * rows as usize];
    for row in 0..rows {
        let dy = y0 + row;
        let sy = ((dy as u64 * src_h as u64) / dst_h.max(1) as u64).min(src_h.saturating_sub(1) as u64)
            as u32;
        for dx in 0..dst_w {
            let sx = ((dx as u64 * src_w as u64) / dst_w.max(1) as u64).min(src_w.saturating_sub(1) as u64)
                as u32;
            let si = ((sy * src_w + sx) as usize) * 4;
            if si + 2 >= rgba.len() {
                continue;
            }
            let r = rgba[si];
            let g = rgba[si + 1];
            let b = rgba[si + 2];
            let pixel = shift_into_mask(r, red_mask)
                | shift_into_mask(g, green_mask)
                | shift_into_mask(b, blue_mask);
            let oi = row as usize * stride + dx as usize * bpp;
            if oi + bpp <= out.len() {
                let bytes = pixel.to_ne_bytes();
                out[oi..oi + bpp].copy_from_slice(&bytes[..bpp]);
            }
        }
    }
    out
}

fn window_name(conn: &XCBConnection, win: Window) -> Option<String> {
    let reply = conn
        .get_property(false, win, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 256)
        .ok()?
        .reply()
        .ok()?;
    if reply.value.is_empty() {
        return None;
    }
    String::from_utf8(reply.value).ok()
}

fn rgb_pixel(c: sdl2::pixels::Color) -> u32 {
    ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32)
}

fn launch_fm(path: Option<&std::path::Path>) -> Result<()> {
    let exe = std::env::current_exe()?;
    let fm = exe.with_file_name("fuckfm");
    let mut cmd = Command::new(if fm.is_file() { fm } else { exe });
    if let Some(path) = path {
        cmd.arg(path);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}
