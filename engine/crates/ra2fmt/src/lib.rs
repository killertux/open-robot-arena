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
pub mod event;
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

/// Resolve a path the game's own data writes under a root, case-insensitively.
///
/// The data writes Windows paths — `\common\bottomnav.gib`, `.\botlab\
/// blueprint.bmp`, `UI\help\helpframe.gib` — and the shipped files are named
/// lower case. Each component is tried as written and then matched against the
/// directory's entries ignoring case, so a `Proxy`'s `Filename` finds its file
/// on a case-sensitive filesystem.
pub fn find_ci(root: &std::path::Path, rel: &str) -> Option<std::path::PathBuf> {
    let rel = rel.replace('\\', "/");
    let mut cur = root.to_path_buf();
    for part in rel.split('/').filter(|p| !p.is_empty() && *p != ".") {
        if part == ".." {
            cur.pop();
            continue;
        }
        let direct = cur.join(part);
        if direct.exists() {
            cur = direct;
            continue;
        }
        let want = part.to_ascii_lowercase();
        let found = std::fs::read_dir(&cur)
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().to_ascii_lowercase() == want)?;
        cur = found.path();
    }
    cur.exists().then_some(cur)
}

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
