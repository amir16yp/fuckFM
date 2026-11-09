//! `#.fficons` — fuckFM binary icon pack.
//!
//! On-disk layout (little-endian):
//! ```text
//! magic            : 8 bytes  b"FFICONS\0"
//! version          : u16      = 1
//! flags            : u16      bit0 = zlib-compressed body
//! uncompressed_len : u32      length of inflated body
//! body             : zlib (or raw) bytes
//! ```
//!
//! Inflated body is a series of entries:
//! ```text
//! purpose_len : u16
//! purpose     : utf-8   (e.g. "places/folder@48")
//! payload_len : u32
//! payload     : QOI image bytes
//! ```
//!
//! Convert freedesktop / B00merang packs with `b00merang2fficons`.

use anyhow::{bail, Context, Result};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

pub const MAGIC: &[u8; 8] = b"FFICONS\0";
pub const VERSION: u16 = 1;
pub const FLAG_ZLIB: u16 = 1;

/// Decoded RGBA icon.
#[derive(Clone, Debug)]
pub struct IconRgba {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// In-memory icon pack keyed by purpose string.
#[derive(Clone, Debug, Default)]
pub struct IconPack {
    /// purpose → rgba (decoded from QOI at load time)
    icons: HashMap<String, IconRgba>,
}

impl IconPack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, purpose: impl Into<String>, icon: IconRgba) {
        self.icons.insert(purpose.into(), icon);
    }

    pub fn get(&self, purpose: &str) -> Option<&IconRgba> {
        self.icons.get(purpose)
    }

    /// Resolve with size preference: exact `@size`, then bare name, then closest `@N`.
    pub fn resolve(&self, base: &str, size: u32) -> Option<&IconRgba> {
        let exact = format!("{base}@{size}");
        if let Some(i) = self.icons.get(&exact) {
            return Some(i);
        }
        if let Some(i) = self.icons.get(base) {
            return Some(i);
        }
        // Closest sized variant of this base.
        let prefix = format!("{base}@");
        let mut best: Option<(&IconRgba, u32)> = None;
        for (key, icon) in &self.icons {
            if let Some(rest) = key.strip_prefix(&prefix) {
                if let Ok(sz) = rest.parse::<u32>() {
                    let dist = sz.abs_diff(size);
                    match best {
                        None => best = Some((icon, dist)),
                        Some((_, d)) if dist < d => best = Some((icon, dist)),
                        _ => {}
                    }
                }
            }
        }
        best.map(|(i, _)| i)
    }

    pub fn len(&self) -> usize {
        self.icons.len()
    }

    pub fn is_empty(&self) -> bool {
        self.icons.is_empty()
    }

    pub fn purposes(&self) -> impl Iterator<Item = &str> {
        self.icons.keys().map(|s| s.as_str())
    }

    /// Encode pack: zlib-compressed series of (purpose, len, qoi).
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut body = Vec::new();
        // Stable order for reproducible packs.
        let mut keys: Vec<_> = self.icons.keys().cloned().collect();
        keys.sort();
        for purpose in keys {
            let icon = &self.icons[&purpose];
            let qoi = encode_qoi(icon)?;
            write_str16(&mut body, &purpose)?;
            let plen = u32::try_from(qoi.len()).context("qoi too large")?;
            body.write_all(&plen.to_le_bytes())?;
            body.write_all(&qoi)?;
        }

        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&body)?;
        let compressed = enc.finish()?;

        let mut out = Vec::new();
        out.write_all(MAGIC)?;
        out.write_all(&VERSION.to_le_bytes())?;
        out.write_all(&FLAG_ZLIB.to_le_bytes())?;
        out.write_all(&(body.len() as u32).to_le_bytes())?;
        out.write_all(&compressed)?;
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 16 {
            bail!("fficons truncated header");
        }
        if &bytes[0..8] != MAGIC {
            bail!("not an fficons file (bad magic)");
        }
        let version = u16::from_le_bytes([bytes[8], bytes[9]]);
        if version == 0 || version > VERSION {
            bail!("unsupported fficons version {version}");
        }
        let flags = u16::from_le_bytes([bytes[10], bytes[11]]);
        let unc_len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
        let payload = &bytes[16..];

        let body = if flags & FLAG_ZLIB != 0 {
            let mut dec = ZlibDecoder::new(payload);
            let mut buf = Vec::with_capacity(unc_len);
            dec.read_to_end(&mut buf)
                .context("inflate fficons body")?;
            if buf.len() != unc_len {
                // Tolerate mismatch but warn via context if wildly off.
                if buf.len() < unc_len / 2 {
                    bail!(
                        "fficons inflate size mismatch: got {}, header said {unc_len}",
                        buf.len()
                    );
                }
            }
            buf
        } else {
            payload.to_vec()
        };

        let mut pack = IconPack::new();
        let mut cur = body.as_slice();
        while !cur.is_empty() {
            let purpose = read_str16(&mut cur)?;
            let plen = read_u32(&mut cur)? as usize;
            if cur.len() < plen {
                bail!("truncated qoi payload for {purpose}");
            }
            let qoi = &cur[..plen];
            cur = &cur[plen..];
            let icon = decode_qoi(qoi).with_context(|| format!("qoi decode {purpose}"))?;
            pack.insert(purpose, icon);
        }
        Ok(pack)
    }

    pub fn write_file(&self, path: &Path) -> Result<()> {
        let data = self.encode()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("fficons.tmp");
        fs::write(&tmp, &data)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn read_file(path: &Path) -> Result<Self> {
        let data = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        Self::decode(&data)
    }

    /// Load from path, or empty pack on missing/error.
    pub fn load_or_empty(path: &Path) -> Self {
        Self::read_file(path).unwrap_or_default()
    }
}

fn encode_qoi(icon: &IconRgba) -> Result<Vec<u8>> {
    use image::codecs::qoi::QoiEncoder;
    use image::{ExtendedColorType, ImageEncoder};
    let mut buf = Vec::new();
    QoiEncoder::new(&mut buf)
        .write_image(
            &icon.rgba,
            icon.width,
            icon.height,
            ExtendedColorType::Rgba8,
        )
        .context("qoi encode")?;
    Ok(buf)
}

/// Pick a freedesktop-style purpose from common file extensions.
pub fn purpose_for_file(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "qoi" | "ico" | "svg" | "tga" => {
            purpose::IMAGE
        }
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "wma" | "opus" => purpose::AUDIO,
        "mp4" | "mkv" | "avi" | "webm" | "mov" | "wmv" | "m4v" => purpose::VIDEO,
        "exe" | "msi" | "com" | "bat" | "cmd" | "sh" | "bin" | "AppImage" | "run" => purpose::EXEC,
        "zip" | "tar" | "gz" | "xz" | "bz2" | "7z" | "rar" | "deb" | "rpm" => {
            "mimetypes/package-x-generic"
        }
        _ => purpose::FILE,
    }
}

/// Bundled pack filename for a built-in theme (under `assets/icons/`).
///
/// B00merang-Artwork has XP / 7 / 10 themes — not Win95/98. Classic 9x themes
/// share the XP pack until a real 9x icon set is added.
pub fn pack_name_for_theme(theme: crate::theme::ThemeId) -> &'static str {
    use crate::theme::ThemeId;
    match theme {
        ThemeId::Win95 | ThemeId::Win98 | ThemeId::WinXpLuna => "winxp_luna.fficons",
        ThemeId::Win7Aero => "win7_aero.fficons",
        ThemeId::ModernLight | ThemeId::ModernDark => "modern.fficons",
    }
}

/// Resolve a pack file by walking common install / dev locations.
pub fn resolve_pack_file(file_name: &str) -> Option<std::path::PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(dir) = crate::settings::config_dir_public() {
        candidates.push(dir.join("icons").join(file_name));
        candidates.push(dir.join(file_name));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("assets").join("icons").join(file_name));
            candidates.push(dir.join("icons").join(file_name));
            candidates.push(dir.join(file_name));
            // cargo run: target/debug → ../../assets/icons
            candidates.push(dir.join("..").join("..").join("assets").join("icons").join(file_name));
        }
    }
    candidates.push(std::path::PathBuf::from("assets/icons").join(file_name));
    candidates.push(std::path::PathBuf::from(file_name));
    candidates.push(std::path::PathBuf::from("icons.fficons"));
    for p in candidates {
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Default pack path: config, then theme-agnostic `icons.fficons`.
pub fn default_pack_path() -> Option<std::path::PathBuf> {
    resolve_pack_file("icons.fficons")
        .or_else(|| resolve_pack_file("winxp_luna.fficons"))
}

impl IconPack {
    /// Load settings override, else theme-matched bundled pack, else empty.
    pub fn load_configured(path: &Option<std::path::PathBuf>) -> Self {
        Self::load_for_theme(crate::theme::ThemeId::Win98, path)
    }

    pub fn load_for_theme(
        theme: crate::theme::ThemeId,
        path: &Option<std::path::PathBuf>,
    ) -> Self {
        if let Some(p) = path {
            if !p.as_os_str().is_empty() {
                return Self::load_or_empty(p);
            }
        }
        let name = pack_name_for_theme(theme);
        if let Some(p) = resolve_pack_file(name) {
            return Self::load_or_empty(&p);
        }
        match default_pack_path() {
            Some(p) => Self::load_or_empty(&p),
            None => Self::new(),
        }
    }
}

fn decode_qoi(bytes: &[u8]) -> Result<IconRgba> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Qoi)
        .context("load qoi")?
        .to_rgba8();
    let (w, h) = img.dimensions();
    Ok(IconRgba {
        width: w,
        height: h,
        rgba: img.into_raw(),
    })
}

fn write_str16(out: &mut Vec<u8>, s: &str) -> Result<()> {
    let b = s.as_bytes();
    let len = u16::try_from(b.len()).context("purpose too long")?;
    out.write_all(&len.to_le_bytes())?;
    out.write_all(b)?;
    Ok(())
}

fn read_u32(r: &mut &[u8]) -> Result<u32> {
    if r.len() < 4 {
        bail!("truncated u32");
    }
    let v = u32::from_le_bytes([r[0], r[1], r[2], r[3]]);
    *r = &r[4..];
    Ok(v)
}

fn read_str16(r: &mut &[u8]) -> Result<String> {
    if r.len() < 2 {
        bail!("truncated purpose len");
    }
    let len = u16::from_le_bytes([r[0], r[1]]) as usize;
    *r = &r[2..];
    if r.len() < len {
        bail!("truncated purpose");
    }
    let s = String::from_utf8(r[..len].to_vec()).context("utf-8 purpose")?;
    *r = &r[len..];
    Ok(s)
}

/// Semantic aliases used by fuckFM / fuckWM (resolved via [`IconPack::resolve`]).
pub mod purpose {
    pub const FOLDER: &str = "places/folder";
    pub const FOLDER_OPEN: &str = "places/folder-open";
    pub const FILE: &str = "mimetypes/text-x-generic";
    pub const IMAGE: &str = "mimetypes/image-x-generic";
    pub const AUDIO: &str = "mimetypes/audio-x-generic";
    pub const VIDEO: &str = "mimetypes/video-x-generic";
    pub const EXEC: &str = "mimetypes/application-x-executable";
    pub const DRIVE: &str = "devices/drive-harddisk";
    pub const TRASH: &str = "places/user-trash";
    pub const TRASH_FULL: &str = "places/user-trash-full";
    pub const HOME: &str = "places/user-home";
    pub const DESKTOP: &str = "places/user-desktop";
    pub const COMPUTER: &str = "places/computer";
    pub const NETWORK: &str = "places/network-workgroup";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_zlib_qoi() {
        let mut pack = IconPack::new();
        // 2×2 red pixel
        pack.insert(
            "places/folder@2",
            IconRgba {
                width: 2,
                height: 2,
                rgba: vec![255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255],
            },
        );
        let bytes = pack.encode().unwrap();
        assert!(bytes.starts_with(MAGIC));
        let back = IconPack::decode(&bytes).unwrap();
        let icon = back.resolve("places/folder", 2).unwrap();
        assert_eq!(icon.width, 2);
        assert_eq!(icon.rgba[0], 255);
    }
}
