use crate::theme::FontKind;
use anyhow::{anyhow, Context, Result};
use fontdue::Font;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::render::{BlendMode, Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};
use std::collections::HashMap;
use std::path::PathBuf;

fn glyph_key(kind: FontKind, size: f32, ch: char) -> u64 {
    let kind_bits = match kind {
        FontKind::ClassicBitmap => 0u64,
        FontKind::Sans => 1u64,
    };
    let size_bits = (size * 10.0).round() as u64 & 0xFFFF;
    let ch_bits = ch as u64;
    (kind_bits << 48) | (size_bits << 32) | ch_bits
}

struct Glyph {
    texture: Texture<'static>,
    width: i32,
    height: i32,
    bearing_y: i32,
}

struct WallpaperTex {
    stamp: u64,
    src_w: u32,
    src_h: u32,
    texture: Texture<'static>,
}

// SAFETY: textures are owned by FontAtlas and never outlive the TextureCreator
// held in the same struct. We transmute lifetimes carefully below.
pub struct FontAtlas {
    classic: Font,
    sans: Font,
    texture_creator: TextureCreator<WindowContext>,
    cache: HashMap<u64, Glyph>,
    wallpaper: Option<WallpaperTex>,
}

impl FontAtlas {
    pub fn new(canvas: &Canvas<Window>) -> Result<Self> {
        let classic_bytes = load_font_bytes("classic.ttf")
            .context("load classic.ttf (Win95/98 bitmap-style font)")?;
        let sans_bytes =
            load_font_bytes("ui.ttf").context("load ui.ttf (modern sans font)")?;

        let classic = Font::from_bytes(classic_bytes.as_slice(), fontdue::FontSettings::default())
            .map_err(|e| anyhow!("parse classic font: {e}"))?;
        let sans = Font::from_bytes(sans_bytes.as_slice(), fontdue::FontSettings::default())
            .map_err(|e| anyhow!("parse ui font: {e}"))?;

        Ok(Self {
            classic,
            sans,
            texture_creator: canvas.texture_creator(),
            cache: HashMap::new(),
            wallpaper: None,
        })
    }

    fn font(&self, kind: FontKind) -> &Font {
        match kind {
            FontKind::ClassicBitmap => &self.classic,
            FontKind::Sans => &self.sans,
        }
    }

    pub fn blit_rgba(
        &mut self,
        canvas: &mut Canvas<Window>,
        dx: i32,
        dy: i32,
        src_w: u32,
        src_h: u32,
        dst_w: u32,
        dst_h: u32,
        rgba: &[u8],
    ) -> Result<()> {
        let mut texture = self
            .texture_creator
            .create_texture_target(PixelFormatEnum::RGBA32, src_w, src_h)
            .map_err(|e| anyhow!("create rgba texture: {e}"))?;
        texture.set_blend_mode(BlendMode::Blend);
        texture
            .update(None, rgba, src_w as usize * 4)
            .map_err(|e| anyhow!("upload rgba: {e}"))?;
        let dest = sdl2::rect::Rect::new(dx, dy, dst_w, dst_h);
        canvas
            .copy(&texture, None, dest)
            .map_err(|e| anyhow!("blit rgba: {e}"))?;
        Ok(())
    }

    /// Stretch-blit a wallpaper; reuploads only when `stamp` / size changes.
    pub fn blit_wallpaper(
        &mut self,
        canvas: &mut Canvas<Window>,
        stamp: u64,
        src_w: u32,
        src_h: u32,
        dx: i32,
        dy: i32,
        dst_w: u32,
        dst_h: u32,
        rgba: &[u8],
    ) -> Result<()> {
        let needs = match &self.wallpaper {
            Some(w) if w.stamp == stamp && w.src_w == src_w && w.src_h == src_h => false,
            _ => true,
        };
        if needs {
            let mut texture = self
                .texture_creator
                .create_texture_target(PixelFormatEnum::RGBA32, src_w, src_h)
                .map_err(|e| anyhow!("create wallpaper texture: {e}"))?;
            texture.set_blend_mode(BlendMode::None);
            texture
                .update(None, rgba, src_w as usize * 4)
                .map_err(|e| anyhow!("upload wallpaper: {e}"))?;
            let texture: Texture<'static> = unsafe { std::mem::transmute(texture) };
            self.wallpaper = Some(WallpaperTex {
                stamp,
                src_w,
                src_h,
                texture,
            });
        }
        let tex = self
            .wallpaper
            .as_ref()
            .ok_or_else(|| anyhow!("wallpaper texture missing"))?;
        let dest = sdl2::rect::Rect::new(dx, dy, dst_w, dst_h);
        canvas
            .copy(&tex.texture, None, dest)
            .map_err(|e| anyhow!("blit wallpaper: {e}"))?;
        Ok(())
    }

    pub fn measure(&mut self, kind: FontKind, size: f32, text: &str) -> (i32, i32) {
        let font = self.font(kind);
        let mut width = 0i32;
        let mut height = 0i32;
        for ch in text.chars() {
            // metrics-only — never allocate a glyph bitmap just to measure.
            let metrics = font.metrics(ch, size);
            width += metrics.advance_width.ceil() as i32;
            height = height.max(metrics.height as i32);
        }
        (width, height.max(size.ceil() as i32))
    }

    pub fn draw_text(
        &mut self,
        canvas: &mut Canvas<Window>,
        kind: FontKind,
        size: f32,
        text: &str,
        mut x: i32,
        y: i32,
        color: Color,
    ) -> Result<(i32, i32)> {
        let mut max_h = 0i32;
        let start_x = x;
        for ch in text.chars() {
            if ch == '\n' {
                continue;
            }
            let advance = {
                let font = self.font(kind);
                font.metrics(ch, size).advance_width.ceil() as i32
            };
            let (gw, gh) = self.draw_glyph(canvas, kind, size, ch, x, y, color)?;
            x += advance.max(gw);
            max_h = max_h.max(gh);
        }
        Ok((x - start_x, max_h))
    }

    fn draw_glyph(
        &mut self,
        canvas: &mut Canvas<Window>,
        kind: FontKind,
        size: f32,
        ch: char,
        x: i32,
        y: i32,
        color: Color,
    ) -> Result<(i32, i32)> {
        let key = glyph_key(kind, size, ch);
        if !self.cache.contains_key(&key) {
            let glyph = self.rasterize(kind, size, ch)?;
            self.cache.insert(key, glyph);
        }

        let glyph = self.cache.get_mut(&key).unwrap();
        glyph.texture.set_color_mod(color.r, color.g, color.b);
        glyph.texture.set_alpha_mod(color.a);
        let dest = sdl2::rect::Rect::new(
            x,
            y + (size.ceil() as i32 - glyph.bearing_y).max(0),
            glyph.width.max(1) as u32,
            glyph.height.max(1) as u32,
        );
        canvas
            .copy(&glyph.texture, None, dest)
            .map_err(|e| anyhow!("blit glyph: {e}"))?;
        Ok((glyph.width, glyph.height))
    }

    fn rasterize(&mut self, kind: FontKind, size: f32, ch: char) -> Result<Glyph> {
        let font = self.font(kind);
        let (metrics, bitmap) = font.rasterize(ch, size);
        let w = metrics.width.max(1);
        let h = metrics.height.max(1);

        let mut rgba = vec![0u8; w * h * 4];
        for (i, coverage) in bitmap.iter().enumerate() {
            let o = i * 4;
            rgba[o] = 255;
            rgba[o + 1] = 255;
            rgba[o + 2] = 255;
            rgba[o + 3] = *coverage;
        }

        let mut texture = self
            .texture_creator
            .create_texture_target(PixelFormatEnum::RGBA32, w as u32, h as u32)
            .map_err(|e| anyhow!("create glyph texture: {e}"))?;
        texture.set_blend_mode(BlendMode::Blend);
        texture
            .update(None, &rgba, w * 4)
            .map_err(|e| anyhow!("upload glyph: {e}"))?;

        // Extend lifetime: texture is owned by this atlas alongside TextureCreator.
        let texture: Texture<'static> = unsafe { std::mem::transmute(texture) };

        Ok(Glyph {
            texture,
            width: w as i32,
            height: h as i32,
            bearing_y: metrics.height as i32 + metrics.ymin,
        })
    }
}

fn load_font_bytes(file_name: &str) -> Result<Vec<u8>> {
    for dir in asset_dirs() {
        let path = dir.join(file_name);
        if path.is_file() {
            return std::fs::read(&path).with_context(|| format!("read {}", path.display()));
        }
    }
    Err(anyhow!(
        "font '{file_name}' not found in {:?}",
        asset_dirs()
    ))
}

fn asset_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.join("assets"));
            dirs.push(parent.join("../assets"));
            dirs.push(parent.join("../../assets"));
        }
    }
    dirs.push(PathBuf::from("assets"));
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"));
    dirs
}