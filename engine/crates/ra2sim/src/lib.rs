//! `ra2sim` — Robot Arena 2 physics simulation (M2 sandbox).
//!
//! Talks to a swappable [`backend::PhysicsBackend`] (Jolt today) and builds
//! bodies from the game's own `.gmf`/`.bot`/arena-script data.

pub mod backend;
pub mod jolt;
pub mod scene;
pub mod sim;

pub use backend::{
    BackendError, BodyDesc, BodyHandle, BodyState, MotionType, PhysicsBackend, RayHit, ShapeDesc,
    StepInfo,
};
pub use jolt::JoltBackend;
pub use scene::{load_arena, load_bot, ArenaScene, BotRig, USER_ARENA, USER_BOT};
pub use sim::{CameraMode, DriveInput, DriveScript, DriveTuning, Sandbox, BOT_FORWARD};
