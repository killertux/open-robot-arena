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
pub mod dashpot;
pub mod deactivation;
pub mod joints;
pub mod mass;
pub mod mopp;
pub mod narrow;
pub mod obbtree;
pub mod resolver;

use std::collections::HashMap;

use glam::{Mat3, Quat, Vec3, Vec4};
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
/// `RA2_H17_BACKSTEPS=1` prints every backstep with the pairs that caused it.
fn debug_body() -> Option<usize> {
    use std::sync::OnceLock;
    static WATCH: OnceLock<Option<usize>> = OnceLock::new();
    *WATCH.get_or_init(|| std::env::var("RA2_H17_DEBUG").ok()?.parse().ok())
}

/// Whether a fixed mesh's triangles come in its MOPP's order (`RA2_H17_MOPP=0`:
/// ascending, as before docs/29 section 12v; a diagnostic).
fn mopp_order() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("RA2_H17_MOPP").map_or(true, |v| v != "0"))
}

/// `RA2_H17_DUMP=<file>` writes the rows `frida/ra2_contacts.js` captures in
/// the original (`R` the contacts handed to the resolver, `B` every dynamic
/// body, `A` each accepted step piece), so one script reads both.
pub fn dump_line(line: std::fmt::Arguments) {
    use std::io::Write;
    use std::sync::{Mutex, OnceLock};
    static DUMP: OnceLock<Option<Mutex<std::io::BufWriter<std::fs::File>>>> = OnceLock::new();
    let Some(out) = DUMP.get_or_init(|| {
        let path = std::env::var_os("RA2_H17_DUMP")?;
        std::fs::File::create(path)
            .ok()
            .map(|f| Mutex::new(std::io::BufWriter::new(f)))
    }) else {
        return;
    };
    if let Ok(mut w) = out.lock() {
        let _ = w.write_fmt(line);
        let _ = w.write_all(b"\n");
        let _ = w.flush();
    }
}

pub fn dumping() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("RA2_H17_DUMP").is_some())
}

fn dump_vec(v: Vec3) -> String {
    format!("[{},{},{}]", v.x, v.y, v.z)
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
    /// The phantom's pieces, body-local (the start-point lowering's test sees
    /// them, [`Havok17Backend::spawn_touch`]).
    phantom_pieces: Vec<Piece>,
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
    /// Its box in the broadphase (`+0x20`/`+0x30`, without the tolerance):
    /// a moving collidable's the cube around its bounding sphere, a fixed
    /// one's its box.
    sweep: Aabb,
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
    /// The angular dashpots (actions, run before the constraint solver).
    dashpots: Vec<dashpot::AngularDashpot>,
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
    /// The robots' systems ([`deactivation`]), one per collision group.
    systems: Vec<deactivation::System>,
    /// The world deactivator's timers.
    world_deactivator: deactivation::WorldDeactivator,
    /// The world's time, as the deactivators read it.
    clock: deactivation::Clock,
    /// Substeps run.
    substeps: u64,
    /// `RA2_H17_DEACTIVATION=0` turns the deactivators off (diagnostics).
    deactivation: bool,
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
            dashpots: Vec::new(),
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
            systems: Vec::new(),
            world_deactivator: Default::default(),
            clock: Default::default(),
            substeps: 0,
            deactivation: std::env::var("RA2_H17_DEACTIVATION").map_or(true, |v| v != "0"),
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
            // An inactive body is not integrated and keeps its accumulators
            // for when it wakes (`0x52f090` integrates the active list only).
            if !b.moves() || b.asleep() {
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
                let sweep = if b.moves() {
                    let c = pose.apply(p.sphere.0);
                    let r = Vec3::splat(p.sphere.1);
                    Aabb {
                        min: c - r,
                        max: c + r,
                    }
                } else {
                    bounds
                };
                out.push(WorldPiece {
                    body: i,
                    piece: k as u32,
                    body_pose: body,
                    pose,
                    vertices,
                    radius,
                    bounds,
                    sweep,
                });
            }
        }
        out
    }

    /// The candidate pairs in the order the broadphase lists them
    /// (`FUN_00559d30`), which is the order the narrowphase appends their
    /// contacts and so the resolver's: the collidables are kept in three lists
    /// (active, inactive, fixed), each sorted by its box's lower bound along
    /// the sweep axis, the world's x (`0x5411f0`: `+0x1d4 = (1, 0, 0)`;
    /// `0x559c20`, `0x559ec0`); for every active collidable in turn come its
    /// pairs with the active ones after it, then with the inactive ones, then
    /// with the fixed ones, each in its list's order (`0x55a750` keeps those
    /// whose boxes overlap). Live (`twobody-scout-alarm-box__live__001`, a
    /// rest): the floor's contacts come wheel by wheel in ascending x. A
    /// moving collidable's box is the cube around its smallest bounding
    /// sphere (ours `WorldPiece::sweep`), so it does not turn with the body; a
    /// fixed one's is its box (live `X` rows, docs/29 section 12u). The lists
    /// are rebuilt from the collidables every detection and sorted afresh
    /// (`0x559c20` projects, `0x559ec0` sorts: insertion sort up to 16
    /// entries), so a woken or new pair has no insertion order of its own.
    fn broadphase_pairs(
        &self,
        pieces: &[WorldPiece],
        level: &[Option<u32>],
    ) -> Vec<(usize, usize)> {
        let key = |k: &usize| (pieces[*k].sweep.min.x, *k);
        let sorted = |mut v: Vec<usize>| {
            v.sort_by(|a, b| {
                key(a)
                    .partial_cmp(&key(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            v
        };
        let (mut active, mut inactive, mut fixed) = (Vec::new(), Vec::new(), Vec::new());
        for (k, p) in pieces.iter().enumerate() {
            if !self.bodies[p.body].moves() {
                fixed.push(k);
            } else if level[p.body].is_some() {
                active.push(k);
            } else {
                inactive.push(k);
            }
        }
        let (active, inactive, fixed) = (sorted(active), sorted(inactive), sorted(fixed));
        let tol = TOLERANCE;
        let mut out = Vec::new();
        for (n, &a) in active.iter().enumerate() {
            let ba = pieces[a].sweep.grown(tol);
            for &b in active[n + 1..].iter().chain(&inactive).chain(&fixed) {
                if ba.overlaps(&pieces[b].sweep) {
                    out.push((a, b));
                }
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
        // Only pairs with an active body are detected (the broadphase keeps
        // the inactive collidables apart, `0x55a9a0`). A detected pair whose
        // other body is inactive wakes it (`0x548da0`), and its own pairs are
        // then detected in the same detection (`0x542db0` runs again): `level`
        // is the pass a body became active in.
        let mut level: Vec<Option<u32>> =
            self.bodies.iter().map(|b| b.awake().then_some(0)).collect();
        let mut pass = 0u32;
        loop {
            for (ia, ib) in self.broadphase_pairs(&pieces, &level) {
                // The pair's A is the collidable made later (`0x55a750`: the
                // greater shape type first, a convex piece before a mesh, which
                // the agents below see to; between equal types the greater
                // `+0x50`, the collidables' creation count, live
                // `ord-scout-alarm-box__live__000`): ours, the greater body
                // handle and piece (the arena's bodies come first, then the
                // robots in order). The resolver does not see the order (the
                // impulse and the push-out are the same either way round).
                let (pa, pb) = {
                    let (a, b) = (&pieces[ia], &pieces[ib]);
                    if (b.body, b.piece) > (a.body, a.piece) {
                        (b, a)
                    } else {
                        (a, b)
                    }
                };
                let i = pa.body;
                let bi = &self.bodies[i];
                let (dynamic_i, friction_i, restitution_i) =
                    (bi.is_dynamic(), bi.friction, bi.restitution);
                {
                    let j = pb.body;
                    if j == i {
                        continue;
                    }
                    let bj = &self.bodies[j];
                    if !dynamic_i && !bj.is_dynamic() {
                        continue;
                    }
                    let (friction_j, restitution_j) = (bj.friction, bj.restitution);
                    if self.same_group(i, j) {
                        continue;
                    }
                    let first_awake = match (level[i], level[j]) {
                        (Some(a), Some(b)) => a.min(b),
                        (Some(a), None) | (None, Some(a)) => a,
                        (None, None) => continue,
                    };
                    if first_awake != pass {
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
                                    let kept = m.update(
                                        p,
                                        c.persists(),
                                        tol,
                                        &pa.body_pose,
                                        &pb.body_pose,
                                    );
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
                            penetrating = self.convex_mesh(
                                key,
                                cx,
                                ms,
                                tol,
                                stamp,
                                &mut triangles,
                                &mut points,
                            );
                        }
                        (true, true) => {
                            // Mesh x mesh: a fixed mesh (its MOPP) is A
                            // (`FUN_0054a2a0`); two moving meshes keep the
                            // pair's order (`FUN_00549d00`).
                            swap = !self.bodies[pb.body].moves();
                            let (ma, mb) = if swap { (pb, pa) } else { (pa, pb) };
                            penetrating =
                                self.mesh_mesh(ma, mb, tol, &out, &mut triangles, &mut points);
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
                        let pair = if self.bodies[i].moves() {
                            (i, j)
                        } else {
                            (j, i)
                        };
                        if !penetrating_bodies.contains(&pair) {
                            penetrating_bodies.push(pair);
                        }
                    } else if out[first..].iter().any(|c| c.distance < current_tol) {
                        bad.push(key);
                    }
                }
            }
            let mut woke = false;
            for c in &out {
                for k in [c.a, c.b] {
                    if level[k].is_none() && self.bodies[k].asleep() {
                        // `0x548ef0`: `setActive(1, 0)`, the references kept.
                        self.bodies[k].active = true;
                        level[k] = Some(pass + 1);
                        woke = true;
                    }
                }
            }
            if !woke {
                break;
            }
            pass += 1;
        }
        // Ours: Havok reports nothing for overlapping shapes and backsteps
        // until they are apart; when the step is accepted overlapping anyway,
        // the deepest points Jolt finds for the pair push it apart.
        for (i, j) in penetrating_bodies {
            out.extend(self.overlap_contacts(i, j));
        }
        // A pair between two inactive bodies keeps its agent (the frozen
        // pairs, `0x548ab0`) for when one of them wakes.
        let bodies = &self.bodies;
        let frozen = |k: &PairKey| !bodies[k.0].awake() && !bodies[k.2].awake();
        self.mesh_manifolds
            .retain(|k, m| m.seen == stamp || frozen(k));
        self.convex_manifolds
            .retain(|k, m| m.seen == stamp || frozen(k));
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
        // The agent meets the triangles in the order its query gives them: a
        // moving mesh's tree depth first (`FUN_00550560`), a fixed mesh's
        // MOPP's leaves (`havok17::mopp`).
        if self.bodies[ms.body].moves() {
            triangles.sort_by_key(|&t| mesh.tree.rank[t as usize]);
        } else if mopp_order() {
            triangles.sort_by_key(|&t| mesh.mopp[t as usize]);
        }
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

    /// The mesh x mesh agents (`pa` is the pair's A, the normals point from B
    /// toward it): every pair of triangles, one of each mesh, within the
    /// tolerance of each other gives the midpoint of their closest points,
    /// unless a point already reported in this detection lies within
    /// `sqrt(0.001)` tolerances of it. No manifold is kept. `true` when two
    /// triangles touch (`d < 1e-5`).
    ///
    /// The order the pairs of triangles come in (docs/29 section 12u): with a
    /// fixed mesh as A (`FUN_0054a2a0`), B's tree is walked depth first
    /// (`FUN_0054a080`) and each of its triangles near A's MOPP brings A's
    /// triangles near it in the MOPP's order ([`super::mopp`]); between two
    /// moving meshes
    /// (`FUN_00549d00`) the two trees are walked together (`FUN_00550100`,
    /// [`obbtree::ObbTree::pairs`]). Either walk keeps every pair of
    /// triangles within the tolerance: the two-tree walk tests each pair of
    /// boxes with both grown by half the tolerance (`FUN_00557870`), so a
    /// whole tolerance between them (ours: the first's box grown by the
    /// whole tolerance against the other's).
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
        let mut candidates: Vec<(u32, u32)> = Vec::new();
        if !self.bodies[pa.body].moves() {
            let mut order: Vec<u32> = (0..mb.triangles.len() as u32).collect();
            order.sort_by_key(|&t| mb.tree.rank[t as usize]);
            for tb in order {
                let tri_b =
                    mb.triangles[tb as usize].map(|v| pb.pose.apply(mb.vertices[v as usize]));
                let local = Aabb::of(tri_b.iter().map(|v| pa.pose.inverse_apply(*v))).grown(tol);
                ma.query(&local, triangles);
                if mopp_order() {
                    triangles.sort_by_key(|&t| ma.mopp[t as usize]);
                }
                candidates.extend(triangles.iter().map(|&ta| (ta, tb)));
            }
        } else {
            let rot = Mat3::from_quat(pa.pose.rotation.inverse() * pb.pose.rotation);
            let pos = pa.pose.inverse_apply(pb.pose.position);
            obbtree::ObbTree::pairs(&ma.tree, &mb.tree, (rot, pos), tol, &mut candidates);
        }
        for (ta, tb) in candidates {
            let tri_a = ma.triangles[ta as usize].map(|v| pa.pose.apply(ma.vertices[v as usize]));
            let tri_b = mb.triangles[tb as usize].map(|v| pb.pose.apply(mb.vertices[v as usize]));
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
    /// end of `stepTo`). A phantom is a collidable flagged `DISABLE_COLLISIONS`,
    /// never resolved, and its pair is listed only while it overlaps: at a
    /// negative distance, as a SmartZone's (docs/29 sections 12d, 12i), not
    /// within the tolerance a solid pair is detected at. Live (section 12r):
    /// ALARM on its tail, its rear wheels' traction rings 0.07-0.09 m off the
    /// floor, has none of its wheels touching and so no thrust, and a robot
    /// rolling back onto its wheels finds each wheel touching only once its
    /// ring reaches the floor.
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
            let query = CollideShape::new(phantom, rv(b.origin()), jq(b.rotation));
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
                if n.length_squared() < 0.5 || hit.penetration_depth < 0.0 {
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
            if b.is_dynamic() && b.active {
                b.force -= b.velocity * DRAG_LINEAR;
                b.torque -= b.angular_velocity * DRAG_ANGULAR;
            }
        }
    }

    /// The game pushed `handle` (its force, torque and impulse wrappers wake
    /// the body first, `0x4b0860`/`0x4b0950`/`0x4b0a10`).
    fn wake_pushed(&mut self, handle: BodyHandle) {
        if let Some(b) = self.bodies.get_mut(handle as usize) {
            if b.asleep() {
                b.active = true;
                b.freeze.forget();
            }
        }
    }

    /// The squared half diagonal of body `i`'s collidables' bounding box
    /// where they are now (`0x522cb0`'s first check).
    fn bounding_radius2(&self, i: usize) -> f32 {
        let mut bounds: Option<Aabb> = None;
        for w in self.world_pieces().iter().filter(|w| w.body == i) {
            bounds = Some(match bounds {
                Some(b) => b.union(w.bounds),
                None => w.bounds,
            });
        }
        if std::env::var_os("RA2_H17_DEACT_DEBUG").is_some() {
            for w in self.world_pieces().iter().filter(|w| w.body == i) {
                eprintln!(
                    "deact bounds body {i} piece {} {:?} {:?}",
                    w.piece, w.bounds.min, w.bounds.max
                );
            }
        }
        bounds.map_or(0.0, |b| ((b.max - b.min) * 0.5).length_squared())
    }

    /// The world's deactivator and the robots' systems, once a frame
    /// ([`deactivation`]).
    fn run_deactivators(&mut self) {
        use deactivation::{
            SYSTEM_LONG_PERIOD, SYSTEM_SHORT_PERIOD, THRESHOLDS, WORLD_LONG_PERIOD,
            WORLD_SHORT_PERIOD,
        };
        let now = self.clock;
        // The world's (`0x53cc00`): entities in no system.
        let w = self.world_deactivator;
        let short = WORLD_SHORT_PERIOD < now.since(&w.last_short);
        let long = WORLD_LONG_PERIOD < now.since(&w.last_long);
        if short || long {
            for i in 0..self.bodies.len() {
                let b = &self.bodies[i];
                if !b.alive || b.motion != MotionType::Dynamic || !b.active || b.system.is_some() {
                    continue;
                }
                if self.freeze_check(i, short, long, &THRESHOLDS) {
                    self.bodies[i].active = false;
                }
            }
            if short {
                self.world_deactivator.last_short = now;
            }
            if long {
                self.world_deactivator.last_long = now;
            }
        }
        // The systems' (`0x534050`).
        for s in 0..self.systems.len() {
            let bodies: Vec<usize> = self.systems[s]
                .bodies
                .iter()
                .copied()
                .filter(|&i| self.bodies[i].alive)
                .collect();
            if bodies.is_empty() {
                continue;
            }
            if !self.systems[s].active {
                if bodies.iter().any(|&i| self.bodies[i].active) {
                    self.systems[s].active = true;
                    for &i in &bodies {
                        let b = &mut self.bodies[i];
                        b.active = true;
                        b.freeze.forget();
                    }
                }
                continue;
            }
            let sys = &self.systems[s];
            let short = SYSTEM_SHORT_PERIOD < now.since(&sys.last_short);
            let long = SYSTEM_LONG_PERIOD < now.since(&sys.last_long);
            if !short && !long {
                continue;
            }
            if short {
                self.systems[s].last_short = now;
            }
            if long {
                self.systems[s].last_long = now;
            }
            let t = self.systems[s].thresholds;
            let mut all = true;
            for &i in &bodies {
                all &= self.freeze_check(i, short, long, &t);
            }
            if all {
                self.systems[s].active = false;
                for &i in &bodies {
                    self.bodies[i].active = false;
                }
            }
        }
    }

    fn freeze_check(&mut self, i: usize, short: bool, long: bool, t: &[f32; 4]) -> bool {
        let r2 = if self.bodies[i].freeze.radius2.is_none() {
            self.bounding_radius2(i)
        } else {
            0.0
        };
        let b = &mut self.bodies[i];
        let (com, rotation) = (b.com, b.rotation);
        let before = b.freeze;
        let frozen = b.freeze.check(com, rotation, || r2, short, long, t);
        if std::env::var_os("RA2_H17_DEACT_DEBUG").is_some() {
            let d = |r: Option<(Vec3, Quat)>| {
                r.map(|(c, q)| {
                    let dq = Vec4::from(q) - Vec4::from(rotation);
                    format!(
                        "{:.4}/{:.4}",
                        (c - com).length(),
                        dq.length() * before.radius2.unwrap_or(r2).sqrt()
                    )
                })
                .unwrap_or_else(|| "-".into())
            };
            eprintln!(
                "deact t={:.3} body {i} short {short} long {long} r2 {:.4} s {} l {} -> {frozen}",
                self.clock.since(&Default::default()),
                b.freeze.radius2.unwrap_or(0.0),
                d(before.short),
                d(before.long)
            );
        }
        frozen
    }

    fn joint(&self, handle: JointHandle) -> Option<&Joint> {
        self.joints.get(handle as usize).filter(|j| j.alive)
    }

    fn joint_mut(&mut self, handle: JointHandle) -> Option<&mut Joint> {
        self.joints.get_mut(handle as usize).filter(|j| j.alive)
    }

    /// The `R` and `B` rows of [`dump_line`].
    fn dump_resolver_entry(&self) {
        let contacts: Vec<String> = self
            .contacts
            .iter()
            .map(|c| {
                format!(
                    "[\"h{}\",\"h{}\",{},{},{},{},{},{},{},\"h{}\",\"h{}\"]",
                    c.a,
                    c.b,
                    dump_vec(c.normal),
                    dump_vec(c.point),
                    c.distance,
                    c.friction,
                    c.friction,
                    c.restitution,
                    c.restitution,
                    c.a,
                    c.b
                )
            })
            .collect();
        dump_line(format_args!(
            "[\"R\",0,{TOLERANCE},[{}]]",
            contacts.join(",")
        ));
        for (i, b) in self.bodies.iter().enumerate() {
            if !b.is_dynamic() {
                continue;
            }
            // The rotation column by column, as the original's body `+0xcc`
            // holds it.
            let r = Mat3::from_quat(b.rotation).to_cols_array();
            dump_line(format_args!(
                "[\"B\",0,\"h{i}\",{},{},{},{},{:?}]",
                b.mass,
                dump_vec(b.com),
                dump_vec(b.velocity),
                dump_vec(b.angular_velocity),
                r
            ));
        }
    }

    /// The `V` rows of [`dump_line`]: every dynamic body as the resolver
    /// leaves it (with the `B` rows, each body's contact impulse).
    fn dump_resolver_exit(&self) {
        for (i, b) in self.bodies.iter().enumerate() {
            if b.is_dynamic() {
                dump_line(format_args!(
                    "[\"V\",0,\"h{i}\",{},{}]",
                    dump_vec(b.velocity),
                    dump_vec(b.angular_velocity)
                ));
            }
        }
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
                // The atom's target angle is never reset when a motor comes
                // on: it keeps where the servo last left it (0 from the atom's
                // construction, `0x5359d0`), a hinge's only moved by whole turns
                // to within π of the angle (`0x5355a0`). Live
                // (`firetiming-scout-alarm__live__000`): the Scout's fork locks
                // at -1.18 rad with its target at -2.59, and 29 substeps later
                // the cocking motor starts from that same -2.59.
                if !j.motor && matches!(j.kind, Kind::Hinge { .. }) {
                    j.target = position + joints::wrap(j.target - position);
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
            let (m, com, inv) = Self::mass_of(&desc.shape, desc.mass);
            match desc.inertia.filter(|i| i.determinant().abs() > 1e-12) {
                Some(i) => (m, com, i.inverse()),
                None => (m, com, inv),
            }
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
            active: true,
            freeze: Default::default(),
            system: None,
        };
        body.set_origin(desc.position, desc.rotation);
        let handle = self.bodies.len();
        // Every robot is a system of its bodies (`0x5236e0`).
        if let (Some(g), MotionType::Dynamic) = (desc.collision_group, desc.motion) {
            let s = match self.systems.iter().position(|s| s.group == g.group) {
                Some(s) => s,
                None => {
                    self.systems.push(deactivation::System::new(g.group));
                    self.systems.len() - 1
                }
            };
            self.systems[s].bodies.push(handle);
            body.system = Some(s);
        }
        self.bodies.push(body);
        self.lookup.insert(id.to_raw(), handle);
        self.geometry.push(Some(Geometry {
            id,
            shape,
            phantom,
            pieces: narrow::pieces_of(&desc.shape),
            phantom_pieces: desc
                .phantom
                .as_ref()
                .map(narrow::pieces_of)
                .unwrap_or_default(),
        }));
        self.phantom_touch.push(None);
        if std::env::var_os("RA2_H17_PIECES").is_some() {
            // Diagnostics: every piece of the body, as `frida/ra2_order.js`'s `C` rows.
            let g = self.geometry[handle].as_ref().unwrap();
            for (k, p) in g.pieces.iter().enumerate() {
                let (kind, v, t, leaves): (&str, Vec<Vec3>, Vec<[u32; 3]>, Vec<usize>) =
                    match &p.shape {
                        PieceShape::Convex { vertices, .. } => {
                            ("convex", vertices.clone(), Vec::new(), Vec::new())
                        }
                        PieceShape::Mesh(m) => {
                            let mut l: Vec<usize> = (0..m.triangles.len()).collect();
                            l.sort_by_key(|&t| m.tree.rank[t]);
                            ("mesh", m.vertices.clone(), m.triangles.clone(), l)
                        }
                    };
                eprintln!(
                    "PIECE h{handle} {k} {kind} pos {:?} rot {:?} verts {:?} tris {:?} leaves {leaves:?}",
                    p.position.to_array(),
                    p.rotation.to_array(),
                    v.iter().map(|x| x.to_array()).collect::<Vec<_>>(),
                    t
                );
            }
        }
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
        self.wake_pushed(handle);
        if let Some(b) = self.body_mut(handle) {
            let com = b.com;
            b.apply_impulse_at(impulse, com);
        }
    }

    fn apply_impulse_at(&mut self, handle: BodyHandle, impulse: Vec3, point: Vec3) {
        self.wake_pushed(handle);
        if let Some(b) = self.body_mut(handle) {
            b.apply_impulse_at(impulse, point);
        }
    }

    fn apply_angular_impulse(&mut self, handle: BodyHandle, torque_impulse: Vec3) {
        self.wake_pushed(handle);
        if let Some(b) = self.body_mut(handle) {
            b.apply_angular_impulse(torque_impulse);
        }
    }

    fn set_velocity(&mut self, handle: BodyHandle, linear: Vec3, angular: Vec3) {
        self.wake_pushed(handle);
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

    fn mass_properties(&self, handle: BodyHandle) -> Option<(f32, Vec3, Mat3)> {
        self.body(handle)
            .filter(|b| b.is_dynamic())
            .map(|b| (b.mass, b.com_local, b.inv_inertia_local))
    }

    fn center_of_mass(&self, handle: BodyHandle) -> Option<Vec3> {
        self.body(handle).map(|b| b.com)
    }

    fn add_force(&mut self, handle: BodyHandle, force: Vec3) {
        self.wake_pushed(handle);
        if let Some(b) = self.body_mut(handle) {
            b.force += force;
        }
    }

    fn add_torque(&mut self, handle: BodyHandle, torque: Vec3) {
        self.wake_pushed(handle);
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
        // The deactivators run once a frame of two substeps, after the
        // frame's last actions (ours: the game's pushes of this step are in).
        if self.deactivation && self.substeps % 2 == 1 {
            self.run_deactivators();
        }
        self.substeps += 1;
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
            if dumping() {
                self.dump_resolver_entry();
            }
            let contacts = std::mem::take(&mut self.contacts);
            resolver::resolve(&mut self.bodies, &contacts, TOLERANCE);
            self.contacts = contacts;
            if dumping() {
                self.dump_resolver_exit();
            }
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
                    if std::env::var_os("RA2_H17_BACKSTEPS").is_some() {
                        eprintln!("backstep h {h:.5} tol {current_tol:.4} bad {bad:?}");
                    }
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
            if dumping() {
                dump_line(format_args!("[\"A\",0,{h},{}]", self.backsteps));
            }
            for d in &self.dashpots {
                d.apply(&mut self.bodies, h);
            }
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
                    for d in &self.dashpots {
                        d.apply(&mut self.bodies, rest);
                    }
                    joints::solve(&mut self.bodies, &mut self.joints, dt);
                    self.drag();
                }
                break;
            }
        }
        self.detect_phantoms();
        self.clock.advance(dt);
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
            steer_rest: None,
        });
        Ok((self.joints.len() - 1) as JointHandle)
    }

    fn create_point_to_point(
        &mut self,
        a: BodyHandle,
        b: BodyHandle,
        anchor: Vec3,
    ) -> Result<JointHandle, BackendError> {
        let (Some(ba), Some(bb)) = (self.body(a), self.body(b)) else {
            return Err(BackendError::Body(
                "point-to-point on a missing body".into(),
            ));
        };
        let anchor_a = ba.rotation.inverse() * (anchor - ba.origin());
        let anchor_b = bb.rotation.inverse() * (anchor - bb.origin());
        self.joints.push(Joint {
            alive: true,
            a: a as usize,
            b: b as usize,
            anchor_a,
            anchor_b,
            kind: Kind::PointToPoint,
            limits: [0.0, 0.0],
            limited: false,
            motor: false,
            max_torque: 0.0,
            target_velocity: 0.0,
            target: 0.0,
            friction: 0.0,
            saved_limits: None,
            steer_rest: None,
        });
        Ok((self.joints.len() - 1) as JointHandle)
    }

    fn create_angular_dashpot(
        &mut self,
        a: BodyHandle,
        b: BodyHandle,
        rest: Quat,
        strength: f32,
        damping: f32,
    ) -> Result<(), BackendError> {
        if self.body(a).is_none() || self.body(b).is_none() {
            return Err(BackendError::Body("dashpot on a missing body".into()));
        }
        self.dashpots.push(dashpot::AngularDashpot {
            a: a as usize,
            b: b as usize,
            rest,
            strength,
            damping,
        });
        Ok(())
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
            steer_rest: None,
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

    fn steer_hinge(&mut self, handle: JointHandle, steer_axis: Vec3, angle: f32) {
        let Some(j) = self.joint_mut(handle) else {
            return;
        };
        let rest = j.steer_rest;
        let joints::Kind::Hinge { axis_a, ref_a, .. } = &mut j.kind else {
            return;
        };
        let (rest_axis, rest_ref) = rest.unwrap_or((*axis_a, *ref_a));
        // `cos θ a + sin θ (a × s)` is the turn by `-θ` about `s`.
        let turn = Quat::from_axis_angle(steer_axis.normalize_or_zero(), -angle);
        *axis_a = (turn * rest_axis).normalize_or_zero();
        *ref_a = (turn * rest_ref).normalize_or_zero();
        j.steer_rest = Some((rest_axis, rest_ref));
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
        b.active = true;
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

    fn deactivation_system(&mut self, bodies: &[BodyHandle], scale: f32) {
        let s = self.systems.len();
        let mut sys = deactivation::System::new(u32::MAX);
        sys.thresholds = deactivation::THRESHOLDS.map(|t| t * scale);
        for &h in bodies {
            let i = h as usize;
            let Some(b) = self.bodies.get_mut(i).filter(|b| b.alive) else {
                continue;
            };
            if let Some(old) = b.system {
                self.systems[old].bodies.retain(|&k| k != i);
            }
            b.system = Some(s);
            sys.bodies.push(i);
        }
        self.systems.push(sys);
    }

    fn spawn_touch(&mut self, bodies: &[BodyHandle]) -> Option<bool> {
        // `FUN_00542a70` for each body: the broadphase, then the agents of
        // every pair with the body, and any contact record counts. Live
        // (`spawnq-...__live__000`) the record that stops the lowering is a
        // wheel's within the tolerance of the floor at the distance its
        // traction phantom is (M.A.D. 0.0859 m live, 0.0847 ours; the Scout
        // 0.0962, 0.0955): the phantoms count here although they hold
        // nothing up in a step.
        let set: Vec<usize> = bodies
            .iter()
            .filter(|&&h| self.body(h).is_some())
            .map(|&h| h as usize)
            .collect();
        // The test leaves the world as it found it (ours: the agents' kept
        // points, the excused pairs and the bodies' activity are put back).
        let kept = (
            self.convex_manifolds.clone(),
            self.mesh_manifolds.clone(),
            self.excused.clone(),
            self.detections,
            self.bodies.iter().map(|b| b.active).collect::<Vec<_>>(),
        );
        let (contacts, _) = self.detect(TOLERANCE * 0.5);
        let (convex, mesh, excused, detections, active) = kept;
        self.convex_manifolds = convex;
        self.mesh_manifolds = mesh;
        self.excused = excused;
        self.detections = detections;
        for (b, a) in self.bodies.iter_mut().zip(active) {
            b.active = a;
        }
        if contacts
            .iter()
            .any(|c| set.contains(&c.a) || set.contains(&c.b))
        {
            return Some(true);
        }
        let pieces = self.world_pieces();
        for &i in &set {
            let Some(g) = self.geometry[i].as_ref() else {
                continue;
            };
            let pose = Pose {
                position: self.bodies[i].origin(),
                rotation: self.bodies[i].rotation,
            };
            for ph in &g.phantom_pieces {
                let PieceShape::Convex { vertices, radius } = &ph.shape else {
                    continue;
                };
                let at = Pose {
                    position: pose.apply(ph.position),
                    rotation: pose.rotation * ph.rotation,
                };
                let v: Vec<Vec3> = vertices.iter().map(|p| at.apply(*p)).collect();
                let reach = Aabb::of(v.iter().copied()).grown(radius + TOLERANCE);
                for w in &pieces {
                    if w.body == i || self.same_group(i, w.body) || !reach.overlaps(&w.bounds) {
                        continue;
                    }
                    let near = match self.geometry[w.body]
                        .as_ref()
                        .and_then(|g| g.pieces.get(w.piece as usize))
                        .map(|p| &p.shape)
                    {
                        Some(PieceShape::Mesh(mesh)) => {
                            let local = Aabb::of(v.iter().map(|p| w.pose.inverse_apply(*p)))
                                .grown(radius + TOLERANCE);
                            let mut tris = Vec::new();
                            mesh.query(&local, &mut tris);
                            tris.iter().any(|&t| {
                                let tri = mesh.triangles[t as usize]
                                    .map(|k| w.pose.apply(mesh.vertices[k as usize]));
                                narrow::closest(&v, &tri).distance - radius < TOLERANCE
                            })
                        }
                        Some(PieceShape::Convex { .. }) => {
                            narrow::closest(&v, &w.vertices).distance - radius - w.radius
                                < TOLERANCE
                        }
                        None => false,
                    };
                    if near {
                        return Some(true);
                    }
                }
            }
        }
        Some(false)
    }

    fn detect_now(&mut self) {
        let (contacts, _) = self.detect(TOLERANCE * 0.5);
        self.contacts = contacts;
        if dumping() {
            self.dump_resolver_entry();
        }
    }

    fn is_active(&self, handle: BodyHandle) -> bool {
        self.body(handle)
            .is_none_or(|b| b.motion != MotionType::Dynamic || b.active)
    }

    fn set_active(&mut self, handle: BodyHandle, active: bool) {
        if let Some(b) = self.body_mut(handle) {
            if active && !b.active {
                b.freeze.forget();
            }
            b.active = active;
        }
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
                // And the driven body stops dead, in the world, not relative
                // to its parent: both locks set its velocity and angular
                // velocity (`+0x54`, `+0x64`) to the zero vector `0x71abcc`
                // (`0x480e66..0x480e89`, `0x4b5110..0x4b513d`). Live
                // (docs/29 section 12r): ALARM's rod, locked home while the
                // robot falls at 3.3 m/s, leaves its stop at +3.30 m/s.
                let b = j.b;
                if let Some(body) = self.bodies.get_mut(b) {
                    if body.is_dynamic() {
                        body.velocity = Vec3::ZERO;
                        body.angular_velocity = Vec3::ZERO;
                    }
                }
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

    fn set_body_inertia(&mut self, handle: BodyHandle, inertia: Mat3) {
        if inertia.determinant().abs() <= 1e-12 {
            return;
        }
        if let Some(b) = self.bodies.get_mut(handle as usize) {
            if b.motion == MotionType::Dynamic {
                b.inv_inertia_local = inertia.inverse();
                b.refresh_inertia();
            }
        }
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
