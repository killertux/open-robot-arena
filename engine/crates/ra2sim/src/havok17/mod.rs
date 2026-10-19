//! `havok17`: rigid-body dynamics written from the recovered specification of
//! the Havok 1.7.1 subset *Robot Arena 2* ships (`0x51a000..0x568000` in the
//! executable; see `docs/29-havok17-backend.md`).
//!
//! Our own code does everything Havok does to a body — integration, the
//! collision resolver, the constraint solver with its motors, the drag action,
//! backstepping — in Havok's order. Jolt is used only as a geometry engine: it
//! holds every body's shape at its current pose and answers closest-point
//! queries ("which surfaces are within the tolerance of this shape") and ray
//! casts; Jolt's own solver never runs.
//!
//! One [`PhysicsBackend::step`] is one Havok substep. The game steps its world
//! at a fixed 1/30 s split in two substeps (`FUN_004ac1d0`: `+0x4 = 1/30`,
//! `+0x8 = 2`), which is the 1/60 s step the match runs at. A substep is the
//! loop of `World::stepTo` (`FUN_0052aa60`):
//!
//! 1. fire the collision events of the last detection (`0x549c30`);
//! 2. resolve those contacts with impulses ([`resolver`]);
//! 3. integrate (explicit Euler, `0x680ff0`) and detect collisions; if a new
//!    pair overlaps, roll the bodies back and try half the step, at most 30
//!    times ("Backstep/Roll Back");
//! 4. run the actions: the constraint solver with its motors ([`joints`]) and
//!    the world's drag action (`FUN_0052d140(0.1, 0.3)`).
//!
//! and repeats from 1 until the substep's time is used up.

mod body;
pub mod joints;
pub mod mass;
pub mod narrow;
pub mod resolver;

use std::collections::HashMap;

use glam::{Mat3, Quat, Vec3};
use oxijolt::prelude::math as m;
use oxijolt::prelude::{
    Activation, BodyId, BodySettings, PhysicsWorld, QueryFilter, RayCast, Shape, WorldSettings,
};
use oxijolt::CollideShape;

use crate::backend::{
    BackendError, BodyDesc, BodyHandle, BodyState, ContactEvent, HingeDesc, JointHandle,
    MotionType, PhysicsBackend, RayHit, ShapeDesc, SliderDesc, StepInfo,
};
use crate::jolt::JoltBackend;
use body::Body;
use joints::{Joint, Kind};
use narrow::{Aabb, ConvexManifold, MeshManifold, Piece, PieceShape, Pose};
use resolver::Contact;

/// World gravity as the game sets it (`FUN_0052a000(0, -12, 0)` at
/// `0x4ac377`), read back live from the world object (`havok-step__live__017`).
/// The arenas' `.gmf` say -9.81, which the game never applies.
pub const GRAVITY: Vec3 = Vec3::new(0.0, -12.0, 0.0);

/// The collision tolerance: surfaces closer than this are contacts. The arena
/// `.gmf`'s `SIMTOLERANCE` is 0.1, and live the resolver sees contact
/// distances up to 0.0999 with resting bodies at 0.083..0.0999
/// (`contacts_rest`, 800 impulses on the Scout and ALARM at rest). Each
/// backstep halves it (`setCurrentTolerance`, `0x52b300`).
pub const TOLERANCE: f32 = 0.1;

/// Backstepping: at most this many halvings of a step (`0x52aa60`), never
/// shorter than `MIN_STEP`.
pub const MAX_BACKSTEPS: u32 = 30;
pub const MIN_STEP: f32 = 1e-5;

/// The world's drag action (`FUN_0052d140(0.1, 0.3)` at init, applied by
/// `0x52d200` to every body's accumulators): `F -= 0.1 v`, `τ -= 0.3 ω`.
pub const DRAG_LINEAR: f32 = 0.1;
pub const DRAG_ANGULAR: f32 = 0.3;

/// Havok's body defaults (`FUN_0051f8c0`: `+0x194 = 0.2`, `+0x198 = 0.5`).
pub const DEFAULT_FRICTION: f32 = 0.5;
pub const DEFAULT_RESTITUTION: f32 = 0.2;

/// The density a body without an explicit mass gets (Jolt's default, for the
/// few test bodies that do not state one).
const DEFAULT_DENSITY: f32 = 1000.0;

/// Our cap on step pieces per substep: backstepped pieces plus the remainder.
const MAX_PIECES: u32 = 8;

/// `RA2_H17_DEBUG=<body>` prints that body's state and contacts every step.
fn debug_body() -> Option<usize> {
    use std::sync::OnceLock;
    static WATCH: OnceLock<Option<usize>> = OnceLock::new();
    *WATCH.get_or_init(|| std::env::var("RA2_H17_DEBUG").ok()?.parse().ok())
}

fn rv(v: Vec3) -> m::RVec3 {
    m::RVec3::new(v.x as m::Real, v.y as m::Real, v.z as m::Real)
}

fn jv(v: Vec3) -> m::Vec3 {
    m::Vec3::new(v.x, v.y, v.z)
}

#[allow(clippy::unnecessary_cast)]
fn un_rv(v: m::RVec3) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}

fn un_v(v: m::Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn jq(q: Quat) -> m::Quat {
    m::Quat::from_xyzw(q.x, q.y, q.z, q.w)
}

/// A body's geometry in the Jolt query world.
struct Geometry {
    id: BodyId,
    shape: Shape,
    phantom: Option<Shape>,
    /// The body's collidables, body-local (what [`narrow`] collides).
    pieces: Vec<Piece>,
}

/// A pair of collidables: (body, piece) of A, then of B.
type PairKey = (usize, u32, usize, u32);

/// One collidable where its body is now.
struct WorldPiece {
    body: usize,
    piece: u32,
    body_pose: Pose,
    pose: Pose,
    /// A convex piece's vertices in world space (empty for a mesh).
    vertices: Vec<Vec3>,
    radius: f32,
    bounds: Aabb,
}

/// What a body looked like before a step piece, for rolling back.
#[derive(Clone, Copy)]
struct Saved {
    com: Vec3,
    rotation: Quat,
    velocity: Vec3,
    angular_velocity: Vec3,
    force: Vec3,
    torque: Vec3,
}

/// The Havok-1.7-faithful backend.
pub struct Havok17Backend {
    world: PhysicsWorld,
    bodies: Vec<Body>,
    geometry: Vec<Option<Geometry>>,
    /// Jolt body id (raw) → our handle.
    lookup: HashMap<u32, usize>,
    joints: Vec<Joint>,
    /// The contacts of the last collision detection.
    contacts: Vec<Contact>,
    /// The agents' persistent manifolds, per pair of collidables.
    mesh_manifolds: HashMap<PairKey, MeshManifold>,
    convex_manifolds: HashMap<PairKey, ConvexManifold>,
    /// Overlapping pairs a bad step let through (`detector + 0xe4`): they do
    /// not backstep again while they keep overlapping.
    excused: std::collections::HashSet<PairKey>,
    /// Detections run (the manifolds' age stamp).
    detections: u64,
    /// Collision events of the last step.
    events: Vec<ContactEvent>,
    /// Per body: the surface normal its phantom overlapped in the last step.
    phantom_touch: Vec<Option<Vec3>>,
    last_dt: f32,
    /// Body pairs (lower handle first) that never collide.
    disabled: std::collections::HashSet<(usize, usize)>,
    /// Diagnostics: step pieces and backsteps since the world was made.
    pub pieces: u64,
    pub backsteps: u64,
}

impl Havok17Backend {
    pub fn new() -> Result<Self, BackendError> {
        let world = PhysicsWorld::new(WorldSettings::default().gravity(jv(GRAVITY)))
            .map_err(|e| BackendError::World(format!("{e:?}")))?;
        Ok(Self {
            world,
            bodies: Vec::new(),
            geometry: Vec::new(),
            lookup: HashMap::new(),
            joints: Vec::new(),
            contacts: Vec::new(),
            mesh_manifolds: HashMap::new(),
            convex_manifolds: HashMap::new(),
            excused: Default::default(),
            detections: 0,
            events: Vec::new(),
            phantom_touch: Vec::new(),
            last_dt: 1.0 / 60.0,
            disabled: Default::default(),
            pieces: 0,
            backsteps: 0,
        })
    }

    fn body(&self, handle: BodyHandle) -> Option<&Body> {
        self.bodies.get(handle as usize).filter(|b| b.alive)
    }

    fn body_mut(&mut self, handle: BodyHandle) -> Option<&mut Body> {
        self.bodies.get_mut(handle as usize).filter(|b| b.alive)
    }

    /// Mass, centre and inverse inertia of `shape` at `mass` (or the default
    /// density).
    fn mass_of(shape: &ShapeDesc, mass: Option<f32>) -> (f32, Vec3, Mat3) {
        let props = mass::of_shape(shape);
        let m = mass
            .filter(|m| *m > 0.0)
            .unwrap_or(props.volume * DEFAULT_DENSITY)
            .max(1e-3);
        let inertia = if props.volume > 0.0 {
            props.inertia * (m / props.volume)
        } else {
            Mat3::from_diagonal(Vec3::splat(m * 0.01))
        };
        let inv = if inertia.determinant().abs() > 1e-12 {
            inertia.inverse()
        } else {
            Mat3::from_diagonal(Vec3::splat(1.0 / (m * 0.01)))
        };
        (m, props.com, inv)
    }

    /// Put every moving body's geometry where the body is.
    fn sync(&mut self) {
        for (i, b) in self.bodies.iter().enumerate() {
            if !b.moves() {
                continue;
            }
            if let Some(g) = &self.geometry[i] {
                if let Ok(mut body) = self.world.body_mut(g.id) {
                    let _ = body.set_position_and_rotation(
                        rv(b.origin()),
                        jq(b.rotation),
                        Activation::DontActivate,
                    );
                }
            }
        }
    }

    fn sync_one(&mut self, i: usize) {
        let b = &self.bodies[i];
        if let Some(g) = &self.geometry[i] {
            if let Ok(mut body) = self.world.body_mut(g.id) {
                let _ = body.set_position_and_rotation(
                    rv(b.origin()),
                    jq(b.rotation),
                    Activation::DontActivate,
                );
            }
        }
    }

    /// Whether two bodies never collide: parts of one robot, or a pair the
    /// arena disables.
    fn same_group(&self, i: usize, j: usize) -> bool {
        if self.disabled.contains(&(i.min(j), i.max(j))) {
            return true;
        }
        match (self.bodies[i].group, self.bodies[j].group) {
            (Some(a), Some(b)) => a.group == b.group,
            _ => false,
        }
    }

    /// Explicit Euler over `h` (`0x680ff0`: `y += h f(y)` on the packed state):
    /// the position moves with the velocity the step started with, the velocity
    /// with the accumulated force plus gravity, the angular velocity with
    /// `I⁻¹ τ` (no gyroscopic term: the state carries ω, `FUN_00521330`).
    fn integrate(&mut self, h: f32) {
        for b in &mut self.bodies {
            if !b.moves() {
                continue;
            }
            let w = b.angular_velocity;
            b.com += b.velocity * h;
            let q = b.rotation;
            b.rotation = (q + Quat::from_xyzw(w.x, w.y, w.z, 0.0) * q * (0.5 * h)).normalize();
            if b.motion == MotionType::Dynamic {
                b.velocity += (b.force * b.inv_mass + GRAVITY) * h;
                b.angular_velocity += b.inv_inertia_world * b.torque * h;
            }
            b.force = Vec3::ZERO;
            b.torque = Vec3::ZERO;
            b.refresh_inertia();
        }
    }

    fn save(&self) -> Vec<Saved> {
        self.bodies
            .iter()
            .map(|b| Saved {
                com: b.com,
                rotation: b.rotation,
                velocity: b.velocity,
                angular_velocity: b.angular_velocity,
                force: b.force,
                torque: b.torque,
            })
            .collect()
    }

    fn restore(&mut self, saved: &[Saved]) {
        for (b, s) in self.bodies.iter_mut().zip(saved) {
            b.com = s.com;
            b.rotation = s.rotation;
            b.velocity = s.velocity;
            b.angular_velocity = s.angular_velocity;
            b.force = s.force;
            b.torque = s.torque;
            b.refresh_inertia();
        }
    }

    /// The bodies' collidables where they are now.
    fn world_pieces(&self) -> Vec<WorldPiece> {
        let mut out = Vec::new();
        for (i, b) in self.bodies.iter().enumerate() {
            if !b.alive || b.sensor {
                continue;
            }
            let Some(g) = &self.geometry[i] else { continue };
            let body = Pose {
                position: b.origin(),
                rotation: b.rotation,
            };
            for (k, p) in g.pieces.iter().enumerate() {
                let pose = Pose {
                    position: body.apply(p.position),
                    rotation: body.rotation * p.rotation,
                };
                let (vertices, radius, bounds) = match &p.shape {
                    PieceShape::Convex { vertices, radius } => {
                        let v: Vec<Vec3> = vertices.iter().map(|v| pose.apply(*v)).collect();
                        let bounds = Aabb::of(v.iter().copied()).grown(*radius);
                        (v, *radius, bounds)
                    }
                    PieceShape::Mesh(mesh) => (
                        Vec::new(),
                        0.0,
                        mesh.bounds.transformed(pose.position, pose.rotation),
                    ),
                };
                out.push(WorldPiece {
                    body: i,
                    piece: k as u32,
                    body_pose: body,
                    pose,
                    vertices,
                    radius,
                    bounds,
                });
            }
        }
        out
    }

    /// Collision detection (`FUN_00542810`): every pair of collidables within
    /// the tolerance gets its agent's contact points (see [`narrow`]). Returns
    /// the contacts and the pairs that make this a bad step (`detector +
    /// 0x130`, `FUN_00542db0`): a pair whose shapes overlap, unless an earlier
    /// bad step excused it, and a pair with a point closer than `current_tol`.
    fn detect(&mut self, current_tol: f32) -> (Vec<Contact>, Vec<PairKey>) {
        self.sync();
        self.detections += 1;
        let stamp = self.detections;
        let tol = TOLERANCE;
        let pieces = self.world_pieces();
        let mut out = Vec::new();
        let mut bad: Vec<PairKey> = Vec::new();
        let mut excused_seen: std::collections::HashSet<PairKey> = Default::default();
        let mut penetrating_bodies: Vec<(usize, usize)> = Vec::new();
        let mut triangles = Vec::new();
        for pa in &pieces {
            let i = pa.body;
            let bi = &self.bodies[i];
            if !bi.moves() {
                continue;
            }
            let (dynamic_i, friction_i, restitution_i) =
                (bi.is_dynamic(), bi.friction, bi.restitution);
            for pb in &pieces {
                let j = pb.body;
                if j == i {
                    continue;
                }
                let bj = &self.bodies[j];
                // A pair of moving bodies is seen from both sides; keep one.
                if bj.moves() && j < i {
                    continue;
                }
                if !dynamic_i && !bj.is_dynamic() {
                    continue;
                }
                let (friction_j, restitution_j) = (bj.friction, bj.restitution);
                if self.same_group(i, j) {
                    continue;
                }
                if !pa.bounds.grown(tol).overlaps(&pb.bounds) {
                    continue;
                }
                let key: PairKey = (i, pa.piece, j, pb.piece);
                let first = out.len();
                let mut penetrating = false;
                let mut points: Vec<narrow::Point> = Vec::new();
                // Whether the pair's agent sees the bodies the other way round
                // (A is the convex piece of a convex x mesh pair).
                let mut swap = false;
                match (pa.vertices.is_empty(), pb.vertices.is_empty()) {
                    (false, false) => {
                        // Convex x convex (`FUN_00544820`).
                        match narrow::convex_point(
                            &pa.vertices,
                            pa.radius,
                            &pb.vertices,
                            pb.radius,
                            &pa.body_pose,
                            &pb.body_pose,
                        ) {
                            None => penetrating = true,
                            Some((p, c)) if p.distance < tol => {
                                let m = self.convex_manifolds.entry(key).or_default();
                                m.seen = stamp;
                                let kept =
                                    m.update(p, c.persists(), tol, &pa.body_pose, &pb.body_pose);
                                points.push(p);
                                points.extend(kept);
                            }
                            Some(_) => {}
                        }
                    }
                    (false, true) | (true, false) => {
                        // Convex x mesh (`FUN_00545b50`), the convex piece as A.
                        swap = pa.vertices.is_empty();
                        let (cx, ms) = if swap { (pb, pa) } else { (pa, pb) };
                        let key = (cx.body, cx.piece, ms.body, ms.piece);
                        penetrating =
                            self.convex_mesh(key, cx, ms, tol, stamp, &mut triangles, &mut points);
                    }
                    (true, true) => {
                        // Mesh x mesh (`FUN_0054a2a0`).
                        penetrating =
                            self.mesh_mesh(pa, pb, tol, &out, &mut triangles, &mut points);
                    }
                }
                let friction = (friction_i * friction_j).max(0.0).sqrt();
                let restitution = restitution_i * restitution_j;
                let (ca, cb) = if swap { (j, i) } else { (i, j) };
                for p in points {
                    out.push(Contact {
                        a: ca,
                        b: cb,
                        normal: p.normal,
                        point: p.point,
                        distance: p.distance,
                        friction,
                        restitution,
                    });
                }
                if penetrating {
                    if self.excused.contains(&key) {
                        excused_seen.insert(key);
                    } else {
                        bad.push(key);
                    }
                    if !penetrating_bodies.contains(&(i, j)) {
                        penetrating_bodies.push((i, j));
                    }
                } else if out[first..].iter().any(|c| c.distance < current_tol) {
                    bad.push(key);
                }
            }
        }
        // Ours: Havok reports nothing for overlapping shapes and backsteps
        // until they are apart; when the step is accepted overlapping anyway,
        // the deepest points Jolt finds for the pair push it apart.
        for (i, j) in penetrating_bodies {
            out.extend(self.overlap_contacts(i, j));
        }
        self.mesh_manifolds.retain(|_, m| m.seen == stamp);
        self.convex_manifolds.retain(|_, m| m.seen == stamp);
        if bad.is_empty() {
            self.excused.retain(|k| excused_seen.contains(k));
        }
        (out, bad)
    }

    fn mesh_piece(&self, w: &WorldPiece) -> Option<&narrow::Mesh> {
        match &self.geometry[w.body]
            .as_ref()?
            .pieces
            .get(w.piece as usize)?
            .shape
        {
            PieceShape::Mesh(m) => Some(m),
            PieceShape::Convex { .. } => None,
        }
    }

    /// The convex x mesh agent (`FUN_00545b50`, see [`MeshManifold`]): its
    /// points into `points`; `true` when the shapes overlap (no points then
    /// but those of the triangles before the overlapping one).
    #[allow(clippy::too_many_arguments)]
    fn convex_mesh(
        &mut self,
        key: PairKey,
        cx: &WorldPiece,
        ms: &WorldPiece,
        tol: f32,
        stamp: u64,
        triangles: &mut Vec<u32>,
        points: &mut Vec<narrow::Point>,
    ) -> bool {
        let Some(PieceShape::Mesh(mesh)) = self.geometry[ms.body]
            .as_ref()
            .and_then(|g| g.pieces.get(ms.piece as usize))
            .map(|p| &p.shape)
        else {
            return false;
        };
        let local =
            Aabb::of(cx.vertices.iter().map(|v| ms.pose.inverse_apply(*v))).grown(cx.radius + tol);
        mesh.query(&local, triangles);
        let m = self
            .mesh_manifolds
            .entry(key)
            .or_insert_with(|| MeshManifold {
                tol2: tol * tol,
                ..Default::default()
            });
        m.seen = stamp;
        m.refresh(&cx.body_pose, &ms.body_pose);
        for &t in triangles.iter() {
            let tri = mesh.triangles[t as usize].map(|v| ms.pose.apply(mesh.vertices[v as usize]));
            match narrow::convex_point(
                &cx.vertices,
                cx.radius,
                &tri,
                0.0,
                &cx.body_pose,
                &ms.body_pose,
            ) {
                None => return true,
                Some((p, c)) if p.distance < tol => {
                    if c.persists() {
                        m.offer(p);
                    } else {
                        points.push(p);
                    }
                }
                Some(_) => {}
            }
        }
        points.extend(m.points.iter().copied());
        false
    }

    /// The mesh x mesh agent (`FUN_0054a2a0`, a fixed mesh; `FUN_00549d00`,
    /// two moving ones): every pair of triangles, one
    /// of each mesh, within the tolerance of each other gives the midpoint of
    /// their closest points, unless a point already reported in this
    /// detection lies within `sqrt(0.001)` tolerances of it. No manifold is
    /// kept. `true` when two triangles touch (`d < 1e-5`).
    fn mesh_mesh(
        &self,
        pa: &WorldPiece,
        pb: &WorldPiece,
        tol: f32,
        reported: &[Contact],
        triangles: &mut Vec<u32>,
        points: &mut Vec<narrow::Point>,
    ) -> bool {
        let (Some(ma), Some(mb)) = (self.mesh_piece(pa), self.mesh_piece(pb)) else {
            return false;
        };
        let same = (tol * tol) * narrow::MESH_DUPLICATE;
        // The candidate triangles: within the tolerance of a fixed mesh's
        // (its MOPP query, `FUN_0054a080`), within half of it between two
        // moving meshes' trees (`FUN_00549d00` → `FUN_00550100`).
        let margin = if self.bodies[pb.body].moves() {
            tol * 0.5
        } else {
            tol
        };
        for ta in &ma.triangles {
            let tri_a = ta.map(|v| pa.pose.apply(ma.vertices[v as usize]));
            let local = Aabb::of(tri_a.iter().map(|v| pb.pose.inverse_apply(*v))).grown(margin);
            mb.query(&local, triangles);
            for &t in triangles.iter() {
                let tri_b =
                    mb.triangles[t as usize].map(|v| pb.pose.apply(mb.vertices[v as usize]));
                let (d, on_a, on_b) = narrow::triangle_closest(&tri_a, &tri_b);
                if d < 1e-5 {
                    return true;
                }
                if d >= tol {
                    continue;
                }
                let point = (on_a + on_b) * 0.5;
                if reported
                    .iter()
                    .any(|c| c.point.distance_squared(point) < same)
                    || points
                        .iter()
                        .any(|p| p.point.distance_squared(point) < same)
                {
                    continue;
                }
                points.push(narrow::Point {
                    normal: (on_a - on_b) / d,
                    point,
                    distance: d,
                    local_a: pa.body_pose.inverse_apply(on_a),
                    local_b: pb.body_pose.inverse_apply(on_b),
                });
            }
        }
        false
    }

    /// The overlap of bodies `i` and `j` as Jolt measures it: one point per
    /// overlapping pair of sub-shapes, at negative distance.
    fn overlap_contacts(&self, i: usize, j: usize) -> Vec<Contact> {
        let (bi, bj) = (&self.bodies[i], &self.bodies[j]);
        let Some(g) = &self.geometry[i] else {
            return Vec::new();
        };
        let Some(target) = self.geometry[j].as_ref().map(|g| g.id) else {
            return Vec::new();
        };
        let query = CollideShape::new(&g.shape, rv(bi.origin()), jq(bi.rotation));
        let Ok(hits) = self
            .world
            .collide_shape(&query, &QueryFilter::new().exclude_body(g.id))
        else {
            return Vec::new();
        };
        let friction = (bi.friction * bj.friction).max(0.0).sqrt();
        let restitution = bi.restitution * bj.restitution;
        hits.into_iter()
            .filter(|h| h.body == target && h.penetration_depth > 0.0)
            .filter_map(|h| {
                let n = un_v(h.normal);
                (n.length_squared() > 0.5).then(|| Contact {
                    a: i,
                    b: j,
                    normal: n.normalize(),
                    point: (un_rv(h.point_on_shape) + un_rv(h.point_on_body)) * 0.5,
                    distance: -h.penetration_depth,
                    friction,
                    restitution,
                })
            })
            .collect()
    }

    /// Which surfaces the bodies' phantoms touch now (the phantom events at the
    /// end of `stepTo`). A phantom is a collidable flagged `DISABLE_COLLISIONS`:
    /// detected like any other, within the tolerance, but never resolved.
    fn detect_phantoms(&mut self) {
        for i in 0..self.bodies.len() {
            self.phantom_touch[i] = None;
            let b = &self.bodies[i];
            if !b.alive {
                continue;
            }
            let Some(Some(phantom)) = self.geometry[i].as_ref().map(|g| g.phantom.as_ref()) else {
                continue;
            };
            let id = self.geometry[i].as_ref().map(|g| g.id);
            let query = CollideShape::new(phantom, rv(b.origin()), jq(b.rotation))
                .max_separation_distance(TOLERANCE);
            let filter = match id {
                Some(id) => QueryFilter::new().exclude_body(id),
                None => QueryFilter::new(),
            };
            let Ok(hits) = self.world.collide_shape(&query, &filter) else {
                continue;
            };
            let mut best: Option<(f32, Vec3)> = None;
            for hit in hits {
                let Some(&j) = self.lookup.get(&hit.body.to_raw()) else {
                    continue;
                };
                if j == i || !self.bodies[j].alive || self.bodies[j].sensor || self.same_group(i, j)
                {
                    continue;
                }
                let n = un_v(hit.normal);
                if n.length_squared() < 0.5 || -hit.penetration_depth >= TOLERANCE {
                    continue;
                }
                if best.is_none_or(|(d, _)| hit.penetration_depth > d) {
                    best = Some((hit.penetration_depth, n.normalize()));
                }
            }
            self.phantom_touch[i] = best.map(|(_, n)| n);
        }
    }

    /// The collision events of the contacts about to be resolved: one per
    /// point, with both bodies as they are now.
    fn fire_events(&mut self) {
        for c in &self.contacts {
            self.events.push(ContactEvent {
                a: c.a as BodyHandle,
                b: c.b as BodyHandle,
                normal: -c.normal,
                point: c.point,
                state_a: self.bodies[c.a].state(),
                state_b: self.bodies[c.b].state(),
            });
        }
    }

    /// The world's drag action.
    fn drag(&mut self) {
        for b in &mut self.bodies {
            if b.is_dynamic() {
                b.force -= b.velocity * DRAG_LINEAR;
                b.torque -= b.angular_velocity * DRAG_ANGULAR;
            }
        }
    }

    fn joint(&self, handle: JointHandle) -> Option<&Joint> {
        self.joints.get(handle as usize).filter(|j| j.alive)
    }

    fn joint_mut(&mut self, handle: JointHandle) -> Option<&mut Joint> {
        self.joints.get_mut(handle as usize).filter(|j| j.alive)
    }

    /// Switch a joint's motor to `velocity` (0 = off), starting the servo's
    /// target where the joint is.
    fn set_motor(&mut self, handle: JointHandle, velocity: f32) {
        let position = match self.joint(handle) {
            Some(j) => j.position(&self.bodies),
            None => return,
        };
        if let Some(j) = self.joint_mut(handle) {
            if velocity == 0.0 {
                j.motor = false;
            } else {
                if !j.motor {
                    j.target = position;
                }
                j.motor = true;
            }
            j.target_velocity = velocity;
        }
    }
}

impl PhysicsBackend for Havok17Backend {
    fn name(&self) -> &'static str {
        "havok17"
    }

    fn create_body(&mut self, desc: &BodyDesc) -> Result<BodyHandle, BackendError> {
        let shape = JoltBackend::build_shape(&desc.shape)?;
        let phantom = desc
            .phantom
            .as_ref()
            .map(JoltBackend::build_shape)
            .transpose()?;
        let settings = match desc.motion {
            MotionType::Static => BodySettings::new_static(),
            _ => BodySettings::new_kinematic(),
        }
        .position(rv(desc.position))
        .rotation(jq(desc.rotation))
        .allow_sleeping(false)
        .user_data(desc.user_data);
        let id = self
            .world
            .create_body(&shape, &settings)
            .map_err(|e| BackendError::Body(format!("{e:?}")))?;
        let (mass, com_local, inv_inertia) = if desc.motion == MotionType::Dynamic {
            Self::mass_of(&desc.shape, desc.mass)
        } else {
            (0.0, mass::of_shape(&desc.shape).com, Mat3::ZERO)
        };
        let mut body = Body {
            alive: true,
            motion: desc.motion,
            sensor: desc.is_sensor,
            group: desc.collision_group,
            com: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            com_local,
            mass,
            inv_mass: if mass > 0.0 { 1.0 / mass } else { 0.0 },
            inv_inertia_local: inv_inertia,
            inv_inertia_world: Mat3::ZERO,
            force: Vec3::ZERO,
            torque: Vec3::ZERO,
            friction: desc.friction,
            restitution: desc.restitution,
        };
        body.set_origin(desc.position, desc.rotation);
        let handle = self.bodies.len();
        self.bodies.push(body);
        self.lookup.insert(id.to_raw(), handle);
        self.geometry.push(Some(Geometry {
            id,
            shape,
            phantom,
            pieces: narrow::pieces_of(&desc.shape),
        }));
        self.phantom_touch.push(None);
        Ok(handle as BodyHandle)
    }

    fn remove_body(&mut self, handle: BodyHandle) {
        let i = handle as usize;
        let Some(b) = self.bodies.get_mut(i) else {
            return;
        };
        b.alive = false;
        if let Some(g) = self.geometry[i].take() {
            let _ = self.world.remove_body(g.id);
            self.lookup.remove(&g.id.to_raw());
        }
        self.contacts.retain(|c| c.a != i && c.b != i);
        self.mesh_manifolds.retain(|k, _| k.0 != i && k.2 != i);
        self.convex_manifolds.retain(|k, _| k.0 != i && k.2 != i);
        self.excused.retain(|k| k.0 != i && k.2 != i);
        for j in &mut self.joints {
            if j.a == i || j.b == i {
                j.alive = false;
            }
        }
    }

    fn state(&self, handle: BodyHandle) -> Option<BodyState> {
        self.body(handle).map(Body::state)
    }

    fn set_transform(&mut self, handle: BodyHandle, position: Vec3, rotation: Quat) {
        if let Some(b) = self.body_mut(handle) {
            b.set_origin(position, rotation);
            self.sync_one(handle as usize);
        }
    }

    fn apply_impulse(&mut self, handle: BodyHandle, impulse: Vec3) {
        if let Some(b) = self.body_mut(handle) {
            let com = b.com;
            b.apply_impulse_at(impulse, com);
        }
    }

    fn apply_impulse_at(&mut self, handle: BodyHandle, impulse: Vec3, point: Vec3) {
        if let Some(b) = self.body_mut(handle) {
            b.apply_impulse_at(impulse, point);
        }
    }

    fn apply_angular_impulse(&mut self, handle: BodyHandle, torque_impulse: Vec3) {
        if let Some(b) = self.body_mut(handle) {
            b.apply_angular_impulse(torque_impulse);
        }
    }

    fn set_velocity(&mut self, handle: BodyHandle, linear: Vec3, angular: Vec3) {
        if let Some(b) = self.body_mut(handle) {
            if b.motion != MotionType::Static {
                b.velocity = linear;
                b.angular_velocity = angular;
            }
        }
    }

    fn gravity(&self) -> Vec3 {
        GRAVITY
    }

    fn mass(&self, handle: BodyHandle) -> Option<f32> {
        self.body(handle).filter(|b| b.is_dynamic()).map(|b| b.mass)
    }

    fn center_of_mass(&self, handle: BodyHandle) -> Option<Vec3> {
        self.body(handle).map(|b| b.com)
    }

    fn add_force(&mut self, handle: BodyHandle, force: Vec3) {
        if let Some(b) = self.body_mut(handle) {
            b.force += force;
        }
    }

    fn add_torque(&mut self, handle: BodyHandle, torque: Vec3) {
        if let Some(b) = self.body_mut(handle) {
            b.torque += torque;
        }
    }

    fn phantom_touch(&self, handle: BodyHandle) -> Option<Vec3> {
        self.phantom_touch.get(handle as usize).copied().flatten()
    }

    fn last_dt(&self) -> f32 {
        self.last_dt
    }

    fn step(&mut self, dt: f32) -> StepInfo {
        self.last_dt = dt;
        self.events.clear();
        let mut done = 0.0f32;
        let mut pieces = 0;
        let mut complete = true;
        // The tolerance a detection's points must keep (`setCurrentTolerance`):
        // half the collision tolerance at the first attempt, halved again at
        // every attempt for the rest of this substep (`0x52aa60`).
        let mut current_tol = TOLERANCE;
        while done < dt * (1.0 - 1e-4) {
            pieces += 1;
            self.pieces += 1;
            let mut h = dt - done;
            // 1-2: the events of the last detection, then their impulses. The
            // resolver works with the world's tolerance (`vt + 0x18`).
            self.fire_events();
            let contacts = std::mem::take(&mut self.contacts);
            resolver::resolve(&mut self.bodies, &contacts, TOLERANCE);
            self.contacts = contacts;
            // 3: integrate and detect, halving the step on a bad detection.
            let saved = self.save();
            let mut backsteps = 0;
            loop {
                current_tol *= 0.5;
                self.integrate(h);
                let (contacts, bad) = self.detect(current_tol);
                if !bad.is_empty()
                    && h > MIN_STEP
                    && backsteps < MAX_BACKSTEPS
                    && pieces < MAX_PIECES
                {
                    self.restore(&saved);
                    h = (h * 0.5).max(MIN_STEP);
                    backsteps += 1;
                    self.backsteps += 1;
                    continue;
                }
                if !bad.is_empty() {
                    // "Backstep/Bad Step": accept it, and let its pairs
                    // overlap from now on (`FUN_00542760`).
                    complete = false;
                    self.excused.extend(bad);
                }
                self.contacts = contacts;
                break;
            }
            // 4: actions. The motors use the substep, not the piece.
            joints::solve(&mut self.bodies, &mut self.joints, dt);
            self.drag();
            done += h;
            if pieces >= MAX_PIECES {
                // Ours: give the rest of the substep to one last piece.
                if done < dt * (1.0 - 1e-4) {
                    let rest = dt - done;
                    self.fire_events();
                    let contacts = std::mem::take(&mut self.contacts);
                    resolver::resolve(&mut self.bodies, &contacts, TOLERANCE);
                    self.integrate(rest);
                    let (contacts, bad) = self.detect(current_tol * 0.5);
                    self.excused.extend(bad);
                    self.contacts = contacts;
                    joints::solve(&mut self.bodies, &mut self.joints, dt);
                    self.drag();
                }
                break;
            }
        }
        self.detect_phantoms();
        if let Some(watch) = debug_body() {
            if let Some(b) = self.bodies.get(watch) {
                let mine: Vec<String> = self
                    .contacts
                    .iter()
                    .filter(|c| c.a == watch || c.b == watch)
                    .map(|c| {
                        let r = c.point - b.com;
                        format!(
                            "[{}-{} d {:.3} r ({:.3},{:.3},{:.3}) n ({:.2},{:.2},{:.2})]",
                            c.a, c.b, c.distance, r.x, r.y, r.z, c.normal.x, c.normal.y, c.normal.z
                        )
                    })
                    .collect();
                eprintln!(
                    "h17 body {watch}: pieces {pieces} com ({:.4},{:.4},{:.4}) v ({:.2},{:.2},{:.2}) w ({:.2},{:.2},{:.2}) {}",
                    b.com.x,
                    b.com.y,
                    b.com.z,
                    b.velocity.x,
                    b.velocity.y,
                    b.velocity.z,
                    b.angular_velocity.x,
                    b.angular_velocity.y,
                    b.angular_velocity.z,
                    mine.join(" ")
                );
            }
        }
        StepInfo { complete }
    }

    fn take_contacts(&mut self) -> Vec<ContactEvent> {
        std::mem::take(&mut self.events)
    }

    fn in_contact(&self, a: BodyHandle, b: BodyHandle) -> bool {
        let (a, b) = (a as usize, b as usize);
        self.contacts
            .iter()
            .any(|c| (c.a == a && c.b == b) || (c.a == b && c.b == a))
    }

    fn ray_cast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<RayHit> {
        self.ray_cast_excluding(origin, direction, max_distance, u32::MAX)
    }

    fn ray_cast_excluding(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        exclude: BodyHandle,
    ) -> Option<RayHit> {
        let dir = direction.normalize_or_zero() * max_distance;
        if dir == Vec3::ZERO {
            return None;
        }
        let ray = RayCast::new(rv(origin), jv(dir));
        let skip = self
            .geometry
            .get(exclude as usize)
            .and_then(|g| g.as_ref())
            .map(|g| g.id);
        let filter = match skip {
            Some(id) => QueryFilter::new().exclude_body(id),
            None => QueryFilter::default(),
        };
        let hit = self.world.cast_ray(&ray, &filter).ok()??;
        Some(RayHit {
            body: self
                .lookup
                .get(&hit.body.to_raw())
                .map(|&h| h as BodyHandle)
                .unwrap_or(u32::MAX),
            distance: hit.distance,
            point: origin + direction.normalize_or_zero() * hit.distance,
            normal: un_v(hit.normal),
        })
    }

    fn create_hinge(&mut self, desc: &HingeDesc) -> Result<JointHandle, BackendError> {
        let (a, b) = (desc.body_a as usize, desc.body_b as usize);
        let (Some(ba), Some(bb)) = (self.body(desc.body_a), self.body(desc.body_b)) else {
            return Err(BackendError::Body("hinge on a missing body".into()));
        };
        let axis_a = desc.axis_a.normalize_or_zero();
        let axis_b = desc.axis_b.normalize_or_zero();
        if axis_a == Vec3::ZERO || axis_b == Vec3::ZERO {
            return Err(BackendError::Body("hinge without an axis".into()));
        }
        let ref_a = joints::perpendicular(axis_a);
        let ref_b = bb.rotation.inverse() * (ba.rotation * ref_a);
        let free = desc.limits[0] <= -std::f32::consts::PI + 1e-4
            && desc.limits[1] >= std::f32::consts::PI - 1e-4;
        self.joints.push(Joint {
            alive: true,
            a,
            b,
            anchor_a: desc.anchor_a,
            anchor_b: desc.anchor_b,
            kind: Kind::Hinge {
                axis_a,
                axis_b,
                ref_a,
                ref_b,
            },
            limits: desc.limits,
            limited: !free,
            motor: false,
            max_torque: desc.max_torque,
            target_velocity: 0.0,
            target: 0.0,
            friction: desc.friction_torque,
            saved_limits: None,
        });
        Ok((self.joints.len() - 1) as JointHandle)
    }

    fn create_slider(&mut self, desc: &SliderDesc) -> Result<JointHandle, BackendError> {
        let (a, b) = (desc.body_a as usize, desc.body_b as usize);
        let (Some(ba), Some(bb)) = (self.body(desc.body_a), self.body(desc.body_b)) else {
            return Err(BackendError::Body("slider on a missing body".into()));
        };
        let axis_a = desc.axis_a.normalize_or_zero();
        if axis_a == Vec3::ZERO {
            return Err(BackendError::Body("slider without an axis".into()));
        }
        let rest = ba.rotation.inverse() * bb.rotation;
        self.joints.push(Joint {
            alive: true,
            a,
            b,
            anchor_a: desc.anchor_a,
            anchor_b: desc.anchor_b,
            kind: Kind::Slider { axis_a, rest },
            limits: desc.limits,
            limited: true,
            motor: false,
            max_torque: desc.max_force,
            target_velocity: 0.0,
            target: 0.0,
            friction: desc.friction_force,
            saved_limits: None,
        });
        Ok((self.joints.len() - 1) as JointHandle)
    }

    fn drive_slider(&mut self, handle: JointHandle, target_velocity: f32) {
        self.set_motor(handle, target_velocity);
    }

    fn slider_offset(&self, handle: JointHandle) -> f32 {
        self.joint(handle)
            .map(|j| j.position(&self.bodies))
            .unwrap_or(0.0)
    }

    fn remove_joint(&mut self, handle: JointHandle) {
        if let Some(j) = self.joints.get_mut(handle as usize) {
            j.alive = false;
        }
    }

    fn drive_hinge(&mut self, handle: JointHandle, target_velocity: f32) {
        self.set_motor(handle, target_velocity);
    }

    fn hinge_angle(&self, handle: JointHandle) -> f32 {
        self.joint(handle)
            .map(|j| j.position(&self.bodies))
            .unwrap_or(0.0)
    }

    fn set_motion_type(
        &mut self,
        handle: BodyHandle,
        motion: MotionType,
    ) -> Result<(), BackendError> {
        let i = handle as usize;
        let Some(b) = self.bodies.get(i).filter(|b| b.alive) else {
            return Err(BackendError::Body("motion type on a missing body".into()));
        };
        if motion == MotionType::Dynamic && b.mass <= 0.0 {
            return Err(BackendError::Body(
                "a body without mass cannot be dynamic".into(),
            ));
        }
        let b = &mut self.bodies[i];
        b.motion = motion;
        if motion != MotionType::Dynamic {
            b.force = Vec3::ZERO;
            b.torque = Vec3::ZERO;
        }
        if motion == MotionType::Static {
            b.velocity = Vec3::ZERO;
            b.angular_velocity = Vec3::ZERO;
        }
        Ok(())
    }

    fn set_joint_strength(&mut self, handle: JointHandle, strength: f32, friction: f32) {
        if let Some(j) = self.joint_mut(handle) {
            j.max_torque = strength;
            j.friction = friction;
        }
    }

    fn disable_collision(&mut self, a: BodyHandle, b: BodyHandle) {
        let (a, b) = (a as usize, b as usize);
        self.disabled.insert((a.min(b), a.max(b)));
    }

    fn lock_joint(&mut self, handle: JointHandle, locked: bool) -> bool {
        let Some(position) = self.joint(handle).map(|j| j.position(&self.bodies)) else {
            return true;
        };
        let Some(j) = self.joint_mut(handle) else {
            return true;
        };
        match (locked, j.saved_limits) {
            (true, None) => {
                // A slider locks at its offset clamped to its travel
                // (`FUN_004b50c0`); a hinge at its angle (`FUN_00480e20`).
                let at = if matches!(j.kind, Kind::Slider { .. }) {
                    position.clamp(j.limits[0], j.limits[1])
                } else {
                    position
                };
                j.saved_limits = Some((j.limits, j.limited));
                j.limits = [at, at];
                j.limited = true;
                j.motor = false;
                j.target_velocity = 0.0;
            }
            (false, Some((limits, limited))) => {
                j.limits = limits;
                j.limited = limited;
                j.saved_limits = None;
            }
            _ => {}
        }
        true
    }

    fn set_body_shape(
        &mut self,
        handle: BodyHandle,
        shape: &ShapeDesc,
        mass: Option<f32>,
    ) -> Result<(), BackendError> {
        let i = handle as usize;
        if self.body(handle).is_none() {
            return Err(BackendError::Body("set_shape on a missing body".into()));
        }
        let built = JoltBackend::build_shape(shape)?;
        if let Some(g) = &mut self.geometry[i] {
            if let Ok(mut body) = self.world.body_mut(g.id) {
                body.set_shape(&built, None, Activation::DontActivate)
                    .map_err(|e| BackendError::Body(format!("set_shape: {e:?}")))?;
            }
            g.shape = built;
            g.pieces = narrow::pieces_of(shape);
        }
        let b = &mut self.bodies[i];
        let (origin, rotation) = (b.origin(), b.rotation);
        if b.motion == MotionType::Dynamic {
            let (m, com, inv) = Self::mass_of(shape, mass.or(Some(b.mass)));
            b.mass = m;
            b.inv_mass = 1.0 / m;
            b.com_local = com;
            b.inv_inertia_local = inv;
        } else {
            b.com_local = mass::of_shape(shape).com;
        }
        b.set_origin(origin, rotation);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
