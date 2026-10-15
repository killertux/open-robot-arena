//! Booting the interpreter and loading the game's own scripts.
//!
//! The scripts are the *shipped* ones. They are Python 2.2, so they are loaded
//! through `legacy.py`'s translating loader rather than being rewritten in the
//! repository: the point of the layer is that the behaviour under test is the
//! game's, not our paraphrase of it.

use std::path::{Path, PathBuf};

use glam::Vec3;
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
    if out.triangles.is_empty() {
        // No proxies: fall back to everything the scene draws.
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(scene) = ra2fmt::gmi::parse(&bytes) {
                out.triangles = ra2_render::scene_collision_triangles(&scene, glam::Mat4::IDENTITY);
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
}

impl ScriptHost {
    /// Boot CPython and point it at a game installation.
    ///
    /// `root` is the directory holding `Arenas/`, `AI/`, `Components` and the
    /// rest — the equivalent of the original's working directory.
    pub fn boot(root: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let root = root.as_ref().to_path_buf();
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
            install_legacy(py, &root)?;
            let sys = py.import("sys")?;
            let path: Bound<'_, PyList> = sys.getattr("path")?.downcast_into()?;
            for dir in [root.clone(), root.join("Scripts")] {
                path.insert(0, dir.to_string_lossy().to_string())?;
            }
            Ok(())
        })?;

        Ok(ScriptHost { root })
    }

    /// The game installation the scripts are running from.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Import a module from the game (translating it if it is Python 2).
    pub fn import<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyModule>> {
        PyModule::import(py, name)
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

    /// Call the arena's `Tick`, which the engine does once per frame.
    pub fn tick_arena(&self, py: Python<'_>, dt: f32) -> PyResult<bool> {
        {
            let mut st = crate::state();
            st.time += dt;
            st.dt = dt;
        }
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

/// Install the translating loader for the game's Python-2 sources.
fn install_legacy(py: Python<'_>, root: &Path) -> PyResult<()> {
    let module = PyModule::from_code(
        py,
        c_str!(include_str!("legacy.py")),
        c_str!("ra2_legacy.py"),
        c_str!("ra2_legacy"),
    )?;
    let roots: Vec<String> = ["Arenas", "AI"]
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
