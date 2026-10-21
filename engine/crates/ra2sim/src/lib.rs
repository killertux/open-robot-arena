//! `ra2sim` — Robot Arena 2 physics simulation (M2 sandbox).
//!
//! Talks to a swappable [`backend::PhysicsBackend`] (Jolt today) and builds
//! bodies from the game's own `.gmf`/`.bot`/arena-script data.

pub mod backend;
pub mod camera;
pub mod career;
pub mod combat;
pub mod controls;
pub mod drive;
pub mod duel;
pub mod energy;
pub mod flame;
pub mod havok17;
pub mod hazards;
pub mod immobility;
pub mod jolt;
pub mod massprops;
pub mod r#match;
pub mod montage;
pub mod motor;
pub mod rig;
pub mod robot;
pub mod scene;
pub mod scriptai;
pub mod sim;

pub use backend::{
    BackendError, BodyDesc, BodyHandle, BodyState, MotionType, PhysicsBackend, RayHit, ShapeDesc,
    StepInfo,
};
pub use career::{
    Career, CareerPolicy, EventRecord, MatchAward, Prize, Repair, SeasonPhase, SIDECAR_NAME,
};
pub use combat::{CombatConfig, Impact};
pub use duel::{BotSetup, Debris, Duel, DuelCamera};
pub use jolt::JoltBackend;
pub use r#match::{
    countdown_number, event_num_bots, ComponentDamage, Controller, GameType, Match, MatchConfig,
    MatchReport, MatchState, RobotEntry, RobotReport, WinReason, COUNTDOWN_END_SECONDS,
    COUNTDOWN_SECONDS, DEFAULT_MATCH_MINUTES, DEFAULT_MATCH_SECONDS, DEFAULT_RESULTS_DELAY,
    DEFAULT_STEP,
};
pub use rig::{Component, MotorStats, RobotRig, Role, WeaponStats};
pub use robot::Robot;
pub use scene::{load_arena, ArenaScene, USER_ARENA, USER_BOT};
pub use sim::{CameraMode, DriveInput, DriveScript, DriveTuning, PropEntity, Sandbox, BOT_FORWARD};
