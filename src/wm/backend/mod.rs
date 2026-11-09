//! Native display backends for fuckWM.
//!
//! - Unix: X11 reparenting WM (`x11` module)
//! - Windows: Win32 shell host (`win32` module)
//!
//! SDL preview remains available via `--sdl` / `FUCKWM_BACKEND=sdl`.

use anyhow::Result;

#[cfg(unix)]
mod x11;

#[cfg(windows)]
mod win32;

/// Run the platform-native shell/WM host until quit.
pub fn run_native(product: &str) -> Result<()> {
    #[cfg(unix)]
    {
        x11::run(product)
    }
    #[cfg(windows)]
    {
        win32::run(product)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = product;
        anyhow::bail!("no native WM backend on this platform")
    }
}
