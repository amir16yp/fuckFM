//! Desktop surface — wallpaper fill + icon layer from ~/Desktop.

use crate::iconpack::{purpose, purpose_for_file, IconPack};
use crate::render::{Frame, Rect};
use crate::theme::Theme;
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct DeskIcon {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub rect: Rect,
}

pub struct Desktop {
    pub icons: Vec<DeskIcon>,
    pub hover: Option<usize>,
    pub selected: Option<usize>,
}

impl Desktop {
    pub fn load() -> Self {
        let mut icons = Vec::new();
        if let Some(dir) = desktop_dir() {
            if let Ok(rd) = fs::read_dir(&dir) {
                let mut entries: Vec<_> = rd.flatten().collect();
                entries.sort_by_key(|e| e.file_name());
                for ent in entries.into_iter().take(48) {
                    let path = ent.path();
                    let name = ent.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') {
                        continue;
                    }
                    icons.push(DeskIcon {
                        is_dir: path.is_dir(),
                        name,
                        path,
                        rect: Rect::default(),
                    });
                }
            }
        }
        if icons.is_empty() {
            if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
            {
                icons.push(DeskIcon {
                    name: "Home".into(),
                    path: PathBuf::from(home),
                    is_dir: true,
                    rect: Rect::default(),
                });
            }
        }
        Self {
            icons,
            hover: None,
            selected: None,
        }
    }

    pub fn layout(&mut self, area: Rect, theme: &Theme) {
        let cell_w = 84;
        let cell_h = theme.row_height + 52;
        let cols = ((area.w - 16) / cell_w).max(1);
        for (i, icon) in self.icons.iter_mut().enumerate() {
            let col = (i as i32) % cols;
            let row = (i as i32) / cols;
            icon.rect = Rect::new(
                area.x + 12 + col * cell_w,
                area.y + 12 + row * cell_h,
                cell_w - 8,
                cell_h - 8,
            );
        }
    }

    pub fn index_at(&self, x: i32, y: i32) -> Option<usize> {
        self.icons.iter().position(|ic| ic.rect.contains(x, y))
    }

    pub fn draw(&self, frame: &mut Frame<'_>, pack: &IconPack) -> Result<()> {
        let theme = frame.theme;
        for (i, icon) in self.icons.iter().enumerate() {
            let selected = self.selected == Some(i);
            let hover = self.hover == Some(i);
            if selected {
                frame.fill(icon.rect, theme.selection);
            } else if hover {
                frame.flat_border(icon.rect, theme.selection);
            }
            let ix = icon.rect.x + (icon.rect.w - 32) / 2;
            let iy = icon.rect.y + 4;
            draw_desk_glyph(frame, ix, iy, 32, icon.is_dir, &icon.path, pack);
            let label = truncate(frame, &icon.name, icon.rect.w - 6);
            let tw = frame.text_width(&label);
            let color = if selected {
                theme.selection_text
            } else {
                theme.text
            };
            if !selected {
                frame.text(
                    &label,
                    icon.rect.x + (icon.rect.w - tw) / 2 + 1,
                    iy + 36 + 1,
                    theme.chrome.dark_shadow,
                )?;
            }
            frame.text(
                &label,
                icon.rect.x + (icon.rect.w - tw) / 2,
                iy + 36,
                color,
            )?;
        }
        Ok(())
    }
}

fn desktop_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_DESKTOP_DIR") {
        let p = PathBuf::from(xdg);
        if p.is_dir() {
            return Some(p);
        }
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let desk = PathBuf::from(home).join("Desktop");
    if desk.is_dir() {
        Some(desk)
    } else {
        None
    }
}

fn draw_desk_glyph(
    frame: &mut Frame<'_>,
    x: i32,
    y: i32,
    size: i32,
    is_dir: bool,
    path: &Path,
    pack: &IconPack,
) {
    let purpose = if is_dir {
        purpose::FOLDER
    } else {
        purpose_for_file(path)
    };
    if let Some(icon) = pack.resolve(purpose, size as u32) {
        let scale = (size as f32 / icon.width.max(1) as f32)
            .min(size as f32 / icon.height.max(1) as f32);
        let dw = (icon.width as f32 * scale).max(1.0) as u32;
        let dh = (icon.height as f32 * scale).max(1.0) as u32;
        let dx = x + (size - dw as i32) / 2;
        let dy = y + (size - dh as i32) / 2;
        let _ = frame.fonts.blit_rgba(
            frame.canvas,
            dx,
            dy,
            icon.width,
            icon.height,
            dw,
            dh,
            &icon.rgba,
        );
        return;
    }
    let theme = frame.theme;
    if is_dir {
        frame.fill(
            Rect::new(x + 2, y + 8, size - 4, size - 12),
            theme.icon_folder,
        );
        frame.fill(Rect::new(x + 2, y + 4, size / 2, 8), theme.icon_folder);
        frame.flat_border(Rect::new(x + 2, y + 4, size - 4, size - 8), theme.border);
    } else {
        frame.fill(Rect::new(x + 6, y + 2, size - 12, size - 4), theme.icon_file);
        frame.flat_border(Rect::new(x + 6, y + 2, size - 12, size - 4), theme.border);
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

pub fn open_desk_item(path: &Path) -> Result<()> {
    if path.is_dir() {
        let exe = std::env::current_exe().ok();
        if let Some(exe) = exe {
            let fm = exe.with_file_name(if cfg!(windows) {
                "fuckfm.exe"
            } else {
                "fuckfm"
            });
            if fm.is_file() {
                std::process::Command::new(fm)
                    .arg(path)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()?;
                return Ok(());
            }
        }
    }
    crate::platform::open_default(path)
}
