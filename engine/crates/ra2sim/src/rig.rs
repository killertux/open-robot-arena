//! The robot's component tree: what a `.bot` is actually built from.
//!
//! Every attached part is a node with a mount transform, a role (chassis, wheel,
//! motor, weapon, structure), its own geometry, and the stats the game derives
//! from the component definitions (mass, hitpoints, weapon damage, motor power).

use std::collections::BTreeMap;

use glam::{Mat4, Vec3};
use ra2_render::DrawItem;
use ra2fmt::gmi::Attachment;

/// What a component does, derived from the `.bot` class string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Chassis,
    Wheel,
    /// A spin/servo/burst motor: turns its attached subtree.
    Motor,
    /// A linear actuator (servo/burst piston).
    Piston,
    Weapon,
    Control,
    Power,
    /// Extenders, anchors, plates — anything structural.
    Structure,
}

impl Role {
    /// A component whose children hang off a joint the engine drives.
    ///
    /// A motor turns its subtree about the mount axis; a piston *slides* it along
    /// that axis (F8). Both split the child off into its own body.
    pub fn drives(self) -> bool {
        matches!(self, Role::Motor | Role::Piston)
    }

    pub fn of(class: &str, base: &str) -> Role {
        match class {
            "Chassis" => Role::Chassis,
            "Wheel" => Role::Wheel,
            "SpinMotor" | "ServoMotor" | "BurstMotor" => Role::Motor,
            "BurstPiston" | "ServoPiston" => Role::Piston,
            "Weapon" => Role::Weapon,
            "ControlBoard" => Role::Control,
            "Battery" | "AirTank" => Role::Power,
            _ => match base {
                "Weapon" => Role::Weapon,
                "SpinMotor" | "ServoMotor" | "BurstMotor" => Role::Motor,
                "ServoPiston" | "BurstPiston" => Role::Piston,
                "Wheel" => Role::Wheel,
                "ControlBoard" => Role::Control,
                _ => Role::Structure,
            },
        }
    }

    /// Structural hitpoints when the component definition does not say.
    pub fn default_hitpoints(self) -> f32 {
        match self {
            Role::Chassis => 1000.0,
            Role::Weapon => 600.0,
            Role::Motor => 350.0,
            Role::Piston => 350.0,
            Role::Wheel => 250.0,
            Role::Structure => 300.0,
            Role::Control => 200.0,
            Role::Power => 250.0,
        }
    }

    /// Contributes to the robot's collision hull and can be knocked off.
    pub fn is_physical(self) -> bool {
        !matches!(self, Role::Control)
    }
}

/// Weapon behaviour from the component definition.
#[derive(Clone, Copy, Debug)]
pub struct WeaponStats {
    /// Blunt damage factor (`concussion`).
    pub concussion: f32,
    /// Cutting damage factor (`piercing`).
    pub piercing: f32,
    /// Blade spin frequency from the definition (used for the damage rate).
    pub frequency: f32,
    /// Blade reach (component bounding radius), in metres.
    pub reach: f32,
}

/// Motor behaviour from the component definition.
#[derive(Clone, Copy, Debug)]
pub struct MotorStats {
    /// Rotor rate at full direction, rad/s: the first number of the definition's
    /// `power = <speed> <torque>` line.
    pub speed: f32,
    /// Peak motor torque in N·m: the second number, scaled by [`torque_unit`].
    pub torque: f32,
    /// Burst motor travel, radians (`start`, `end`).
    pub burst: Option<[f32; 2]>,
}

/// Wheel behaviour from the component definition.
#[derive(Clone, Debug, Default)]
pub struct WheelStats {
    /// `contact` — the geometry object the wheel's traction surface is built
    /// from. It is a second copy of the wheel with a different material, so this
    /// is the surface that actually meets the ground.
    pub contact: Option<String>,
    /// `grip` — friction of that surface.
    pub grip: f32,
    /// `resistance` — rolling resistance, in torque units; becomes the joint's
    /// friction torque.
    pub resistance: f32,
}

/// Piston behaviour from the component definition (F8).
///
/// A piston is a *prismatic* joint: the rod slides along the mount axis instead
/// of turning about it. The definition carries the travel —
/// `prismatic = <housing>, <rod>, ax, ay, az, neutral, lower, upper` — whose last
/// two numbers are already Jolt-shaped (`lower <= 0 <= upper`, measured from
/// where the two frame points coincide, which is the rest pose). Every stock
/// piston declares the axis `0, 0, 1`: the same local +Z every other RA2 axle
/// uses, which is what the mount point's Z gives us.
#[derive(Clone, Copy, Debug)]
pub struct PistonStats {
    /// Slide axis in the piston's own component space. Every stock piston says
    /// `0, 0, 1` — the same local +Z every other RA2 axle uses — but the axis is
    /// declared here, not taken from the mount point a child hangs off.
    pub axis: Vec3,
    /// Travel limits in metres, from the rest pose.
    pub limits: [f32; 2],
    /// Peak rod speed at full power (m/s): the first number of the definition's
    /// `power` pair.
    pub speed: f32,
    /// Peak rod force (N): the second number, scaled by [`torque_unit`].
    pub force: f32,
    /// A burst piston (`base = BurstPiston`): parked until something fires it,
    /// like a burst motor — see [`JointUse::Burst`].
    pub burst: bool,
}

/// One part of a robot.
pub struct Component {
    pub index: usize,
    pub parent: Option<usize>,
    /// Attachment point **on the parent** this component mounts to (`mP`).
    pub parent_mount: i32,
    /// Attachment point **on this component** used for the mount (`mC`).
    pub own_mount: i32,
    /// This component's own attachment points, keyed by the `.bot`'s ids.
    ///
    /// Not used for placement yet — the original also aligns the child's
    /// attachment frame to the parent's, which is still open (F1 in
    /// `docs/06-deferred-fidelity.md`). They are exposed so the joint/mount work
    /// has them.
    pub attachments: BTreeMap<i32, Attachment>,
    /// `.bot` class string (`SpinMotor`, `Wheel`, ...).
    pub class: String,
    /// Component definition `base` class (`Weapon`, `SpinMotor`, ...).
    pub base: String,
    /// Display name from the definition.
    pub name: String,
    /// Bot-local transform in the rest pose.
    pub local: Mat4,
    /// Visual batches (component-local).
    pub items: Vec<DrawItem>,
    /// Visual batches for the motor's rotating hub, if it has one.
    pub rotor_items: Vec<DrawItem>,
    /// Collision hull input (component-local, incl. physics proxies).
    pub hull_points: Vec<Vec3>,
    pub min: Vec3,
    pub max: Vec3,
    /// Mass in kg from the mesh's rigid bodies.
    pub mass: f32,
    /// Structural hitpoints.
    pub hitpoints: f32,
    /// `fracture` from the component definition (or its category default).
    pub fracture: f32,
    /// `material` from the component definition (`metal`, `rubber`, …).
    pub material: Option<String>,
    /// `false` when the definition says `damageable = false`.
    pub damageable: bool,
    pub role: Role,
    pub weapon: Option<WeaponStats>,
    pub motor: Option<MotorStats>,
    pub wheel: Option<WheelStats>,
    /// Linear stats when this component is a piston (F8).
    pub piston: Option<PistonStats>,
    /// The chassis' buckle model (F9): the positions the bot file marks as
    /// movable, which the original shifts "down and in" as the chassis is hit.
    /// Only the chassis has one.
    pub buckle: Vec<Vec3>,
    /// How far the chassis has already buckled (0 pristine, 1 at its last
    /// hitpoint).
    pub buckle_fraction: f32,
    /// Children component indices (motor-driven subtree).
    pub children: Vec<usize>,
}

impl Component {
    /// Bounding radius about the component origin.
    pub fn radius(&self) -> f32 {
        ((self.max - self.min) * 0.5).length()
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Half the component's own axis-aligned extent, in its local frame.
    ///
    /// The oriented box the script layer tests `plus.rayTest` against: a part is
    /// rarely a ball (a spinner's blades least of all), and this is the shape the
    /// component actually occupies.
    pub fn half_extents(&self) -> Vec3 {
        (self.max - self.min) * 0.5
    }

    /// Rolling radius of a wheel: half its extent across the axle (a wheel mesh
    /// is a disc in the component's XY plane, so the axle is Z).
    pub fn wheel_radius(&self) -> f32 {
        ((self.max.x - self.min.x).max(self.max.y - self.min.y)).abs() * 0.5
    }
}

/// How a joint's motor is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointUse {
    /// A wheel: driven from the drive input.
    Wheel,
    /// An actively spun assembly (a spinner, a flail).
    Spun,
    /// A burst motor: parked at its start angle until something triggers it.
    ///
    /// "A Burst Motor is like a spring-loaded device that slowly, but powerfully
    /// cocks the axle in a starting position and waits to be triggered" —
    /// `Tutorials/Mechanics_Tutorial.htm`. The scripts park one with `Lock(True)`
    /// — the joint's friction — and fire it with `SetPowerSettings` plus
    /// `SetDirection`. Nothing triggers one yet (that is the control wiring), so
    /// it stays parked, held by the same friction.
    Burst,
    /// A free hinge with no motor.
    Passive,
    /// A linear joint at rest: a servo piston holds its rod where it is, by the
    /// definition's force, until the control layer moves it.
    Piston,
}

/// The motor connecting a part to its parent.
#[derive(Clone, Copy, Debug)]
pub struct JointInfo {
    /// The motor component that owns the joint.
    pub motor: usize,
    /// Angular rate at full power (rad/s).
    pub rate: f32,
    /// Peak motor torque (N·m).
    pub max_torque: f32,
    /// Angle limits in radians, measured from the rest pose (`[-PI, PI]` =
    /// free).
    pub limits: [f32; 2],
    /// The joint's pivot in bot space: the parent's mount point.
    pub pivot: Vec3,
    /// The joint's rotation axis in bot space: the mount point's Z — the same
    /// axis the placement rule turns a child about (F1).
    pub axis: Vec3,
    /// Rolling resistance: torque in N·m the joint's friction may resist an
    /// undriven rotation with (a wheel's `resistance`).
    pub friction_torque: f32,
    /// A prismatic joint (a piston's rod) rather than a revolute one. Its
    /// `limits` are metres of travel, `rate` is m/s and the torques are forces.
    pub slider: bool,
    pub use_kind: JointUse,
}

/// One rigid body of the assembled robot: components welded together. A motor
/// between two components splits them into separate bodies joined by a hinge —
/// which is how the original builds its robots (the `.gmf` ships the motor as a
/// Havok constraint).
pub struct Part {
    /// The component whose frame is the body's origin.
    pub root: usize,
    /// Components welded into this body (`root` first).
    pub members: Vec<usize>,
    /// Parent part index; `None` for the chassis body.
    pub parent: Option<usize>,
    /// The joint to the parent, when a motor sits between them.
    pub joint: Option<JointInfo>,
}

/// A whole robot: the ordered component list plus its bounds.
pub struct RobotRig {
    pub name: String,
    pub components: Vec<Component>,
    pub min: Vec3,
    pub max: Vec3,
    /// Natural total mass (before any `RA2_MASS` scaling).
    pub mass: f32,
    /// The assembled rigid bodies (chassis first).
    pub parts: Vec<Part>,
    /// Component index → part index.
    pub part_of: Vec<usize>,
    /// Component index → transform within its part.
    pub part_local: Vec<Mat4>,
}

impl RobotRig {
    /// Animated bot-local transform of every component for a set of motor
    /// angles. A motor turns its rotor about the local +Z axis (RA2's axle
    /// direction), carrying its whole attached subtree.
    pub fn worlds(&self, thetas: &[f32]) -> Vec<Mat4> {
        let mut out = vec![Mat4::IDENTITY; self.components.len()];
        for (i, c) in self.components.iter().enumerate() {
            let parent = match c.parent {
                Some(p) => out[p],
                None => Mat4::IDENTITY,
            };
            out[i] = parent * c.local;
        }
        // Second pass: rotate each motor's subtree about the motor frame. Doing
        // it after the base pass keeps the "static body" transform for the
        // motor itself and gives children the rotor frame.
        for (i, c) in self.components.iter().enumerate() {
            if c.motor.is_none() {
                continue;
            }
            let rotor =
                out[i] * Mat4::from_axis_angle(Vec3::Z, thetas.get(i).copied().unwrap_or(0.0));
            self.apply_rotor(i, rotor, &mut out);
        }
        out
    }

    fn apply_rotor(&self, index: usize, rotor: Mat4, out: &mut [Mat4]) {
        for &child in &self.components[index].children {
            let c = &self.components[child];
            let base = rotor * c.local;
            out[child] = base;
            // A nested motor's own rotor is applied by the outer pass; parents
            // are always visited before their children, so that is safe.
            if c.motor.is_none() {
                self.apply_rotor(child, base, out);
            }
        }
    }

    /// Rotor frame for motor `index` at angle `theta`.
    pub fn rotor_world(&self, worlds: &[Mat4], index: usize, theta: f32) -> Mat4 {
        worlds[index] * Mat4::from_axis_angle(Vec3::Z, theta)
    }
}

/// The mesh object that hosts a component's `attaching` point — a motor's rotor
/// hub (`attaching = axle` on every stock motor). Falls back to the first
/// attachment that declares a host.
fn attach_host(atts: &BTreeMap<i32, Attachment>, attaching: &str) -> Option<String> {
    let want = attaching.trim().to_ascii_lowercase();
    let mut fallback = None;
    for a in atts.values() {
        let Some(host) = &a.host else { continue };
        if fallback.is_none() {
            fallback = Some(host.clone());
        }
        if want.is_empty() {
            continue;
        }
        if a.types
            .iter()
            .any(|t| t.starts_with(&want) || want.starts_with(t.as_str()))
        {
            return Some(host.clone());
        }
    }
    fallback
}

/// How many newton-metres one unit of a component's `power`/`resistance` is
/// worth.
///
/// A definition's `power` line is `power = <speed> <torque>`, the same pair the
/// scripts set with `hinge.SetPowerSettings(speed, torque)` — the shipped arenas
/// use it directly (`Arenas/Hazards.py`: `SetPowerSettings(95, 15000)` to smash,
/// `SetPowerSettings(12, 700)` to retract). Its implementation multiplies only
/// the torque, by 40, before storing it (`0x4810e0`: `this->torque = t` and
/// `this->max_torque = t * 40`), while the speed is stored as given. So the
/// speed is already rad/s and the torque needs this factor to become N·m.
///
/// The check that it is right: `Z-Tek Motor (HP)` declares `36 48`, i.e. 1920
/// N·m, and the guess this replaces — which was the only reason a wheeled robot
/// could move at all — was 1600 N·m.
///
/// Overridable with `RA2_TORQUE_UNIT`.
fn torque_unit() -> f32 {
    use std::sync::OnceLock;
    static SCALE: OnceLock<f32> = OnceLock::new();
    *SCALE.get_or_init(|| {
        std::env::var("RA2_TORQUE_UNIT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(40.0)
    })
}

/// The two numbers of a definition's `power = <speed> <torque>` line.
/// A piston's travel and power, from its definition (F8).
///
/// `prismatic = <housing>, <rod>, ax, ay, az, neutral, lower, upper` — the two
/// object names are not numeric, so a plain number scan lands on the axis
/// `[0..3]` first, then the neutral point and the two travel limits. The names
/// are real objects in the component mesh: `burstpiston1.gmf` carries
/// `rb "body"` (14 kg) and `rb "shaft"` (8 kg), and the children mount on
/// `Host = shaft`, so the rod is the moving half.
///
/// `lower <= 0 <= upper` is already the shape a Jolt slider wants, measured from
/// the rest pose.
fn piston_stats(def: &ra2fmt::component::ComponentDef, burst: bool) -> PistonStats {
    let nums: Vec<f32> = def
        .get("prismatic")
        .map(|v| {
            v.split(',')
                .filter_map(|t| t.trim().parse::<f32>().ok())
                .collect()
        })
        .unwrap_or_default();
    let axis = if nums.len() >= 3 {
        Vec3::new(nums[0], nums[1], nums[2])
    } else {
        Vec3::Z
    };
    let limits = if nums.len() >= 6 {
        [nums[4], nums[5]]
    } else {
        // No usable travel: a barely-moving rod still beats a weld, and keeps
        // the rod a body of its own.
        [-0.05, 0.05]
    };
    let (speed, force) = power_pair(def);
    PistonStats {
        axis: if axis == Vec3::ZERO { Vec3::Z } else { axis },
        limits,
        speed: speed.unwrap_or(0.0),
        force: force.unwrap_or(0.0) * torque_unit(),
        burst,
    }
}

fn power_pair(def: &ra2fmt::component::ComponentDef) -> (Option<f32>, Option<f32>) {
    let Some(v) = def.get("power") else {
        return (None, None);
    };
    let mut it = v.split_whitespace().filter_map(|t| t.parse::<f32>().ok());
    (it.next(), it.next())
}

fn num(def: &ra2fmt::component::ComponentDef, key: &str) -> Option<f32> {
    def.get(key)?.split_whitespace().next()?.parse().ok()
}

/// The definition's `type` category, lower-cased.
fn category(def: Option<&ra2fmt::component::ComponentDef>) -> Option<String> {
    def.and_then(|d| d.get("type")).map(|s| {
        s.split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
    })
}

/// The maximum hitpoints the component factory gives a category when the
/// definition does not declare them.
///
/// Recovered from the binary: `FUN_0045cf40` switches on the definition's `type`
/// (the field the `.txt` parser fills from the `type = ` key, read back by
/// `FUN_0049aa30`) and stores 400 / 100 / 400 / 400 / 1000 / 1000 into the
/// component's maximum, after which a `hitpoints = ` line overrides it.
fn type_hitpoints(def: Option<&ra2fmt::component::ComponentDef>, role: Role) -> f32 {
    match category(def).as_deref() {
        Some("power") => 400.0,
        Some("extenders") => 100.0,
        Some("mechanics") => 400.0,
        Some("mobility") => 400.0,
        Some("weapons") => 1000.0,
        Some("extras") => 1000.0,
        // Unknown category: the factory sets nothing, so fall back to the role.
        _ => role.default_hitpoints(),
    }
}

/// The `fracture` default the same factory sets per category.
fn type_fracture(def: Option<&ra2fmt::component::ComponentDef>) -> f32 {
    match category(def).as_deref() {
        Some("extenders") => 15.0,
        Some("mechanics") | Some("mobility") | Some("weapons") => 20.0,
        Some("extras") => 60.0,
        _ => 0.0,
    }
}

fn flag_false(def: &ra2fmt::component::ComponentDef, key: &str) -> bool {
    def.get(key)
        .map(|v| v.trim().eq_ignore_ascii_case("false"))
        .unwrap_or(false)
}

/// Group the components into welded bodies and work out the joints between them.
///
/// A component is welded into its parent's body unless the parent is a motor, in
/// which case it starts a new body joined by a hinge — the structure the `.gmf`
/// motor constraints describe.
fn assemble(comps: &[Component]) -> (Vec<Part>, Vec<usize>, Vec<Mat4>) {
    let n = comps.len();

    // The body that owns component `i`: walk up until the parent is a motor or
    // there is no parent.
    let root_of = |mut i: usize| -> usize {
        loop {
            match comps[i].parent {
                None => return i,
                Some(p) if comps[p].role.drives() => return i,
                Some(p) => i = p,
            }
        }
    };

    let mut part_of = vec![usize::MAX; n];
    let mut parts: Vec<Part> = Vec::new();
    for i in 0..n {
        let r = root_of(i);
        if part_of[r] == usize::MAX {
            part_of[r] = parts.len();
            parts.push(Part {
                root: r,
                members: Vec::new(),
                parent: None,
                joint: None,
            });
        }
        let p = part_of[r];
        part_of[i] = p;
        parts[p].members.push(i);
    }

    for part in parts.iter_mut() {
        let root = part.root;
        let Some(parent) = comps[root].parent else {
            continue;
        };
        let parent_part = part_of[parent];
        part.parent = Some(parent_part);

        // A joint belongs to the parent: a motor turns its subtree about the
        // mount axis, a piston slides it along that axis (F8).
        let piston = comps[parent].piston;
        let motor = comps[parent].motor;
        if motor.is_none() && piston.is_none() {
            // A split that carries no joint data of its own: weld the child group
            // back on rather than leave it floating.
            part.parent = None;
            continue;
        };
        let is_wheel = part.members.iter().any(|&i| comps[i].role == Role::Wheel);
        // The joint turns about the parent's mount point — the same frame the
        // placement rule turns a child about (F1): its Z is the axis and its
        // origin the pivot. The motor's own frame is the fallback, for a point
        // no mesh carries.
        let parent_local = comps[parent].local;
        let mount = comps[parent]
            .attachments
            .get(&comps[root].parent_mount)
            .filter(|a| a.transform[2] != [0.0; 3]);
        let pivot = match mount {
            Some(a) => parent_local.transform_point3(Vec3::from(a.transform[3])),
            None => parent_local.transform_point3(Vec3::ZERO),
        };
        let axis = match mount {
            Some(a) => parent_local.transform_vector3(Vec3::from(a.transform[2])),
            None => parent_local.transform_vector3(Vec3::Z),
        }
        .normalize_or_zero();
        let axis = if axis == Vec3::ZERO { Vec3::Z } else { axis };
        // A piston declares its own slide axis in component space — the mount
        // point's Z is where a child hangs off, not where the rod travels (F8).
        let axis = match piston {
            Some(p) => parent_local.transform_vector3(p.axis).normalize_or_zero(),
            None => axis,
        };
        let axis = if axis == Vec3::ZERO { Vec3::Z } else { axis };
        // A burst motor is a limited hinge; everything else swings freely. Its
        // `[start, end]` are the axle's absolute angles, and the rest pose
        // leaves the axle at `start`, so the constraint has to allow the travel
        // from there to `end`. Jolt measures the hinge angle from the pose the
        // constraint is created in and requires the range to straddle zero, so
        // the limits are that travel, split about zero.
        //
        // A piston's travel comes straight from its definition, already in that
        // shape (metres either side of the rest pose).
        let (rate, max_torque, limits, friction_torque, use_kind) = if let Some(p) = piston {
            (
                p.speed,
                p.force,
                p.limits,
                // Parked, like a burst motor: the definition's force is what
                // holds the rod against a load until the control layer moves it.
                p.force,
                if p.burst {
                    JointUse::Burst
                } else {
                    JointUse::Piston
                },
            )
        } else {
            let m = motor.expect("checked above");
            let limits = match m.burst {
                Some([start, end]) => {
                    let d = (end - start).clamp(-std::f32::consts::PI, std::f32::consts::PI);
                    [d.min(0.0), d.max(0.0)]
                }
                None => [-std::f32::consts::PI, std::f32::consts::PI],
            };
            // A wheel's `resistance` is rolling resistance: the torque the
            // joint's friction may oppose an undriven rotation with. A parked
            // burst motor is held by the same friction at its own torque — the
            // scripts' `Lock(True)`.
            let friction_torque = if is_wheel {
                comps[root]
                    .wheel
                    .as_ref()
                    .map(|w| w.resistance * torque_unit())
                    .unwrap_or(0.0)
            } else if m.burst.is_some() {
                m.torque
            } else {
                0.0
            };
            let use_kind = if is_wheel {
                JointUse::Wheel
            } else if m.burst.is_some() {
                JointUse::Burst
            } else {
                JointUse::Spun
            };
            (m.speed, m.torque, limits, friction_torque, use_kind)
        };
        part.joint = Some(JointInfo {
            motor: parent,
            rate,
            max_torque,
            friction_torque,
            limits,
            pivot,
            axis,
            slider: piston.is_some(),
            use_kind,
        });
    }

    // Any part still without a joint had no motor and no piston behind it, so
    // re-weld its members into the parent rather than leave them floating.
    for _ in 0..parts.len() {
        for p in 0..parts.len() {
            if parts[p].root == 0 || parts[p].parent.is_some() || parts[p].members.is_empty() {
                continue;
            }
            let root = parts[p].root;
            let Some(parent) = comps[root].parent else {
                continue;
            };
            let into = part_of[parent];
            if into == p {
                continue;
            }
            let moved = std::mem::take(&mut parts[p].members);
            for i in moved {
                part_of[i] = into;
                parts[into].members.push(i);
            }
        }
    }

    let part_local = (0..n)
        .map(|i| {
            let r = parts[part_of[i]].root;
            comps[r].local.inverse() * comps[i].local
        })
        .collect();

    (parts, part_of, part_local)
}

/// Load a `.bot` into a component tree (no physics yet).
pub fn load(path: &str) -> Result<RobotRig, Box<dyn std::error::Error>> {
    let (bot, _root, loaded) = ra2_render::load_bot_components(path)?;
    let rest =
        ra2_render::component_worlds_mounted(&bot, &loaded, ra2_render::MountMode::from_env());
    let mut components: Vec<Component> = Vec::with_capacity(bot.components.len());

    // The chassis' crush cage (F9) is the bot's, not a component definition's.
    let chassis_buckle: Vec<Vec3> = bot
        .components
        .first()
        .and_then(|c| c.chassis.as_ref())
        .map(|ch| ch.buckling.iter().map(|b| Vec3::from(b.position)).collect())
        .unwrap_or_default();

    for (i, c) in bot.components.iter().enumerate() {
        let parent = if i > 0 && c.parent >= 0 && (c.parent as usize) < i {
            Some(c.parent as usize)
        } else {
            None
        };

        // Chassis geometry is embedded in the `.bot`; everything else comes from
        // a `.gmf`.
        let (items, hull_points, rotor_items, def, base, name, mass, attachments) = if i == 0 {
            let chassis = bot.components.first().and_then(|c| c.chassis.as_ref());
            let points = chassis
                .and_then(|ch| ch.mesh.as_ref())
                .map(|m| m.points.iter().map(|p| Vec3::from(*p)).collect::<Vec<_>>())
                .unwrap_or_default();
            // The chassis is not a `.gmf`: its mass is the armour weight per
            // square unit times the chassis mesh's surface area, exactly as the
            // bot format defines it.
            (
                ra2_render::chassis_items(&bot, Mat4::IDENTITY),
                points,
                Vec::new(),
                None,
                "Chassis".to_string(),
                "Chassis".to_string(),
                chassis.map(|ch| ch.mass()).unwrap_or(0.0).max(5.0),
                BTreeMap::new(),
            )
        } else if let Some(lc) = loaded.get(i).and_then(|l| l.as_ref()) {
            let items = ra2_render::scene_items(&lc.scene, Mat4::IDENTITY, &lc.dir, &lc.label);
            let points = ra2_render::scene_points(&lc.scene, true);
            let attachments = lc.scene.attachment_table();
            let rotor = attach_host(&attachments, lc.def.get("attaching").unwrap_or(""))
                .map(|host| ra2_render::object_scene_items(&lc.scene, &host, &lc.dir))
                .unwrap_or_default();
            let mass = if let Some(m) = num(&lc.def, "mass") {
                m
            } else {
                let sum: f32 = lc
                    .scene
                    .objects
                    .iter()
                    .flat_map(|o| &o.rigid_bodies)
                    .map(|rb| rb.mass)
                    .sum();
                if sum > 0.0 {
                    sum
                } else {
                    8.0
                }
            };
            (
                items,
                points,
                rotor,
                Some(lc.def.clone()),
                lc.def.class.clone(),
                lc.def.name.clone(),
                mass,
                attachments,
            )
        } else {
            (
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                String::new(),
                c.class.clone(),
                0.0,
                BTreeMap::new(),
            )
        };

        // NOTE: a wheel's `contact` names the geometry object carrying the
        // traction material, and `passthru` names the one excluded from
        // collision, so the game does not collide the visible hub. Making the
        // collider that `contact` object instead measurably *hurts*: the shipped
        // traction meshes are not always big enough to hold the robot up, and
        // `AI/Team10/Bot2` goes from driving 5.5 m to not moving at all. Until
        // the two object sets are separated properly (F4), the whole scene hull
        // is used.

        let role = Role::of(&c.class, &base);
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for p in &hull_points {
            min = min.min(*p);
            max = max.max(*p);
        }
        if min.x > max.x {
            min = Vec3::ZERO;
            max = Vec3::ZERO;
        }

        // Chassis hitpoints are the armour hardness the bot file records. Every
        // other component's come from the game's own rule: the component factory
        // (`FUN_0045cf40`) switches on the definition's `type` and stores a
        // default maximum, then a `hitpoints =` line in the definition overrides
        // it. Recovered from the binary: power 400, extenders 100, mechanics 400,
        // mobility 400, weapons 1000, extras 1000 — see F7.
        let chassis_armour = bot
            .components
            .first()
            .and_then(|c| c.chassis.as_ref())
            .and_then(|c| c.armour);
        let hitpoints = if i == 0 {
            chassis_armour
                .map(|a| a.hardness)
                .unwrap_or_else(|| role.default_hitpoints())
        } else {
            def.as_ref()
                .and_then(|d| num(d, "hitpoints"))
                .unwrap_or_else(|| type_hitpoints(def.as_ref(), role))
        };
        // `fracture` is a per-category default too (extenders 15, mechanics and
        // mobility 20, weapons 20, extras 60) overridden by the definition.
        let fracture = def
            .as_ref()
            .and_then(|d| num(d, "fracture"))
            .unwrap_or_else(|| type_fracture(def.as_ref()));
        let material = def
            .as_ref()
            .and_then(|d| d.get("material"))
            .map(|s| s.to_ascii_lowercase());
        let damageable = def
            .as_ref()
            .map(|d| !flag_false(d, "damageable"))
            .unwrap_or(true);
        let piston = (role == Role::Piston).then(|| {
            piston_stats(
                def.as_ref().expect("a piston has a definition"),
                base.eq_ignore_ascii_case("BurstPiston"),
            )
        });
        let weapon = (role == Role::Weapon).then(|| WeaponStats {
            concussion: def
                .as_ref()
                .and_then(|d| num(d, "concussion"))
                .unwrap_or(0.5),
            piercing: def.as_ref().and_then(|d| num(d, "piercing")).unwrap_or(0.5),
            frequency: def
                .as_ref()
                .and_then(|d| num(d, "frequency"))
                .unwrap_or(4.0),
            reach: ((max - min) * 0.5).length().max(0.1),
        });

        let motor = (role == Role::Motor).then(|| {
            // `power = <speed> <torque>`, the pair the scripts set through
            // `SetPowerSettings`. The speed is rad/s as shipped; the torque is in
            // torque units and becomes N·m through `torque_unit`.
            let (speed, torque) = def.as_ref().map(power_pair).unwrap_or((None, None));
            MotorStats {
                speed: speed.unwrap_or(20.0),
                torque: torque.unwrap_or(20.0) * torque_unit(),
                burst: bot.components[i].burst_angles,
            }
        });

        let wheel = (role == Role::Wheel).then(|| {
            let d = def.as_ref();
            WheelStats {
                contact: d
                    .and_then(|d| d.get("contact"))
                    .map(|s| s.trim().to_string()),
                grip: d.and_then(|d| num(d, "grip")).unwrap_or(1.0),
                resistance: d.and_then(|d| num(d, "resistance")).unwrap_or(0.0),
            }
        });

        components.push(Component {
            index: i,
            parent,
            parent_mount: c.parent_mount,
            own_mount: c.own_mount,
            attachments,
            class: c.class.clone(),
            base,
            name,
            local: rest[i],
            items,
            rotor_items,
            hull_points,
            min,
            max,
            mass,
            hitpoints,
            fracture,
            material,
            damageable,
            role,
            weapon,
            motor,
            wheel,
            piston,
            // The chassis' crush cage (F9). Its points name the chassis mesh's
            // own space, which for component 0 is bot space.
            buckle: if i == 0 {
                chassis_buckle.clone()
            } else {
                Vec::new()
            },
            buckle_fraction: 0.0,
            children: Vec::new(),
        });
    }

    // Link children.
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); components.len()];
    for (i, c) in components.iter().enumerate() {
        if let Some(p) = c.parent {
            children[p].push(i);
        }
    }
    for (i, kids) in children.into_iter().enumerate() {
        components[i].children = kids;
    }

    // Robot bounds and natural mass.
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    let mut mass = 0.0f32;
    for c in &components {
        if c.hull_points.is_empty() {
            continue;
        }
        let world = c.local;
        for p in &c.hull_points {
            let p = world.transform_point3(*p);
            min = min.min(p);
            max = max.max(p);
        }
        mass += c.mass;
    }
    if min.x > max.x {
        min = Vec3::ZERO;
        max = Vec3::ZERO;
    }

    let (parts, part_of, part_local) = assemble(&components);

    Ok(RobotRig {
        name: bot.name.clone(),
        components,
        min,
        max,
        mass: mass.max(1.0),
        parts,
        part_of,
        part_local,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_come_from_the_bot_class_then_the_definition_base() {
        assert_eq!(Role::of("Chassis", "Chassis"), Role::Chassis);
        assert_eq!(Role::of("Wheel", "Wheel"), Role::Wheel);
        assert_eq!(Role::of("SpinMotor", "SpinMotor"), Role::Motor);
        assert_eq!(Role::of("BurstMotor", "BurstMotor"), Role::Motor);
        assert_eq!(Role::of("ServoMotor", "ServoMotor"), Role::Motor);
        assert_eq!(Role::of("Weapon", "Weapon"), Role::Weapon);
        // `.bot` class can lag the definition; the base class wins for unknowns.
        assert_eq!(Role::of("Custom", "Weapon"), Role::Weapon);
        assert_eq!(Role::of("Component", "Component"), Role::Structure);
        assert_eq!(Role::of("ControlBoard", "ControlBoard"), Role::Control);
    }

    /// F7: component hitpoints come from the game's own rule, not from a guess.
    ///
    /// The component factory (`FUN_0045cf40`) switches on the definition's `type`
    /// and stores a default maximum; a `hitpoints =` line then overrides it.
    /// Recovered from the binary, so the table below is the original's.
    #[test]
    fn hitpoints_follow_the_factorys_category_table() {
        let hp = |text: &str| {
            let d = ra2fmt::component::parse(text);
            d.get("hitpoints")
                .and_then(|v| v.trim().parse::<f32>().ok())
                .unwrap_or_else(|| type_hitpoints(Some(&d), Role::Structure))
        };
        assert_eq!(hp("type = power\n"), 400.0);
        assert_eq!(hp("type = extenders\n"), 100.0);
        assert_eq!(hp("type = mechanics\n"), 400.0);
        assert_eq!(hp("type = mobility\n"), 400.0);
        assert_eq!(hp("type = weapons\n"), 1000.0);
        assert_eq!(hp("type = extras\n"), 1000.0);
        // A declared value wins (the ramplate declares 1000, a disc 500).
        assert_eq!(hp("type = weapons\nhitpoints = 500\n"), 500.0);
        // `fracture` has category defaults of its own.
        assert_eq!(
            type_fracture(Some(&ra2fmt::component::parse("type = extenders\n"))),
            15.0
        );
        assert_eq!(
            type_fracture(Some(&ra2fmt::component::parse("type = weapons\n"))),
            20.0
        );
        assert_eq!(
            type_fracture(Some(&ra2fmt::component::parse("type = extras\n"))),
            60.0
        );
        assert_eq!(
            type_fracture(Some(&ra2fmt::component::parse("type = power\n"))),
            0.0
        );
    }

    /// F8: a piston's child hangs off a prismatic joint, not the parent's body.
    ///
    /// The definition's `prismatic` line carries the travel, and the axis is in
    /// the piston's own space — `burstpiston1.txt` says
    /// `body, shaft, 0, 0, 1, 0.0, -0.60, .8`, which is a rod that slides along
    /// its local +Z from 0.60 back to 0.80 out.
    #[test]
    fn piston_travel_comes_from_the_prismatic_line() {
        let d = ra2fmt::component::parse(
            "prismatic = body, shaft, 0, 0, 1, 0.0, -0.60, .8\npower = 60 100\n",
        );
        let p = piston_stats(&d, true);
        assert_eq!(p.axis, Vec3::Z);
        assert_eq!(p.limits, [-0.60, 0.8]);
        assert_eq!(p.speed, 60.0);
        assert_eq!(p.force, 100.0 * torque_unit());
        assert!(p.burst);

        // No usable line at all still yields a joint, and one that moves.
        let p = piston_stats(&ra2fmt::component::parse("name = piston\n"), false);
        assert_eq!(p.axis, Vec3::Z);
        assert!(p.limits[0] < 0.0 && p.limits[1] > 0.0);
        assert!(!p.burst);
    }

    /// The assembled robot splits the rod off into a body of its own, and the
    /// joint carries the definition's limits rather than a hinge's ±π.
    #[test]
    fn a_piston_slides_its_child_instead_of_welding_it() {
        // chassis → piston → rod.
        let mut piston = part(1, Some(0), Mat4::IDENTITY, false);
        piston.role = Role::Piston;
        piston.piston = Some(PistonStats {
            axis: Vec3::Z,
            limits: [-0.73, 0.2],
            speed: 1.0,
            force: 1200.0,
            burst: false,
        });
        let components = vec![
            part(0, None, Mat4::IDENTITY, false),
            piston,
            part(2, Some(1), Mat4::IDENTITY, false),
        ];
        let (parts, part_of, _) = assemble(&components);

        assert_eq!(parts.len(), 2, "the rod is a body of its own");
        assert_eq!(part_of[1], part_of[0], "the housing stays on the chassis");
        assert_eq!(part_of[2], 1, "the rod is that body");
        let joint = parts[1].joint.expect("the rod is jointed");
        assert!(joint.slider, "the joint is prismatic");
        assert_eq!(parts[1].parent, Some(0));
        assert_eq!(joint.limits, [-0.73, 0.2]);
        assert_eq!(joint.rate, 1.0, "speed is the power pair's first number");
        assert_eq!(joint.max_torque, 1200.0);
        assert_eq!(
            joint.friction_torque, 1200.0,
            "parked: its own force holds it"
        );
        assert_eq!(joint.use_kind, JointUse::Piston);
        assert!(parts[0].joint.is_none(), "the chassis has no joint");
    }

    /// A definition's `power` line is `<speed> <torque>`, and the torque needs
    /// the factor the hinge's own `SetPowerSettings` applies.
    ///
    /// The shipped numbers come from the real definitions: `Z-Tek Motor (HP)`
    /// declares `36 48`, which is 36 rad/s and 1920 N·m — within 20% of the 1600
    /// N·m guess it replaces, which is why the old code could move a robot at all.
    #[test]
    fn motor_power_is_a_speed_and_a_torque() {
        let def = ra2fmt::component::parse("base = SpinMotor\npower = 36 48\n");
        let (speed, torque) = power_pair(&def);
        assert_eq!(speed, Some(36.0));
        assert_eq!(torque, Some(48.0));
        assert_eq!(torque.unwrap() * torque_unit(), 1920.0);

        // A definition with no `power` line (or a single number) cannot panic.
        assert_eq!(
            power_pair(&ra2fmt::component::parse("base = Wheel\n")),
            (None, None)
        );
        let (s, t) = power_pair(&ra2fmt::component::parse("power = 14\n"));
        assert_eq!((s, t), (Some(14.0), None));
    }

    fn part(index: usize, parent: Option<usize>, local: Mat4, motor: bool) -> Component {
        Component {
            index,
            parent,
            parent_mount: 0,
            own_mount: 0,
            attachments: BTreeMap::new(),
            class: String::new(),
            base: String::new(),
            name: String::new(),
            local,
            items: Vec::new(),
            rotor_items: Vec::new(),
            hull_points: Vec::new(),
            min: Vec3::ZERO,
            max: Vec3::ZERO,
            mass: 1.0,
            hitpoints: 1.0,
            fracture: 0.0,
            material: None,
            damageable: true,
            role: if motor { Role::Motor } else { Role::Structure },
            weapon: None,
            motor: motor.then_some(MotorStats {
                speed: 10.0,
                torque: 400.0,
                burst: None,
            }),
            wheel: None,
            piston: None,
            buckle: Vec::new(),
            buckle_fraction: 0.0,
            children: Vec::new(),
        }
    }

    #[test]
    fn a_motor_turns_its_children_but_not_itself() {
        // chassis → motor → child, with the child 1 m along +X of the motor.
        let components = vec![
            part(0, None, Mat4::IDENTITY, false),
            part(1, Some(0), Mat4::IDENTITY, true),
            part(
                2,
                Some(1),
                Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0)),
                false,
            ),
        ];
        let (parts, part_of, part_local) = assemble(&components);
        let mut rig = RobotRig {
            name: "t".into(),
            components,
            min: Vec3::ZERO,
            max: Vec3::ZERO,
            mass: 1.0,
            parts,
            part_of,
            part_local,
        };
        rig.components[1].children = vec![2];

        let rest = rig.worlds(&[0.0, 0.0, 0.0]);
        let spun = rig.worlds(&[0.0, std::f32::consts::FRAC_PI_2, 0.0]);

        // The motor body itself does not move.
        assert_eq!(rest[1], spun[1]);
        // The child is carried a quarter turn about the motor's Z axis.
        assert!(rest[2]
            .transform_point3(Vec3::ZERO)
            .abs_diff_eq(Vec3::new(1.0, 0.0, 0.0), 1e-5));
        assert!(spun[2]
            .transform_point3(Vec3::ZERO)
            .abs_diff_eq(Vec3::new(0.0, 1.0, 0.0), 1e-5));
    }
}
