//! Parsers for Robot Arena 2 data formats.
//!
//! Currently implemented:
//! - [`gmi`] — `.gmf` binary meshes (GMI): geometry, materials, attachment
//!   points and embedded Havok physics metadata.
//!
//! Planned: `.bot` robots, `.cfz` (CFL3) archives, `.gib` GUI layouts,
//! `*_txt.gmf` (GMA ASCII) meshes.

pub mod bot;
pub mod component;
pub mod gmi;

pub use bot::{Bot, BotMesh, Chassis, Component};
pub use component::ComponentDef;
pub use gmi::{GmiScene, Material, Mesh, RigidBody, SceneObject, Texture};

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
                write!(f, "unexpected end of data at offset {at} (wanted {wanted} bytes)")
            }
            Error::BadMagic(m) => write!(f, "bad magic/header: {m}"),
            Error::Structure(m) => write!(f, "structure error: {m}"),
        }
    }
}

impl std::error::Error for Error {}
