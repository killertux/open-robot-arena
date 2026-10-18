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
//! * **Presentation still deferred** — particles and the camera are recorded so
//!   a run reports exactly what it dropped rather than silently ignoring it.

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
    crate::state().back_color = Some([r, g, b].map(|c| c.clamp(0.0, 1.0)));
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

/// The `Forward`/`LeftRight` channels a brain has set, as `(throttle, steer)`
/// in `[-1, 1]` — what `Robot::drive` takes.
///
/// `SuperAI.Turn` sets `LeftRight` 0 and 1 opposite (`-turning` and `turning`),
/// so the steer is their difference; `Throttle` sets `Forward` 0.
///
/// The scripts' `Turn` is positive for a *positive* heading error (`AimToHeading`
/// turns by `dir = sign(heading)`), while `Robot::drive` applies
/// `yaw = -steer * rate` — the stand-in driver compensates by negating the error,
/// and so does this.
pub fn read_drive(brain: &Py<PyAny>, py: Python<'_>) -> Option<(f32, f32)> {
    let obj = brain.bind(py);
    let ai = obj.downcast::<Ai>().ok()?;
    let ai = ai.borrow();
    let channel = |name: &str, index: i32| {
        ai.inputs
            .get(&(name.to_string(), index))
            .copied()
            .unwrap_or(0.0)
    };
    let forward = channel("Forward", 0);
    let left = channel("LeftRight", 0);
    let right = channel("LeftRight", 1);
    Some((
        (forward / 100.0).clamp(-1.0, 1.0),
        ((left - right) / 200.0).clamp(-1.0, 1.0),
    ))
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
    /// SmartZones this brain asked to be told about, `(name, id)`.
    pub zones: Vec<(String, i32)>,
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
    /// `Forward` and `LeftRight` are the drive; anything else (a `Spin` or a paw
    /// trigger the bot's own AI names) is read back by those scripts.
    fn Input(&mut self, name: &str, index: i32, value: f32) {
        self.inputs.insert((name.to_string(), index), value);
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

    fn GetDistanceTo(&self, target: (f32, f32, f32)) -> f32 {
        with_bot(self.bot, |b| {
            (Vec3::new(target.0, target.1, target.2) - b.location).length()
        })
        .unwrap_or(0.0)
    }

    fn GetDistanceToID(&self, id: i32) -> f32 {
        let st = crate::state();
        let (Some(a), Some(b)) = (st.bot(self.bot), st.bot(id)) else {
            return 0.0;
        };
        (b.location - a.location).length()
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

    /// `(component, amount, time)` of the last hit we landed.
    fn GetLastDamageDone(&self) -> (i32, f32, f32) {
        with_bot(self.bot, |b| b.last_damage_done).unwrap_or((0, 0.0, 0.0))
    }

    fn GetLastDamageReceived(&self) -> (i32, f32, f32) {
        with_bot(self.bot, |b| b.last_damage_received).unwrap_or((0, 0.0, 0.0))
    }

    /// `RegisterSmartZone(name, id)` — "tell me when a robot enters this zone".
    fn RegisterSmartZone(&mut self, name: &str, id: i32) {
        if !self.zones.iter().any(|(n, _)| n == name) {
            self.zones.push((name.to_string(), id));
        }
    }

    /// The robots inside a named zone, as `(robot, chassis)` readings.
    fn GetZoneContents(&self, zone: &str) -> Vec<(i32, bool)> {
        crate::state()
            .zone_contents
            .get(zone)
            .cloned()
            .unwrap_or_default()
    }

    fn ResetComponents(&self) {}
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
