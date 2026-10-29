//! The game shell's entry point, as a library so its routes are testable.
//!
//! * [`game`] finds an installation and its shipped screens.
//! * [`nav`] reads what a control's `Notes` say happens next, and holds the small
//!   table of purpose-named links (every entry checked against the data).
//! * [`script`] is the headless input format.
//! * [`shell`] is the live screen: a click is hit-tested and the data decides.
//!
//! The binary in `src/main.rs` is a thin runner over these.

pub mod bindings;
pub mod game;
pub mod nav;
pub mod script;
pub mod shell;

pub use shell::{LobbyPick, PanelButton, PanelRequest, Pick, Shell, Shot, Step, PREMATCH};
