//! Desktop wallpapers — OS wallpaper (Windows registry), theme defaults, optional override.

use crate::platform::wallpaper::{desktop_wallpaper, SystemWallpaper, WallpaperStyle};
use crate::render::{Frame, Rect};
use crate::theme::ThemeId;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Soft cap so we don't keep a full 4K RGBA buffer around.
const MAX_EDGE: u32 = 1600;

#[derive(Clone, Debug)]
pub struct WallpaperRgba {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Bumps whenever pixels change — used to refresh GPU/SDL texture caches.
    pub stamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SourceKey {
    Theme(ThemeId),
    Image {
        path: PathBuf,
        style: WallpaperStyle,
        bg: (u8, u8, u8),
        mtime: Option<SystemTime>,
    },
    Solid {
        bg: (u8, u8, u8),
    },
}

pub struct WallpaperCache {
    key: SourceKey,
    out_w: u32,
    out_h: u32,
    body: WallpaperRgba,
    next_stamp: u64,
}

impl WallpaperCache {
    pub fn new(theme: ThemeId) -> Self {
        let (w, h) = (64, 64);
        let mut next_stamp = 1;
        let body = make_default(theme, w, h, &mut next_stamp);
        Self {
            key: SourceKey::Theme(theme),
            out_w: w,
            out_h: h,
            body,
            next_stamp,
        }
    }

    /// Resolve wallpaper for the desk area.
    ///
    /// Priority: settings override → OS desktop wallpaper → theme default.
    pub fn ensure(
        &mut self,
        theme: ThemeId,
        override_path: Option<&Path>,
        area_w: u32,
        area_h: u32,
    ) {
        let (tw, th) = fit_size(area_w.max(1), area_h.max(1));
        let key = resolve_key(theme, override_path);
        if self.key == key && self.out_w == tw && self.out_h == th {
            return;
        }
        self.key = key.clone();
        self.out_w = tw;
        self.out_h = th;
        self.body = match key {
            SourceKey::Theme(id) => make_default(id, tw, th, &mut self.next_stamp),
            SourceKey::Solid { bg } => make_solid(bg, tw, th, &mut self.next_stamp),
            SourceKey::Image {
                path,
                style,
                bg,
                ..
            } => compose_image(&path, style, bg, tw, th, &mut self.next_stamp)
                .unwrap_or_else(|| make_default(theme, tw, th, &mut self.next_stamp)),
        };
    }

    pub fn body(&self) -> &WallpaperRgba {
        &self.body
    }

    /// Cover-blit into `area`.
    pub fn draw(&self, frame: &mut Frame<'_>, area: Rect) -> Result<()> {
        if area.w <= 0 || area.h <= 0 || self.body.width == 0 || self.body.height == 0 {
            return Ok(());
        }
        frame.fonts.blit_wallpaper(
            frame.canvas,
            self.body.stamp,
            self.body.width,
            self.body.height,
            area.x,
            area.y,
            area.w as u32,
            area.h as u32,
            &self.body.rgba,
        )
    }
}

fn resolve_key(theme: ThemeId, override_path: Option<&Path>) -> SourceKey {
    if let Some(path) = override_path {
        let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        return SourceKey::Image {
            path: path.to_path_buf(),
            style: WallpaperStyle::Fill,
            bg: (0, 0, 0),
            mtime,
        };
    }
    if let Some(sys) = desktop_wallpaper() {
        return system_key(sys);
    }
    SourceKey::Theme(theme)
}

fn system_key(sys: SystemWallpaper) -> SourceKey {
    match sys.path {
        Some(path) => {
            let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            SourceKey::Image {
                path,
                style: sys.style,
                bg: sys.background,
                mtime,
            }
        }
        None => SourceKey::Solid {
            bg: sys.background,
        },
    }
}

fn fit_size(w: u32, h: u32) -> (u32, u32) {
    let w = w.max(1);
    let h = h.max(1);
    if w <= MAX_EDGE && h <= MAX_EDGE {
        return (w, h);
    }
    let scale = (MAX_EDGE as f32 / w as f32).min(MAX_EDGE as f32 / h as f32);
    (
        ((w as f32) * scale).round().max(1.0) as u32,
        ((h as f32) * scale).round().max(1.0) as u32,
    )
}

fn bump(stamp: &mut u64) -> u64 {
    *stamp += 1;
    *stamp
}

fn make_solid(bg: (u8, u8, u8), w: u32, h: u32, stamp: &mut u64) -> WallpaperRgba {
    let mut rgba = Vec::with_capacity((w as usize) * (h as usize) * 4);
    for _ in 0..(w * h) {
        rgba.extend_from_slice(&[bg.0, bg.1, bg.2, 255]);
    }
    WallpaperRgba {
        width: w,
        height: h,
        rgba,
        stamp: bump(stamp),
    }
}

fn make_default(theme: ThemeId, w: u32, h: u32, stamp: &mut u64) -> WallpaperRgba {
    let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
    match theme {
        ThemeId::Win95 => paint_win95(&mut rgba, w, h),
        ThemeId::Win98 => paint_win98(&mut rgba, w, h),
        ThemeId::WinXpLuna => paint_xp(&mut rgba, w, h),
        ThemeId::Win7Aero => paint_aero(&mut rgba, w, h),
        ThemeId::ModernLight => paint_modern_light(&mut rgba, w, h),
        ThemeId::ModernDark => paint_modern_dark(&mut rgba, w, h),
    }
    WallpaperRgba {
        width: w,
        height: h,
        rgba,
        stamp: bump(stamp),
    }
}

fn compose_image(
    path: &Path,
    style: WallpaperStyle,
    bg: (u8, u8, u8),
    w: u32,
    h: u32,
    stamp: &mut u64,
) -> Option<WallpaperRgba> {
    let img = image::open(path).ok()?.to_rgba8();
    let (sw, sh) = img.dimensions();
    if sw == 0 || sh == 0 {
        return None;
    }

    let mut canvas = image::RgbaImage::from_pixel(w, h, image::Rgba([bg.0, bg.1, bg.2, 255]));

    match style {
        WallpaperStyle::Stretch => {
            let scaled = image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle);
            image::imageops::replace(&mut canvas, &scaled, 0, 0);
        }
        WallpaperStyle::Fill | WallpaperStyle::Span => {
            let scaled = image::DynamicImage::ImageRgba8(img.clone())
                .resize_to_fill(w, h, image::imageops::FilterType::Triangle)
                .to_rgba8();
            image::imageops::replace(&mut canvas, &scaled, 0, 0);
        }
        WallpaperStyle::Fit => {
            let scale = (w as f32 / sw as f32).min(h as f32 / sh as f32);
            let dw = ((sw as f32) * scale).round().max(1.0) as u32;
            let dh = ((sh as f32) * scale).round().max(1.0) as u32;
            let scaled =
                image::imageops::resize(&img, dw, dh, image::imageops::FilterType::Triangle);
            let x = (w.saturating_sub(dw) / 2) as i64;
            let y = (h.saturating_sub(dh) / 2) as i64;
            image::imageops::overlay(&mut canvas, &scaled, x, y);
        }
        WallpaperStyle::Center => {
            // Native pixels, centered (crop if larger than the desktop).
            let x = (w as i64 - sw as i64) / 2;
            let y = (h as i64 - sh as i64) / 2;
            image::imageops::overlay(&mut canvas, &img, x, y);
        }
        WallpaperStyle::Tile => {
            let mut y = 0i64;
            while y < h as i64 {
                let mut x = 0i64;
                while x < w as i64 {
                    image::imageops::overlay(&mut canvas, &img, x, y);
                    x += sw as i64;
                }
                y += sh as i64;
            }
        }
    }

    Some(WallpaperRgba {
        width: w,
        height: h,
        rgba: canvas.into_raw(),
        stamp: bump(stamp),
    })
}

fn put(px: &mut [u8], w: u32, x: u32, y: u32, r: u8, g: u8, b: u8) {
    let i = ((y * w + x) as usize) * 4;
    px[i] = r;
    px[i + 1] = g;
    px[i + 2] = b;
    px[i + 3] = 255;
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

fn mix(c0: (u8, u8, u8), c1: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    (lerp(c0.0, c1.0, t), lerp(c0.1, c1.1, t), lerp(c0.2, c1.2, t))
}

/// Classic teal field with a soft vignette (Win95 default energy).
fn paint_win95(px: &mut [u8], w: u32, h: u32) {
    let base = (0x00, 0x80, 0x80);
    let edge = (0x00, 0x55, 0x55);
    let cx = (w as f32) * 0.5;
    let cy = (h as f32) * 0.5;
    let max_d = (cx * cx + cy * cy).sqrt().max(1.0);
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let t = ((dx * dx + dy * dy).sqrt() / max_d).powf(1.35);
            let (r, g, b) = mix(base, edge, t);
            put(px, w, x, y, r, g, b);
        }
    }
}

/// Teal sky with soft cloudy blobs (Win98-ish).
fn paint_win98(px: &mut [u8], w: u32, h: u32) {
    let top = (0x2A, 0xA0, 0xA8);
    let bot = (0x00, 0x6E, 0x6E);
    for y in 0..h {
        let t = y as f32 / (h - 1).max(1) as f32;
        let (br, bg, bb) = mix(top, bot, t);
        for x in 0..w {
            let mut r = br;
            let mut g = bg;
            let mut b = bb;
            for (cx, cy, rad, strength) in [
                (0.22, 0.28, 0.18, 0.35),
                (0.55, 0.22, 0.22, 0.28),
                (0.78, 0.35, 0.16, 0.30),
                (0.40, 0.45, 0.14, 0.22),
            ] {
                let dx = x as f32 / w as f32 - cx;
                let dy = (y as f32 / h as f32 - cy) * 1.35;
                let d = (dx * dx + dy * dy).sqrt() / rad;
                if d < 1.0 {
                    let k = (1.0 - d * d) * strength;
                    r = lerp(r, 0xD8, k);
                    g = lerp(g, 0xEC, k);
                    b = lerp(b, 0xE8, k);
                }
            }
            put(px, w, x, y, r, g, b);
        }
    }
}

/// Original Bliss-inspired hill + sky (not the MS bitmap).
fn paint_xp(px: &mut [u8], w: u32, h: u32) {
    let sky_top = (0x3A, 0x8F, 0xD9);
    let sky_bot = (0xB8, 0xD8, 0xF0);
    let hill = (0x3B, 0x8C, 0x2A);
    let hill_deep = (0x2A, 0x68, 0x1C);
    let sun = (0xFF, 0xF2, 0xA8);
    let hill_y = |x: f32| {
        let n = (x * std::f32::consts::TAU * 0.85).sin() * 0.04
            + (x * std::f32::consts::TAU * 1.7).sin() * 0.02;
        0.58 + n
    };
    for y in 0..h {
        let v = y as f32 / (h - 1).max(1) as f32;
        for x in 0..w {
            let u = x as f32 / (w - 1).max(1) as f32;
            let ridge = hill_y(u);
            let (mut r, mut g, mut b) = if v < ridge {
                let t = (v / ridge).clamp(0.0, 1.0);
                mix(sky_top, sky_bot, t.powf(0.85))
            } else {
                let t = ((v - ridge) / (1.0 - ridge).max(0.001)).clamp(0.0, 1.0);
                mix(hill, hill_deep, t)
            };
            let sdx = u - 0.82;
            let sdy = v - 0.18;
            let sd = (sdx * sdx + sdy * sdy).sqrt();
            if sd < 0.22 {
                let k = (1.0 - sd / 0.22).powf(1.6) * 0.55;
                r = lerp(r, sun.0, k);
                g = lerp(g, sun.1, k);
                b = lerp(b, sun.2, k);
            }
            put(px, w, x, y, r, g, b);
        }
    }
}

/// Soft Aero blue with light streaks.
fn paint_aero(px: &mut [u8], w: u32, h: u32) {
    let c0 = (0x1C, 0x4F, 0x8A);
    let c1 = (0x5B, 0x9B, 0xD5);
    let c2 = (0xA8, 0xCC, 0xEA);
    for y in 0..h {
        for x in 0..w {
            let u = x as f32 / (w - 1).max(1) as f32;
            let v = y as f32 / (h - 1).max(1) as f32;
            let diagonal = ((u + v) * 0.5).clamp(0.0, 1.0);
            let (mut r, mut g, mut b) = if diagonal < 0.55 {
                mix(c0, c1, diagonal / 0.55)
            } else {
                mix(c1, c2, (diagonal - 0.55) / 0.45)
            };
            let beam = ((u * 0.7 + 0.15) - v).abs();
            if beam < 0.12 {
                let k = (1.0 - beam / 0.12) * 0.25;
                r = lerp(r, 0xF0, k);
                g = lerp(g, 0xF6, k);
                b = lerp(b, 0xFF, k);
            }
            put(px, w, x, y, r, g, b);
        }
    }
}

fn paint_modern_light(px: &mut [u8], w: u32, h: u32) {
    let top = (0xF2, 0xF4, 0xF7);
    let bot = (0xD5, 0xDC, 0xE6);
    let accent = (0x5B, 0x9F, 0xD6);
    for y in 0..h {
        let v = y as f32 / (h - 1).max(1) as f32;
        let (br, bg, bb) = mix(top, bot, v);
        for x in 0..w {
            let u = x as f32 / (w - 1).max(1) as f32;
            let mut r = br;
            let mut g = bg;
            let mut b = bb;
            let d = ((u - 1.05).powi(2) + (v + 0.05).powi(2)).sqrt();
            if d < 0.85 {
                let k = (1.0 - d / 0.85) * 0.18;
                r = lerp(r, accent.0, k);
                g = lerp(g, accent.1, k);
                b = lerp(b, accent.2, k);
            }
            put(px, w, x, y, r, g, b);
        }
    }
}

fn paint_modern_dark(px: &mut [u8], w: u32, h: u32) {
    let top = (0x1A, 0x1C, 0x22);
    let bot = (0x0C, 0x0D, 0x10);
    let accent = (0x3A, 0x6E, 0xA5);
    for y in 0..h {
        let v = y as f32 / (h - 1).max(1) as f32;
        let (br, bg, bb) = mix(top, bot, v);
        for x in 0..w {
            let u = x as f32 / (w - 1).max(1) as f32;
            let mut r = br;
            let mut g = bg;
            let mut b = bb;
            let d = ((u + 0.05).powi(2) + (v - 1.05).powi(2)).sqrt();
            if d < 0.9 {
                let k = (1.0 - d / 0.9) * 0.22;
                r = lerp(r, accent.0, k);
                g = lerp(g, accent.1, k);
                b = lerp(b, accent.2, k);
            }
            put(px, w, x, y, r, g, b);
        }
    }
}
