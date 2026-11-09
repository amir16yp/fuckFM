use anyhow::Result;
use std::path::{Path, PathBuf};

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextKind {
    /// Empty space in a folder (directory background).
    Background,
    /// One or more selected files/folders.
    Selection,
}

#[derive(Clone, Debug)]
pub enum ContextAction {
    Open,
    /// Open the selected folder in a new tab.
    OpenInNewTab,
    /// Open file with a specific handler (`desktop` id / exe name).
    OpenWith { app_id: String },
    /// System / DE “Open with” chooser for a file.
    OpenWithOther,
    OpenTerminal,
    Rename,
    NewFolder,
    /// Create a new empty file in the current folder, then rename.
    NewFile,
    /// Duplicate selected items into the same folder (auto-renamed).
    Duplicate,
    /// Close the tab at `index`.
    CloseTab { index: usize },
    /// Close every tab except `index`.
    CloseOtherTabs { index: usize },
    /// Close tabs to the right of `index` (higher indices).
    CloseTabsToRight { index: usize },
    #[cfg(windows)]
    OpenCmd,
    #[cfg(windows)]
    OpenCmdAdmin,
    #[cfg(windows)]
    OpenPowerShell,
    #[cfg(windows)]
    OpenPowerShellAdmin,
    Cut,
    Copy,
    /// Copy selected path(s) as plain text to the system clipboard.
    CopyPath,
    /// Copy selected path(s) as `file://` URIs to the system clipboard.
    CopyUri,
    Paste,
    /// Move selection into a known folder (Desktop / Home / Documents…).
    SendTo { dest: PathBuf },
    /// Send selection to Trash / Recycle Bin.
    SendToTrash,
    /// Restore selection from Trash / Recycle Bin.
    RestoreFromTrash,
    /// Permanently delete selection (bypass trash).
    DeletePermanently,
    /// Empty the entire Trash / Recycle Bin.
    EmptyTrash,
    /// Open folder (or stay in current) and focus the search bar.
    SearchInFolder,
    /// Pin selected/current folder to the side pane bookmarks.
    AddBookmark,
    /// Remove a bookmark path from the side pane.
    RemoveBookmark { path: PathBuf },
    /// Rename the display label of a side-pane bookmark.
    RenameBookmark { path: PathBuf },
    /// Show properties for the current selection.
    Properties,
    /// Jump `index` steps back in navigation history (0 = previous).
    HistoryBack { index: usize },
    /// Jump `index` steps forward in navigation history (0 = next).
    HistoryForward { index: usize },
    /// Create a symlink / link next to the selected item.
    CreateLink,
    /// Platform/registry-provided command string.
    #[cfg(windows)]
    ShellCommand { command: String, admin: bool },
}

#[derive(Clone, Debug)]
pub struct ContextItem {
    pub label: String,
    pub action: ContextAction,
    pub separator_before: bool,
}

pub fn build_context_menu(
    kind: ContextKind,
    folder: Option<&Path>,
    selected: &[PathBuf],
    in_trash: bool,
) -> Result<Vec<ContextItem>> {
    #[cfg(unix)]
    {
        unix::build_context_menu(kind, folder, selected, in_trash)
    }
    #[cfg(windows)]
    {
        windows::build_context_menu(kind, folder, selected, in_trash)
    }
}

pub fn run_action(action: &ContextAction, folder: Option<&Path>) -> Result<()> {
    match action {
        ContextAction::OpenTerminal => {
            let dir = folder.ok_or_else(|| anyhow::anyhow!("no folder for terminal"))?;
            crate::platform::open_terminal(dir)
        }
        #[cfg(windows)]
        ContextAction::OpenCmd => {
            let dir = folder.ok_or_else(|| anyhow::anyhow!("no folder"))?;
            crate::platform::shell::open_cmd(dir)
        }
        #[cfg(windows)]
        ContextAction::OpenCmdAdmin => {
            let dir = folder.ok_or_else(|| anyhow::anyhow!("no folder"))?;
            crate::platform::shell::open_cmd_admin(dir)
        }
        #[cfg(windows)]
        ContextAction::OpenPowerShell => {
            let dir = folder.ok_or_else(|| anyhow::anyhow!("no folder"))?;
            crate::platform::shell::open_powershell(dir)
        }
        #[cfg(windows)]
        ContextAction::OpenPowerShellAdmin => {
            let dir = folder.ok_or_else(|| anyhow::anyhow!("no folder"))?;
            crate::platform::shell::open_powershell_admin(dir)
        }
        #[cfg(windows)]
        ContextAction::ShellCommand { command, admin } => run_shell_command(command, *admin),
        ContextAction::Open
        | ContextAction::OpenInNewTab
        | ContextAction::OpenWith { .. }
        | ContextAction::OpenWithOther
        | ContextAction::Rename
        | ContextAction::NewFolder
        | ContextAction::NewFile
        | ContextAction::Duplicate
        | ContextAction::CloseTab { .. }
        | ContextAction::CloseOtherTabs { .. }
        | ContextAction::CloseTabsToRight { .. }
        | ContextAction::Cut
        | ContextAction::Copy
        | ContextAction::CopyPath
        | ContextAction::CopyUri
        | ContextAction::Paste
        | ContextAction::SendTo { .. }
        | ContextAction::SendToTrash
        | ContextAction::RestoreFromTrash
        | ContextAction::DeletePermanently
        | ContextAction::EmptyTrash
        | ContextAction::SearchInFolder
        | ContextAction::AddBookmark
        | ContextAction::RemoveBookmark { .. }
        | ContextAction::RenameBookmark { .. }
        | ContextAction::Properties
        | ContextAction::HistoryBack { .. }
        | ContextAction::HistoryForward { .. }
        | ContextAction::CreateLink => Ok(()),
    }
}

/// Append “Send to …” destinations for the current selection.
pub fn push_send_to_items(items: &mut Vec<ContextItem>) {
    let mut first = true;
    for (label, path) in send_to_destinations() {
        items.push(ContextItem {
            label: format!("Send to {label}"),
            action: ContextAction::SendTo { dest: path },
            separator_before: first,
        });
        first = false;
    }
}

fn send_to_destinations() -> Vec<(String, PathBuf)> {
    use crate::ops::{desktop_dir, home_dir};
    let mut out = Vec::new();
    let mut push = |label: &str, path: PathBuf| {
        if !path.is_dir() {
            return;
        }
        if out.iter().any(|(_, p)| p == &path) {
            return;
        }
        out.push((label.to_string(), path));
    };
    push("Desktop", desktop_dir());
    push("Home", home_dir());
    for (label, path) in crate::platform::folders::extra_places() {
        push(&label, path);
    }
    out
}

pub fn tab_context_menu(index: usize, tab_count: usize) -> Vec<ContextItem> {
    let mut items = vec![ContextItem {
        label: "Close Tab".into(),
        action: ContextAction::CloseTab { index },
        separator_before: false,
    }];
    if tab_count > 1 {
        items.push(ContextItem {
            label: "Close Other Tabs".into(),
            action: ContextAction::CloseOtherTabs { index },
            separator_before: true,
        });
    }
    if index + 1 < tab_count {
        items.push(ContextItem {
            label: "Close Tabs to the Right".into(),
            action: ContextAction::CloseTabsToRight { index },
            separator_before: tab_count <= 1,
        });
    }
    items
}

/// Append “Open with …” entries for a single selected file.
pub fn push_open_with_items(items: &mut Vec<ContextItem>, path: &Path) {
    if !path.is_file() {
        return;
    }
    let apps = crate::platform::open::list_open_with(path);
    let mut first = true;
    for app in apps {
        items.push(ContextItem {
            label: format!("Open with {}", app.name),
            action: ContextAction::OpenWith { app_id: app.id },
            separator_before: first,
        });
        first = false;
    }
    items.push(ContextItem {
        label: if first {
            "Open with…".into()
        } else {
            "Open with Other…".into()
        },
        action: ContextAction::OpenWithOther,
        separator_before: first,
    });
}

#[cfg(windows)]
fn run_shell_command(command: &str, admin: bool) -> Result<()> {
    use std::process::{Command, Stdio};
    if admin {
        Command::new("powershell.exe")
            .args([
                "-Command",
                &format!(
                    "Start-Process -FilePath cmd.exe -Verb RunAs -ArgumentList '/C {}'",
                    command.replace('\'', "''")
                ),
            ])
            .stdin(Stdio::null())
            .spawn()?;
    } else {
        Command::new("cmd.exe")
            .args(["/C", command])
            .stdin(Stdio::null())
            .spawn()?;
    }
    Ok(())
}
