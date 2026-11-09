//! fuckWM / `dwm` — shell + window-manager host.
//!
//! - **Native backends** (`backend/`): X11 reparenting WM (Linux), Win32 shell host (Windows)
//! - **GPU** (`gpu/`): EGL/GLES2 on XCB (Linux, incl. Xephyr+Glamor), WGL on HWND (Windows)
//! - **Xephyr** (`xephyr`): `--xephyr` nested server with `-glamor` when available
//! - **SDL preview** (`shell`): `--sdl` / fallback when native is unavailable

mod backend;
mod desktop;
mod gpu;
mod shell;
pub(crate) mod start_menu;
mod taskbar;
pub(crate) mod wallpaper;

#[cfg(unix)]
mod xephyr;

pub use shell::boot;
