//! OS-specific integration. Dependencies for each target are gated in Cargo.toml
//! (`libc` on unix, `winreg` on windows) so the other OS build excludes them.

pub mod assoc;
pub mod attrs;
mod context;
pub mod elevate;
pub mod folders;
pub mod network;
mod open;
pub mod shell;
mod trash;
pub mod volumes;
pub mod wallpaper;

pub use context::{
    build_context_menu, detail_column_menu, run_action, tab_context_menu, ContextAction,
    ContextItem, ContextKind,
};
pub use open::{open_default, open_with, open_with_dialog, set_default_app};
pub use shell::open_terminal;
pub use trash::{
    delete_permanently, empty_trash, list_trash, restore_from_trash, trash_label, trash_path,
};
pub use volumes::{
    eject_volume, format_volume, free_space, human_bytes, list_volumes, mount_volume,
    set_volume_label, this_pc_entries, unmount_volume, Volume, VolumeKind,
};