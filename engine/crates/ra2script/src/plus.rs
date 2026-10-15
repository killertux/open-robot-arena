//! The `plus` module: the engine's face to the scripts.
//!
//! `Scripts/Main.py` opens with `from plus import *`, and every arena and AI is
//! a subclass of a class defined here. The free functions fall into three
//! groups:
//!
//! * **Engine state** — `getLocation`, `getWeight`, `damage`, `force`,
//!   `rayTest`, `isDefeated`, the match clock. These go to the live match
//!   through [`crate::MatchApi`].
//! * **Scene description** — the camera and backdrop calls, the arena picker.
//! * **Presentation** — sounds and particles, which belong to M5. They are
//!   accepted, given handles where the scripts expect them, and recorded so a
//!   run reports exactly what it dropped rather than silently ignoring it.

use pyo3::prelude::*;
use pyo3::types::{PyTuple, PyType};

use glam::Vec3;

use crate::arena::{Arena, ArenaDetail, HingeController, ParticleEmitter, PrismaticController};

/// Register the module, if it is not already in `sys.modules`.
pub fn register(py: Python<'_>) -> PyResult<()> {
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    if modules.contains("plus")? {
        return Ok(());
    }
    let m = PyModule::new(py, "plus")?;
    m.add(
        "__doc__",
        "The Robot Arena 2 engine, as the game's own scripts see it.",
    )?;

    m.add_class::<Arena>()?;
    m.add_class::<ArenaDetail>()?;
    m.add_class::<PrismaticController>()?;
    m.add_class::<HingeController>()?;
    m.add_class::<ParticleEmitter>()?;
    m.add_class::<Ai>()?;

    m.add_function(wrap_pyfunction!(get_game_type, &m)?)?;
    m.add_function(wrap_pyfunction!(is_debug_mode, &m)?)?;
    m.add_function(wrap_pyfunction!(get_time_elapsed, &m)?)?;
    m.add_function(wrap_pyfunction!(is_match_over, &m)?)?;
    m.add_function(wrap_pyfunction!(is_match_paused, &m)?)?;
    m.add_function(wrap_pyfunction!(intro_complete, &m)?)?;
    m.add_function(wrap_pyfunction!(get_players, &m)?)?;
    m.add_function(wrap_pyfunction!(set_arena, &m)?)?;
    m.add_function(wrap_pyfunction!(enum_arena, &m)?)?;
    m.add_function(wrap_pyfunction!(get_location, &m)?)?;
    m.add_function(wrap_pyfunction!(get_weight, &m)?)?;
    m.add_function(wrap_pyfunction!(is_upside_down, &m)?)?;
    m.add_function(wrap_pyfunction!(damage, &m)?)?;
    m.add_function(wrap_pyfunction!(force, &m)?)?;
    m.add_function(wrap_pyfunction!(ray_test, &m)?)?;
    m.add_function(wrap_pyfunction!(zap, &m)?)?;
    m.add_function(wrap_pyfunction!(is_defeated, &m)?)?;
    m.add_function(wrap_pyfunction!(is_eliminated, &m)?)?;
    m.add_function(wrap_pyfunction!(is_human, &m)?)?;
    m.add_function(wrap_pyfunction!(eliminate_player, &m)?)?;

    m.add_function(wrap_pyfunction!(create_sound, &m)?)?;
    m.add_function(wrap_pyfunction!(play_sound, &m)?)?;
    m.add_function(wrap_pyfunction!(play_once, &m)?)?;
    m.add_function(wrap_pyfunction!(loop_sound, &m)?)?;
    m.add_function(wrap_pyfunction!(stop_sound, &m)?)?;
    m.add_function(wrap_pyfunction!(remove_sound, &m)?)?;
    m.add_function(wrap_pyfunction!(set_volume, &m)?)?;
    m.add_function(wrap_pyfunction!(fade_in_to_loop, &m)?)?;
    m.add_function(wrap_pyfunction!(fade_out_loop, &m)?)?;

    m.add_function(wrap_pyfunction!(set_back_color, &m)?)?;
    m.add_function(wrap_pyfunction!(set_camera_position, &m)?)?;
    m.add_function(wrap_pyfunction!(set_camera_rotation, &m)?)?;
    m.add_function(wrap_pyfunction!(set_camera_fov, &m)?)?;
    m.add_function(wrap_pyfunction!(set_camera_fov_range, &m)?)?;
    m.add_function(wrap_pyfunction!(animate_camera, &m)?)?;
    m.add_function(wrap_pyfunction!(fade_from_black, &m)?)?;

    m.add_function(wrap_pyfunction!(add_particle_emitter, &m)?)?;
    m.add_function(wrap_pyfunction!(add_window, &m)?)?;
    m.add_function(wrap_pyfunction!(add_text, &m)?)?;
    m.add_function(wrap_pyfunction!(add_points, &m)?)?;

    modules.set_item("plus", &m)?;
    Ok(())
}

fn defer(what: &str) {
    crate::state().defer(what);
}

// ---------------------------------------------------------------------------
// match state
// ---------------------------------------------------------------------------

#[pyfunction]
#[pyo3(name = "getGameType")]
fn get_game_type() -> String {
    crate::state().game_type.clone()
}

#[pyfunction]
#[pyo3(name = "isDebugMode")]
fn is_debug_mode() -> bool {
    std::env::var("RA2_DEBUG").is_ok()
}

#[pyfunction]
#[pyo3(name = "getTimeElapsed")]
fn get_time_elapsed() -> f32 {
    crate::state().time
}

#[pyfunction]
#[pyo3(name = "isMatchOver")]
fn is_match_over() -> bool {
    crate::state().over
}

#[pyfunction]
#[pyo3(name = "isMatchPaused")]
fn is_match_paused() -> bool {
    crate::state().paused
}

#[pyfunction]
#[pyo3(name = "introComplete")]
fn intro_complete() {}

#[pyfunction]
#[pyo3(name = "getPlayers")]
fn get_players() -> Vec<i32> {
    crate::state().players.clone()
}

#[pyfunction]
#[pyo3(name = "setArena")]
fn set_arena(py: Python<'_>, arena: Py<PyAny>) {
    crate::state().arena_object = Some(arena.clone_ref(py));
}

#[pyfunction]
#[pyo3(name = "enumArena")]
fn enum_arena(_detail: &Bound<'_, ArenaDetail>) -> bool {
    // The picker that consumes these is M5; the enumeration itself is real.
    defer("arena picker (M5)");
    true
}

fn with_match<R>(f: impl FnOnce(&mut dyn crate::MatchApi) -> R) -> Option<R> {
    let mut st = crate::state();
    let api = st.physics.as_mut()?;
    Some(f(api.as_mut()))
}

#[pyfunction]
#[pyo3(name = "getLocation")]
fn get_location(bot: i32) -> (f32, f32, f32) {
    match with_match(|m| m.location(bot)) {
        Some(p) => (p.x, p.y, p.z),
        None => (0.0, 0.0, 0.0),
    }
}

#[pyfunction]
#[pyo3(name = "getWeight")]
fn get_weight(bot: i32) -> f32 {
    with_match(|m| m.weight(bot)).unwrap_or(0.0)
}

#[pyfunction]
#[pyo3(name = "isUpsideDown")]
fn is_upside_down(bot: i32) -> bool {
    with_match(|m| m.upside_down(bot)).unwrap_or(false)
}

#[pyfunction]
#[pyo3(name = "damage")]
fn damage(bot: i32, component: i32, amount: f32, point: (f32, f32, f32)) {
    let p = Vec3::new(point.0, point.1, point.2);
    with_match(|m| m.damage(bot, component, amount, p));
}

#[pyfunction]
#[pyo3(name = "force")]
fn force(bot: i32, x: f32, y: f32, z: f32) {
    let v = Vec3::new(x, y, z);
    with_match(|m| m.force(bot, v));
}

#[pyfunction]
#[pyo3(name = "rayTest")]
fn ray_test(from: (f32, f32, f32), to: (f32, f32, f32)) -> (i32, i32, f32, f32, f32) {
    let a = Vec3::new(from.0, from.1, from.2);
    let b = Vec3::new(to.0, to.1, to.2);
    match with_match(|m| m.ray_test(a, b)) {
        Some((bot, comp, p)) => (bot, comp, p.x, p.y, p.z),
        None => (-1, -1, 0.0, 0.0, 0.0),
    }
}

#[pyfunction]
#[pyo3(name = "zap")]
fn zap(bot: i32, amount: f32, radius: f32) {
    let _ = (bot, amount, radius);
    defer("zap visual (M5)");
}

#[pyfunction]
#[pyo3(name = "isDefeated")]
fn is_defeated(bot: i32) -> bool {
    match with_match(|m| m.alive()) {
        Some(alive) => !alive.get(bot.max(0) as usize).copied().unwrap_or(false),
        None => false,
    }
}

#[pyfunction]
#[pyo3(name = "isEliminated")]
fn is_eliminated(bot: i32) -> bool {
    is_defeated(bot)
}

#[pyfunction]
#[pyo3(name = "isHuman")]
fn is_human(_bot: i32) -> bool {
    false
}

#[pyfunction]
#[pyo3(name = "eliminatePlayer")]
fn eliminate_player(bot: i32) {
    let _ = bot;
    defer("elimination flow (M5)");
}

// ---------------------------------------------------------------------------
// presentation (M5) — accepted, given handles, and reported
// ---------------------------------------------------------------------------

#[pyfunction]
#[pyo3(signature = (path, looping, location))]
#[pyo3(name = "createSound")]
fn create_sound(path: &str, looping: &Bound<'_, PyAny>, location: (f32, f32, f32)) -> u64 {
    let _ = (looping, location);
    let mut st = crate::state();
    let handle = st.next_handle;
    st.next_handle += 1;
    st.sounds.insert(handle, path.to_string());
    st.defer("audio (M5)");
    handle
}

#[pyfunction]
#[pyo3(name = "playSound")]
fn play_sound(_sound: u64) {
    defer("audio (M5)");
}

#[pyfunction]
#[pyo3(name = "playOnce")]
fn play_once(_sound: u64) {
    defer("audio (M5)");
}

#[pyfunction]
#[pyo3(name = "loopSound")]
fn loop_sound(_sound: u64) {
    defer("audio (M5)");
}

#[pyfunction]
#[pyo3(name = "stopSound")]
fn stop_sound(_sound: u64) {
    defer("audio (M5)");
}

#[pyfunction]
#[pyo3(name = "removeSound")]
fn remove_sound(_sound: u64) {
    defer("audio (M5)");
}

#[pyfunction]
#[pyo3(name = "setVolume")]
fn set_volume(_sound: u64, _a: f32, _b: f32) {
    defer("audio (M5)");
}

#[pyfunction]
#[pyo3(name = "fadeInToLoop")]
fn fade_in_to_loop(_sound: u64, _volume: f32, _ms: f32) {
    defer("audio (M5)");
}

#[pyfunction]
#[pyo3(name = "fadeOutLoop")]
fn fade_out_loop(_sound: u64, _ms: f32) {
    defer("audio (M5)");
}

#[pyfunction]
#[pyo3(name = "setBackColor")]
fn set_back_color(r: f32, g: f32, b: f32) {
    let _ = (r, g, b);
    defer("camera/backdrop (M5)");
}

#[pyfunction]
#[pyo3(name = "setCameraPosition")]
fn set_camera_position(x: f32, y: f32, z: f32) {
    let _ = (x, y, z);
    defer("camera/backdrop (M5)");
}

#[pyfunction]
#[pyo3(name = "setCameraRotation")]
fn set_camera_rotation(x: f32, y: f32) {
    let _ = (x, y);
    defer("camera/backdrop (M5)");
}

#[pyfunction]
#[pyo3(name = "setCameraFOV")]
fn set_camera_fov(fov: f32) {
    let _ = fov;
    defer("camera/backdrop (M5)");
}

#[pyfunction]
#[pyo3(name = "setCameraFOVRange")]
fn set_camera_fov_range(a: f32, b: f32, c: f32, d: f32) {
    let _ = (a, b, c, d);
    defer("camera/backdrop (M5)");
}

/// `animateCamera(from_pos, from_rot, from_fov, to_pos, to_rot, to_fov, ?,
/// seconds)` — the intro fly-throughs and the hazard showcase shots.
#[pyfunction]
#[pyo3(signature = (*_args))]
#[pyo3(name = "animateCamera")]
fn animate_camera(_args: &Bound<'_, PyTuple>) {
    defer("camera/backdrop (M5)");
}

#[pyfunction]
#[pyo3(name = "fadeFromBlack")]
fn fade_from_black(_seconds: f32) {
    defer("camera/backdrop (M5)");
}

#[pyfunction]
#[pyo3(signature = (location, velocity, variance))]
#[pyo3(name = "AddParticleEmitter")]
fn add_particle_emitter(
    location: (f32, f32, f32),
    velocity: (f32, f32, f32),
    variance: (f32, f32, f32),
) -> ParticleEmitter {
    defer("particles (M5)");
    ParticleEmitter {
        location: [location.0, location.1, location.2],
        velocity: [velocity.0, velocity.1, velocity.2],
        variance: [variance.0, variance.1, variance.2],
        emitting: false,
    }
}

#[pyfunction]
#[pyo3(signature = (*_args))]
#[pyo3(name = "addWindow")]
fn add_window(_args: &Bound<'_, PyTuple>) -> i32 {
    defer("Gooey UI (M5)");
    0
}

#[pyfunction]
#[pyo3(signature = (*_args))]
#[pyo3(name = "addText")]
fn add_text(_args: &Bound<'_, PyTuple>) -> i32 {
    defer("Gooey UI (M5)");
    0
}

#[pyfunction]
#[pyo3(signature = (*_args))]
#[pyo3(name = "addPoints")]
fn add_points(_args: &Bound<'_, PyTuple>) -> i32 {
    defer("Gooey UI (M5)");
    0
}

// ---------------------------------------------------------------------------
// plus.AI
// ---------------------------------------------------------------------------

/// `plus.AI` — the C++ side of every AI script (`AI.SuperAI` subclasses it).
///
/// The queries the strategies drive themselves with (`GetLocation`, `Input`,
/// `GetEnemies`, ...) are the next slice; the shape is here so the shipped
/// `AI/` package imports and subclasses exactly as it does in the game.
#[pyclass(subclass, name = "AI", module = "plus")]
#[derive(Default)]
pub struct Ai {
    /// Which robot this brain drives.
    pub bot: i32,
    /// Whether `Activate(True)` has been seen.
    pub active: bool,
}

#[pymethods]
impl Ai {
    #[new]
    fn new() -> Self {
        Ai::default()
    }

    fn __init__(&self) {}

    fn Activate(&mut self, active: bool) -> bool {
        self.active = active;
        true
    }

    fn Tick(&self) -> bool {
        true
    }

    fn ImmobilityWarning(&self, _id: i32, _on: bool) {}

    fn GetID(&self) -> i32 {
        self.bot
    }

    fn GetNumComponents(&self) -> i32 {
        0
    }

    fn GetComponentType(&self, _i: i32) -> String {
        String::new()
    }

    fn GetComponentHealth(&self, _i: i32) -> f32 {
        0.0
    }

    fn ResetComponents(&self) {}

    fn GetZoneContents(&self, _zone: &str) -> Vec<(i32, bool)> {
        Vec::new()
    }

    fn GetLastDamageDone(&self) -> (i32, f32, f32) {
        (0, 0.0, 0.0)
    }

    fn GetLastDamageReceived(&self) -> (i32, f32, f32) {
        (0, 0.0, 0.0)
    }
}

/// `plus.AI` subclasses are declared by the scripts; keep the type name usable
/// for `isinstance` checks from Rust when the AI slice lands.
#[allow(dead_code)]
pub fn is_ai(ty: &Bound<'_, PyType>) -> bool {
    ty.name().map(|n| n == "AI").unwrap_or(false)
}
