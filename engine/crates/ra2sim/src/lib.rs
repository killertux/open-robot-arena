//! `ra2sim` — Robot Arena 2 physics simulation (M2 sandbox).
//!
//! Talks to a swappable [`backend::PhysicsBackend`] (Jolt today) and builds
//! bodies from the game's own `.gmf`/`.bot`/arena-script data.

pub mod backend;
pub mod combat;
pub mod duel;
pub mod jolt;
pub mod montage;
pub mod rig;
pub mod robot;
pub mod scene;
pub mod scriptai;
pub mod sim;

pub use backend::{
    BackendError, BodyDesc, BodyHandle, BodyState, MotionType, PhysicsBackend, RayHit, ShapeDesc,
    StepInfo,
};
pub use combat::{CombatConfig, Impact};
pub use duel::{BotSetup, Debris, Duel, DuelCamera};
pub use jolt::JoltBackend;
pub use rig::{Component, MotorStats, RobotRig, Role, WeaponStats};
pub use robot::Robot;
pub use scene::{load_arena, ArenaScene, USER_ARENA, USER_BOT};
pub use sim::{CameraMode, DriveInput, DriveScript, DriveTuning, PropEntity, Sandbox, BOT_FORWARD};
