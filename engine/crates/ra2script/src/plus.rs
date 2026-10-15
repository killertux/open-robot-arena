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

use std::collections::BTreeMap;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

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

#[pyfunction]
#[pyo3(name = "force")]
fn force(bot: i32, x: f32, y: f32, z: f32) {
    crate::state().requests.push(crate::Request::Force {
        bot,
        impulse: Vec3::new(x, y, z),
    });
}

/// `rayTest(from, to)` -> `(bot, component, x, y, z)`, or `(-1, -1, ...)`.
///
/// Tested against the per-component spheres the engine publishes rather than the
/// full hulls: it is the flame hazard's "did the jet reach a component" test, and
/// a sphere per part is what the snapshot carries. See F13.
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
        for (ci, (c, r)) in bot.spheres.iter().enumerate() {
            let oc = a - *c;
            let bq = oc.dot(u);
            let disc = bq * bq - (oc.dot(oc) - r * r);
            if disc < 0.0 {
                continue;
            }
            let t = -bq - disc.sqrt();
            if t > 0.0 && t < best && t <= len {
                best = t;
                hit = (bi as i32, ci as i32);
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

#[pyfunction]
#[pyo3(name = "isDefeated")]
fn is_defeated(bot: i32) -> bool {
    with_bot(bot, |b| !b.alive).unwrap_or(false)
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
