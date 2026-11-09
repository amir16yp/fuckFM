//! Undo / redo for common file operations (rename, move, copy, trash).

use crate::ops::{move_path, remove_path};
use crate::platform::{restore_from_trash, trash_path};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;

const MAX_DEPTH: usize = 40;

#[derive(Clone, Debug)]
pub enum UndoOp {
    /// Renamed `from` → `to`.
    Rename { from: PathBuf, to: PathBuf },
    /// Moved each `from` → `to` (full destination paths).
    Move { pairs: Vec<(PathBuf, PathBuf)> },
    /// Created by copy / duplicate — undo removes permanently; redo not supported.
    Create { paths: Vec<PathBuf> },
    /// Sent to trash. `trashed` paths are inside the trash when known (XDG).
    /// Redo re-trashes the restored originals when possible.
    Trash {
        /// (original path before trash, path inside trash)
        items: Vec<(PathBuf, PathBuf)>,
    },
}

#[derive(Default)]
pub struct UndoStack {
    undo: Vec<UndoOp>,
    redo: Vec<UndoOp>,
}

impl UndoStack {
    pub fn push(&mut self, op: UndoOp) {
        if op_empty(&op) {
            return;
        }
        self.undo.push(op);
        self.redo.clear();
        while self.undo.len() > MAX_DEPTH {
            self.undo.remove(0);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> Result<Option<&'static str>> {
        let Some(op) = self.undo.pop() else {
            return Ok(None);
        };
        let label = op_label(&op);
        reverse(&op)?;
        self.redo.push(op);
        while self.redo.len() > MAX_DEPTH {
            self.redo.remove(0);
        }
        Ok(Some(label))
    }

    pub fn redo(&mut self) -> Result<Option<&'static str>> {
        let Some(op) = self.redo.pop() else {
            return Ok(None);
        };
        let label = op_label(&op);
        forward(&op)?;
        self.undo.push(op);
        Ok(Some(label))
    }
}

fn op_empty(op: &UndoOp) -> bool {
    match op {
        UndoOp::Rename { .. } => false,
        UndoOp::Move { pairs } => pairs.is_empty(),
        UndoOp::Create { paths } => paths.is_empty(),
        UndoOp::Trash { items } => items.is_empty(),
    }
}

fn op_label(op: &UndoOp) -> &'static str {
    match op {
        UndoOp::Rename { .. } => "rename",
        UndoOp::Move { .. } => "move",
        UndoOp::Create { .. } => "copy",
        UndoOp::Trash { .. } => "trash",
    }
}

fn reverse(op: &UndoOp) -> Result<()> {
    match op {
        UndoOp::Rename { from, to } => {
            if !to.exists() {
                bail!("cannot undo rename: {} missing", to.display());
            }
            fs::rename(to, from)
                .with_context(|| format!("undo rename {} -> {}", to.display(), from.display()))?;
        }
        UndoOp::Move { pairs } => {
            for (from, to) in pairs.iter().rev() {
                if !to.exists() {
                    continue;
                }
                let parent = from
                    .parent()
                    .ok_or_else(|| anyhow::anyhow!("undo move: no parent"))?;
                fs::create_dir_all(parent)?;
                if !from.exists() {
                    fs::rename(to, from).or_else(|_| {
                        move_path(to, parent).map(|_| ())
                    })?;
                } else {
                    move_path(to, parent)?;
                }
            }
        }
        UndoOp::Create { paths } => {
            for path in paths {
                if path.exists() {
                    remove_path(path)?;
                }
            }
        }
        UndoOp::Trash { items } => {
            for (_original, trashed) in items {
                if trashed.exists() {
                    restore_from_trash(trashed)?;
                }
            }
        }
    }
    Ok(())
}

fn forward(op: &UndoOp) -> Result<()> {
    match op {
        UndoOp::Rename { from, to } => {
            if !from.exists() {
                bail!("cannot redo rename: {} missing", from.display());
            }
            fs::rename(from, to)
                .with_context(|| format!("redo rename {} -> {}", from.display(), to.display()))?;
        }
        UndoOp::Move { pairs } => {
            for (from, to) in pairs {
                if !from.exists() {
                    continue;
                }
                let parent = to
                    .parent()
                    .ok_or_else(|| anyhow::anyhow!("redo move: no parent"))?;
                fs::create_dir_all(parent)?;
                if !to.exists() {
                    let _ = fs::rename(from, to).or_else(|_| move_path(from, parent).map(|_| ()));
                } else {
                    move_path(from, parent)?;
                }
            }
        }
        UndoOp::Create { .. } => {
            bail!("cannot redo copy/duplicate (content was removed)");
        }
        UndoOp::Trash { items } => {
            for (original, _) in items {
                if original.exists() {
                    let _ = trash_path(original)?;
                }
            }
        }
    }
    Ok(())
}
