//! A simulated robot: its rig assembled into jointed rigid bodies.
//!
//! The original builds a robot out of one Havok body per rigidly-mounted group
//! of components, with the motors exporting as Havok constraints between them
//! (see `docs/05-validation-report.md`). We do the same: `rig::assemble` groups
//! the components into bodies, each body gets a compound shape from its members'
//! geometry, and every motor becomes a hinge the engine drives.

use glam::{Mat4, Quat, Vec3};

use crate::backend::{
    BackendError, BodyDesc, BodyHandle, CollisionGroupDesc, CompoundChild, HingeDesc, JointHandle,
    MotionType, PhysicsBackend, ShapeDesc, SliderDesc,
};
use crate::controls::{ControlState, FULL};
use crate::drive::{MagicAction, Steer, WheelRecord};
use crate::energy::{Channel, Energy};
use crate::massprops;
use crate::motor::{self, HingeSettings, PrismaticSettings};
use crate::rig::{JointInfo, JointUse, RobotRig, Role, WheelDrive};
use crate::scene::USER_BOT;
use crate::sim::{DriveInput, DriveTuning};

/// Stock component friction/restitution (see the validation report).
/// One collision piece of a part (see [`Robot::collider_pieces`]): the part, the component tag,
/// the points in the part's frame and, for a mesh piece, its triangles.
pub type ColliderPiece = (usize, u32, Vec<Vec3>, Option<Vec<[u32; 3]>>);

/// Set in the tag of a piece whose collidable has no entry in the collision-event filter (a
/// motor's, piston's or axle mount's housing, an anchor, docs/29 12at): its contacts are no
/// event of its own, the other collidable's entry has to let them through.
pub const UNREGISTERED: u32 = 0x4000_0000;

/// Whether the `k`-th housing piece (`HavokParts::collision`) of `c` is registered with the filter.
///
/// Live (`R` rows of `frida/ra2_robotcolliders.js`, 3 fights: 109 collidables) the filter has an
/// entry for the chassis, for the pieces of a component's own body (a motor's rotor, the piston's
/// shaft, the hubs, an arm, a hammer) and for the pieces of a component with no rotor, but not for the
/// housing of a component that has a rotor (`anglemotor` against `axle`, `body` against `shaft`) or
/// for a component with no `passthru` that is `damageable = false` (the anchor): the stock
/// definitions name the registered objects under `passthru`. A steering unit's three base pieces
/// have one entry between them. `RA2_REGISTRY=0` registers every piece (the component's own rule).
fn housing_registered(c: &crate::rig::Component, k: usize) -> bool {
    if !registry_rule() {
        return true;
    }
    if matches!(c.role, Role::Power | Role::Control) {
        return false;
    }
    if !c.havok.rotor_collision.is_empty() {
        return c.class == "Steering" && k == 0;
    }
    c.has_passthru || c.damageable
}

/// `RA2_REGISTRY=0`: every collidable of a component raises events as its component's does.
fn registry_rule() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("RA2_REGISTRY").map_or(true, |v| v != "0"))
}

/// Whether the batteries, air tanks, battery packs and control boards collide as the original's
/// collidables do (docs/29 12at). `RA2_INTERNALS=0` leaves them out, as before.
pub fn internals_collide() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("RA2_INTERNALS").map_or(true, |v| v != "0"))
}

const STOCK_FRICTION: f32 = 0.30;
const STOCK_RESTITUTION: f32 = 0.30;

/// A fresh collision-group id for one robot's assembly. Bodies of the same
/// robot share the id and never collide with one another; different robots
/// always have different ids, so they do. Zero is skipped because Jolt's group
/// filter treats it as a valid id but a table's first entry is the natural
/// "unset" one in our own code.
fn next_robot_group() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// A blow to a chassis whose hitpoints are gone must exceed this to be a
/// critical hit (`FUN_0044f370`, constant at `0x6a69f8`).
pub const CRITICAL_BLOW: f32 = 40.0;
/// The critical hit that destroys the robot (`FUN_004df120`: `crit >= 3`).
pub const CRITICALS_TO_DESTROY: u32 = 3;

/// What one blow did to a component ([`Robot::damage`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Applied {
    /// Hitpoints actually removed; the score a hit is worth.
    pub effective: f32,
    /// The component should come off the robot.
    pub detach: bool,
    /// The blow was a critical hit on the chassis.
    pub critical: bool,
    /// That critical hit was the third: the robot is DESTROYED.
    pub destroyed: bool,
}

/// The sign of a positive ("clockwise") command about a joint's axis. The
/// original builds the target velocity as `-direction * 0.01 * speed`
/// (`0x481450`); which way a motor's axis points is settled by the survey of
/// the stock bots, whose wiring drives the two sides of a robot with opposite
/// `Spin Clockwise`/`Spin Counter-clockwise` wires.
const CLOCKWISE: f32 = -1.0;
/// The sense an airborne wheel's hinge motor turns, per part (docs/29 section
/// 12af): `+1` as the drive step always did, `-1` for a wheel whose velocity
/// the original does not negate.
///
/// The hinge controller (`0x481450`) builds the velocity as `trunc(-p · 0.01 ·
/// speed)` and negates it (`0x4814e1..0x48151a`) when a component of the
/// hinge atom's axis in body B's frame (`atom + 0x60`) is below -0.5. B is the
/// wheel's body, which the original builds in the frame of the motor's rotor
/// object: the wheel's placement times the rotor's own `TM` (`axle`: a
/// quarter turn about x for `anglemotor1`, the identity for `anglemotor2`;
/// live `spinup-*` `B` rows, 15 wheels of 7 robots), so that axis is `(TM ·
/// placement)⁻¹ · axle`. Live (`hingectl-scout-alarm-box__live__000` `L`
/// rows) it reads `(0, 1, 0)` for the wheels on `anglemotor1` and `(0, 0,
/// -0.8)` on `anglemotor2`, and the wheels in the air at full throttle turn
/// `+x` on the first and `-x` on the second. Ours turned them all `-x`,
/// which is the negated sense.
/// `RA2_AIR_SIGN=0` turns every wheel the old way.
pub(crate) fn air_signs(rig: &RobotRig) -> Vec<f32> {
    let mut v = vec![1.0; rig.parts.len()];
    if std::env::var("RA2_AIR_SIGN").is_ok_and(|s| s == "0") {
        return v;
    }
    for (p, part) in rig.parts.iter().enumerate() {
        let (Some(joint), root) = (part.joint, &rig.components[part.root]) else {
            continue;
        };
        if joint.use_kind != JointUse::Wheel || joint.slider {
            continue;
        }
        let Some(hinge) = root
            .parent
            .and_then(|par| rig.components[par].hinge.as_ref())
        else {
            continue;
        };
        let placed = glam::Mat3::from_mat4(root.local) * glam::Mat3::from_mat4(hinge.rotor_tm);
        let axis = placed.transpose() * joint.axis;
        let negated = axis.x < -0.5 || axis.y < -0.5 || axis.z < -0.5;
        if !negated {
            v[p] = -1.0;
        }
    }
    v
}
/// The game's frame (`FUN_004ac1d0`: two Havok substeps of 1/60 s): its
/// components tick once a frame with the frame's time.
pub const FRAME: f32 = 1.0 / 30.0;

/// How far under a wheel the ground still counts as touching it (m): the
/// `tolerance` of 0.1 the game's `.gmf` hands Havok (`SimulationObject`).
const WHEEL_REACH: f32 = 0.1;

/// How long a wheel stays "touching" after it last was (s): the 0.2 the wheel's
/// own contact tracker compares its timer against (`0x508080` against the
/// constant at `0x6a6080`).
const WHEEL_GROUND_MEMORY: f32 = 0.2;

/// Rolling drag of a wheel, per unit of its `resistance`, as a rate (1/s) times
/// the robot's mass. **Fitted**: the original Scout's coast-down is exponential
/// with a time constant of 0.75 s (k = 1.34/s); its two mini wheels declare
/// `resistance = 0.1`, and with the ~0.3/s our contact friction adds, a rate of
/// 5.3 per unit resistance gives our Scout the same 1.34/s. The thrust-to-drag
/// ratio then gives a terminal speed of about 10.5 m/s (the original's 11.1).
const WHEEL_DRAG_RATE: f32 = 4.3;

/// How fast a grounded wheel kills its sideways speed (1/s) and the most
/// sideways acceleration it can give, in g. **Fitted** (the original's lateral
/// grip is not recovered): stiff enough that a robot holds a straight line at
/// 10 m/s, capped at half a g so a skid-steered four-wheeler can still scrub its
/// wheels sideways to turn.
const WHEEL_LATERAL_RATE: f32 = 20.0;

/// How stiffly the wheels' speeds are tied to their commands (1/s). **Fitted**
/// only in that it must be stiff compared with the drag; see
/// [`Robot::push_wheels`].
const WHEEL_COUPLING_RATE: f32 = 30.0;

const WHEEL_LATERAL_G: f32 = 0.5;

/// The collision skin every robot hull is grown by (m).
///
/// **Inferred from the original's rest heights**: Havok's convex shapes carry a
/// radius their hull is grown by, and with a 0.05 m skin the stock Scout rests
/// at y = -1.880 and ALARM (without its hammer, which does not rest on the
/// floor in the original) at -1.802, against the original's measured -1.884 and
/// -1.811 (0.0, the bare hull, gives -1.935 and -1.855; 0.04 and 0.06 give
/// -1.910/-1.869 for the Scout). It also makes a mini wheel 0.38 m, not 0.33 m,
/// where it meets the ground.
const HULL_SKIN: f32 = 0.045;

/// A body's piece: its points, its triangles when it collides as a mesh, its
/// own rigid body's mass, and, for a motor's rotor, the triangles that place
/// that mass.
///
/// The stock rotor proxies (`axle_collision` of the angle motors and the
/// Z-Tek) are boxes without their inboard end face. Live, every wheel body on
/// such a rotor has its centre of mass 9 mm (angle motor) and 11 mm (Z-Tek)
/// further out along the axle than the proxy's box centre puts it, and no
/// other body differs (`contacts-nohaz-*__live__*`, five robots); the mass
/// sits where the tetrahedra from the hinge pivot to the proxy's triangles
/// put it, 3/8 of the box from its closed end. The bodies' inertia stays the
/// box's: live, ALARM's wheel spins with 0.5569 kg·m² and Wide Load's with
/// 1.2935, exactly the box's (docs/29 section 12h).
type MassPiece = (
    Vec<Vec3>,
    Option<Vec<[u32; 3]>>,
    f32,
    Option<Vec<[u32; 3]>>,
    Option<Vec<[u32; 3]>>,
    // The original's mass properties at unit mass (centre, inertia about it)
    // in the part's frame, computed in the piece's own frame (`add`).
    Option<(Vec3, glam::Mat3)>,
);

/// The original's mass properties of a rigid body at unit mass (centre,
/// inertia about it), integrated in the body's own frame (`pts` as the `.gmf`
/// stores them: an open surface's integrals depend on the axes) and carried by
/// `m` into the part's frame.
fn original_props(
    pts: &[Vec3],
    faces: Option<&Vec<[u32; 3]>>,
    m: Mat4,
) -> Option<(Vec3, glam::Mat3)> {
    if !original_inertia() {
        return None;
    }
    let (_, c, j) = massprops::original_mass_props(pts, faces?, 1.0)?;
    let r = glam::Mat3::from_mat4(m);
    Some((m.transform_point3(c), r * j * r.transpose()))
}

/// `RA2_INERTIA=old`: the rigid bodies' mass properties from hulls and the
/// rotor's box, as before docs/29 section 12bf (default: the original's own
/// integrals over each body's triangles, `massprops::original_mass_props`).
fn original_inertia() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("RA2_INERTIA").map_or(true, |v| v != "old"))
}

/// Where an open piece's mass sits: its triangles' solid from `pivot`.
fn open_centroid(pts: &[Vec3], tris: &[[u32; 3]], pivot: Vec3) -> Option<Vec3> {
    massprops::triangles_mass_from(pts, tris, pivot).map(|(_, c, _)| c)
}

/// Where a component's mass sits in a Havok body (measured live on 14 stock
/// robots, `assembly-*__live__000`, to 0.1 mm on the chassis bodies): each of
/// its rigid bodies' mass at the centroid of the solid that body's geometry
/// bounds (a mesh body's own triangles, a convex body's hull, a rotor's
/// triangles from the hinge `pivot`): their total mass and its centre, `None`
/// for no mass or geometry.
fn mass_point_weighted(pieces: &[MassPiece], pivot: Vec3) -> Option<(f32, Vec3)> {
    let (w, at) = pieces
        .iter()
        .filter(|p| p.2 > 0.0)
        .filter_map(|(pts, mesh, m, open, _, props)| {
            if original_inertia() {
                if let Some((c, _)) = props {
                    return Some((*m, *c));
                }
            }
            open.as_ref()
                .and_then(|tris| open_centroid(pts, tris, pivot).map(|c| (0.0, c)))
                .or_else(|| {
                    mesh.as_ref()
                        .and_then(|tris| massprops::mesh_volume_centroid(pts, tris))
                })
                .or_else(|| massprops::hull_volume_centroid(pts))
                .map(|(_, c)| (*m, c))
        })
        .fold((0.0f32, Vec3::ZERO), |(w, at), (m, c)| (w + m, at + c * m));
    (w > 1e-9).then(|| (w, at / w))
}

fn hull_skin() -> f32 {
    HULL_SKIN
}

/// A wheel's hull as a fine round ring instead of the model's few-sided one.
///
/// The stock tyres are 8 to 13 sided, and a polygon rolls with a bump at every
/// facet; the ring has the polygon's *mean* radius (between its corners and its
/// flats) and the same width, about the thinnest axis of the hull (the axle).
fn round_wheel(points: &[Vec3]) -> Vec<Vec3> {
    if points.len() < 6 {
        return points.to_vec();
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in points {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let size = hi - lo;
    let axis = if size.z <= size.x && size.z <= size.y {
        2
    } else if size.y <= size.x {
        1
    } else {
        0
    };
    let (a, b) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let centre = (lo + hi) * 0.5;
    let radius = points
        .iter()
        .map(|p| ((p[a] - centre[a]).powi(2) + (p[b] - centre[b]).powi(2)).sqrt())
        .fold(0.0f32, f32::max);
    // Corners within 5 % of the largest radius, split between the two faces.
    let corners = points
        .iter()
        .filter(|p| {
            ((p[a] - centre[a]).powi(2) + (p[b] - centre[b]).powi(2)).sqrt() > radius * 0.95
        })
        .count();
    let sides = (corners / 2).clamp(6, 32) as f32;
    let mean = radius * (1.0 + (std::f32::consts::PI / sides).cos()) * 0.5;
    let mut out = Vec::with_capacity(48);
    for k in 0..24 {
        let t = k as f32 / 24.0 * std::f32::consts::TAU;
        for face in [lo[axis], hi[axis]] {
            let mut p = centre;
            p[a] += mean * t.cos();
            p[b] += mean * t.sin();
            p[axis] = face;
            out.push(p);
        }
    }
    out
}

/// With `RA2_LEGACY_WHEELS`, a multiplier on the hinge motor's speed and torque
/// cap (`RA2_WHEEL_PASSES`, default 1). The static study of the Havok hinge servo
/// (`0x535a50`) suggested the servo might run `k` times per substep, which would
/// give the Scout 11.1 m/s; a live measurement in the original refuted it
/// (k = 1.02, with the Scout's drive hinge at 17 rad/s and 675 N·m), so this stays
/// a comparison knob, not a fit.
/// The velocity to hand the backend for a hinge controller's: `0` turns the
/// motor off, so a non-zero percentage whose truncated velocity is zero (the
/// original's motor *on* at velocity 0, which holds the hinge) is passed as a
/// vanishing velocity of the percentage's sign.
fn hinge_target(velocity: f32, percent: i32) -> f32 {
    if velocity == 0.0 && percent != 0 {
        1e-9 * percent.signum() as f32
    } else {
        velocity
    }
}

fn motor_passes() -> f32 {
    std::env::var("RA2_WHEEL_PASSES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0)
}

/// The first hit of a ray that is not one of `own`'s bodies, never seeing
/// `wheel` at all: a hit on another of the robot's own bodies is passed through
/// and the ray goes on from just beyond it.
///
/// The interim wheel sampler casts up from below the floor. With the robots
/// assembled as the original builds them, a wheel resting on the floor can
/// reach a centimetre below its surface (Jolt lets a resting body sink by its
/// penetration slop); the ray then met the wheel first — and a ray that starts
/// inside a convex shape hits it at once, so it could not get past — which
/// read "no ground" for a wheel standing on the floor: seven stock robots
/// (Sentinel, Wide Load, ...) did not move on Jolt.
fn ray_past_own(
    physics: &dyn PhysicsBackend,
    origin: Vec3,
    direction: Vec3,
    reach: f32,
    own: &[BodyHandle],
    wheel: BodyHandle,
) -> Option<crate::backend::RayHit> {
    let mut from = origin;
    let mut left = reach;
    for _ in 0..8 {
        let hit = physics.ray_cast_excluding(from, direction, left, wheel)?;
        if !own.contains(&hit.body) {
            return Some(hit);
        }
        let step = hit.distance + 1e-3;
        if step >= left {
            return None;
        }
        from += direction * step;
        left -= step;
    }
    None
}

/// `RA2_LEGACY_WHEELS=1` drives wheels with a plain velocity motor again (for
/// comparison runs).
fn legacy_wheels() -> bool {
    std::env::var_os("RA2_LEGACY_WHEELS").is_some()
}

/// Whether robots drive through the game's own drive model
/// ([`crate::drive::MagicAction`]) rather than the fitted wheel thrust. It is
/// the model the `havok17` backend runs; `RA2_DRIVE=magic|interim` overrides.
fn magic_drive(physics: &dyn PhysicsBackend) -> bool {
    match std::env::var("RA2_DRIVE") {
        Ok(v) if v.eq_ignore_ascii_case("magic") => true,
        Ok(v) if v.eq_ignore_ascii_case("interim") => false,
        _ => physics.name() == "havok17",
    }
}

/// The game's raw torque unit: `SetPowerSettings` stores the definition's
/// torque at `+0xb4` and ×40 of it at `+0xb8` (`FUN_004810e0`, `0x6a69f8`).
const TORQUE_UNIT: f32 = 40.0;

/// One body as the Havok backend gets it.
struct HavokPart {
    shape: ShapeDesc,
    /// Per component that has mass in this body (a rotor under its motor's
    /// index): its weight and mass point, in the part's frame.
    points: Vec<(usize, f32, Vec3)>,
    /// The body's inertia about its centre of mass (part frame).
    inertia: Option<glam::Mat3>,
    mass: f32,
    friction: f32,
    restitution: f32,
    phantom: Option<ShapeDesc>,
}

/// One grounded wheel, sampled for [`Robot::push_wheels`].
struct WheelSample {
    parent: usize,
    info: JointInfo,
    wheel: WheelDrive,
    command: f32,
    /// The way a positive command pushes the robot, along the ground.
    along: Vec3,
    /// Across the wheel, along the ground.
    side: Vec3,
    velocity: Vec3,
    point: Vec3,
}

/// Live state of one assembled body.
pub struct PartState {
    pub body: BodyHandle,
    /// The hinge to the parent body, when a motor sits between them.
    pub joint: Option<JointHandle>,
    pub alive: bool,
}

/// One robot in the world.
pub struct Robot {
    pub rig: RobotRig,
    /// Per part, the share of its mass it had the last time the original's
    /// build woke the robot ([`Robot::build_shares`]); 1 for the chassis and
    /// for every part complete by then.
    pub build_share: Vec<f32>,
    pub team: usize,
    /// One entry per [`crate::rig::Part`].
    pub parts: Vec<PartState>,
    /// Remaining hitpoints per component.
    pub health: Vec<f32>,
    /// Whether each component is still on the robot.
    pub attached: Vec<bool>,
    /// Whether the component was torn off by a single blow larger than its
    /// `fracture` threshold (`FUN_0045f2b0`'s second break rule). The repaired
    /// state is the original's `Team+0x13c` fractured flag; a component that
    /// merely ground down to zero hitpoints is detached but not fractured.
    pub fractured: Vec<bool>,
    /// Natural mass × `mass_scale`.
    pub mass: f32,
    pub mass_scale: f32,
    /// Robot state 1, DESTROYED: the chassis took its third critical hit.
    pub destroyed: bool,
    /// Critical hits the chassis has taken (`bot + 0xbc`).
    pub criticals: u32,
    /// Robot state 2, IMMOBILIZED: it stayed inside a sqrt(2) m sphere for 20 s.
    pub immobilized: bool,
    /// `plus.eliminatePlayer` — the original's robot state 3. The robot stays in
    /// the world (it may still be falling), but it is out of the match.
    pub eliminated: bool,
    /// The electric hazard's `plus.zap` visual, as a deadline on the match
    /// clock. The shipped `Electricity` hazard applies the damage itself.
    pub zapped_until: f32,
    /// The control board's live state: held keys, the AI's `Input` levels,
    /// latched switches (see [`crate::controls`]).
    pub control: ControlState,
    /// Where each part's burst device is in its fire/re-cock cycle.
    burst: Vec<BurstPhase>,
    /// Per wheel part: seconds since it last had ground within reach.
    wheel_air: Vec<f32>,
    /// Per wheel part: the surface normal it last found under it.
    wheel_normal: Vec<Vec3>,
    /// Per part: the mass-weighted centre of its components, in the part's frame.
    mass_centre: Vec<Vec3>,
    /// Per part: the maximum torque (force) its joint motor was last set to.
    motor_torque: Vec<f32>,
    /// Per hinge part: its hinge controller's power settings ([`motor`]),
    /// capped by the load it carries.
    hinge: Vec<HingeSettings>,
    /// Per hinge part: the pivot and the reference perpendicular in the child
    /// body's frame, as the constraint was built (the load cap's `d`).
    load_frame: Vec<(Vec3, Vec3)>,
    /// Per slider part: its prismatic controller's power settings.
    prismatic: Vec<PrismaticSettings>,
    /// Per burst part: the fire timer (`+0x298`: 0.6 s for a burst motor,
    /// 0.25 s for a burst piston) that ends the stroke.
    burst_timer: Vec<f32>,
    /// Per slider part: the prismatic controller's direction (`+0x80`).
    slider_dir: Vec<f32>,
    /// Per burst part: a `Fire` press not yet taken by the component's tick
    /// (`+0x284/+0x285` on a piston, `+0x288/+0x289` on a burst motor).
    burst_pending: Vec<bool>,
    /// Per part: the sense an airborne wheel's hinge motor turns (+1 as ours
    /// always did, -1 for the wheels the original's rule at `0x4814e1..0x48151a`
    /// negates; docs/29 section 12af).
    air_sign: Vec<f32>,
    /// Per burst part: whether its joint is locked. A lock holds until the
    /// device unlocks it (`Lock(0)` at the fire and at the end of the
    /// stroke); arriving at a stop locks it (`0x481450`, `0x4b5240`), and
    /// nothing in between unlocks it.
    burst_locked: Vec<bool>,
    /// Game time since the components last ticked: they tick once a frame
    /// ([`FRAME`]), not every substep.
    frame_clock: f32,
    /// A diagnostic's schedule of component ticks ([`Robot::set_tick_schedule`]):
    /// per drive step from now, the tick's time or none.
    tick_schedule: std::collections::VecDeque<Option<f32>>,
    /// Per part: how long a burst device still draws power (`0.6 s` after a
    /// burst motor is triggered, `0.25 s` for a piston: `0x446916`, `0x44880d`).
    burst_draw: Vec<f32>,
    /// Weapon triggers since [`Robot::take_fire_events`]: the motor component
    /// of each trigger press that makes the original play its `firesound`
    /// (`docs/35-audio-events.md`). Bounded; nothing but the client's audio reads it.
    fire_events: Vec<usize>,
    /// The robot's Flamethrowers: the component index, the component's state
    /// ([`crate::flame`]) and the command its `Fire` wires last delivered.
    pub flames: Vec<FlameUnit>,
    /// What the flames' sounds should do since [`Robot::take_flame_sounds`]:
    /// `(component, call)`. Bounded; only the client's audio reads it.
    flame_sounds: Vec<(usize, crate::flame::Sound)>,
    /// The batteries and air tanks and what they supplied last step
    /// ([`crate::energy`]): the HUD's battery and air bars.
    pub energy: Energy,
    /// The game's own drive model ([`crate::drive`]), when the robot uses it.
    pub magic: Option<MagicAction>,
}

/// One Flamethrower of a robot.
#[derive(Clone, Debug)]
pub struct FlameUnit {
    pub component: usize,
    pub state: crate::flame::Flamethrower,
    /// The `Fire` command (`-100..=100`) last delivered to the handler.
    pub command: i32,
}

/// Where a Flamethrower's jet starts and where it flies (`0x5048e0`).
#[derive(Clone, Copy, Debug)]
pub struct FlameJet {
    pub robot_component: usize,
    pub on: bool,
    pub position: Vec3,
    pub velocity: Vec3,
}

/// A burst motor's or piston's cycle: parked (cocked) at its start, released at
/// full speed to its end, then cocked again slowly (the tutorial: "slowly, but
/// powerfully cocks the axle ... when it is triggered, the spring is released and
/// the axle spins at very high torque to its ending position").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BurstPhase {
    Cocked,
    Firing,
    Cocking,
}

impl Robot {
    /// Assemble and spawn the robot at `position`/`rotation`.
    ///
    /// Bodies are created first, then the joints between them (a joint needs both
    /// bodies to exist).
    pub fn spawn(
        physics: &mut dyn PhysicsBackend,
        rig: RobotRig,
        team: usize,
        position: Vec3,
        rotation: Quat,
        mass_scale: f32,
    ) -> Result<Self, BackendError> {
        let bot = Mat4::from_rotation_translation(rotation, position);
        let n = rig.components.len();
        let group = next_robot_group();
        let sub_groups = rig.parts.len() as u32;

        let havok = physics.name() == "havok17";
        // Every part starts where the assembly put it: live, a burst piston's
        // rod is built at its neutral point (ALARM's hammer 0.55 m in front of
        // its piston, at chassis height) and a burst motor's rotor at its
        // start angle, both already in the rig's rest pose.
        let mut parts = Vec::with_capacity(rig.parts.len());
        let mut mass_points: Vec<Vec<(usize, f32, Vec3)>> = Vec::new();
        // Each body's inertia as built (body frame), for a piston rod's.
        let mut built_inertia: Vec<Option<glam::Mat3>> = Vec::with_capacity(rig.parts.len());
        for (p, part) in rig.parts.iter().enumerate() {
            let world = bot * rig.components[part.root].local;
            let (_, rot, pos) = world.to_scale_rotation_translation();
            let (shape, mass, friction, restitution, phantom, inertia) = if havok {
                let h = Self::havok_part(&rig, p, None);
                mass_points.push(h.points);
                (
                    h.shape,
                    h.mass,
                    h.friction,
                    h.restitution,
                    h.phantom,
                    h.inertia,
                )
            } else {
                let mass = part
                    .members
                    .iter()
                    .map(|&i| rig.components[i].mass)
                    .sum::<f32>();
                (
                    Self::part_shape(&rig, &part.members),
                    mass,
                    Self::part_friction(&rig, part.members.iter().copied()),
                    STOCK_RESTITUTION,
                    None,
                    None,
                )
            };
            let natural = mass;
            let mass = (mass * mass_scale).max(0.2);
            let mut inertia = inertia.map(|i| i * (mass / natural.max(1e-3)));
            // A piston's rod takes its housing body's inertia with the
            // diagonal scaled by the definition's factor
            // ([`crate::rig::PistonStats::rod_inertia`]: the `prismatic` line's
            // last number, `0x4b4d4f..0x4b4de7`), the products of inertia
            // unchanged. Measured live: ALARM's 33 kg hammer rod reads
            // (9.830, 14.541, 7.282) against its chassis' (49.149, 72.705,
            // 36.411); Wide Load's 63 kg rod the same against its chassis
            // (`fightcontacts-scout-alarm-box__live__001..003`,
            // `contacts-nohaz-wideload__live__001`, P rows `+0x19c`).
            if havok {
                if let (Some(parent), Some(info)) = (part.parent, part.joint) {
                    if info.slider && info.rod_inertia != 0.0 {
                        if let Some(Some(pi)) = built_inertia.get(parent) {
                            let mut i = *pi;
                            i.x_axis.x *= info.rod_inertia;
                            i.y_axis.y *= info.rod_inertia;
                            i.z_axis.z *= info.rod_inertia;
                            // The rod's body has the piston's frame, not the
                            // frame of the part bolted to it: live ALARM's rod
                            // is its chassis turned half a turn about y, as
                            // its Burst Piston, and Wide Load's is its
                            // chassis' frame, as its piston (`B` rows of
                            // `fightcontacts-scout-alarm-box__live__001`,
                            // `contacts-nohaz-wideload__live__001`; docs/29
                            // §12q). Ours is the root component's, so the
                            // tensor is turned into it.
                            let (_, piston, _) = (bot * rig.components[info.motor].local)
                                .to_scale_rotation_translation();
                            let q = glam::Mat3::from_quat(rot.inverse() * piston);
                            inertia = Some(q * i * q.transpose());
                        }
                    }
                }
            }
            built_inertia.push(inertia);
            let mut desc = BodyDesc::new(MotionType::Dynamic, shape)
                .at(pos)
                .with_rotation(rot)
                .with_friction(friction)
                .with_restitution(restitution)
                .with_mass(mass)
                // Driven / hit every step: never let the backend park a body.
                .with_sleeping(false)
                // A robot's own parts never collide with each other (F4): the
                // wheel sits inside the chassis' hull, and without this it is
                // pinched there and the motor cannot turn it.
                .with_collision_group(CollisionGroupDesc {
                    group,
                    sub_group: p as u32,
                    sub_groups,
                })
                .with_user_data(USER_BOT);
            if let Some(phantom) = phantom {
                desc = desc.with_phantom(phantom);
            }
            if let Some(inertia) = inertia {
                desc = desc.with_inertia(inertia);
            }
            let body = physics.create_body(&desc)?;
            parts.push(PartState {
                body,
                joint: None,
                alive: true,
            });
        }

        // Joints, now that every body exists.
        for p in 0..rig.parts.len() {
            let (Some(parent), Some(info)) = (rig.parts[p].parent, rig.parts[p].joint) else {
                continue;
            };
            let child_pivot = info.anchor;
            let joint = if info.slider {
                Self::make_slider(physics, &rig, &parts, parent, p, info, child_pivot)?
            } else {
                Self::make_hinge(physics, &rig, &parts, parent, p, info, child_pivot)?
            };
            parts[p].joint = Some(joint);
        }

        let magic = magic_drive(physics)
            .then(|| Self::magic_records(physics, &rig, &parts, &mass_points, rotation));
        let build_share = Self::build_shares(&rig, &mass_points);

        let mut robot = Robot {
            magic,
            build_share,
            health: rig.components.iter().map(|c| c.hitpoints).collect(),
            attached: vec![true; n],
            fractured: vec![false; n],
            parts,
            mass: rig.mass * mass_scale,
            mass_scale,
            destroyed: false,
            criticals: 0,
            immobilized: false,
            eliminated: false,
            zapped_until: -1.0,
            control: ControlState::default(),
            burst: vec![BurstPhase::Cocked; rig.parts.len()],
            wheel_air: vec![WHEEL_GROUND_MEMORY; rig.parts.len()],
            wheel_normal: vec![Vec3::Y; rig.parts.len()],
            motor_torque: rig
                .parts
                .iter()
                .map(|q| q.joint.map(|j| j.max_torque).unwrap_or(0.0))
                .collect(),
            hinge: rig
                .parts
                .iter()
                .map(|q| match q.joint {
                    Some(j) if !j.slider => HingeSettings::new(j.rate, j.max_torque / TORQUE_UNIT),
                    _ => HingeSettings::new(0.0, 0.0),
                })
                .collect(),
            load_frame: (0..rig.parts.len())
                .map(|q| Self::load_frame(&rig, q))
                .collect(),
            prismatic: rig
                .parts
                .iter()
                .map(|q| PrismaticSettings {
                    speed: q.joint.map(|j| j.rate).unwrap_or(0.0),
                    force: q.joint.map(|j| j.max_torque / TORQUE_UNIT).unwrap_or(0.0),
                })
                .collect(),
            burst_timer: vec![0.0; rig.parts.len()],
            slider_dir: vec![0.0; rig.parts.len()],
            burst_pending: vec![false; rig.parts.len()],
            air_sign: air_signs(&rig),
            burst_locked: vec![true; rig.parts.len()],
            frame_clock: FRAME,
            tick_schedule: std::collections::VecDeque::new(),
            burst_draw: vec![0.0; rig.parts.len()],
            fire_events: Vec::new(),
            flames: rig
                .components
                .iter()
                .filter(|c| c.is_flamethrower())
                .map(|c| FlameUnit {
                    component: c.index,
                    state: crate::flame::Flamethrower::default(),
                    command: 0,
                })
                .collect(),
            flame_sounds: Vec::new(),
            energy: Self::build_energy(&rig),
            mass_centre: rig
                .parts
                .iter()
                .map(|part| Self::mass_center(&rig, &part.members).unwrap_or(Vec3::ZERO))
                .collect(),
            team,
            rig,
        };
        robot.cap_loads(physics);
        // A burst device is built locked where it stands: live the Scout's
        // burst motor hinge is limited to `[0, 0]` from its first set-up
        // (`spinup-*__live__000` `H` rows), and ALARM's burst piston rod comes
        // back to its rest point when the match pulls the robot round onto
        // its start point (docs/29 section 12w). The first tick keeps it so
        // (`burst_locked`).
        for (p, part) in robot.rig.parts.iter().enumerate() {
            if let (Some(info), Some(joint)) = (part.joint, robot.parts[p].joint) {
                if info.use_kind == JointUse::Burst {
                    physics.lock_joint(joint, true);
                }
            }
        }
        Ok(robot)
    }

    /// A hinge part's pivot and reference perpendicular in its own body's
    /// frame, as `0x525830` stores them when it builds the constraint (`+0x40`,
    /// `+0x80`). The perpendicular is taken in the robot's frame.
    fn load_frame(rig: &RobotRig, p: usize) -> (Vec3, Vec3) {
        let Some(info) = rig.parts[p].joint.filter(|j| !j.slider) else {
            return (Vec3::ZERO, Vec3::ZERO);
        };
        let to_b = rig.components[rig.parts[p].root].local.inverse();
        (
            to_b.transform_point3(info.pivot),
            to_b.transform_vector3(motor::reference_perpendicular(info.axis))
                .normalize_or_zero(),
        )
    }

    /// Take the parked joints' friction (ours: what an undriven motor or axle
    /// resists its motion with, see `havok17::joints::Joint::friction`) off
    /// (`false`) or put it back (`true`). While the match places a robot
    /// (`duel::turn_into_place`) nothing holds a hinge in the original: live
    /// a wheel pulled round onto its start point keeps turning at 0.4-0.6
    /// rad/s, slowing by under 1 % a substep, until its robot freezes
    /// (docs/29 section 12w). A burst motor's or piston's joint keeps its
    /// friction (the game's `Lock`).
    pub fn joint_friction(&self, physics: &mut dyn PhysicsBackend, on: bool) {
        for (p, part) in self.rig.parts.iter().enumerate() {
            let (Some(info), Some(joint)) = (part.joint, self.parts[p].joint) else {
                continue;
            };
            if info.slider || info.use_kind == JointUse::Burst || info.friction_torque <= 0.0 {
                continue;
            }
            let strength = match info.use_kind {
                JointUse::Passive => 0.0,
                _ => self.motor_torque[p],
            };
            let friction = if on { info.friction_torque } else { 0.0 };
            physics.set_joint_strength(joint, strength, friction);
        }
    }

    /// Cap every hinge controller of the robot by the load it carries
    /// ([`HingeSettings::load_cap`]). The original does it for all of a robot's
    /// controllers each time a component is added (`0x4dc390` from
    /// `0x4dc287`) and again when one comes off (`0x4dbe3f`, `0x417c1d`); the
    /// last call of the build is the one that holds, with every body complete.
    /// The motor's torque follows.
    pub fn cap_loads(&mut self, physics: &mut dyn PhysicsBackend) {
        for p in 0..self.rig.parts.len() {
            let Some(info) = self.rig.parts[p].joint else {
                continue;
            };
            if info.slider || info.use_kind == JointUse::Passive || !self.parts[p].alive {
                continue;
            }
            let body = self.parts[p].body;
            let (mass, com) = match physics.mass_properties(body) {
                Some((m, com, _)) => (m, com),
                None => (
                    physics.mass(body).unwrap_or(0.0),
                    self.mass_centre.get(p).copied().unwrap_or(Vec3::ZERO),
                ),
            };
            if mass <= 0.0 {
                continue;
            }
            let (pivot_b, perp_b) = self.load_frame[p];
            let d = motor::load_offset(pivot_b, com, perp_b);
            self.hinge[p].load_cap(mass, d);
            if std::env::var_os("RA2_MOTOR_DEBUG").is_some() {
                eprintln!(
                    "load cap: part {p} ({}) m {mass:.2} d {d:.3} com {com:?} pivot {pivot_b:?} perp {perp_b:?} inertia {:?}: {:?}",
                    self.rig.components[self.rig.parts[p].root].name,
                    physics
                        .mass_properties(body)
                        .map(|(_, _, inv)| inv.inverse()),
                    self.hinge[p]
                );
            }
            if let Some(joint) = self.parts[p].joint {
                let k = if info.use_kind == JointUse::Wheel {
                    motor_passes()
                } else {
                    1.0
                };
                let torque = self.hinge[p].max_torque * k;
                // A burst motor's strength follows its stroke (`drive_burst`).
                if info.use_kind != JointUse::Burst {
                    physics.set_joint_strength(joint, torque, info.friction_torque);
                    self.motor_torque[p] = torque;
                }
            }
        }
    }

    /// Part `p`'s hinge controller settings as they stand ([`motor`]): the
    /// definition's power pair, capped by the load (`0x480510`) or reset by a
    /// burst motor's stroke.
    pub fn hinge_settings(&self, p: usize) -> Option<HingeSettings> {
        self.rig.parts.get(p)?.joint.filter(|j| !j.slider)?;
        self.hinge.get(p).copied()
    }

    /// The robot's batteries and air tanks, full.
    fn build_energy(rig: &RobotRig) -> Energy {
        let mut energy = Energy::none();
        for c in &rig.components {
            let is = |name: &str| {
                c.class.eq_ignore_ascii_case(name) || c.base.eq_ignore_ascii_case(name)
            };
            if is("Battery") {
                energy.add_source(c.index, Channel::Elec, c.power, c.hitpoints);
            } else if is("AirTank") {
                energy.add_source(c.index, Channel::Air, c.power, c.hitpoints);
            }
        }
        energy
    }

    /// The electric and air draw of this step, in the component files' units
    /// (`dt` already multiplied in): `docs/30-energy-and-watch-camera.md` §3.
    ///
    /// * a spin motor (and a wheel's motor) draws `|command|` a second, the
    ///   wired `-100..=100` value the original keeps in `SpinMotor+0x298`;
    /// * a burst motor draws its `elecmaxinoutrate`, a burst piston its
    ///   `airmaxinoutrate`, while it is triggered or travelling;
    /// * any other motor or piston draws its definition's rate scaled by
    ///   `|command| / 100` (**not measured**: the stock servo and linear
    ///   actuator classes were not driven on the original).
    fn power_demand(&mut self, dt: f32) -> (f32, f32) {
        let mut elec = 0.0;
        let mut air = 0.0;
        let mut seen: Vec<usize> = Vec::new();
        for p in 0..self.rig.parts.len() {
            if !self.parts[p].alive {
                continue;
            }
            let Some(info) = self.rig.parts[p].joint else {
                continue;
            };
            let motor = info.motor;
            if seen.contains(&motor) || !self.attached.get(motor).copied().unwrap_or(false) {
                continue;
            }
            seen.push(motor);
            let c = &self.rig.components[motor];
            let (power, role) = (c.power, c.role);
            let spin = c.class.eq_ignore_ascii_case("SpinMotor")
                || c.base.eq_ignore_ascii_case("SpinMotor")
                || c.power.elec_rate == 0.0;
            let command = self
                .control
                .command(&self.rig.wiring, motor)
                .unwrap_or(0.0)
                .abs();
            if c.steer_power.is_some() {
                // A `Steering` unit asks `|command| · dt` like a spin motor
                // (`0x4f6cd0`, vtable slot `+0x5c`); steering itself is free.
                elec += command * dt;
                continue;
            }
            match info.use_kind {
                JointUse::Passive => {}
                JointUse::Burst => {
                    // The firesound plays on the trigger itself: a piston
                    // (`0x448750`) on every press, an electric burst motor
                    // (`0x446810`) only while it is cocked.
                    if self.control.fired(&self.rig.wiring, motor)
                        && (info.slider || self.burst[p] == BurstPhase::Cocked)
                        && self.fire_events.len() < 256
                    {
                        self.fire_events.push(motor);
                    }
                    // The trigger starts a fixed draw time (the burst timers of
                    // the original), whatever the stroke then takes.
                    if self.burst_draw[p] <= 0.0
                        && self.burst[p] == BurstPhase::Cocked
                        && self.control.fired(&self.rig.wiring, motor)
                    {
                        self.burst_draw[p] = if info.slider { 0.25 } else { 0.6 };
                    }
                    if self.burst_draw[p] > 0.0 {
                        self.burst_draw[p] -= dt;
                        if info.slider {
                            air += power.air_rate.abs() * dt;
                        } else {
                            elec += power.elec_rate.abs() * dt;
                        }
                    }
                }
                JointUse::Wheel | JointUse::Spun | JointUse::Piston => {
                    if info.slider || role == Role::Piston {
                        air += power.air_rate.abs() * command / FULL * dt;
                    } else if spin {
                        elec += command * dt;
                    } else {
                        elec += power.elec_rate.abs() * command / FULL * dt;
                    }
                }
            }
        }
        // A Flamethrower asks `dt` of both channels at its definition's rates,
        // whatever its level, while its jet is on or its burst timer runs (`0x504c50`).
        for f in &self.flames {
            if f.state.drawing() && self.attached[f.component] {
                let power = self.rig.components[f.component].power;
                elec += power.elec_rate.abs() * dt;
                air += power.air_rate.abs() * dt;
            }
        }
        (elec, air)
    }

    /// The Havok body of part `p` (see [`crate::rig::HavokParts`]): one convex
    /// piece per colliding rigid body of its components (no skin, the real
    /// wheel hubs), a motor's rotor moved to the part it drives, every rigid
    /// body's mass (phantoms included), the top rigid body's material — the
    /// chassis body is the game's own, with Havok's defaults (live 0.5 / 0.2) —
    /// and the wheels' traction phantoms.
    ///
    /// `attached`: only the members still on the robot count (a body rebuilt
    /// after a part came off or the chassis buckled); `None` takes them all.
    fn havok_part(rig: &RobotRig, p: usize, attached: Option<&[bool]>) -> HavokPart {
        let part = &rig.parts[p];
        let frame = rig.components[part.root].local;
        // Which motors drive another part: their rotor belongs there.
        let drives: Vec<usize> = rig
            .parts
            .iter()
            .filter_map(|q| q.joint.map(|j| j.motor))
            .collect();
        let mut children = Vec::new();
        let mut phantom = Vec::new();
        let (mut mass, mut moment) = (0.0f32, Vec3::ZERO);
        let mut points = Vec::new();
        // Per rigid body with mass: (mass, centroid, inertia about it).
        let mut bodies_inertia: Vec<(f32, Vec3, glam::Mat3)> = Vec::new();
        type Piece = MassPiece;
        // `phantoms`: rigid bodies that collide with nothing but carry mass.
        let mut add = |pieces: &[Piece],
                       registered: &[bool],
                       phantoms: &[(Vec<Vec3>, f32)],
                       phantom_faces: &[Vec<[u32; 3]>],
                       m: Mat4,
                       weight: f32,
                       tag: u32,
                       at: Vec3| {
            let mut placed = Vec::new();
            // Each placed piece's points in the component's own frame (where
            // the original keeps a mesh's geometry).
            let mut raws = Vec::new();
            let mut placed_registered = Vec::new();
            for (k, (pts, mesh, piece_mass, open, faces, _)) in pieces.iter().enumerate() {
                let local: Vec<Vec3> = pts.iter().map(|q| m.transform_point3(*q)).collect();
                if massprops::hull_volume_centroid(&local).is_some() {
                    placed.push((
                        local,
                        mesh.clone(),
                        *piece_mass,
                        open.clone(),
                        faces.clone(),
                        original_props(pts, faces.as_ref(), m),
                    ));
                    raws.push(pts.clone());
                    placed_registered.push(registered.get(k).copied().unwrap_or(true));
                }
            }
            let ghosts: Vec<Piece> = phantoms
                .iter()
                .enumerate()
                .map(|(gk, (pts, pm))| {
                    let gfaces = phantom_faces.get(gk).cloned().filter(|f| !f.is_empty());
                    let gprops = original_props(pts, gfaces.as_ref(), m);
                    (
                        pts.iter().map(|q| m.transform_point3(*q)).collect(),
                        None,
                        *pm,
                        None,
                        gfaces.clone(),
                        gprops,
                    )
                })
                .collect();
            let centre = {
                // The colliding bodies share what the component weighs beyond
                // its phantoms, in proportion to their own masses (by volume
                // when they store none, as every stock wheel does).
                let ghost_mass: f32 = ghosts.iter().map(|g| g.2).sum();
                let own: f32 = placed.iter().map(|p| p.2).sum();
                let body_mass = (weight - ghost_mass).max(0.0);
                let mut all: Vec<Piece> = if own > 0.0 {
                    placed
                        .iter()
                        .map(|(p, t, pm, o, f, g)| {
                            (
                                p.clone(),
                                t.clone(),
                                pm * body_mass / own,
                                o.clone(),
                                f.clone(),
                                *g,
                            )
                        })
                        .collect()
                } else {
                    let vols: Vec<f32> = placed
                        .iter()
                        .map(|(p, _, _, _, _, _)| {
                            massprops::hull_volume_centroid(p).map_or(0.0, |v| v.0)
                        })
                        .collect();
                    let total: f32 = vols.iter().sum();
                    placed
                        .iter()
                        .zip(&vols)
                        .map(|((p, t, _, o, f, g), v)| {
                            let share = if total > 0.0 { v / total } else { 0.0 };
                            (
                                p.clone(),
                                t.clone(),
                                body_mass * share,
                                o.clone(),
                                f.clone(),
                                *g,
                            )
                        })
                        .collect()
                };
                all.extend(ghosts.iter().cloned());
                for (pts, mesh, pm, open, faces, props) in &all {
                    if *pm <= 0.0 {
                        continue;
                    }
                    // The original's own integrals over the body's triangles.
                    if original_inertia() {
                        if let Some((c, j)) = props {
                            let (c, i) = (*c, *j * *pm);
                            if std::env::var_os("RA2_MASS_DEBUG").is_some() {
                                eprintln!(
                                    "   piece {} pts {} tris {} mass {pm:.3} com {c:?} I diag {:?} off {:?}",
                                    tag,
                                    pts.len(),
                                    faces.as_ref().map_or(0, |f| f.len()),
                                    [i.x_axis.x, i.y_axis.y, i.z_axis.z],
                                    [i.y_axis.x, i.z_axis.x, i.z_axis.y]
                                );
                            }
                            bodies_inertia.push((*pm, c, i));
                            continue;
                        }
                    }
                    // A rotor: its box's inertia, carried from where its
                    // triangles put its mass.
                    if let Some(c) = open.as_ref().and_then(|tris| open_centroid(pts, tris, at)) {
                        let props = crate::havok17::mass::of_shape(&ShapeDesc::ConvexHull {
                            points: pts.clone(),
                        });
                        if props.volume > 0.0 {
                            bodies_inertia.push((*pm, c, props.inertia * (*pm / props.volume)));
                        }
                        continue;
                    }
                    // A mesh body's own triangles, as for its centre.
                    if let Some((v, c, i)) = mesh
                        .as_ref()
                        .and_then(|tris| massprops::mesh_inertia(pts, tris))
                    {
                        bodies_inertia.push((*pm, c, i * (*pm / v)));
                        continue;
                    }
                    let props = crate::havok17::mass::of_shape(&ShapeDesc::ConvexHull {
                        points: pts.clone(),
                    });
                    if props.volume > 0.0 {
                        bodies_inertia.push((*pm, props.com, props.inertia * (*pm / props.volume)));
                    }
                }
                // Nothing to collide: the rest of the mass sits at `at`.
                let rest = if placed.is_empty() { body_mass } else { 0.0 };
                match mass_point_weighted(&all, at) {
                    Some((w, c)) if w + rest > 0.0 => (c * w + at * rest) / (w + rest),
                    _ => at,
                }
            };
            if weight > 0.0 {
                mass += weight;
                moment += centre * weight;
                points.push((tag as usize, weight, centre));
            }
            if std::env::var_os("RA2_MASS_DEBUG").is_some() {
                eprintln!(
                    "mass: part body tag {tag} weight {weight:.3} at {centre:?} origin {at:?} pieces {}",
                    placed.len()
                );
                for (pts, _, _, _, _, _) in &placed {
                    let lo = pts.iter().copied().fold(Vec3::splat(f32::MAX), Vec3::min);
                    let hi = pts.iter().copied().fold(Vec3::splat(f32::MIN), Vec3::max);
                    eprintln!("      piece {} points, box {lo:?}..{hi:?}", pts.len());
                }
            }
            for (((pts, mesh, _, _, _, _), raw), registered) in
                placed.into_iter().zip(raws).zip(placed_registered)
            {
                let shape = match mesh {
                    Some(indices) => ShapeDesc::MeshHull {
                        points: pts,
                        indices,
                        raw,
                    },
                    None => ShapeDesc::ConvexHull { points: pts },
                };
                children.push(CompoundChild {
                    local_position: Vec3::ZERO,
                    local_rotation: Quat::IDENTITY,
                    shape,
                    tag: if registered { tag } else { tag | UNREGISTERED },
                });
            }
        };
        for &i in &part.members {
            if attached.is_some_and(|a| !a.get(i).copied().unwrap_or(true)) {
                continue;
            }
            let c = &rig.components[i];
            let m = rig.part_local[i];
            let h = &c.havok;
            let own_rotor = drives.contains(&i);
            let mut weight = c.mass + h.phantom_mass;
            if own_rotor {
                weight -= h.rotor_mass;
            }
            // As with the Jolt bodies: a power cell sits inside the chassis (live,
            // the resting Scout touches the floor with its hubs and fork arm
            // only, `clist_rest`), and a non-physical fitting is no obstacle.
            // Every component with a rigid body that is not a phantom has a collidable in the
            // original: batteries, air tanks, battery packs and control boards too (their `.gmf`
            // rigid bodies are `nocoll=false`, and live they take contacts, docs/29 12at).
            let collides = internals_collide() || (c.role.is_physical() && c.role != Role::Power);
            let mesh_of = |pts: &Vec<Vec3>,
                           mesh: Option<&Option<Vec<[u32; 3]>>>,
                           m: Option<&f32>,
                           faces: Option<&Vec<[u32; 3]>>| {
                (
                    pts.clone(),
                    mesh.cloned().flatten(),
                    m.copied().unwrap_or(0.0),
                    None,
                    faces.cloned().filter(|f| !f.is_empty()),
                    None,
                )
            };
            let mut registered: Vec<bool> = Vec::new();
            let pieces: Vec<Piece> = if !collides {
                Vec::new()
            } else if !h.collision.is_empty() || !h.rotor_collision.is_empty() {
                registered.extend((0..h.collision.len()).map(|k| housing_registered(c, k)));
                let mut v: Vec<Piece> = h
                    .collision
                    .iter()
                    .enumerate()
                    .map(|(k, pts)| {
                        mesh_of(
                            pts,
                            h.collision_mesh.get(k),
                            h.collision_mass.get(k),
                            h.collision_faces.get(k),
                        )
                    })
                    .collect();
                if !own_rotor {
                    registered.extend(h.rotor_collision.iter().map(|_| true));
                    v.extend(h.rotor_collision.iter().enumerate().map(|(k, pts)| {
                        mesh_of(
                            pts,
                            h.rotor_collision_mesh.get(k),
                            h.rotor_piece_mass.get(k),
                            h.rotor_faces.get(k),
                        )
                    }));
                }
                v
            } else if !c.hull_points.is_empty() {
                vec![(c.hull_points.clone(), None, 0.0, None, None, None)]
            } else {
                Vec::new()
            };
            // A component with nothing to collide (a power cell, a control
            // board, a SmartZone) still has its mass where its geometry is:
            // live, the chassis bodies' centres of mass come out only so.
            let at = if pieces.is_empty() {
                let src = if h.mass_points.is_empty() {
                    &c.hull_points
                } else {
                    &h.mass_points
                };
                let pts: Vec<Vec3> = src.iter().map(|q| m.transform_point3(*q)).collect();
                massprops::hull_volume_centroid(&pts)
                    .map(|(_, at)| at)
                    .unwrap_or(m.w_axis.truncate())
            } else {
                m.w_axis.truncate()
            };
            let solid: Vec<(Vec<Vec3>, f32)>;
            let phantom_faces: &[Vec<[u32; 3]>] = if !pieces.is_empty() && collides {
                &h.phantom_faces
            } else {
                &[]
            };
            let phantoms: &[(Vec<Vec3>, f32)] = if pieces.is_empty() {
                // Nothing to collide (a power cell, a control board, a
                // SmartZone): its mass and inertia are still its geometry's.
                let src = if h.mass_points.is_empty() {
                    &c.hull_points
                } else {
                    &h.mass_points
                };
                solid = vec![(src.clone(), weight.max(0.0))];
                &solid
            } else if collides {
                &h.phantom_pieces
            } else {
                &[]
            };
            add(
                &pieces,
                &registered,
                phantoms,
                phantom_faces,
                m,
                weight.max(0.0),
                i as u32,
                at,
            );
            phantom.extend(h.phantom.iter().map(|q| m.transform_point3(*q)));
        }
        // The rotor of the motor this part hangs from.
        if let Some(info) = part.joint {
            let motor = &rig.components[info.motor];
            // A burst motor's rotor is built turned to its start angle.
            let m = frame.inverse() * motor.rotor_local;
            // A rotor's pieces; a `Steering` unit has two (`hub1`, `hub2`), one
            // per hub body: the one nearest this hinge's pivot is this part's.
            let all: Vec<Piece> = motor
                .havok
                .rotor_collision
                .iter()
                .enumerate()
                .map(|(k, pts)| {
                    (
                        pts.clone(),
                        motor.havok.rotor_collision_mesh.get(k).cloned().flatten(),
                        motor.havok.rotor_piece_mass.get(k).copied().unwrap_or(0.0),
                        motor.havok.rotor_faces.get(k).cloned(),
                        motor
                            .havok
                            .rotor_faces
                            .get(k)
                            .cloned()
                            .filter(|f| !f.is_empty()),
                        None,
                    )
                })
                .collect();
            let pivot = frame.inverse().transform_point3(info.pivot);
            let (rotor, rotor_mass) = if all.len() > 1 && motor.role == Role::Axle {
                let near = all
                    .iter()
                    .map(|(pts, _, _, _, _, _)| {
                        let pts: Vec<Vec3> = pts.iter().map(|q| m.transform_point3(*q)).collect();
                        massprops::hull_volume_centroid(&pts)
                            .map(|(_, c)| c.distance(pivot))
                            .unwrap_or(f32::MAX)
                    })
                    .enumerate()
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(k, _)| k)
                    .unwrap_or(0);
                let piece = all[near].clone();
                let mass = piece.2;
                (vec![piece], mass)
            } else {
                (all, motor.havok.rotor_mass)
            };
            add(
                &rotor,
                &[],
                &[],
                &[],
                m,
                rotor_mass,
                info.motor as u32,
                pivot,
            );
        }
        let shape = match children.len() {
            0 => ShapeDesc::Sphere { radius: 0.1 },
            _ => ShapeDesc::Compound(children),
        };
        let centre = (mass > 1e-6).then(|| moment / mass);
        // The body's inertia: every rigid body's own (its geometry's at its
        // mass) carried to the body's centre of mass and summed. Live, the
        // Scout's chassis reads (23.28, 39.08, 21.27) on the diagonal and
        // (-0.26, 1.26, 0.40) off it (`bodies-scout-alarm__live__000`, body
        // `+0x19c`); this gives (23.29, 39.07, 21.27), (-0.27, 1.28, 0.40).
        let inertia = centre.and_then(|c| {
            let m: f32 = bodies_inertia.iter().map(|b| b.0).sum();
            let total = bodies_inertia
                .iter()
                .fold(glam::Mat3::ZERO, |acc, (m, at, i)| {
                    acc + *i + crate::havok17::mass::parallel_axis(*m, *at - c)
                });
            if std::env::var_os("RA2_MASS_DEBUG").is_some() {
                eprintln!("inertia part {p}: {m:.2} kg {:?}", total.to_cols_array_2d());
            }
            // Scaled to the body's whole mass (bodies without geometry are
            // missing from the sum).
            (m > 0.0 && total.determinant() > 1e-9).then(|| total * (mass / m))
        });
        let shape = match centre {
            Some(c) => {
                let offset = c - crate::havok17::mass::of_shape(&shape).com;
                if offset.is_finite() && offset.length() > 1e-4 {
                    ShapeDesc::OffsetCenterOfMass {
                        shape: Box::new(shape),
                        offset,
                    }
                } else {
                    shape
                }
            }
            None => shape,
        };
        let root = &rig.components[part.root].havok;
        let (friction, restitution) = if p == 0 {
            (
                crate::havok17::DEFAULT_FRICTION,
                crate::havok17::DEFAULT_RESTITUTION,
            )
        } else {
            (
                root.friction.unwrap_or(STOCK_FRICTION),
                root.restitution.unwrap_or(STOCK_RESTITUTION),
            )
        };
        let phantom = (phantom.len() >= 4).then_some(ShapeDesc::ConvexHull { points: phantom });
        HavokPart {
            shape,
            points,
            inertia,
            mass: mass.max(0.2),
            friction,
            restitution,
            phantom,
        }
    }

    /// Diagnostic (docs/29 12at): every collision piece the Havok body of each part is built
    /// from, `(part, component tag, points in the part's frame, the triangles of a mesh piece)`.
    pub fn collider_pieces(rig: &RobotRig) -> Vec<ColliderPiece> {
        let mut out = Vec::new();
        for p in 0..rig.parts.len() {
            let mut shape = Self::havok_part(rig, p, None).shape;
            if let ShapeDesc::OffsetCenterOfMass { shape: inner, .. } = shape {
                shape = *inner;
            }
            let ShapeDesc::Compound(children) = shape else {
                continue;
            };
            for c in children {
                match c.shape {
                    ShapeDesc::MeshHull {
                        points, indices, ..
                    } => out.push((p, c.tag, points, Some(indices))),
                    ShapeDesc::ConvexHull { points } => out.push((p, c.tag, points, None)),
                    _ => {}
                }
            }
        }
        out
    }

    /// What each part weighed, as a share of its mass, the last time the
    /// original's build had the robot awake (docs/29 section 12x).
    ///
    /// The original builds a robot one component per frame, stepping the world
    /// in between. A component that brings a body of its own (a motor's rotor,
    /// a steering unit's hubs) wakes the robot; it falls asleep again some 14
    /// substeps later, and from then on the components it carries only add
    /// their mass to it (a wheel onto its motor's rotor). Havok adds gravity to
    /// a body's force accumulator with its mass of the moment, as an action
    /// after the integration (live, the placement's first substep moves
    /// REVENGE's 27 kg wheels by `-0.2 · 10 / 27` m/s, the Scout's fork arm by
    /// `-0.2 · 12 / 26`, ALARM's piston rod by `-0.2 · 8 / 33`:
    /// `spinup-*__live__000`), so a body that grew after the robot's last wake
    /// keeps the weight of its smaller self until it is next integrated: the
    /// first substep after the placement. Its share is the mass of its members
    /// that come no later in the `.bot` than the robot's last wake: the last
    /// body-bringing component, or the build's last late wake
    /// ([`RobotRig::late_wakes`], docs/29 section 12z), which comes as the
    /// component that causes it first appears and so counts the members before
    /// it (live, MiniBerg's battery wakes it with its top at 28 of 28 kg,
    /// Iceberg's Ice Pick with its top at 22 of 38).
    /// `RA2_NO_LATE_WAKES=1` leaves the late wakes out.
    fn build_shares(rig: &RobotRig, mass_points: &[Vec<(usize, f32, Vec3)>]) -> Vec<f32> {
        let late = if std::env::var_os("RA2_NO_LATE_WAKES").is_some() {
            Vec::new()
        } else {
            rig.late_wakes()
        };
        let last = rig
            .parts
            .iter()
            .filter_map(|q| q.joint.map(|j| j.motor))
            .chain(late.iter().map(|&k| k - 1))
            .max();
        (0..rig.parts.len())
            .map(|p| {
                let (Some(last), Some(pts)) = (last, mass_points.get(p)) else {
                    return 1.0;
                };
                if p == 0 {
                    return 1.0;
                }
                let total: f32 = pts.iter().map(|(_, m, _)| m).sum();
                let then: f32 = pts
                    .iter()
                    .filter(|(i, _, _)| *i <= last)
                    .map(|(_, m, _)| m)
                    .sum();
                if total > 0.0 {
                    then / total
                } else {
                    1.0
                }
            })
            .collect()
    }

    /// The robot's wheel records (`AddWheel`, `FUN_00485490` from
    /// `FUN_004dc180`): every wheel on a `SpinMotor` or an `AxleMount`, with its
    /// hub centre and axle in the chassis frame, `grip` and `resistance`.
    fn magic_records(
        physics: &dyn PhysicsBackend,
        rig: &RobotRig,
        parts: &[PartState],
        mass_points: &[Vec<(usize, f32, Vec3)>],
        rotation: Quat,
    ) -> MagicAction {
        let chassis = parts[0].body;
        let centre = physics.center_of_mass(chassis).unwrap_or(Vec3::ZERO);
        // `AddWheel` stores its offset while the robot is still being built
        // (`0x4dc180`, once per wheel as its component is added): the wheel's
        // own mass point (its rigid bodies, not the motor's rotor it rides)
        // less the chassis body's centre of mass at that moment, i.e. of the
        // chassis members that come before the wheel in the `.bot`. Live
        // (FlapJack, `magic-flapjack__live__000`): (0.465, -0.152, -0.405) and
        // (-0.450, -0.145, -0.394); ours reproduces them to 1 cm.
        let world_point = |p: usize, local: Vec3| {
            physics
                .state(parts[p].body)
                .map(|s| s.position + s.rotation * local)
        };
        let partial = |before: usize| -> Option<Vec3> {
            let pts = mass_points.first()?;
            let (w, at) = pts
                .iter()
                .filter(|(i, _, _)| *i < before)
                .fold((0.0f32, Vec3::ZERO), |(w, at), (_, m, c)| {
                    (w + m, at + *c * *m)
                });
            (w > 0.0).then(|| at / w).and_then(|c| world_point(0, c))
        };
        let mut wheels = Vec::new();
        for (p, part) in rig.parts.iter().enumerate() {
            let Some(info) = part.joint else { continue };
            let root = &rig.components[part.root];
            if root.role != Role::Wheel || info.slider {
                continue;
            }
            let class = rig.components[info.motor].class.to_ascii_lowercase();
            let driven = class == "spinmotor";
            if !driven && class != "axlemount" && class != "steering" {
                continue;
            }
            let stats = root.wheel.clone().unwrap_or_default();
            let own = mass_points
                .get(p)
                .and_then(|pts| pts.iter().find(|(i, _, _)| *i == part.root))
                .and_then(|(_, _, c)| world_point(p, *c));
            let (hub, centre) = match (own, partial(part.root)) {
                (Some(hub), Some(at)) => (hub, at),
                _ => (
                    physics.center_of_mass(parts[p].body).unwrap_or(centre),
                    centre,
                ),
            };
            let host = rig.components[rig.parts[0].root].local;
            // The record's axle is the hinge axis turned by the motor's own
            // sense, `CLOCKWISE` (the motor step drives at `-direction`,
            // `0x481450`): live, the Razor's right-hand wheels read
            // `(-1, 0, 0)` where their hinge axis is `+X`
            // (`magic-razor__live__000`), and with it a positive `Forward`
            // drives the chassis toward its front.
            let axle =
                (host.inverse().transform_vector3(info.axis) * CLOCKWISE).normalize_or_zero();
            // A `Steering` unit's hubs (`0x4dc912..0x4dca24`): their records get
            // the unit's thrust, and their axle and angle from the hub every
            // update. The unit steers about its own up, its mount point's `-Z`
            // (every chassis child's mount `Z` is the chassis' down). Both
            // hubs' axles are the unit's hub line, the model's `+Z` from `hub1`
            // to `hub2`, not each mount point's own axis: live, Sentinel's two
            // steering records read `+X` where its spin motors' read `+X` and
            // `-X` (`magic-sentinel__live__000`).
            let steer = (class == "steering").then(|| {
                let unit = &rig.components[info.motor];
                let mount_z = unit
                    .attachments
                    .get(&unit.own_mount)
                    .map(|a| Vec3::from(a.transform[2]))
                    .filter(|z| *z != Vec3::ZERO)
                    .unwrap_or(Vec3::Z);
                let up = (-unit.local.transform_vector3(mount_z)).normalize_or(Vec3::Y);
                let line = unit.local.transform_vector3(Vec3::Z);
                let axle = host.inverse().transform_vector3(line).normalize_or(axle);
                let frame_a = rig
                    .parts
                    .get(rig.parts[p].parent.unwrap_or(0))
                    .map(|q| rig.components[q.root].local)
                    .unwrap_or(host);
                Steer {
                    unit: info.motor,
                    power: unit.steer_power.unwrap_or(1.0),
                    axis: host.inverse().transform_vector3(up).normalize_or(Vec3::Y),
                    hinge_axis: frame_a
                        .inverse()
                        .transform_vector3(up)
                        .normalize_or(Vec3::Y),
                    rest_axle: axle,
                    angle: 0.0,
                    target: 0.0,
                }
            });
            wheels.push(WheelRecord {
                part: p,
                motor: driven.then_some(info.motor),
                steer,
                point: rotation.inverse() * (hub - centre),
                axle,
                grip: stats.grip,
                resistance: stats.resistance,
                thrust: 0.0,
                touching: false,
                air: crate::drive::TOUCH_MEMORY * 2.0,
                normal: Vec3::Y,
                // The wheel's constructor (`0x507a77`).
                contact_normal: Vec3::Y,
            });
        }
        MagicAction { wheels }
    }

    /// Drive through the game's own model: the wheels' records get their
    /// thrust and their touching flag, a wheel in the air spins on its hinge
    /// servo, and the [`MagicAction`] pushes the chassis.
    fn drive_magic(&mut self, physics: &mut dyn PhysicsBackend, elec_scale: f32) {
        let Some(mut magic) = self.magic.take() else {
            return;
        };
        let dt = physics.last_dt();
        let own: Vec<BodyHandle> = self.parts.iter().map(|q| q.body).collect();
        for w in &mut magic.wheels {
            let alive = self.parts[w.part].alive && self.attached[self.rig.parts[w.part].root];
            // What the wheel's phantom touches; a backend without phantoms (Jolt)
            // looks for ground just under the hub instead.
            let touch = if !alive {
                None
            } else {
                physics.phantom_touch(self.parts[w.part].body).or_else(|| {
                    if physics.name() == "havok17" {
                        return None;
                    }
                    let info = self.rig.parts[w.part].joint?;
                    let wheel = info.wheel?;
                    let hub = physics.center_of_mass(self.parts[w.part].body)?;
                    let reach = wheel.radius + WHEEL_REACH;
                    physics
                        .ray_cast(hub - Vec3::Y * reach, Vec3::Y, reach)
                        .filter(|hit| !own.contains(&hit.body))
                        .map(|hit| hit.normal * hit.normal.y.signum())
                })
            };
            MagicAction::touch(w, touch.is_some(), dt);
            if !alive {
                w.touching = false;
            }
            if let Some(mut steer) = w.steer {
                // The unit's tick (`0x4f6850`): `+0x2d0 = ftol(f(ftol(command ·
                // g)))`; its thrust `ftol(+0x2d0 · power · 0.7)` (`0x4f6700`),
                // doubled into the record like a spin motor's. The hub turns
                // toward the steer command's angle (`0x508920` -> `0x5089d0`)
                // and the record takes its axle (`0x486860`) and angle
                // (`0x486890`).
                let command = self
                    .control
                    .command(&self.rig.wiring, steer.unit)
                    .unwrap_or(0.0);
                let percent = self.scaled_percent(steer.unit, command, elec_scale);
                w.thrust = MagicAction::thrust(steer.power, percent);
                steer.target = Steer::target_for(
                    self.control
                        .steer_command(&self.rig.wiring, steer.unit)
                        .unwrap_or(0.0),
                );
                steer.turn(dt);
                w.axle = steer.axle();
                if let Some(joint) = self.parts[w.part].joint {
                    physics.steer_hinge(joint, steer.hinge_axis, steer.angle);
                }
                w.steer = Some(steer);
                continue;
            }
            let Some(motor) = w.motor else {
                w.thrust = 0.0;
                continue;
            };
            let Some(info) = self.rig.parts[w.part].joint else {
                continue;
            };
            let command = self.control.command(&self.rig.wiring, motor).unwrap_or(0.0);
            // `SetPercent(ftol(command · g))` (`0x4f59f0`) stores
            // `motor + 0x29c = ftol(f(percent))` (`0x4f5800`); the thrust takes
            // the hinge controller's *raw* torque (`+0xb4`, `0x4d33b0`), which
            // neither `g` nor `f` touches: live, the Razor's records fall
            // 1940, 1920, 1900, 1880, 1862 (= 2 · trunc(0.7 · 14 · {99..95}))
            // as its undersized battery's scale drops (`magic-razor__live__000`).
            let percent = self.scaled_percent(motor, command, elec_scale);
            let torque = info.max_torque / TORQUE_UNIT;
            w.thrust = MagicAction::thrust(torque, percent);
            if let Some(joint) = self.parts[w.part].joint {
                if w.touching || percent == 0 {
                    // `SetPercent` (`0x4f5800`): the hinge motor is off while the
                    // wheel is down.
                    physics.drive_hinge(joint, 0.0);
                } else {
                    // In the air the hinge controller spins it (`0x481450`):
                    // `trunc(p · 0.01 · speed)` rad/s at the load-capped
                    // torque ([`motor`]); `p` already carries `f` and `g`.
                    let ctl = self.hinge[w.part];
                    if (ctl.max_torque - self.motor_torque[w.part]).abs() > 1e-3 {
                        self.motor_torque[w.part] = ctl.max_torque;
                        physics.set_joint_strength(joint, ctl.max_torque, 0.0);
                    }
                    physics.drive_hinge(
                        joint,
                        hinge_target(ctl.velocity(percent), percent)
                            * CLOCKWISE
                            * self.air_sign[w.part],
                    );
                }
            }
        }
        let chassis = self.parts[0].body;
        if self.parts[0].alive {
            // A backend that runs actions per piece takes the records and applies them after every
            // piece; the others get the action applied here, once a frame.
            if !physics.set_piece_action(chassis, Some(magic.clone())) {
                magic.apply(physics, chassis);
            }
        } else {
            physics.set_piece_action(chassis, None);
        }
        self.magic = Some(magic);
    }

    /// The compound shape of a body made of `members`.
    fn part_shape(rig: &RobotRig, members: &[usize]) -> ShapeDesc {
        let children: Vec<CompoundChild> = members
            .iter()
            .filter(|&&i| {
                let c = &rig.components[i];
                // A fitting the definitions call non-physical (a control board)
                // is not an obstacle in the original's single robot body, and a
                // power cell sits inside the chassis: neither adds a hull.
                !c.hull_points.is_empty() && c.role.is_physical() && c.role != Role::Power
            })
            .map(|&i| {
                let m = rig.part_local[i];
                let (_, local_rotation, local_position) = m.to_scale_rotation_translation();
                CompoundChild {
                    local_position,
                    local_rotation,
                    shape: ShapeDesc::ConvexHull {
                        points: massprops::skinned(
                            &if rig.components[i].role == Role::Wheel {
                                round_wheel(&rig.components[i].hull_points)
                            } else {
                                rig.components[i].hull_points.clone()
                            },
                            hull_skin(),
                        ),
                    },
                    tag: i as u32,
                }
            })
            .collect();
        let offset = Self::center_offset(rig, members);
        let shape = match children.len() {
            0 => return ShapeDesc::Sphere { radius: 0.1 },
            1 => children
                .into_iter()
                .next()
                .map(|c| c.shape)
                .unwrap_or(ShapeDesc::Sphere { radius: 0.1 }),
            _ => ShapeDesc::Compound(children),
        };
        match offset {
            Some(offset) => ShapeDesc::OffsetCenterOfMass {
                shape: Box::new(shape),
                offset,
            },
            None => shape,
        }
    }

    /// The components' mass-weighted centre of one body, in the body's own
    /// frame (`None` for a body with no mass or geometry).
    fn mass_center(rig: &RobotRig, members: &[usize]) -> Option<Vec3> {
        let (mut mass, mut moment) = (0.0f32, Vec3::ZERO);
        for &i in members {
            let m = rig.components[i].mass;
            if m <= 0.0 {
                continue;
            }
            let m_local = rig.part_local[i];
            let pts: Vec<Vec3> = rig.components[i]
                .hull_points
                .iter()
                .map(|p| m_local.transform_point3(*p))
                .collect();
            let at = massprops::hull_volume_centroid(&pts)
                .map(|(_, at)| at)
                .unwrap_or_else(|| m_local.w_axis.truncate());
            mass += m;
            moment += m * at;
        }
        (mass > 1e-6).then(|| moment / mass)
    }

    /// How far to move a body's centre of mass from where the backend puts it
    /// (the collision hulls' volume-weighted centre) to the components'
    /// mass-weighted centre, which is what the original's per-component rigid
    /// bodies integrate to. `None` when there is nothing to correct.
    fn center_offset(rig: &RobotRig, members: &[usize]) -> Option<Vec3> {
        let (mut vol, mut vol_moment) = (0.0f32, Vec3::ZERO);
        for &i in members {
            let c = &rig.components[i];
            if c.hull_points.is_empty() || !c.role.is_physical() || c.role == Role::Power {
                continue;
            }
            let m_local = rig.part_local[i];
            let pts: Vec<Vec3> = c
                .hull_points
                .iter()
                .map(|p| m_local.transform_point3(*p))
                .collect();
            if let Some((v, at)) = massprops::hull_volume_centroid(&pts) {
                vol += v;
                vol_moment += v * at;
            }
        }
        let mass_centre = Self::mass_center(rig, members)?;
        if vol <= 1e-9 {
            return None;
        }
        let offset = mass_centre - vol_moment / vol;
        (offset.is_finite() && offset.length() > 1e-4).then_some(offset)
    }

    /// Friction of a body: a wheel gets the `grip` its definition declares for the
    /// `contact` surface it rolls on, everything else the stock component value.
    fn part_friction(rig: &RobotRig, members: impl Iterator<Item = usize>) -> f32 {
        let members: Vec<usize> = members.collect();
        members
            .iter()
            .filter_map(|&i| rig.components[i].wheel.as_ref())
            .map(|w| w.grip)
            .next()
            .unwrap_or(STOCK_FRICTION)
    }

    /// Build the hinge for part `p` on `parent`.
    fn make_hinge(
        physics: &mut dyn PhysicsBackend,
        rig: &RobotRig,
        parts: &[PartState],
        parent: usize,
        p: usize,
        info: JointInfo,
        child_pivot: Vec3,
    ) -> Result<JointHandle, BackendError> {
        // The joint's pivot and axis are the parent's mount point, in bot space;
        // both bodies express them in their own space (they coincide in the rest
        // pose, which is where the hinge is created).
        let (pivot_bot, axis_bot) = (info.anchor, info.axis);
        let frame_a = rig.components[rig.parts[parent].root].local;
        let frame_b = rig.components[rig.parts[p].root].local;

        let max_torque = match info.use_kind {
            JointUse::Passive => 0.0,
            JointUse::Wheel => info.max_torque * motor_passes(),
            _ => info.max_torque,
        };
        let desc = HingeDesc {
            body_a: parts[parent].body,
            body_b: parts[p].body,
            anchor_a: frame_a.inverse().transform_point3(pivot_bot),
            anchor_b: frame_b.inverse().transform_point3(child_pivot),
            axis_a: frame_a.inverse().transform_vector3(axis_bot),
            axis_b: frame_b.inverse().transform_vector3(axis_bot),
            limits: info.limits,
            max_torque,
            friction_torque: info.friction_torque,
            suspension: Self::steering_up(rig, &info)
                .map(|up| frame_a.inverse().transform_vector3(up).normalize_or_zero())
                .filter(|_| std::env::var_os("RA2_STEER_HINGE").is_none()),
        };
        physics.create_hinge(&desc)
    }

    /// A `Steering` unit's up, its mount point's `-Z`, in bot space (`None`
    /// for any other motor): its wheel constraints' steering and suspension
    /// axis (live, Sentinel's wheel atoms read `(0, 1, 0)` for both in the
    /// chassis frame, `0x553c30` `+0x98` and `+0xc8`).
    fn steering_up(rig: &RobotRig, info: &JointInfo) -> Option<Vec3> {
        let unit = &rig.components[info.motor];
        if !unit.class.eq_ignore_ascii_case("steering") {
            return None;
        }
        let mount_z = unit
            .attachments
            .get(&unit.own_mount)
            .map(|a| Vec3::from(a.transform[2]))
            .filter(|z| *z != Vec3::ZERO)
            .unwrap_or(Vec3::Z);
        Some((-unit.local.transform_vector3(mount_z)).normalize_or(Vec3::Y))
    }

    /// The same placement rule as a hinge, on a prismatic joint (F8): a piston's
    /// rod slides along the axis its mount point gives, with the travel its
    /// definition asks for. The motor's "torque" is a force here.
    fn make_slider(
        physics: &mut dyn PhysicsBackend,
        rig: &RobotRig,
        parts: &[PartState],
        parent: usize,
        p: usize,
        info: JointInfo,
        child_pivot: Vec3,
    ) -> Result<JointHandle, BackendError> {
        let (pivot_bot, axis_bot) = (info.pivot, info.axis);
        let frame_a = rig.components[rig.parts[parent].root].local;
        let frame_b = rig.components[rig.parts[p].root].local;

        let max_force = match info.use_kind {
            JointUse::Passive => 0.0,
            _ => info.max_torque,
        };
        let desc = SliderDesc {
            body_a: parts[parent].body,
            body_b: parts[p].body,
            anchor_a: frame_a.inverse().transform_point3(pivot_bot),
            anchor_b: frame_b.inverse().transform_point3(child_pivot),
            axis_a: frame_a.inverse().transform_vector3(axis_bot),
            axis_b: frame_b.inverse().transform_vector3(axis_bot),
            limits: info.limits,
            max_force,
            friction_force: info.friction_torque,
        };
        physics.create_slider(&desc)
    }

    /// The bot's world frame (derived from the chassis body).
    pub fn transform(&self, physics: &dyn PhysicsBackend) -> Mat4 {
        let root = self.rig.components[self.rig.parts[0].root].local;
        self.part_world(physics, 0) * root.inverse()
    }

    /// World transform of one assembled body.
    pub fn part_world_of(&self, physics: &dyn PhysicsBackend, part: usize) -> Mat4 {
        self.part_world(physics, part)
    }

    fn part_world(&self, physics: &dyn PhysicsBackend, part: usize) -> Mat4 {
        match physics.state(self.parts[part].body) {
            Some(s) => Mat4::from_rotation_translation(s.rotation, s.position),
            None => Mat4::IDENTITY,
        }
    }

    /// World transform of every component, in component index order. A body that
    /// has been shot off keeps its last-known frame.
    pub fn component_worlds(&self, physics: &dyn PhysicsBackend) -> Vec<Mat4> {
        let part_worlds: Vec<Mat4> = (0..self.rig.parts.len())
            .map(|p| self.part_world(physics, p))
            .collect();
        self.rig
            .part_of
            .iter()
            .enumerate()
            .map(|(i, &p)| part_worlds[p] * self.rig.part_local[i])
            .collect()
    }

    pub fn position(&self, physics: &dyn PhysicsBackend) -> Vec3 {
        self.transform(physics).w_axis.truncate()
    }

    pub fn velocity(&self, physics: &dyn PhysicsBackend) -> Vec3 {
        physics
            .state(self.parts[0].body)
            .map(|s| s.linear_velocity)
            .unwrap_or(Vec3::ZERO)
    }

    /// The chassis body — the one the fight revolves around.
    pub fn body(&self) -> BodyHandle {
        self.parts[0].body
    }

    /// Whether the robot can still fight.
    ///
    /// An eliminated robot (state 3) has lost; so has one whose chassis is
    /// gone. `plus.isDefeated` is the original's "state != 0", so both report
    /// defeated, while `plus.isEliminated` only reports the eliminated state.
    pub fn is_alive(&self) -> bool {
        self.parts[0].alive && !self.destroyed && !self.immobilized && !self.eliminated
    }

    /// Feed the control board: the keys held now, and a stand-in driver's
    /// `throttle`/`steer` (held as the `Forward`/`LeftRight` keys).
    fn read_input(&mut self, input: DriveInput) {
        self.control.set_keys(&self.rig.wiring, input.keys);
        self.control
            .set_drive(&self.rig.wiring, input.throttle, input.steer);
    }

    /// Whether the components tick ([`FRAME`]) at the next drive step.
    pub fn tick_due(&self) -> bool {
        self.frame_clock >= FRAME - 1e-6
    }

    /// Tick the components on the coming drive steps as a live capture did
    /// (diagnostics replaying the original's irregular frames): one entry per
    /// drive step, the tick's time or none; after the schedule the frame
    /// clock takes over again.
    pub fn set_tick_schedule(&mut self, ticks: impl IntoIterator<Item = Option<f32>>) {
        self.tick_schedule = ticks.into_iter().collect();
    }

    /// The AI's `Input(name, slot, value)` calls, in order.
    pub fn apply_inputs(&mut self, events: &[(String, i32, f32)]) {
        for (name, slot, value) in events {
            self.control.input(&self.rig.wiring, name, *slot, *value);
        }
    }

    /// Drive the motors through the control board.
    ///
    /// Every motor, wheel and piston is moved by its **wiring** (see
    /// [`crate::controls`]): the command is the clamped sum, in `-100..=100`, of
    /// what the held keys and the AI's inputs contribute through the bot's wires.
    /// A wheel or spinner then turns at `command / 100` of its motor's rated
    /// speed (the original's motor step `0x481450`); a zero command releases the
    /// motor (a servo holds). A component nothing is wired to is never driven.
    /// A burst device waits cocked until its `Fire` wire is pressed.
    ///
    /// A skid-steered robot turns because the `LeftRight` wires drive its two
    /// sides differently; a `Steering` unit turns its hubs (see
    /// [`crate::drive::Steer`]). `DriveInput::throttle`
    /// and `steer` are the `Forward`/`LeftRight` keys of a stand-in driver.
    pub fn drive(
        &mut self,
        physics: &mut dyn PhysicsBackend,
        input: DriveInput,
        _tuning: &DriveTuning,
    ) {
        self.read_input(input);
        let mut samples: Vec<WheelSample> = Vec::new();
        let magic_wheels: Vec<usize> = self
            .magic
            .as_ref()
            .map(|m| m.wheels.iter().map(|w| w.part).collect())
            .unwrap_or_default();

        // The power step (`0x4dc3f0`): what the motors may use this step. A robot
        // with no source on a channel (a hand-built test rig) is not throttled.
        let dt = physics.last_dt();
        // The components' own update (`0x448650`, `0x446670`) runs once a
        // frame, between the frame's two Havok substeps, with the frame's
        // time; see [`Robot::drive_burst`].
        let tick = if let Some(t) = self.tick_schedule.pop_front() {
            t
        } else {
            let t = if self.frame_clock >= FRAME - 1e-6 {
                self.frame_clock -= FRAME;
                Some(FRAME)
            } else {
                None
            };
            self.frame_clock += dt;
            t
        };
        self.flame_inputs();
        let (elec_demand, air_demand) = self.power_demand(dt);
        let budgets = self
            .energy
            .step(dt, elec_demand, air_demand, &self.health, &self.attached);
        // The published scales; a Flamethrower lights only while both are above
        // zero, so a channel the robot has no source on counts as 0 here.
        self.flame_update(dt, budgets.air.scale, budgets.elec.scale);
        // The original publishes scale 0 on a step with no draw too; nothing is
        // driven then, but a servo still has to hold, so the scale only applies
        // to a step that asks for power.
        let elec_scale = if self.energy.has(Channel::Elec) && elec_demand > 0.0 {
            budgets.elec.scale
        } else {
            1.0
        };
        let air_scale = if self.energy.has(Channel::Air) && air_demand > 0.0 {
            budgets.air.scale
        } else {
            1.0
        };

        for p in 0..self.rig.parts.len() {
            if !self.parts[p].alive {
                continue;
            }
            let (Some(info), Some(joint)) = (self.rig.parts[p].joint, self.parts[p].joint) else {
                continue;
            };
            if magic_wheels.contains(&p) {
                continue;
            }
            match info.use_kind {
                JointUse::Passive => {}
                JointUse::Burst => {
                    let scale = if info.slider { air_scale } else { elec_scale };
                    self.drive_burst(physics, p, info, joint, scale, tick)
                }
                JointUse::Wheel if info.wheel.is_some() && !legacy_wheels() => {
                    let command = self
                        .control
                        .command(&self.rig.wiring, info.motor)
                        .unwrap_or(0.0);
                    let power = self.motor_factor(info.motor) * elec_scale;
                    if let Some(sample) =
                        self.sample_wheel(physics, p, info, joint, command / FULL * power)
                    {
                        samples.push(sample);
                    }
                }
                JointUse::Spun | JointUse::Wheel | JointUse::Piston if info.slider => {
                    let command = self
                        .control
                        .command(&self.rig.wiring, info.motor)
                        .unwrap_or(0.0);
                    // The prismatic controller's direction is the integer
                    // percentage, as a hinge's (`+0x80`).
                    let dir = self.scaled_percent(info.motor, command, air_scale) as f32;
                    self.slider_dir[p] = dir;
                    let stops = (info.limits[0], info.limits[1]);
                    self.drive_prismatic(physics, p, info, joint, dir, stops, true, false);
                }
                JointUse::Spun | JointUse::Wheel | JointUse::Piston => {
                    let command = self
                        .control
                        .command(&self.rig.wiring, info.motor)
                        .unwrap_or(0.0);
                    let k = if info.use_kind == JointUse::Wheel {
                        motor_passes()
                    } else {
                        1.0
                    };
                    // The hinge controller (`0x481450`, [`motor`]): velocity
                    // `trunc(p · 0.01 · speed)` at the integer percentage `p`
                    // (`ftol(f(ftol(command · g)))`, the health factor and the
                    // power budget's scale), the load-capped maximum torque,
                    // which neither scales.
                    let percent = self.scaled_percent(info.motor, command, elec_scale);
                    let torque = self.hinge[p].max_torque * k;
                    if (torque - self.motor_torque[p]).abs() > 1e-3 {
                        self.motor_torque[p] = torque;
                        physics.set_joint_strength(joint, torque, info.friction_torque);
                    }
                    let target = hinge_target(self.hinge[p].velocity(percent), percent);
                    physics.drive_hinge(joint, target * CLOCKWISE * k);
                }
            }
        }
        self.push_wheels(physics, &samples);
        self.drive_magic(physics, elec_scale);
        self.control.end_step();
    }

    /// A motor's integer percentage as the original stores it:
    /// `ftol(f(ftol(command · g)))`, `g` the power budget's scale and `f` the
    /// component's health factor ([`Robot::motor_factor`]).
    fn scaled_percent(&self, motor: usize, command: f32, scale: f32) -> i32 {
        let asked = (command * scale).trunc();
        (asked * self.motor_factor(motor)).trunc() as i32
    }

    /// The integer command a motor holds (`SetPercent`, `0x4f59f0`): what the original's
    /// snapshot of a motor carries as its command (docs/38 section 16).
    pub fn motor_percent(&self, motor: usize) -> i32 {
        let command = self.control.command(&self.rig.wiring, motor).unwrap_or(0.0);
        self.scaled_percent(motor, command, self.energy.last.elec.scale)
    }

    /// The hub angle of the `k`-th wheel (part order) of a `Steering` unit (`+0x68`), 0 when the
    /// unit has not steered.
    pub fn steering_angle(&self, unit: usize, k: usize) -> f32 {
        let Some(magic) = &self.magic else { return 0.0 };
        let parts: Vec<usize> = self
            .rig
            .parts
            .iter()
            .enumerate()
            .filter(|(_, p)| p.joint.as_ref().is_some_and(|j| j.motor == unit))
            .map(|(i, _)| i)
            .collect();
        let Some(part) = parts.get(k) else { return 0.0 };
        magic
            .wheels
            .iter()
            .find(|w| w.part == *part)
            .and_then(|w| w.steer)
            .map_or(0.0, |s| s.angle)
    }

    /// The control board's calls on the Flamethrowers: the `Fire` wires' command
    /// to the analog handler (when it changed), a `Fire burst` press to the
    /// button handler. They land before the power step, which then sees the jet.
    fn flame_inputs(&mut self) {
        // The scales the last power step published (`*0x714a6c`, `*0x714a70`).
        let powered = self.energy.last.air.scale > 0.0 && self.energy.last.elec.scale > 0.0;
        for k in 0..self.flames.len() {
            let component = self.flames[k].component;
            let disabled = !self.attached[component];
            let mut sounds = Vec::new();
            if let Some(v) = self.control.flame_level(&self.rig.wiring, component) {
                let command = v.trunc() as i32;
                if command != self.flames[k].command {
                    self.flames[k].command = command;
                    self.flames[k].state.fire(
                        command != 0,
                        command,
                        disabled,
                        powered,
                        &mut sounds,
                    );
                }
            }
            if self.control.burst_pressed(&self.rig.wiring, component) {
                self.flames[k]
                    .state
                    .press_burst(100, disabled, powered, &mut sounds);
            }
            for s in sounds {
                if self.flame_sounds.len() < 256 {
                    self.flame_sounds.push((component, s));
                }
            }
        }
    }

    /// The Flamethrowers' own step (`0x504790`), after the power step has
    /// published its scales.
    fn flame_update(&mut self, dt: f32, air: f32, elec: f32) {
        for k in 0..self.flames.len() {
            let component = self.flames[k].component;
            let disabled = !self.attached[component];
            let mut sounds = Vec::new();
            self.flames[k]
                .state
                .update(dt, disabled, air, elec, &mut sounds);
            for s in sounds {
                if self.flame_sounds.len() < 256 {
                    self.flame_sounds.push((component, s));
                }
            }
        }
    }

    /// The jets of this robot's Flamethrowers, in the world (`0x5048e0`): the
    /// nozzle `(0, 0.176, -0.9)` of the component's frame, flying along its
    /// `(0, 0, -5)` times the speed scale, plus the chassis' own velocity.
    pub fn flame_jets(&self, physics: &dyn PhysicsBackend) -> Vec<FlameJet> {
        if self.flames.is_empty() {
            return Vec::new();
        }
        let worlds = self.component_worlds(physics);
        let carried = self.velocity(physics);
        self.flames
            .iter()
            .map(|f| {
                let w = worlds[f.component];
                FlameJet {
                    robot_component: f.component,
                    on: f.state.emitting,
                    position: w.transform_point3(crate::flame::NOZZLE_LOCAL),
                    velocity: w.transform_vector3(crate::flame::JET_LOCAL * f.state.scale)
                        + carried,
                }
            })
            .collect()
    }

    /// The flame sound calls since the last call: `(component, call)`.
    pub fn take_flame_sounds(&mut self) -> Vec<(usize, crate::flame::Sound)> {
        std::mem::take(&mut self.flame_sounds)
    }

    /// Each burst device's cycle, for tracing: `(part, slider, phase)` with the
    /// phase 0 cocked, 1 firing, 2 cocking (`fidtrace`'s `FIDTRACE_FIRE`).
    pub fn burst_phases(&self) -> Vec<(usize, bool, u8)> {
        (0..self.rig.parts.len())
            .filter_map(|p| {
                let info = self.rig.parts[p].joint?;
                (info.use_kind == JointUse::Burst).then(|| {
                    (
                        p,
                        info.slider,
                        match self.burst[p] {
                            BurstPhase::Cocked => 0,
                            BurstPhase::Firing => 1,
                            BurstPhase::Cocking => 2,
                        },
                    )
                })
            })
            .collect()
    }

    /// The weapon triggers (motor component indices) since the last call.
    pub fn take_fire_events(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.fire_events)
    }

    /// How much of its rated speed and torque a motor still delivers.
    ///
    /// `FUN_0045fa90`: `f(x) = x * (0.5 + 0.5 * hp / max)`, and `0.5 * x` once the
    /// motor's hitpoints are below zero — a hurt motor is weaker, down to half.
    /// The original feeds `f` both the speed and the torque of
    /// `SetPowerSettings` (the `SpinMotor` apply at `0x4468bc`), after a global
    /// factor `g` from the robot's power budget (`0x45f130`, set at `0x4dc76c`
    /// from `min(1, robot+0x9c / robot+0xa0)`): that is the power budget's
    /// scale ([`crate::energy`]), which the callers multiply in.
    pub fn motor_factor(&self, motor: usize) -> f32 {
        let hp = self.health.get(motor).copied().unwrap_or(0.0);
        let max = self.rig.components[motor].hitpoints.max(1.0);
        if hp < 0.0 {
            0.5
        } else {
            0.5 + 0.5 * (hp / max).min(1.0)
        }
    }

    /// Sample one wheel: is it on the ground, and which way would it push?
    ///
    /// The hinge is left free, so the wheel rolls at the speed the ground gives
    /// it; the push itself is applied by [`Robot::push_wheels`], which needs every
    /// wheel's sample at once. `None` for a wheel that has not touched ground for
    /// [`WHEEL_GROUND_MEMORY`].
    fn sample_wheel(
        &mut self,
        physics: &mut dyn PhysicsBackend,
        p: usize,
        info: JointInfo,
        joint: JointHandle,
        command: f32,
    ) -> Option<WheelSample> {
        let wheel = info.wheel?;
        let parent = self.rig.parts[p].parent?;
        physics.drive_hinge(joint, 0.0);
        let dt = physics.last_dt();

        // The pivot and axis are in bot space at the rest pose; the body the
        // motor sits on carries them (a wheel on an arm moves with the arm).
        let host = self.rig.components[self.rig.parts[parent].root].local;
        let to_world = self.part_world(physics, parent) * host.inverse();
        let centre = to_world.transform_point3(info.pivot);
        let axle = to_world.transform_vector3(info.axis).normalize_or_zero();
        if axle == Vec3::ZERO {
            return None;
        }

        // Cast *up* from the lowest the ground may be and still count: a mesh is
        // hit from either side, so this finds the floor whether the wheel hovers
        // above it or has sunk a little into it, and anything of the robot's own
        // in the way (the wheel itself, once the floor is out of reach) is not
        // ground.
        let own: Vec<BodyHandle> = self.parts.iter().map(|q| q.body).collect();
        let reach = wheel.radius + hull_skin() + WHEEL_REACH;
        let found = ray_past_own(
            physics,
            centre - Vec3::Y * reach,
            Vec3::Y,
            reach,
            &own,
            self.parts[p].body,
        );
        match found {
            Some(hit) => {
                self.wheel_air[p] = 0.0;
                self.wheel_normal[p] = if hit.normal.y.abs() > 0.3 {
                    hit.normal * hit.normal.y.signum()
                } else {
                    Vec3::Y
                };
            }
            None => self.wheel_air[p] += dt,
        }
        if self.wheel_air[p] > WHEEL_GROUND_MEMORY {
            return None;
        }
        let normal = self.wheel_normal[p];
        // A positive command pushes the robot along `axle x normal`, the same
        // way the game's own drive model does (`MagicAction`'s
        // `-(record axle x normal)` with the record axle `CLOCKWISE · axle`).
        let along = axle.cross(normal).normalize_or_zero();
        let side = (axle - normal * axle.dot(normal)).normalize_or_zero();
        if along == Vec3::ZERO {
            return None;
        }
        let velocity = physics
            .state(self.parts[p].body)
            .map(|s| s.linear_velocity)
            .unwrap_or(Vec3::ZERO);
        let point = match physics.state(self.parts[parent].body) {
            Some(s) => {
                // Through the centre of mass's height: a push at the axle would
                // pitch the robot, and the original's pitch is constant through a
                // full-throttle run.
                let com = s.position + s.rotation * self.mass_centre[parent];
                Vec3::new(centre.x, com.y, centre.z)
            }
            None => centre,
        };
        Some(WheelSample {
            parent,
            info,
            wheel,
            command,
            along,
            side,
            velocity,
            point,
        })
    }

    /// Push every grounded wheel: a thrust along the ground, a drag against it, a
    /// grip sideways, and a coupling that keeps the wheels at the speeds their
    /// commands ask for relative to each other.
    ///
    /// The thrust is the motor's full torque over the wheel's radius, scaled by
    /// the command and the wheel's `grip` — **recovered** from the original's
    /// drive trace: two 560 N·m wheels of 0.33 m push a 223 kg Scout at
    /// 15.2 m/s², and the trace's first 0.1 s reads 13.8 m/s² at 1 m/s. It is
    /// not limited by friction, because the original's is not (the Scout
    /// out-accelerates 1 g from a rest pose that is not on its wheels), and not
    /// by the motor's rated speed, because the Scout reaches 11.1 m/s with
    /// 18 rad/s motors on 0.33 m wheels. The drag is the fitted
    /// [`WHEEL_DRAG_RATE`]. The wheels' coupling stands in for the hinge motors'
    /// stiff speed servos, which hold both wheels of a pair at one speed (a
    /// centre of mass a few centimetres off the middle of the axle would
    /// otherwise turn a free pair of thrusts into a spin).
    fn push_wheels(&mut self, physics: &mut dyn PhysicsBackend, samples: &[WheelSample]) {
        if samples.is_empty() {
            return;
        }
        let dt = physics.last_dt();
        let wheels = self
            .rig
            .parts
            .iter()
            .filter(|q| q.joint.is_some_and(|j| j.wheel.is_some()))
            .count()
            .max(1) as f32;
        let share = self.mass / wheels;
        // The speed scale the commands imply: least squares of v_i = c_i * u.
        let (mut cv, mut cc) = (0.0f32, 0.0f32);
        for w in samples {
            cv += w.command * w.velocity.dot(w.along);
            cc += w.command * w.command;
        }
        let scale = if cc > 1e-4 { cv / cc } else { 0.0 };
        for w in samples {
            let thrust = w.command * w.wheel.grip * w.info.max_torque / w.wheel.radius;
            let v = w.velocity.dot(w.along);
            let drag = self.mass * WHEEL_DRAG_RATE * w.wheel.resistance * v;
            let couple = if cc > 1e-4 && w.command != 0.0 {
                share * WHEEL_COUPLING_RATE * (v - w.command * scale)
            } else {
                0.0
            };
            let cap = WHEEL_LATERAL_G * w.wheel.grip * share * physics.gravity().length();
            let lateral = (-share * WHEEL_LATERAL_RATE * w.velocity.dot(w.side)).clamp(-cap, cap);
            let force = w.along * (thrust - drag - couple) + w.side * lateral;
            physics.apply_impulse_at(self.parts[w.parent].body, force * dt, w.point);
        }
    }

    /// A burst device's stroke, as the original runs it.
    ///
    /// **Burst motor** (`0x446810` fire, `0x446670` tick): triggered while
    /// it is home, it unlocks, takes its *firing* power
    /// (`SetPowerSettings(f(g · burst speed), f(g · burst torque))`, the
    /// definition's `burst` line, `f` the health factor and `g` the power
    /// budget's scale) and drives at `+100` for 0.6 s (`+0x298`); the hinge
    /// locks itself on arriving within 0.05 rad of the end (`0x481450`). When
    /// the timer runs out it unlocks and cocks back at `-100` with its `power`
    /// pair, again `f(g · x)`, and locks home; it may fire again once it is
    /// locked within 0.05 of the start (`0x4810b0`). Live (Scout's fork,
    /// `burst motor.txt`: `burst = 0 50 90`, `power = 5 35`): fire at 49 rad/s
    /// and 3600 N·m, cock at 4 rad/s and 1400 N·m.
    ///
    /// **Burst piston** (`0x448750` fire, `0x448650` tick): it unlocks, gives
    /// the rod a one-substep push of `1000 · g` (`0x4b51b0(-1000 · g)`), turns
    /// its auto-lock off and drives the prismatic controller at direction
    /// `ftol(f(100 · g))` for 0.25 s; then auto-lock back on and direction
    /// `ftol(f(-100))`, home at the lower stop; ready once locked there
    /// (`0x4b5180`). The controller pushes with `|direction| · F` (the
    /// definition's raw force) up to the rod speed `S`
    /// ([`PrismaticSettings::force`]).
    ///
    /// Both ticks run once a frame (`tick`, the frame's time), between the
    /// frame's two Havok substeps: a press waits for the next tick, which
    /// fires and takes its frame off the timer at once, and the stroke ends
    /// at the tick that takes it below zero; the prismatic controller still
    /// pushes every substep. Live (docs/29 section 12r, `frida` on `0x448650`,
    /// `0x446670`, `0x4b51b0`, `0x4815c0`): the ticks come between the
    /// substep pairs with the frame's time (1/30 s, less when a frame was
    /// short), and from the one-shot push to the return push the piston
    /// takes 15 substeps (17 after a short frame), the burst motor from its
    /// firing setting to its cocking one 34 (36): 0.25 and 0.6 s counted in
    /// whole frames. Counted every substep they took 17 and 36.
    fn drive_burst(
        &mut self,
        physics: &mut dyn PhysicsBackend,
        p: usize,
        info: JointInfo,
        joint: JointHandle,
        scale: f32,
        tick: Option<f32>,
    ) {
        // The health factor `f` (`0x45fa90`) as a multiplier.
        let hf = self.motor_factor(info.motor);
        // A press waits for the component's tick, which takes it whatever the
        // device is doing (`0x448650`: the flags are cleared, then `0x448750`
        // fires only when cocked).
        if self.control.fired(&self.rig.wiring, info.motor) {
            self.burst_pending[p] = true;
        }
        let fired = tick.is_some() && std::mem::take(&mut self.burst_pending[p]);
        let position = if info.slider {
            physics.slider_offset(joint)
        } else {
            physics.hinge_angle(joint)
        };
        // The stroke's two ends: a motor fires from its start angle (the rest
        // pose) to the far end of its travel; a piston from its neutral point
        // to its lower stop (live, ALARM's rod: stops `+0x6c = 0` and `+0x70 =
        // -0.73`, out to -0.73 in six substeps and back in five,
        // `weapons-scout-alarm__live__000`).
        let (home, end) = if info.slider {
            (0.0, info.limits[0])
        } else if info.limits[1].abs() >= info.limits[0].abs() {
            (0.0, info.limits[1])
        } else {
            (0.0, info.limits[0])
        };
        let tol = 0.05;
        let at_home = (position - home).abs() < tol;
        let at_end = (position - end).abs() < tol;

        let before = self.burst[p];
        match before {
            BurstPhase::Cocked => {
                if fired {
                    self.burst[p] = BurstPhase::Firing;
                    self.burst_locked[p] = false;
                    // Set in the fire call; the same tick then takes its
                    // frame off (`0x4486a0`, `0x4466c4`).
                    self.burst_timer[p] =
                        if info.slider { 0.25 } else { 0.6 } - tick.unwrap_or(0.0);
                    if info.slider {
                        self.slider_dir[p] = (FULL * scale * hf).trunc();
                        // The one-shot push toward the far stop
                        // (`0x4b51b0(-1000 · g)`).
                        self.push_slider(physics, p, info, -1000.0 * scale);
                    } else {
                        let [speed, torque] = info
                            .fire
                            .unwrap_or([info.rate, info.max_torque / TORQUE_UNIT]);
                        self.hinge[p].set_power(scale * speed * hf, scale * torque * hf);
                    }
                }
            }
            BurstPhase::Firing => {
                if let Some(dt) = tick {
                    self.burst_timer[p] -= dt;
                }
                if tick.is_some() && self.burst_timer[p] <= 0.0 {
                    self.burst_timer[p] = 0.0;
                    self.burst[p] = BurstPhase::Cocking;
                    self.burst_locked[p] = false;
                    if info.slider {
                        self.slider_dir[p] = (-FULL * hf).trunc();
                    } else {
                        let torque = info.max_torque / TORQUE_UNIT;
                        self.hinge[p].set_power(scale * info.rate * hf, scale * torque * hf);
                    }
                }
            }
            BurstPhase::Cocking => {
                if tick.is_some() && at_home {
                    self.burst[p] = BurstPhase::Cocked;
                }
            }
        }
        let phase = self.burst[p];
        if phase != before && crate::havok17::dumping() {
            // The live `M` row (`frida/ra2_flipfight.js`): the motor's new
            // velocity and torque, so the live scripts find our strokes.
            let h = self.hinge[p];
            crate::havok17::dump_line(format_args!(
                "[\"M\",0,\"{}:{p}\",{},{}]",
                self.rig.name.trim(),
                if phase == BurstPhase::Firing {
                    -h.speed
                } else {
                    h.speed
                },
                h.max_torque
            ));
        }
        if phase != before && std::env::var_os("RA2_BURST_DEBUG").is_some() {
            eprintln!(
                "burst {} part {p}: {before:?} -> {phase:?} at {position:.3} (home {home:.3}, end {end:.3}) {:?}",
                self.rig.name, self.hinge[p]
            );
        }
        if info.slider {
            // Auto-lock is off while it fires; at home (or never fired) it
            // stays locked.
            let dir = match phase {
                BurstPhase::Cocked => 0.0,
                _ => self.slider_dir[p],
            };
            self.drive_prismatic(
                physics,
                p,
                info,
                joint,
                dir,
                (end, home),
                phase != BurstPhase::Firing,
                true,
            );
            return;
        }
        // The hinge: on arriving at the end it is driven toward it locks
        // (`0x481450`, auto-lock at the stops), and stays locked until the
        // next `Lock(0)`.
        // Cocked is only reached by arriving home, which locks it.
        if (phase == BurstPhase::Firing && at_end)
            || (phase == BurstPhase::Cocking && at_home)
            || phase == BurstPhase::Cocked
        {
            self.burst_locked[p] = true;
        }
        // The hinge controller (`0x481450`) is the same kind of Havok action as the
        // prismatic one: its auto-lock runs after the piece's integration, before the solver.
        if prelock_hinge_enabled() {
            if phase == BurstPhase::Firing && !at_end {
                physics.arm_slider_lock(joint, end, tol);
            } else if phase == BurstPhase::Cocking && !at_home {
                physics.arm_slider_lock(joint, home, tol);
            }
        }
        let locked = self.burst_locked[p];
        let dir = if end < home { -1.0 } else { 1.0 };
        let percent = match phase {
            BurstPhase::Firing => 100,
            BurstPhase::Cocking => -100,
            BurstPhase::Cocked => 0,
        };
        let torque = self.hinge[p].max_torque;
        if (torque - self.motor_torque[p]).abs() > 1e-3 || phase != before {
            self.motor_torque[p] = torque;
            let friction = if phase == BurstPhase::Cocked {
                info.friction_torque
            } else {
                0.0
            };
            physics.set_joint_strength(joint, torque, friction);
        }
        // Parked, a backend without the lock keeps the friction above.
        let lock_supported = physics.lock_joint(joint, locked);
        let target = if locked && lock_supported {
            0.0
        } else {
            hinge_target(self.hinge[p].velocity(percent), percent) * dir
        };
        physics.drive_hinge(joint, target);
    }

    /// The prismatic controller's update (`0x4b5240`) for a robot slider:
    /// locked while its direction is zero or it sits at the stop it is driven
    /// toward (when it auto-locks), else pushing the rod with
    /// `-(dir · F)` along its axis ([`PrismaticSettings::force`]) (the rod
    /// only, [`Self::push_slider`]). A positive direction drives the rod toward `-axis`, to the
    /// first of `stops` (`+0x70`), a negative one to the second (`+0x6c`).
    #[allow(clippy::too_many_arguments)]
    fn drive_prismatic(
        &mut self,
        physics: &mut dyn PhysicsBackend,
        p: usize,
        info: JointInfo,
        joint: JointHandle,
        dir: f32,
        stops: (f32, f32),
        auto_lock: bool,
        burst: bool,
    ) {
        let position = physics.slider_offset(joint);
        let arrived = (dir > 0.0 && (position - stops.0).abs() < 0.05)
            || (dir < 0.0 && (position - stops.1).abs() < 0.05);
        let hold = dir == 0.0 || arrived;
        // The motor is never used: the controller is a force. A backend without
        // the lock holds the rod on the definition's force as friction.
        physics.drive_slider(joint, 0.0);
        // A burst piston's lock is the device's own: set on arrival, cleared
        // only by its `Lock(0)` ([`Robot::drive_burst`]).
        let locked = if burst {
            if hold && auto_lock {
                self.burst_locked[p] = true;
            }
            self.burst_locked[p]
        } else {
            hold && auto_lock
        };
        let lock_supported = physics.lock_joint(joint, locked);
        let friction = if locked && !lock_supported {
            info.friction_torque
        } else {
            0.0
        };
        if (friction - self.motor_torque[p]).abs() > 1e-3 {
            self.motor_torque[p] = friction;
            physics.set_joint_strength(joint, 0.0, friction);
        }
        if hold {
            return;
        }
        // The controller's own auto-lock runs after the next piece's integration, before its
        // solver (docs/29 section 12ay): arm it where the backend steps.
        if auto_lock {
            physics.arm_slider_lock(joint, if dir > 0.0 { stops.0 } else { stops.1 }, 0.05);
        }
        let Some(parent) = self.rig.parts[p].parent else {
            return;
        };
        let (Some(a), Some(b)) = (
            physics.state(self.parts[parent].body),
            physics.state(self.parts[p].body),
        ) else {
            return;
        };
        let axis = self.slider_axis(physics, p, info);
        let rel = b.linear_velocity - a.linear_velocity;
        // The rod's speed in the sense a positive direction drives it.
        let along = -rel.dot(axis);
        if let Some(force) = self.prismatic[p].force(dir, along, rel.length()) {
            self.push_slider(physics, p, info, -force);
        }
    }

    /// A slider's axis in the world, from its housing's frame.
    fn slider_axis(&self, physics: &dyn PhysicsBackend, p: usize, info: JointInfo) -> Vec3 {
        let Some(parent) = self.rig.parts[p].parent else {
            return Vec3::ZERO;
        };
        let frame_a = self.rig.components[self.rig.parts[parent].root].local;
        let local = frame_a.inverse().transform_vector3(info.axis);
        let rot = physics
            .state(self.parts[parent].body)
            .map(|s| s.rotation)
            .unwrap_or(Quat::IDENTITY);
        (rot * local).normalize_or_zero()
    }

    /// Push a slider's rod along its axis with `force` newtons for this
    /// substep (`0x4b51b0`).
    ///
    /// The rod alone: `0x4b51b0` scales the constraint's axis by the force,
    /// turns it into the world with the housing's frame and applies it once,
    /// to the rod's body (`this + 0x5c`, `0x4b0950`); nothing pushes the
    /// housing back. The robot takes the rod's momentum only when the rod
    /// reaches its stop. Live (`fightcontacts-scout-alarm-box__live__001/002`,
    /// ALARM's hammer): the chassis' speed along the stroke goes
    /// `0 → +0.4 → +1.9` m/s with no recoil first, on its wheels and on its
    /// tail alike; with the reaction ours recoiled to -0.8 first and kept
    /// only +1.2 (docs/29 section 12k).
    fn push_slider(&self, physics: &mut dyn PhysicsBackend, p: usize, info: JointInfo, force: f32) {
        if self.rig.parts[p].parent.is_none() {
            return;
        }
        let axis = self.slider_axis(physics, p, info);
        if crate::havok17::dumping() {
            // The live `F` rows (`frida/ra2_robotpair.js`, `0x4b51b0`).
            crate::havok17::dump_line(format_args!(
                "[\"F\",0,\"{}\",{p},{force}]",
                self.rig.name.trim()
            ));
        }
        physics.add_force(self.parts[p].body, axis * force);
    }

    /// Cave the chassis in as it takes damage (F9).
    ///
    /// The bot file marks which points of the chassis model the game may move:
    /// it subdivides the sides and top, and "every hit then will shift these
    /// 'buckling' points down and in, causing the sides of the robot to buckle"
    /// (`tools/vendor/PyRA2/.../Bot file format analysis.txt` §"3D model of bot
    /// with buckling points"). Until now a chassis only ever died whole.
    ///
    /// Both the drawn chassis and its collider follow, so a hurt robot is
    /// visibly crushed *and* its hull is the crushed one. Jolt refuses
    /// `set_shape` on a constrained body (`UsedByConstraint`), which is every
    /// motorized stock robot; the backend therefore removes the chassis'
    /// hinges, re-shapes the body and recreates each hinge with its **current
    /// angle as the reference**, so a burst motor's travel does not move. The
    /// recreated joints keep their handles, so `parts[].joint` stays valid.
    /// Measured on `AI/Team1/Bot0`: the collider's top falls 0.205 m at half
    /// damage and the robot still drives 6.81 m in 3 s. The movable region is
    /// the file's own: the lowest buckle point pins the floor plate (the
    /// Octagon-sized robots keep everything below the mid-height rigid), and
    /// the motion grows with height above it, so the top caves in furthest.
    ///
    /// **How far it moves is the file's own limit.** The buckle points are a ring
    /// at mid-height and a ring at the top, and the movable region is the top
    /// half of the chassis — verified across all 43 stock bots that carry a
    /// buckle model (the other 14 ship without one, from before the format had
    /// it). So a full crush takes the top ring down onto the mid ring, which is
    /// as far as it can go before the model turns itself inside out; the code
    /// reads that region out of the file rather than assuming the half, and
    /// clamps to it. What is still ours is the *curve* between the two ends
    /// (linear in the damage taken) and how far the sides come *in* — the
    /// footprint halves over the same sweep — plus the game's subdivided crush
    /// mesh, which is built at runtime from indices that point into a mesh the
    /// file does not contain.
    pub fn buckle(&mut self, physics: &mut dyn PhysicsBackend) {
        /// How far the top plate comes *in* over the whole crush: the footprint
        /// halves. The engine's own figure is not reachable.
        const IN: f32 = 0.5;
        if self.health.is_empty() || self.rig.components[0].buckle.is_empty() {
            return;
        }
        let max_hp = self.rig.components[0].hitpoints.max(1.0);
        let fraction = (1.0 - self.health[0] / max_hp).clamp(0.0, 1.0);
        let c = &mut self.rig.components[0];
        if fraction <= 1e-4 || (fraction - c.buckle_fraction).abs() <= 1e-4 {
            return;
        }
        if c.rest_hull.len() != c.hull_points.len() || c.rest_items.len() != c.items.len() {
            return;
        }
        c.buckle_fraction = fraction;

        // The file's own movable region: the lowest buckle point pins the floor
        // of it, and the top of the chassis is its ceiling. A full crush takes
        // the top exactly onto that floor.
        let floor = c
            .buckle
            .iter()
            .map(|p| p.y)
            .fold(f32::MAX, f32::min)
            .min(c.max.y);
        let span = (c.max.y - floor).max(1e-3);
        // Applied **from the pristine mesh**, not from the last crushed one.
        // The displacement is then linear in the damage fraction: at f = 1 the
        // top ring lands on `floor` exactly. Compounding a relative step on the
        // current height instead is asymptotic (measured: 0.2536 m of the
        // 0.3709 m span at f = 1, 68 %), so a full crush never reached the
        // file's own limit.
        let crush = |orig: Vec3| {
            let w = ((orig.y - floor) / span).clamp(0.0, 1.0);
            if w <= 0.0 {
                return orig;
            }
            let shrink = 1.0 - fraction * IN * w;
            Vec3::new(
                orig.x * shrink,
                orig.y - fraction * span * w,
                orig.z * shrink,
            )
        };

        for (item, rest) in c.items.iter_mut().zip(&c.rest_items) {
            for (v, orig) in item.vertices.iter_mut().zip(rest) {
                v.pos = crush(*orig).into();
            }
        }
        for (p, orig) in c.hull_points.iter_mut().zip(&c.rest_hull) {
            *p = crush(*orig);
        }

        self.rebuild_part(physics, 0);
    }

    /// Apply one blow that already passed the arena's `>= 10` gate to a
    /// component: the original's per-class `apply` (vtable `+0x1c`).
    ///
    /// An ordinary component is `FUN_0045f2b0` (`d` its mount depth, `+0xe4`,
    /// [`RobotRig::depth`]):
    ///
    /// ```text
    /// if d > 1 && hp <= 0 && fracture > 0 && fracture < blow   -> queue detach (FUN_00417d70)
    /// if damageable || d > 1 { hp -= min(blow, hp); return that }  // else return the blow
    /// ```
    ///
    /// The detach test runs **before** the subtraction and reads the hitpoints
    /// the component *already* has, so a part that is merely ground down to zero
    /// stays on the robot; it comes away on the next single blow larger than its
    /// `fracture` (a component whose `fracture` is zero never does). A part bolted
    /// straight to the chassis (`d == 1`: the motors, batteries, boards, a piston, a
    /// SmartZone) never comes off at all (live: all 20 detach queues of 16 Box and Compressor
    /// fights are parts of depth 2 or more, and 25 blows over the fracture on dead depth-1
    /// parts took none off, `partsfight-*`), and an
    /// indestructible one of that depth (a control board, an axle mount) keeps its
    /// hitpoints while the function still returns the whole blow, which is what the match
    /// scores. Hitpoints cannot go below zero, and the amount returned is what was
    /// actually subtracted, which is what the match awards as score.
    ///
    /// The chassis is `FUN_0044f370` instead: hitpoints go negative without a
    /// floor, hitpoints reaching zero defeat nothing, and once they are `<= 0`
    /// every blow over `40.0` is a *critical hit*; the third one is the
    /// DESTROYED state (`FUN_00417cc0`). It returns the whole blow. It also hands
    /// part of every blow to the parts mounted in it, see [`Robot::splash`].
    pub fn damage(&mut self, index: usize, amount: f32) -> Applied {
        let mut out = Applied::default();
        if index >= self.health.len() || !self.attached[index] || amount <= 0.0 {
            return out;
        }
        let c = &self.rig.components[index];
        if index == 0 {
            if !c.damageable {
                return out;
            }
            if self.health[0] <= 0.0 && amount > CRITICAL_BLOW {
                out.critical = true;
                self.criticals += 1;
                if self.criticals >= CRITICALS_TO_DESTROY {
                    out.destroyed = true;
                }
            }
            self.health[0] -= amount;
            out.effective = amount;
            return out;
        }
        // `FUN_0045f2b0`: the detach and the hitpoints both look at `+0xe4`, the mount depth.
        // A part bolted straight to the chassis (depth 1) is never queued for detach, and an
        // indestructible one (`+0x9c` clear) of that depth loses no hitpoints, though the
        // blow is what the function returns (and so what the hit scores).
        let gate = self.mount_depth_gate();
        if !gate && !c.damageable {
            return out; // the old rule
        }
        let deep = !gate || self.rig.depth(index) > 1;
        if deep && self.health[index] <= 0.0 && c.fracture > 0.0 && c.fracture < amount {
            self.fractured[index] = true;
            out.detach = true;
        }
        if c.damageable || deep {
            let taken = amount.min(self.health[index].max(0.0));
            self.health[index] -= taken;
            out.effective = taken;
        } else {
            out.effective = amount;
        }
        out
    }

    /// `RA2_DEPTH=0`: the old rules (every part may break off, an indestructible one is
    /// untouchable) for comparison.
    fn mount_depth_gate(&self) -> bool {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ON.get_or_init(|| std::env::var("RA2_DEPTH").map(|v| v != "0").unwrap_or(true))
    }

    /// Whether the chassis' splash (see [`Robot::splash`]) reaches this component: the
    /// original lists a robot's components and `FUN_004e2540` takes those at mount depth 1
    /// (`+0xe4`, `FUN_0045ed20`; [`RobotRig::depth`]): live every motor, piston, battery,
    /// air tank, control board and SmartZone, never a wheel, a weapon or an extender.
    pub fn takes_splash(&self, index: usize) -> bool {
        index != 0 && self.rig.depth(index) == 1
    }

    /// What a blow on the chassis does to the parts mounted in it (docs/29 section 12ar).
    ///
    /// The chassis' apply (`FUN_0044f370`) passes `(1 - hp / maxhp) * blow`, with the
    /// hitpoints it had before the blow, to the robot's group (`FUN_004df200` ->
    /// `FUN_004e2500`, one `FUN_004e2540` per listed component). Each component of kind 1
    /// takes `(1 - d) * that`, `d` the distance in metres from the hit point to the
    /// component's origin (`vtable + 4`), when that is above zero, through its own
    /// `apply` (`FUN_00489620` -> `0x45f2b0`): no `>= 10` gate, no score. So the more a
    /// chassis is hurt, the more of every further blow the motors, batteries and boards
    /// near the point take; with the chassis' hitpoints below zero it is more than the
    /// blow itself. Live `0x4e2540`: `x = 1 - d` exactly, and `C` rows of
    /// `(1 - d) * 1.287` for a blow of 18.4 on a chassis at 1700 of 1828.
    ///
    /// Returns the components that took something, with what [`Robot::damage`] said.
    pub fn splash(
        &mut self,
        physics: &dyn PhysicsBackend,
        point: Vec3,
        blow: f32,
        hp_before: f32,
    ) -> Vec<(usize, Applied)> {
        let max = self.rig.components[0].hitpoints;
        // `(1 - hp / (float)maxhp) * blow`, and the group is only called when it is positive.
        let amount = (1.0 - hp_before / max.trunc().max(1.0)) * blow;
        if amount.is_nan() || amount <= 0.0 {
            return Vec::new();
        }
        let worlds = self.component_worlds(physics);
        let mut out = Vec::new();
        for (i, world) in worlds.iter().enumerate().skip(1) {
            if !self.attached[i] || !self.takes_splash(i) {
                continue;
            }
            let d = (world.w_axis.truncate() - point).length();
            let share = (1.0 - d) * amount;
            if share > 0.0 {
                out.push((i, self.damage(i, share)));
            }
        }
        out
    }

    /// Whether a component is smoking: a motor, or a car-steering unit
    /// (`FUN_004f6dd0`, the emitter `FUN_004f6a10`), at or under half its
    /// hitpoints turns its smoke emitter on (`FUN_0048e0d0` → `FUN_0048e0b0` /
    /// `FUN_0048dc50`).
    /// The renderer may draw the particles; the simulation only keeps the state.
    pub fn smoking(&self, index: usize) -> bool {
        let Some(c) = self.rig.components.get(index) else {
            return false;
        };
        (c.role == Role::Motor || c.steer_power.is_some())
            && self.attached[index]
            && c.hitpoints > 0.0
            && self.health[index] <= 0.5 * c.hitpoints
    }

    /// Break `component` (and anything welded past it) off the robot.
    ///
    /// Returns each detached component with the world transform it had at the
    /// moment it came away, so the caller can spawn debris.
    pub fn detach(
        &mut self,
        physics: &mut dyn PhysicsBackend,
        component: usize,
    ) -> Vec<(usize, Mat4)> {
        let mut out = Vec::new();
        if component >= self.attached.len() || !self.attached[component] {
            return out;
        }
        let part = self.rig.part_of[component];
        if !self.parts[part].alive {
            return out;
        }
        let worlds = self.component_worlds(physics);
        let root = self.rig.parts[part].root;

        // What comes away with it: itself plus its welded descendants.
        let members = self.rig.parts[part].members.clone();
        let removed: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&i| {
                let mut j = i;
                loop {
                    if j == component {
                        return true;
                    }
                    if j == root {
                        return false;
                    }
                    match self.rig.components[j].parent {
                        Some(p) if self.rig.part_of[p] == part => j = p,
                        _ => return false,
                    }
                }
            })
            .collect();
        if removed.is_empty() {
            return out;
        }

        if removed.len() == members.len() {
            self.remove_part(physics, part, &worlds, &mut out);
        } else {
            for &i in &removed {
                self.attached[i] = false;
                self.health[i] = 0.0;
                out.push((i, worlds[i]));
            }
            self.rebuild_part(physics, part);
        }
        // A component coming off re-caps every hinge (`0x4dbe3f`).
        self.cap_loads(physics);
        out
    }

    /// Remove a whole body (and its children, whose joints die with it).
    fn remove_part(
        &mut self,
        physics: &mut dyn PhysicsBackend,
        part: usize,
        worlds: &[Mat4],
        out: &mut Vec<(usize, Mat4)>,
    ) {
        if !self.parts[part].alive {
            return;
        }
        let children: Vec<usize> = (0..self.rig.parts.len())
            .filter(|&q| self.rig.parts[q].parent == Some(part))
            .collect();
        for q in children {
            self.remove_part(physics, q, worlds, out);
        }
        if let Some(j) = self.parts[part].joint.take() {
            physics.remove_joint(j);
        }
        physics.remove_body(self.parts[part].body);
        self.parts[part].alive = false;
        for &i in &self.rig.parts[part].members {
            if self.attached[i] {
                self.attached[i] = false;
                self.health[i] = 0.0;
                out.push((i, worlds[i]));
            }
        }
    }

    /// Re-shape a body after some of its members came off. The body keeps its id,
    /// pose and joints, so nothing else has to be rebuilt.
    fn rebuild_part(&mut self, physics: &mut dyn PhysicsBackend, part: usize) {
        let members: Vec<usize> = self.rig.parts[part]
            .members
            .iter()
            .copied()
            .filter(|&i| self.attached[i])
            .collect();
        if physics.name() == "havok17" {
            // The body as it was built (`havok_part`): its composite of rigid
            // bodies, a rotor's mass with the body it drives, the inertia
            // summed per rigid body. Rebuilding by the Jolt rule instead (the
            // members' masses, the hull's inertia) gave the Scout's chassis
            // 193.5 kg at its first buckle against the 163.5 kg it is built
            // with and keeps live for the whole fight
            // (`fightcontacts-scout-alarm-box__live__000..003`).
            // Live, the Scout's chassis keeps its mass and centre through a
            // fight, and the original's chassis collider does not crush: with
            // 971 of its 1828 hitpoints gone (`fightcontacts-scout-alarm-box__live__006`,
            // 20 s) a contact still lands on its top at y 0.359 above its
            // centre of mass, the pristine height. So a body is rebuilt from
            // its Havok parts as built (the chassis' collision mesh is the
            // `.bot` mesh, which the crush does not move), less what came off.
            let h = Self::havok_part(&self.rig, part, Some(&self.attached));
            let natural = h.mass;
            let mass = (natural * self.mass_scale).max(0.2);
            let body = self.parts[part].body;
            if let Err(e) = physics.set_body_shape(body, &h.shape, Some(mass)) {
                eprintln!("ra2sim: rebuild failed: {e}");
            }
            // A piston rod keeps the inertia it was built with (see `spawn`).
            let rod = self.rig.parts[part].joint.is_some_and(|j| j.slider);
            if let (Some(inertia), false) = (h.inertia, rod) {
                physics.set_body_inertia(body, inertia * (mass / natural.max(1e-3)));
            }
            return;
        }
        let shape = Self::part_shape(&self.rig, &members);
        let mass = (members
            .iter()
            .map(|&i| self.rig.components[i].mass)
            .sum::<f32>()
            * self.mass_scale)
            .max(0.2);
        if let Err(e) = physics.set_body_shape(self.parts[part].body, &shape, Some(mass)) {
            eprintln!("ra2sim: rebuild failed: {e}");
        }
    }

    /// The mass the chassis body itself carries.
    ///
    /// The original is one rigid body per robot, so a force on the robot is a
    /// force on all of it; ours is a body per component, and the chassis is the
    /// one the fight revolves around. Anything that pushes the *robot* has to go
    /// through this ratio to accelerate it as one thing.
    pub fn chassis_mass(&self) -> f32 {
        // The same sum `new` gives the body: a part's own mass is its members'.
        (self.rig.parts[0]
            .members
            .iter()
            .map(|&i| self.rig.components[i].mass)
            .sum::<f32>()
            * self.mass_scale)
            .max(0.2)
    }

    /// Mass of the components still attached.
    pub fn remaining_mass(&self) -> f32 {
        self.rig
            .components
            .iter()
            .enumerate()
            .filter(|(i, _)| self.attached[*i])
            .map(|(_, c)| c.mass)
            .sum::<f32>()
            * self.mass_scale
    }

    /// World-space centre and radius of an attached component.
    pub fn component_sphere(&self, worlds: &[Mat4], index: usize) -> (Vec3, f32) {
        let c = &self.rig.components[index];
        (worlds[index].transform_point3(c.center()), c.radius())
    }
}

/// `RA2_PRELOCK_HINGE=0` runs a burst motor's hinge lock in `Robot::drive`, a piece late, as
/// before (docs/29 section 12az).
fn prelock_hinge_enabled() -> bool {
    std::env::var("RA2_PRELOCK_HINGE").map_or(true, |v| v != "0")
}
