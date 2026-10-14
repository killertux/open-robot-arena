//! The robot's component tree: what a `.bot` is actually built from.
//!
//! Every attached part is a node with a mount transform, a role (chassis, wheel,
//! motor, weapon, structure), its own geometry, and the stats the game derives
//! from the component definitions (mass, hitpoints, weapon damage, motor power).

use glam::{Mat4, Vec3};
use ra2_render::DrawItem;

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
    /// Burst motor travel, radians (`start`, `end`).
    pub burst: Option<[f32; 2]>,
}

/// One part of a robot.
pub struct Component {
    pub index: usize,
    pub parent: Option<usize>,
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
}

/// A whole robot: the ordered component list plus its bounds.
pub struct RobotRig {
    pub name: String,
    pub components: Vec<Component>,
    pub min: Vec3,
    pub max: Vec3,
    /// Natural total mass (before any `RA2_MASS` scaling).
    pub mass: f32,
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

/// Split a `.gmf` attachment payload into key/value pairs. Fields are one
/// `Key = Value` per line (CRLF), e.g.
/// `Type = Attach\r\nID = 4\r\nAttach = Axle_M\r\nHost = Axle\r\n...`.
fn attach_fields(data: &str) -> Vec<(String, String)> {
    data.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let (k, v) = line.split_once('=')?;
            Some((k.trim().to_ascii_lowercase(), v.trim().to_string()))
        })
        .collect()
}

fn attach_host(scene: &ra2fmt::gmi::GmiScene, attaching: &str) -> Option<String> {
    let want = attaching.trim().to_ascii_lowercase();
    let mut fallback = None;
    for a in scene.attachments() {
        let fields = attach_fields(&a.data);
        let get = |k: &str| {
            fields
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        let host = match get("host") {
            Some(h) if !h.is_empty() => h.to_string(),
            _ => continue,
        };
        if fallback.is_none() {
            fallback = Some(host.clone());
        }
        if want.is_empty() {
            continue;
        }
        let types = get("attach").unwrap_or("").to_ascii_lowercase();
        if types
            .split(',')
            .map(|t| t.trim())
            .any(|t| !t.is_empty() && (t.starts_with(&want) || want.starts_with(t)))
        {
            return Some(host);
        }
    }
    fallback
}

fn num(def: &ra2fmt::component::ComponentDef, key: &str) -> Option<f32> {
    def.get(key)?.split_whitespace().next()?.parse().ok()
}

fn flag_false(def: &ra2fmt::component::ComponentDef, key: &str) -> bool {
    def.get(key)
        .map(|v| v.trim().eq_ignore_ascii_case("false"))
        .unwrap_or(false)
}

/// Load a `.bot` into a component tree (no physics yet).
pub fn load(path: &str) -> Result<RobotRig, Box<dyn std::error::Error>> {
    let (bot, _root, loaded) = ra2_render::load_bot_components(path)?;
    let rest = ra2_render::component_worlds(&bot);
    let mut components: Vec<Component> = Vec::with_capacity(bot.components.len());

    for (i, c) in bot.components.iter().enumerate() {
        let parent = if i > 0 && c.parent >= 0 && (c.parent as usize) < i {
            Some(c.parent as usize)
        } else {
            None
        };

        // Chassis geometry is embedded in the `.bot`; everything else comes from
        // a `.gmf`.
        let (items, hull_points, rotor_items, def, base, name, mass) = if i == 0 {
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
            )
        } else if let Some(lc) = loaded.get(i).and_then(|l| l.as_ref()) {
            let items = ra2_render::scene_items(&lc.scene, Mat4::IDENTITY, &lc.dir, &lc.label);
            let points = ra2_render::scene_points(&lc.scene, true);
            let rotor = attach_host(&lc.scene, lc.def.get("attaching").unwrap_or(""))
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
                burst,
            }
        });

        components.push(Component {
            index: i,
            parent,
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

    Ok(RobotRig {
        name: bot.name.clone(),
        components,
        min,
        max,
        mass: mass.max(1.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_crlf_attachment_fields() {
        let data = "Type = Attach\r\nID = 4\r\nAttach = Axle_M\r\nHost = Axle\r\n\r\nMass = 0.000000\r\nUnyielding = 0";
        let f = attach_fields(data);
        let get = |k: &str| f.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str());
        assert_eq!(get("type"), Some("Attach"));
        assert_eq!(get("id"), Some("4"));
        assert_eq!(get("attach"), Some("Axle_M"));
        assert_eq!(get("host"), Some("Axle"));
        assert_eq!(get("unyielding"), Some("0"));
        assert_eq!(f.len(), 6);
    }

    #[test]
    fn attachment_without_host_is_ignored() {
        let data = "Type = Attach\r\nID = 1\r\nAttach = Base_F, Generic_F";
        let f = attach_fields(data);
        assert!(f.iter().all(|(k, _)| k != "host"));
    }

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
                burst: None,
            }),
            children: Vec::new(),
        }
    }

    #[test]
    fn a_motor_turns_its_children_but_not_itself() {
        // chassis → motor → child, with the child 1 m along +X of the motor.
        let mut rig = RobotRig {
            name: "t".into(),
            components: vec![
                part(0, None, Mat4::IDENTITY, false),
                part(1, Some(0), Mat4::IDENTITY, true),
                part(
                    2,
                    Some(1),
                    Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0)),
                    false,
                ),
            ],
            min: Vec3::ZERO,
            max: Vec3::ZERO,
            mass: 1.0,
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
