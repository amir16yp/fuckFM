use crate::browser::Location;
use crate::ops::{desktop_dir, home_dir};
use crate::platform::trash_label;
use crate::render::{Frame, Rect};
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub enum PlaceTarget {
    ThisPc,
    Trash,
    Path(PathBuf),
}

#[derive(Clone, Debug)]
pub struct Place {
    pub label: String,
    pub target: PlaceTarget,
    /// User bookmark (can be removed); false for built-in places.
    pub bookmark: bool,
}

pub struct SidePane {
    pub rect: Rect,
    pub places: Vec<Place>,
    pub selected: Option<usize>,
    pub hover: Option<usize>,
    pub visible: bool,
}

impl SidePane {
    pub fn new(rect: Rect) -> Self {
        Self {
            rect,
            places: default_places(&[], &BTreeMap::new()),
            selected: Some(0),
            hover: None,
            visible: true,
        }
    }

    pub fn rebuild(&mut self, bookmarks: &[PathBuf], labels: &BTreeMap<String, String>) {
        let selected_target = self
            .selected
            .and_then(|i| self.places.get(i).map(|p| p.target.clone()));
        self.places = default_places(bookmarks, labels);
        self.selected = selected_target.and_then(|t| {
            self.places.iter().position(|p| place_targets_eq(&p.target, &t))
        });
    }

    pub fn sync_selection(&mut self, location: &Location) {
        self.selected = self.places.iter().position(|p| match (&p.target, location) {
            (PlaceTarget::ThisPc, Location::ThisPc) => true,
            (PlaceTarget::Trash, Location::Trash) => true,
            (PlaceTarget::Path(a), Location::Path(b)) => same_path(a, b),
            _ => false,
        });
    }

    pub fn index_at(&self, y: i32, row_height: i32) -> Option<usize> {
        if !self.visible || !self.rect.contains(self.rect.x, y) || row_height <= 0 {
            return None;
        }
        let inner = self.rect.shrink(2);
        let rel = y - inner.y;
        if rel < 0 {
            return None;
        }
        let idx = (rel / row_height) as usize;
        if idx < self.places.len() {
            Some(idx)
        } else {
            None
        }
    }

    pub fn draw(&self, frame: &mut Frame<'_>) -> Result<()> {
        if !self.visible || self.rect.w <= 0 {
            return Ok(());
        }
        let theme = frame.theme;
        frame.sunken(self.rect, theme.sunken);
        let inner = self.rect.shrink(2);
        frame.fill(inner, theme.window);

        let row_h = theme.row_height + 2;
        for (index, place) in self.places.iter().enumerate() {
            let y = inner.y + index as i32 * row_h;
            if y + row_h > inner.y + inner.h {
                break;
            }
            let row = Rect::new(inner.x, y, inner.w, row_h);
            let selected = self.selected == Some(index);
            if selected {
                frame.fill(row, theme.selection);
            } else if self.hover == Some(index) {
                frame.fill(
                    row,
                    sdl2::pixels::Color::RGB(
                        ((theme.window.r as u16 + theme.selection.r as u16) / 2) as u8,
                        ((theme.window.g as u16 + theme.selection.g as u16) / 2) as u8,
                        ((theme.window.b as u16 + theme.selection.b as u16) / 2) as u8,
                    ),
                );
            }
            let color = if selected {
                theme.selection_text
            } else {
                theme.text
            };
            let label = if place.bookmark {
                format!("★ {}", place.label)
            } else {
                place.label.clone()
            };
            frame.text(
                &label,
                inner.x + 8,
                y + (row_h - theme.font_size as i32) / 2,
                color,
            )?;
        }
        Ok(())
    }
}

fn default_places(bookmarks: &[PathBuf], labels: &BTreeMap<String, String>) -> Vec<Place> {
    let mut places = vec![
        Place {
            label: "This PC".into(),
            target: PlaceTarget::ThisPc,
            bookmark: false,
        },
        Place {
            label: "Home".into(),
            target: PlaceTarget::Path(home_dir()),
            bookmark: false,
        },
        Place {
            label: "Desktop".into(),
            target: PlaceTarget::Path(desktop_dir()),
            bookmark: false,
        },
    ];
    for (label, path) in crate::platform::folders::extra_places() {
        if places.iter().any(|p| match &p.target {
            PlaceTarget::Path(existing) => same_path(existing, &path),
            _ => false,
        }) {
            continue;
        }
        places.push(Place {
            label,
            target: PlaceTarget::Path(path),
            bookmark: false,
        });
    }

    places.push(Place {
        label: trash_label().into(),
        target: PlaceTarget::Trash,
        bookmark: false,
    });
    #[cfg(unix)]
    places.push(Place {
        label: "Root".into(),
        target: PlaceTarget::Path(PathBuf::from("/")),
        bookmark: false,
    });

    for path in bookmarks {
        if !path.is_absolute() {
            continue;
        }
        // Skip duplicates of built-ins.
        if places.iter().any(|p| match &p.target {
            PlaceTarget::Path(existing) => same_path(existing, path),
            _ => false,
        }) {
            continue;
        }
        let key = path.display().to_string();
        let label = labels
            .get(&key)
            .cloned()
            .or_else(|| {
                path.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| path.display().to_string());
        places.push(Place {
            label,
            target: PlaceTarget::Path(path.clone()),
            bookmark: true,
        });
    }
    places
}

fn place_targets_eq(a: &PlaceTarget, b: &PlaceTarget) -> bool {
    match (a, b) {
        (PlaceTarget::ThisPc, PlaceTarget::ThisPc) => true,
        (PlaceTarget::Trash, PlaceTarget::Trash) => true,
        (PlaceTarget::Path(a), PlaceTarget::Path(b)) => same_path(a, b),
        _ => false,
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
