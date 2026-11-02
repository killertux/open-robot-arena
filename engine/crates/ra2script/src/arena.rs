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

    /// Whether a circle of `radius` at `p` (world `x`, `z`) crosses one of the
    /// arena's collision lines (`FUN_004058b0`).
    ///
    /// The original tests every `AddCollisionLine` segment with a
    /// line-against-circle intersection (`FUN_0061b260`): a segment counts when
    /// one of the points where its line crosses the circle lies on the segment,
    /// so a segment wholly inside the circle does not.
    pub fn blocked_at(&self, p: [f32; 2], radius: f32) -> bool {
        self.collision_lines
            .iter()
            .any(|l| segment_crosses_circle(l.a, l.b, p, radius))
    }

    /// How far along the unit `dir` from `from` the arena's own triangles are first struck, within
    /// `len`: what `Arena.RayTest` and the first-hit test of `plus.rayTest` read.
    pub fn first_triangle_hit(&self, from: Vec3, dir: Vec3, len: f32) -> Option<f32> {
        let mut best = f32::INFINITY;
        for tri in &self.triangles {
            if let Some(t) = ray_triangle(from, dir, tri) {
                if t > 1e-4 && t < best {
                    best = t;
                }
            }
        }
        (best.is_finite() && best <= len).then_some(best)
    }

    /// `IsStraightPathClear` (`FUN_00405720`): march from `from` toward `to` in
    /// steps of half the search radius and fail at the first stop whose circle
    /// crosses a collision line. The steps are `trunc(length / step)` and start
    /// at `from`; the end point itself is not tested.
    pub fn straight_path_clear(&self, from: Vec3, to: Vec3, radius: f32) -> bool {
        self.straight_clear_2d([from.x, from.z], [to.x, to.z], radius)
    }

    fn straight_clear_2d(&self, from: [f32; 2], to: [f32; 2], radius: f32) -> bool {
        let d = [to[0] - from[0], to[1] - from[1]];
        let length = (d[0] * d[0] + d[1] * d[1]).sqrt();
        let step = radius * 0.5;
        if step <= 0.0 || length <= 0.0 {
            return !self.blocked_at(from, radius);
        }
        let n = (length / step) as i32;
        let dir = [d[0] / length * step, d[1] / length * step];
        let mut p = from;
        for _ in 0..n {
            if self.blocked_at(p, radius) {
                return false;
            }
            p = [p[0] + dir[0], p[1] + dir[1]];
        }
        true
    }

    /// The march of `FUN_00405720` (and of the A* graph's edges): stops from
    /// `a` toward `b` in steps of half the search radius, `trunc(length /
    /// step)` of them starting at `a` itself, `b` not tested; clear when no
    /// stop's circle crosses a collision line.
    fn march_clear(&self, a: [f32; 2], b: [f32; 2], radius: f32) -> bool {
        self.straight_clear_2d(a, b, radius)
    }

    /// The vantage points as the route sees them: the `AddPOV` locations cut
    /// to whole numbers toward zero (live, POV 11 at `(1.5, -5.3)` is the
    /// waypoint `(1, -5)`, POV 3 at `(-5, 5.5)` is `(-5, 5)`), with their own
    /// neighbour lists as indices into the list.
    fn route_povs(&self) -> Vec<([f32; 2], Vec<usize>)> {
        self.povs
            .iter()
            .map(|p| {
                let at = [p.location[0].trunc(), p.location[1].trunc()];
                let nb = p
                    .neighbours
                    .iter()
                    .filter_map(|&n| self.povs.iter().position(|q| q.index == n))
                    .collect();
                (at, nb)
            })
            .collect()
    }

    /// The native `GetPath(p1, p2, ignore_hazards)` (`FUN_00405990`), as world
    /// `(x, z)` waypoints in the native list's order, `[p2 (moved), ..., p1]`
    /// (the Python method `Arena.GetPath(from, to)` is `FUN_004ba370`: it calls
    /// the native one with `p1 = from`, `p2 = to` and reverses the list, so the
    /// script sees `[from, ..., to']`; docs/29 section 12ah). The "grid" of the
    /// scripts is the world plane: `ToGrid` is `(x, z)` and `FromGrid` puts the
    /// map's floor height back. Recovered, and every one of 5179 live calls in
    /// twelve box fights (`boxfight-*`, `boxstart-*`) reproduced:
    ///
    /// * The straight test marches from `p1` toward `p2` (`FUN_00405720`,
    ///   stops `trunc(length / (radius / 2))` from `p1` on, the end point not
    ///   tested, each stop a circle of the search radius against every
    ///   collision line); clear, the route is `[p2, p1]`.
    /// * Otherwise a `p2` whose own circle crosses a line is moved: the march
    ///   from it toward each vantage point in turn, the first clear stop.
    ///   (The same is attempted for `p1`, but the found point is written into
    ///   the vantage point's temporary, so `p1` never moves.)
    /// * The route is an A* (`FUN_004067b0`, `FUN_00407330`, at most 1000
    ///   expansions) from `p1` to the moved `p2` over the vantage points cut
    ///   to whole numbers ([`Self::route_povs`]), with *squared* distances for
    ///   the steps and the estimate (`FUN_00407890`). A free point's
    ///   successors are the vantage points its march reaches; a vantage
    ///   point's are the goal when its march reaches it, then its own
    ///   neighbours; the parent is skipped (`FUN_00407680`). The open list is
    ///   a binary heap on `f` ([`AStarHeap`]), which settles ties.
    /// * When the A* fails the route is `[moved p2, p1]`: the AI then drives
    ///   straight at its target.
    pub fn path(&self, p1: Vec3, p2: Vec3, radius: f32) -> Vec<[f32; 2]> {
        let (s, g) = ([p2.x, p2.z], [p1.x, p1.z]);
        if self.march_clear(g, s, radius) {
            return vec![s, g];
        }
        let povs = self.route_povs();
        let mut s2 = s;
        if self.blocked_at(s, radius) {
            for (at, _) in &povs {
                if let Some(q) = self.first_clear_stop(s, *at, radius) {
                    s2 = q;
                    break;
                }
            }
        }
        self.astar(g, s2, radius, &povs)
            .unwrap_or_else(|| vec![s2, g])
    }

    /// `FUN_00405c30`: march from `p` toward `toward` in half-radius steps
    /// (`trunc(length / step)` stops, `p` itself first) and answer the first
    /// stop whose circle crosses no line.
    fn first_clear_stop(&self, p: [f32; 2], toward: [f32; 2], radius: f32) -> Option<[f32; 2]> {
        let step = radius * 0.5;
        let d = [toward[0] - p[0], toward[1] - p[1]];
        let length = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if step <= 0.0 || length <= 0.0 {
            return None;
        }
        let n = (length / step) as i32;
        let dir = [d[0] / length * step, d[1] / length * step];
        let mut q = p;
        for _ in 0..n {
            if !self.blocked_at(q, radius) {
                return Some(q);
            }
            q = [q[0] + dir[0], q[1] + dir[1]];
        }
        None
    }

    /// The A* of `GetPath` from `start` (the route's target) to `goal` (the
    /// robot): the waypoints from `goal` back to `start`, or `None`.
    fn astar(
        &self,
        start: [f32; 2],
        goal: [f32; 2],
        radius: f32,
        povs: &[([f32; 2], Vec<usize>)],
    ) -> Option<Vec<[f32; 2]>> {
        // A node is a vantage point (by index) or a free point.
        #[derive(Clone, Copy, PartialEq)]
        enum Node {
            Pov(usize),
            Free([f32; 2]),
        }
        let pos = |n: Node| match n {
            Node::Pov(k) => povs[k].0,
            Node::Free(p) => p,
        };
        let d2 = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2);
        // Steps and estimate are `FUN_00407890`'s squared distance (stored at
        // node+0x0c, f = +0x0c + +0x08: 0x4072d7, 0x407497, 0x407596).
        let h = d2;
        let mut nodes: Vec<(Node, f32, f32, Option<usize>)> = Vec::new(); // node, g, f, parent
        nodes.push((Node::Free(start), 0.0, h(start, goal), None));
        let mut open = AStarHeap::default();
        open.push(0, nodes[0].2);
        let mut closed: Vec<usize> = Vec::new();
        for _ in 0..1000 {
            let cur = open.pop()?;
            let (node, g_cur, _, parent) = nodes[cur];
            if node == Node::Free(goal) {
                let mut route = Vec::new();
                let mut k = Some(cur);
                while let Some(i) = k {
                    route.push(pos(nodes[i].0));
                    k = nodes[i].3;
                }
                return Some(route);
            }
            let parent_node = parent.map(|p| nodes[p].0);
            let mut succ: Vec<Node> = Vec::new();
            match node {
                Node::Free(p) => {
                    for (k, (at, _)) in povs.iter().enumerate() {
                        if parent_node != Some(Node::Pov(k)) && self.march_clear(p, *at, radius) {
                            succ.push(Node::Pov(k));
                        }
                    }
                }
                Node::Pov(k) => {
                    if self.march_clear(povs[k].0, goal, radius) {
                        succ.push(Node::Free(goal));
                    }
                    for &j in &povs[k].1 {
                        if parent_node != Some(Node::Pov(j)) {
                            succ.push(Node::Pov(j));
                        }
                    }
                }
            }
            for c in succ {
                let g = g_cur + d2(pos(node), pos(c));
                let in_open = open.find(|i| nodes[i].0 == c);
                if let Some(i) = in_open {
                    if nodes[i].1 <= g {
                        continue;
                    }
                }
                let in_closed = closed.iter().position(|&i| nodes[i].0 == c);
                if let Some(ci) = in_closed {
                    if nodes[closed[ci]].1 <= g {
                        continue;
                    }
                }
                if let Some(ci) = in_closed {
                    closed.remove(ci);
                }
                if let Some(i) = in_open {
                    open.remove(i);
                }
                let f = g + h(pos(c), goal);
                nodes.push((c, g, f, Some(cur)));
                open.push(nodes.len() - 1, f);
            }
            closed.push(cur);
        }
        None
    }

    /// The map's floor under a world point, for `FromGrid`.
    pub fn floor_height(&self, x: f32, z: f32) -> f32 {
        let (i, j) = self.map.to_grid(Vec3::new(x, 0.0, z));
        self.map.from_grid(i, j).y
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

/// Whether the segment `a`–`b` crosses the circle of `radius` about `c`: one of
/// the two points where its line meets the circle lies on the segment
/// (`FUN_0061b260`'s parametric test).
fn segment_crosses_circle(a: [f32; 2], b: [f32; 2], c: [f32; 2], radius: f32) -> bool {
    let d = [b[0] - a[0], b[1] - a[1]];
    let f = [a[0] - c[0], a[1] - c[1]];
    let qa = d[0] * d[0] + d[1] * d[1];
    if qa <= 0.0 {
        return false;
    }
    let qb = 2.0 * (f[0] * d[0] + f[1] * d[1]);
    let qc = f[0] * f[0] + f[1] * f[1] - radius * radius;
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 {
        return false;
    }
    let root = disc.sqrt();
    let t0 = (-qb - root) / (2.0 * qa);
    let t1 = (-qb + root) / (2.0 * qa);
    (0.0..=1.0).contains(&t0) || (0.0..=1.0).contains(&t1)
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

    /// `AddCollisionLine((x0, z0), (x1, z1))` — a wall segment for the AI's
    /// navigation. The original keeps a list of them (`IsStraightPathClear` and
    /// `GetPath` test against it); it does not touch the height map.
    fn AddCollisionLine(&self, a: (f32, f32), b: (f32, f32)) {
        self.d().collision_lines.push(CollisionLine {
            a: [a.0, a.1],
            b: [b.0, b.1],
        });
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

    /// `GetMapSize()` (`0x4ba690` -> `0x405410`): the cell counts `(cols, rows)` as floats (live box:
    /// `(30.0, 30.0)`).
    fn GetMapSize(&self) -> (f32, f32) {
        let d = self.d();
        (d.map.cols as f32, d.map.rows as f32)
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

    /// `ToGrid(p)` — the plane point `(x, z)` (`FUN_00406240` keeps the two
    /// floats as they are).
    fn ToGrid(&self, p: (f32, f32, f32)) -> (f32, f32) {
        (p.0, p.2)
    }

    /// `FromGrid((x, z))` — the world point on the map's floor
    /// (`FUN_00405fa0`).
    fn FromGrid(&self, grid: (f32, f32)) -> (f32, f32, f32) {
        let y = self.d().floor_height(grid.0, grid.1);
        (grid.0, y, grid.1)
    }

    fn SetSearchRadius(&mut self, radius: f32) {
        self.search_radius = radius;
    }

    /// `GetPath(from_world, to_world, ignore_hazards)` (`FUN_004ba370`) — `(x, z)`
    /// waypoints from `from_world` to `to_world` (moved, if it sits on a line):
    /// the native search with `p1 = from`, `p2 = to`, its list reversed (see
    /// [`ArenaData::path`], docs/29 section 12ah).
    fn GetPath(
        &self,
        from: (f32, f32, f32),
        to: (f32, f32, f32),
        _ignore_hazards: bool,
    ) -> Vec<(f32, f32)> {
        let r = self.search_radius;
        self.data
            .lock()
            .unwrap()
            .path(
                Vec3::new(from.0, from.1, from.2),
                Vec3::new(to.0, to.1, to.2),
                r,
            )
            .into_iter()
            .rev()
            .map(|p| (p[0], p[1]))
            .collect()
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
    ///
    /// A ray that strikes nothing (or has no length) answers `(0, (None, None, None))` (live).
    #[allow(clippy::type_complexity)]
    fn RayTest(
        &self,
        a: (f32, f32, f32),
        b: (f32, f32, f32),
    ) -> (bool, (Option<f32>, Option<f32>, Option<f32>)) {
        let from = Vec3::new(a.0, a.1, a.2);
        let to = Vec3::new(b.0, b.1, b.2);
        let dir = to - from;
        let len = dir.length();
        if len <= 1e-6 {
            return (false, (None, None, None));
        }
        let dir = dir / len;
        let d = self.data.lock().unwrap();
        match d.first_triangle_hit(from, dir, len) {
            Some(best) => {
                let p = from + dir * best;
                (true, (Some(p.x), Some(p.y), Some(p.z)))
            }
            None => (false, (None, None, None)),
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
    /// A slider's two stops, as `AddPrismatic` passes them (`0x404800` →
    /// `0x4b47b0`): `neutral` is the one a negative direction drives it to
    /// (the controller's `+0x6c`), `lower` the one a positive direction does
    /// (`+0x70`); the travel is between them. `upper` is the script's last
    /// number, a start offset (nonzero in no stock arena) the engine ignores.
    pub neutral: f32,
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
    // A prismatic controller starts locked with power settings `(1, 60)`
    // (`0x4b47b0`: `+0x74 = 1`, `+0x78 = 60`, then `Lock(1)`).
    let drive = Arc::new(Mutex::new(Drive {
        locked: true,
        speed: if slider { 1.0 } else { 0.0 },
        torque: if slider { 60.0 } else { 0.0 },
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

    /// `SetDirection(d)`: the controller keeps it as an integer (`+0x80`).
    fn SetDirection(&self, direction: f32) {
        self.d().direction = direction.trunc();
    }

    /// `Lock(on)` (`0x4b50c0`): locking also zeroes the direction, so the
    /// script gives it one again after unlocking.
    fn Lock(&self, on: bool) {
        let mut d = self.d();
        d.locked = on;
        if on {
            d.direction = 0.0;
        }
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

/// The open list of `GetPath`'s A*: a binary heap kept with the standard
/// library's heap algorithms of the original's compiler (push sifts the new
/// entry up past every parent it beats; pop moves the hole at the top down
/// along the better child to the bottom and pushes the last entry into it),
/// with the lowest `f` on top. Its order is what breaks ties between routes
/// of equal cost (live: from the box's north the route goes round the west
/// side, POV 3, not the east).
#[derive(Default)]
struct AStarHeap {
    items: Vec<(usize, f32)>,
}

impl AStarHeap {
    /// `a` ranks below `b`.
    fn below(a: (usize, f32), b: (usize, f32)) -> bool {
        a.1 > b.1
    }

    fn sift_up(&mut self, mut i: usize, v: (usize, f32)) {
        while i > 0 {
            let p = (i - 1) / 2;
            if Self::below(self.items[p], v) {
                self.items[i] = self.items[p];
                i = p;
            } else {
                break;
            }
        }
        self.items[i] = v;
    }

    fn push(&mut self, id: usize, f: f32) {
        self.items.push((id, f));
        let i = self.items.len() - 1;
        self.sift_up(i, (id, f));
    }

    fn pop(&mut self) -> Option<usize> {
        let n = self.items.len().checked_sub(1)?;
        let top = self.items[0];
        let v = self.items[n];
        self.items[n] = top;
        let (mut i, mut c) = (0usize, 2usize);
        while c < n {
            if Self::below(self.items[c], self.items[c - 1]) {
                c -= 1;
            }
            self.items[i] = self.items[c];
            i = c;
            c = 2 * c + 2;
        }
        if c == n {
            self.items[i] = self.items[c - 1];
            i = c - 1;
        }
        self.sift_up(i, v);
        self.items.pop();
        Some(top.0)
    }

    fn find(&self, pred: impl Fn(usize) -> bool) -> Option<usize> {
        self.items.iter().find(|e| pred(e.0)).map(|e| e.0)
    }

    /// Take entry `id` out (`FUN_004514b0`: the vector erase keeps the order of
    /// the rest) and `make_heap` what is left (`FUN_0040d590` -> `FUN_0040f6d0`:
    /// from `len / 2` down to the first slot, each entry moved down along the
    /// better child to the bottom and sifted back up).
    fn remove(&mut self, id: usize) {
        self.items.retain(|e| e.0 != id);
        let n = self.items.len();
        if n < 2 {
            return;
        }
        let mut hole = n / 2;
        while hole > 0 {
            hole -= 1;
            let v = self.items[hole];
            let top = hole;
            let (mut i, mut c) = (hole, 2 * hole + 2);
            while c < n {
                if Self::below(self.items[c], self.items[c - 1]) {
                    c -= 1;
                }
                self.items[i] = self.items[c];
                i = c;
                c = 2 * c + 2;
            }
            if c == n {
                self.items[i] = self.items[c - 1];
                i = c - 1;
            }
            // push_heap back up, not past the slot the adjust began at.
            while i > top {
                let p = (i - 1) / 2;
                if Self::below(self.items[p], v) {
                    self.items[i] = self.items[p];
                    i = p;
                } else {
                    break;
                }
            }
            self.items[i] = v;
        }
    }
}
