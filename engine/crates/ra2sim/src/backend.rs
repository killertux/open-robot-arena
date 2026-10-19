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
    /// `shape` with the body's centre of mass moved by `offset` (shape space).
    /// Only the centre moves; the collision surface stays where it is. A robot's
    /// body uses it to put the centre at the components' mass-weighted mean
    /// instead of the collision hulls' volume-weighted one.
    OffsetCenterOfMass {
        shape: Box<ShapeDesc>,
        offset: Vec3,
    },
}

/// A body's membership in a robot's self-collision group.
///
/// A robot is several rigid bodies joined by motors. In the original those
/// bodies are one assembly: a wheel does not collide with the chassis it is
/// bolted to, nor a spinner arm with its own hull. Jolt collides two bodies
/// joined by a constraint unless a group filter says otherwise (the default
/// `ConstraintSettings` does not disable it), which is why a wheel embedded in
/// its chassis' hull stalls the whole robot. This is the neutral description of
/// that filter: every body of one robot shares `group` (unique per robot) and a
/// distinct `sub_group`, and the backend builds a table for the group in which
/// no two sub groups collide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionGroupDesc {
    /// Unique per robot. Bodies with different group ids always collide.
    pub group: u32,
    /// This body's index within the robot (`0..sub_groups`).
    pub sub_group: u32,
    /// How many bodies the robot has (the table's size).
    pub sub_groups: u32,
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
    /// Which robot's parts this body must not collide with (see
    /// [`CollisionGroupDesc`]). `None` collides with everything its layer allows.
    pub collision_group: Option<CollisionGroupDesc>,
    /// Free-form tag (used to tell arena from robot bodies).
    pub user_data: u64,
    /// A *phantom* riding on the body: a volume that only reports what it
    /// overlaps (see [`PhysicsBackend::phantom_touch`]). A wheel's
    /// `wheel_traction` object is one — `DISABLE_COLLISIONS` in its `.gmf` —
    /// which the game counts contacts with to know whether the wheel is on the
    /// ground (`Wheel` update `0x508080`). Body-local, like `shape`.
    pub phantom: Option<ShapeDesc>,
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
            collision_group: None,
            user_data: 0,
            phantom: None,
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

    /// Put this body in a robot's self-collision group; no two bodies of the
    /// same robot collide. See [`CollisionGroupDesc`].
    pub fn with_collision_group(mut self, group: CollisionGroupDesc) -> Self {
        self.collision_group = Some(group);
        self
    }

    pub fn with_user_data(mut self, user_data: u64) -> Self {
        self.user_data = user_data;
        self
    }

    /// Give the body a phantom volume (see [`BodyDesc::phantom`]).
    pub fn with_phantom(mut self, phantom: ShapeDesc) -> Self {
        self.phantom = Some(phantom);
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

/// One contact point of one step: Havok's collision event, regenerated every
/// step while two bodies touch (`FUN_00549c30`).
///
/// The two states are the bodies as they were at the *start* of the step, which
/// is what the original's event record measures its closing speed from
/// (`FUN_0045a510`): the events fire before the step resolves the contact.
#[derive(Clone, Copy, Debug)]
pub struct ContactEvent {
    pub a: BodyHandle,
    pub b: BodyHandle,
    /// Unit normal, pointing from `a` toward `b`.
    pub normal: Vec3,
    /// The contact point in world space.
    pub point: Vec3,
    pub state_a: BodyState,
    pub state_b: BodyState,
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

/// Which physics engine a world runs on: `RA2_PHYSICS=havok17` (the
/// Havok-1.7-faithful dynamics of [`crate::havok17`]) or `jolt` (Jolt Physics,
/// the default).
pub fn backend_kind() -> &'static str {
    match std::env::var("RA2_PHYSICS") {
        Ok(v) if v.eq_ignore_ascii_case("havok17") || v.eq_ignore_ascii_case("havok") => "havok17",
        Ok(v) if v.eq_ignore_ascii_case("jolt") => "jolt",
        _ => DEFAULT_BACKEND,
    }
}

/// The engine a world runs on when `RA2_PHYSICS` does not say.
pub const DEFAULT_BACKEND: &str = "jolt";

/// Create the physics world [`backend_kind`] selects. `gravity` is what the
/// arena's `.gmf` asks for; the Havok backend uses the game's own world gravity
/// instead (see [`crate::havok17::GRAVITY`]).
pub fn create_backend(gravity: Vec3) -> Result<Box<dyn PhysicsBackend>, BackendError> {
    Ok(match backend_kind() {
        "havok17" => Box::new(crate::havok17::Havok17Backend::new()?),
        _ => Box::new(crate::jolt::JoltBackend::new(gravity)?),
    })
}

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

    /// The body's mass in kg (`None` for a static body or one the backend does
    /// not know).
    fn mass(&self, _handle: BodyHandle) -> Option<f32> {
        None
    }

    /// The body's centre of mass in world space. The default is the body's
    /// origin, which is exact only for a body whose shape is centred on it.
    fn center_of_mass(&self, handle: BodyHandle) -> Option<Vec3> {
        self.state(handle).map(|s| s.position)
    }

    /// Add a force (N, world space, through the centre of mass) to the body's
    /// accumulator: it acts over the next step and is then cleared — Havok's
    /// `applyForce` (`body + 0x74`).
    fn add_force(&mut self, handle: BodyHandle, force: Vec3) {
        let dt = self.last_dt();
        self.apply_impulse(handle, force * dt);
    }

    /// Add a torque (N·m, world space) to the body's accumulator for the next
    /// step — Havok's `applyTorque` (`body + 0x84`).
    fn add_torque(&mut self, handle: BodyHandle, torque: Vec3) {
        let dt = self.last_dt();
        self.apply_angular_impulse(handle, torque * dt);
    }

    /// Whether the body's phantom ([`BodyDesc::phantom`]) overlapped another
    /// body (not of the same collision group) during the last step, with that
    /// surface's normal (pointing away from it). `None` when it touched nothing,
    /// or for a backend without phantoms.
    fn phantom_touch(&self, _handle: BodyHandle) -> Option<Vec3> {
        None
    }
    fn step(&mut self, dt: f32) -> StepInfo;
    /// The length of the last [`step`](Self::step) (a step before any is 1/60 s).
    /// A caller that turns a force into an impulse uses it as the next step's
    /// length.
    fn last_dt(&self) -> f32 {
        1.0 / 60.0
    }
    /// Whether two bodies touched during the last step (a default `false` lets
    /// simpler backends skip contact reporting).
    /// The contact points of the last [`step`](Self::step), one event per
    /// point, in a deterministic order. Empty for a backend that cannot report
    /// them.
    fn take_contacts(&mut self) -> Vec<ContactEvent> {
        Vec::new()
    }

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

    /// Make a body static, dynamic or kinematic in place.
    ///
    /// The arena scripts' `SetPinned(name, True)` wants exactly this: the named
    /// part stops moving, wherever it happens to be.
    fn set_motion_type(
        &mut self,
        _handle: BodyHandle,
        _motion: MotionType,
    ) -> Result<(), BackendError> {
        Err(BackendError::Body(
            "set_motion_type unsupported by this backend".into(),
        ))
    }

    /// Set a joint's drive strength and the friction that holds it while its
    /// motor is off — both in the joint's own units (N for a slider, N·m for a
    /// hinge).
    ///
    /// A hazard states these at any time, not only when it is built: `Hammer`
    /// parks its arm with `Lock(True)` and only reaches for `SetPowerSettings`
    /// when a robot steps into its zone, and `HellRaiser` throttles itself down
    /// again on the way back up. A parked joint is held by the friction, the way
    /// a parked piston is (F8).
    fn set_joint_strength(&mut self, _handle: JointHandle, _strength: f32, _friction: f32) {}

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
