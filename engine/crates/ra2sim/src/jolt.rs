//! Jolt Physics backend (via the safe `oxijolt` bindings).
//!
//! This is the only module that knows about Jolt; the rest of the crate talks
//! to [`PhysicsBackend`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use glam::{Quat, Vec3};
use oxijolt::prelude::math as m;
use oxijolt::prelude::{
    Activation, BodyId, BodySettings, CompoundChild as JoltCompoundChild, PhysicsWorld,
    QueryFilter, RayCast, Shape, WorldSettings,
};
use oxijolt::{
    AnyConstraintId, CollisionGroup, ContactListener, ContactManifold, ContactSettings,
    GroupFilterTable, GroupFilterTableBuilder, HingeConstraint, HingeConstraintSettings,
    MotorSettings, MotorState, SliderConstraint, SliderConstraintSettings,
};

/// Havok 1.x's friction-combination law, in place of Jolt's `sqrt(µ₁·µ₂)`.
///
/// Havok 1.7.1 (the engine the game ships, `docs/06` F6) combines the two
/// bodies' coefficients as `sqrt(µ₁² + µ₂²)`. `oxijolt` 1.1.0 has no
/// combine-mode field, but it does expose Jolt's contact listener, whose
/// `ContactSettings` carries the resolved `combined_friction`; the listener
/// below replaces Jolt's value with Havok's on every contact.
pub fn havok_combined_friction(a: f32, b: f32) -> f32 {
    (a * a + b * b)
        .sqrt()
        .clamp(0.0, oxijolt::limits::MAX_FRICTION)
}

/// The listener that implements [`havok_combined_friction`].
///
/// It needs each body's own coefficient, which Jolt does not expose as a
/// getter, so the backend records every body's `BodyDesc::friction` by raw body
/// id when it creates the body and removes it when the body goes. The map is
/// fixed for the duration of a step; the callbacks only read it.
struct HavokFriction {
    friction: Arc<RwLock<HashMap<u32, f32>>>,
    /// Whether to replace Jolt's friction with Havok's law.
    havok: bool,
    /// Every contact point of the step, for the damage model.
    log: Arc<Mutex<Vec<RawContact>>>,
}

/// One contact manifold as Jolt reported it, before it is matched to bodies'
/// pre-step states. Havok's event manager fires once per contact point per
/// step (`FUN_00549c30`); the damage pipeline wants the same stream.
struct RawContact {
    body1: u32,
    body2: u32,
    normal: Vec3,
    penetration: f32,
    points: Vec<Vec3>,
}

impl HavokFriction {
    fn record(&self, manifold: &ContactManifold) {
        let points = manifold
            .points
            .iter()
            .map(|p| {
                let a = JoltBackend::un_rv(p.point_on1);
                let b = JoltBackend::un_rv(p.point_on2);
                (a + b) * 0.5
            })
            .collect();
        if let Ok(mut log) = self.log.lock() {
            if log.len() < 65_536 {
                log.push(RawContact {
                    body1: manifold.pair.body1.to_raw(),
                    body2: manifold.pair.body2.to_raw(),
                    normal: JoltBackend::un_v(manifold.normal),
                    penetration: manifold.penetration_depth,
                    points,
                });
            }
        }
    }

    fn combine(&self, manifold: &ContactManifold, settings: &mut ContactSettings) {
        self.record(manifold);
        if !self.havok {
            return;
        }
        let Ok(table) = self.friction.read() else {
            return;
        };
        let (Some(&a), Some(&b)) = (
            table.get(&manifold.pair.body1.to_raw()),
            table.get(&manifold.pair.body2.to_raw()),
        ) else {
            return;
        };
        // The setter refuses values outside the body-friction range; the
        // combine of two legal coefficients can only exceed it for absurd
        // inputs, and `havok_combined_friction` clamps.
        let _ = settings.set_combined_friction(havok_combined_friction(a, b));
    }
}

impl ContactListener for HavokFriction {
    fn contact_added(&self, manifold: &ContactManifold, settings: &mut ContactSettings) {
        self.combine(manifold, settings);
    }

    fn contact_persisted(&self, manifold: &ContactManifold, settings: &mut ContactSettings) {
        self.combine(manifold, settings);
    }
}

use crate::backend::{
    BackendError, BodyDesc, BodyHandle, BodyState, ContactEvent, HingeDesc, JointHandle,
    MotionType, PhysicsBackend, RayHit, ShapeDesc, SliderDesc, StepInfo,
};

/// How far apart (m) a contact may be and still count as touching.
const CONTACT_TOLERANCE: f32 = 0.01;

/// The description of one joint, kept so a body can be re-shaped without
/// losing its constraints (F9). The motor's *state* is here too: recreating a
/// hinge resets Jolt's angle reference to zero, so the recreation restores the
/// current angle with an offset frame and replays the drive.
enum Joint {
    Hinge {
        id: AnyConstraintId,
        desc: HingeDesc,
        target: f32,
        driven: bool,
    },
    Slider {
        id: AnyConstraintId,
        desc: SliderDesc,
        target: f32,
        driven: bool,
    },
}

impl Joint {
    fn id(&self) -> AnyConstraintId {
        match self {
            Joint::Hinge { id, .. } | Joint::Slider { id, .. } => *id,
        }
    }
}

/// A physics world backed by Jolt 5.
pub struct JoltBackend {
    world: PhysicsWorld,
    /// handle → Jolt body id (indexed by `BodyHandle`).
    ids: Vec<Option<BodyId>>,
    /// Shapes must outlive the bodies that reference them.
    shapes: Vec<Shape>,
    /// Jolt body id (raw) → our handle, for ray-hit attribution.
    lookup: HashMap<u32, BodyHandle>,
    /// handle → description + Jolt constraint id (indexed by `JointHandle`).
    joints: Vec<Option<Joint>>,
    /// A robot's self-collision table, by group id. Built once, when its first
    /// body is created, and never changes.
    collision_tables: HashMap<u32, GroupFilterTable>,
    /// Each body's own friction coefficient, by raw Jolt body id, for the
    /// Havok friction-combination listener (F6). Jolt has no getter for it.
    friction: Arc<RwLock<HashMap<u32, f32>>>,
    /// The contact points the listener logged during the last step.
    contact_log: Arc<Mutex<Vec<RawContact>>>,
    /// Every body's state at the start of the last step (raw id), which is what
    /// Havok's collision events see: they fire before the step resolves.
    pre_step: HashMap<u32, BodyState>,
}

impl JoltBackend {
    /// Create a world with the given gravity vector (m/s²).
    pub fn new(gravity: Vec3) -> Result<Self, BackendError> {
        let settings = WorldSettings::default().gravity(Self::v(gravity));
        let mut world =
            PhysicsWorld::new(settings).map_err(|e| BackendError::World(format!("{e:?}")))?;
        // F6: install Havok's friction law. The listener does not need contact
        // events recorded, only the settings callback; oxijolt installs the
        // native contact listener whenever one is set.
        let friction = Arc::new(RwLock::new(HashMap::new()));
        // The game's own combine law is the default; `RA2_FRICTION_LAW=jolt`
        // opts into Jolt's `sqrt(µ₁·µ₂)` for comparison. With Havok's law the
        // 45-bot survey measures 38/45 against 39/45 under Jolt's: the two
        // losses are a ~2 cm-low wheel on two bots (F1 mount residual), not
        // the law. See F6/F1 in `docs/06-deferred-fidelity.md`.
        let havok = std::env::var("RA2_FRICTION_LAW")
            .map(|v| !v.eq_ignore_ascii_case("jolt"))
            .unwrap_or(true);
        // The listener is always installed: it also logs every contact point for
        // the damage model, and only applies the friction law when asked to.
        let contact_log = Arc::new(Mutex::new(Vec::new()));
        world.set_contact_listener(Some(Arc::new(HavokFriction {
            friction: Arc::clone(&friction),
            havok,
            log: Arc::clone(&contact_log),
        })));
        Ok(Self {
            world,
            ids: Vec::new(),
            shapes: Vec::new(),
            lookup: HashMap::new(),
            joints: Vec::new(),
            collision_tables: HashMap::new(),
            friction,
            contact_log,
            pre_step: HashMap::new(),
        })
    }

    fn v(v: Vec3) -> m::Vec3 {
        m::Vec3::new(v.x, v.y, v.z)
    }

    fn rv(v: Vec3) -> m::RVec3 {
        m::RVec3::new(v.x as m::Real, v.y as m::Real, v.z as m::Real)
    }

    fn un_v(v: m::Vec3) -> Vec3 {
        Vec3::new(v.x, v.y, v.z)
    }

    // The casts are no-ops in the default `f32` build and needed if the
    // `double-precision` feature is ever enabled, so keep them.
    #[allow(clippy::unnecessary_cast)]
    fn un_rv(v: m::RVec3) -> Vec3 {
        Vec3::new(v.x as f32, v.y as f32, v.z as f32)
    }

    fn q(q: Quat) -> m::Quat {
        m::Quat::from_xyzw(q.x, q.y, q.z, q.w)
    }

    fn un_q(q: m::Quat) -> Quat {
        Quat::from_xyzw(q.x, q.y, q.z, q.w)
    }

    fn build_shape(shape: &ShapeDesc) -> Result<Shape, BackendError> {
        // Compounds recurse, so they are handled before the leaf closure.
        if let ShapeDesc::Compound(children) = shape {
            let built: Vec<Shape> = children
                .iter()
                .map(|c| Self::build_shape(&c.shape))
                .collect::<Result<_, BackendError>>()?;
            let refs: Vec<JoltCompoundChild> = children
                .iter()
                .zip(&built)
                .map(|(c, s)| JoltCompoundChild {
                    shape: s,
                    position: Self::v(c.local_position),
                    rotation: Self::q(c.local_rotation),
                    user_data: c.tag,
                })
                .collect();
            return Shape::new_compound(&refs).map_err(|e| BackendError::Shape(format!("{e:?}")));
        }
        let build = || -> Result<Shape, oxijolt::prelude::ShapeError> {
            Ok(match shape {
                ShapeDesc::Box { half_extents } => Shape::new_box(Self::v(*half_extents))?,
                ShapeDesc::Sphere { radius } => Shape::new_sphere(*radius)?,
                ShapeDesc::Cylinder {
                    half_height,
                    radius,
                } => Shape::new_cylinder(*half_height, *radius)?,
                ShapeDesc::Capsule {
                    half_height,
                    radius,
                } => Shape::new_capsule(*half_height, *radius)?,
                ShapeDesc::ConvexHull { points } => {
                    let pts: Vec<m::Vec3> = points.iter().map(|p| Self::v(*p)).collect();
                    Shape::new_convex_hull(&pts)?
                }
                ShapeDesc::TriMesh { vertices, indices } => {
                    let verts: Vec<m::Vec3> = vertices.iter().map(|p| Self::v(*p)).collect();
                    let (mesh, _dropped) = Shape::new_mesh(&verts, indices)?;
                    mesh
                }
                // Handled above; a compound never reaches this closure.
                ShapeDesc::Compound(_) => unreachable!("compound handled before this closure"),
            })
        };
        build().map_err(|e| BackendError::Shape(format!("{e:?}")))
    }

    fn id(&self, handle: BodyHandle) -> Option<BodyId> {
        self.ids.get(handle as usize).copied().flatten()
    }
}

impl PhysicsBackend for JoltBackend {
    fn name(&self) -> &'static str {
        "jolt"
    }

    fn create_body(&mut self, desc: &BodyDesc) -> Result<BodyHandle, BackendError> {
        let shape = Self::build_shape(&desc.shape)?;
        let mut settings = match desc.motion {
            MotionType::Static => BodySettings::new_static(),
            MotionType::Dynamic => BodySettings::new_dynamic(),
            MotionType::Kinematic => BodySettings::new_kinematic(),
        }
        .position(Self::rv(desc.position))
        .rotation(Self::q(desc.rotation))
        .friction(desc.friction)
        .restitution(desc.restitution)
        .linear_damping(desc.linear_damping)
        .angular_damping(desc.angular_damping)
        .allow_sleeping(desc.allow_sleeping)
        .sensor(desc.is_sensor)
        .user_data(desc.user_data);
        if let Some(mass) = desc.mass {
            settings = settings.mass(mass);
        }
        // A robot's bodies share one filter table in which no two sub groups
        // collide, so its wheel does not jam inside its own chassis and its
        // rotor does not fight its hull. Different robots have different group
        // ids, so they still collide with each other.
        if let Some(g) = desc.collision_group {
            let table = match self.collision_tables.get(&g.group) {
                Some(t) => t.clone(),
                None => {
                    let mut b = GroupFilterTableBuilder::new(g.sub_groups)
                        .map_err(|e| BackendError::Body(format!("collision group: {e:?}")))?;
                    for a in 0..g.sub_groups {
                        for c in (a + 1)..g.sub_groups {
                            b.disable_collision(a, c).map_err(|e| {
                                BackendError::Body(format!("collision group: {e:?}"))
                            })?;
                        }
                    }
                    let t = b.build();
                    self.collision_tables.insert(g.group, t.clone());
                    t
                }
            };
            let group = CollisionGroup::new(&table, g.group, g.sub_group)
                .map_err(|e| BackendError::Body(format!("collision group: {e:?}")))?;
            settings = settings.collision_group(group);
        }

        let id = self
            .world
            .create_body(&shape, &settings)
            .map_err(|e| BackendError::Body(format!("{e:?}")))?;

        self.shapes.push(shape);
        let handle = self.ids.len() as BodyHandle;
        self.ids.push(Some(id));
        self.lookup.insert(id.to_raw(), handle);
        if let Ok(mut table) = self.friction.write() {
            table.insert(id.to_raw(), desc.friction);
        }
        Ok(handle)
    }

    fn remove_body(&mut self, handle: BodyHandle) {
        if let Some(id) = self.id(handle) {
            let _ = self.world.remove_body(id);
            self.lookup.remove(&id.to_raw());
            if let Ok(mut table) = self.friction.write() {
                table.remove(&id.to_raw());
            }
        }
        if let Some(slot) = self.ids.get_mut(handle as usize) {
            *slot = None;
        }
    }

    fn state(&self, handle: BodyHandle) -> Option<BodyState> {
        let body = self.world.body(self.id(handle)?).ok()?;
        Some(BodyState {
            position: Self::un_rv(body.position()),
            rotation: Self::un_q(body.rotation()),
            linear_velocity: Self::un_v(body.linear_velocity()),
            angular_velocity: Self::un_v(body.angular_velocity()),
        })
    }

    fn set_transform(&mut self, handle: BodyHandle, position: Vec3, rotation: Quat) {
        if let Some(id) = self.id(handle) {
            if let Ok(mut body) = self.world.body_mut(id) {
                let _ = body.set_position_and_rotation(
                    Self::rv(position),
                    Self::q(rotation),
                    Activation::Activate,
                );
            }
        }
    }

    fn apply_impulse(&mut self, handle: BodyHandle, impulse: Vec3) {
        if let Some(id) = self.id(handle) {
            if let Ok(mut body) = self.world.body_mut(id) {
                // A sleeping body ignores impulses too, so wake it first — the
                // same gotcha `set_velocity` has. A hazard pushes a part that
                // has been sitting still since the match began.
                body.activate();
                let _ = body.add_impulse(Self::v(impulse));
            }
        }
    }

    fn apply_impulse_at(&mut self, handle: BodyHandle, impulse: Vec3, point: Vec3) {
        if let Some(id) = self.id(handle) {
            if let Ok(mut body) = self.world.body_mut(id) {
                let _ = body.add_impulse_at_point(Self::v(impulse), Self::rv(point));
            }
        }
    }

    fn apply_angular_impulse(&mut self, handle: BodyHandle, torque_impulse: Vec3) {
        if let Some(id) = self.id(handle) {
            if let Ok(mut body) = self.world.body_mut(id) {
                body.activate();
                let _ = body.add_angular_impulse(Self::v(torque_impulse));
            }
        }
    }

    fn set_velocity(&mut self, handle: BodyHandle, linear: Vec3, angular: Vec3) {
        if let Some(id) = self.id(handle) {
            if let Ok(mut body) = self.world.body_mut(id) {
                // A sleeping body ignores velocity changes, so wake it first.
                body.activate();
                let _ = body.set_linear_velocity(Self::v(linear));
                let _ = body.set_angular_velocity(Self::v(angular));
            }
        }
    }

    fn gravity(&self) -> Vec3 {
        Self::un_v(self.world.gravity())
    }

    fn step(&mut self, dt: f32) -> StepInfo {
        // The collision events of this step see the bodies as they are now.
        self.pre_step.clear();
        if let Ok(mut log) = self.contact_log.lock() {
            log.clear();
        }
        for handle in 0..self.ids.len() {
            let handle = handle as BodyHandle;
            if let (Some(id), Some(state)) = (self.id(handle), self.state(handle)) {
                self.pre_step.insert(id.to_raw(), state);
            }
        }
        match self.world.step(dt) {
            Ok(report) => StepInfo {
                complete: report.is_complete(),
            },
            Err(e) => {
                eprintln!("ra2sim: step failed: {e:?}");
                StepInfo { complete: false }
            }
        }
    }

    fn take_contacts(&mut self) -> Vec<ContactEvent> {
        let raw = match self.contact_log.lock() {
            Ok(mut log) => std::mem::take(&mut *log),
            Err(_) => return Vec::new(),
        };
        let mut out = Vec::new();
        for c in raw {
            // Havok keeps only points closer than its tolerance; Jolt also
            // reports speculative contacts before the bodies touch.
            if c.penetration < -CONTACT_TOLERANCE {
                continue;
            }
            let (Some(&a), Some(&b)) = (self.lookup.get(&c.body1), self.lookup.get(&c.body2))
            else {
                continue;
            };
            let (Some(&state_a), Some(&state_b)) =
                (self.pre_step.get(&c.body1), self.pre_step.get(&c.body2))
            else {
                continue;
            };
            for point in c.points {
                out.push(ContactEvent {
                    a,
                    b,
                    normal: c.normal,
                    point,
                    state_a,
                    state_b,
                });
            }
        }
        // Jolt's job threads report contacts in no fixed order.
        out.sort_by(|x, y| {
            (x.a, x.b)
                .cmp(&(y.a, y.b))
                .then(x.point.x.total_cmp(&y.point.x))
                .then(x.point.y.total_cmp(&y.point.y))
                .then(x.point.z.total_cmp(&y.point.z))
        });
        out
    }

    fn in_contact(&self, a: BodyHandle, b: BodyHandle) -> bool {
        match (self.id(a), self.id(b)) {
            (Some(ia), Some(ib)) => self.world.were_bodies_in_contact(ia, ib).unwrap_or(false),
            _ => false,
        }
    }

    fn ray_cast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<RayHit> {
        let dir = direction.normalize_or_zero() * max_distance;
        if dir == Vec3::ZERO {
            return None;
        }
        let ray = RayCast::new(Self::rv(origin), Self::v(dir));
        let hit = self.world.cast_ray(&ray, &QueryFilter::default()).ok()??;
        Some(RayHit {
            body: self
                .lookup
                .get(&hit.body.to_raw())
                .copied()
                .unwrap_or(u32::MAX),
            distance: hit.distance,
            point: origin + direction.normalize_or_zero() * hit.distance,
            normal: Self::un_v(hit.normal),
        })
    }

    fn create_hinge(&mut self, desc: &HingeDesc) -> Result<JointHandle, BackendError> {
        let id = self.build_hinge(desc, 0.0)?;
        let handle = self.joints.len() as JointHandle;
        self.joints.push(Some(Joint::Hinge {
            id,
            desc: *desc,
            target: 0.0,
            driven: false,
        }));
        Ok(handle)
    }

    fn create_slider(&mut self, desc: &SliderDesc) -> Result<JointHandle, BackendError> {
        let id = self.build_slider(desc, 0.0)?;
        let handle = self.joints.len() as JointHandle;
        self.joints.push(Some(Joint::Slider {
            id,
            desc: *desc,
            target: 0.0,
            driven: false,
        }));
        Ok(handle)
    }

    fn drive_slider(&mut self, handle: JointHandle, target_velocity: f32) {
        let id = match self.joints.get_mut(handle as usize) {
            Some(Some(Joint::Slider {
                id, target, driven, ..
            })) => {
                *target = target_velocity;
                *driven = target_velocity.abs() >= 1e-4;
                *id
            }
            _ => return,
        };
        let Some(cid) = id.downcast::<SliderConstraint>() else {
            return;
        };
        let Ok(mut c) = self.world.constraint_mut(cid) else {
            return;
        };
        if target_velocity.abs() < 1e-4 {
            // Parked: the joint's friction holds the rod where it is.
            c.set_motor_state(MotorState::Off);
        } else {
            c.set_motor_state(MotorState::Velocity);
            // Clamped to what a solver can express; see `drive_hinge`.
            let _ = c.set_target_velocity(clamp_speed(target_velocity, MAX_LINEAR_SPEED));
        }
    }

    fn slider_offset(&self, handle: JointHandle) -> f32 {
        let Some(Some(Joint::Slider { id, .. })) = self.joints.get(handle as usize) else {
            return 0.0;
        };
        let Some(cid) = id.downcast::<SliderConstraint>() else {
            return 0.0;
        };
        self.world
            .constraint(cid)
            .map(|c| c.current_position())
            .unwrap_or(0.0)
    }

    fn remove_joint(&mut self, handle: JointHandle) {
        let Some(Some(joint)) = self.joints.get_mut(handle as usize).map(Option::take) else {
            return;
        };
        let _ = self.world.remove_constraint(joint.id());
    }

    fn drive_hinge(&mut self, handle: JointHandle, target_velocity: f32) {
        let id = match self.joints.get_mut(handle as usize) {
            Some(Some(Joint::Hinge {
                id, target, driven, ..
            })) => {
                *target = target_velocity;
                *driven = target_velocity.abs() >= 1e-4;
                *id
            }
            _ => return,
        };
        let Some(cid) = id.downcast::<HingeConstraint>() else {
            return;
        };
        let Ok(mut c) = self.world.constraint_mut(cid) else {
            return;
        };
        if target_velocity.abs() < 1e-4 {
            // Free: let the joint swing (the original's motors idle here too).
            c.set_motor_state(MotorState::Off);
        } else {
            c.set_motor_state(MotorState::Velocity);
            let _ = c.set_target_angular_velocity(clamp_speed(target_velocity, MAX_ANGULAR_SPEED));
        }
    }

    fn hinge_angle(&self, handle: JointHandle) -> f32 {
        let Some(Some(Joint::Hinge { id, .. })) = self.joints.get(handle as usize) else {
            return 0.0;
        };
        let Some(cid) = id.downcast::<HingeConstraint>() else {
            return 0.0;
        };
        self.world
            .constraint(cid)
            .map(|c| c.current_angle())
            .unwrap_or(0.0)
    }

    fn set_joint_strength(&mut self, handle: JointHandle, strength: f32, friction: f32) {
        let strength = strength.max(0.0);
        let friction = friction.max(0.0);
        enum Kind {
            Hinge(AnyConstraintId),
            Slider(AnyConstraintId),
        }
        let kind = match self.joints.get_mut(handle as usize) {
            Some(Some(Joint::Slider { id, desc, .. })) => {
                desc.max_force = strength;
                desc.friction_force = friction;
                Kind::Slider(*id)
            }
            Some(Some(Joint::Hinge { id, desc, .. })) => {
                desc.max_torque = strength;
                desc.friction_torque = friction;
                Kind::Hinge(*id)
            }
            _ => return,
        };
        match kind {
            Kind::Slider(id) => {
                if let Some(cid) = id.downcast::<SliderConstraint>() {
                    if let Ok(mut c) = self.world.constraint_mut(cid) {
                        let _ = c.set_motor_settings(
                            MotorSettings::default().torque_limits(-strength, strength),
                        );
                        let _ = c.set_max_friction_force(friction);
                    }
                }
            }
            Kind::Hinge(id) => {
                if let Some(cid) = id.downcast::<HingeConstraint>() {
                    if let Ok(mut c) = self.world.constraint_mut(cid) {
                        let _ = c.set_motor_settings(
                            MotorSettings::default().torque_limits(-strength, strength),
                        );
                        let _ = c.set_max_friction_torque(friction);
                    }
                }
            }
        }
    }

    fn set_motion_type(
        &mut self,
        handle: BodyHandle,
        motion: MotionType,
    ) -> Result<(), BackendError> {
        let Some(id) = self.id(handle) else {
            return Err(BackendError::Body("motion type on a missing body".into()));
        };
        let mut body = self
            .world
            .body_mut(id)
            .map_err(|e| BackendError::Body(format!("motion type: {e:?}")))?;
        let oxi = match motion {
            MotionType::Static => oxijolt::MotionType::Static,
            MotionType::Dynamic => oxijolt::MotionType::Dynamic,
            MotionType::Kinematic => oxijolt::MotionType::Kinematic,
        };
        body.set_motion_type(oxi, Activation::Activate)
            .map_err(|e| BackendError::Body(format!("motion type: {e:?}")))
    }

    fn set_body_shape(
        &mut self,
        handle: BodyHandle,
        shape: &ShapeDesc,
        mass: Option<f32>,
    ) -> Result<(), BackendError> {
        let Some(id) = self.id(handle) else {
            return Err(BackendError::Body("set_shape on a missing body".into()));
        };
        let built = Self::build_shape(shape)?;

        // Jolt refuses `set_shape` while a constraint uses the body
        // (`UsedByConstraint`). Every stock chassis is hinged to its wheels, so
        // a buckle hits this on every motorized robot. Remove the joints,
        // re-shape, and create them again; each keeps its description and its
        // motor command, and the current angle/offset becomes the new
        // constraint's reference so the travel limits do not move (F9).
        let affected: Vec<usize> = self
            .joints
            .iter()
            .enumerate()
            .filter_map(|(h, joint)| match joint {
                Some(Joint::Hinge { desc, .. })
                    if desc.body_a == handle || desc.body_b == handle =>
                {
                    Some(h)
                }
                Some(Joint::Slider { desc, .. })
                    if desc.body_a == handle || desc.body_b == handle =>
                {
                    Some(h)
                }
                _ => None,
            })
            .collect();

        let mut saved = Vec::with_capacity(affected.len());
        for h in affected {
            let Some(joint) = self.joints[h].take() else {
                continue;
            };
            let reference = match &joint {
                Joint::Hinge { id, .. } => {
                    let Some(cid) = id.downcast::<HingeConstraint>() else {
                        continue;
                    };
                    self.world
                        .constraint(cid)
                        .map(|c| c.current_angle())
                        .unwrap_or(0.0)
                }
                Joint::Slider { id, .. } => {
                    let Some(cid) = id.downcast::<SliderConstraint>() else {
                        continue;
                    };
                    self.world
                        .constraint(cid)
                        .map(|c| c.current_position())
                        .unwrap_or(0.0)
                }
            };
            let _ = self.world.remove_constraint(joint.id());
            saved.push((h, joint, reference));
        }

        let reshaped = self.apply_shape(id, &built, mass);

        let mut recreate_error = None;
        for (h, joint, reference) in saved {
            match joint {
                Joint::Hinge {
                    desc,
                    target,
                    driven,
                    ..
                } => match self.build_hinge(&desc, reference) {
                    Ok(new_id) => {
                        self.joints[h] = Some(Joint::Hinge {
                            id: new_id,
                            desc,
                            target,
                            driven,
                        });
                        self.restore_hinge(new_id, target, driven);
                    }
                    Err(e) => recreate_error = Some(e),
                },
                Joint::Slider {
                    desc,
                    target,
                    driven,
                    ..
                } => match self.build_slider(&desc, reference) {
                    Ok(new_id) => {
                        self.joints[h] = Some(Joint::Slider {
                            id: new_id,
                            desc,
                            target,
                            driven,
                        });
                        self.restore_slider(new_id, target, driven);
                    }
                    Err(e) => recreate_error = Some(e),
                },
            }
        }

        // Jolt keeps its own reference to the shape, but we own the published one
        // for the lifetime of the world.
        self.shapes.push(built);

        reshaped?;
        if let Some(e) = recreate_error {
            return Err(e);
        }
        Ok(())
    }
}

impl JoltBackend {
    /// Replace a body's shape after its constraints have been removed.
    fn apply_shape(
        &mut self,
        id: BodyId,
        built: &Shape,
        mass: Option<f32>,
    ) -> Result<(), BackendError> {
        let mut body = self
            .world
            .body_mut(id)
            .map_err(|e| BackendError::Body(format!("set_shape: {e:?}")))?;
        // A body with forces added since the last step refuses a new shape.
        // Nothing in this pass adds one between steps, but resetting is free
        // and keeps the re-bake valid whatever the caller did.
        body.reset_forces();
        body.set_shape(built, mass, Activation::Activate)
            .map_err(|e| BackendError::Body(format!("set_shape: {e:?}")))
    }

    /// Build one hinge from its description. `reference_angle` is the angle the
    /// constraint should already report at the bodies' current poses: Jolt
    /// measures the angle from where the two frames' normals coincide, so the
    /// second normal is rotated back by that angle.
    fn build_hinge(
        &mut self,
        desc: &HingeDesc,
        reference_angle: f32,
    ) -> Result<AnyConstraintId, BackendError> {
        let (Some(ia), Some(ib)) = (self.id(desc.body_a), self.id(desc.body_b)) else {
            return Err(BackendError::Body("hinge on a missing body".into()));
        };
        let (Some(sa), Some(sb)) = (self.state(desc.body_a), self.state(desc.body_b)) else {
            return Err(BackendError::Body("hinge on a body with no state".into()));
        };
        // Jolt takes the frames in world space at creation; the caller gives them
        // in body space, so bring them over (both sides end up coincident).
        let point_a = sa.position + sa.rotation * desc.anchor_a;
        let point_b = sb.position + sb.rotation * desc.anchor_b;
        let axis_a = (sa.rotation * desc.axis_a).normalize_or_zero();
        let axis_b = (sb.rotation * desc.axis_b).normalize_or_zero();
        let axis_a = if axis_a == Vec3::ZERO {
            Vec3::Z
        } else {
            axis_a
        };
        let axis_b = if axis_b == Vec3::ZERO {
            Vec3::Z
        } else {
            axis_b
        };
        let mut normal_b = normal_of(axis_b);
        if reference_angle != 0.0 {
            let q = Quat::from_axis_angle(axis_b, reference_angle);
            normal_b = Self::v(q * Self::un_v(normal_b));
        }

        let mut settings =
            HingeConstraintSettings::new(Self::rv(point_a), Self::v(axis_a), normal_of(axis_a))
                .frame2(Self::rv(point_b), Self::v(axis_b), normal_b)
                .limits(desc.limits[0], desc.limits[1])
                // Jolt resists rotation with this only while the motor is off —
                // which is exactly where a wheel's rolling resistance belongs: a
                // driven wheel is governed by its motor's torque instead.
                .max_friction_torque(desc.friction_torque.max(0.0));
        if desc.max_torque > 0.0 {
            settings = settings
                .motor(MotorSettings::default().torque_limits(-desc.max_torque, desc.max_torque));
        }
        let id = self
            .world
            .create_constraint(ia, ib, &settings)
            .map_err(|e| BackendError::Body(format!("hinge: {e:?}")))?;
        Ok(AnyConstraintId::from(id))
    }

    /// Build one slider from its description. `reference_offset` is the
    /// position the constraint should report at the current poses; the second
    /// frame point is shifted along the axis by that much so Jolt's zero does
    /// not move.
    fn build_slider(
        &mut self,
        desc: &SliderDesc,
        reference_offset: f32,
    ) -> Result<AnyConstraintId, BackendError> {
        let (Some(ia), Some(ib)) = (self.id(desc.body_a), self.id(desc.body_b)) else {
            return Err(BackendError::Body("slider on a missing body".into()));
        };
        let (Some(sa), Some(sb)) = (self.state(desc.body_a), self.state(desc.body_b)) else {
            return Err(BackendError::Body("slider on a body with no state".into()));
        };
        // As for a hinge: the frames are created coincident in world space.
        let point_a = sa.position + sa.rotation * desc.anchor_a;
        let point_b = sb.position + sb.rotation * desc.anchor_b;
        let axis_a = (sa.rotation * desc.axis_a).normalize_or_zero();
        let axis_b = (sb.rotation * desc.axis_b).normalize_or_zero();
        let axis_a = if axis_a == Vec3::ZERO {
            Vec3::Z
        } else {
            axis_a
        };
        let axis_b = if axis_b == Vec3::ZERO {
            Vec3::Z
        } else {
            axis_b
        };
        // Unlike a hinge's angular reference, a slider's position is measured
        // from the two body-local anchors, and both are recreated from the same
        // description; the offset is therefore already preserved. The parameter
        // is kept so the call site reads like the hinge's.
        let _ = reference_offset;

        let mut settings =
            SliderConstraintSettings::new(Self::rv(point_a), Self::v(axis_a), normal_of(axis_a))
                .frame2(Self::rv(point_b), Self::v(axis_b), normal_of(axis_b))
                .limits(desc.limits[0], desc.limits[1])
                // Jolt applies this only while the motor is off, which is where a parked
                // piston's hold belongs: a driven rod is governed by its motor's force.
                .max_friction_force(desc.friction_force.max(0.0));
        if desc.max_force > 0.0 {
            settings = settings
                .motor(MotorSettings::default().torque_limits(-desc.max_force, desc.max_force));
        }
        let id = self
            .world
            .create_constraint(ia, ib, &settings)
            .map_err(|e| BackendError::Body(format!("slider: {e:?}")))?;
        Ok(AnyConstraintId::from(id))
    }

    /// Replay a hinge's motor command on a recreated constraint.
    fn restore_hinge(&mut self, id: AnyConstraintId, target: f32, driven: bool) {
        let Some(cid) = id.downcast::<HingeConstraint>() else {
            return;
        };
        let Ok(mut c) = self.world.constraint_mut(cid) else {
            return;
        };
        if driven {
            c.set_motor_state(MotorState::Velocity);
            let _ = c.set_target_angular_velocity(clamp_speed(target, MAX_ANGULAR_SPEED));
        } else {
            c.set_motor_state(MotorState::Off);
        }
    }

    /// Replay a slider's motor command on a recreated constraint.
    fn restore_slider(&mut self, id: AnyConstraintId, target: f32, driven: bool) {
        let Some(cid) = id.downcast::<SliderConstraint>() else {
            return;
        };
        let Ok(mut c) = self.world.constraint_mut(cid) else {
            return;
        };
        if driven {
            c.set_motor_state(MotorState::Velocity);
            let _ = c.set_target_velocity(clamp_speed(target, MAX_LINEAR_SPEED));
        } else {
            c.set_motor_state(MotorState::Off);
        }
    }
}

/// A unit vector perpendicular to `axis`, for Jolt's hinge frame.
/// The fastest a joint's motor may be driven, in rad/s.
///
/// The game's own hazards ask for more than this — the Half Circle Arena's
/// `Hammer` smashes at `SetPowerSettings(95, 1200)`, i.e. 15 turns a second, and
/// `Smasher` at 95 — and the solver refuses a target past the limit rather than
/// clamping it, which leaves the arm standing still. Clamping is what the drive
/// means here anyway: every one of those hinges has angle limits a few degrees
/// wide, so the arm reaches its stop within a step either way.
const MAX_ANGULAR_SPEED: f32 = oxijolt::limits::MAX_ANGULAR_VELOCITY;

/// The fastest a slider's motor may be driven, in m/s.
const MAX_LINEAR_SPEED: f32 = oxijolt::limits::MAX_LINEAR_VELOCITY;

fn clamp_speed(target: f32, limit: f32) -> f32 {
    if target.is_finite() {
        target.clamp(-limit, limit)
    } else {
        0.0
    }
}

fn normal_of(axis: Vec3) -> m::Vec3 {
    let helper = if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let n = axis.cross(helper).normalize_or_zero();
    let n = if n == Vec3::ZERO { Vec3::X } else { n };
    m::Vec3::new(n.x, n.y, n.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{CollisionGroupDesc, HingeDesc, MotionType, ShapeDesc, SliderDesc};

    fn box_body(group: Option<CollisionGroupDesc>) -> BodyDesc {
        let mut d = BodyDesc::new(
            MotionType::Dynamic,
            ShapeDesc::Box {
                half_extents: Vec3::splat(0.5),
            },
        )
        .at(Vec3::ZERO)
        .with_mass(10.0)
        // Keep both bodies awake so `in_contact` is asked of a live pair.
        .with_sleeping(false);
        if let Some(g) = group {
            d = d.with_collision_group(g);
        }
        d
    }

    /// F6: the law itself, against the numbers the docs record for the octagon
    /// (floor 0.80, robot 0.30): Havok 0.854, Jolt 0.490.
    #[test]
    fn the_combine_law_is_havok_not_jolt() {
        let havok = havok_combined_friction(0.8, 0.3);
        let jolt = (0.8f32 * 0.3).sqrt();
        assert!((havok - 0.8544).abs() < 1e-3, "{havok}");
        assert!((jolt - 0.4899).abs() < 1e-3, "{jolt}");
        assert!((havok / jolt - 1.744).abs() < 1e-2);
    }

    /// The law as a *physical* measurement: a 1 kg box on a 20° slope with
    /// `µ = 0.3` on both surfaces. Jolt's geometric mean gives 0.300 < tan 20°
    /// = 0.364, so the box slides; Havok's 0.424 > 0.364, so it holds. This is
    /// the same difference F6 names, observed through the solver rather than
    /// read off the formula.
    #[test]
    fn havok_friction_holds_a_box_that_jolt_friction_lets_slide() {
        fn rot(angle: f32) -> m::Quat {
            let (s, c) = (angle * 0.5).sin_cos();
            m::Quat::from_xyzw(0.0, 0.0, s, c)
        }
        fn run(havok: bool) -> f32 {
            let angle = 20f32.to_radians();
            let q = rot(angle);
            let mut world =
                PhysicsWorld::new(WorldSettings::default().gravity(m::Vec3::new(0.0, -9.81, 0.0)))
                    .unwrap();
            let ground_shape = Shape::new_box(m::Vec3::new(2.0, 0.1, 2.0)).unwrap();
            let ground = world
                .create_body(
                    &ground_shape,
                    &BodySettings::new_static().rotation(q).friction(0.3),
                )
                .unwrap();
            // The ramp's top face is 0.1 above its centre; the box's half height
            // is 0.2, so its centre starts at 0.3 along the ramp's local Y.
            let ramp = Quat::from_xyzw(0.0, 0.0, (angle * 0.5).sin(), (angle * 0.5).cos());
            let start = ramp * Vec3::new(0.0, 0.3, 0.0) + Vec3::Y * 0.001;
            let box_shape = Shape::new_box(m::Vec3::new(0.2, 0.2, 0.2)).unwrap();
            let body = world
                .create_body(
                    &box_shape,
                    &BodySettings::new_dynamic()
                        .position(JoltBackend::rv(start))
                        .rotation(q)
                        .friction(0.3)
                        .allow_sleeping(false),
                )
                .unwrap();
            if havok {
                let table = Arc::new(RwLock::new(HashMap::from([
                    (ground.to_raw(), 0.3),
                    (body.to_raw(), 0.3),
                ])));
                world.set_contact_listener(Some(Arc::new(HavokFriction {
                    friction: table,
                    havok: true,
                    log: Arc::new(Mutex::new(Vec::new())),
                })));
            }
            for _ in 0..60 {
                let _ = world.step(1.0 / 60.0);
            }
            let end = world.body(body).unwrap().position();
            JoltBackend::un_rv(end).distance(start)
        }
        let without = run(false);
        let with = run(true);
        println!("20° slope: jolt {without:.3} m, havok {with:.3} m");
        assert!(
            without > 0.1,
            "Jolt's sqrt(µ₁·µ₂) should let the box slide at 20°: {without:.3} m"
        );
        assert!(
            with < 0.02,
            "Havok's sqrt(µ₁²+µ₂²) should hold the box at 20°: {with:.3} m"
        );
    }

    /// F9: reshaping a body that a hinge uses. Jolt refuses the plain
    /// `set_shape`, so the backend removes the hinge, reshapes, and recreates
    /// it with the current angle as the reference. The motor must still drive
    /// afterwards and the limits must stay in the same place.
    #[test]
    fn a_hinged_body_can_be_reshaped_without_losing_its_angle() {
        let mut backend = JoltBackend::new(Vec3::ZERO).unwrap();
        let a = backend
            .create_body(
                &BodyDesc::new(
                    MotionType::Static,
                    ShapeDesc::Box {
                        half_extents: Vec3::splat(0.5),
                    },
                )
                .at(Vec3::ZERO),
            )
            .unwrap();
        let b = backend
            .create_body(&box_body(None).at(Vec3::new(3.0, 0.0, 0.0)))
            .unwrap();
        let mut hinge = HingeDesc::new(a, b, Vec3::new(3.0, 0.0, 0.0), Vec3::Z);
        hinge.anchor_b = Vec3::ZERO;
        hinge = hinge.with_limits(-1.0, 1.0).with_motor(1000.0);
        let joint = backend.create_hinge(&hinge).unwrap();
        backend.drive_hinge(joint, 1.0);
        for _ in 0..30 {
            backend.step(1.0 / 60.0);
        }
        let before = backend.hinge_angle(joint);
        assert!(before.abs() > 0.2, "the motor turned the hinge: {before}");

        let new_shape = ShapeDesc::Box {
            half_extents: Vec3::new(0.5, 0.4, 0.5),
        };
        backend
            .set_body_shape(a, &new_shape, Some(10.0))
            .expect("a constrained body can be re-shaped by removing and recreating the joint");

        let after = backend.hinge_angle(joint);
        assert!(
            (after - before).abs() < 1e-3,
            "the hinge lost its travel reference: {before} -> {after}"
        );

        backend.drive_hinge(joint, 1.0);
        for _ in 0..10 {
            backend.step(1.0 / 60.0);
        }
        assert!(
            backend.hinge_angle(joint) > after + 0.05,
            "the recreated hinge still drives"
        );
    }

    /// The same for a piston's slider: its offset is its travel reference.
    #[test]
    fn a_sliding_body_can_be_reshaped_without_losing_its_offset() {
        let mut backend = JoltBackend::new(Vec3::ZERO).unwrap();
        let a = backend
            .create_body(
                &BodyDesc::new(
                    MotionType::Static,
                    ShapeDesc::Box {
                        half_extents: Vec3::splat(0.5),
                    },
                )
                .at(Vec3::ZERO),
            )
            .unwrap();
        let b = backend
            .create_body(&box_body(None).at(Vec3::new(2.0, 0.0, 0.0)))
            .unwrap();
        let mut slider = SliderDesc::new(a, b, Vec3::new(2.0, 0.0, 0.0), Vec3::X);
        slider.anchor_b = Vec3::ZERO;
        slider.limits = [-1.0, 1.0];
        slider.max_force = 1000.0;
        let joint = backend.create_slider(&slider).unwrap();
        backend.drive_slider(joint, 0.5);
        for _ in 0..24 {
            backend.step(1.0 / 60.0);
        }
        let before = backend.slider_offset(joint);
        assert!(before.abs() > 0.05, "the rod moved: {before}");

        backend
            .set_body_shape(
                a,
                &ShapeDesc::Box {
                    half_extents: Vec3::new(0.5, 0.4, 0.5),
                },
                Some(10.0),
            )
            .expect("a sliding body can be re-shaped by removing and recreating the joint");

        let after = backend.slider_offset(joint);
        assert!(
            (after - before).abs() < 1e-3,
            "the slider lost its travel reference: {before} -> {after}"
        );
        backend.drive_slider(joint, 0.5);
        for _ in 0..10 {
            backend.step(1.0 / 60.0);
        }
        assert!(
            (backend.slider_offset(joint) - after).abs() > 0.02,
            "the recreated slider still drives"
        );
    }

    /// F4: Jolt collides two bodies joined by a constraint unless a group filter
    /// says otherwise. Every stock robot's wheel sits *inside* its chassis'
    /// hull, so without this the hinge motor turns a wheel that the chassis has
    /// pinched and the robot never moves. Two bodies of one robot share a group
    /// id and never touch; a second robot (another id) still does.
    #[test]
    fn parts_of_one_robot_do_not_collide_but_two_robots_do() {
        let mut backend = JoltBackend::new(Vec3::ZERO).unwrap();
        let mine = CollisionGroupDesc {
            group: 1,
            sub_group: 0,
            sub_groups: 2,
        };
        let sibling = CollisionGroupDesc {
            group: 1,
            sub_group: 1,
            sub_groups: 2,
        };
        let other = CollisionGroupDesc {
            group: 2,
            sub_group: 0,
            sub_groups: 1,
        };
        let a = backend.create_body(&box_body(Some(mine))).unwrap();
        let b = backend.create_body(&box_body(Some(sibling))).unwrap();
        let c = backend.create_body(&box_body(Some(other))).unwrap();

        backend.step(1.0 / 60.0);
        assert!(
            !backend.in_contact(a, b),
            "two parts of the same robot must not collide"
        );
        assert!(
            backend.in_contact(a, c),
            "parts of different robots must still collide"
        );
    }
}
