//! The scoring events of a network match (docs/38 §6.1).
//!
//! The original's host prices every hit, awards the points and tells the
//! joiners with `giBotDamageMessage` (`0x135`), `giAddPointsMessage` (`0x139`),
//! `giBotImmobileMessage` (`0x13b`) and `giPlayerOutMessage` (`0x13a`); a
//! joiner shows what it is told. The simulation reports them as [`NetEvent`]s
//! (host: [`crate::duel::Duel::net_events`]) and applies them on the other end
//! ([`crate::r#match::Match::apply_net_event`]); `ra2-app::net_play` turns them
//! into messages and back, so this crate stays free of the wire format.

use glam::Vec3;

pub use ra2script::NetRole;

/// `FUN_00418040`'s reason words (`giPlayerOutMessage`'s second word).
pub mod out {
    /// A destroyed chassis (the "out" banner, `FUN_004d53e0(0x14, 1)`).
    pub const DESTROYED: u32 = 1;
    /// Immobilised for 20 s (`FUN_00412fc0` calls `FUN_00418040(robot, 2)`).
    pub const IMMOBILIZED: u32 = 2;
    /// Eliminated by the arena script (`plus.eliminatePlayer`); ours, the
    /// reason word the original's third branch takes.
    pub const ELIMINATED: u32 = 3;
}

/// One thing the host decided, for the joiners to show.
#[derive(Clone, Debug, PartialEq)]
pub enum NetEvent {
    /// A hit that reached a component (`FUN_00417330` / `FUN_004148f0` ->
    /// `giBotDamageMessage`). `attacker` is (robot, its component) or `None`
    /// for a hazard or flame.
    Hit {
        attacker: Option<(usize, usize)>,
        victim: usize,
        component: usize,
        damage: f32,
        point: Vec3,
        normal: Vec3,
    },
    /// Points for a player (`FUN_004177e0` -> `giAddPointsMessage`). `script`
    /// marks an arena script's `plus.addPoints` (an integer add) as against a
    /// damage award (the truncated float add).
    Points {
        player: usize,
        points: f32,
        script: bool,
    },
    /// The immobility warning on or off (`giBotImmobileMessage`): where the
    /// robot sits and the seconds it has not moved (the counter's source).
    Immobile {
        robot: usize,
        on: bool,
        at: [f32; 3],
        seconds: f32,
    },
    /// A robot is out (`giPlayerOutMessage`), with an [`out`] reason.
    Out { robot: usize, reason: u32 },
}
