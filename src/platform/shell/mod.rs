use anyhow::Result;
use std::path::Path;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

/// Open a terminal/shell with cwd set to `dir`.
pub fn open_terminal(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        unix::open_terminal(dir)
    }
    #[cfg(windows)]
    {
        windows::open_terminal(dir)
    }
}

#[cfg(windows)]
pub use windows::{
    ensure_cmd_context_menu_entries, open_cmd, open_cmd_admin, open_powershell,
    open_powershell_admin,
};
