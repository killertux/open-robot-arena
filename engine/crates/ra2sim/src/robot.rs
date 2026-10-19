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
use crate::controls::{ControlState, COCK_FRACTION, FULL};
use crate::drive::{MagicAction, WheelRecord};
use crate::massprops;
use crate::rig::{JointInfo, JointUse, RobotRig, Role, WheelDrive};
use crate::scene::USER_BOT;
use crate::sim::{DriveInput, DriveTuning};

/// Stock component friction/restitution (see the validation report).
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
fn motor_passes() -> f32 {
    std::env::var("RA2_WHEEL_PASSES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0)
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
    /// Per part: the power factor its motor's torque was last set for.
    motor_power: Vec<f32>,
    /// The game's own drive model ([`crate::drive`]), when the robot uses it.
    pub magic: Option<MagicAction>,
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
        mut rig: RobotRig,
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
        let shift = Self::cock_burst_pistons(&mut rig);
        let mut parts = Vec::with_capacity(rig.parts.len());
        for (p, part) in rig.parts.iter().enumerate() {
            let world = bot * Mat4::from_translation(shift[p]) * rig.components[part.root].local;
            let (_, rot, pos) = world.to_scale_rotation_translation();
            let (shape, mass, friction, restitution, phantom) = if havok {
                let h = Self::havok_part(&rig, p);
                (h.shape, h.mass, h.friction, h.restitution, h.phantom)
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
                )
            };
            let mass = (mass * mass_scale).max(0.2);
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
            // The pivot rides on the parent; a child moved off its rest pose
            // (a cocked piston rod) names it from where it now is.
            let child_pivot = info.pivot + shift[parent] - shift[p];
            let joint = if info.slider {
                Self::make_slider(physics, &rig, &parts, parent, p, info, child_pivot)?
            } else {
                Self::make_hinge(physics, &rig, &parts, parent, p, info, child_pivot)?
            };
            parts[p].joint = Some(joint);
        }

        let magic =
            magic_drive(physics).then(|| Self::magic_records(physics, &rig, &parts, rotation));

        Ok(Robot {
            magic,
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
            motor_power: vec![1.0; rig.parts.len()],
            mass_centre: rig
                .parts
                .iter()
                .map(|part| Self::mass_center(&rig, &part.members).unwrap_or(Vec3::ZERO))
                .collect(),
            team,
            rig,
        })
    }

    /// Spawn every burst piston cocked: its rod retracted to the far end of
    /// its travel, from where it fires. Returns each part's bot-space offset
    /// from the rest pose, and re-bases those pistons' travel on the cocked
    /// pose.
    ///
    /// Live, ALARM's hammer body (33 kg: hammer head 25 + piston `shaft` 8)
    /// sits at the chassis' height, 0.82 m in front of it, and the robot rests
    /// on its four hubs (`bodies.json`, `clist_rest`); at the model's own pose
    /// the hammer hangs 0.74 m lower, below the wheels, and holds the robot
    /// up. Its `burstpiston2` declares `prismatic = body, shaft, 0, 0, 1, 0,
    /// -0.73, 0.2`: the 0.73 m the rod can retract is that difference. So the
    /// travel counts along the rod's extension, and a burst piston stands at
    /// its lower limit — retracted — until it fires.
    fn cock_burst_pistons(rig: &mut RobotRig) -> Vec<Vec3> {
        let n = rig.parts.len();
        let mut own = vec![Vec3::ZERO; n];
        for (p, part) in rig.parts.iter_mut().enumerate() {
            let Some(info) = part.joint.as_mut() else {
                continue;
            };
            if !(info.slider && info.use_kind == JointUse::Burst) {
                continue;
            }
            let axis = info.axis.normalize_or_zero();
            let [lower, upper] = info.limits;
            if axis == Vec3::ZERO || lower >= 0.0 {
                continue;
            }
            // Retracting runs along the declared axis (toward the housing).
            own[p] = axis * -lower;
            info.limits = [-(upper - lower), 0.0];
        }
        // A part moves with everything it hangs from.
        (0..n)
            .map(|p| {
                let mut total = Vec3::ZERO;
                let mut q = Some(p);
                while let Some(i) = q {
                    total += own[i];
                    q = rig.parts[i].parent;
                }
                total
            })
            .collect()
    }

    /// The Havok body of part `p` (see [`crate::rig::HavokParts`]): one convex
    /// piece per colliding rigid body of its components (no skin, the real
    /// wheel hubs), a motor's rotor moved to the part it drives, every rigid
    /// body's mass (phantoms included), the top rigid body's material — the
    /// chassis body is the game's own, with Havok's defaults (live 0.5 / 0.2) —
    /// and the wheels' traction phantoms.
    fn havok_part(rig: &RobotRig, p: usize) -> HavokPart {
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
        let mut add = |pieces: &[Vec<Vec3>], m: Mat4, weight: f32, tag: u32, at: Vec3| {
            let mut placed = Vec::new();
            for pts in pieces {
                let local: Vec<Vec3> = pts.iter().map(|q| m.transform_point3(*q)).collect();
                if massprops::hull_volume_centroid(&local).is_some() {
                    placed.push(local);
                }
            }
            let centre = placed
                .iter()
                .filter_map(|pts| massprops::hull_volume_centroid(pts))
                .fold((0.0f32, Vec3::ZERO), |(v, c), (vi, ci)| {
                    (v + vi, c + ci * vi)
                });
            let centre = if centre.0 > 1e-9 {
                centre.1 / centre.0
            } else {
                at
            };
            if weight > 0.0 {
                mass += weight;
                moment += centre * weight;
            }
            for pts in placed {
                children.push(CompoundChild {
                    local_position: Vec3::ZERO,
                    local_rotation: Quat::IDENTITY,
                    shape: ShapeDesc::ConvexHull { points: pts },
                    tag,
                });
            }
        };
        for &i in &part.members {
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
            let collides = c.role.is_physical() && c.role != Role::Power;
            let pieces: Vec<Vec<Vec3>> = if !collides {
                Vec::new()
            } else if !h.collision.is_empty() || !h.rotor_collision.is_empty() {
                let mut v = h.collision.clone();
                if !own_rotor {
                    v.extend(h.rotor_collision.iter().cloned());
                }
                v
            } else if !c.hull_points.is_empty() {
                vec![c.hull_points.clone()]
            } else {
                Vec::new()
            };
            add(&pieces, m, weight.max(0.0), i as u32, m.w_axis.truncate());
            phantom.extend(h.phantom.iter().map(|q| m.transform_point3(*q)));
        }
        // The rotor of the motor this part hangs from.
        if let Some(info) = part.joint {
            let motor = &rig.components[info.motor];
            let m = frame.inverse() * motor.local;
            add(
                &motor.havok.rotor_collision,
                m,
                motor.havok.rotor_mass,
                info.motor as u32,
                frame.inverse().transform_point3(info.pivot),
            );
        }
        let shape = match children.len() {
            0 => ShapeDesc::Sphere { radius: 0.1 },
            _ => ShapeDesc::Compound(children),
        };
        let centre = (mass > 1e-6).then(|| moment / mass);
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
            mass: mass.max(0.2),
            friction,
            restitution,
            phantom,
        }
    }

    /// The robot's wheel records (`AddWheel`, `FUN_00485490` from
    /// `FUN_004dc180`): every wheel on a `SpinMotor` or an `AxleMount`, with its
    /// hub centre and axle in the chassis frame, `grip` and `resistance`.
    fn magic_records(
        physics: &dyn PhysicsBackend,
        rig: &RobotRig,
        parts: &[PartState],
        rotation: Quat,
    ) -> MagicAction {
        let chassis = parts[0].body;
        let centre = physics.center_of_mass(chassis).unwrap_or(Vec3::ZERO);
        let mut wheels = Vec::new();
        for (p, part) in rig.parts.iter().enumerate() {
            let Some(info) = part.joint else { continue };
            let root = &rig.components[part.root];
            if root.role != Role::Wheel || info.slider {
                continue;
            }
            let class = rig.components[info.motor].class.to_ascii_lowercase();
            let driven = class == "spinmotor";
            if !driven && class != "axlemount" {
                continue;
            }
            let stats = root.wheel.clone().unwrap_or_default();
            let hub = physics.center_of_mass(parts[p].body).unwrap_or(centre);
            let host = rig.components[rig.parts[0].root].local;
            wheels.push(WheelRecord {
                part: p,
                motor: driven.then_some(info.motor),
                point: rotation.inverse() * (hub - centre),
                axle: host
                    .inverse()
                    .transform_vector3(info.axis)
                    .normalize_or_zero(),
                grip: stats.grip,
                resistance: stats.resistance,
                thrust: 0.0,
                touching: false,
                air: crate::drive::TOUCH_MEMORY * 2.0,
                normal: Vec3::Y,
            });
        }
        MagicAction { wheels }
    }

    /// Drive through the game's own model: the wheels' records get their
    /// thrust and their touching flag, a wheel in the air spins on its hinge
    /// servo, and the [`MagicAction`] pushes the chassis.
    fn drive_magic(&mut self, physics: &mut dyn PhysicsBackend) {
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
            MagicAction::touch(w, touch, dt);
            if !alive {
                w.touching = false;
            }
            let Some(motor) = w.motor else {
                w.thrust = 0.0;
                continue;
            };
            let Some(info) = self.rig.parts[w.part].joint else {
                continue;
            };
            let command = self.control.command(&self.rig.wiring, motor).unwrap_or(0.0);
            let power = self.motor_factor(motor);
            // `motor + 0x29c = ftol(f(percent))`, the torque `f(torque)`.
            let percent = (command * power).trunc() as i32;
            let torque = info.max_torque / TORQUE_UNIT * power;
            w.thrust = MagicAction::thrust(torque, percent);
            if let Some(joint) = self.parts[w.part].joint {
                if w.touching || percent == 0 {
                    // `SetPercent` (`0x4f5800`): the hinge motor is off while the
                    // wheel is down.
                    physics.drive_hinge(joint, 0.0);
                } else {
                    // In the air the hinge servo spins it: `ftol(-pct · 0.01 ·
                    // speed)` rad/s (`0x481450`), the speed also `f`-scaled.
                    let target = (percent as f32 * 0.01 * info.rate * power).trunc();
                    if (power - self.motor_power[w.part]).abs() > 1e-4 {
                        self.motor_power[w.part] = power;
                        physics.set_joint_strength(joint, info.max_torque * power, 0.0);
                    }
                    physics.drive_hinge(joint, target * CLOCKWISE);
                }
            }
        }
        if self.parts[0].alive {
            magic.apply(physics, self.parts[0].body);
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
        let (pivot_bot, axis_bot) = (info.pivot, info.axis);
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
        };
        physics.create_hinge(&desc)
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
    /// There is no steering model beyond the wiring: a robot turns because the
    /// `LeftRight` wires drive its two sides differently. `DriveInput::throttle`
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
                JointUse::Burst => self.drive_burst(physics, p, info, joint),
                JointUse::Wheel if info.wheel.is_some() && !legacy_wheels() => {
                    let command = self
                        .control
                        .command(&self.rig.wiring, info.motor)
                        .unwrap_or(0.0);
                    let power = self.motor_factor(info.motor);
                    if let Some(sample) =
                        self.sample_wheel(physics, p, info, joint, command / FULL * power)
                    {
                        samples.push(sample);
                    }
                }
                JointUse::Spun | JointUse::Wheel | JointUse::Piston => {
                    let command = self
                        .control
                        .command(&self.rig.wiring, info.motor)
                        .unwrap_or(0.0);
                    // TODO(load limit): the original caps a hinge's torque by the
                    // load it carries (`HingeController` vtable 0x6a7e44 slot 10 =
                    // 0x480510, "min(75T, 25m(1-|d|))", speed S*min(1, 2T/(m(1+|d|))))
                    // but nothing calls it directly: the constructors at 0x480293 and
                    // 0x480320 only install the vtable, so it is a Havok motor
                    // callback and the caller is not recovered.
                    let k = if info.use_kind == JointUse::Wheel {
                        motor_passes()
                    } else {
                        1.0
                    };
                    let power = self.motor_factor(info.motor);
                    if (power - self.motor_power[p]).abs() > 1e-4 {
                        self.motor_power[p] = power;
                        physics.set_joint_strength(
                            joint,
                            info.max_torque * k * power,
                            info.friction_torque,
                        );
                    }
                    let target = command / FULL * info.rate * CLOCKWISE * k * power;
                    if info.slider {
                        physics.drive_slider(joint, target);
                    } else {
                        physics.drive_hinge(joint, target);
                    }
                }
            }
        }
        self.push_wheels(physics, &samples);
        self.drive_magic(physics);
        self.control.end_step();
    }

    /// How much of its rated speed and torque a motor still delivers.
    ///
    /// `FUN_0045fa90`: `f(x) = x * (0.5 + 0.5 * hp / max)`, and `0.5 * x` once the
    /// motor's hitpoints are below zero — a hurt motor is weaker, down to half.
    /// The original feeds `f` both the speed and the torque of
    /// `SetPowerSettings` (the `SpinMotor` apply at `0x4468bc`), after a global
    /// factor `g` from the robot's power budget (`0x45f130`, set at `0x4dc76c`
    /// from `min(1, robot+0x9c / robot+0xa0)`). Those two robot fields are not
    /// identified, so `g` is taken as 1.
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
        let found = physics
            .ray_cast(centre - Vec3::Y * reach, Vec3::Y, reach)
            .filter(|hit| !own.contains(&hit.body));
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
        // A positive target turns the wheel about its axle by the right-hand
        // rule, which pushes the robot along `axle x normal`; the control board's
        // sense is `CLOCKWISE`.
        let along = axle.cross(normal).normalize_or_zero() * CLOCKWISE;
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

    /// One step of a burst motor's or piston's fire cycle.
    fn drive_burst(
        &mut self,
        physics: &mut dyn PhysicsBackend,
        p: usize,
        info: JointInfo,
        joint: JointHandle,
    ) {
        // The end of the travel is the farther limit from the rest pose: the
        // motor's `end - start` (its other limit is zero), or a piston's longer
        // stroke. (Ours for a piston: the rest pose is taken as the retracted end.)
        let travel = if info.limits[1].abs() >= info.limits[0].abs() {
            info.limits[1]
        } else {
            info.limits[0]
        };
        let dir = if travel < 0.0 { -1.0 } else { 1.0 };
        let position = if info.slider {
            physics.slider_offset(joint)
        } else {
            physics.hinge_angle(joint)
        };
        let reached = (position * dir) >= travel.abs() - if info.slider { 0.01 } else { 0.05 };
        let home = (position * dir) <= if info.slider { 0.01 } else { 0.05 };
        let fired = self.control.fired(&self.rig.wiring, info.motor);

        let phase = &mut self.burst[p];
        let before = *phase;
        match *phase {
            BurstPhase::Cocked => {
                if fired {
                    *phase = BurstPhase::Firing;
                }
            }
            BurstPhase::Firing => {
                if reached {
                    *phase = BurstPhase::Cocking;
                }
            }
            BurstPhase::Cocking => {
                if fired && !reached {
                    // Triggered again before it is home: released from here.
                    *phase = BurstPhase::Firing;
                } else if home {
                    *phase = BurstPhase::Cocked;
                }
            }
        }
        let phase = *phase;
        let target = match phase {
            BurstPhase::Cocked => 0.0,
            BurstPhase::Firing => dir * info.rate,
            BurstPhase::Cocking => -dir * info.rate * COCK_FRACTION,
        };
        // Parked, the joint's friction holds it at the motor's torque; moving, it
        // must not fight the motor.
        if phase != before {
            let friction = if phase == BurstPhase::Cocked {
                info.friction_torque
            } else {
                0.0
            };
            physics.set_joint_strength(joint, info.max_torque, friction);
        }
        if info.slider {
            physics.drive_slider(joint, target);
        } else {
            physics.drive_hinge(joint, target);
        }
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
    /// An ordinary component is `FUN_0045f2b0`:
    ///
    /// ```text
    /// if hp <= 0 && fracture > 0 && fracture < blow   -> queue detach (FUN_00417d70)
    /// hp -= min(blow, hp)                              -> returns that amount
    /// ```
    ///
    /// The detach test runs **before** the subtraction and reads the hitpoints
    /// the component *already* has, so a part that is merely ground down to zero
    /// stays on the robot; it comes away on the next single blow larger than its
    /// `fracture` (a component whose `fracture` is zero never does). Hitpoints
    /// cannot go below zero, and the amount returned is what was actually
    /// subtracted, which is what the match awards as score.
    ///
    /// The chassis is `FUN_0044f370` instead: hitpoints go negative without a
    /// floor, hitpoints reaching zero defeat nothing, and once they are `<= 0`
    /// every blow over `40.0` is a *critical hit*; the third one is the
    /// DESTROYED state (`FUN_00417cc0`). It returns the whole blow.
    pub fn damage(&mut self, index: usize, amount: f32) -> Applied {
        let mut out = Applied::default();
        if index >= self.health.len() || !self.attached[index] || amount <= 0.0 {
            return out;
        }
        let c = &self.rig.components[index];
        if !c.damageable {
            return out;
        }
        if index == 0 {
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
        if self.health[index] <= 0.0 && c.fracture > 0.0 && c.fracture < amount {
            self.fractured[index] = true;
            out.detach = true;
        }
        let taken = amount.min(self.health[index].max(0.0));
        self.health[index] -= taken;
        out.effective = taken;
        out
    }

    /// Whether a component is smoking: a motor under half its hitpoints turns
    /// its smoke emitter on (`FUN_0048e0d0` → `FUN_0048e0b0` / `FUN_0048dc50`).
    /// The renderer may draw the particles; the simulation only keeps the state.
    pub fn smoking(&self, index: usize) -> bool {
        let Some(c) = self.rig.components.get(index) else {
            return false;
        };
        c.role == Role::Motor
            && self.attached[index]
            && c.hitpoints > 0.0
            && self.health[index] < 0.5 * c.hitpoints
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
