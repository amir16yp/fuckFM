//! OS desktop wallpaper query.
//!
//! - Windows: `HKCU\Control Panel\Desktop` (+ `Control Panel\Colors\Background`)
//! - Linux: GNOME/MATE `gsettings` picture-uri / picture-options (best-effort)

use std::path::PathBuf;

/// How the image is placed on the desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallpaperStyle {
    Center,
    Tile,
    Stretch,
    Fit,
    Fill,
    /// Multi-monitor span — treated as Fill on a single surface.
    Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemWallpaper {
    /// Image path when set; `None` means solid desktop color only.
    pub path: Option<PathBuf>,
    pub style: WallpaperStyle,
    /// Desktop background / letterbox color.
    pub background: (u8, u8, u8),
}

/// Read the current user desktop wallpaper + placement from the OS.
pub fn desktop_wallpaper() -> Option<SystemWallpaper> {
    #[cfg(windows)]
    {
        windows::read()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        unix::read()
    }
    #[cfg(target_os = "macos")]
    {
        None
    }
}

#[cfg(windows)]
mod windows {
    use super::{SystemWallpaper, WallpaperStyle};
    use std::path::PathBuf;
    use winreg::enums::*;
    use winreg::RegKey;

    pub fn read() -> Option<SystemWallpaper> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let desk = hkcu.open_subkey(r"Control Panel\Desktop").ok()?;

        let wallpaper_raw: String = desk.get_value("Wallpaper").unwrap_or_default();
        let style_raw: String = desk
            .get_value("WallpaperStyle")
            .unwrap_or_else(|_| "0".into());
        let tile_raw: String = desk
            .get_value("TileWallpaper")
            .unwrap_or_else(|_| "0".into());

        let style = parse_style(&style_raw, &tile_raw);
        let background = read_background_color().unwrap_or((0, 0, 0));
        let path = resolve_wallpaper_path(&wallpaper_raw);

        Some(SystemWallpaper {
            path,
            style,
            background,
        })
    }

    /// | WallpaperStyle | TileWallpaper | Mode    |
    /// |----------------|---------------|---------|
    /// | 0              | 1             | Tile    |
    /// | 0              | 0             | Center  |
    /// | 2              | *             | Stretch |
    /// | 6              | *             | Fit     |
    /// | 10             | *             | Fill    |
    /// | 22             | *             | Span    |
    fn parse_style(style: &str, tile: &str) -> WallpaperStyle {
        let style = style.trim();
        let tile = tile.trim() == "1";
        match style {
            "2" => WallpaperStyle::Stretch,
            "6" => WallpaperStyle::Fit,
            "10" => WallpaperStyle::Fill,
            "22" => WallpaperStyle::Span,
            _ if tile => WallpaperStyle::Tile,
            _ => WallpaperStyle::Center,
        }
    }

    fn read_background_color() -> Option<(u8, u8, u8)> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let colors = hkcu.open_subkey(r"Control Panel\Colors").ok()?;
        let raw: String = colors.get_value("Background").ok()?;
        parse_rgb(&raw)
    }

    fn parse_rgb(s: &str) -> Option<(u8, u8, u8)> {
        let mut parts = s.split_whitespace();
        let r: u8 = parts.next()?.parse().ok()?;
        let g: u8 = parts.next()?.parse().ok()?;
        let b: u8 = parts.next()?.parse().ok()?;
        Some((r, g, b))
    }

    fn resolve_wallpaper_path(raw: &str) -> Option<PathBuf> {
        let trimmed = raw.trim().trim_matches('"');
        if !trimmed.is_empty() {
            let p = PathBuf::from(expand_env(trimmed));
            if p.is_file() {
                return Some(p);
            }
        }
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let transcoded =
                PathBuf::from(appdata).join(r"Microsoft\Windows\Themes\TranscodedWallpaper");
            if transcoded.is_file() {
                return Some(transcoded);
            }
        }
        None
    }

    fn expand_env(s: &str) -> String {
        let mut out = String::new();
        let mut rest = s;
        while let Some(start) = rest.find('%') {
            out.push_str(&rest[..start]);
            rest = &rest[start + 1..];
            if let Some(end) = rest.find('%') {
                let key = &rest[..end];
                if let Ok(val) = std::env::var(key) {
                    out.push_str(&val);
                } else {
                    out.push('%');
                    out.push_str(key);
                    out.push('%');
                }
                rest = &rest[end + 1..];
            } else {
                out.push('%');
                out.push_str(rest);
                return out;
            }
        }
        out.push_str(rest);
        out
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod unix {
    use super::{SystemWallpaper, WallpaperStyle};
    use std::path::PathBuf;
    use std::process::Command;

    pub fn read() -> Option<SystemWallpaper> {
        read_gsettings("org.gnome.desktop.background")
            .or_else(|| read_gsettings("org.mate.background"))
    }

    fn read_gsettings(schema: &str) -> Option<SystemWallpaper> {
        let uri = gsettings_get(schema, "picture-uri")?;
        let options = gsettings_get(schema, "picture-options").unwrap_or_else(|| "zoom".into());
        let primary = gsettings_get(schema, "primary-color").unwrap_or_else(|| "#000000".into());
        let path = uri_to_path(&uri);
        // Solid-only desktops still count as a system wallpaper (letterbox color).
        if path.is_none() && options.trim() == "none" {
            return Some(SystemWallpaper {
                path: None,
                style: WallpaperStyle::Center,
                background: parse_css_color(&primary).unwrap_or((0, 0, 0)),
            });
        }
        path.as_ref()?;
        Some(SystemWallpaper {
            path,
            style: parse_gnome_options(&options),
            background: parse_css_color(&primary).unwrap_or((0, 0, 0)),
        })
    }

    fn gsettings_get(schema: &str, key: &str) -> Option<String> {
        let out = Command::new("gsettings")
            .args(["get", schema, key])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if raw.is_empty() || raw == "''" || raw == "\"\"" {
            return None;
        }
        // gsettings quotes strings as 'value' or "value".
        Some(raw.trim_matches(|c| c == '\'' || c == '"').to_string())
    }

    fn uri_to_path(uri: &str) -> Option<PathBuf> {
        let uri = uri.trim();
        if uri.is_empty() || uri == "none" {
            return None;
        }
        let path = if let Some(rest) = uri.strip_prefix("file://") {
            // Percent-decode lightly for spaces.
            PathBuf::from(percent_decode(rest))
        } else if uri.starts_with('/') {
            PathBuf::from(uri)
        } else {
            return None;
        };
        path.is_file().then_some(path)
    }

    fn percent_decode(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                    if let Ok(v) = u8::from_str_radix(hex, 16) {
                        out.push(v as char);
                        i += 3;
                        continue;
                    }
                }
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        out
    }

    /// GNOME picture-options → placement.
    fn parse_gnome_options(opts: &str) -> WallpaperStyle {
        match opts.trim() {
            "wallpaper" => WallpaperStyle::Tile,
            "centered" => WallpaperStyle::Center,
            "scaled" => WallpaperStyle::Fit,
            "stretched" => WallpaperStyle::Stretch,
            "zoom" => WallpaperStyle::Fill,
            "spanned" => WallpaperStyle::Span,
            _ => WallpaperStyle::Fill,
        }
    }

    fn parse_css_color(s: &str) -> Option<(u8, u8, u8)> {
        let s = s.trim();
        if let Some(hex) = s.strip_prefix('#') {
            if hex.len() == 6 {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                return Some((r, g, b));
            }
        }
        // rgb(r,g,b) from some gsettings themes
        if let Some(inner) = s.strip_prefix("rgb(").and_then(|t| t.strip_suffix(')')) {
            let mut parts = inner.split(',');
            let r: u8 = parts.next()?.trim().parse().ok()?;
            let g: u8 = parts.next()?.trim().parse().ok()?;
            let b: u8 = parts.next()?.trim().parse().ok()?;
            return Some((r, g, b));
        }
        None
    }
}
