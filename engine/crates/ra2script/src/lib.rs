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

use glam::Vec3;

pub mod arena;
pub mod host;
pub mod plus;

pub use arena::{Arena, ArenaData, ArenaDetail, Drive, HingeController, PrismaticController};
pub use host::ScriptHost;

/// The physics side of the free functions, installed by the engine.
///
/// The `plus` functions are called *from* Python but operate on the live match:
/// `plus.getLocation(bot)`, `plus.damage(...)`, `plus.force(...)`,
/// `plus.rayTest(...)`, `plus.getWeight(bot)`. Rather than have this crate
/// depend on the simulator, the simulator implements this and installs it.
pub trait MatchApi: Send {
    /// World position of a robot's chassis.
    fn location(&self, bot: i32) -> Vec3;
    /// Total mass in kg.
    fn weight(&self, bot: i32) -> f32;
    /// Whether the robot is on its back.
    fn upside_down(&self, bot: i32) -> bool;
    /// Apply `amount` points of damage to one component, from a point.
    fn damage(&mut self, bot: i32, component: i32, amount: f32, point: Vec3);
    /// Apply an impulse to a robot.
    fn force(&mut self, bot: i32, impulse: Vec3);
    /// Ray test against robots: `(bot, component, point)`, `bot = -1` for none.
    fn ray_test(&self, from: Vec3, to: Vec3) -> (i32, i32, Vec3);
    /// Robots currently inside a named SmartZone.
    fn robots_in_zone(&self, zone: &str) -> Vec<(i32, bool)>;
    /// Robots that exist and are still in the fight.
    fn alive(&self) -> Vec<bool>;
    /// Match clock, in seconds.
    fn time(&self) -> f32;
}

/// The live match: what the free functions and `plus.Arena` refer to.
pub struct State {
    pub game_root: Option<PathBuf>,
    /// The arena script's data, shared with the live `plus.Arena` object.
    pub arena: Option<Arc<Mutex<ArenaData>>>,
    /// The Python arena instance (the subclass), so the engine can call `Tick`.
    pub arena_object: Option<pyo3::Py<pyo3::PyAny>>,
    /// The running AI instances.
    pub ai: Vec<pyo3::Py<pyo3::PyAny>>,
    pub physics: Option<Box<dyn MatchApi>>,
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
            physics: None,
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
}

static STATE: Mutex<State> = Mutex::new(State::new());

/// Lock the live match state.
///
/// Keep the guard short: never call back into Python while holding it.
pub fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
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

    /// The script layer is process-global (`plus` is a module, and the game's
    /// own `Arenas.currentArena` is a module global), so the tests share one
    /// interpreter and run one at a time.
    static LOCK: Mutex<()> = Mutex::new(());

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
        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
