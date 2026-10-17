//! Booting the interpreter and loading the game's own scripts.
//!
//! The scripts are the *shipped* ones. They are Python 2.2, so they are loaded
//! through `legacy.py`'s translating loader rather than being rewritten in the
//! repository: the point of the layer is that the behaviour under test is the
//! game's, not our paraphrase of it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use glam::{Mat4, Vec3};
use pyo3::ffi::c_str;
use pyo3::prelude::*;
use pyo3::types::{PyList, PyModule, PyTuple};

/// What the arena model contributes to `plus.Arena`.
#[derive(Default)]
pub struct BuiltArena {
    pub start_points: Vec<(Vec3, Vec3)>,
    pub object_names: Vec<String>,
    pub triangles: Vec<[Vec3; 3]>,
    pub extent: [f32; 4],
    /// Phantom volumes for `GetRobotsInPhantom`, keyed lower-case.
    pub phantoms: BTreeMap<String, crate::arena::PhantomVolume>,
}

/// Read an arena `.gmf`: its named objects, its collidable triangles and its
/// declared start points.
///
/// `RayTest` and `GetStartPointLocation` are answered from this, which is what
/// `Arenas.SuperArena.CreateMap` needs when an arena ships no `.map`.
pub fn load_arena_model(path: &Path) -> Result<BuiltArena, Box<dyn std::error::Error>> {
    let mut out = BuiltArena::default();
    let input = path.to_string_lossy().to_string();
    out.start_points = ra2_render::start_points(&input).unwrap_or_default();

    if let Ok(Some(havok)) = ra2_render::collect_havok(&input) {
        out.triangles = havok.static_triangles.clone();
        for b in &havok.bodies {
            if !b.collision_object.is_empty() {
                out.object_names.push(b.collision_object.clone());
            }
            if !b.display_object.is_empty() {
                out.object_names.push(b.display_object.clone());
            }
            if !b.name.is_empty() {
                out.object_names.push(b.name.clone());
            }
        }
    }
    if let Ok(bytes) = std::fs::read(path) {
        if let Ok(scene) = ra2fmt::gmi::parse(&bytes) {
            // The phantoms (`King_Zone`, `kingzone`) are ordinary `.gmf` mesh
            // objects; the KOTH scripts look them up by name, not through
            // `RegisterZone`, so the volume comes from the model itself. An
            // object's `NODE_TM` is used as its frame, the same way the start
            // points' is; the stock phantom objects are top-level, so there is
            // no parent chain to compose.
            for o in &scene.objects {
                let Some(mesh) = &o.mesh else { continue };
                if mesh.points.is_empty() {
                    continue;
                }
                let mut min = Vec3::splat(f32::MAX);
                let mut max = Vec3::splat(f32::MIN);
                for p in &mesh.points {
                    let p = Vec3::from(*p);
                    min = min.min(p);
                    max = max.max(p);
                }
                let transform = ra2_render::tm_mat4(&o.transform);
                out.phantoms.insert(
                    o.name.to_ascii_lowercase(),
                    crate::arena::PhantomVolume {
                        inv: transform.inverse(),
                        min,
                        max,
                    },
                );
            }
            if out.triangles.is_empty() {
                // No proxies: fall back to everything the scene draws.
                out.triangles = ra2_render::scene_collision_triangles(&scene, Mat4::IDENTITY);
                for o in &scene.objects {
                    out.object_names.push(o.name.clone());
                }
            }
        }
    }
    out.object_names.sort();
    out.object_names.dedup();
    Ok(out)
}

/// The embedded interpreter and the game scripts loaded into it.
pub struct ScriptHost {
    root: PathBuf,
    /// The live brains, `(robot, object)`. Kept here so the Python handles never
    /// leave this crate.
    brains: Vec<(usize, Py<PyAny>)>,
}

impl ScriptHost {
    /// Boot CPython and point it at a game installation.
    ///
    /// `root` is the directory holding `Arenas/`, `AI/`, `Components` and the
    /// rest — the equivalent of the original's working directory.
    pub fn boot(root: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        // Absolute from here on: the scripts are addressed relative to this
        // directory and every path in them is relative too, so the interpreter's
        // `sys.path` entry and the process working directory have to agree.
        let root = root
            .as_ref()
            .canonicalize()
            .unwrap_or_else(|_| root.as_ref().to_path_buf());
        if !root.join("Arenas").is_dir() {
            return Err(format!(
                "{} does not look like a Robot Arena 2 install (no Arenas/)",
                root.display()
            )
            .into());
        }
        crate::set_game_root(root.clone());
        // The game ran with its install directory as the working directory and
        // every script path is relative to it ("Arenas/box/boxarena.gmf",
        // "Sounds/..."), so the script layer needs the same footing.
        std::env::set_current_dir(&root)?;

        Python::with_gil(|py| -> PyResult<()> {
            crate::plus::register(py)?;
            crate::giscript::register(py)?;
            install_legacy(py, &root)?;
            let sys = py.import("sys")?;
            let path: Bound<'_, PyList> = sys.getattr("path")?.downcast_into()?;
            for dir in [root.clone(), root.join("Scripts")] {
                path.insert(0, dir.to_string_lossy().to_string())?;
            }
            Ok(())
        })?;

        Ok(ScriptHost {
            root,
            brains: Vec::new(),
        })
    }

    /// The game installation the scripts are running from.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Import a module from the game (translating it if it is Python 2).
    pub fn import<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyModule>> {
        PyModule::import(py, name)
    }

    /// Run the shell's own entry point, `Scripts/Main.py`, the way the game
    /// does.
    ///
    /// That file is the boot sequence rather than a convenience: it hooks
    /// `sys.stdout` and `sys.stderr` into `giScript`, imports `Arenas` and `AI`
    /// — every arena and brain in the game registering itself as it loads — and
    /// hands the engine `dispatchEvent` to call back. `boot` has already put
    /// `plus` and `giScript` in place and `Scripts/` on `sys.path`, so this is
    /// the import the engine does at startup, and it is the shipped file that
    /// runs.
    pub fn boot_shell(&self, py: Python<'_>) -> Result<(), Box<dyn std::error::Error>> {
        self.import(py, "Main")?;
        Ok(())
    }

    /// Import `Arenas` and every arena script, which registers them.
    ///
    /// The original does this from `Scripts/Main.py`; each arena module calls
    /// `Arenas.register` at the bottom, and the list is what the game's arena
    /// picker enumerates.
    pub fn load_arenas(&self, py: Python<'_>) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let arenas = self.import(py, "Arenas")?;
        let dir = self.root.join("Arenas");
        let mut modules = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".py") {
                continue;
            }
            let stem = name.trim_end_matches(".py");
            if stem == "__init__" || stem == "Hazards" {
                continue;
            }
            // They live *in* the `Arenas` package, and each registers itself
            // with `Arenas.register` when its module body runs.
            modules.push(format!("Arenas.{stem}"));
        }
        modules.sort();
        for m in &modules {
            self.import(py, m)?;
        }
        let list = arenas.getattr("arenas")?;
        let mut names = Vec::new();
        for item in list.try_iter()? {
            let item = item?;
            names.push(item.getattr("name")?.extract::<String>()?);
        }
        Ok(names)
    }

    /// `Arenas.createArenaByName(name)` — instantiate an arena and make it the
    /// live one (`Arenas.currentArena`, which the AI reads).
    pub fn create_arena(
        &self,
        py: Python<'_>,
        name: &str,
    ) -> Result<Py<PyAny>, Box<dyn std::error::Error>> {
        let arenas = self.import(py, "Arenas")?;
        arenas.call_method1("createArenaByName", (name,))?;
        let current = arenas.getattr("currentArena")?;
        if current.is_none() {
            return Err(format!("no arena named {name:?}").into());
        }
        let obj = current.unbind();
        crate::state().arena_object = Some(obj.clone_ref(py));
        // The engine's own step after construction: `Arenas.SuperArena` defines
        // `AddShadowReceivers` as a hook and each arena fills it in, so the
        // shadow source and receiver triangles only reach the engine through
        // this call.
        obj.bind(py).call_method0("AddShadowReceivers")?;
        Ok(obj)
    }

    /// Import `AI` and every AI script, which registers them.
    ///
    /// `AI/__init__.py` pulls in `Bindings` (the bot→brain table) and, at the
    /// bottom, calls `Bindings.load`; each per-bot module calls `AI.register` at
    /// the bottom, so importing them all is what fills the registry — exactly as
    /// `Scripts/Main.py` does in the game.
    pub fn load_ai(&self, py: Python<'_>) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        crate::gooey::install(py)?;
        let ai = self.import(py, "AI")?;
        let dir = self.root.join("AI");
        let mut modules = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".py") {
                continue;
            }
            let stem = name.trim_end_matches(".py");
            // `__init__` is already in; `Bindings` and `Tactics` are imported by
            // it; everything else defines and registers a brain.
            if matches!(stem, "__init__" | "Bindings" | "Tactics") {
                continue;
            }
            modules.push(format!("AI.{stem}"));
        }
        modules.sort();
        for m in &modules {
            self.import(py, m)?;
        }
        let list = ai.getattr("ai_list")?;
        let mut names = Vec::new();
        for item in list.try_iter()? {
            names.push(item?.getattr("name")?.extract::<String>()?);
        }
        Ok(names)
    }

    /// Create the brain for a robot name, or `None` when the bot has no binding.
    ///
    /// `AI.createAIForRobot` looks the name up in `Bindings.py`'s table and
    /// raises when there is no entry — a custom robot, or a stock one the table
    /// does not cover — which is a fallback case for the caller, not an error.
    pub fn create_ai(&self, py: Python<'_>, bot_name: &str, bot: i32) -> Option<Py<PyAny>> {
        let ai = self.import(py, "AI").ok()?;
        let obj = match ai.call_method1("createAIForRobot", (bot_name,)) {
            Ok(o) => o,
            Err(e) => {
                e.restore(py);
                let _ = PyErr::fetch(py);
                crate::state()
                    .log
                    .push(format!("no AI binding for {bot_name:?}"));
                return None;
            }
        };
        if obj.is_none() {
            return None;
        }
        obj.call_method1("__ra2_bind__", (bot,)).ok()?;
        Some(obj.unbind())
    }

    /// Boot, then load the game's arenas and its shipped AI.
    ///
    /// The order matters: `AI/__init__.py` reads `Arenas`, and a brain asks
    /// `Arenas.currentArena` for its navigation map, so the arenas come first and
    /// `arena_script` (when the match was given one) is instantiated before any
    /// brain is built.
    pub fn boot_with_scripts(
        root: impl AsRef<Path>,
        arena_script: Option<&Path>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let host = ScriptHost::boot(root)?;
        Python::with_gil(|py| -> Result<(), Box<dyn std::error::Error>> {
            host.load_arenas(py)?;
            if let Some(script) = arena_script {
                if let Err(e) = host.create_arena_for_script(py, script) {
                    eprintln!("ra2script: {}: {e}", script.display());
                }
            }
            // Hazards are a match option in the game, and on by default here;
            // `RA2_HAZARDS=0` runs the arena without them.
            let hazards = std::env::var("RA2_HAZARDS")
                .map(|v| v != "0")
                .unwrap_or(true);
            if hazards && arena_script.is_some() {
                Python::with_gil(|py| {
                    let _ = host.hazards_on(py, true);
                });
            }
            host.load_ai(py)?;
            // The match begins: the arenas' own `Activate`.
            host.activate_arena(py, true)?;
            Ok(())
        })?;
        Ok(host)
    }

    /// Build and keep a brain for a robot, if `Bindings.py` knows the name.
    pub fn add_brain(&mut self, bot_name: &str, bot: i32) {
        Python::with_gil(|py| {
            let Some(brain) = self.create_ai(py, bot_name, bot) else {
                return;
            };
            if let Err(e) = brain.bind(py).call_method1("Activate", (true,)) {
                eprintln!("ra2script: activating the {bot_name} brain: {e}");
            }
            self.brains.push((bot.max(0) as usize, brain));
        });
    }

    /// How many robots the shipped AI is driving.
    pub fn brain_count(&self) -> usize {
        self.brains.len()
    }

    /// Run every brain one tick and read back its drive, `(robot, (throttle,
    /// steer))`.
    pub fn tick_brains(&self) -> Vec<(usize, (f32, f32))> {
        Python::with_gil(|py| {
            self.brains
                .iter()
                .filter_map(|(i, brain)| {
                    if let Err(e) = brain.bind(py).call_method0("Tick") {
                        report_once(&format!("brain {i} Tick failed: {e}"));
                        return None;
                    }
                    let d = ra2script_read_drive(brain, py);
                    if d.is_none() {
                        report_once(&format!("brain {i} is not a plus.AI"));
                    }
                    d.map(|d| (*i, d))
                })
                .collect()
        })
    }

    /// Read the channels each brain last set, without running one.
    ///
    /// A brain is designed for an eighth of a second per tick (see
    /// [`crate::AI_TICK_INTERVAL`]), while the physics runs many times per tick;
    /// in between, the controls simply hold.
    pub fn read_drives(&self) -> Vec<(usize, (f32, f32))> {
        Python::with_gil(|py| {
            self.brains
                .iter()
                .filter_map(|(i, brain)| ra2script_read_drive(brain, py).map(|d| (*i, d)))
                .collect()
        })
    }

    /// `Arenas.<script>`'s arena class, instantiated and made current.
    ///
    /// The arena scripts are addressed by file (`Arenas/box.py`), the game by
    /// name (`"Combat Arena"`); the class's `__module__` is the bridge, and the
    /// class then goes through the same `plus.setArena` path the game's picker
    /// uses, so `Arenas.currentArena` — which the AI reads for its pathfinding —
    /// is the scripted one.
    pub fn create_arena_for_script(
        &self,
        py: Python<'_>,
        script: &Path,
    ) -> Result<Option<Py<PyAny>>, Box<dyn std::error::Error>> {
        let stem = match script.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => return Ok(None),
        };
        let module = format!("Arenas.{stem}");
        let arenas = self.import(py, "Arenas")?;
        let classes = arenas.getattr("arenas")?;
        let mut chosen: Option<Bound<'_, PyAny>> = None;
        for item in classes.try_iter()? {
            let item = item?;
            let name: String = item.getattr("__module__")?.extract()?;
            if name == module {
                chosen = Some(item);
                break;
            }
        }
        let Some(class) = chosen else {
            return Ok(None);
        };
        let instance = class.call0()?;
        let plus = self.import(py, "plus")?;
        plus.call_method1("setArena", (instance.clone(),))?;
        arenas.setattr("currentArena", instance.clone())?;
        crate::state().arena_object = Some(instance.clone().unbind());
        instance.call_method0("AddShadowReceivers")?;
        Ok(Some(instance.unbind()))
    }

    /// Enable the live arena's hazards (`HazardsOn(True)`).
    pub fn hazards_on(&self, py: Python<'_>, on: bool) -> PyResult<bool> {
        let arena = crate::state()
            .arena_object
            .as_ref()
            .map(|a| a.clone_ref(py));
        match arena {
            Some(a) => a.bind(py).call_method1("HazardsOn", (on,))?.extract(),
            None => Ok(false),
        }
    }

    /// Tell the arena the match has begun, or that it is over (`Activate`).
    ///
    /// The arenas do their start-of-match work here, and some of it matters:
    /// the Octagon and the Skull only open their trap floors from
    /// `PitSlider.setactive(True)` in `Activate`, King of the Hill reads the game
    /// type, and the base class retires its intro generator.
    pub fn activate_arena(&self, py: Python<'_>, on: bool) -> PyResult<()> {
        let arena = crate::state()
            .arena_object
            .as_ref()
            .map(|a| a.clone_ref(py));
        match arena {
            Some(a) => {
                a.bind(py).call_method1("Activate", (on,))?;
                Ok(())
            }
            None => Ok(()),
        }
    }

    /// Call the arena's `Tick` (GIL handled here, so callers need no `pyo3`).
    /// The clock is [`crate::publish`]'s.
    ///
    /// A Python error is returned, with its traceback, rather than swallowed:
    /// an arena whose `Tick` raises is a fault that must be visible. The
    /// simulator logs the first one and keeps stepping the physics, but does
    /// not pretend the tick succeeded.
    pub fn run_arena_tick(&self, dt: f32) -> Result<bool, String> {
        Python::with_gil(|py| {
            self.tick_arena(py, dt)
                .map_err(|e| describe_python_error(py, e))
        })
    }

    /// Tell the arena a robot entered or left one of its zones, without the
    /// caller needing the GIL.
    pub fn send_zone_event(&self, direction: i32, zone: i32, robot: i32, chassis: bool) -> bool {
        Python::with_gil(|py| {
            self.zone_event(py, direction, zone, robot, chassis)
                .unwrap_or(true)
        })
    }

    /// Call the arena's `Tick`. The clock is [`crate::publish`]'s.
    pub fn tick_arena(&self, py: Python<'_>, _dt: f32) -> PyResult<bool> {
        let arena = crate::state()
            .arena_object
            .as_ref()
            .map(|a| a.clone_ref(py));
        match arena {
            Some(a) => a.bind(py).call_method0("Tick")?.extract(),
            None => Ok(true),
        }
    }

    /// Dispatch a SmartZone event to the arena (`Tick`'s counterpart).
    pub fn zone_event(
        &self,
        py: Python<'_>,
        direction: i32,
        zone: i32,
        robot: i32,
        chassis: bool,
    ) -> PyResult<bool> {
        let arena = crate::state()
            .arena_object
            .as_ref()
            .map(|a| a.clone_ref(py));
        match arena {
            Some(a) => a
                .bind(py)
                .call_method1("ZoneEvent", (direction, zone, robot, chassis))?
                .extract(),
            None => Ok(true),
        }
    }
}

fn ra2script_read_drive(brain: &Py<PyAny>, py: Python<'_>) -> Option<(f32, f32)> {
    crate::plus::read_drive(brain, py)
}

/// A Python exception rendered with its traceback.
///
/// `PyErr`'s `Display` is only `Type: message`; the script and line come from
/// `traceback.format_exception`, which is what the operator needs when an
/// arena's `Tick` is the thing that broke.
fn describe_python_error(py: Python<'_>, e: PyErr) -> String {
    let formatted = py.import("traceback").and_then(|tb| {
        tb.call_method1(
            "format_exception",
            (e.get_type(py), e.value(py), e.traceback(py)),
        )
    });
    match formatted {
        Ok(lines) => match lines.extract::<Vec<String>>() {
            Ok(lines) => lines.concat(),
            Err(_) => e.to_string(),
        },
        Err(_) => e.to_string(),
    }
}

/// Report a script fault once. A broken brain fails every tick, and a duel runs
/// hundreds of them.
fn report_once(message: &str) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static REPORTED: AtomicBool = AtomicBool::new(false);
    if !REPORTED.swap(true, Ordering::Relaxed) {
        eprintln!("ra2script: {message}");
    }
}

/// Install the translating loader for the game's Python-2 sources.
fn install_legacy(py: Python<'_>, root: &Path) -> PyResult<()> {
    let module = PyModule::from_code(
        py,
        c_str!(include_str!("legacy.py")),
        c_str!("ra2_legacy.py"),
        c_str!("ra2_legacy"),
    )?;
    // `Scripts/Lib` is the shipped Python 2.2 standard library and is excluded:
    // the host's own is used instead, behind `legacy.py`'s shim.
    let roots: Vec<String> = ["Arenas", "AI", "Scripts"]
        .iter()
        .map(|d| root.join(d).to_string_lossy().to_string())
        .collect();
    let excludes: Vec<String> = vec![root
        .join("Scripts")
        .join("Lib")
        .to_string_lossy()
        .to_string()];
    module.call_method1(
        "install",
        (PyTuple::new(py, roots)?, PyTuple::new(py, excludes)?),
    )?;
    Ok(())
}
