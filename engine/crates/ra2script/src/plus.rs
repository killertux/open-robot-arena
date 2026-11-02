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
//! * **Sound** — `createSound`, `playSound`, `loopSound`, `stopSound` and
//!   `removeSound` are a real handle table and an ordered queue
//!   ([`crate::SoundQueue`]); a caller drains it into `ra2-audio`.
//! * **Particles** — `AddParticleEmitter` registers a flame emitter in
//!   [`crate::State::emitters`] (`SetEmitting` flips it); the match's particle
//!   system draws it.
//! * **Presentation still deferred** — the camera calls are recorded so a run
//!   reports exactly what it dropped rather than silently ignoring it.

use std::collections::BTreeMap;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

use glam::{Quat, Vec3};

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
    m.add_class::<crate::ui::Window>()?;
    m.add_class::<crate::ui::Text>()?;

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
    m.add_function(wrap_pyfunction!(get_points, &m)?)?;

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
fn intro_complete() {
    crate::state().intro_running = false;
}

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

fn with_bot<R>(bot: i32, f: impl FnOnce(&crate::BotSnapshot) -> R) -> Option<R> {
    let st = crate::state();
    st.bot(bot).map(f)
}

#[pyfunction]
#[pyo3(name = "getLocation")]
fn get_location(bot: i32) -> (f32, f32, f32) {
    with_bot(bot, |b| (b.location.x, b.location.y, b.location.z)).unwrap_or((0.0, 0.0, 0.0))
}

#[pyfunction]
#[pyo3(name = "getWeight")]
fn get_weight(bot: i32) -> f32 {
    with_bot(bot, |b| b.mass).unwrap_or(0.0)
}

#[pyfunction]
#[pyo3(name = "isUpsideDown")]
fn is_upside_down(bot: i32) -> bool {
    with_bot(bot, |b| b.upside_down).unwrap_or(false)
}

#[pyfunction]
#[pyo3(name = "damage")]
fn damage(bot: i32, component: i32, amount: f32, point: (f32, f32, f32)) {
    crate::state().requests.push(crate::Request::Damage {
        bot,
        component,
        amount,
        point: Vec3::new(point.0, point.1, point.2),
    });
}

/// `force(bot, x, y, z)` — a **force** in N on the robot's body, for one step.
///
/// Recovered from the binary rather than guessed (F13). The engine adds the
/// vector into the body's force accumulator (`FUN_0045e940`, via `FUN_004b0950`)
/// — and the *joints'* `ApplyForce` calls that very same function with `axis *
/// power`, so `plus.force` and a hazard's `ApplyForce` are one operation. A force
/// integrated over one step is `force * dt` of impulse, which is what the
/// simulator applies.
///
/// It is the reading that makes `Hazards.Electricity` sane: its single caller
/// lifts a zapped robot by `115 * getWeight(bot)`, i.e. 115 N per kilogram, an
/// acceleration of ~11.7 g held for a step — about 1.9 m/s of kick per zap.
#[pyfunction]
#[pyo3(name = "force")]
fn force(bot: i32, x: f32, y: f32, z: f32) {
    crate::state().requests.push(crate::Request::Force {
        bot,
        momentum: Vec3::new(x, y, z),
    });
}

/// `rayTest(from, to)` -> `(bot, component, x, y, z)`, or `(-1, -1, ...)`.
///
/// The flame hazards are the callers: `Hazards.Flame.Tick` fires a ray down the
/// jet and damages the first component it reaches. It is answered against the
/// components' own oriented boxes rather than the spheres around them — a part
/// is rarely a ball, and a flame that misses a spinner's blades by half a metre
/// should not set them alight.
#[pyfunction]
#[pyo3(name = "rayTest")]
fn ray_test(from: (f32, f32, f32), to: (f32, f32, f32)) -> (i32, i32, f32, f32, f32) {
    let a = Vec3::new(from.0, from.1, from.2);
    let b = Vec3::new(to.0, to.1, to.2);
    let d = b - a;
    let len = d.length();
    if len <= 1e-6 {
        return (-1, -1, a.x, a.y, a.z);
    }
    let u = d / len;
    let st = crate::state();
    let mut best = f32::INFINITY;
    let mut hit = (-1, -1);
    for (bi, bot) in st.bots.iter().enumerate() {
        if bot.boxes.is_empty() {
            // No box for this part: the sphere around it, as before.
            for (ci, (c, r)) in bot.spheres.iter().enumerate() {
                if let Some(t) = ray_sphere(a, u, len, *c, *r) {
                    if t < best {
                        best = t;
                        hit = (bi as i32, ci as i32);
                    }
                }
            }
            continue;
        }
        for (ci, (centre, half, rotation)) in bot.boxes.iter().enumerate() {
            if let Some(t) = ray_box(a, u, len, *centre, *half, *rotation) {
                if t < best {
                    best = t;
                    hit = (bi as i32, ci as i32);
                }
            }
        }
    }
    if best.is_finite() {
        let p = a + u * best;
        (hit.0, hit.1, p.x, p.y, p.z)
    } else {
        (-1, -1, a.x, a.y, a.z)
    }
}

/// How far along `u` a ray enters a sphere, if it does.
fn ray_sphere(origin: Vec3, u: Vec3, len: f32, centre: Vec3, radius: f32) -> Option<f32> {
    let oc = origin - centre;
    let bq = oc.dot(u);
    let disc = bq * bq - (oc.dot(oc) - radius * radius);
    if disc < 0.0 {
        return None;
    }
    let t = -bq - disc.sqrt();
    if t > 0.0 && t <= len {
        Some(t)
    } else {
        None
    }
}

/// How far along `u` a ray enters an oriented box, if it does.
///
/// The box is a component's own extents carried by its world rotation, so the
/// ray is moved into the box's frame and clipped against its three slabs. Where
/// the ray starts inside, the entry distance is zero.
fn ray_box(
    origin: Vec3,
    u: Vec3,
    len: f32,
    centre: Vec3,
    half: Vec3,
    rotation: Quat,
) -> Option<f32> {
    let inv = rotation.conjugate();
    let o = inv * (origin - centre);
    let d = inv * u;
    let mut enter = 0.0f32;
    let mut exit = len;
    for axis in 0..3 {
        let (o, d, h) = (o[axis], d[axis], half[axis]);
        if d.abs() < 1e-9 {
            if o.abs() > h {
                return None;
            }
            continue;
        }
        let t1 = (-h - o) / d;
        let t2 = (h - o) / d;
        let (near, far) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
        enter = enter.max(near);
        exit = exit.min(far);
        if enter > exit {
            return None;
        }
    }
    Some(enter)
}

/// `plus.zap(bot, amount, radius)` — the electric hazard's shock.
///
/// The original's zap is a presentation event around the robot (`Hazards.
/// Electricity` then applies its own damage and lift in `Tick`). The exact
/// damage the zap itself deals is **not established**, so this records the
/// observable effect (`Robot::zapped_until`) and leaves the visual to M5; the
/// shipped hazard's `plus.damage` is what actually hurts the robot.
#[pyfunction]
#[pyo3(name = "zap")]
fn zap(bot: i32, amount: f32, radius: f32) {
    crate::state().requests.push(crate::Request::Zap {
        bot,
        amount,
        radius,
    });
    defer("zap visual (M5)");
}

/// `isDefeated` — the original's robot state is *not* 0 (`0x4b9ca0`).
///
/// A robot that fell off a tabletop is state 3, which is also non-zero, so it
/// is defeated as well as eliminated.
#[pyfunction]
#[pyo3(name = "isDefeated")]
fn is_defeated(bot: i32) -> bool {
    with_bot(bot, |b| !b.alive || b.eliminated).unwrap_or(false)
}

/// `isEliminated` — exactly the original's state 3 (`0x4b9c70`), not the
/// broader "defeated". The tabletop arenas check this before eliminating a
/// robot that has already fallen out.
#[pyfunction]
#[pyo3(name = "isEliminated")]
fn is_eliminated(bot: i32) -> bool {
    with_bot(bot, |b| b.eliminated).unwrap_or(false)
}

#[pyfunction]
#[pyo3(name = "isHuman")]
fn is_human(_bot: i32) -> bool {
    false
}

#[pyfunction]
#[pyo3(name = "eliminatePlayer")]
fn eliminate_player(bot: i32) {
    crate::state()
        .requests
        .push(crate::Request::Eliminate { bot });
}

// ---------------------------------------------------------------------------
// presentation (M5) — accepted, given handles, and reported
// ---------------------------------------------------------------------------

#[pyfunction]
#[pyo3(signature = (path, looping, location))]
#[pyo3(name = "createSound")]
fn create_sound(
    path: &str,
    looping: &Bound<'_, PyAny>,
    location: (f32, f32, f32),
) -> PyResult<u64> {
    // The scripts pass both booleans and `1` for the loop flag; both mean the
    // same thing to the engine.
    let looping = looping.is_truthy()?;
    let mut st = crate::state();
    Ok(st
        .sounds
        .create(path, looping, [location.0, location.1, location.2])
        .0)
}

#[pyfunction]
#[pyo3(name = "playSound")]
fn play_sound(sound: u64) -> PyResult<()> {
    crate::state()
        .sounds
        .play(crate::SoundId(sound))
        .map_err(PyValueError::new_err)
}

#[pyfunction]
#[pyo3(name = "playOnce")]
fn play_once(sound: u64) -> PyResult<()> {
    crate::state()
        .sounds
        .play_once(crate::SoundId(sound))
        .map_err(PyValueError::new_err)
}

#[pyfunction]
#[pyo3(name = "loopSound")]
fn loop_sound(sound: u64) -> PyResult<()> {
    crate::state()
        .sounds
        .loop_sound(crate::SoundId(sound))
        .map_err(PyValueError::new_err)
}

/// `plus.stopSound(handle)` — stop it, keeping the handle for a later play.
///
/// A script's `__del__` may run after the match has already removed the handle
/// (`Arenas.SuperArena.__del__` stops and removes its end music), so stopping an
/// unknown handle is a named warning rather than an exception.
#[pyfunction]
#[pyo3(name = "stopSound")]
fn stop_sound(sound: u64) -> PyResult<()> {
    let id = crate::SoundId(sound);
    let mut st = crate::state();
    if !st.sounds.stop(id) {
        st.log.push(format!(
            "plus.stopSound: sound {id} is already stopped or removed"
        ));
    }
    Ok(())
}

/// `plus.removeSound(handle)` — free it, stopping it if it is sounding.
///
/// Tolerated when the handle is already gone: the shipped arenas remove sounds
/// from `__del__`, after the match has torn the arena down and often after the
/// same handle was removed in `DoIntro`. The warning names the handle.
#[pyfunction]
#[pyo3(name = "removeSound")]
fn remove_sound(sound: u64) -> PyResult<()> {
    let id = crate::SoundId(sound);
    let mut st = crate::state();
    if !st.sounds.remove(id) {
        st.log
            .push(format!("plus.removeSound: sound {id} was already removed"));
    }
    Ok(())
}

/// `plus.setVolume(handle, volume, ?)` — the handle's level in DirectSound
/// hundredths of a decibel (`0` is unity, `-10000` silence). Every shipped call
/// is `(handle, 0, 0)`; the second integer is a time in milliseconds in the
/// same two-integer binding as `fadeInToLoop`, so it is accepted and unused
/// (a level of 0 needs no ramp).
#[pyfunction]
#[pyo3(name = "setVolume")]
fn set_volume(sound: u64, volume: f32, _ms: f32) -> PyResult<()> {
    crate::state()
        .sounds
        .set_volume(crate::SoundId(sound), volume.round() as i32)
        .map_err(PyValueError::new_err)
}

/// `plus.fadeInToLoop(handle, volume, ms)` — start the sound looping from
/// silence and ramp it to `volume` (hundredths of a decibel) over `ms`.
#[pyfunction]
#[pyo3(name = "fadeInToLoop")]
fn fade_in_to_loop(sound: u64, volume: f32, ms: f32) -> PyResult<()> {
    crate::state()
        .sounds
        .fade_in_to_loop(crate::SoundId(sound), volume.round() as i32, ms)
        .map_err(PyValueError::new_err)
}

/// `plus.fadeOutLoop(handle, ms)` — ramp to silence over `ms`, then stop.
#[pyfunction]
#[pyo3(name = "fadeOutLoop")]
fn fade_out_loop(sound: u64, ms: f32) -> PyResult<()> {
    // Like `stopSound`: an arena's teardown may fade a handle that is gone.
    let mut st = crate::state();
    if let Err(message) = st.sounds.fade_out_loop(crate::SoundId(sound), ms) {
        st.log.push(message);
    }
    Ok(())
}

#[pyfunction]
#[pyo3(name = "setBackColor")]
fn set_back_color(r: f32, g: f32, b: f32) {
    crate::state().back_color = Some([r, g, b].map(|c| c.clamp(0.0, 1.0)));
}

#[pyfunction]
#[pyo3(name = "setCameraPosition")]
fn set_camera_position(x: f32, y: f32, z: f32) {
    crate::state()
        .camera_commands
        .push(crate::camera::CameraCommand::Position([x, y, z]));
}

#[pyfunction]
#[pyo3(name = "setCameraRotation")]
fn set_camera_rotation(pitch: f32, yaw: f32) {
    crate::state()
        .camera_commands
        .push(crate::camera::CameraCommand::Rotation([pitch, yaw]));
}

#[pyfunction]
#[pyo3(name = "setCameraFOV")]
fn set_camera_fov(fov: f32) {
    crate::state()
        .camera_commands
        .push(crate::camera::CameraCommand::Fov(fov));
}

#[pyfunction]
#[pyo3(name = "setCameraFOVRange")]
fn set_camera_fov_range(min_distance: f32, max_distance: f32, fov_near: f32, fov_far: f32) {
    crate::state()
        .camera_commands
        .push(crate::camera::CameraCommand::FovRange([
            min_distance,
            max_distance,
            fov_near,
            fov_far,
        ]));
}

/// `animateCamera(pos0, rot0, fov0, pos1, rot1, fov1, t0, t1)` — the intro
/// fly-throughs and the hazard showcase shots. See [`crate::camera`].
#[pyfunction]
#[pyo3(signature = (*args))]
#[pyo3(name = "animateCamera")]
fn animate_camera(args: &Bound<'_, PyTuple>) -> PyResult<()> {
    let animation = crate::camera::parse_animation(args)?;
    crate::state()
        .camera_commands
        .push(crate::camera::CameraCommand::Animate(animation));
    Ok(())
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
    let mut st = crate::state();
    st.emitters.push(crate::arena::EmitterState {
        location: [location.0, location.1, location.2],
        velocity: [velocity.0, velocity.1, velocity.2],
        variance: [variance.0, variance.1, variance.2],
        emitting: false,
    });
    ParticleEmitter {
        id: st.emitters.len() - 1,
    }
}

/// `plus.addWindow(name, x, y, w, h)` — a window a script is building.
///
/// The shipped `Scripts/Gooey.py` wraps this, and every AI's debug window comes
/// through it. What it returns is a handle onto a real
/// [`ra2fmt::gib::Widget`] tree — see [`crate::ui`] — so a script's window is
/// placed and drawn by the same code a `.gib` screen is.
#[pyfunction]
#[pyo3(name = "addWindow")]
fn add_window(
    py: Python<'_>,
    name: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> PyResult<Py<crate::ui::Window>> {
    let id = crate::ui::add_window(name, x, y, w, h);
    Py::new(py, crate::ui::Window { id })
}

/// `plus.addText(window, x, y, w, h)` — one line inside that window. The name
/// is the wrapper's, not the engine's: the shipped `Gooey.Plain` keeps it as the
/// key of its own dictionary and passes only the geometry on.
#[pyfunction]
#[pyo3(name = "addText")]
fn add_text(
    py: Python<'_>,
    window: &Bound<'_, PyAny>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> PyResult<Py<crate::ui::Text>> {
    let parent = window
        .extract::<PyRef<'_, crate::ui::Window>>()
        .map(|w| w.id)
        .unwrap_or(0);
    let id = crate::ui::add_text(parent, x, y, w, h);
    Py::new(py, crate::ui::Text { id })
}

/// `plus.addPoints(player, amount)` — the robot's `+0x20` score.
///
/// Recovered from the binary: the bound function (`0x4b9d80`) resolves the
/// player and calls `FUN_004177e0` → `FUN_004178a0`, whose write is
/// `fild [robot+0x20]` / `fadd amount` (`0x417918`–`0x417928`). It therefore
/// **adds**; it is not a setter. `king.py:44` is the only shipped caller.
#[pyfunction]
#[pyo3(name = "addPoints")]
fn add_points(bot: i32, amount: i32) {
    crate::add_points(bot, amount);
}

/// `plus.getPoints(player)` — the getter behind the score (`0x494f50`).
#[pyfunction]
#[pyo3(name = "getPoints")]
fn get_points(bot: i32) -> i32 {
    crate::points(bot)
}

/// One `Input(name, slot, value)` call a brain made: the control's name, its
/// binding slot and the value.
pub type InputEvent = (String, i32, f32);

/// Take the `Input` calls a brain has made since the last call, in order.
///
/// The original delivers every call to the matching controls as an event (a
/// non-zero value is a press, zero a release; `0x4012f0` -> `0x4dd5f0` ->
/// `0x508ce0`), so a burst device fires on each press even when the level does
/// not change. The levels stay available to the script through
/// `GetInputStatus`. The wiring (which wires listen to `Forward`, `LeftRight`,
/// `Spin`, `Fire!` ...) is the robot's own and is applied by the simulator, which
/// keeps the `LeftRight` slots 0 and 1 apart instead of collapsing them.
pub fn take_input_events(brain: &Py<PyAny>, py: Python<'_>) -> Option<Vec<InputEvent>> {
    let obj = brain.bind(py);
    let ai = obj.downcast::<Ai>().ok()?;
    let mut ai = ai.borrow_mut();
    Some(std::mem::take(&mut ai.events))
}

/// The zones a brain asked to be told about.
pub fn registered_zones(brain: &Py<PyAny>, py: Python<'_>) -> Vec<(String, i32)> {
    let obj = brain.bind(py);
    match obj.downcast::<Ai>() {
        Ok(ai) => ai.borrow().zones.clone(),
        Err(_) => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// plus.AI
// ---------------------------------------------------------------------------

/// `plus.AI` — the C++ side of every AI script (`AI.SuperAI` subclasses it).
///
/// The strategies ask the engine where they are, which way they are facing, how
/// fast they are going, who the enemies are and what they last hit; they tell it
/// what to do through `Input`. Every question is answered from the robot
/// snapshot the simulator publishes, and every action is either an input (held
/// here, read back by the simulator) or a queued request.
#[pyclass(subclass, name = "AI", module = "plus")]
#[derive(Default)]
pub struct Ai {
    /// Which robot this brain drives; the engine binds it after construction.
    pub bot: i32,
    /// Whether `Activate(True)` has been seen.
    pub active: bool,
    /// The control board's inputs, `(name, index) -> value`.
    pub inputs: BTreeMap<(String, i32), f32>,
    /// Every `Input` call since the simulator last took them.
    pub events: Vec<InputEvent>,
    /// SmartZones this brain asked to be told about, `(name, id)`.
    pub zones: Vec<(String, i32)>,
    /// `fNoseOffset`.
    pub nose: f32,
}

impl Ai {
    /// The heading whose `(sin, cos)` is the robot's facing — the convention the
    /// scripts use when they build a direction with `vector3(angle)`.
    fn heading_of(dir: Vec3) -> f32 {
        f32::atan2(dir.x, dir.z)
    }

    fn own_heading(&self, reverse: bool) -> f32 {
        with_bot(self.bot, |b| {
            let d = if reverse { -b.direction } else { b.direction };
            Ai::heading_of(d)
        })
        .unwrap_or(0.0)
    }

    fn heading_to(&self, target: Vec3, reverse: bool) -> f32 {
        let own = self.own_heading(reverse);
        let st = crate::state();
        let Some(b) = st.bot(self.bot) else {
            return 0.0;
        };
        let d = target - b.location;
        let mut e = Ai::heading_of(d) - own;
        while e > std::f32::consts::PI {
            e -= 2.0 * std::f32::consts::PI;
        }
        while e < -std::f32::consts::PI {
            e += 2.0 * std::f32::consts::PI;
        }
        e
    }
}

/// `RA2_AI_DIST3D=1`: `GetDistanceTo` / `GetDistanceToID` with the `y` difference (the old length).
fn ai_dist_3d() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("RA2_AI_DIST3D").is_ok_and(|v| v != "0"))
}

/// `0x4019c0`: the length of the difference with both `y` words zeroed.
fn ai_distance(from: Vec3, to: Vec3, three_d: bool) -> f32 {
    let mut d = to - from;
    if !three_d {
        d.y = 0.0;
    }
    d.length()
}

#[pymethods]
impl Ai {
    /// A subclass is constructed with the script's own constructor arguments
    /// (`Rammer(topspeed=8.0, turnspeed=1.75)`), which Python passes to `__new__`
    /// as well as `__init__`; `AI.SuperAI.__init__` is where they are read.
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(_args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> Self {
        Ai::default()
    }

    fn __init__(&self) {}

    /// The engine's hand-off: which robot this brain drives.
    fn __ra2_bind__(&mut self, bot: i32) {
        self.bot = bot;
    }

    fn Activate(&mut self, active: bool) -> bool {
        self.active = active;
        true
    }

    fn Tick(&self) -> bool {
        true
    }

    fn ImmobilityWarning(&self, _id: i32, _on: bool) {}

    /// The engine's tick period, in seconds — `AimToHeading` scales its turning
    /// rate by it.
    #[getter]
    fn tickInterval(&self) -> f32 {
        crate::AI_TICK_INTERVAL
    }

    fn GetID(&self) -> i32 {
        self.bot
    }

    /// `Input(name, index, value)` — a control-board channel.
    ///
    /// The name is a control on the bot's own board (`Forward`, `LeftRight`,
    /// `Spin`, `Fire!`, `Lefthook` ...), the index its binding slot. The call is
    /// queued as an event for the simulator and the level kept for
    /// `GetInputStatus`.
    fn Input(&mut self, name: &str, index: i32, value: f32) {
        self.inputs.insert((name.to_string(), index), value);
        self.events.push((name.to_string(), index, value));
    }

    fn GetInputStatus(&self, name: &str, index: i32) -> f32 {
        self.inputs
            .get(&(name.to_string(), index))
            .copied()
            .unwrap_or(0.0)
    }

    fn GetLocation(&self) -> (f32, f32, f32) {
        with_bot(self.bot, |b| (b.location.x, b.location.y, b.location.z))
            .unwrap_or((0.0, 0.0, 0.0))
    }

    fn GetDirection(&self) -> (f32, f32, f32) {
        with_bot(self.bot, |b| (b.direction.x, b.direction.y, b.direction.z))
            .unwrap_or((0.0, 0.0, 1.0))
    }

    fn GetSpeed(&self) -> f32 {
        with_bot(self.bot, |b| b.speed).unwrap_or(0.0)
    }

    fn GetTurning(&self) -> f32 {
        with_bot(self.bot, |b| b.turning).unwrap_or(0.0)
    }

    fn IsUpsideDown(&self) -> bool {
        with_bot(self.bot, |b| b.upside_down).unwrap_or(false)
    }

    /// The original's hook for robots that can fight on their back; a bot's own
    /// script overrides it (`AI/Flipper.py`).
    fn CanDriveUpsideDown(&self, _bot: i32) -> bool {
        false
    }

    /// Everyone still in the fight except us.
    fn GetEnemies(&self) -> Vec<i32> {
        let st = crate::state();
        st.bots
            .iter()
            .enumerate()
            .filter(|(i, b)| b.alive && *i as i32 != self.bot)
            .map(|(i, _)| i as i32)
            .collect()
    }

    /// The distance is **planar**: `0x4019c0` copies the bot's location and the
    /// target into two static vectors (`0x714610`, `0x714600`), zeroes both `y`
    /// words (`0x401a4d`, `0x401a6b`) and takes the length of the difference
    /// (`0x401aa0`, `0x401ae0`). The map's cells carry heights (the Octagon's
    /// 1.9 m cells beside its hellraisers), which `FromGrid` hands back as a
    /// waypoint's `y`; a 3-D length kept a robot from ever reaching them.
    /// `RA2_AI_DIST3D=1` is the old length.
    fn GetDistanceTo(&self, target: (f32, f32, f32)) -> f32 {
        let three_d = ai_dist_3d();
        with_bot(self.bot, |b| {
            ai_distance(b.location, Vec3::new(target.0, target.1, target.2), three_d)
        })
        .unwrap_or(0.0)
    }

    fn GetDistanceToID(&self, id: i32) -> f32 {
        let st = crate::state();
        let (Some(a), Some(b)) = (st.bot(self.bot), st.bot(id)) else {
            return 0.0;
        };
        ai_distance(a.location, b.location, ai_dist_3d()) // the same `0x4019c0`
    }

    fn GetHeading(&self, reverse: bool) -> f32 {
        self.own_heading(reverse)
    }

    fn GetHeadingTo(&self, target: (f32, f32, f32), reverse: bool) -> f32 {
        self.heading_to(Vec3::new(target.0, target.1, target.2), reverse)
    }

    fn GetHeadingToID(&self, id: i32, reverse: bool) -> f32 {
        let target = crate::state().bot(id).map(|b| b.location);
        match target {
            Some(t) => self.heading_to(t, reverse),
            None => 0.0,
        }
    }

    fn GetNumComponents(&self) -> i32 {
        with_bot(self.bot, |b| b.components.len() as i32).unwrap_or(0)
    }

    fn GetComponentType(&self, index: i32) -> String {
        with_bot(self.bot, |b| {
            b.components
                .get(index.max(0) as usize)
                .map(|(c, _)| c.clone())
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn GetComponentHealth(&self, index: i32) -> f32 {
        with_bot(self.bot, |b| {
            b.components
                .get(index.max(0) as usize)
                .map(|(_, h)| *h)
                .unwrap_or(0.0)
        })
        .unwrap_or(0.0)
    }

    /// `(component, amount, time, victim, victim component)` of the last hit
    /// we landed (see [`crate::DamageRecord`]).
    fn GetLastDamageDone(&self) -> crate::DamageRecord {
        with_bot(self.bot, |b| b.last_damage_done).unwrap_or(crate::NO_DAMAGE)
    }

    /// `(component, amount, time, attacker, attacker component)` of the last
    /// hit we took. `AI/Spinner.py` reads index 3, so a 3-tuple raises.
    fn GetLastDamageReceived(&self) -> crate::DamageRecord {
        with_bot(self.bot, |b| b.last_damage_received).unwrap_or(crate::NO_DAMAGE)
    }

    /// `RegisterSmartZone(name, id)` — "tell me when a robot enters this zone".
    fn RegisterSmartZone(&mut self, name: &str, id: i32) {
        if !self.zones.iter().any(|(n, _)| n == name) {
            self.zones.push((name.to_string(), id));
        }
    }

    /// The robots inside one of *this robot's* SmartZones, as `(1-based
    /// robot, chassis)` readings (`FUN_004014f0`: the robot's SmartZone
    /// component whose name matches, then every body its phantom overlaps,
    /// decoded from the body's tag `robot << 4 | chassis`).
    fn GetZoneContents(&self, zone: &str) -> Vec<(i32, bool)> {
        crate::state()
            .robot_zones
            .get(&(self.bot, zone.to_string()))
            .cloned()
            .unwrap_or_default()
    }

    fn ResetComponents(&self) {}

    // The engine's notifications. `AI.SuperAI` and the brains override them
    // and call up into these base versions (`AI.SuperAI.LostComponent(self,
    // id)` in every weapon brain), so each must exist and answer `True`, as
    // the C++ defaults bound at `0x4bba00` do.

    /// `SmartZoneEvent(direction, id, robot, chassis)` — a robot entered (`1`)
    /// or left (`-1`) one of our registered SmartZones.
    #[pyo3(signature = (_direction, _id, _robot, _chassis))]
    fn SmartZoneEvent(&self, _direction: i32, _id: i32, _robot: i32, _chassis: bool) -> bool {
        true
    }

    /// `LostComponent(id)` — one of our components came off or was destroyed.
    fn LostComponent(&self, _id: i32) -> bool {
        true
    }

    /// `FractureEvent()` — a part broke loose somewhere; time to refresh.
    fn FractureEvent(&self) -> bool {
        true
    }

    /// `CriticalHit(...)` — unused by the shipped brains.
    #[pyo3(signature = (*_args))]
    fn CriticalHit(&self, _args: &Bound<'_, PyTuple>) -> bool {
        true
    }

    /// `PlayerOut(id)` — a robot is out of the match.
    fn PlayerOut(&self, _id: i32) -> bool {
        true
    }

    /// `ReThink()` — re-evaluate now.
    fn ReThink(&self) -> bool {
        true
    }

    /// `Winner(...)` — the match is decided.
    #[pyo3(signature = (*_args))]
    fn Winner(&self, _args: &Bound<'_, PyTuple>) -> bool {
        true
    }

    /// `fNoseOffset` — the brain's `nose` constructor argument (`AI+0x40`,
    /// bound read/write next to `tickInterval`). The shipped scripts only store
    /// it, and none of the C++ queries the AI uses reads it.
    #[getter]
    fn fNoseOffset(&self) -> f32 {
        self.nose
    }

    #[setter]
    fn set_fNoseOffset(&mut self, nose: f32) {
        self.nose = nose;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BotSnapshot, SoundId, SoundQueue, SoundRequest, Vec3};

    /// Start with an empty sound table and queue. The script layer is
    /// process-global, so every sound test clears it first.
    fn reset_sounds() {
        let mut st = crate::state();
        st.sounds = SoundQueue::new();
        st.log.clear();
    }

    /// The five calls the shipped arenas make are a handle table and an
    /// ordered queue, with no device and no `ra2-app` in sight.
    ///
    /// `Arenas/__init__.py`, `king.py` and the hazards are the callers: they
    /// create music with `loop=False` and loop it later, create hazard loops
    /// with `loop=True`, play a handle more than once, stop it, and remove it.
    /// This runs the same sequence through the real `plus` module.
    #[test]
    fn plus_sounds_are_handles_on_an_ordered_queue() {
        let _guard = crate::lock();
        reset_sounds();
        Python::with_gil(|py| {
            crate::plus::register(py).expect("register plus");
            py.run(
                c"import plus\n\
intro = plus.createSound(r'Sounds\\intro_music\\smell_glue.wav', True, (1.0, 2.0, 3.0))\n\
crowd = plus.createSound('Sounds/crowd/LoudCheer_Loop.wav', False, (4.0, 5.0, 6.0))\n\
plus.playSound(intro)\n\
plus.playSound(intro)\n\
plus.loopSound(crowd)\n\
plus.stopSound(intro)\n\
plus.removeSound(intro)\n",
                None,
                None,
            )
            .expect("the script's sound sequence runs");
            let ids: (u64, u64) = py
                .eval(c"(intro, crowd)", None, None)
                .expect("both handles")
                .extract()
                .expect("handles are integers");
            assert_eq!(ids, (1, 2), "handles are stable and start at 1");
        });

        let st = crate::state();
        assert_eq!(
            st.sounds.requests(),
            &[
                SoundRequest::Play {
                    id: SoundId(1),
                    path: r"Sounds\intro_music\smell_glue.wav".to_string(),
                    looping: true,
                    position: [1.0, 2.0, 3.0],
                },
                SoundRequest::Play {
                    id: SoundId(1),
                    path: r"Sounds\intro_music\smell_glue.wav".to_string(),
                    looping: true,
                    position: [1.0, 2.0, 3.0],
                },
                SoundRequest::Loop { id: SoundId(2) },
                SoundRequest::Stop { id: SoundId(1) },
                SoundRequest::Stop { id: SoundId(1) },
            ],
            "play, play, loop, stop, and the stop half of remove, in call order"
        );
        assert!(
            st.sounds.get(SoundId(1)).is_none(),
            "removeSound frees the handle"
        );
        let crowd = st
            .sounds
            .get(SoundId(2))
            .expect("the other sound is still live");
        assert!(crowd.looping, "loopSound sets the loop flag");
        assert_eq!(crowd.path, "Sounds/crowd/LoudCheer_Loop.wav");
        assert_eq!(crowd.position, [4.0, 5.0, 6.0]);
        assert!(
            !st.deferred.iter().any(|what| what.contains("audio")),
            "sounds are no longer deferred: {:?}",
            st.deferred
        );
    }

    /// `Hazards.Flame` makes its emitter with `plus.AddParticleEmitter` and
    /// flips it with `SetEmitting`; the engine side reads the switch from the
    /// script state (`FUN_004b5760` builds the emitter, `FUN_0048e0b0` is the
    /// switch).
    #[test]
    fn a_flame_emitter_is_registered_and_switched() {
        let _guard = crate::lock();
        crate::state().emitters.clear();
        Python::with_gil(|py| {
            crate::plus::register(py).expect("register plus");
            py.run(
                c"import plus\n\
e = plus.AddParticleEmitter((1.0, 2.0, 3.0), (0.0, 0.5, 0.0), (0.1, 0.0, 0.1))\n\
f = plus.AddParticleEmitter((4.0, 5.0, 6.0), (0.0, 1.0, 0.0), (0.0, 0.0, 0.0))\n\
assert e.emitting == False\n\
e.SetEmitting(True)\n\
assert e.emitting == True and f.emitting == False\n\
f.emitting = True\n\
e.SetEmitting(False)\n",
                None,
                None,
            )
            .expect("the flame hazard's calls run");
        });
        let st = crate::state();
        assert_eq!(st.emitters.len(), 2);
        assert_eq!(st.emitters[0].location, [1.0, 2.0, 3.0]);
        assert_eq!(st.emitters[0].velocity, [0.0, 0.5, 0.0]);
        assert_eq!(st.emitters[0].variance, [0.1, 0.0, 0.1]);
        assert!(!st.emitters[0].emitting);
        assert!(st.emitters[1].emitting, "the attribute is the switch too");
        assert!(
            !st.deferred.iter().any(|what| what.contains("particles")),
            "particles are no longer deferred"
        );
    }

    /// `createSound` takes the loop flag the scripts actually pass, which is
    /// both a bool and the integer `1` (`Hazards.RotClaw`).
    #[test]
    fn create_sound_takes_the_scripts_truthy_loop_flag() {
        let _guard = crate::lock();
        reset_sounds();
        Python::with_gil(|py| {
            crate::plus::register(py).unwrap();
            py.run(
                c"import plus\n\
looped = plus.createSound('Sounds/default_loop.wav', 1, (0, 0, 0))\n\
once = plus.createSound('Sounds/default_once.wav', False, (0, 0, 0))\n",
                None,
                None,
            )
            .unwrap();
        });
        let st = crate::state();
        assert!(st.sounds.get(SoundId(1)).unwrap().looping);
        assert!(!st.sounds.get(SoundId(2)).unwrap().looping);
        assert!(st.sounds.requests().is_empty(), "creating does not play");
    }

    /// An unknown handle is a named error, not a silent no-op — a typo in a
    /// script must not look like success.
    #[test]
    fn plus_play_sound_names_an_unknown_handle() {
        let _guard = crate::lock();
        reset_sounds();
        Python::with_gil(|py| {
            crate::plus::register(py).unwrap();
            let plus = py.import("plus").unwrap();
            let error = plus
                .call_method1("playSound", (4242u64,))
                .expect_err("an unknown handle must raise");
            assert!(
                error.is_instance_of::<PyValueError>(py),
                "the error should be a ValueError: {error}"
            );
            let message = error.to_string();
            assert!(
                message.contains("4242"),
                "the message names the handle: {message}"
            );
            assert!(message.contains("playSound"), "{message}");
        });
        assert!(
            crate::state().sounds.requests().is_empty(),
            "a failed play queues nothing"
        );
    }

    /// The arenas remove their sounds from `__del__`; by then the match may
    /// already have removed the same handle in `DoIntro`. That is a named
    /// warning, not a panic and not a Python exception.
    #[test]
    fn a_scripts_del_can_remove_an_already_removed_sound() {
        let _guard = crate::lock();
        reset_sounds();
        Python::with_gil(|py| {
            crate::plus::register(py).unwrap();
            py.run(
                c"import plus\nsound = plus.createSound('Sounds/button_click.wav', False, (0.0, 0.0, 0.0))\nclass Teardown(object):\n    def __init__(self, handle):\n        self.handle = handle\n    def __del__(self):\n        plus.removeSound(self.handle)\nkeeper = Teardown(sound)\nplus.removeSound(sound)\ndel keeper\n",
                None,
                None,
            )
            .expect("the second removal is tolerated");
        });
        let st = crate::state();
        assert_eq!(
            st.sounds.requests(),
            &[SoundRequest::Stop { id: SoundId(1) }],
            "one stop, not two"
        );
        assert!(
            st.log
                .iter()
                .any(|line| line.contains("sound 1") && line.contains("already removed")),
            "the warning names the handle: {:?}",
            st.log
        );
        assert!(st.sounds.get(SoundId(1)).is_none());
    }

    /// `GetDistanceTo` is a planar length (`0x4019c0`): a waypoint on a map cell
    /// 1.9 m up is as far as the one on the floor beneath it.
    #[test]
    fn the_ais_distance_ignores_height() {
        let from = Vec3::new(1.0, 0.2, 2.0);
        let to = Vec3::new(1.0, 1.66, 2.0);
        assert_eq!(ai_distance(from, to, false), 0.0);
        assert!((ai_distance(from, to, true) - 1.46).abs() < 1e-5);
        let far = Vec3::new(4.0, 9.0, 6.0);
        assert!((ai_distance(from, far, false) - 5.0).abs() < 1e-5);
    }

    /// A blade: long in Y, thin in X and Z. Its bounding sphere is 2 m across,
    /// so it is the shape that tells the two tests apart.
    fn a_blade() -> BotSnapshot {
        BotSnapshot {
            alive: true,
            spheres: vec![(Vec3::ZERO, 2.0)],
            boxes: vec![(Vec3::ZERO, Vec3::new(0.05, 2.0, 0.05), Quat::IDENTITY)],
            ..BotSnapshot::default()
        }
    }

    /// `rayTest` answers against the parts, not the balls around them.
    ///
    /// The flame jet is a short ray and it damages what it reaches, so a ray
    /// that passes a hand's width to the side of a blade must not set it alight
    /// — which is exactly what the bounding sphere would do.
    #[test]
    fn a_ray_tests_parts_not_their_bounding_balls() {
        let _guard = crate::lock();
        crate::state().bots = vec![a_blade()];

        // Broadside: straight through the blade's thin axis.
        let (bot, component, x, _, _) = ray_test((-3.0, 0.0, 0.0), (3.0, 0.0, 0.0));
        assert_eq!((bot, component), (0, 0), "the ray should hit the blade");
        assert!((x + 0.05).abs() < 1e-3, "hit point x = {x}");

        // A metre to the side, still inside the bounding sphere but clear of
        // the blade itself.
        let missed = ray_test((-3.0, 0.0, 1.0), (3.0, 0.0, 1.0));
        assert_eq!(missed.0, -1, "the ray should miss the blade: {missed:?}");

        // Without a box for the part, the sphere is what is left.
        crate::state().bots[0].boxes.clear();
        let crude = ray_test((-3.0, 0.0, 1.0), (3.0, 0.0, 1.0));
        assert_eq!(
            crude.0, 0,
            "the sphere fallback should still hit: {crude:?}"
        );
        crate::state().bots.clear();
    }
}
