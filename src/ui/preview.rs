use crate::platform::human_bytes;
use crate::render::{Frame, Rect};
use anyhow::Result;
use image::GenericImageView;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Text sample size (not the whole file).
const MAX_TEXT_PREVIEW_BYTES: u64 = 8 * 1024;
const MAX_TEXT_LINES: usize = 120;
const HEX_BYTES_PER_ROW: usize = 16;
/// Hex is only a header peek — first N bytes / rows, never a deep dump.
const MAX_HEX_PREVIEW_BYTES: u64 = 256;
const MAX_HEX_ROWS: usize = 16;
/// Longest image edge stored in the preview pane (never keep full-res RGBA).
const MAX_IMAGE_EDGE: u32 = 640;
/// Skip decoding images larger than this on disk.
const MAX_IMAGE_FILE_BYTES: u64 = 24 * 1024 * 1024;

#[derive(Debug, Default)]
enum PreviewBody {
    #[default]
    Empty,
    Text(String),
    Hex(String),
    Image {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        /// Original pixel size before downscale (for the title line).
        source_w: u32,
        source_h: u32,
    },
    Message(String),
}

/// Wrapped lines for the current content width — rebuilt only when body/width change.
#[derive(Default)]
struct TextLayout {
    content_w: i32,
    line_h: i32,
    lines: Vec<String>,
}

pub struct PreviewPane {
    pub rect: Rect,
    pub visible: bool,
    path: Option<PathBuf>,
    body: PreviewBody,
    text_layout: Option<TextLayout>,
}

impl PreviewPane {
    pub fn new() -> Self {
        Self {
            rect: Rect::default(),
            visible: false,
            path: None,
            body: PreviewBody::Empty,
            text_layout: None,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            self.clear();
        }
    }

    pub fn clear(&mut self) {
        self.path = None;
        self.body = PreviewBody::Empty;
        self.text_layout = None;
    }

    pub fn show_path(&mut self, path: Option<&Path>) {
        if !self.visible {
            return;
        }
        let Some(path) = path else {
            self.clear();
            return;
        };
        if self.path.as_deref() == Some(path) {
            return;
        }
        self.path = Some(path.to_path_buf());
        self.body = load_preview(path);
        self.text_layout = None;
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>) -> Result<()> {
        if !self.visible {
            return Ok(());
        }
        let theme = frame.theme;
        frame.sunken(self.rect, theme.sunken);
        let inner = self.rect.shrink(2);
        frame.fill(inner, theme.client);

        let mut title = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Preview".into());
        if let PreviewBody::Image {
            source_w,
            source_h,
            ..
        } = &self.body
        {
            title = format!("{title}  ({source_w}×{source_h})");
        }
        frame.text(&title, inner.x + 6, inner.y + 4, theme.text)?;

        let content = Rect::new(
            inner.x + 4,
            inner.y + theme.row_height + 4,
            inner.w - 8,
            inner.h - theme.row_height - 8,
        );

        match &self.body {
            PreviewBody::Empty => {
                frame.text(
                    "Select a file",
                    content.x + 4,
                    content.y + 4,
                    theme.text_disabled,
                )?;
            }
            PreviewBody::Message(_) | PreviewBody::Text(_) | PreviewBody::Hex(_) => {
                let color = match &self.body {
                    PreviewBody::Message(_) => theme.text_disabled,
                    _ => theme.text,
                };
                let line_h = theme.font_size as i32 + 2;
                let needs = match &self.text_layout {
                    Some(l) => l.content_w != content.w || l.line_h != line_h,
                    None => true,
                };
                if needs {
                    let src = match &self.body {
                        PreviewBody::Message(s) | PreviewBody::Text(s) | PreviewBody::Hex(s) => {
                            s.as_str()
                        }
                        _ => "",
                    };
                    let lines = wrap_text(frame, src, content.w, content.h, line_h);
                    self.text_layout = Some(TextLayout {
                        content_w: content.w,
                        line_h,
                        lines,
                    });
                }
                if let Some(layout) = &self.text_layout {
                    let mut y = content.y;
                    for line in &layout.lines {
                        if y + line_h > content.y + content.h {
                            break;
                        }
                        if !line.is_empty() {
                            frame.text(line, content.x + 2, y, color)?;
                        }
                        y += line_h;
                    }
                }
            }
            PreviewBody::Image {
                width,
                height,
                rgba,
                ..
            } => {
                blit_fit(frame, content, *width, *height, rgba)?;
            }
        }
        Ok(())
    }
}

fn load_preview(path: &Path) -> PreviewBody {
    if path.is_dir() {
        return PreviewBody::Message("Folder".into());
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    if is_image_ext(&ext) {
        return load_image_preview(path, size);
    }

    // Known binary media: still only a tiny hex header peek (not a full dump).
    if media_kind(&ext).is_some() {
        return match read_prefix(path, MAX_HEX_PREVIEW_BYTES) {
            Ok(buf) => {
                let mut text =
                    format!("{}  —  {}\n\n", media_kind(&ext).unwrap(), human_bytes(size));
                text.push_str(&format_hex_dump(&buf, size > buf.len() as u64));
                PreviewBody::Hex(text)
            }
            Err(err) => PreviewBody::Message(format!("Read error: {err}")),
        };
    }

    match read_prefix(path, MAX_TEXT_PREVIEW_BYTES) {
        Ok(buf) => {
            let truncated = size > buf.len() as u64;
            if looks_like_text(&buf) {
                PreviewBody::Text(format_text_preview(&buf, truncated))
            } else {
                // Unknown binary: shrink to the hex peek size before formatting.
                let hex_buf = &buf[..buf.len().min(MAX_HEX_PREVIEW_BYTES as usize)];
                PreviewBody::Hex(format_hex_dump(
                    hex_buf,
                    size > hex_buf.len() as u64,
                ))
            }
        }
        Err(err) => PreviewBody::Message(format!("Read error: {err}")),
    }
}

fn load_image_preview(path: &Path, size: u64) -> PreviewBody {
    if size > MAX_IMAGE_FILE_BYTES {
        return PreviewBody::Message(format!(
            "Image too large to preview ({})\nOpen the file to view it.",
            human_bytes(size)
        ));
    }

    match image::open(path) {
        Ok(img) => {
            let (source_w, source_h) = img.dimensions();
            // Always downscale before converting to RGBA — full-res RGBA is the stall.
            let thumb = img.thumbnail(MAX_IMAGE_EDGE, MAX_IMAGE_EDGE);
            let rgba = thumb.to_rgba8();
            let (w, h) = rgba.dimensions();
            PreviewBody::Image {
                width: w,
                height: h,
                rgba: rgba.into_raw(),
                source_w,
                source_h,
            }
        }
        Err(err) => PreviewBody::Message(format!("Image error: {err}")),
    }
}

fn is_image_ext(ext: &str) -> bool {
    matches!(
        ext,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "qoi" | "ico" | "tga"
    )
}

fn media_kind(ext: &str) -> Option<&'static str> {
    match ext {
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "wmv" | "m4v" | "mpeg" | "mpg" | "flv" | "ts"
        | "m2ts" => Some("Video file"),
        "mp3" | "flac" | "ogg" | "wav" | "m4a" | "aac" | "opus" | "wma" | "aiff" => {
            Some("Audio file")
        }
        "zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz" | "zst" | "tgz" | "iso" | "dmg" => {
            Some("Archive")
        }
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" => {
            Some("Document")
        }
        "exe" | "dll" | "so" | "dylib" | "bin" | "dat" | "db" | "sqlite" => Some("Binary file"),
        _ => None,
    }
}

fn read_prefix(path: &Path, max_bytes: u64) -> Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
    let mut buf = Vec::new();
    file.by_ref().take(max_bytes).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Prefer text when the sample is mostly printable UTF-8 without NULs.
fn looks_like_text(buf: &[u8]) -> bool {
    if buf.is_empty() {
        return true;
    }
    if buf.contains(&0) {
        return false;
    }
    match std::str::from_utf8(buf) {
        Ok(s) => {
            let mut weird = 0usize;
            let mut total = 0usize;
            for ch in s.chars() {
                total += 1;
                if ch == '\n' || ch == '\r' || ch == '\t' {
                    continue;
                }
                if ch.is_control() {
                    weird += 1;
                }
            }
            if total == 0 {
                return true;
            }
            (weird as f32 / total as f32) < 0.02
        }
        Err(_) => {
            let mut weird = 0usize;
            for &b in buf {
                let printable = (0x20..0x7F).contains(&b) || b == b'\n' || b == b'\r' || b == b'\t';
                if !printable {
                    weird += 1;
                }
            }
            (weird as f32 / buf.len() as f32) < 0.10
        }
    }
}

fn format_text_preview(buf: &[u8], truncated: bool) -> String {
    let text = String::from_utf8_lossy(buf);
    let mut out = String::new();
    let mut lines = 0usize;
    let mut hit_line_cap = false;
    for line in text.lines() {
        if lines >= MAX_TEXT_LINES {
            hit_line_cap = true;
            break;
        }
        if lines > 0 {
            out.push('\n');
        }
        out.push_str(line);
        lines += 1;
    }
    if truncated || hit_line_cap {
        out.push_str("\n…");
    }
    out
}

fn format_hex_dump(buf: &[u8], truncated: bool) -> String {
    let mut out = String::new();
    let rows = buf.len().div_ceil(HEX_BYTES_PER_ROW);
    let show_rows = rows.min(MAX_HEX_ROWS);
    for row in 0..show_rows {
        let start = row * HEX_BYTES_PER_ROW;
        let end = (start + HEX_BYTES_PER_ROW).min(buf.len());
        let chunk = &buf[start..end];

        out.push_str(&format!("{start:08X}  "));

        for i in 0..HEX_BYTES_PER_ROW {
            if i == 8 {
                out.push(' ');
            }
            if i < chunk.len() {
                out.push_str(&format!("{:02X} ", chunk[i]));
            } else {
                out.push_str("   ");
            }
        }

        out.push_str(" |");
        for &b in chunk {
            let c = if (0x20..0x7F).contains(&b) {
                b as char
            } else {
                '.'
            };
            out.push(c);
        }
        out.push('|');
        out.push('\n');
    }

    if show_rows < rows || truncated {
        out.push_str("…");
    }
    out
}

/// Wrap once; result is cached by the pane until body/width change.
fn wrap_text(
    frame: &mut Frame<'_>,
    text: &str,
    content_w: i32,
    content_h: i32,
    line_h: i32,
) -> Vec<String> {
    let max_w = (content_w - 4).max(1);
    let max_lines = ((content_h / line_h).max(1)) as usize;
    let mut lines = Vec::new();

    for line in text.lines() {
        if lines.len() >= max_lines {
            break;
        }
        if line.is_empty() {
            lines.push(String::new());
            continue;
        }
        if frame.text_width(line) <= max_w {
            lines.push(line.to_string());
            continue;
        }
        let mut remaining = line;
        while !remaining.is_empty() && lines.len() < max_lines {
            let mut take = remaining.len();
            while take > 0 && frame.text_width(&remaining[..take]) > max_w {
                take = take.saturating_sub(1);
                while take > 0 && !remaining.is_char_boundary(take) {
                    take -= 1;
                }
            }
            if take == 0 {
                break;
            }
            lines.push(remaining[..take].to_string());
            remaining = &remaining[take..];
        }
    }
    lines
}

fn blit_fit(
    frame: &mut Frame<'_>,
    rect: Rect,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<()> {
    if width == 0 || height == 0 || rect.w <= 0 || rect.h <= 0 {
        return Ok(());
    }
    let scale = (rect.w as f32 / width as f32).min(rect.h as f32 / height as f32);
    let dw = (width as f32 * scale).max(1.0) as u32;
    let dh = (height as f32 * scale).max(1.0) as u32;
    let dx = rect.x + (rect.w - dw as i32) / 2;
    let dy = rect.y + (rect.h - dh as i32) / 2;
    frame
        .fonts
        .blit_rgba(frame.canvas, dx, dy, width, height, dw, dh, rgba)
}
