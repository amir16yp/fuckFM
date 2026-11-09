use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct ThumbRgba {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Small LRU-ish cache of downscaled image thumbs for Icons view.
pub struct ThumbCache {
    map: HashMap<PathBuf, Cached>,
    max_entries: usize,
}

struct Cached {
    mtime: Option<SystemTime>,
    size: u32,
    thumb: ThumbRgba,
}

impl ThumbCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            map: HashMap::new(),
            max_entries: max_entries.max(16),
        }
    }

    pub fn get_or_load(&mut self, path: &Path, target: u32) -> Option<&ThumbRgba> {
        if !is_image_path(path) {
            return None;
        }
        let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        let key = path.to_path_buf();

        let needs = match self.map.get(&key) {
            Some(c) if c.size == target && c.mtime == mtime => false,
            _ => true,
        };
        if needs {
            if let Some(thumb) = load_thumb(path, target) {
                if self.map.len() >= self.max_entries {
                    // Drop an arbitrary entry (HashMap iter order is fine for MVP).
                    if let Some(old) = self.map.keys().next().cloned() {
                        self.map.remove(&old);
                    }
                }
                self.map.insert(
                    key.clone(),
                    Cached {
                        mtime,
                        size: target,
                        thumb,
                    },
                );
            } else {
                return None;
            }
        }
        self.map.get(&key).map(|c| &c.thumb)
    }
}

pub fn is_image_path(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "qoi"
    )
}

fn load_thumb(path: &Path, target: u32) -> Option<ThumbRgba> {
    // Skip huge on-disk images in Icons view — full decode stalls scrolling.
    const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
    let len = std::fs::metadata(path).ok()?.len();
    if len > MAX_FILE_BYTES {
        return None;
    }
    let img = image::open(path).ok()?;
    let thumb = img.thumbnail(target.max(16), target.max(16));
    let rgba = thumb.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some(ThumbRgba {
        width: w,
        height: h,
        rgba: rgba.into_raw(),
    })
}

#[allow(dead_code)]
pub fn preload(_cache: &mut ThumbCache, _paths: &[PathBuf], _target: u32) -> Result<()> {
    Ok(())
}