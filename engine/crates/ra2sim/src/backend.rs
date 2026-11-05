//! Backend-neutral physics types and the [`PhysicsBackend`] trait.
//!
//! The sandbox talks only to this interface, so the concrete engine ([`crate::havok17`],
//! with Jolt as an opt-in fallback behind the `jolt` feature) can be swapped for another by adding an implementation. All values
//! are in RA2 world units, treated as metres, in the engine's Y-up frame.

use glam::{Mat3, Quat, Vec3};

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
    /// A closed triangle mesh on a moving body that Havok collides as a mesh
    /// (shape class `GEOMETRIC`): a robot part whose `.gmf` asks for that
    /// (`rig::collides_as_mesh`) and the chassis the game builds. Its mass
    /// and inertia are its hull's, and a backend without moving meshes (Jolt)
    /// collides it as the convex hull of `points`.
    MeshHull {
        points: Vec<Vec3>,
        indices: Vec<[u32; 3]>,
        /// The same vertices in the frame the original keeps the mesh's
        /// geometry in (the component's own, before it is placed in the body;
        /// empty: the same as `points`). Havok 1.x builds its tree over the
        /// triangles in that frame, and the tree orders the contacts (havok17,
        /// docs/29 section 12u).
        raw: Vec<Vec3>,
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
    /// Explicit inertia tensor (kg·m², about the centre of mass, body frame);
    /// `None` lets the backend derive it from the shape at the body's mass.
    pub inertia: Option<glam::Mat3>,
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
            inertia: None,
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

    pub fn with_inertia(mut self, inertia: glam::Mat3) -> Self {
        self.inertia = Some(inertia);
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
    /// The tags of the two collidables (compound children) that made the point
    /// ([`NO_TAG`] when the backend does not know them): the component each
    /// belongs to, as the original's event names the colliding collidables.
    pub tag_a: u32,
    pub tag_b: u32,
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

/// A [`ContactEvent`] tag that names no component.
pub const NO_TAG: u32 = u32::MAX;

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
    /// frictionless. Ours, for a parked burst motor or servo; a wheel's hinge
    /// has none (`rig::wheel_friction`, docs/29 section 12ae).
    pub friction_torque: f32,
    /// Havok's wheel constraint (a `Steering` unit's hubs, `0x553c30`)
    /// rather than the hinge: B may move along this axis (A's space, the
    /// unit's up) within a limit of `[0, 0]`, which acts once per solver call
    /// like a hinge's limit, and the block locks only the two directions
    /// across it.
    pub suspension: Option<Vec3>,
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
            suspension: None,
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
/// Havok-1.7-faithful dynamics of [`crate::havok17`], the default) or `jolt`
/// (Jolt Physics, an opt-in fallback that exists only in a build with the
/// `jolt` cargo feature). Without the feature `RA2_PHYSICS=jolt` is refused
/// with a message on stderr (once) and the world runs on `havok17`.
pub fn backend_kind() -> &'static str {
    match std::env::var("RA2_PHYSICS") {
        Ok(v) if v.eq_ignore_ascii_case("havok17") || v.eq_ignore_ascii_case("havok") => "havok17",
        #[cfg(feature = "jolt")]
        Ok(v) if v.eq_ignore_ascii_case("jolt") => "jolt",
        #[cfg(not(feature = "jolt"))]
        Ok(v) if v.eq_ignore_ascii_case("jolt") => {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| {
                eprintln!(
                    "RA2_PHYSICS=jolt: this build has no Jolt backend (it is the opt-in cargo \
                     feature `jolt`: build with `--features jolt`); using havok17"
                );
            });
            DEFAULT_BACKEND
        }
        _ => DEFAULT_BACKEND,
    }
}

/// The engine a world runs on when `RA2_PHYSICS` does not say.
pub const DEFAULT_BACKEND: &str = "havok17";

/// Create the physics world [`backend_kind`] selects. `gravity` is what the
/// arena's `.gmf` asks for; the Havok backend uses the game's own world gravity
/// instead (see [`crate::havok17::GRAVITY`]).
pub fn create_backend(gravity: Vec3) -> Result<Box<dyn PhysicsBackend>, BackendError> {
    Ok(match backend_kind() {
        #[cfg(feature = "jolt")]
        "jolt" => Box::new(crate::jolt::JoltBackend::new(gravity)?),
        _ => {
            let _ = gravity;
            Box::new(crate::havok17::Havok17Backend::new()?)
        }
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

    /// [`Self::ray_cast`] that does not see `exclude`. The default ignores the
    /// exclusion.
    fn ray_cast_excluding(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        _exclude: BodyHandle,
    ) -> Option<RayHit> {
        self.ray_cast(origin, direction, max_distance)
    }

    /// Create a revolute joint. Backends without joints may reject this.
    fn create_hinge(&mut self, _desc: &HingeDesc) -> Result<JointHandle, BackendError> {
        Err(BackendError::Body(
            "joints unsupported by this backend".into(),
        ))
    }

    /// Create a point-to-point (ball-and-socket) joint holding `anchor` (world
    /// space, at creation) of both bodies together. Backends without one may
    /// reject this.
    fn create_point_to_point(
        &mut self,
        _a: BodyHandle,
        _b: BodyHandle,
        _anchor: Vec3,
    ) -> Result<JointHandle, BackendError> {
        Err(BackendError::Body(
            "point-to-point joints unsupported by this backend".into(),
        ))
    }

    /// Create an angular dashpot (`GM_HavokAngularDashpot`): a damped spring
    /// pulling `b`'s orientation towards `a`'s times `rest` (`b`'s orientation
    /// relative to `a` that it holds). Backends without one may reject this.
    fn create_angular_dashpot(
        &mut self,
        _a: BodyHandle,
        _b: BodyHandle,
        _rest: Quat,
        _strength: f32,
        _damping: f32,
    ) -> Result<(), BackendError> {
        Err(BackendError::Body(
            "dashpots unsupported by this backend".into(),
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

    /// A body's mass, centre of mass (body frame, from its origin) and
    /// inverse inertia (body frame), when the backend holds them.
    fn mass_properties(&self, _handle: BodyHandle) -> Option<(f32, Vec3, Mat3)> {
        None
    }

    /// Steer a hinge: turn its free axis, in body A's frame, about `steer_axis`
    /// (also in A's frame) from where it was built, so that it reads
    /// `cos(angle) axis + sin(angle) (axis x steer_axis)` — Havok's wheel
    /// constraint's `setSteeringAngle` (`FUN_00553af0`). A backend without it
    /// ignores the call.
    fn steer_hinge(&mut self, _handle: JointHandle, _steer_axis: Vec3, _angle: f32) {}

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
    /// Make `bodies` one deactivation system (havok17, docs/29 section 12t):
    /// they freeze and wake together, with the default thresholds times
    /// `scale`. A robot's bodies are one already (its collision group).
    fn deactivation_system(&mut self, _bodies: &[BodyHandle], _scale: f32) {}

    /// Whether any of `bodies` has a contact (a point within the collision
    /// tolerance, or an overlap) with anything, the collision detection run
    /// where the bodies are now: the test the original lowers a robot onto its
    /// start point with (`FUN_00418b90`, docs/29 section 12v). `None` for a
    /// backend without one.
    fn spawn_touch(&mut self, _bodies: &[BodyHandle]) -> Option<bool> {
        None
    }

    /// Diagnostic: run the collision detection where the bodies are now, as at
    /// the end of a step piece, and keep its contacts for the next resolver
    /// (havok17 writes them as an `R` row under `RA2_H17_DUMP`;
    /// `examples/orderprobe.rs`). Backends without one do nothing.
    fn detect_now(&mut self) {}

    /// Diagnostic: every joint's controller state against where it is now,
    /// `[servo target - position, lock bound - position]` (0 where it has
    /// none), so a probe that moves the bodies can keep it
    /// (`examples/stepprobe.rs`). Backends without one give nothing.
    fn joint_errors(&self) -> Vec<[f32; 2]> {
        Vec::new()
    }

    /// Diagnostic: put back [`PhysicsBackend::joint_errors`] about the
    /// joints' positions now.
    fn set_joint_errors(&mut self, _errors: &[[f32; 2]]) {}

    /// Whether the body is active: not frozen by the backend's deactivation
    /// (havok17, docs/29 section 12t). Backends without one always say yes.
    fn is_active(&self, _handle: BodyHandle) -> bool {
        true
    }

    /// Set a body active or inactive by hand (havok17's `setActive`): an
    /// inactive body is not integrated and the constraint solver leaves its
    /// joints alone, a fixed body as well (docs/29 section 12w: a robot's
    /// chassis turned onto its start point stays inactive until its system
    /// wakes it at the end of the frame). Backends without deactivation
    /// ignore it.
    fn set_active(&mut self, _handle: BodyHandle, _active: bool) {}

    fn set_joint_strength(&mut self, _handle: JointHandle, _strength: f32, _friction: f32) {}

    /// Lock a joint where it is, or release it — the game's `Lock`.
    ///
    /// The original never parks a motor on friction: its hinge controller's
    /// `Lock(1)` (`FUN_00480e20`) reads the current angle (`FUN_00481190`),
    /// sets *both* hinge limits to it (`FUN_00480880(θ, θ)`) and turns the
    /// motor off (`atom + 0x91 = 0`); `Lock(0)` restores the stored limits
    /// (`+0x68/+0x6c`). The prismatic controller's (`FUN_004b50c0`) sets both
    /// travel limits to the current offset clamped to the travel. Returns
    /// `false` for a backend that has no such lock (the caller then parks the
    /// joint on its friction, see [`Self::set_joint_strength`]).
    fn lock_joint(&mut self, _handle: JointHandle, _locked: bool) -> bool {
        false
    }

    /// Ask for a prismatic controller's auto-lock inside the next substep
    /// (docs/29 section 12ay). The original's controller update (`FUN_004b5240`) is an
    /// action of the Havok world: it runs after a piece's integration and *before* that
    /// piece's constraint solver, sees the offsets the integration just made, and locks
    /// (`Lock(1)`: both travel limits to the offset, the rod's velocities to zero) when the
    /// offset is within `band` of `stop`. A caller that updates its controllers before the
    /// substep sees the offsets one piece late, so the rod meets its stop in the solver with
    /// all its velocity; this arms the same test where the original runs it. Valid for the
    /// next substep only; a backend without the hook ignores it.
    fn arm_slider_lock(&mut self, _handle: JointHandle, _stop: f32, _band: f32) {}

    /// Hand the backend a robot's `MagicAction` to run in its action stage, as the original does:
    /// the action is a Havok action (`0x4856f0`), so it runs after the integration of *every* piece
    /// of a substep, not once per frame (docs/29 section 12ba: the live rows have exactly one
    /// `MagicAction` apply per piece per chassis, whatever the piece's length). The caller keeps
    /// refreshing the records once a frame, as the robot's update does. `None` takes it away.
    /// Returns `false` for a backend that runs actions once per step: the caller then applies the
    /// action itself.
    fn set_piece_action(
        &mut self,
        _chassis: BodyHandle,
        _action: Option<crate::drive::MagicAction>,
    ) -> bool {
        false
    }

    /// Tell the backend which controllers the match has, in the order the original creates them,
    /// so it runs each armed lock where that controller sits in the world's action list
    /// (docs/29 section 12ba): `robots[r]` holds robot `r`'s chassis body and its hinge and slider
    /// joints in the order of their motors' components, `arena` the arena's joints in the order they were made. A
    /// controller (and the robot's `MagicAction`, [`Self::set_piece_action`]) listed after its group
    /// runs after the solver of the piece instead of before it. Called again with the full set whenever it grows; a backend without actions ignores it.
    fn declare_action_order(
        &mut self,
        _robots: &[(BodyHandle, Vec<JointHandle>)],
        _arena: &[JointHandle],
    ) {
    }

    /// Never collide these two bodies: a pair from the arena's
    /// `GMID_HAVOK_DIS_COLLISION_PAIRS`. A backend may ignore it.
    fn disable_collision(&mut self, _a: BodyHandle, _b: BodyHandle) {}

    /// Never collide body `a` with the piece `piece` (a child of its
    /// compound, in order) of body `b`: a pair the arena disables between a
    /// movable body and one of the fixed bodies merged into the static
    /// collider. A backend may ignore it.
    fn disable_collision_piece(&mut self, _a: BodyHandle, _b: BodyHandle, _piece: u32) {}

    /// Set a dynamic body's inertia tensor about its centre of mass (body
    /// frame), as [`BodyDesc::with_inertia`] does at creation. A backend may
    /// ignore it.
    fn set_body_inertia(&mut self, _handle: BodyHandle, _inertia: Mat3) {}

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
