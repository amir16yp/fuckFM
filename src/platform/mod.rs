//! OS-specific integration. Dependencies for each target are gated in Cargo.toml
//! (`libc` on unix, `winreg` on windows) so the other OS build excludes them.

pub mod assoc;
mod context;
pub mod folders;
mod open;
pub mod shell;
mod trash;
mod volumes;
pub mod wallpaper;

pub use context::{
    build_context_menu, run_action, tab_context_menu, ContextAction, ContextItem, ContextKind,
};
pub use open::{open_default, open_with, open_with_dialog};
pub use shell::open_terminal;
pub use trash::{
    delete_permanently, empty_trash, list_trash, restore_from_trash, trash_label, trash_path,
};
pub use volumes::{free_space, human_bytes, this_pc_entries};