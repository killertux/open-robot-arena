//! Parsers for Robot Arena 2 data formats.
//!
//! Currently implemented:
//! - [`gmi`] — `.gmf` binary meshes (GMI): geometry, materials, attachment
//!   points and embedded Havok physics metadata.
//! - [`gib`] — `.gib` GUI layouts: the shell's widget trees, as the game's own
//!   editor wrote them.
//!
//! Planned: `.bot` robots, `.cfz` (CFL3) archives, `*_txt.gmf` (GMA ASCII)
//! meshes.

pub mod arena;
pub mod armor;
pub mod bot;
pub mod cfl;
pub mod component;
pub mod gib;
pub mod giff;
pub mod gmi;

pub use arena::ArenaScript;
pub use armor::ArmourDef;
pub use bot::{Armour, Bot, BotMesh, Chassis, Component};
pub use cfl::Cfl;
pub use component::ComponentDef;
pub use gib::{Axes, Block, Field, Gib, Length, Widget, WidgetKind};
pub use giff::Font;
pub use gmi::{GmiScene, Material, Mesh, RigidBody, SceneObject, SimulationObject, Texture};

/// Error type shared by all format parsers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Ran out of bytes while reading.
    UnexpectedEof { at: usize, wanted: usize },
    /// A magic/header check failed.
    BadMagic(String),
    /// A structural assertion failed (node id/flags, counts, ...).
    Structure(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::UnexpectedEof { at, wanted } => {
                write!(
                    f,
                    "unexpected end of data at offset {at} (wanted {wanted} bytes)"
                )
            }
            Error::BadMagic(m) => write!(f, "bad magic/header: {m}"),
            Error::Structure(m) => write!(f, "structure error: {m}"),
        }
    }
}

impl std::error::Error for Error {}
