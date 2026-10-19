//! The `plus.Arena` object model.
//!
//! Every stock arena is a Python subclass of `plus.Arena`, and everything it
//! configures — shadow receivers, cameras, extra props, hazards, SmartZone ids,
//! the navigation map — it configures through *this* class. The original's C++
//! `Arena` is the engine side of that conversation; this is the same surface,
//! backed by the native engine.
//!
//! The method list is not invented: it is exactly the set the shipped scripts
//! call (see `docs/06-deferred-fidelity.md`, F13). Where a call belongs to the
//! presentation layer that lands in M5 (sounds, lightning) it is
//! recorded rather than performed, and says so once.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use glam::Vec3;
use pyo3::exceptions::PyIndexError;
use pyo3::prelude::*;

/// What the navigation grid stores per cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    /// A wall: impassable.
    pub wall: bool,
    /// A hazard: recorded for the map file and the AI's hazard avoidance.
    pub hazard: bool,
    /// A floor was found when the map was baked.
    pub floor: bool,
    /// Height above the map's altitude, in the file's tenths of a metre.
    pub height: i32,
}

/// The arena's navigation map.
///
/// `Arenas/SuperArena.LoadMap` reads `<model>.map` and feeds it here through
/// `SetMapSize`/`SetMapResolution`/`SetMapAltitude`/`SetMapHeight`/
/// `SetMapWall`/`SetMapHazard`, and `CreateMap` re-bakes it with `RayTest` when
/// no file exists. Either way the AI paths over this grid.
#[derive(Clone, Debug, Default)]
pub struct Map {
    pub cols: i32,
    pub rows: i32,
    pub resolution: f32,
    pub altitude: f32,
    /// `(x0, z0, x1, z1)` as the scripts write it.
    pub extent: [f32; 4],
    pub cells: Vec<Cell>,
    pub hazards_on: bool,
    /// `SetMapHazardHeight`, in the same tenths.
    pub hazard_height: i32,
}

impl Map {
    fn index(&self, i: i32, j: i32) -> Option<usize> {
        if i < 0 || j < 0 || i >= self.cols || j >= self.rows {
            return None;
        }
        let idx = (j * self.cols + i) as usize;
        (idx < self.cells.len()).then_some(idx)
    }

    fn get(&self, i: i32, j: i32) -> Cell {
        self.index(i, j).map(|k| self.cells[k]).unwrap_or_default()
    }

    /// World position of a grid cell.
    pub fn from_grid(&self, i: i32, j: i32) -> Vec3 {
        let x = self.extent[0] + i as f32 * self.resolution;
        let z = self.extent[1] - j as f32 * self.resolution;
        let y = self.altitude + self.get(i, j).height as f32 * 0.1;
        Vec3::new(x, y, z)
    }

    /// Grid cell containing a world position.
    pub fn to_grid(&self, p: Vec3) -> (i32, i32) {
        let i = ((p.x - self.extent[0]) / self.resolution).floor() as i32;
        let j = ((self.extent[1] - p.z) / self.resolution).floor() as i32;
        (i, j)
    }

    /// A cell the AI may occupy: inside the grid, with a floor, no wall.
    fn passable(&self, i: i32, j: i32) -> bool {
        self.index(i, j).is_some() && {
            let c = self.get(i, j);
            !c.wall && c.floor
        }
    }

    /// Number of `resolution` steps that must be clear of walls around a cell.
    fn clearance_cells(&self, radius: f32) -> i32 {
        if self.resolution <= 0.0 {
            return 0;
        }
        (radius / self.resolution).ceil() as i32
    }

    fn clear(&self, i: i32, j: i32, cells: i32) -> bool {
        for dj in -cells..=cells {
            for di in -cells..=cells {
                if self.get(i + di, j + dj).wall {
                    return false;
                }
            }
        }
        true
    }
}

/// A hazard object the arena registered (`AddHazard`).
pub struct HazardRef {
    pub object: Py<PyAny>,
    pub location: Vec3,
}

/// An obstacle the script placed (`AddXtra` / `AddXtraSound`).
#[derive(Clone, Debug)]
pub struct Xtra {
    pub name: String,
    pub gmf: String,
    pub material: String,
    pub sound: Option<String>,
}

/// A camera the script placed.
#[derive(Clone, Debug)]
pub struct StaticCamera {
    pub name: String,
    pub position: [f32; 3],
    pub rotation: [f32; 2],
    pub fov: f32,
}

#[derive(Clone, Debug)]
pub struct WatchCamera {
    pub name: String,
    pub position: [f32; 3],
    pub range: [f32; 4],
}

/// A phantom volume the model carries (`GetRobotsInPhantom`).
///
/// The KOTH scoring zone is a `.gmf` `GEOMOBJECT` named `King_Zone` (hilltop)
/// or `kingzone` (LumaZone); it is not a registered SmartZone — `king.py:85`
/// leaves its `RegisterZone` commented out — so the engine answers the script's
/// phantom query from the model's own object geometry. The volume is the
/// object's mesh bounding box in its own frame; the transform places it.
#[derive(Clone, Debug)]
pub struct PhantomVolume {
    pub inv: glam::Mat4,
    pub min: Vec3,
    pub max: Vec3,
}

impl PhantomVolume {
    /// Whether a sphere of `radius` around `centre` touches the volume.
    ///
    /// The same test the registered SmartZones use: the stock phantom objects
    /// are eight-vertex boxes, for which this is exact.
    pub fn touches(&self, centre: Vec3, radius: f32) -> bool {
        let p = self.inv.transform_point3(centre);
        let r = Vec3::splat(radius);
        (p.cmpge(self.min - r) & p.cmple(self.max + r)).all()
    }
}

/// A 2D wall segment the script adds to the navigation map.
#[derive(Clone, Copy, Debug)]
pub struct CollisionLine {
    pub a: [f32; 2],
    pub b: [f32; 2],
}

/// A navigation vantage point (`AddPOV`).
#[derive(Clone, Debug)]
pub struct Pov {
    pub index: i32,
    pub location: [f32; 2],
    pub neighbours: Vec<i32>,
}

/// Everything an arena script configures.
#[derive(Default)]
pub struct ArenaData {
    /// The model path exactly as the script wrote it.
    pub model: String,
    /// Start points from the model, resolved at load.
    pub start_points: Vec<(Vec3, Vec3)>,
    /// Every named object in the model (hazards name their parts).
    pub object_names: Vec<String>,
    /// Triangles for `RayTest` (used by `CreateMap`).
    pub triangles: Vec<[Vec3; 3]>,
    pub xtras: Vec<Xtra>,
    pub shadow_source: Option<[f32; 3]>,
    pub shadow_primitive: Option<String>,
    pub shadow_triangles: Vec<[[f32; 3]; 3]>,
    pub static_cameras: Vec<StaticCamera>,
    pub watch_cameras: Vec<WatchCamera>,
    /// `RegisterZone(name, id)` — the SmartZone names and their ids.
    pub zones: BTreeMap<String, i32>,
    /// The model's phantom volumes, keyed lower-case for the case-insensitive
    /// lookup `GetRobotsInPhantom` does (`King_Zone` vs `kingzone`).
    pub phantoms: BTreeMap<String, PhantomVolume>,
    pub hazards: Vec<HazardRef>,
    pub collision_lines: Vec<CollisionLine>,
    pub povs: Vec<Pov>,
    pub map: Map,
    pub extent: [f32; 4],
    /// Objects the script pinned or deactivated, by name.
    pub pinned: BTreeMap<String, bool>,
    pub active: BTreeMap<String, bool>,
    pub sub_materials: Vec<(String, String, f32, Option<String>)>,
    pub lightning: Vec<(i32, [f32; 3], [f32; 3], bool)>,
    /// Warnings for calls that belong to a later milestone.
    pub deferred: Vec<String>,
}

impl ArenaData {
    pub fn defer(&mut self, what: &str) {
        if !self.deferred.iter().any(|w| w == what) {
            self.deferred.push(what.to_string());
        }
    }

    /// Nearest grid cell the AI can stand on, by world position.
    fn nearest_passable(&self, i: i32, j: i32) -> Option<(i32, i32)> {
        if self.map.passable(i, j) {
            return Some((i, j));
        }
        for r in 1..=(self.map.cols.max(self.map.rows) + 1) {
            let mut best: Option<(i32, i32, f32)> = None;
            for dj in -r..=r {
                for di in -r..=r {
                    if di.abs() != r && dj.abs() != r {
                        continue;
                    }
                    let (a, b) = (i + di, j + dj);
                    if self.map.passable(a, b) {
                        let d = (di * di + dj * dj) as f32;
                        if best.map(|(_, _, bd)| d < bd).unwrap_or(true) {
                            best = Some((a, b, d));
                        }
                    }
                }
            }
            if let Some((a, b, _)) = best {
                return Some((a, b));
            }
        }
        None
    }

    /// A* over the navigation grid, 8-connected, no corner cutting.
    ///
    /// Returns grid cells from start to goal, or empty when no route exists. The
    /// AI calls `FromGrid` on each, so this is in grid space by design.
    pub fn path(&self, from: Vec3, to: Vec3, radius: f32) -> Vec<(i32, i32)> {
        let (si, sj) = self.map.to_grid(from);
        let (gi, gj) = self.map.to_grid(to);
        let Some(start) = self.nearest_passable(si, sj) else {
            return Vec::new();
        };
        let Some(goal) = self.nearest_passable(gi, gj) else {
            return Vec::new();
        };
        let cells = self.map.clearance_cells(radius);
        let (cols, rows) = (self.map.cols, self.map.rows);
        let idx = |i: i32, j: i32| (j * cols + i) as usize;
        let n = (cols * rows).max(1) as usize;

        let h = |i: i32, j: i32| {
            let (dx, dy) = ((i - goal.0).abs() as f32, (j - goal.1).abs() as f32);
            // Octile distance: the cost of 8-connected travel.
            (dx.max(dy)) + (std::f32::consts::SQRT_2 - 1.0) * (dx.min(dy))
        };

        let mut g = vec![f32::INFINITY; n];
        let mut came = vec![usize::MAX; n];
        let mut open: Vec<(f32, i32, i32)> = vec![(h(start.0, start.1), start.0, start.1)];
        g[idx(start.0, start.1)] = 0.0;

        let neighbours: [(i32, i32, f32); 8] = [
            (1, 0, 1.0),
            (-1, 0, 1.0),
            (0, 1, 1.0),
            (0, -1, 1.0),
            (1, 1, std::f32::consts::SQRT_2),
            (1, -1, std::f32::consts::SQRT_2),
            (-1, 1, std::f32::consts::SQRT_2),
            (-1, -1, std::f32::consts::SQRT_2),
        ];

        while !open.is_empty() {
            let mut best = 0;
            for k in 1..open.len() {
                if open[k].0 < open[best].0 {
                    best = k;
                }
            }
            let (_, ci, cj) = open.swap_remove(best);
            if (ci, cj) == goal {
                let mut out = Vec::new();
                let mut cur = idx(ci, cj);
                while cur != usize::MAX {
                    out.push(((cur % cols as usize) as i32, (cur / cols as usize) as i32));
                    cur = came[cur];
                }
                out.reverse();
                return out;
            }
            let cg = g[idx(ci, cj)];
            for (di, dj, cost) in neighbours {
                let (ni, nj) = (ci + di, cj + dj);
                if !self.map.passable(ni, nj) || !self.map.clear(ni, nj, cells) {
                    continue;
                }
                if di != 0 && dj != 0 {
                    // No cutting a corner between two walls.
                    if !self.map.passable(ci + di, cj) || !self.map.passable(ci, cj + dj) {
                        continue;
                    }
                }
                let ng = cg + cost;
                let k = idx(ni, nj);
                if ng + 1e-6 < g[k] {
                    g[k] = ng;
                    came[k] = idx(ci, cj);
                    open.push((ng + h(ni, nj), ni, nj));
                }
            }
        }
        Vec::new()
    }

    /// Whether a straight line between two world points stays on the map.
    pub fn straight_path_clear(&self, from: Vec3, to: Vec3, radius: f32) -> bool {
        let cells = self.map.clearance_cells(radius);
        let steps = ((to - from).length() / (self.map.resolution.max(0.05) * 0.5)).ceil() as i32;
        for s in 0..=steps.max(1) {
            let t = s as f32 / steps.max(1) as f32;
            let (i, j) = self.map.to_grid(from.lerp(to, t));
            if !self.map.passable(i, j) || !self.map.clear(i, j, cells) {
                return false;
            }
        }
        true
    }

    /// Distance from `p` to the nearest cell the AI cannot stand on.
    ///
    /// Returns `(distance, over, height)`. `over` is true when `p` is already
    /// off the drivable map — the two tabletop tactics read exactly that.
    pub fn distance_to_edge(&self, p: Vec3) -> (f32, bool, f32) {
        let (ci, cj) = self.map.to_grid(p);
        let here = self.map.get(ci, cj);
        let over = !self.map.passable(ci, cj);
        let mut best = f32::INFINITY;
        let reach = self.map.cols.max(self.map.rows);
        for dj in -reach..=reach {
            for di in -reach..=reach {
                if self.map.passable(ci + di, cj + dj) {
                    continue;
                }
                let a = self.map.from_grid(ci + di, cj + dj);
                let d = ((a.x - p.x).powi(2) + (a.z - p.z).powi(2)).sqrt();
                if d < best {
                    best = d;
                }
            }
        }
        let height = self.map.altitude + here.height as f32 * 0.1;
        (if best.is_finite() { best } else { 0.0 }, over, height)
    }

    /// Heading, in radians, that points away from the nearest edge.
    pub fn heading_away_from_edge(&self, p: Vec3) -> f32 {
        let (ci, cj) = self.map.to_grid(p);
        let mut best: Option<(f32, Vec3)> = None;
        let reach = self.map.cols.max(self.map.rows);
        for dj in -reach..=reach {
            for di in -reach..=reach {
                if self.map.passable(ci + di, cj + dj) {
                    continue;
                }
                let a = self.map.from_grid(ci + di, cj + dj);
                let d = ((a.x - p.x).powi(2) + (a.z - p.z).powi(2)).sqrt();
                if best.map(|(bd, _)| d < bd).unwrap_or(true) {
                    best = Some((d, a));
                }
            }
        }
        match best {
            Some((_, e)) => {
                // The scripts build a vector with `vector3(angle)`, which is
                // `(sin a, 0, cos a)`, so the heading is `atan2(x, z)` of the
                // away-from-edge direction.
                let d = Vec3::new(p.x - e.x, 0.0, p.z - e.z);
                f32::atan2(d.x, d.z)
            }
            None => 0.0,
        }
    }
}

/// `plus.Arena` — the C++ side of every arena script.
///
/// A Python subclass (`Arenas.SuperArena`) drives it, so the base methods are
/// the ones the scripts call through `plus.Arena.<name>(self, ...)`; the
/// overridable ones are no-ops here and the Python layer supplies the behaviour.
#[pyclass(subclass, name = "Arena", module = "plus")]
pub struct Arena {
    pub data: Arc<Mutex<ArenaData>>,
    /// Navigation clearance the AI asked for (`SetSearchRadius`).
    pub search_radius: f32,
    pub hazards_on: bool,
}

impl Arena {
    fn d(&self) -> std::sync::MutexGuard<'_, ArenaData> {
        self.data.lock().unwrap()
    }
}

#[pymethods]
impl Arena {
    #[new]
    #[pyo3(signature = (model=None))]
    fn new(model: Option<String>) -> Self {
        let mut arena = Arena {
            data: Arc::new(Mutex::new(ArenaData::default())),
            search_radius: 1.0,
            hazards_on: false,
        };
        // The live arena, for the engine's side of `plus` and for tests.
        crate::state().arena = Some(Arc::clone(&arena.data));
        if let Some(m) = model {
            arena.load(&m);
        }
        arena
    }

    /// `plus.Arena.__init__(self, modelfile)`.
    #[pyo3(signature = (modelfile))]
    fn __init__(&mut self, modelfile: &str) {
        self.load(modelfile);
    }

    // -- engine-side surface the scripts call -------------------------------

    /// `AddXtra(name, gmf, material)` / `AddXtraSound(name, gmf, material, sound)`.
    #[pyo3(signature = (name, gmf, material, sound=None))]
    fn AddXtra(&self, name: &str, gmf: &str, material: &str, sound: Option<String>) {
        self.d().xtras.push(Xtra {
            name: name.to_string(),
            gmf: gmf.to_string(),
            material: material.to_string(),
            sound,
        });
    }

    #[pyo3(signature = (name, gmf, material, sound))]
    fn AddXtraSound(&self, name: &str, gmf: &str, material: &str, sound: &str) {
        self.AddXtra(name, gmf, material, Some(sound.to_string()));
    }

    /// `RemoveXtra(name)` — the practice arena swaps its obstacle.
    fn RemoveXtra(&self, name: &str) {
        self.d().xtras.retain(|x| x.name != name);
    }

    fn SetShadowSource(&self, x: f32, y: f32, z: f32) {
        self.d().shadow_source = Some([x, y, z]);
    }

    fn SetShadowPrimitive(&self, name: &str) {
        self.d().shadow_primitive = Some(name.to_string());
    }

    /// `AddShadowTriangle(a, b, c)` — one flat receiver, three points.
    ///
    /// The arenas hand it `(x, y, z)` tuples; `BoxArena` adds ten of them for
    /// the floor and walls.
    fn AddShadowTriangle(&self, a: (f32, f32, f32), b: (f32, f32, f32), c: (f32, f32, f32)) {
        self.d()
            .shadow_triangles
            .push([[a.0, a.1, a.2], [b.0, b.1, b.2], [c.0, c.1, c.2]]);
    }

    /// `RegisterZone(name, id)` — the SmartZone whose events this arena wants.
    fn RegisterZone(&self, name: &str, id: i32) {
        self.d().zones.insert(name.to_string(), id);
    }

    /// `AddPrismatic(host, child, ax, ay, az, neutral, lower, upper)`.
    ///
    /// Creates the joint the hazard drives, exactly as a piston definition
    /// does (`docs/06`, F8): the axis is in the host's own space.
    #[pyo3(signature = (host, child, ax, ay, az, neutral, lower, upper))]
    #[allow(clippy::too_many_arguments)] // the script's own signature
    fn AddPrismatic(
        &self,
        host: &str,
        child: &str,
        ax: f32,
        ay: f32,
        az: f32,
        neutral: f32,
        lower: f32,
        upper: f32,
    ) -> PrismaticController {
        let drive = register_joint(true, host, child, [ax, ay, az], neutral, lower, upper);
        PrismaticController {
            host: host.to_string(),
            child: child.to_string(),
            axis: [ax, ay, az],
            neutral,
            lower,
            upper,
            drive,
        }
    }

    /// `GetHinge(name)` — a hinge the model already carries.
    ///
    /// The name is the model's own constraint (`clawtop.py` asks for
    /// `Hinge01`); the engine finds it in the arena's solver.
    fn GetHinge(&self, name: &str) -> HingeController {
        let drive = register_joint(false, "", name, [0.0, 1.0, 0.0], 0.0, 0.0, 0.0);
        HingeController {
            name: name.to_string(),
            drive,
        }
    }

    fn SetSubMaterial(&self, name: &str, material: &str, volume: f32) {
        self.d()
            .sub_materials
            .push((name.to_string(), material.to_string(), volume, None));
    }

    fn SetSubMaterialSound(&self, name: &str, material: &str, volume: f32, sound: &str) {
        self.d().sub_materials.push((
            name.to_string(),
            material.to_string(),
            volume,
            Some(sound.to_string()),
        ));
    }

    /// `SetActive(name, on)` — the cinderblock arenas hide and show props:
    /// not there at all until a trap door opens over the pit.
    ///
    /// The simulator reads these as the *wanted* state and reconciles the
    /// world against them every step, so the call is idempotent.
    #[pyo3(signature = (name, on))]
    fn SetActive(&self, name: &str, on: bool) {
        self.d().active.insert(name.to_string(), on);
    }

    /// `SetPinned(name, on)` — the part stops moving where it is.
    /// `flextop.py` pins its flexing floor when the hazards are off.
    #[pyo3(signature = (name, on))]
    fn SetPinned(&self, name: &str, on: bool) {
        self.d().pinned.insert(name.to_string(), on);
    }

    /// `AddCollisionLine((x0, z0), (x1, z1))` — a wall on the navigation map.
    fn AddCollisionLine(&self, a: (f32, f32), b: (f32, f32)) {
        let line = CollisionLine {
            a: [a.0, a.1],
            b: [b.0, b.1],
        };
        let mut d = self.d();
        let res = d.map.resolution.max(0.01);
        let steps = (((line.b[0] - line.a[0]).powi(2) + (line.b[1] - line.a[1]).powi(2)).sqrt()
            / (res * 0.5))
            .ceil() as i32;
        for s in 0..=steps.max(1) {
            let t = s as f32 / steps.max(1) as f32;
            let x = line.a[0] + (line.b[0] - line.a[0]) * t;
            let z = line.a[1] + (line.b[1] - line.a[1]) * t;
            let i = ((x - d.map.extent[0]) / res).floor() as i32;
            let j = ((d.map.extent[1] - z) / res).floor() as i32;
            if let Some(k) = d.map.index(i, j) {
                d.map.cells[k].wall = true;
            }
        }
        d.collision_lines.push(line);
    }

    /// `AddPOV(index, (x, z), (neighbours...))` — the AI's vantage graph.
    #[pyo3(signature = (index, location, neighbours))]
    fn AddPOV(&self, index: i32, location: (f32, f32), neighbours: Vec<i32>) {
        self.d().povs.push(Pov {
            index,
            location: [location.0, location.1],
            neighbours,
        });
    }

    fn CreateLightning(&self, index: i32, a: (f32, f32, f32), b: (f32, f32, f32)) {
        let mut d = self.d();
        d.lightning
            .push((index, [a.0, a.1, a.2], [b.0, b.1, b.2], false));
        d.defer("lightning (M5)");
    }

    fn SetLightningVisible(&self, index: i32, on: bool) {
        let mut d = self.d();
        if let Some(e) = d.lightning.iter_mut().find(|e| e.0 == index) {
            e.3 = on;
        }
    }

    fn SetLightningStartEnd(&self, index: i32, a: (f32, f32, f32), b: (f32, f32, f32)) {
        let mut d = self.d();
        if let Some(e) = d.lightning.iter_mut().find(|e| e.0 == index) {
            e.1 = [a.0, a.1, a.2];
            e.2 = [b.0, b.1, b.2];
        }
    }

    fn SetAllLightningVisible(&self, on: bool) {
        for e in self.d().lightning.iter_mut() {
            e.3 = on;
        }
    }

    fn ResetLightningPositions(&self) {
        self.d().defer("lightning (M5)");
    }

    // -- navigation map -----------------------------------------------------

    fn SetMapSize(&self, cols: i32, rows: i32) {
        let mut d = self.d();
        d.map.cols = cols;
        d.map.rows = rows;
        d.map.cells = vec![Cell::default(); (cols.max(0) * rows.max(0)) as usize];
    }

    fn SetMapResolution(&self, resolution: f32) {
        self.d().map.resolution = resolution;
    }

    fn SetMapAltitude(&self, altitude: f32) {
        self.d().map.altitude = altitude;
    }

    fn SetMapExtent(&self, extent: (f32, f32, f32, f32)) {
        let e = [extent.0, extent.1, extent.2, extent.3];
        let mut d = self.d();
        d.map.extent = e;
        d.extent = e;
    }

    fn SetMapHeight(&self, i: i32, j: i32, height: i32) {
        let mut d = self.d();
        if let Some(k) = d.map.index(i, j) {
            d.map.cells[k].height = height;
            d.map.cells[k].floor = true;
        }
    }

    fn SetMapWall(&self, i: i32, j: i32) {
        let mut d = self.d();
        if let Some(k) = d.map.index(i, j) {
            d.map.cells[k].wall = true;
        }
    }

    fn SetMapHazard(&self, i: i32, j: i32) {
        let mut d = self.d();
        if let Some(k) = d.map.index(i, j) {
            d.map.cells[k].hazard = true;
        }
    }

    fn SetMapHazardHeight(&self, height: i32) {
        self.d().map.hazard_height = height;
    }

    fn SetMapHazardsOn(&self, on: bool) {
        self.d().map.hazards_on = on;
    }

    fn GetMapResolution(&self) -> f32 {
        self.d().map.resolution
    }

    fn GetMapAltitude(&self) -> f32 {
        self.d().map.altitude
    }

    fn GetMapHeight(&self, i: i32, j: i32) -> i32 {
        self.d().map.get(i, j).height
    }

    fn IsMapWall(&self, i: i32, j: i32) -> bool {
        self.d().map.get(i, j).wall
    }

    fn IsMapHazard(&self, i: i32, j: i32) -> bool {
        self.d().map.get(i, j).hazard
    }

    fn IsMapEmpty(&self, i: i32, j: i32) -> bool {
        let c = self.d().map.get(i, j);
        !c.wall && !c.hazard && !c.floor
    }

    fn ToGrid(&self, p: (f32, f32, f32)) -> (i32, i32) {
        self.d().map.to_grid(Vec3::new(p.0, p.1, p.2))
    }

    fn FromGrid(&self, grid: (i32, i32)) -> (f32, f32, f32) {
        let v = self.d().map.from_grid(grid.0, grid.1);
        (v.x, v.y, v.z)
    }

    fn SetSearchRadius(&mut self, radius: f32) {
        self.search_radius = radius;
    }

    /// `GetPath(from_world, to_world, flag)` — grid cells to follow.
    fn GetPath(
        &self,
        from: (f32, f32, f32),
        to: (f32, f32, f32),
        _blocked: bool,
    ) -> Vec<(i32, i32)> {
        let r = self.search_radius;
        self.data.lock().unwrap().path(
            Vec3::new(from.0, from.1, from.2),
            Vec3::new(to.0, to.1, to.2),
            r,
        )
    }

    fn IsStraightPathClear(
        &self,
        from: (f32, f32, f32),
        to: (f32, f32, f32),
        _blocked: bool,
    ) -> bool {
        let r = self.search_radius;
        self.data.lock().unwrap().straight_path_clear(
            Vec3::new(from.0, from.1, from.2),
            Vec3::new(to.0, to.1, to.2),
            r,
        )
    }

    fn DistanceToEdge(&self, p: (f32, f32, f32)) -> (f32, bool, f32) {
        self.data
            .lock()
            .unwrap()
            .distance_to_edge(Vec3::new(p.0, p.1, p.2))
    }

    fn HeadingAwayFromEdge(&self, p: (f32, f32, f32)) -> f32 {
        self.data
            .lock()
            .unwrap()
            .heading_away_from_edge(Vec3::new(p.0, p.1, p.2))
    }

    /// `RayTest((x0,y0,z0), (x1,y1,z1))` -> `(hit, (x, y, z))`.
    ///
    /// `CreateMap` bakes the navigation grid with this when an arena ships no
    /// `.map`, so it has to be the arena's own geometry.
    fn RayTest(&self, a: (f32, f32, f32), b: (f32, f32, f32)) -> (bool, (f32, f32, f32)) {
        let from = Vec3::new(a.0, a.1, a.2);
        let to = Vec3::new(b.0, b.1, b.2);
        let dir = to - from;
        let len = dir.length();
        if len <= 1e-6 {
            return (false, (0.0, 0.0, 0.0));
        }
        let dir = dir / len;
        let d = self.data.lock().unwrap();
        let mut best = f32::INFINITY;
        for tri in &d.triangles {
            if let Some(t) = ray_triangle(from, dir, tri) {
                if t > 1e-4 && t < best {
                    best = t;
                }
            }
        }
        if best.is_finite() && best <= len {
            let p = from + dir * best;
            (true, (p.x, p.y, p.z))
        } else {
            (false, (0.0, 0.0, 0.0))
        }
    }

    fn GetStartPointLocation(&self, index: usize) -> PyResult<(f32, f32, f32)> {
        let d = self.d();
        d.start_points
            .get(index)
            .map(|(p, _)| (p.x, p.y, p.z))
            .ok_or_else(|| PyIndexError::new_err("start point index out of range"))
    }

    fn GetStartPointFacing(&self, index: usize) -> (f32, f32) {
        let d = self.d();
        match d.start_points.get(index) {
            Some((_, f)) => (f.x, f.z),
            None => (0.0, 0.0),
        }
    }

    /// `GetRobotsInPhantom(name)` — who is standing in a named phantom zone.
    ///
    /// `king.py:56` is the caller: the scoring zone, reported as **1-based**
    /// robot ids (line 57 subtracts one). The simulator publishes the contents
    /// every step; a name the arena's model does not carry is empty.
    fn GetRobotsInPhantom(&self, name: &str) -> Vec<i32> {
        let st = crate::state();
        match st.zone_contents.get(name) {
            Some(v) => v.iter().map(|(robot, _)| *robot).collect(),
            None => st
                .zone_contents
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.iter().map(|(robot, _)| *robot).collect())
                .unwrap_or_default(),
        }
    }

    /// `GetZoneContents(zone)` — `(robot, chassis)` readings for a SmartZone.
    ///
    /// The ids are 1-based, as `AI.SuperAI.SmartZoneEvent` expects
    /// (`AI/__init__.py:417` does `r = robot - 1`).
    fn GetZoneContents(&self, zone: &str) -> Vec<(i32, bool)> {
        let st = crate::state();
        match st.zone_contents.get(zone) {
            Some(v) => v.clone(),
            None => st
                .zone_contents
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(zone))
                .map(|(_, v)| v.clone())
                .unwrap_or_default(),
        }
    }

    /// `HazardsOn(on)` — the Python layer re-implements this; the base records it.
    #[pyo3(signature = (on))]
    fn HazardsOn(&mut self, on: bool) -> bool {
        self.hazards_on = on;
        self.d().map.hazards_on = on;
        true
    }

    // -- overridable no-ops the scripts call through the base ---------------

    fn AddShadowReceivers(&self) {}

    fn Activate(&self, _on: bool) {}

    fn Winner(&self, _a: i32, _b: i32) {}

    fn Tick(&self) -> bool {
        true
    }

    /// The engine's arena tick period, in seconds.
    ///
    /// Every arena reads `self.tickInterval` (`Arenas/__init__.py:94`,
    /// `king.py:40,47`). It is the **0.25 s** the hazards already count in:
    /// `Hazards.Flame` adds 0.25 four times for a one-second flame and
    /// `Hazards.Electricity` counts a 5 s charge down by 0.25.
    #[getter]
    fn tickInterval(&self) -> f32 {
        crate::ARENA_TICK_INTERVAL
    }

    fn DoIntro(&self, _on: bool) {}

    fn Introduction(&self) {}

    fn NextStaticCamera(&self) -> String {
        String::new()
    }

    fn NextWatchCamera(&self) -> String {
        String::new()
    }

    fn NextAutoCamera(&self) -> String {
        String::new()
    }

    fn SaveMap(&self, _filename: &str) {}
}

impl Arena {
    /// Record the model path and everything the engine needs from it.
    pub fn load(&mut self, model: &str) {
        let mut d = self.data.lock().unwrap();
        d.model = model.to_string();
        drop(d);

        let root = crate::game_root();
        let Some(root) = root else { return };
        let full = root.join(model.replace('/', std::path::MAIN_SEPARATOR_STR));
        let Ok(built) = crate::host::load_arena_model(&full) else {
            return;
        };
        let mut d = self.data.lock().unwrap();
        d.start_points = built.start_points;
        d.object_names = built.object_names;
        d.triangles = built.triangles;
        d.phantoms = built.phantoms;
        if d.extent == [0.0; 4] {
            d.extent = built.extent;
            d.map.extent = built.extent;
        }
    }
}

/// Möller–Trumbore.
fn ray_triangle(origin: Vec3, dir: Vec3, tri: &[Vec3; 3]) -> Option<f32> {
    const EPS: f32 = 1e-7;
    let e1 = tri[1] - tri[0];
    let e2 = tri[2] - tri[0];
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < EPS {
        return None;
    }
    let inv = 1.0 / det;
    let t = origin - tri[0];
    let u = t.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    Some(e2.dot(q) * inv)
}

// ---------------------------------------------------------------------------
// controllers
// ---------------------------------------------------------------------------

/// The settings a hazard applies to a joint, live.
///
/// A script mutates this through the controller object and the engine reads it
/// every step, so the two share one `Arc<Mutex<..>>` rather than passing copies.
///
/// The numbers are the script's own. `SetPowerSettings` scales the torque by
/// **40** in the engine (`docs/06`, F3), so the scale is applied where the joint
/// is built, not here.
#[derive(Clone, Debug, Default)]
pub struct Drive {
    /// Target speed — rad/s for a hinge, m/s for a slider.
    pub speed: f32,
    /// The script's torque, before the engine's ×40.
    pub torque: f32,
    /// `SetDirection`: a sign selector (`1`, `-1` or the scripts' `±100`).
    pub direction: f32,
    /// `Lock(True)` parks it: no motor, held by the joint's friction.
    pub locked: bool,
    /// `SetAutoLock(x)` — hold the joint at the end of its travel.
    ///
    /// A hinge's `SetAutoLocks(a, b)` sets the two halves separately: `a` is
    /// this one and `b` is [`auto_lock_idle`](Self::auto_lock_idle). The engine
    /// keeps them apart because the scripts use them for different things — the
    /// Compressor's smash arms grip at whichever stop they arrive at, while the
    /// Clawtop claws only hold once their motor stops.
    pub auto_lock_stops: bool,
    /// `SetAutoLock(x)` / `SetAutoLocks(_, b)` — hold the joint where it is
    /// while its motor is stopped, so a floor does not sag under the robots
    /// standing on it before its script sets a direction.
    pub auto_lock_idle: bool,
    pub pinned: bool,
    pub limits: Option<(f32, f32)>,
    /// A one-shot `ApplyForce` (N·s at the joint's axis), cleared once applied.
    pub pending_force: f32,
    /// A one-shot `ApplyTorque`, cleared once applied.
    pub pending_torque: f32,
}

impl Drive {
    /// The target velocity the engine should hold, or `0` for "motor off".
    ///
    /// `SetDirection` is a sign selector (`PitSlider` passes `1`, `Hammer`
    /// passes `±100`), so only its sign matters; with no direction the joint is
    /// left free, which is what the hazards' `Shutdown` asks for.
    pub fn target_velocity(&self) -> f32 {
        if self.locked || self.direction == 0.0 {
            return 0.0;
        }
        self.speed * self.direction.signum()
    }

    /// Take the pending one-shot impulse, if any.
    pub fn take_force(&mut self) -> f32 {
        std::mem::take(&mut self.pending_force)
    }

    pub fn take_torque(&mut self) -> f32 {
        std::mem::take(&mut self.pending_torque)
    }
}

/// A joint a script asked the engine for.
///
/// `AddPrismatic` and `GetHinge` only *record* the request: the engine builds
/// the joint at the top of the next step, by which time every hazard's
/// `__init__` has run and its power settings are in place.
#[derive(Clone)]
pub struct JointRequest {
    /// A slider (`AddPrismatic`) or a hinge (`AddHinge`/`GetHinge`).
    pub slider: bool,
    /// Slider: the object the joint is anchored to. Hinge: unused.
    pub host: String,
    /// Slider: the object that moves. Hinge: the model's constraint name.
    pub child: String,
    /// The slide axis, in the host's space (`AddPrismatic`'s `ax, ay, az`).
    pub axis: [f32; 3],
    pub neutral: f32,
    /// Travel limits. The scripts pass them unordered — `box.py` asks for
    /// `(0, .8, 0)`, which is `neutral`, then `0.8`, then `0` — so the engine
    /// takes the min and the max.
    pub lower: f32,
    pub upper: f32,
    pub drive: Arc<Mutex<Drive>>,
}

/// Record a joint request and hand back the settings the script will drive.
fn register_joint(
    slider: bool,
    host: &str,
    child: &str,
    axis: [f32; 3],
    neutral: f32,
    lower: f32,
    upper: f32,
) -> Arc<Mutex<Drive>> {
    let drive = Arc::new(Mutex::new(Drive {
        locked: true,
        ..Drive::default()
    }));
    crate::state().joints.push(JointRequest {
        slider,
        host: host.to_string(),
        child: child.to_string(),
        axis,
        neutral,
        lower,
        upper,
        drive: Arc::clone(&drive),
    });
    drive
}

/// `plus.PrismaticController` — a slider the arena script created.
#[pyclass(name = "PrismaticController", module = "plus")]
#[derive(Clone)]
pub struct PrismaticController {
    pub host: String,
    pub child: String,
    pub axis: [f32; 3],
    pub neutral: f32,
    pub lower: f32,
    pub upper: f32,
    pub drive: Arc<Mutex<Drive>>,
}

impl PrismaticController {
    fn d(&self) -> std::sync::MutexGuard<'_, Drive> {
        self.drive.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[pymethods]
impl PrismaticController {
    fn SetPowerSettings(&self, speed: f32, torque: f32) {
        let mut d = self.d();
        d.speed = speed;
        d.torque = torque;
    }

    fn SetDirection(&self, direction: f32) {
        self.d().direction = direction;
    }

    fn Lock(&self, on: bool) {
        self.d().locked = on;
    }

    fn SetAutoLock(&self, on: bool) {
        let mut d = self.d();
        d.auto_lock_stops = on;
        d.auto_lock_idle = on;
    }

    fn SetLimits(&self, lower: f32, upper: f32) {
        self.d().limits = Some((lower, upper));
    }

    /// `ApplyForce(f)` — one shot, along the joint's own axis.
    fn ApplyForce(&self, force: f32) {
        let mut d = self.d();
        d.pending_force += force;
        d.locked = false;
    }

    fn SetPinned(&self, on: bool) {
        let mut d = self.d();
        d.pinned = on;
        d.locked = on;
    }

    fn SetActive(&self, _on: bool) {}
}

/// `plus.HingeController` — a hinge the arena model already carries.
#[pyclass(name = "HingeController", module = "plus")]
#[derive(Clone)]
pub struct HingeController {
    pub name: String,
    pub drive: Arc<Mutex<Drive>>,
}

impl HingeController {
    fn d(&self) -> std::sync::MutexGuard<'_, Drive> {
        self.drive.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[pymethods]
impl HingeController {
    fn SetPowerSettings(&self, speed: f32, torque: f32) {
        let mut d = self.d();
        d.speed = speed;
        d.torque = torque;
    }

    fn SetDirection(&self, direction: f32) {
        self.d().direction = direction;
    }

    fn Lock(&self, on: bool) {
        self.d().locked = on;
    }

    fn SetAutoLock(&self, on: bool) {
        let mut d = self.d();
        d.auto_lock_stops = on;
        d.auto_lock_idle = on;
    }

    /// `SetAutoLocks(at_stops, when_idle)` — the hinge's two halves separately.
    fn SetAutoLocks(&self, at_stops: bool, when_idle: bool) {
        let mut d = self.d();
        d.auto_lock_stops = at_stops;
        d.auto_lock_idle = when_idle;
    }

    fn SetLimits(&self, lower: f32, upper: f32) {
        self.d().limits = Some((lower, upper));
    }

    /// `ApplyTorque(t)` — one shot, about the hinge's axis.
    fn ApplyTorque(&self, torque: f32) {
        let mut d = self.d();
        d.pending_torque += torque;
        d.locked = false;
    }

    fn SetPinned(&self, on: bool) {
        let mut d = self.d();
        d.pinned = on;
        d.locked = on;
    }

    fn SetActive(&self, _on: bool) {}
}

/// `plus.ArenaDetail` — the record `Arenas.enumAvailable` hands back.
#[pyclass(name = "ArenaDetail", module = "plus")]
#[derive(Clone, Debug, Default)]
pub struct ArenaDetail {
    #[pyo3(get, set)]
    pub name: String,
    #[pyo3(get, set)]
    pub preview: String,
    #[pyo3(get, set)]
    pub description: String,
    #[pyo3(get, set)]
    pub id: i64,
}

/// `plus.AddParticleEmitter(location, velocity, variance)` — the flame hazard's
/// flame (`FUN_004b5760`). The script holds a handle; the emitter's state lives
/// in [`crate::State::emitters`], where the match's particle system reads it.
#[pyclass(name = "ParticleEmitter", module = "plus")]
#[derive(Clone, Debug)]
pub struct ParticleEmitter {
    pub id: usize,
}

/// What the engine sees of one `ParticleEmitter`.
#[derive(Clone, Debug, PartialEq)]
pub struct EmitterState {
    pub location: [f32; 3],
    pub velocity: [f32; 3],
    pub variance: [f32; 3],
    pub emitting: bool,
}

#[pymethods]
impl ParticleEmitter {
    /// `SetEmitting(on)` (`FUN_0048e0b0`).
    fn SetEmitting(&mut self, on: bool) {
        if let Some(e) = crate::state().emitters.get_mut(self.id) {
            e.emitting = on;
        }
    }

    #[getter]
    fn emitting(&self) -> bool {
        crate::state()
            .emitters
            .get(self.id)
            .is_some_and(|e| e.emitting)
    }

    #[setter(emitting)]
    fn set_emitting_attr(&mut self, on: bool) {
        self.SetEmitting(on);
    }
}
