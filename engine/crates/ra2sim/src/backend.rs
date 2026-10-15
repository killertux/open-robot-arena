//! Backend-neutral physics types and the [`PhysicsBackend`] trait.
//!
//! The sandbox talks only to this interface, so the concrete engine (Jolt
//! today) can be swapped for another by adding an implementation. All values
//! are in RA2 world units, treated as metres, in the engine's Y-up frame.

use glam::{Quat, Vec3};

/// Opaque handle to a body within a backend.
pub type BodyHandle = u32;

/// How a body is simulated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionType {
    Static,
    Dynamic,
    Kinematic,
}

/// One child of a compound shape, placed in the body's own space.
#[derive(Clone, Debug)]
pub struct CompoundChild {
    pub local_position: Vec3,
    pub local_rotation: Quat,
    pub shape: ShapeDesc,
    /// Caller tag, reported back on hits (we store the component index).
    pub tag: u32,
}

/// A collision shape.
#[derive(Clone, Debug)]
pub enum ShapeDesc {
    Box {
        half_extents: Vec3,
    },
    Sphere {
        radius: f32,
    },
    Cylinder {
        half_height: f32,
        radius: f32,
    },
    Capsule {
        half_height: f32,
        radius: f32,
    },
    /// Convex hull of `points` (the backend builds the hull).
    ConvexHull {
        points: Vec<Vec3>,
    },
    /// Triangle mesh (static/kinematic only in most engines).
    TriMesh {
        vertices: Vec<Vec3>,
        indices: Vec<[u32; 3]>,
    },
    /// Several shapes in one body, each with its own pose — how a body made of
    /// separately-positioned parts (a robot's welded components) is built.
    Compound(Vec<CompoundChild>),
}

/// Everything needed to create one body.
#[derive(Clone, Debug)]
pub struct BodyDesc {
    pub motion: MotionType,
    pub shape: ShapeDesc,
    pub position: Vec3,
    pub rotation: Quat,
    pub friction: f32,
    pub restitution: f32,
    /// Explicit mass in kg; `None` lets the backend derive it from the shape.
    pub mass: Option<f32>,
    pub linear_damping: f32,
    pub angular_damping: f32,
    /// Whether the backend may put this body to sleep when it settles.
    pub allow_sleeping: bool,
    /// A sensor detects other bodies without any collision response — the shape
    /// is a volume, not an obstacle. The engine's own joints hang from one, so
    /// their pin can be big enough for Jolt's constraint maths without becoming
    /// something a robot can drive into.
    pub is_sensor: bool,
    /// Free-form tag (used to tell arena from robot bodies).
    pub user_data: u64,
}

impl BodyDesc {
    pub fn new(motion: MotionType, shape: ShapeDesc) -> Self {
        Self {
            motion,
            shape,
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            friction: 0.6,
            restitution: 0.1,
            mass: None,
            linear_damping: 0.05,
            angular_damping: 0.2,
            allow_sleeping: true,
            is_sensor: false,
            user_data: 0,
        }
    }

    pub fn at(mut self, position: Vec3) -> Self {
        self.position = position;
        self
    }

    pub fn with_rotation(mut self, rotation: Quat) -> Self {
        self.rotation = rotation;
        self
    }

    pub fn with_friction(mut self, friction: f32) -> Self {
        self.friction = friction;
        self
    }

    pub fn with_restitution(mut self, restitution: f32) -> Self {
        self.restitution = restitution;
        self
    }

    pub fn with_mass(mut self, mass: f32) -> Self {
        self.mass = Some(mass);
        self
    }

    pub fn with_damping(mut self, linear: f32, angular: f32) -> Self {
        self.linear_damping = linear;
        self.angular_damping = angular;
        self
    }

    /// Bodies the game drives must never sleep, or they ignore later velocity
    /// changes (Jolt leaves a sleeping body untouched until something wakes it).
    pub fn with_sleeping(mut self, allow: bool) -> Self {
        self.allow_sleeping = allow;
        self
    }

    /// A volume that detects bodies without colliding with them.
    pub fn with_sensor(mut self, is_sensor: bool) -> Self {
        self.is_sensor = is_sensor;
        self
    }

    pub fn with_user_data(mut self, user_data: u64) -> Self {
        self.user_data = user_data;
        self
    }
}

/// The kinematic state of a body at one instant.
#[derive(Clone, Copy, Debug)]
pub struct BodyState {
    pub position: Vec3,
    pub rotation: Quat,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
}

/// Result of a ray cast.
#[derive(Clone, Copy, Debug)]
pub struct RayHit {
    pub body: BodyHandle,
    pub distance: f32,
    pub point: Vec3,
    pub normal: Vec3,
}

/// Opaque handle to a joint (constraint) within a backend.
pub type JointHandle = u32;

/// A prismatic (slider) joint between two bodies — a piston's rod (F8).
///
/// The anchors and axes are in each body's own local space and must name the
/// same world point/axis when the joint is created (which is how the mount
/// transform places them). The motor is a velocity motor along the axis, and its
/// "torque" is a force: the definition's `power` pair, the same field a motor
/// takes its speed and torque from.
#[derive(Clone, Copy, Debug)]
pub struct SliderDesc {
    pub body_a: BodyHandle,
    pub body_b: BodyHandle,
    /// The point both bodies share, in each body's local space.
    pub anchor_a: Vec3,
    pub anchor_b: Vec3,
    /// Slide axis in each body's local space.
    pub axis_a: Vec3,
    pub axis_b: Vec3,
    /// Travel limits in metres, measured from where the two frame points
    /// coincide (the rest pose). Jolt requires `min <= 0 <= max`, which is the
    /// shape of the definition's `prismatic = …, [-x, +y]` pair.
    pub limits: [f32; 2],
    /// Peak motor force in N; `0` creates the joint without a motor.
    pub max_force: f32,
    /// Force the joint's friction can resist sliding with (N); `0` is
    /// frictionless. A parked piston is held by this.
    pub friction_force: f32,
}

impl SliderDesc {
    /// A free-sliding joint at `anchor` along `axis`, both body-local.
    pub fn new(body_a: BodyHandle, body_b: BodyHandle, anchor: Vec3, axis: Vec3) -> Self {
        SliderDesc {
            body_a,
            body_b,
            anchor_a: anchor,
            anchor_b: anchor,
            axis_a: axis,
            axis_b: axis,
            limits: [-0.5, 0.5],
            max_force: 0.0,
            friction_force: 0.0,
        }
    }
}

/// A revolute joint between two bodies — the engine's motor.
///
/// The anchors and axes are given in each body's own local space and must name
/// the same world point/axis when the joint is created (which is how the mount
/// transform places them). The motor is a velocity motor: the game sets a target
/// angular velocity and the joint supplies torque up to `max_torque`.
#[derive(Clone, Copy, Debug)]
pub struct HingeDesc {
    pub body_a: BodyHandle,
    pub body_b: BodyHandle,
    /// Pivot in each body's local space.
    pub anchor_a: Vec3,
    pub anchor_b: Vec3,
    /// Rotation axis in each body's local space.
    pub axis_a: Vec3,
    pub axis_b: Vec3,
    /// Angle limits in radians (`[-PI, PI]`; both `PI` means free).
    pub limits: [f32; 2],
    /// Peak motor torque in N·m; `0` creates the joint without a motor.
    pub max_torque: f32,
    /// Torque the joint's friction can resist rotation with (N·m); `0` is
    /// frictionless. A wheel's rolling resistance is this — the game's `SetLimits`
    /// pair for a hinge, and the wheel definitions' `resistance`.
    pub friction_torque: f32,
}

impl HingeDesc {
    /// A free-swinging hinge at `anchor` about `axis`, both body-local.
    pub fn new(body_a: BodyHandle, body_b: BodyHandle, anchor: Vec3, axis: Vec3) -> Self {
        HingeDesc {
            body_a,
            body_b,
            anchor_a: anchor,
            anchor_b: anchor,
            axis_a: axis,
            axis_b: axis,
            limits: [-std::f32::consts::PI, std::f32::consts::PI],
            max_torque: 0.0,
            friction_torque: 0.0,
        }
    }

    pub fn with_limits(mut self, min: f32, max: f32) -> Self {
        self.limits = [min, max];
        self
    }

    pub fn with_motor(mut self, max_torque: f32) -> Self {
        self.max_torque = max_torque;
        self
    }
}

/// What a step did.
#[derive(Clone, Copy, Debug)]
pub struct StepInfo {
    /// `false` when the solver dropped contacts under load.
    pub complete: bool,
}

/// A physics-engine failure.
#[derive(Debug)]
pub enum BackendError {
    World(String),
    Shape(String),
    Body(String),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::World(m) => write!(f, "physics world: {m}"),
            BackendError::Shape(m) => write!(f, "physics shape: {m}"),
            BackendError::Body(m) => write!(f, "physics body: {m}"),
        }
    }
}

impl std::error::Error for BackendError {}

/// The engine-agnostic physics interface the sandbox drives.
pub trait PhysicsBackend {
    /// Backend name, for reporting.
    fn name(&self) -> &'static str;

    fn create_body(&mut self, desc: &BodyDesc) -> Result<BodyHandle, BackendError>;
    fn remove_body(&mut self, handle: BodyHandle);

    fn state(&self, handle: BodyHandle) -> Option<BodyState>;
    fn set_transform(&mut self, handle: BodyHandle, position: Vec3, rotation: Quat);

    fn apply_impulse(&mut self, handle: BodyHandle, impulse: Vec3);
    fn apply_impulse_at(&mut self, handle: BodyHandle, impulse: Vec3, point: Vec3);
    fn apply_angular_impulse(&mut self, handle: BodyHandle, torque_impulse: Vec3);
    fn set_velocity(&mut self, handle: BodyHandle, linear: Vec3, angular: Vec3);

    fn gravity(&self) -> Vec3;
    fn step(&mut self, dt: f32) -> StepInfo;
    /// Whether two bodies touched during the last step (a default `false` lets
    /// simpler backends skip contact reporting).
    fn in_contact(&self, _a: BodyHandle, _b: BodyHandle) -> bool {
        false
    }
    fn ray_cast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<RayHit>;

    /// Create a revolute joint. Backends without joints may reject this.
    fn create_hinge(&mut self, _desc: &HingeDesc) -> Result<JointHandle, BackendError> {
        Err(BackendError::Body(
            "joints unsupported by this backend".into(),
        ))
    }

    /// Create a prismatic joint. Backends without sliders may reject this.
    fn create_slider(&mut self, _desc: &SliderDesc) -> Result<JointHandle, BackendError> {
        Err(BackendError::Body(
            "sliders unsupported by this backend".into(),
        ))
    }

    /// Drive a prismatic joint's rod at `target_velocity` m/s; `0` parks it (the
    /// motor goes off and the joint's friction holds the rod).
    fn drive_slider(&mut self, _handle: JointHandle, _target_velocity: f32) {}

    /// The rod's offset along the slide axis in metres (`0.0` if unknown).
    fn slider_offset(&self, _handle: JointHandle) -> f32 {
        0.0
    }

    fn remove_joint(&mut self, _handle: JointHandle) {}

    /// Drive a hinge's motor: target angular velocity in rad/s. A target of zero
    /// stops the motor and lets the joint swing freely.
    fn drive_hinge(&mut self, _handle: JointHandle, _target_velocity: f32) {}

    /// The hinge's current angle in radians (`0.0` if unknown).
    fn hinge_angle(&self, _handle: JointHandle) -> f32 {
        0.0
    }

    /// Replace a body's collision shape in place, optionally with a new mass.
    /// Used to rebuild a robot's body after a part comes off: the body keeps its
    /// id, pose and joints.
    fn set_body_shape(
        &mut self,
        _handle: BodyHandle,
        _shape: &ShapeDesc,
        _mass: Option<f32>,
    ) -> Result<(), BackendError> {
        Err(BackendError::Body(
            "set_shape unsupported by this backend".into(),
        ))
    }
}
