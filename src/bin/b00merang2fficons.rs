//! Convert a B00merang / freedesktop icon theme directory into `.fficons`.
//!
//! Usage:
//! ```text
//! b00merang2fficons <theme-dir> [-o out.fficons] [--all] [--sizes 16,24,32,48]
//! ```
//!
//! Theme dir is a clone of e.g. https://github.com/B00merang-Artwork/Windows-XP
//! (`index.theme` + `48x48/places/folder.png` …).

use anyhow::{bail, Context, Result};
use fuckfm::iconpack::{IconPack, IconRgba};
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    if let Err(err) = run() {
        eprintln!("b00merang2fficons: {err:?}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!(
            "Usage: b00merang2fficons <theme-dir> [-o out.fficons] [--all] [--sizes 16,24,32,48]\n\
             \n\
             Converts B00merang-Artwork / freedesktop icon themes to fuckFM .fficons\n\
             (zlib body of purpose + length + QOI)."
        );
        std::process::exit(if args.is_empty() { 1 } else { 0 });
    }

    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut all = false;
    let mut sizes: Vec<u32> = vec![16, 24, 32, 48];

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                output = Some(PathBuf::from(
                    args.get(i).context("-o needs a path")?,
                ));
            }
            "--all" => all = true,
            "--sizes" => {
                i += 1;
                sizes = args
                    .get(i)
                    .context("--sizes needs a list")?
                    .split(',')
                    .filter_map(|s| s.trim().parse().ok())
                    .collect();
                if sizes.is_empty() {
                    bail!("--sizes produced no sizes");
                }
            }
            s if s.starts_with('-') => bail!("unknown flag {s}"),
            s => {
                if input.is_some() {
                    bail!("unexpected argument {s}");
                }
                input = Some(PathBuf::from(s));
            }
        }
        i += 1;
    }

    let input = input.context("missing <theme-dir>")?;
    if !input.is_dir() {
        bail!("{} is not a directory", input.display());
    }
    let out = output.unwrap_or_else(|| {
        let name = input
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("icons");
        PathBuf::from(format!("{name}.fficons"))
    });

    let pack = if all {
        convert_all(&input, &sizes)?
    } else {
        convert_essentials(&input, &sizes)?
    };

    pack.write_file(&out)?;
    println!(
        "Wrote {} icons → {} ({} bytes)",
        pack.len(),
        out.display(),
        fs::metadata(&out)?.len()
    );
    Ok(())
}

/// Core icons fuckFM / fuckWM need, mapped from freedesktop names (first hit wins).
fn essentials() -> &'static [(&'static str, &'static [&'static str])] {
    &[
        (
            "places/folder",
            &[
                "places/folder",
                "places/inode-directory",
                "places/gtk-directory",
                "places/gnome-fs-directory",
            ],
        ),
        (
            "places/folder-open",
            &["places/folder-open", "places/folder_open", "places/folder-drag-accept"],
        ),
        (
            "places/user-home",
            &[
                "places/user-home",
                "places/folder-home",
                "places/folder_home",
                "places/go-home",
                "places/gnome-fs-home",
            ],
        ),
        (
            "places/user-desktop",
            &[
                "places/user-desktop",
                "places/desktop",
                "places/folder-desktop",
                "places/gnome-fs-desktop",
            ],
        ),
        (
            "places/user-trash",
            &[
                "places/user-trash",
                "places/trashcan_empty",
                "places/emptytrash",
                "places/gnome-fs-trash-empty",
                "status/user-trash",
            ],
        ),
        (
            "places/user-trash-full",
            &[
                "places/user-trash-full",
                "places/trashcan_full",
                "places/gnome-fs-trash-full",
                "status/user-trash-full",
            ],
        ),
        (
            "places/computer",
            &[
                "places/computer",
                "devices/computer",
                "places/gnome-fs-client",
                "places/user-computer",
            ],
        ),
        (
            "places/network-workgroup",
            &[
                "places/network-workgroup",
                "places/gnome-fs-network",
                "places/network",
                "places/gtk-network",
            ],
        ),
        (
            "devices/drive-harddisk",
            &[
                "devices/drive-harddisk",
                "devices/harddrive",
                "devices/drive-harddisk-system",
                "devices/gnome-dev-harddisk",
            ],
        ),
        (
            "mimetypes/text-x-generic",
            &[
                "mimetypes/text-x-generic",
                "mimetypes/txt",
                "mimetypes/gnome-mime-text",
                "mimetypes/text-plain",
            ],
        ),
        (
            "mimetypes/image-x-generic",
            &[
                "mimetypes/image-x-generic",
                "mimetypes/image",
                "mimetypes/gnome-mime-image",
            ],
        ),
        (
            "mimetypes/audio-x-generic",
            &["mimetypes/audio-x-generic", "mimetypes/audio", "mimetypes/audio-x-mpeg"],
        ),
        (
            "mimetypes/video-x-generic",
            &["mimetypes/video-x-generic", "mimetypes/video", "mimetypes/video-x-generic-template"],
        ),
        (
            "mimetypes/application-x-executable",
            &[
                "mimetypes/application-x-executable",
                "mimetypes/application-x-ms-dos-executable",
                "mimetypes/exec",
                "mimetypes/binary",
            ],
        ),
        (
            "mimetypes/package-x-generic",
            &["mimetypes/package-x-generic", "mimetypes/folder_tar", "mimetypes/application-x-archive"],
        ),
        (
            "apps/system-file-manager",
            &[
                "apps/system-file-manager",
                "apps/file-manager",
                "places/folder",
            ],
        ),
        (
            "apps/utilities-terminal",
            &[
                "apps/utilities-terminal",
                "apps/terminal",
                "apps/gnome-terminal",
            ],
        ),
    ]
}

fn convert_essentials(theme_dir: &Path, sizes: &[u32]) -> Result<IconPack> {
    let index = scan_theme(theme_dir)?;
    let mut pack = IconPack::new();
    let mut missing = Vec::new();

    for (purpose, candidates) in essentials() {
        let mut found_any = false;
        for &size in sizes {
            if let Some((path, _)) = find_icon(&index, candidates, size) {
                let icon = load_as_rgba(&path)?;
                pack.insert(format!("{purpose}@{size}"), icon);
                found_any = true;
            }
        }
        // Also store unsized alias from preferred size.
        if let Some((path, sz)) = find_best(&index, candidates, sizes) {
            let icon = load_as_rgba(&path)?;
            if pack.get(purpose).is_none() {
                pack.insert(purpose.to_string(), icon.clone());
            }
            // Ensure at least one sized entry.
            let key = format!("{purpose}@{sz}");
            if pack.get(&key).is_none() {
                pack.insert(key, icon);
            }
            found_any = true;
        }
        if !found_any {
            missing.push(purpose.to_string());
        }
    }

    if !missing.is_empty() {
        eprintln!("warning: missing essentials: {}", missing.join(", "));
    }
    if pack.is_empty() {
        bail!("no icons converted — is this a freedesktop/B00merang theme?");
    }
    Ok(pack)
}

fn convert_all(theme_dir: &Path, sizes: &[u32]) -> Result<IconPack> {
    let index = scan_theme(theme_dir)?;
    let size_set: HashSet<u32> = sizes.iter().copied().collect();
    let mut pack = IconPack::new();

    for ((context, name), by_size) in &index {
        for (&size, path) in by_size {
            if !size_set.is_empty() && !size_set.contains(&size) {
                continue;
            }
            let purpose = format!("{context}/{name}@{size}");
            match load_as_rgba(path) {
                Ok(icon) => {
                    pack.insert(purpose, icon);
                }
                Err(err) => eprintln!("skip {}: {err}", path.display()),
            }
        }
    }

    if pack.is_empty() {
        bail!("no icons converted");
    }
    Ok(pack)
}

/// context/name → size → path
type ThemeIndex = BTreeMap<(String, String), BTreeMap<u32, PathBuf>>;

fn scan_theme(theme_dir: &Path) -> Result<ThemeIndex> {
    let mut index: ThemeIndex = BTreeMap::new();
    let rd = fs::read_dir(theme_dir).with_context(|| format!("read {}", theme_dir.display()))?;
    for ent in rd.flatten() {
        let top = ent.path();
        if !top.is_dir() {
            continue;
        }
        let top_name = ent.file_name().to_string_lossy().into_owned();
        if top_name == "scalable" || top_name == "symbolic" || top_name.starts_with('.') {
            continue;
        }

        // Layout A: `{size}/{context}/{name}.png` (XP / Win8 / Win10).
        if let Some(size) = parse_size(&top_name) {
            scan_size_context(&top, size, &mut index);
            continue;
        }

        // Layout B: `{context}/{size}/{name}.png` + optional `{context}/{name}.png` (Win7).
        let context = normalize_context(&top_name);
        let ctx_rd = match fs::read_dir(&top) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for child in ctx_rd.flatten() {
            let path = child.path();
            let name = child.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if let Some(size) = parse_size(&name) {
                    scan_flat_icons(&path, &context, size, &mut index);
                }
            } else if path.is_file() {
                if let Some(icon_name) = strip_icon_ext(&name) {
                    let size = image_size_hint(&path).unwrap_or(48);
                    index
                        .entry((context.clone(), icon_name.to_string()))
                        .or_default()
                        .entry(size)
                        .or_insert(path);
                }
            }
        }
    }
    if index.is_empty() {
        bail!(
            "no size/context/icon.png tree under {} (expected B00merang layout)",
            theme_dir.display()
        );
    }
    Ok(index)
}

fn scan_size_context(size_dir: &Path, size: u32, index: &mut ThemeIndex) {
    let Ok(ctx_rd) = fs::read_dir(size_dir) else {
        return;
    };
    for ctx_ent in ctx_rd.flatten() {
        let ctx_path = ctx_ent.path();
        if !ctx_path.is_dir() {
            continue;
        }
        let context = normalize_context(&ctx_ent.file_name().to_string_lossy());
        scan_flat_icons(&ctx_path, &context, size, index);
    }
}

fn scan_flat_icons(dir: &Path, context: &str, size: u32, index: &mut ThemeIndex) {
    let Ok(files) = fs::read_dir(dir) else {
        return;
    };
    for file in files.flatten() {
        let path = file.path();
        if !path.is_file() {
            continue;
        }
        let fname = file.file_name().to_string_lossy().into_owned();
        let Some(name) = strip_icon_ext(&fname) else {
            continue;
        };
        index
            .entry((context.to_string(), name.to_string()))
            .or_default()
            .entry(size)
            .or_insert(path);
    }
}

fn normalize_context(ctx: &str) -> String {
    match ctx {
        "filesystems" => "places".into(),
        other => other.to_string(),
    }
}

fn image_size_hint(path: &Path) -> Option<u32> {
    let img = image::image_dimensions(path).ok()?;
    Some(img.0.max(img.1))
}

fn parse_size(dir: &str) -> Option<u32> {
    // "48x48", "48"
    if let Some((a, b)) = dir.split_once('x') {
        let w: u32 = a.parse().ok()?;
        let h: u32 = b.parse().ok()?;
        if w == h {
            return Some(w);
        }
    }
    dir.parse().ok()
}

fn strip_icon_ext(name: &str) -> Option<&str> {
    let lower = name.to_ascii_lowercase();
    for ext in [".png", ".svg", ".xpm", ".jpg", ".jpeg", ".gif", ".webp"] {
        if lower.ends_with(ext) {
            return Some(&name[..name.len() - ext.len()]);
        }
    }
    None
}

fn find_icon<'a>(
    index: &'a ThemeIndex,
    candidates: &[&str],
    size: u32,
) -> Option<(&'a PathBuf, u32)> {
    for cand in candidates {
        let (context, name) = split_purpose(cand)?;
        if let Some(by_size) = index.get(&(context.to_string(), name.to_string())) {
            if let Some(path) = by_size.get(&size) {
                return Some((path, size));
            }
        }
    }
    None
}

fn find_best<'a>(
    index: &'a ThemeIndex,
    candidates: &[&str],
    prefer: &[u32],
) -> Option<(PathBuf, u32)> {
    for &size in prefer.iter().chain([128, 256, 64, 22].iter()) {
        if let Some((p, s)) = find_icon(index, candidates, size) {
            return Some((p.clone(), s));
        }
    }
    // Any size for first matching candidate.
    for cand in candidates {
        let (context, name) = split_purpose(cand)?;
        if let Some(by_size) = index.get(&(context.to_string(), name.to_string())) {
            if let Some((&sz, path)) = by_size.iter().next() {
                return Some((path.clone(), sz));
            }
        }
    }
    None
}

fn split_purpose(p: &str) -> Option<(&str, &str)> {
    p.split_once('/')
}

fn load_as_rgba(path: &Path) -> Result<IconRgba> {
    let img = image::open(path)
        .with_context(|| format!("open {}", path.display()))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    Ok(IconRgba {
        width: w,
        height: h,
        rgba: img.into_raw(),
    })
}
