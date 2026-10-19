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
    /// The corners of the body's convex pieces, body-local: where a resting
    /// face meets a surface (see [`Havok17Backend::detect`]).
    vertices: Vec<Vec3>,
    /// How far the shape reaches from the body's origin.
    reach: f32,
}

/// The corners of `shape`'s convex pieces in shape space (none for round
/// primitives and meshes, whose contacts are Jolt's closest points).
fn shape_vertices(shape: &ShapeDesc) -> Vec<Vec3> {
    match shape {
        ShapeDesc::Box { half_extents: h } => {
            let mut out = Vec::with_capacity(8);
            for x in [-h.x, h.x] {
                for y in [-h.y, h.y] {
                    for z in [-h.z, h.z] {
                        out.push(Vec3::new(x, y, z));
                    }
                }
            }
            out
        }
        ShapeDesc::ConvexHull { points } => crate::massprops::hull_vertices(points),
        ShapeDesc::Compound(children) => children
            .iter()
            .flat_map(|c| {
                shape_vertices(&c.shape)
                    .into_iter()
                    .map(move |v| c.local_position + c.local_rotation * v)
            })
            .collect(),
        ShapeDesc::OffsetCenterOfMass { shape, .. } => shape_vertices(shape),
        _ => Vec::new(),
    }
}

/// How far any point of `shape` is from its origin (a bound).
fn shape_reach(shape: &ShapeDesc) -> f32 {
    match shape {
        ShapeDesc::Box { half_extents } => half_extents.length(),
        ShapeDesc::Sphere { radius } => *radius,
        ShapeDesc::Cylinder {
            half_height,
            radius,
        }
        | ShapeDesc::Capsule {
            half_height,
            radius,
        } => half_height + radius,
        ShapeDesc::ConvexHull { points }
        | ShapeDesc::TriMesh {
            vertices: points, ..
        } => points.iter().map(|p| p.length()).fold(0.0, f32::max),
        ShapeDesc::Compound(children) => children
            .iter()
            .map(|c| c.local_position.length() + shape_reach(&c.shape))
            .fold(0.0, f32::max),
        ShapeDesc::OffsetCenterOfMass { shape, .. } => shape_reach(shape),
    }
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
    /// The tolerance that detection used.
    contact_tolerance: f32,
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
            contact_tolerance: TOLERANCE,
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

    /// Closest-point collision detection within `tol`. Returns the contacts and
    /// whether a pair that was apart before now overlaps (the backstep test).
    fn detect(&mut self, tol: f32) -> (Vec<Contact>, bool) {
        self.sync();
        let was_apart: HashMap<(usize, usize), f32> = {
            let mut m: HashMap<(usize, usize), f32> = HashMap::new();
            for c in &self.contacts {
                let e = m.entry((c.a, c.b)).or_insert(f32::MAX);
                *e = e.min(c.distance);
            }
            m
        };
        let mut out = Vec::new();
        let mut penetrated = false;
        // Corners already turned into contacts, per pair: (pair, side, corner).
        let mut used: std::collections::HashSet<(usize, usize, bool, usize)> = Default::default();
        for i in 0..self.bodies.len() {
            let bi = &self.bodies[i];
            if !bi.moves() || bi.sensor {
                continue;
            }
            let Some(g) = &self.geometry[i] else { continue };
            let query = CollideShape::new(&g.shape, rv(bi.origin()), jq(bi.rotation))
                .max_separation_distance(tol);
            let Ok(hits) = self
                .world
                .collide_shape(&query, &QueryFilter::new().exclude_body(g.id))
            else {
                continue;
            };
            let mut hits = hits;
            hits.sort_by(|x, y| {
                x.body
                    .to_raw()
                    .cmp(&y.body.to_raw())
                    .then(x.sub_shape_id.to_raw().cmp(&y.sub_shape_id.to_raw()))
            });
            for hit in hits {
                let Some(&j) = self.lookup.get(&hit.body.to_raw()) else {
                    continue;
                };
                if j == i {
                    continue;
                }
                let bj = &self.bodies[j];
                if !bj.alive || bj.sensor || self.same_group(i, j) {
                    continue;
                }
                // A pair of moving bodies is found from both sides; keep one.
                if bj.moves() && j < i {
                    continue;
                }
                if !bi.is_dynamic() && !bj.is_dynamic() {
                    continue;
                }
                let distance = -hit.penetration_depth;
                if distance >= tol {
                    continue;
                }
                let normal = un_v(hit.normal);
                if normal.length_squared() < 0.5 {
                    continue;
                }
                let normal = normal.normalize();
                let on_a = un_rv(hit.point_on_shape);
                let on_b = un_rv(hit.point_on_body);
                let points = self.manifold(i, j, normal, on_a, on_b, tol, &mut used);
                let friction = (bi.friction * bj.friction).max(0.0).sqrt();
                let restitution = bi.restitution * bj.restitution;
                let mut push = |point: Vec3, distance: f32| {
                    if distance < 0.0 {
                        let before = was_apart.get(&(i, j)).copied();
                        if before.is_none_or(|d| d >= 0.0) {
                            penetrated = true;
                        }
                    }
                    out.push(Contact {
                        a: i,
                        b: j,
                        normal,
                        point,
                        distance,
                        friction,
                        restitution,
                    });
                };
                if points.is_empty() {
                    push((on_a + on_b) * 0.5, distance);
                } else {
                    for (point, d) in points {
                        push(point, d);
                    }
                }
            }
        }
        (out, penetrated)
    }

    /// The contact points of one closest-point hit between `i` (the query
    /// body) and `j`.
    ///
    /// Havok's agents report a point per pair of features within the
    /// tolerance — two for a mini wheel's flat on the floor, one per hub
    /// triangle (`clist_rest`, live) — where Jolt reports one deepest point per
    /// pair of shapes. A face resting on a surface then rocks on one point. So
    /// every corner of either body that lies within the tolerance of the other's
    /// surface plane (`normal` through the hit) becomes a point, if a short ray
    /// along the normal confirms the surface is really under it.
    #[allow(clippy::too_many_arguments)]
    fn manifold(
        &self,
        i: usize,
        j: usize,
        normal: Vec3,
        on_a: Vec3,
        on_b: Vec3,
        tol: f32,
        used: &mut std::collections::HashSet<(usize, usize, bool, usize)>,
    ) -> Vec<(Vec3, f32)> {
        let mut out = Vec::new();
        for (side, (body, other, plane, n)) in
            [(false, (i, j, on_b, normal)), (true, (j, i, on_a, -normal))]
        {
            let Some(g) = &self.geometry[body] else {
                continue;
            };
            let Some(target) = self.geometry[other].as_ref().map(|g| g.id) else {
                continue;
            };
            let b = &self.bodies[body];
            let (origin, rotation) = (b.origin(), b.rotation);
            let reach = self.geometry[other]
                .as_ref()
                .map(|g| g.reach)
                .unwrap_or(0.0)
                .min(g.reach)
                .max(0.05)
                * 2.0;
            for (k, v) in g.vertices.iter().enumerate() {
                let w = origin + rotation * *v;
                let d = n.dot(w - plane);
                if d >= tol || d < -tol {
                    continue;
                }
                if w.distance(plane) > reach {
                    continue;
                }
                if used.contains(&(i.min(j), i.max(j), side, k)) {
                    continue;
                }
                // Confirm the surface under the corner.
                let lift = 0.02 - d.min(0.0);
                let ray = RayCast::new(rv(w + n * lift), jv(-n * (lift + tol)));
                let hit = self
                    .world
                    .cast_ray(&ray, &QueryFilter::new().exclude_body(g.id))
                    .ok()
                    .flatten();
                if hit.is_none_or(|h| h.body != target) {
                    continue;
                }
                used.insert((i.min(j), i.max(j), side, k));
                // The contact normal always points from j to i.
                let point = w - n * (d * 0.5);
                out.push((point, d));
            }
        }
        out
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
            vertices: shape_vertices(&desc.shape),
            reach: shape_reach(&desc.shape),
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
        while done < dt * (1.0 - 1e-4) {
            pieces += 1;
            self.pieces += 1;
            let mut h = dt - done;
            // 1-2: the events of the last detection, then their impulses.
            self.fire_events();
            let contacts = std::mem::take(&mut self.contacts);
            resolver::resolve(&mut self.bodies, &contacts, self.contact_tolerance);
            self.contacts = contacts;
            // 3: integrate and detect, halving on a new overlap.
            let saved = self.save();
            let mut tol = TOLERANCE;
            let mut backsteps = 0;
            loop {
                self.integrate(h);
                let (contacts, penetrated) = self.detect(tol);
                if penetrated && h > MIN_STEP && backsteps < MAX_BACKSTEPS && pieces < MAX_PIECES {
                    self.restore(&saved);
                    h = (h * 0.5).max(MIN_STEP);
                    tol *= 0.5;
                    backsteps += 1;
                    self.backsteps += 1;
                    continue;
                }
                if penetrated {
                    complete = false;
                }
                self.contacts = contacts;
                self.contact_tolerance = tol;
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
                    resolver::resolve(&mut self.bodies, &contacts, self.contact_tolerance);
                    self.integrate(rest);
                    let (contacts, _) = self.detect(TOLERANCE);
                    self.contacts = contacts;
                    self.contact_tolerance = TOLERANCE;
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
                    "h17 body {watch}: v ({:.2},{:.2},{:.2}) w ({:.2},{:.2},{:.2}) {}",
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
        let dir = direction.normalize_or_zero() * max_distance;
        if dir == Vec3::ZERO {
            return None;
        }
        let ray = RayCast::new(rv(origin), jv(dir));
        let hit = self.world.cast_ray(&ray, &QueryFilter::default()).ok()??;
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
            g.vertices = shape_vertices(shape);
            g.reach = shape_reach(shape);
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
