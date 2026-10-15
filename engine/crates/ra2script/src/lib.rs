//! The RA2 script layer: the `plus`/`Arenas`/`AI` object model on an embedded
//! CPython.
//!
//! *Robot Arena 2*'s game logic is Python 2.2 — the AI strategies, the arena
//! hazards, the match flow — talking to a C++ engine through a module called
//! `plus`. This crate is that engine side, and it runs the **shipped scripts
//! unmodified** (see [`host::ScriptHost`] and `legacy.py`), which is what makes
//! the port faithful: the behaviour under test is not a re-description of the
//! scripts, it *is* the scripts.
//!
//! Layout:
//!
//! * [`host`] — boots the interpreter, points it at the game's own directories
//!   and installs the translating loader for the Python-2 sources.
//! * [`plus`] — the `plus` module: `Arena`, the controllers, and the free
//!   functions the scripts call.
//! * [`arena`] — `plus.Arena`'s data model, including the navigation map the AI
//!   paths over.
//! * [`State`] — the live match the free functions refer to, plus the
//!   [`MatchApi`] seam the physics side implements.
//!
//! Every bound name here — `GetStartPointLocation`, `SetSubMaterialSound`,
//! `ZoneEvent` — is the name the shipped scripts call, so the crate keeps the
//! original's capitalisation.
#![allow(non_snake_case)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use glam::{Quat, Vec3};

pub mod arena;
pub mod gooey;
pub mod host;
pub mod plus;

pub use arena::{Arena, ArenaData, ArenaDetail, Drive, HingeController, PrismaticController};
pub use host::ScriptHost;
pub use plus::Ai;

/// One robot, as the scripts see it.
///
/// The AI asks the engine these questions every tick — where am I, which way am
/// I facing, how fast, who is the enemy, what did I last hit — so the simulator
/// publishes a snapshot per robot and the script layer answers from it. Keeping
/// it data (rather than a trait the simulator implements) means the two sides
/// never hold each other's locks: the simulator writes snapshots, the scripts
/// read them, and anything the scripts want to *do* goes on [`Request`].
#[derive(Clone, Debug, Default)]
pub struct BotSnapshot {
    pub alive: bool,
    /// Chassis position.
    pub location: Vec3,
    /// Unit vector the robot faces.
    pub direction: Vec3,
    /// Forward speed in m/s (`GetSpeed`).
    pub speed: f32,
    /// Yaw rate in rad/s (`GetTurning`).
    pub turning: f32,
    pub upside_down: bool,
    pub mass: f32,
    /// `(class, health)` per component, in the rig's order.
    pub components: Vec<(String, f32)>,
    /// World-space sphere of each component, for `plus.rayTest`.
    pub spheres: Vec<(Vec3, f32)>,
    /// World-space oriented box of each component — `(centre, half-extents,
    /// rotation)` — which is what `plus.rayTest` answers against: a part is
    /// rarely a ball, and the flame jet is supposed to touch what it hits.
    pub boxes: Vec<(Vec3, Vec3, Quat)>,
    /// `(component, amount, time)` of the last blow this robot landed.
    pub last_damage_done: (i32, f32, f32),
    /// …and the last one it took.
    pub last_damage_received: (i32, f32, f32),
}

/// Something a script asked the engine to do.
///
/// The `plus` functions run inside Python, so they queue; the simulator drains
/// the queue at the top of the next step.
#[derive(Clone, Copy, Debug)]
pub enum Request {
    /// `plus.damage(bot, component, amount, point)`.
    Damage {
        bot: i32,
        component: i32,
        amount: f32,
        point: Vec3,
    },
    /// `plus.force(bot, x, y, z)` — a force in N at the robot's body, for the
    /// step it lands in. The engine turns it into `force * dt` of impulse.
    Force { bot: i32, momentum: Vec3 },
    /// `plus.zap(bot, amount, radius)`.
    Zap { bot: i32, amount: f32, radius: f32 },
    /// `plus.eliminatePlayer(bot)`.
    Eliminate { bot: i32 },
}

/// How often the game ran its AI scripts, in seconds.
///
/// Not a guess: `AI.SuperAI.Tick` refreshes its sensors when its own counter
/// reaches 8, and its comment says "once per second", and `AimToHeading` scales
/// a turning rate by `self.tickInterval`. So a brain tick is an eighth of a
/// second and `tickInterval` is what the strategies multiply time by.
pub const AI_TICK_INTERVAL: f32 = 0.125;

/// How often the game ran an arena's `Tick`, in seconds.
///
/// `Hazards.Flame.Tick` counts by 0.25 and its comment says the flames last "the
/// end of 1 second" after four of them; `Hazards.Electricity` counts a 5 s
/// charge down by 0.25, i.e. twenty ticks.
pub const ARENA_TICK_INTERVAL: f32 = 0.25;

/// The live match: what the free functions and `plus.Arena` refer to.
pub struct State {
    pub game_root: Option<PathBuf>,
    /// The arena script's data, shared with the live `plus.Arena` object.
    pub arena: Option<Arc<Mutex<ArenaData>>>,
    /// The Python arena instance (the subclass), so the engine can call `Tick`.
    pub arena_object: Option<pyo3::Py<pyo3::PyAny>>,
    /// The running AI instances, one per robot.
    pub ai: Vec<pyo3::Py<pyo3::PyAny>>,
    /// What the scripts can see of each robot.
    pub bots: Vec<BotSnapshot>,
    /// What the scripts asked the engine to do.
    pub requests: Vec<Request>,
    /// Joints the arena scripts created (`AddPrismatic`, `GetHinge`), waiting
    /// for the simulator to build them.
    pub joints: Vec<arena::JointRequest>,
    /// Which robots the engine currently reports inside each registered zone.
    pub zone_contents: BTreeMap<String, Vec<(i32, bool)>>,
    pub time: f32,
    pub dt: f32,
    pub paused: bool,
    pub over: bool,
    pub players: Vec<i32>,
    pub game_type: String,
    pub next_handle: u64,
    pub sounds: BTreeMap<u64, String>,
    /// What the script layer asked for that a later milestone owns.
    pub deferred: Vec<String>,
    /// Anything the script layer wants to say to the operator.
    pub log: Vec<String>,
}

impl State {
    const fn new() -> Self {
        State {
            game_root: None,
            arena: None,
            arena_object: None,
            ai: Vec::new(),
            bots: Vec::new(),
            requests: Vec::new(),
            joints: Vec::new(),
            zone_contents: BTreeMap::new(),
            time: 0.0,
            dt: 1.0 / 30.0,
            paused: false,
            over: false,
            players: Vec::new(),
            game_type: String::new(),
            next_handle: 1,
            sounds: BTreeMap::new(),
            deferred: Vec::new(),
            log: Vec::new(),
        }
    }

    pub fn defer(&mut self, what: &str) {
        if !self.deferred.iter().any(|w| w == what) {
            self.deferred.push(what.to_string());
        }
    }

    /// The snapshot for a robot, if the engine has published one.
    pub fn bot(&self, bot: i32) -> Option<&BotSnapshot> {
        if bot < 0 {
            return None;
        }
        self.bots.get(bot as usize)
    }
}

static STATE: Mutex<State> = Mutex::new(State::new());

/// The script layer is process-global — `plus` is a module and the game's own
/// `Arenas.currentArena` is a module global — so the live match is shared.
/// Anything that drives a scripted match (a test, a tool) takes this first.
static SCRIPT_LOCK: Mutex<()> = Mutex::new(());

/// Take the whole script layer for this thread.
pub fn lock() -> MutexGuard<'static, ()> {
    SCRIPT_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Lock the live match state.
///
/// Keep the guard short: never call back into Python while holding it.
pub fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Publish what the scripts can see of the live match, and advance its clock.
///
/// The simulator calls this at the top of every step, after it has run the
/// brains (which read the previous snapshot) and before it applies the drive.
pub fn publish(bots: Vec<BotSnapshot>, time: f32, dt: f32) {
    let mut st = state();
    st.bots = bots;
    st.time = time;
    st.dt = dt;
}

/// Take the requests the scripts queued during the last tick.
pub fn take_requests() -> Vec<Request> {
    std::mem::take(&mut state().requests)
}

/// Take the joints the arena scripts have asked for since the last call.
pub fn take_joint_requests() -> Vec<arena::JointRequest> {
    std::mem::take(&mut state().joints)
}

/// Point the script layer at a game installation.
pub fn set_game_root(root: PathBuf) {
    state().game_root = Some(root);
}

/// The game installation the scripts are being run from.
pub fn game_root() -> Option<PathBuf> {
    state().game_root.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::prelude::*;
    use std::sync::Mutex;
    use std::sync::MutexGuard;

    /// The (uncommitted) game directory, when present.
    fn game_dir() -> Option<PathBuf> {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join("local/game");
            if candidate.is_dir() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    fn boot() -> Option<(ScriptHost, MutexGuard<'static, ()>)> {
        let guard = super::lock();
        let host = ScriptHost::boot(game_dir()?).ok()?;
        Some((host, guard))
    }

    /// Run `f` with the GIL held.
    fn with_py<R>(f: impl FnOnce(Python<'_>) -> R) -> R {
        Python::with_gil(f)
    }

    fn arena_data() -> std::sync::Arc<Mutex<ArenaData>> {
        state().arena.clone().expect("an arena is live")
    }

    /// Length of a list attribute on one of the script's own objects.
    fn list_len(obj: &Py<PyAny>, py: Python<'_>, name: &str) -> usize {
        obj.bind(py).getattr(name).unwrap().len().unwrap()
    }

    /// M4: the shipped arena scripts are Python 2 and run unmodified.
    ///
    /// This is the whole point of the layer — every stock arena is a subclass of
    /// an engine class, and they only register themselves when their module
    /// executes, so a successful list means the translation, the binding surface
    /// and the import path all worked on real game code.
    #[test]
    fn the_shipped_arena_scripts_load_and_register() {
        let Some((host, _guard)) = boot() else { return };
        let names = with_py(|py| host.load_arenas(py)).expect("arena scripts load");

        assert!(
            names.len() >= 12,
            "only {} arenas registered: {names:?}",
            names.len()
        );
        assert!(names.iter().any(|n| n == "Combat Arena"), "{names:?}");
        assert!(names.iter().all(|n| !n.is_empty()), "{names:?}");
        // `Arenas/__init__.py` ends with `import Practice`, which is
        // `Scripts/Practice.py` — a second directory, also Python 2.
        assert!(
            names.iter().any(|n| n == "Practice Arena"),
            "the Scripts/ path is missing: {names:?}"
        );
    }

    /// M4: the translation rewrites what Python 3 will not parse, and nothing
    /// else. Pinned against the constructs the survey found in the game.
    #[test]
    fn the_legacy_loader_rewrites_python2_syntax() {
        let Some((_host, _guard)) = boot() else {
            return;
        };
        with_py(|py| {
            let legacy = py.import("ra2_legacy").unwrap();
            let translate = legacy.getattr("translate").unwrap();
            let cases: &[(&str, &[&str], bool)] = &[
                ("a = 1\nprint \"a\", a\n", &["print(", "\"a\""], true),
                // `print >>f` needs a real file, so it is checked as text only.
                ("print >>f, 1\n", &["file=f"], false),
                (
                    "d = {}\nfor k, v in d.iteritems(): pass\n",
                    &[".items()"],
                    true,
                ),
                (
                    "d = {}\nfor v in d.itervalues(): pass\n",
                    &[".values()"],
                    true,
                ),
                ("it = iter([1])\n[it.next()]\n", &["next(it)"], true),
                ("a = 1\nb = 2\n[a <> b]\n", &["!="], true),
                (
                    "if 0:\n    raise ValueError, \"m\"\n",
                    // The translator inserts the paren where the comma was, so
                    // the spacing follows the source.
                    &["raise ValueError("],
                    true,
                ),
                (
                    "a = 1\nimport Hazards\nfrom king import KingOfHill\n",
                    &["from . import Hazards", "from .king import KingOfHill"],
                    // Relative imports need a package, so this is text-only.
                    false,
                ),
            ];
            for (src, wants, runnable) in cases {
                // `siblings` is how the loader hands the containing package to
                // the translator; here `local/game/Arenas` has both names.
                let dir = game_dir().unwrap().join("Arenas");
                let out: String = translate
                    .call1((*src, dir.to_string_lossy().to_string()))
                    .unwrap()
                    .extract()
                    .unwrap();
                for want in *wants {
                    assert!(out.contains(want), "{src:?} -> {out:?} missing {want:?}");
                }
                if !runnable {
                    continue;
                }
                // And the result has to be valid Python 3.
                let c = std::ffi::CString::new(out.clone()).unwrap();
                py.run(&c, None, None)
                    .unwrap_or_else(|e| panic!("{src:?} -> {out:?} does not compile: {e}"));
            }
            // Text inside a string is not code and is left alone.
            let src = "s = \"a.iteritems() and print x\"\n";
            let out: String = translate.call1((src,)).unwrap().extract().unwrap();
            assert_eq!(out, src);
        });
    }

    /// M4/F13: the engine tells the arena the match has begun, and the arena's
    /// own hazards take it from there.
    ///
    /// `Arena.Activate` is the arenas' start-of-match hook and the game calls it.
    /// Some of them cannot work without it: the Octagon only opens its four trap
    /// floors from `Pit1.setactive(True)` inside `Activate` (King of the Hill
    /// reads the game type there). The floors then open on their *own* script's
    /// clock — `Hazards.PitSlider.Tick` counts halves of a second and its delay is
    /// 45, so it lets go on the ninetieth arena tick — which is what the engine's
    /// `Tick` loop is for.
    #[test]
    fn the_engine_tells_the_arena_the_match_has_begun() {
        let Some(game) = game_dir() else { return };
        let _guard = super::lock();
        let script = game.join("Arenas/octagon.py");
        let host = match ScriptHost::boot_with_scripts(&game, Some(&script)) {
            Ok(h) => h,
            Err(e) => panic!("booting the Octagon: {e}"),
        };
        let pits = ["Pit1", "Pit2", "Pit3", "Pit4"];
        with_py(|py| {
            let arena = state()
                .arena_object
                .as_ref()
                .expect("the arena is live")
                .clone_ref(py);
            let a = arena.bind(py);
            let flag = |pit: &str, name: &str| -> bool {
                a.getattr(pit)
                    .unwrap_or_else(|e| panic!("{pit}: {e}"))
                    .getattr(name)
                    .unwrap()
                    .extract()
                    .unwrap()
            };
            // Only `octagon.py`'s `Activate(True)` arms them.
            for pit in pits {
                assert!(flag(pit, "active"), "{pit} was never told to open");
            }
            // And ninety of the arena's quarter-seconds later they let go.
            for _ in 0..90 {
                host.tick_arena(py, 0.25).unwrap();
            }
            for pit in pits {
                assert!(flag(pit, "sliding"), "{pit} never started sliding");
            }
        });
    }

    /// M4: `Arenas/box.py` drives the engine through `plus.Arena` — cameras,
    /// shadow receivers, the navigation map, the hazard zones.
    #[test]
    fn the_box_arena_configures_itself_from_its_script() {
        let Some((host, _guard)) = boot() else { return };
        let (zones, hazards, spikes_type) = with_py(|py| {
            host.load_arenas(py).unwrap();
            let arena = host.create_arena(py, "Combat Arena").expect("box arena");

            {
                let data = arena_data();
                let d = data.lock().unwrap();
                assert_eq!(d.model, "Arenas/box/boxarena.gmf");
                // `AddShadowReceivers` is the engine's call, and it reaches the
                // engine: `Arenas.SuperArena` leaves `SetShadowSource` and
                // `AddShadowTriangle` to `plus.Arena`.
                assert_eq!(d.shadow_source, Some([0.653, 6.894, -0.418]));
                assert_eq!(d.shadow_triangles.len(), 10);
                // `LoadMap` read the shipped .map: 30x30 cells at 1 m.
                assert_eq!((d.map.cols, d.map.rows), (30, 30));
                assert_eq!(d.map.resolution, 1.0);
                assert!(
                    (d.map.altitude + 2.264_98).abs() < 1e-4,
                    "{}",
                    d.map.altitude
                );
                assert_eq!(d.extent, [-15.0, 15.0, 15.0, -15.0]);
                assert!(d.map.cells.iter().filter(|c| c.wall).count() > 100);
                assert_eq!(d.map.cells.iter().filter(|c| c.hazard).count(), 24);
                // Hazards are off until the match says otherwise.
                assert!(d.zones.is_empty());
                assert!(d.start_points.len() >= 2, "the model declares start points");
            }

            // The cameras are the Python layer's: `Arenas.SuperArena` keeps them
            // in `self.StaticCams`/`self.WatchCams` and applies them through
            // `plus.setCameraPosition`, so the script's own list is the evidence.
            assert_eq!(list_len(&arena, py, "StaticCams"), 3);
            assert_eq!(list_len(&arena, py, "WatchCams"), 1);

            // `HazardsOn(True)` builds the hazards, registers their zones and
            // walls the map around them.
            let walls_before = arena_data()
                .lock()
                .unwrap()
                .map
                .cells
                .iter()
                .filter(|c| c.wall)
                .count();
            assert!(host.hazards_on(py, true).unwrap());
            let (zones, walls_after, hz) = {
                let data = arena_data();
                let d = data.lock().unwrap();
                let walls = d.map.cells.iter().filter(|c| c.wall).count();
                assert!(d.map.hazards_on);
                assert!(d.sub_materials.len() >= 3, "SetSubMaterialSound");
                (d.zones.clone(), walls, list_len(&arena, py, "hazards"))
            };
            assert!(walls_after > walls_before, "AddCollisionLine walls the map");
            let spikes = arena.bind(py).getattr("spikes1").unwrap();
            let ty = spikes.get_type().name().unwrap().to_string();
            (zones, hz, ty)
        });

        assert_eq!(zones.len(), 3, "saw + two spike zones: {zones:?}");
        assert_eq!(zones.get("saw_zone"), Some(&1));
        assert_eq!(zones.get("spike_zone1"), Some(&2));
        assert_eq!(zones.get("spike_zone2"), Some(&3));
        assert_eq!(hazards, 3);
        // The hazards are the script's own classes, with their own state.
        assert_eq!(spikes_type, "Spikes");
    }

    /// M4: `plus.Arena` answers the script's geometry and pathing questions.
    #[test]
    fn the_arena_answers_ray_tests_and_pathing() {
        let Some((host, _guard)) = boot() else { return };
        with_py(|py| {
            host.load_arenas(py).unwrap();
            let arena = host.create_arena(py, "Combat Arena").unwrap();
            let obj = arena.bind(py);

            // `GetStartPointLocation` comes from the model's own start points.
            let start: (f32, f32, f32) = obj
                .call_method1("GetStartPointLocation", (0usize,))
                .unwrap()
                .extract()
                .unwrap();
            assert!(start.1 > -30.0 && start.1 < 30.0, "{start:?}");

            // A ray down from well above the arena must find its floor.
            let hit: (bool, (f32, f32, f32)) = obj
                .call_method1(
                    "RayTest",
                    ((start.0, 20.0, start.2), (start.0, -20.0, start.2)),
                )
                .unwrap()
                .extract()
                .unwrap();
            assert!(hit.0, "the arena floor is under the start point");

            // And the navigation grid gives a route across the arena.
            let path: Vec<(i32, i32)> = obj
                .call_method1(
                    "GetPath",
                    ((start.0, start.1, start.2), (0.0, -2.0, 0.0), false),
                )
                .unwrap()
                .extract()
                .unwrap();
            assert!(
                path.len() >= 2,
                "a route from the start point to the centre"
            );

            let (di, dj) = obj
                .call_method1("ToGrid", ((0.0f32, 0.0f32, 0.0f32),))
                .unwrap()
                .extract::<(i32, i32)>()
                .unwrap();
            let back: (f32, f32, f32) = obj
                .call_method1("FromGrid", ((di, dj),))
                .unwrap()
                .extract()
                .unwrap();
            assert!(back.0.abs() <= 1.0 && back.2.abs() <= 1.0, "{back:?}");
        });
    }
}
