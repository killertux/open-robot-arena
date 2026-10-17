//! Jolt Physics backend (via the safe `oxijolt` bindings).
//!
//! This is the only module that knows about Jolt; the rest of the crate talks
//! to [`PhysicsBackend`].

use std::collections::HashMap;

use glam::{Quat, Vec3};
use oxijolt::prelude::math as m;
use oxijolt::prelude::{
    Activation, BodyId, BodySettings, CompoundChild as JoltCompoundChild, PhysicsWorld,
    QueryFilter, RayCast, Shape, WorldSettings,
};
use oxijolt::{
    AnyConstraintId, CollisionGroup, GroupFilterTable, GroupFilterTableBuilder, HingeConstraint,
    HingeConstraintSettings, MotorSettings, MotorState, SliderConstraint, SliderConstraintSettings,
};

use crate::backend::{
    BackendError, BodyDesc, BodyHandle, BodyState, HingeDesc, JointHandle, MotionType,
    PhysicsBackend, RayHit, ShapeDesc, SliderDesc, StepInfo,
};

/// A physics world backed by Jolt 5.
pub struct JoltBackend {
    world: PhysicsWorld,
    /// handle → Jolt body id (indexed by `BodyHandle`).
    ids: Vec<Option<BodyId>>,
    /// Shapes must outlive the bodies that reference them.
    shapes: Vec<Shape>,
    /// Jolt body id (raw) → our handle, for ray-hit attribution.
    lookup: HashMap<u32, BodyHandle>,
    /// handle → Jolt constraint id (indexed by `JointHandle`).
    joints: Vec<Option<AnyConstraintId>>,
    /// A robot's self-collision table, by group id. Built once, when its first
    /// body is created, and never changes.
    collision_tables: HashMap<u32, GroupFilterTable>,
}

impl JoltBackend {
    /// Create a world with the given gravity vector (m/s²).
    pub fn new(gravity: Vec3) -> Result<Self, BackendError> {
        let settings = WorldSettings::default().gravity(Self::v(gravity));
        let world =
            PhysicsWorld::new(settings).map_err(|e| BackendError::World(format!("{e:?}")))?;
        Ok(Self {
            world,
            ids: Vec::new(),
            shapes: Vec::new(),
            lookup: HashMap::new(),
            joints: Vec::new(),
            collision_tables: HashMap::new(),
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
        Ok(handle)
    }

    fn remove_body(&mut self, handle: BodyHandle) {
        if let Some(id) = self.id(handle) {
            let _ = self.world.remove_body(id);
            self.lookup.remove(&id.to_raw());
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

        let mut settings =
            HingeConstraintSettings::new(Self::rv(point_a), Self::v(axis_a), normal_of(axis_a))
                .frame2(Self::rv(point_b), Self::v(axis_b), normal_of(axis_b))
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
        let handle = self.joints.len() as JointHandle;
        self.joints.push(Some(AnyConstraintId::from(id)));
        Ok(handle)
    }

    fn create_slider(&mut self, desc: &SliderDesc) -> Result<JointHandle, BackendError> {
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
        let handle = self.joints.len() as JointHandle;
        self.joints.push(Some(AnyConstraintId::from(id)));
        Ok(handle)
    }

    fn drive_slider(&mut self, handle: JointHandle, target_velocity: f32) {
        let Some(Some(any)) = self.joints.get(handle as usize).copied() else {
            return;
        };
        let Some(id) = any.downcast::<SliderConstraint>() else {
            return;
        };
        let Ok(mut c) = self.world.constraint_mut(id) else {
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
        let Some(Some(any)) = self.joints.get(handle as usize).copied() else {
            return 0.0;
        };
        let Some(id) = any.downcast::<SliderConstraint>() else {
            return 0.0;
        };
        self.world
            .constraint(id)
            .map(|c| c.current_position())
            .unwrap_or(0.0)
    }

    fn remove_joint(&mut self, handle: JointHandle) {
        if let Some(Some(id)) = self.joints.get(handle as usize).copied() {
            let _ = self.world.remove_constraint(id);
        }
        if let Some(slot) = self.joints.get_mut(handle as usize) {
            *slot = None;
        }
    }

    fn drive_hinge(&mut self, handle: JointHandle, target_velocity: f32) {
        let Some(Some(any)) = self.joints.get(handle as usize).copied() else {
            return;
        };
        let Some(id) = any.downcast::<HingeConstraint>() else {
            return;
        };
        let Ok(mut c) = self.world.constraint_mut(id) else {
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
        let Some(Some(any)) = self.joints.get(handle as usize).copied() else {
            return 0.0;
        };
        let Some(id) = any.downcast::<HingeConstraint>() else {
            return 0.0;
        };
        self.world
            .constraint(id)
            .map(|c| c.current_angle())
            .unwrap_or(0.0)
    }

    fn set_joint_strength(&mut self, handle: JointHandle, strength: f32, friction: f32) {
        let Some(Some(any)) = self.joints.get(handle as usize).copied() else {
            return;
        };
        let strength = strength.max(0.0);
        let friction = friction.max(0.0);
        if let Some(id) = any.downcast::<SliderConstraint>() {
            if let Ok(mut c) = self.world.constraint_mut(id) {
                let _ = c.set_motor_settings(
                    MotorSettings::default().torque_limits(-strength, strength),
                );
                let _ = c.set_max_friction_force(friction);
            }
        } else if let Some(id) = any.downcast::<HingeConstraint>() {
            if let Ok(mut c) = self.world.constraint_mut(id) {
                let _ = c.set_motor_settings(
                    MotorSettings::default().torque_limits(-strength, strength),
                );
                let _ = c.set_max_friction_torque(friction);
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
        {
            let mut body = self
                .world
                .body_mut(id)
                .map_err(|e| BackendError::Body(format!("set_shape: {e:?}")))?;
            body.set_shape(&built, mass, Activation::Activate)
                .map_err(|e| BackendError::Body(format!("set_shape: {e:?}")))?;
        }
        // Jolt keeps its own reference to the shape, but we own the published one
        // for the lifetime of the world.
        self.shapes.push(built);
        Ok(())
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
    use crate::backend::{CollisionGroupDesc, MotionType, ShapeDesc};

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
