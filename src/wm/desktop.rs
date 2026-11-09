//! Desktop surface — wallpaper icon layer, arrange/grid, context actions.

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
    /// Free-placement cell when auto-arrange is off (col, row).
    pub col: i32,
    pub row: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeskArrange {
    Auto,
    AlignGrid,
    Free,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeskAction {
    Refresh,
    OpenFm,
    NewFolder,
    ToggleAutoArrange,
    AlignToGrid,
    Personalize,
    ShowDesktop,
    Exit,
}

pub struct Desktop {
    pub icons: Vec<DeskIcon>,
    pub hover: Option<usize>,
    pub selected: Option<usize>,
    pub arrange: DeskArrange,
    pub peek: bool,
    pub area: Rect,
    pub cell_w: i32,
    pub cell_h: i32,
}

impl Desktop {
    pub fn load() -> Self {
        let mut desk = Self {
            icons: Vec::new(),
            hover: None,
            selected: None,
            arrange: DeskArrange::Auto,
            peek: false,
            area: Rect::default(),
            cell_w: 84,
            cell_h: 72,
        };
        desk.reload_icons();
        desk
    }

    pub fn reload_icons(&mut self) {
        let mut icons = Vec::new();
        if let Some(dir) = desktop_dir() {
            if let Ok(rd) = fs::read_dir(&dir) {
                let mut entries: Vec<_> = rd.flatten().collect();
                entries.sort_by_key(|e| e.file_name());
                for (i, ent) in entries.into_iter().take(64).enumerate() {
                    let path = ent.path();
                    let name = ent.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') {
                        continue;
                    }
                    let col = (i as i32) % 8;
                    let row = (i as i32) / 8;
                    icons.push(DeskIcon {
                        is_dir: path.is_dir(),
                        name,
                        path,
                        rect: Rect::default(),
                        col,
                        row,
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
                    col: 0,
                    row: 0,
                });
            }
        }
        self.icons = icons;
    }

    pub fn layout(&mut self, area: Rect, theme: &Theme) {
        self.area = area;
        self.cell_w = 84;
        self.cell_h = theme.row_height + 52;
        let cols = ((area.w - 16) / self.cell_w).max(1);

        match self.arrange {
            DeskArrange::Auto => {
                for (i, icon) in self.icons.iter_mut().enumerate() {
                    let col = (i as i32) % cols;
                    let row = (i as i32) / cols;
                    icon.col = col;
                    icon.row = row;
                    icon.rect = cell_rect(area, col, row, self.cell_w, self.cell_h);
                }
            }
            DeskArrange::AlignGrid | DeskArrange::Free => {
                for icon in self.icons.iter_mut() {
                    if self.arrange == DeskArrange::AlignGrid {
                        // Snap stored col/row into the visible grid.
                        icon.col = icon.col.clamp(0, cols - 1);
                        icon.row = icon.row.max(0);
                    }
                    icon.rect = cell_rect(area, icon.col, icon.row, self.cell_w, self.cell_h);
                }
            }
        }
    }

    pub fn align_to_grid(&mut self) {
        self.arrange = DeskArrange::AlignGrid;
        if self.area.w > 0 {
            // snap by current rect centers
            for icon in &mut self.icons {
                let cx = icon.rect.x + icon.rect.w / 2 - self.area.x - 12;
                let cy = icon.rect.y + icon.rect.h / 2 - self.area.y - 12;
                icon.col = (cx / self.cell_w.max(1)).max(0);
                icon.row = (cy / self.cell_h.max(1)).max(0);
            }
        }
    }

    pub fn toggle_auto_arrange(&mut self) {
        self.arrange = if self.arrange == DeskArrange::Auto {
            DeskArrange::Free
        } else {
            DeskArrange::Auto
        };
    }

    pub fn index_at(&self, x: i32, y: i32) -> Option<usize> {
        self.icons.iter().position(|ic| ic.rect.contains(x, y))
    }

    pub fn context_items(&self) -> Vec<(String, DeskAction)> {
        let auto = self.arrange == DeskArrange::Auto;
        vec![
            ("Refresh".into(), DeskAction::Refresh),
            ("Open fuckFM".into(), DeskAction::OpenFm),
            ("New Folder".into(), DeskAction::NewFolder),
            (
                if auto {
                    "Auto arrange icons ✓".into()
                } else {
                    "Auto arrange icons".into()
                },
                DeskAction::ToggleAutoArrange,
            ),
            ("Align icons to grid".into(), DeskAction::AlignToGrid),
            ("Personalize (cycle theme)".into(), DeskAction::Personalize),
            ("Show Desktop".into(), DeskAction::ShowDesktop),
            ("Exit".into(), DeskAction::Exit),
        ]
    }

    /// Build platform context items for the SDL shell menu helper.
    pub fn context_menu_labels(&self) -> Vec<String> {
        self.context_items().into_iter().map(|(l, _)| l).collect()
    }

    pub fn draw(&self, frame: &mut Frame<'_>, pack: &IconPack) -> Result<()> {
        let theme = frame.theme;
        if self.peek {
            // Peek: dim icons slightly by skipping draw of selection chrome only.
        }
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

    pub fn new_folder(&mut self) -> Result<PathBuf> {
        let dir = desktop_dir().ok_or_else(|| anyhow::anyhow!("Desktop folder missing"))?;
        let mut n = 1;
        loop {
            let name = if n == 1 {
                "New Folder".to_string()
            } else {
                format!("New Folder ({n})")
            };
            let path = dir.join(&name);
            if !path.exists() {
                fs::create_dir(&path)?;
                self.reload_icons();
                return Ok(path);
            }
            n += 1;
            if n > 99 {
                anyhow::bail!("too many New Folder copies");
            }
        }
    }
}

fn cell_rect(area: Rect, col: i32, row: i32, cell_w: i32, cell_h: i32) -> Rect {
    Rect::new(
        area.x + 12 + col * cell_w,
        area.y + 12 + row * cell_h,
        cell_w - 8,
        cell_h - 8,
    )
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

pub fn desk_action_from_index(desk: &Desktop, index: usize) -> Option<DeskAction> {
    desk.context_items().into_iter().nth(index).map(|(_, a)| a)
}
