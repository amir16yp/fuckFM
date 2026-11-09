//! Shared library for **fuckFM** (file manager) and **fuckWM** / **dwm** (shell + WM host).
//!
//! Binaries:
//! - `fuckfm` — file manager
//! - `fuckwm` — Linux-oriented WM/shell host name
//! - `dwm` — Windows-oriented shell+WM host name (same code path today)

pub mod app;
pub mod browser;
pub mod fatal;
pub mod ffini;
pub mod iconpack;
pub mod ops;
pub mod platform;
pub mod render;
pub mod settings;
pub mod theme;
pub mod ui;
pub mod undo;
pub mod wm;
