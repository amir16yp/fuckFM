use super::{ContextAction, ContextItem, ContextKind};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub fn build_context_menu(
    kind: ContextKind,
    folder: Option<&Path>,
    selected: &[PathBuf],
    in_trash: bool,
) -> Result<Vec<ContextItem>> {
    let mut items = Vec::new();
    let has_folder = folder.map(|p| p.is_dir()).unwrap_or(false);

    if in_trash {
        return Ok(trash_menu(kind, !selected.is_empty()));
    }

    match kind {
        ContextKind::Background => {
            if has_folder {
                items.push(ContextItem {
                    label: "New Folder".into(),
                    action: ContextAction::NewFolder,
                    separator_before: false,
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
                items.push(ContextItem {
                    label: "Open Terminal Here".into(),
                    action: ContextAction::OpenTerminal,
                    separator_before: false,
                });
            }
            items.push(ContextItem {
                label: "Paste".into(),
                action: ContextAction::Paste,
                separator_before: true,
            });
        }
        ContextKind::Selection => {
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
            if selected.len() == 1 && selected[0].is_dir() {
                items.push(ContextItem {
                    label: "Add Bookmark".into(),
                    action: ContextAction::AddBookmark,
                    separator_before: true,
                });
                items.push(ContextItem {
                    label: "Search".into(),
                    action: ContextAction::SearchInFolder,
                    separator_before: false,
                });
                items.push(ContextItem {
                    label: "Open Terminal Here".into(),
                    action: ContextAction::OpenTerminal,
                    separator_before: false,
                });
            } else if has_folder && selected.iter().all(|p| p.is_dir()) {
                items.push(ContextItem {
                    label: "Open Terminal Here".into(),
                    action: ContextAction::OpenTerminal,
                    separator_before: true,
                });
            }
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
            }
            items.push(ContextItem {
                label: "Properties".into(),
                action: ContextAction::Properties,
                separator_before: selected.len() != 1,
            });
            super::push_send_to_items(&mut items);
            items.push(ContextItem {
                label: "Send to Trash".into(),
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
                label: "Empty Trash".into(),
                action: ContextAction::EmptyTrash,
                separator_before: false,
            });
        }
    }
    items
}
