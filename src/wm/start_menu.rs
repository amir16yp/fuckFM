//! Shared Start menu model for SDL preview + native shell hosts.

use crate::browser::THIS_PC;
use crate::iconpack::purpose;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const BRAND_W: i32 = 28;
pub const MENU_W: i32 = 248;
pub const PAD: i32 = 4;
pub const SEP_H: i32 = 7;
pub const ICON_SZ: i32 = 16;

#[derive(Clone, Debug)]
pub enum StartAction {
    LaunchFm,
    OpenDesktop,
    OpenDocuments,
    OpenPictures,
    OpenThisPc,
    OpenCmd,
    CycleTheme,
    LogOff,
    Restart,
    Shutdown,
    QuitSession,
}

#[derive(Clone, Debug)]
pub struct StartEntry {
    pub label: &'static str,
    pub action: StartAction,
    /// Icon pack purpose (e.g. `places/folder`); None = no glyph.
    pub icon: Option<&'static str>,
    pub separator_before: bool,
}

/// Classic places + power layout used by fuckWM Start.
pub fn default_entries() -> Vec<StartEntry> {
    vec![
        StartEntry {
            label: "fuckFM",
            action: StartAction::LaunchFm,
            icon: Some("apps/system-file-manager"),
            separator_before: false,
        },
        StartEntry {
            label: "This PC",
            action: StartAction::OpenThisPc,
            icon: Some(purpose::COMPUTER),
            separator_before: false,
        },
        StartEntry {
            label: "Documents",
            action: StartAction::OpenDocuments,
            icon: Some(purpose::FOLDER),
            separator_before: false,
        },
        StartEntry {
            label: "Desktop",
            action: StartAction::OpenDesktop,
            icon: Some(purpose::DESKTOP),
            separator_before: false,
        },
        StartEntry {
            label: "Pictures",
            action: StartAction::OpenPictures,
            icon: Some(purpose::IMAGE),
            separator_before: false,
        },
        StartEntry {
            label: "Command Prompt",
            action: StartAction::OpenCmd,
            icon: Some("apps/utilities-terminal"),
            separator_before: false,
        },
        StartEntry {
            label: "Cycle theme",
            action: StartAction::CycleTheme,
            icon: None,
            separator_before: true,
        },
        StartEntry {
            label: "Log off",
            action: StartAction::LogOff,
            icon: None,
            separator_before: true,
        },
        StartEntry {
            label: "Restart",
            action: StartAction::Restart,
            icon: None,
            separator_before: false,
        },
        StartEntry {
            label: "Shut down",
            action: StartAction::Shutdown,
            icon: None,
            separator_before: false,
        },
        StartEntry {
            label: "Exit session",
            action: StartAction::QuitSession,
            icon: None,
            separator_before: true,
        },
    ]
}

pub fn menu_height(entries: &[StartEntry], row_h: i32) -> i32 {
    let row = row_h.max(22);
    let mut h = PAD * 2;
    for e in entries {
        if e.separator_before {
            h += SEP_H;
        }
        h += row;
    }
    h
}

/// Map client Y (relative to menu top) to an entry index.
pub fn hit_index(entries: &[StartEntry], y: i32, row_h: i32) -> Option<usize> {
    let row = row_h.max(22);
    let mut cursor = PAD;
    for (i, e) in entries.iter().enumerate() {
        if e.separator_before {
            cursor += SEP_H;
        }
        if y >= cursor && y < cursor + row {
            return Some(i);
        }
        cursor += row;
    }
    None
}

/// Top Y of entry `index` content row (after any separator), relative to menu top.
pub fn entry_y(entries: &[StartEntry], index: usize, row_h: i32) -> i32 {
    let row = row_h.max(22);
    let mut cursor = PAD;
    for (i, e) in entries.iter().enumerate() {
        if e.separator_before {
            cursor += SEP_H;
        }
        if i == index {
            return cursor;
        }
        cursor += row;
    }
    cursor
}

pub fn documents_dir() -> Option<PathBuf> {
    crate::platform::folders::extra_places()
        .into_iter()
        .find(|(label, _)| label == "Documents")
        .map(|(_, p)| p)
        .or_else(|| {
            let p = crate::ops::home_dir().join("Documents");
            p.is_dir().then_some(p)
        })
}

pub fn pictures_dir() -> Option<PathBuf> {
    crate::platform::folders::extra_places()
        .into_iter()
        .find(|(label, _)| label == "Pictures")
        .map(|(_, p)| p)
        .or_else(|| {
            let p = crate::ops::home_dir().join("Pictures");
            p.is_dir().then_some(p)
        })
}

pub fn desktop_dir() -> Option<PathBuf> {
    let desk = crate::ops::home_dir().join("Desktop");
    desk.is_dir().then_some(desk)
}

pub fn launch_fm(path: Option<&Path>) -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    let fm = exe.with_file_name(if cfg!(windows) {
        "fuckfm.exe"
    } else {
        "fuckfm"
    });
    let mut cmd = Command::new(if fm.is_file() { fm } else { exe });
    if let Some(path) = path {
        cmd.arg(path);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

pub fn launch_fm_this_pc() -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    let fm = exe.with_file_name(if cfg!(windows) {
        "fuckfm.exe"
    } else {
        "fuckfm"
    });
    let mut cmd = Command::new(if fm.is_file() { fm } else { exe });
    cmd.arg(THIS_PC);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

pub fn open_command_prompt() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        Command::new("cmd.exe")
            .creation_flags(CREATE_NEW_CONSOLE)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        for bin in ["x-terminal-emulator", "xdg-terminal-exec", "kitty", "alacritty", "xterm"] {
            if Command::new(bin)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .is_ok()
            {
                return Ok(());
            }
        }
        anyhow::bail!("no terminal found");
    }
}

/// Windows power actions via `shutdown.exe` (no SeShutdownPrivilege plumbing).
pub fn power_action(kind: &str) {
    #[cfg(windows)]
    {
        let args: &[&str] = match kind {
            "logoff" => &["/l", "/f"],
            "restart" => &["/r", "/t", "0"],
            "shutdown" => &["/s", "/t", "0"],
            _ => return,
        };
        let _ = Command::new("shutdown.exe")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = kind;
    }
}
