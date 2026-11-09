//! Shared library for **fuckFM** (file manager) and **fuckWM** (shell + WM host).
//!
//! Binaries:
//! - `fuckfm` — file manager
//! - `fuckwm` — shell/WM host on Linux and Windows (Explorer shell replacement on Windows;
//!   does **not** replace Microsoft’s Desktop Window Manager compositor)

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
