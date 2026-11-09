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
    /// Set handler as the default for this file’s type.
    SetDefaultApp { app_id: String },
    /// Toggle Windows Read-only attribute.
    ToggleAttrReadOnly,
    /// Toggle Windows Hidden attribute.
    ToggleAttrHidden,
    /// Edit Unix permissions (octal prompt in the UI).
    EditPermissions,
    /// Take ownership (Windows elevated / Linux pkexec).
    TakeOwnership,
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
    /// Create a hard link next to the selected file (same filesystem).
    CreateHardLink,
    /// Toggle visibility of a Details column (Type / Size / Modified).
    ToggleDetailColumn { key: crate::browser::SortKey },
    /// Reorder an optional Details column one step left/right.
    MoveDetailColumn { key: crate::browser::SortKey, left: bool },
    MountVolume,
    UnmountVolume,
    EjectVolume,
    RenameVolumeLabel,
    FormatVolume,
    ConnectRemote,
    AddRemoteBookmark,
    DisconnectRemote,
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
        | ContextAction::SetDefaultApp { .. }
        | ContextAction::ToggleAttrReadOnly
        | ContextAction::ToggleAttrHidden
        | ContextAction::EditPermissions
        | ContextAction::TakeOwnership
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
        | ContextAction::CreateLink
        | ContextAction::CreateHardLink
        | ContextAction::ToggleDetailColumn { .. }
        | ContextAction::MoveDetailColumn { .. }
        | ContextAction::MountVolume
        | ContextAction::UnmountVolume
        | ContextAction::EjectVolume
        | ContextAction::RenameVolumeLabel
        | ContextAction::FormatVolume
        | ContextAction::ConnectRemote
        | ContextAction::AddRemoteBookmark
        | ContextAction::DisconnectRemote => Ok(()),
    }
}

/// Volume actions for a selected drive / device path on This PC.
pub fn push_volume_items(items: &mut Vec<ContextItem>, path: &Path, mounted: bool) {
    if mounted {
        items.push(ContextItem {
            label: "Unmount".into(),
            action: ContextAction::UnmountVolume,
            separator_before: true,
        });
        items.push(ContextItem {
            label: "Eject".into(),
            action: ContextAction::EjectVolume,
            separator_before: false,
        });
        items.push(ContextItem {
            label: "Rename Volume Label…".into(),
            action: ContextAction::RenameVolumeLabel,
            separator_before: false,
        });
        items.push(ContextItem {
            label: "Format…".into(),
            action: ContextAction::FormatVolume,
            separator_before: false,
        });
    } else {
        items.push(ContextItem {
            label: "Mount".into(),
            action: ContextAction::MountVolume,
            separator_before: true,
        });
        let _ = path;
    }
}

/// Right-click Details header: show/hide + reorder columns.
pub fn detail_column_menu(
    show_type: bool,
    show_size: bool,
    show_modified: bool,
    under_cursor: Option<crate::browser::SortKey>,
) -> Vec<ContextItem> {
    use crate::browser::SortKey;
    let check = |on: bool| if on { "[x] " } else { "[ ] " };
    let mut items = vec![
        ContextItem {
            label: format!("{}Type", check(show_type)),
            action: ContextAction::ToggleDetailColumn { key: SortKey::Type },
            separator_before: false,
        },
        ContextItem {
            label: format!("{}Size", check(show_size)),
            action: ContextAction::ToggleDetailColumn { key: SortKey::Size },
            separator_before: false,
        },
        ContextItem {
            label: format!("{}Modified", check(show_modified)),
            action: ContextAction::ToggleDetailColumn {
                key: SortKey::Modified,
            },
            separator_before: false,
        },
    ];
    if let Some(key) = under_cursor.filter(|k| !matches!(k, SortKey::Name)) {
        let label = match key {
            SortKey::Type => "Type",
            SortKey::Size => "Size",
            SortKey::Modified => "Modified",
            SortKey::Name => "",
        };
        items.push(ContextItem {
            label: format!("Move {label} Left"),
            action: ContextAction::MoveDetailColumn { key, left: true },
            separator_before: true,
        });
        items.push(ContextItem {
            label: format!("Move {label} Right"),
            action: ContextAction::MoveDetailColumn { key, left: false },
            separator_before: false,
        });
    }
    items
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

/// Append “Open with …” + “Set default …” entries for a single selected file.
pub fn push_open_with_items(items: &mut Vec<ContextItem>, path: &Path) {
    if !path.is_file() {
        return;
    }
    let apps = crate::platform::open::list_open_with(path);
    let mut first = true;
    for app in &apps {
        items.push(ContextItem {
            label: format!("Open with {}", app.name),
            action: ContextAction::OpenWith {
                app_id: app.id.clone(),
            },
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
    let mut first_default = true;
    for app in apps {
        items.push(ContextItem {
            label: format!("Set default: {}", app.name),
            action: ContextAction::SetDefaultApp { app_id: app.id },
            separator_before: first_default,
        });
        first_default = false;
    }
}

/// Append attribute / permission / ownership actions for a single path.
pub fn push_attr_perm_items(items: &mut Vec<ContextItem>, path: &Path) {
    #[cfg(windows)]
    {
        let attrs = crate::platform::attrs::get_win_attrs(path).unwrap_or_default();
        let ro = if attrs.readonly { "[x]" } else { "[ ]" };
        let hid = if attrs.hidden { "[x]" } else { "[ ]" };
        items.push(ContextItem {
            label: format!("{ro} Read-only"),
            action: ContextAction::ToggleAttrReadOnly,
            separator_before: true,
        });
        items.push(ContextItem {
            label: format!("{hid} Hidden"),
            action: ContextAction::ToggleAttrHidden,
            separator_before: false,
        });
        items.push(ContextItem {
            label: "Take Ownership (Admin)…".into(),
            action: ContextAction::TakeOwnership,
            separator_before: false,
        });
    }
    #[cfg(unix)]
    {
        let mode = crate::platform::attrs::get_unix_mode(path)
            .ok()
            .flatten()
            .map(|m| format!("Permissions ({m:04o})…"))
            .unwrap_or_else(|| "Permissions…".into());
        items.push(ContextItem {
            label: mode,
            action: ContextAction::EditPermissions,
            separator_before: true,
        });
        items.push(ContextItem {
            label: "Take Ownership…".into(),
            action: ContextAction::TakeOwnership,
            separator_before: false,
        });
        let _ = path;
    }
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
