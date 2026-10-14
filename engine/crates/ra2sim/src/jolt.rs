//! Jolt Physics backend (via the safe `oxijolt` bindings).
//!
//! This is the only module that knows about Jolt; the rest of the crate talks
//! to [`PhysicsBackend`].

use std::collections::HashMap;

use glam::{Quat, Vec3};
use oxijolt::prelude::math as m;
use oxijolt::prelude::{
    Activation, BodyId, BodySettings, PhysicsWorld, QueryFilter, RayCast, Shape, WorldSettings,
};

use crate::backend::{
    BackendError, BodyDesc, BodyHandle, BodyState, MotionType, PhysicsBackend, RayHit, ShapeDesc,
    StepInfo,
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
        .user_data(desc.user_data);
        if let Some(mass) = desc.mass {
            settings = settings.mass(mass);
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
                let _ = body.add_angular_impulse(Self::v(torque_impulse));
            }
        }
    }

    fn set_velocity(&mut self, handle: BodyHandle, linear: Vec3, angular: Vec3) {
        if let Some(id) = self.id(handle) {
            if let Ok(mut body) = self.world.body_mut(id) {
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
}
