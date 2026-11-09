use super::{ContextAction, ContextItem, ContextKind};
use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use winreg::enums::*;
use winreg::RegKey;

pub fn build_context_menu(
    kind: ContextKind,
    folder: Option<&Path>,
    selected: &[PathBuf],
    in_trash: bool,
) -> Result<Vec<ContextItem>> {
    if in_trash {
        return Ok(trash_menu(kind, !selected.is_empty()));
    }

    // Ensure CMD verbs exist in HKCU for Explorer + our menu.
    let _ = crate::platform::shell::ensure_cmd_context_menu_entries();

    let mut items = Vec::new();
    let has_folder = folder.map(|p| p.is_dir()).unwrap_or(false);
    let folder_s = folder.map(|p| p.display().to_string()).unwrap_or_default();

    if kind == ContextKind::Selection {
        items.push(ContextItem {
            label: "Open".into(),
            action: ContextAction::Open,
            separator_before: false,
        });
        if selected.len() == 1 && selected[0].is_dir() {
            items.push(ContextItem {
                label: "Open in New Tab".into(),
                action: ContextAction::OpenInNewTab,
                separator_before: false,
            });
            items.push(ContextItem {
                label: "Add Bookmark".into(),
                action: ContextAction::AddBookmark,
                separator_before: false,
            });
            items.push(ContextItem {
                label: "Search".into(),
                action: ContextAction::SearchInFolder,
                separator_before: false,
            });
        }
        if selected.len() == 1 {
            if selected[0].is_file() {
                super::push_open_with_items(&mut items, &selected[0]);
            }
            items.push(ContextItem {
                label: "Rename".into(),
                action: ContextAction::Rename,
                separator_before: true,
            });
        }
    }

    // Built-in shell entries (always present).
    let shell_sep = !items.is_empty();
    items.push(ContextItem {
        label: "Open with PowerShell".into(),
        action: ContextAction::OpenPowerShell,
        separator_before: shell_sep,
    });
    items.push(ContextItem {
        label: "Open with PowerShell (Admin)".into(),
        action: ContextAction::OpenPowerShellAdmin,
        separator_before: false,
    });
    items.push(ContextItem {
        label: "Open with CMD".into(),
        action: ContextAction::OpenCmd,
        separator_before: false,
    });
    items.push(ContextItem {
        label: "Open with CMD (Admin)".into(),
        action: ContextAction::OpenCmdAdmin,
        separator_before: false,
    });

    // Registry-backed shell verbs (Directory or Directory\Background).
    let mut seen_labels: HashSet<String> = items
        .iter()
        .map(|i| normalize_label(&i.label))
        .collect();
    seen_labels.insert(normalize_label("Open"));
    seen_labels.insert(normalize_label("Rename"));

    let keys = match kind {
        ContextKind::Background => [
            r"Software\Classes\Directory\Background\shell",
            r"Directory\Background\shell",
        ],
        ContextKind::Selection => {
            if selected.iter().any(|p| p.is_dir()) {
                [r"Software\Classes\Directory\shell", r"Directory\shell"]
            } else {
                [r"Software\Classes\*\shell", r"*\shell"]
            }
        }
    };

    let mut reg_items = Vec::new();
    for key_path in keys {
        collect_shell_verbs(key_path, &folder_s, selected, &mut reg_items, &mut seen_labels);
    }

    if !reg_items.is_empty() {
        if let Some(first) = reg_items.first_mut() {
            first.separator_before = true;
        }
        items.append(&mut reg_items);
    }

    match kind {
        ContextKind::Background => {
            if has_folder {
                items.push(ContextItem {
                    label: "New Folder".into(),
                    action: ContextAction::NewFolder,
                    separator_before: true,
                });
                items.push(ContextItem {
                    label: "New Empty File".into(),
                    action: ContextAction::NewFile,
                    separator_before: false,
                });
                items.push(ContextItem {
                    label: "Add Bookmark".into(),
                    action: ContextAction::AddBookmark,
                    separator_before: false,
                });
                items.push(ContextItem {
                    label: "Search".into(),
                    action: ContextAction::SearchInFolder,
                    separator_before: false,
                });
            } else {
                items.push(ContextItem {
                    label: "Connect to Server…".into(),
                    action: ContextAction::ConnectRemote,
                    separator_before: true,
                });
            }
            items.push(ContextItem {
                label: "Paste".into(),
                action: ContextAction::Paste,
                separator_before: false,
            });
        }
        ContextKind::Selection => {
            items.push(ContextItem {
                label: "Cut".into(),
                action: ContextAction::Cut,
                separator_before: true,
            });
            items.push(ContextItem {
                label: "Copy".into(),
                action: ContextAction::Copy,
                separator_before: false,
            });
            items.push(ContextItem {
                label: "Copy Path".into(),
                action: ContextAction::CopyPath,
                separator_before: false,
            });
            items.push(ContextItem {
                label: "Copy as URI".into(),
                action: ContextAction::CopyUri,
                separator_before: false,
            });
            items.push(ContextItem {
                label: "Duplicate".into(),
                action: ContextAction::Duplicate,
                separator_before: false,
            });
            if selected.len() == 1 {
                items.push(ContextItem {
                    label: "Create Link".into(),
                    action: ContextAction::CreateLink,
                    separator_before: true,
                });
                if selected[0].is_file() {
                    items.push(ContextItem {
                        label: "Create Hard Link".into(),
                        action: ContextAction::CreateHardLink,
                        separator_before: false,
                    });
                }
                super::push_attr_perm_items(&mut items, &selected[0]);
                let p = &selected[0];
                let s = p.to_string_lossy();
                let is_drive = (s.len() >= 2 && s.as_bytes().get(1) == Some(&b':'))
                    || s.starts_with("\\\\");
                if is_drive {
                    super::push_volume_items(&mut items, p, p.exists());
                }
                if s.starts_with("smb://") || s.starts_with("\\\\") {
                    items.push(ContextItem {
                        label: "Connect / Open".into(),
                        action: ContextAction::ConnectRemote,
                        separator_before: true,
                    });
                    items.push(ContextItem {
                        label: "Bookmark Remote".into(),
                        action: ContextAction::AddRemoteBookmark,
                        separator_before: false,
                    });
                }
            }
            items.push(ContextItem {
                label: "Properties".into(),
                action: ContextAction::Properties,
                separator_before: selected.len() != 1,
            });
            super::push_send_to_items(&mut items);
            items.push(ContextItem {
                label: "Send to Recycle Bin".into(),
                action: ContextAction::SendToTrash,
                separator_before: true,
            });
            items.push(ContextItem {
                label: "Delete Permanently".into(),
                action: ContextAction::DeletePermanently,
                separator_before: false,
            });
        }
    }

    Ok(items)
}

fn trash_menu(kind: ContextKind, has_selection: bool) -> Vec<ContextItem> {
    let mut items = Vec::new();
    match kind {
        ContextKind::Selection if has_selection => {
            items.push(ContextItem {
                label: "Restore".into(),
                action: ContextAction::RestoreFromTrash,
                separator_before: false,
            });
            items.push(ContextItem {
                label: "Delete Permanently".into(),
                action: ContextAction::DeletePermanently,
                separator_before: true,
            });
        }
        _ => {
            items.push(ContextItem {
                label: "Empty Recycle Bin".into(),
                action: ContextAction::EmptyTrash,
                separator_before: false,
            });
        }
    }
    items
}

fn collect_shell_verbs(
    key_path: &str,
    folder: &str,
    selected: &[PathBuf],
    out: &mut Vec<ContextItem>,
    seen: &mut HashSet<String>,
) {
    let roots = [
        RegKey::predef(HKEY_CURRENT_USER),
        RegKey::predef(HKEY_CLASSES_ROOT),
    ];
    for root in roots {
        let Ok(shell) = root.open_subkey(key_path) else {
            continue;
        };
        for verb_name in shell.enum_keys().filter_map(|k| k.ok()) {
            let Ok(verb) = shell.open_subkey(&verb_name) else {
                continue;
            };
            // Skip legacy/extended-only if marked Extended without showing — still include common ones.
            let label = verb
                .get_value::<String, _>("MUIVerb")
                .or_else(|_| verb.get_value::<String, _>(""))
                .unwrap_or_else(|_| verb_name.clone());
            let label = clean_mui(&label);
            let norm = normalize_label(&label);
            if seen.contains(&norm) || is_duplicate_builtin(&norm) {
                continue;
            }
            let Ok(cmd_key) = verb.open_subkey("command") else {
                continue;
            };
            let Ok(cmd) = cmd_key.get_value::<String, _>("") else {
                continue;
            };
            let admin = verb.get_value::<String, _>("HasLUAShield").is_ok();
            let expanded = expand_command(&cmd, folder, selected);
            seen.insert(norm);
            out.push(ContextItem {
                label,
                action: ContextAction::ShellCommand {
                    command: expanded,
                    admin,
                },
                separator_before: false,
            });
        }
    }
}

fn expand_command(cmd: &str, folder: &str, selected: &[PathBuf]) -> String {
    let first = selected
        .first()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| folder.to_string());
    cmd.replace("%V", folder)
        .replace("%v", folder)
        .replace("%1", &first)
        .replace("%L", &first)
        .replace("%l", &first)
        .replace("%W", folder)
}

fn clean_mui(s: &str) -> String {
    // Strip @dll,-id resource refs poorly; if it looks like that, keep verb-ish fallback.
    if s.starts_with('@') {
        s.trim_start_matches('@').to_string()
    } else {
        s.trim_matches('"').to_string()
    }
}

fn normalize_label(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn is_duplicate_builtin(norm: &str) -> bool {
    matches!(
        norm,
        "openwithpowershell"
            | "openpowershellhere"
            | "powershell"
            | "openwithpowershelladmin"
            | "openwithcmd"
            | "openwithcmdadmin"
            | "cmd"
            | "commandprompt"
            | "opencommandwindowhere"
            | "opencmdherefuckfm"
            | "opencmdhereadminfuckfm"
    ) || norm.contains("powershell")
        || norm.contains("opencmd")
        || norm.contains("commandwindow")
}