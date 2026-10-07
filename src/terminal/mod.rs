//! The built-in terminal pane: a real PTY with alacritty_terminal's emulator,
//! drawn with GPUI. Every command Packrat runs goes through here, so you see
//! exactly what runs and can answer sudo and pacman prompts.

mod colors;
mod keys;
mod session;
mod view;

pub use session::{Command, Status};
pub use view::{PaneEvent, TerminalView, init};
