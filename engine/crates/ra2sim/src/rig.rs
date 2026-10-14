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
    /// Pistons are *not* included yet: their rod stays welded to the body (see
    /// `docs/06-deferred-fidelity.md`, F8).
    pub fn drives(self) -> bool {
        self == Role::Motor
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
    /// Peak `power` from the definition.
    pub power: f32,
    /// Rotor rate at full power (rad/s).
    pub rate: f32,
    /// Peak motor torque (N·m), derived from `power`.
    pub torque: f32,
    /// Burst motor travel, radians (`start`, `end`).
    pub burst: Option<[f32; 2]>,
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
    /// `false` when the definition says `damageable = false`.
    pub damageable: bool,
    pub role: Role,
    pub weapon: Option<WeaponStats>,
    pub motor: Option<MotorStats>,
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
    /// A free hinge with no motor.
    Passive,
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

/// How many newton-metres one unit of a component's `power` is worth.
///
/// The original's units are not recovered (see the validation report); this is
/// the tuning knob, overridable with `RA2_MOTOR_TORQUE`.
fn motor_torque_scale() -> f32 {
    use std::sync::OnceLock;
    static SCALE: OnceLock<f32> = OnceLock::new();
    *SCALE.get_or_init(|| {
        std::env::var("RA2_MOTOR_TORQUE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(14.0)
    })
}

fn num(def: &ra2fmt::component::ComponentDef, key: &str) -> Option<f32> {
    def.get(key)?.split_whitespace().next()?.parse().ok()
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

        let Some(m) = comps[parent].motor else {
            // The split came from a piston, whose rod we do not joint yet: weld
            // it back onto the parent so nothing floats free.
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
        // A burst motor is a limited hinge; everything else swings freely. Its
        // `[start, end]` are the axle's absolute angles, and the rest pose
        // leaves the axle at `start`, so the constraint has to allow the travel
        // from there to `end`. Jolt measures the hinge angle from the pose the
        // constraint is created in and requires the range to straddle zero, so
        // the limits are that travel, split about zero.
        let limits = match m.burst {
            Some([start, end]) => {
                let d = (end - start).clamp(-std::f32::consts::PI, std::f32::consts::PI);
                [d.min(0.0), d.max(0.0)]
            }
            None => [-std::f32::consts::PI, std::f32::consts::PI],
        };
        part.joint = Some(JointInfo {
            motor: parent,
            rate: m.rate,
            max_torque: m.torque,
            limits,
            pivot,
            axis,
            use_kind: if is_wheel {
                JointUse::Wheel
            } else {
                JointUse::Spun
            },
        });
    }

    // A piston-split part has no joint, so re-weld its members into the parent.
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

        // Chassis hitpoints are the armour hardness the bot file records; other
        // components only declare `hitpoints` when they differ from stock.
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
                .unwrap_or_else(|| role.default_hitpoints())
        };
        let damageable = def
            .as_ref()
            .map(|d| !flag_false(d, "damageable"))
            .unwrap_or(true);

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
            let power = def.as_ref().and_then(|d| num(d, "power")).unwrap_or(20.0);
            let burst = bot.components[i].burst_angles;
            MotorStats {
                power,
                rate: power.clamp(4.0, 60.0),
                torque: power * motor_torque_scale(),
                burst,
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
            damageable,
            role,
            weapon,
            motor,
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
            damageable: true,
            role: if motor { Role::Motor } else { Role::Structure },
            weapon: None,
            motor: motor.then_some(MotorStats {
                power: 10.0,
                rate: 10.0,
                torque: 100.0,
                burst: None,
            }),
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
