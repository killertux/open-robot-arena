//! The robot's component tree: what a `.bot` is actually built from.
//!
//! Every attached part is a node with a mount transform, a role (chassis, wheel,
//! motor, weapon, structure), its own geometry, and the stats the game derives
//! from the component definitions (mass, hitpoints, weapon damage, motor power).

use std::collections::BTreeMap;

use crate::controls::Wiring;
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
    /// A free axle a wheel turns on: an `AxleMount` (its `.gmf` declares the
    /// hinge) or a `Steering` unit's two hubs. The original makes each its own
    /// rigid body (live, Sentinel's `hub1`/`hub2` and Ripblade's axles are
    /// bodies of their own, `assembly-*__live__000.json`).
    Axle,
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
        matches!(self, Role::Motor | Role::Piston | Role::Axle)
    }

    pub fn of(class: &str, base: &str) -> Role {
        match class {
            "Chassis" => Role::Chassis,
            "Wheel" => Role::Wheel,
            "SpinMotor" | "ServoMotor" | "BurstMotor" => Role::Motor,
            "BurstPiston" | "ServoPiston" => Role::Piston,
            "AxleMount" | "Steering" => Role::Axle,
            "Weapon" => Role::Weapon,
            "ControlBoard" => Role::Control,
            "Battery" | "AirTank" => Role::Power,
            _ => match base {
                "Weapon" => Role::Weapon,
                "SpinMotor" | "ServoMotor" | "BurstMotor" => Role::Motor,
                "ServoPiston" | "BurstPiston" => Role::Piston,
                "AxleMount" | "Steering" => Role::Axle,
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
            Role::Axle => 350.0,
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
    /// The definition's `frequency`: the weapon's bodies get their collision
    /// events at most this often per second (`0x502f7a` registers them with
    /// the event filter, `FUN_004ad500`); without one they keep the robot's
    /// 1 Hz ([`crate::combat::EventGate`]).
    pub frequency: Option<f32>,
    /// Blade reach (component bounding radius), in metres.
    pub reach: f32,
    /// The striking faces (`normal = <abs> x y z [x y z ...]`), component
    /// frame: `Weapon::computeDamage` (`FUN_005032f0`) scales the hit by
    /// `min(1, Σ max(n·f, 0))` over them (`|n·f|` when `abs`), `n` the contact
    /// normal toward the victim. None listed: the hit is not scaled.
    pub faces: [Vec3; 4],
    pub face_count: usize,
    pub faces_abs: bool,
}

impl WeaponStats {
    /// How squarely a contact with normal `n` (component frame, toward the
    /// victim) meets the striking faces (`FUN_005032f0`).
    pub fn alignment(&self, n: Vec3) -> f32 {
        if self.face_count == 0 {
            return 1.0;
        }
        let sum: f32 = self.faces[..self.face_count]
            .iter()
            .map(|f| {
                let d = n.dot(*f);
                if self.faces_abs {
                    d.abs()
                } else {
                    d.max(0.0)
                }
            })
            .sum();
        sum.min(1.0)
    }
}

/// The weapon's `normal` line: `<abs> x y z [x y z ...]`.
fn weapon_faces(def: Option<&ra2fmt::component::ComponentDef>) -> ([Vec3; 4], usize, bool) {
    let mut faces = [Vec3::ZERO; 4];
    let Some(line) = def.and_then(|d| d.get("normal")) else {
        return (faces, 0, true);
    };
    let mut words = line.split_whitespace();
    let abs = !words
        .next()
        .is_some_and(|w| w.eq_ignore_ascii_case("false") || w == "0");
    let nums: Vec<f32> = words.filter_map(|w| w.parse().ok()).collect();
    let mut n = 0;
    for c in nums.as_chunks::<3>().0.iter().take(4) {
        faces[n] = Vec3::new(c[0], c[1], c[2]);
        n += 1;
    }
    (faces, n, abs)
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
    /// A burst motor's firing power: the last two numbers of its definition's
    /// `burst = <n> <speed> <torque>` line (`burst motor.txt`: `0 50 90`). The
    /// motor fires with these and cocks again with `power` (live: the Scout's
    /// fork is set `SetPowerSettings(50, 90)` when it fires, `(5, 35)` when it
    /// cocks; `0x4468fa`, `0x446737`). The torque is raw, before the ×40.
    pub fire: Option<[f32; 2]>,
    /// A `ServoMotor`: holds its position at a zero command instead of
    /// coasting. (Ours: the binary's auto-lock flag, `motor+0x64`, is set
    /// through `SetAutoLocks` and its default for a servo is not recovered.)
    pub servo: bool,
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
    /// The component frame as its rotor carries it, in the rest pose: `local`
    /// except for a burst motor, whose rotor the original builds turned to its
    /// start angle ([`ra2fmt::mount::burst_rotor`]).
    pub rotor_local: Mat4,
    /// The motor hinge its `.gmf` declares (component space), if any.
    pub hinge: Option<ra2fmt::mount::RotorHinge>,
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
    /// The definition's `electotal`/`elecmaxinoutrate`/`airtotal`/`airmaxinoutrate`
    /// (a negative rate is a draw): the capacity of a battery or tank, the draw
    /// of a burst device. See [`crate::energy`].
    pub power: crate::energy::PowerDef,
    /// `fracture` from the component definition (or its category default).
    pub fracture: f32,
    /// `material` from the component definition (`metal`, `rubber`, …).
    pub material: Option<String>,
    /// The damage multiplier of that material (F7): the float at
    /// `material + 4` in the engine, recovered from the `MaterialObject`
    /// constructors and the definition's `material = <class>(<n>)` override.
    /// Metal (and the default for a component that declares none) is 0.1,
    /// Rubber 0.0, Arenium 0.5; the ram plate overrides Metal to 0.4.
    pub material_scale: f32,
    /// `false` when the definition says `damageable = false`.
    pub damageable: bool,
    /// The sound names the definition carries (`damagesounds`, `motorsound`,
    /// `firesound`, `runsequence`).
    pub sounds: ComponentSounds,
    /// The mark this component leaves on a chassis it hits (`decal = <file> <dir>
    /// <w> <h> <mode>`, the weapon at `+0x80..+0xac`; docs/37-effects.md).
    pub decal: Option<ra2_render::decals::DecalSpec>,
    pub role: Role,
    pub weapon: Option<WeaponStats>,
    pub motor: Option<MotorStats>,
    pub wheel: Option<WheelStats>,
    /// Linear stats when this component is a piston (F8).
    pub piston: Option<PistonStats>,
    /// A `Steering` unit's drive torque: its definition's `power` (`+0x2e0`,
    /// read at `0x4f637b`; the constructor's default is `1.0`, `0x4f5fdb`).
    pub steer_power: Option<f32>,
    /// The chassis' buckle model (F9): the positions the bot file marks as
    /// movable, which the original shifts "down and in" as the chassis is hit.
    /// Only the chassis has one.
    pub buckle: Vec<Vec3>,
    /// How far the chassis has already buckled (0 pristine, 1 at its last
    /// hitpoint).
    pub buckle_fraction: f32,
    /// The chassis' pristine hull points and drawn vertices (F9). The crush is
    /// applied *from* these on every update, so the curve is linear in the
    /// damage fraction and a full crush lands the top ring exactly on the
    /// file's movable floor; compounding the relative step on the already
    /// crushed mesh is asymptotic and never reaches it.
    pub rest_hull: Vec<Vec3>,
    pub rest_items: Vec<Vec<Vec3>>,
    /// Children component indices (motor-driven subtree).
    pub children: Vec<usize>,
    /// The component's Havok rigid bodies, as the original builds them.
    pub havok: HavokParts,
}

/// What the original's Havok simulation makes of one component, read from the
/// `.gmf`'s rigid bodies (the `havok17` backend builds its bodies from this;
/// the Jolt backend keeps the whole-scene hull).
///
/// * Only bodies without `DISABLE_COLLISIONS` collide, each with its own
///   collision object (`GEOMETRY_PROXY_NAME` when set). A wheel's
///   `wheel_traction` is a *phantom*: the game counts what it touches to know
///   the wheel is on the ground (`Wheel` factory `0x507c00`), it holds nothing
///   up. The mini wheel's hub that does is 0.2 m across the flats' corners,
///   the traction ring 0.33 m.
/// * Every rigid body has mass, phantoms included: live, the Scout's wheel
///   bodies weigh 27 kg = mini wheel 9 + its traction phantom 8 + the motor's
///   `axle` 10 (`havok-step`, body `+0x190`).
/// * A motor's `.gmf` has two bodies: the housing stays on the body the motor
///   is bolted to and the rotor (`axle`, `bmotor_axle`) goes with what it
///   drives. Live, the Scout's chassis body weighs 163.49 kg (its chassis plus
///   the motor housings, battery and board) and its fork arm 26 kg (extender 2
///   + fork 12 + `bmotor_axle` 12).
#[derive(Clone, Debug, Default)]
pub struct HavokParts {
    /// One piece per colliding rigid body, component-local.
    pub collision: Vec<Vec<Vec3>>,
    /// Per piece of `collision`: the triangles (into its points) when Havok
    /// collides it as a mesh rather than as a convex hull
    /// ([`collides_as_mesh`]).
    pub collision_mesh: Vec<Option<Vec<[u32; 3]>>>,
    /// Per piece of `collision`: its rigid body's own mass.
    pub collision_mass: Vec<f32>,
    /// The phantom geometry (all of it, component-local).
    pub phantom: Vec<Vec3>,
    /// All the rigid bodies' mass, phantoms included, and the phantoms' share.
    pub mass: f32,
    pub phantom_mass: f32,
    /// A motor's rotor: its mass and collision pieces, which belong to the
    /// driven body.
    pub rotor_mass: f32,
    pub rotor_collision: Vec<Vec<Vec3>>,
    pub rotor_collision_mesh: Vec<Option<Vec<[u32; 3]>>>,
    /// Per piece of `rotor_collision`: its rigid body's own mass.
    pub rotor_piece_mass: Vec<f32>,
    /// Per piece of `rotor_collision`: its triangles as the `.gmf` stores
    /// them, which place the rotor's mass (the stock rotor proxies are boxes
    /// open at their inboard end, docs/29 section 12h).
    pub rotor_faces: Vec<Vec<[u32; 3]>>,
    /// Per phantom rigid body: its geometry and its own mass (it places that
    /// mass, though it collides with nothing).
    pub phantom_pieces: Vec<(Vec<Vec3>, f32)>,
    /// Geometry (component-local) that places the mass of a component with
    /// nothing to collide: a SmartZone's zone.
    pub mass_points: Vec<Vec3>,
    /// The top rigid body's material.
    pub friction: Option<f32>,
    pub restitution: Option<f32>,
}

impl HavokParts {
    /// Read a component's `.gmf` scene.
    pub fn from_scene(scene: &ra2fmt::gmi::GmiScene) -> HavokParts {
        type Geometry = (Vec<Vec3>, Vec<[u32; 3]>);
        let objects: BTreeMap<String, Geometry> = scene
            .objects
            .iter()
            .filter_map(|o| {
                let mesh = o.mesh.as_ref()?;
                if mesh.points.is_empty() {
                    return None;
                }
                let m = ra2_render::object_transform(o);
                let n = mesh.points.len() as u32;
                Some((
                    o.name.to_ascii_lowercase(),
                    (
                        mesh.points
                            .iter()
                            .map(|p| m.transform_point3(Vec3::from(*p)))
                            .collect(),
                        mesh.faces
                            .iter()
                            .map(|f| f.v)
                            .filter(|v| v.iter().all(|&i| i < n))
                            .collect(),
                    ),
                ))
            })
            .collect();
        let geometry_of = |rb: &ra2fmt::gmi::RigidBody| -> Geometry {
            objects
                .get(&rb.collision_object().to_ascii_lowercase())
                .cloned()
                .unwrap_or_default()
        };
        // A motor's rotor (or a piston's rod): `axle`, `bmotor_axle`, `shaft`,
        // `slider` in every stock motor and piston `.gmf`.
        let bodies = scene.rigid_bodies();
        let is_rotor = |rb: &ra2fmt::gmi::RigidBody| {
            let n = rb.name.to_ascii_lowercase();
            // A `Steering` unit's `hub1`/`hub2` (5 kg each) ride its hubs: live
            // Sentinel's hub bodies weigh 23 kg (wheel 18 + hub 5).
            (n.ends_with("axle") || n == "shaft" || n == "slider" || n == "hub1" || n == "hub2")
                && !n.contains("collision")
        };
        let has_rotor = bodies.len() > 1 && bodies.iter().any(|rb| is_rotor(rb));
        let mut out = HavokParts::default();
        for rb in &bodies {
            out.mass += rb.mass.max(0.0);
            if out.friction.is_none() {
                out.friction = Some(rb.friction);
                out.restitution = Some(rb.elasticity);
            }
            let (pts, faces) = geometry_of(rb);
            let mesh = (collides_as_mesh(rb.simulation_geometry) && !faces.is_empty())
                .then(|| faces.clone());
            if rb.disable_collisions {
                if pts.len() >= 4 {
                    out.phantom_pieces.push((pts.clone(), rb.mass.max(0.0)));
                }
                out.phantom.extend(pts);
                out.phantom_mass += rb.mass.max(0.0);
                continue;
            }
            if has_rotor && is_rotor(rb) {
                out.rotor_mass += rb.mass.max(0.0);
                if pts.len() >= 4 {
                    out.rotor_collision.push(pts);
                    out.rotor_collision_mesh.push(mesh);
                    out.rotor_piece_mass.push(rb.mass.max(0.0));
                    out.rotor_faces.push(faces.clone());
                }
                continue;
            }
            if pts.len() >= 4 {
                out.collision.push(pts);
                out.collision_mesh.push(mesh);
                out.collision_mass.push(rb.mass.max(0.0));
            }
        }
        out
    }
}

/// What a SmartZone adds to the body it is bolted to: its `.gmf` rigid body's
/// mass, a phantom (`DISABLE_COLLISIONS`, 2 kg in every stock SmartZone) the
/// renderer's loader skips. Live, the Scout's, ALARM's, FlapJack's and Wide
/// Load's chassis bodies weigh exactly 2 kg more than their other members
/// (163.49 against 161.49 kg; Razor, which has none, 212.52 on both). Only the
/// mass is kept: the zone itself is the game's own, not a collidable.
fn smart_zone_mass(root: &std::path::Path, file: &str) -> HavokParts {
    let cfl = ra2fmt::Cfl::open_path(&root.join("Components.cfz")).ok();
    let text = cfl
        .as_ref()
        .and_then(|cf| cf.get_text(file))
        .or_else(|| std::fs::read_to_string(root.join(file.replace('\\', "/"))).ok());
    let Some(text) = text else {
        return HavokParts::default();
    };
    let def = ra2fmt::component::parse(&text);
    let path = root.join("Components").join(def.mesh_rel_path());
    let Some(Ok(scene)) = std::fs::read(path).ok().map(|b| ra2fmt::gmi::parse(&b)) else {
        return HavokParts::default();
    };
    let h = HavokParts::from_scene(&scene);
    HavokParts {
        mass: h.mass,
        phantom_mass: h.mass,
        ..HavokParts::default()
    }
}

/// A weapon's material float when its definition declares none (`0x502e5f`).
pub const WEAPON_MATERIAL_SCALE: f32 = 0.8;

/// Whether Havok collides a rigid body with this `SIMULATION_GEOMETRY` as a
/// triangle mesh (shape class `GEOMETRIC`, 4) rather than as a convex hull
/// (`CXGEOMETRIC`, 0x400).
///
/// Live (the agent dispatch `FUN_00543370`, collidable `+0x18`, Scout and
/// ALARM): the mini wheel (2), the extender, motor housings and rotors, the
/// control board (3) are convex; the fork arm (6) is a mesh, as are the
/// chassis bodies the game builds. 6 is used by the fork arm, the bracket
/// wedge and the two snowplows; 0 (angler, ram plate, steering hubs) was not
/// observed and stays convex.
pub fn collides_as_mesh(simulation_geometry: i32) -> bool {
    simulation_geometry == 6
}

/// The sound keys of a component definition (`Components.cfz`).
///
/// `damagesounds` is a comma-separated list the original picks from at random
/// when the component is hit (`FUN_0045cad0` takes a random element of the
/// victim's list); the chassis takes its list from its armour material. The
/// rest are played by the motor/weapon: `motorsound` loops while a motor runs,
/// `runsequence` is spin-up / loop / spin-down, `firesound` plays when the
/// device fires. Paths are as the definition spells them (`sounds\\x.wav`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComponentSounds {
    pub damage: Vec<String>,
    pub motor: Option<String>,
    pub fire: Option<String>,
    pub run_sequence: Vec<String>,
}

impl ComponentSounds {
    /// Read the sound keys from a definition.
    pub fn from_def(def: Option<&ra2fmt::component::ComponentDef>) -> ComponentSounds {
        let Some(def) = def else {
            return ComponentSounds::default();
        };
        let list = |key: &str| -> Vec<String> {
            def.get(key)
                .map(|v| {
                    v.split(',')
                        .map(|p| p.trim().to_string())
                        .filter(|p| !p.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        };
        ComponentSounds {
            damage: list("damagesounds"),
            motor: list("motorsound").into_iter().next(),
            fire: list("firesound").into_iter().next(),
            run_sequence: list("runsequence"),
        }
    }

    /// The chassis' impact sounds by armour type (`chassis_plast`, `_alum`,
    /// `_steel`, `_tit` in `Components.cfz`): 0 plastic, 1 aluminium, 2 steel,
    /// 3 titanium.
    pub fn chassis(armour_type: i32) -> ComponentSounds {
        let damage: &[&str] = match armour_type {
            0 => &[
                "sounds\\plastic_collision.wav",
                "sounds\\plastic_collision.wav",
            ],
            1 => &[
                "sounds\\aluminum_collision.wav",
                "sounds\\aluminum_collision2.wav",
                "sounds\\aluminum_collision3.wav",
            ],
            2 => &[
                "sounds\\steel_collision.wav",
                "sounds\\steel_collision2.wav",
            ],
            _ => &["sounds\\titanium_hit.wav", "sounds\\titanium_hit2.wav"],
        };
        ComponentSounds {
            damage: damage.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }
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
    /// A burst motor's firing pair (speed, raw torque): [`MotorStats::fire`].
    pub fire: Option<[f32; 2]>,
    /// A wheel's own numbers (`None` for every other joint).
    pub wheel: Option<WheelDrive>,
}

/// What a wheel joint needs to push the robot along.
///
/// The original's wheels are **not** friction-limited: the Scout's own drive
/// trace accelerates at 13.8 m/s² (1.4 g) at full throttle from a rest pose in
/// which its wheels are not carrying its weight (the rest height matches a
/// robot sitting on its battery and its tail, wheels clear of the floor), and
/// its pitch does not change while it does so. A velocity motor and Coulomb
/// friction cannot do that, so a wheel here is a *thrust* and a *drag*: see
/// [`crate::robot::Robot`]'s `drive_wheel`.
#[derive(Clone, Copy, Debug)]
pub struct WheelDrive {
    /// Rolling radius in metres: the farthest the wheel's collision hull reaches
    /// from its axle (for the stock mini wheel, the 0.33 m `wheel_traction`
    /// ring).
    pub radius: f32,
    /// The definition's `grip`.
    pub grip: f32,
    /// The definition's `resistance`.
    pub resistance: f32,
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
    /// The bot's control board: which controls drive which components.
    pub wiring: Wiring,
    /// The `.bot`'s forward heading (radians, `0` or `PI` in every stock bot):
    /// the direction the robot's front faces in its own frame, as a rotation of
    /// `+Z` about `Y` — the original's `GetHeading` (`0x4df320`) turns
    /// `(0, 0, 1)` by the quaternion about `Y` of robot `+0xac` and then by the
    /// chassis. The stock bots with a `PI` heading drive toward `-Z` on `W`
    /// (`magic-razor__live__000`: 16.4 m/s along the chassis' `-Z`).
    pub heading: f32,
    /// The AI's sensor volumes (`SmartZone` components), see [`SmartZone`].
    pub smart_zones: Vec<SmartZone>,
}

/// A `SmartZone` component: the box a brain watches for enemies
/// (`RegisterSmartZone` / `GetZoneContents`, `0x401340` / `0x4014f0`).
///
/// The component itself has no body; the box rides on whatever part its
/// component is mounted to.
#[derive(Clone, Debug)]
pub struct SmartZone {
    /// The SmartZone component's index.
    pub component: usize,
    /// Its name line (`weapon`, `flip`, ...), what the brains ask for.
    pub name: String,
    /// The zone's frame relative to the component's own world transform (the
    /// component's stored `local` places it without its own mount point).
    pub offset: Mat4,
    pub min: Vec3,
    pub max: Vec3,
}

impl SmartZone {
    /// Whether a sphere of `radius` at `centre` touches the box whose frame is
    /// `world` (the zone component's world transform times [`Self::offset`]).
    pub fn touches(&self, world: Mat4, centre: Vec3, radius: f32) -> bool {
        let p = world.inverse().transform_point3(centre);
        let r = Vec3::splat(radius);
        (p.cmpge(self.min - r) & p.cmple(self.max + r)).all()
    }
}

impl RobotRig {
    /// The robot's front in its own frame: `+Z` turned by [`RobotRig::heading`]
    /// (see there).
    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.heading.sin(), 0.0, self.heading.cos())
    }

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
    let limits = if nums.len() >= 6 && burst {
        // A burst piston's controller stops are its neutral point and its
        // lower travel (live, ALARM's: `+0x6c = 0`, `+0x70 = -0.73`), and the
        // rod comes back to rest exactly at the neutral point: its travel is
        // `[lower, neutral]`, the `upper` number unused
        // (`weapons-scout-alarm__live__000`, docs/29 section 12).
        let lower = nums[4] - nums[3];
        [lower.min(0.0), lower.max(0.0)]
    } else if nums.len() >= 6 {
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

/// A burst motor's firing pair: the second and third numbers of its `burst`
/// line (the first is not used for the power).
fn burst_power(def: &ra2fmt::component::ComponentDef) -> Option<[f32; 2]> {
    let v = def.get("burst")?;
    let n: Vec<f32> = v
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter_map(|t| t.parse::<f32>().ok())
        .collect();
    (n.len() >= 3).then(|| [n[1], n[2]])
}

fn num(def: &ra2fmt::component::ComponentDef, key: &str) -> Option<f32> {
    def.number(key)
}

/// The maximum hitpoints: the definition's `hitpoints`, else the factory's
/// category table, else **100**.
///
/// The component constructor (`FUN_0045cc10`, `0x45cc8e`) stores a maximum of
/// `0x64` before the factory's `type` switch (`FUN_0045cf40`, `0x45d09e`..) runs,
/// so a component whose `type` is none of the six categories keeps 100 — not an
/// invented per-role number. Only a component with no definition at all (never
/// the case in the game) falls back to the role's guess.
fn type_hitpoints(def: Option<&ra2fmt::component::ComponentDef>, role: Role) -> f32 {
    match def {
        Some(d) => d
            .number("hitpoints")
            .or_else(|| ra2fmt::component::category_hitpoints(def))
            .unwrap_or(COMPONENT_DEFAULT_HITPOINTS),
        None => role.default_hitpoints(),
    }
}

/// The maximum hitpoints the component constructor stores (`mov [esi+0xe8], 0x64`).
const COMPONENT_DEFAULT_HITPOINTS: f32 = 100.0;

/// The `fracture` default the same factory sets per category.
fn type_fracture(def: Option<&ra2fmt::component::ComponentDef>) -> f32 {
    def.and_then(|d| d.number("fracture"))
        .or_else(|| ra2fmt::component::category_fracture(def))
        .unwrap_or(0.0)
}

fn flag_false(def: &ra2fmt::component::ComponentDef, key: &str) -> bool {
    ra2fmt::component::flag_false(def, key)
}

/// The mass a `.gmf` rigid body with `mass = 0` is given, from the room it takes
/// up (F12). The rule and its unresolved density conflict live in
/// [`ra2fmt::component::zero_mass_fill`] so the workshop reports the same mass.
fn zero_mass_fill(points: &[Vec3]) -> f32 {
    if points.is_empty() {
        return 7.0;
    }
    let (mut mn, mut mx) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in points {
        mn = mn.min(*p);
        mx = mx.max(*p);
    }
    ra2fmt::component::zero_mass_fill(mn.to_array(), mx.to_array())
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
        let axle = comps[parent].role == Role::Axle;
        if motor.is_none() && piston.is_none() && !axle {
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
        // A burst motor's and an axle mount's hinge is the one their `.gmf`
        // declares (`Hinge01`, housing to rotor): a burst motor's mount point
        // on the rotor faces along the arm, not along the axle, and its start
        // and travel are angles about that hinge's spin axis.
        let gmf_hinge = comps[parent]
            .hinge
            .as_ref()
            .filter(|_| piston.is_none() && (axle || motor.and_then(|m| m.burst).is_some()));
        let (pivot, axis) = match gmf_hinge {
            Some(h) => (
                parent_local.transform_point3(h.pivot),
                parent_local.transform_vector3(h.axis).normalize_or(axis),
            ),
            None => (pivot, axis),
        };
        // A burst motor is a limited hinge; everything else swings freely. The
        // original builds its rotor at the start angle and then re-creates the
        // hinge there (`FUN_00480c60`), so the rest pose is the hinge's zero;
        // the second number is the travel (`FUN_00480dd0` sets the limits to
        // `0` and `-end` in the game's own angle sense, which is the opposite
        // of the right-handed turn about the spin axis the start angle is
        // measured in — Scout's fork swings up, Grog's axe down). Jolt wants
        // the range to straddle zero.
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
        } else if axle {
            // A free axle: the wheel spins on it, nothing drives it here.
            (
                0.0,
                0.0,
                [-std::f32::consts::PI, std::f32::consts::PI],
                comps[root]
                    .wheel
                    .as_ref()
                    .map(|w| w.resistance * torque_unit())
                    .unwrap_or(0.0),
                JointUse::Passive,
            )
        } else {
            let m = motor.expect("checked above");
            let limits = match m.burst {
                Some([_start, travel]) => {
                    let d = travel.clamp(-std::f32::consts::PI, std::f32::consts::PI);
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
            } else if m.burst.is_some() || m.servo {
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
            fire: motor.and_then(|m| m.fire).filter(|_| piston.is_none()),
            wheel: if use_kind == JointUse::Wheel {
                comps[root].wheel.as_ref().map(|w| {
                    let radius = comps[root]
                        .hull_points
                        .iter()
                        .map(|p| {
                            let rel = comps[root].local.transform_point3(*p) - pivot;
                            (rel - axis * rel.dot(axis)).length()
                        })
                        .fold(0.0f32, f32::max);
                    WheelDrive {
                        radius: radius.max(0.05),
                        grip: w.grip,
                        resistance: w.resistance,
                    }
                })
            } else {
                None
            },
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
    let (bot, root, loaded) = ra2_render::load_bot_components(path)?;
    let (rest, carried) =
        ra2_render::component_assembly_mounted(&bot, &loaded, ra2_render::MountMode::from_env());
    let smart_zones: Vec<SmartZone> =
        ra2_render::bot_smart_zones(&bot, &root, &loaded, ra2_render::MountMode::from_env())
            .into_iter()
            .map(|z| SmartZone {
                component: z.component,
                name: z.name,
                offset: rest[z.component].inverse() * z.local,
                min: z.min,
                max: z.max,
            })
            .collect();
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

        // The Havok bodies the original makes of it (the chassis' is built by
        // the game itself from its own mesh).
        let havok = if i == 0 {
            // The chassis the game builds is one mesh collidable (live: a
            // `GEOMETRIC` collidable on every chassis body): the `.bot`'s own
            // chassis mesh, triangles and all.
            bot.components
                .first()
                .and_then(|c| c.chassis.as_ref())
                .and_then(|ch| ch.mesh.as_ref())
                .map(|m| {
                    let n = m.points.len() as u32;
                    let tris: Vec<[u32; 3]> = m
                        .indices
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .copied()
                        .filter(|t| t.iter().all(|&v| v < n))
                        .collect();
                    HavokParts {
                        collision: vec![m.points.iter().map(|p| Vec3::from(*p)).collect()],
                        collision_mesh: vec![(!tris.is_empty()).then_some(tris)],
                        ..HavokParts::default()
                    }
                })
                .filter(|h| h.collision[0].len() >= 4)
                .unwrap_or_default()
        } else if c.class == "SmartZone" {
            smart_zone_mass(&root, &c.file)
        } else {
            loaded
                .get(i)
                .and_then(|l| l.as_ref())
                .map(|lc| HavokParts::from_scene(&lc.scene))
                .unwrap_or_default()
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
                // Every rigid body of the file, children included: a wheel's
                // `wheelgroup` is a zero-mass parent whose `miniwheel` (9) and
                // `wheel_traction` (8, a phantom: `DISABLE_COLLISIONS`, no mass in the
                // simulation) children; only the hub's 9 kg counts.
                let sum: f32 = lc
                    .scene
                    .rigid_bodies()
                    .iter()
                    .filter(|rb| !rb.disable_collisions)
                    .map(|rb| rb.mass)
                    .sum();
                if sum > 0.0 {
                    sum
                } else {
                    zero_mass_fill(&points)
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

        // Chassis hitpoints are trunc(surface area x armour hardness) (FUN_0044d710). Every
        // other component's come from the game's own rule: the component factory
        // (`FUN_0045cf40`) switches on the definition's `type` and stores a
        // default maximum, then a `hitpoints =` line in the definition overrides
        // it. Recovered from the binary: power 400, extenders 100, mechanics 400,
        // mobility 400, weapons 1000, extras 1000 — see F7.
        let chassis_hp = bot
            .components
            .first()
            .and_then(|c| c.chassis.as_ref())
            .and_then(|c| c.hitpoints());
        let hitpoints = if i == 0 {
            chassis_hp.unwrap_or_else(|| role.default_hitpoints())
        } else {
            type_hitpoints(def.as_ref(), role)
        };
        // `fracture` is a per-category default too (extenders 15, mechanics and
        // mobility 20, weapons 20, extras 60) overridden by the definition.
        let fracture = type_fracture(def.as_ref());
        let material = def
            .as_ref()
            .and_then(|d| d.get("material"))
            .map(|s| s.to_ascii_lowercase());
        // A weapon's constructor (`FUN_00502e10`, `0x502e5f`) sets its Metal
        // material's float to 0.8; a `material` line still overrides it.
        let undeclared = if role == Role::Weapon {
            WEAPON_MATERIAL_SCALE
        } else {
            ra2fmt::component::DEFAULT_MATERIAL_SCALE
        };
        let material_scale = def
            .as_ref()
            .filter(|d| d.get("material").is_some())
            .map(|d| d.material().scale)
            .unwrap_or(undeclared);
        let damageable = def
            .as_ref()
            .map(|d| !flag_false(d, "damageable"))
            .unwrap_or(true);
        let mut sounds = ComponentSounds::from_def(def.as_ref());
        if i == 0 && sounds.damage.is_empty() {
            let armour_type = bot
                .components
                .first()
                .and_then(|c| c.chassis.as_ref())
                .and_then(|c| c.armour)
                .map(|a| a.type_id)
                .unwrap_or(2);
            sounds = ComponentSounds::chassis(armour_type);
        }
        let piston = (role == Role::Piston).then(|| {
            piston_stats(
                def.as_ref().expect("a piston has a definition"),
                base.eq_ignore_ascii_case("BurstPiston"),
            )
        });
        let (faces, face_count, faces_abs) = weapon_faces(def.as_ref());
        let weapon = (role == Role::Weapon).then(|| WeaponStats {
            // The component constructor's defaults (`FUN_00489110`: `+0x68`
            // piercing 0, `+0x6c` concussion 1.0), which the weapon constructor
            // keeps: the Hammer Head, Sledge Hammer, Iron Fist, Saw Blade, Ninja
            // Star and ChewBlade declare neither. Live
            // (`fight-scout-alarm-box__live__000`), ALARM's Hammer Head prices
            // its hits at up to exactly 8× what the Scout part it strikes
            // prices the same record at (0.8 / 0.1 material, concussion 1 on
            // both sides).
            concussion: def
                .as_ref()
                .and_then(|d| num(d, "concussion"))
                .unwrap_or(1.0),
            piercing: def.as_ref().and_then(|d| num(d, "piercing")).unwrap_or(0.0),
            frequency: def
                .as_ref()
                .and_then(|d| num(d, "frequency"))
                .filter(|f| *f > 0.0),
            reach: ((max - min) * 0.5).length().max(0.1),
            faces,
            face_count,
            faces_abs,
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
                fire: def.as_ref().and_then(burst_power),
                servo: bot.components[i].class.eq_ignore_ascii_case("ServoMotor"),
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

        // F9: keep the chassis' pristine geometry so every crush update applies
        // an absolute displacement from it (see `Robot::buckle`).
        let rest_hull = if i == 0 {
            hull_points.clone()
        } else {
            Vec::new()
        };
        let rest_items = if i == 0 {
            items
                .iter()
                .map(|item| item.vertices.iter().map(|v| Vec3::from(v.pos)).collect())
                .collect()
        } else {
            Vec::new()
        };

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
            rotor_local: carried[i],
            hinge: loaded
                .get(i)
                .and_then(|l| l.as_ref())
                .and_then(|lc| ra2fmt::mount::rotor_hinge(&lc.scene)),
            items,
            rotor_items,
            hull_points,
            min,
            max,
            mass,
            hitpoints,
            power: crate::energy::PowerDef::from_def(def.as_ref()),
            fracture,
            material,
            material_scale,
            damageable,
            sounds,
            decal: def
                .as_ref()
                .and_then(|d| d.get("decal"))
                .and_then(ra2_render::decals::DecalSpec::parse),
            role,
            weapon,
            motor,
            wheel,
            piston,
            steer_power: (c.class.eq_ignore_ascii_case("Steering")
                || def
                    .as_ref()
                    .and_then(|d| d.get("base"))
                    .is_some_and(|b| b.trim().eq_ignore_ascii_case("Steering")))
            .then(|| def.as_ref().and_then(|d| num(d, "power")).unwrap_or(1.0)),
            // The chassis' crush cage (F9). Its points name the chassis mesh's
            // own space, which for component 0 is bot space.
            buckle: if i == 0 {
                chassis_buckle.clone()
            } else {
                Vec::new()
            },
            buckle_fraction: 0.0,
            rest_hull,
            rest_items,
            children: Vec::new(),
            havok,
        });
    }

    // A SmartZone's mass sits at its zone's centre (the zone box in the
    // component's frame).
    for z in &smart_zones {
        if let Some(c) = components.get_mut(z.component) {
            let (lo, hi) = (z.min, z.max);
            c.havok.mass_points = (0..8)
                .map(|k| {
                    let p = Vec3::new(
                        if k & 1 == 0 { lo.x } else { hi.x },
                        if k & 2 == 0 { lo.y } else { hi.y },
                        if k & 4 == 0 { lo.z } else { hi.z },
                    );
                    z.offset.transform_point3(p)
                })
                .collect();
        }
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
        wiring: Wiring::from_bot(&bot),
        heading: bot.heading.unwrap_or(0.0),
        smart_zones,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Weapon::computeDamage` (`FUN_005032f0`): the hit scales with how
    /// squarely the contact meets the listed faces, `min(1, Σ max(n·f, 0))`
    /// (`|n·f|` for `normal = true ...`).
    #[test]
    fn a_weapon_prices_by_its_striking_faces() {
        let w = WeaponStats {
            concussion: 1.0,
            piercing: 0.0,
            frequency: None,
            reach: 0.3,
            faces: [Vec3::NEG_X, Vec3::ZERO, Vec3::ZERO, Vec3::ZERO],
            face_count: 1,
            faces_abs: false,
        };
        assert_eq!(w.alignment(Vec3::NEG_X), 1.0);
        assert_eq!(w.alignment(Vec3::X), 0.0);
        assert!((w.alignment(Vec3::new(-0.6, 0.8, 0.0)) - 0.6).abs() < 1e-6);
        let both = WeaponStats {
            faces: [Vec3::X, Vec3::Y, Vec3::ZERO, Vec3::ZERO],
            face_count: 2,
            faces_abs: true,
            ..w
        };
        assert!((both.alignment(Vec3::new(-0.6, -0.8, 0.0)) - 1.0).abs() < 1e-6);
        let none = WeaponStats { face_count: 0, ..w };
        assert_eq!(none.alignment(Vec3::X), 1.0);
    }

    /// The Hammer Head declares neither `concussion`, `piercing` nor
    /// `material`: it keeps the component constructor's concussion 1
    /// (`FUN_00489110`) and the weapon constructor's 0.8 material
    /// (`0x502e5f`), and strikes with its -X face (`normal = false -1 0 0`).
    #[test]
    fn the_hammer_head_keeps_the_constructors_defaults() {
        let Some(game) = std::env::var_os("RA2_GAME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                let p =
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../local/game");
                p.exists().then_some(p)
            })
        else {
            return;
        };
        let rig = load(&game.join("AI/Team0/Bot1.bot").to_string_lossy()).expect("ALARM");
        let hammer = rig
            .components
            .iter()
            .find(|c| c.name == "Hammer Head")
            .expect("ALARM's hammer");
        let w = hammer.weapon.expect("a weapon");
        assert_eq!((w.piercing, w.concussion), (0.0, 1.0));
        assert_eq!(hammer.material_scale, WEAPON_MATERIAL_SCALE);
        assert_eq!(
            (w.face_count, w.faces_abs, w.faces[0]),
            (1, false, Vec3::NEG_X)
        );
    }

    /// F12: a `.gmf` body with `mass = 0` is priced by the room it takes up.
    ///
    /// Every stock wheel stores `mass = 0`, and a flat fallback made a 0.30 m³
    /// mudtire and a 0.06 m³ mini wheel weigh the same. The engine's own fill is
    /// not reachable — its `.gmf` reader takes the zero at face value
    /// (`FUN_005c6820` reads the mass straight into the body at `+0x20`) — so the
    /// rule is ours, but it is now size-aware, and the mini wheel lands on the
    /// 8 kg the rest of the simulation was calibrated against.
    #[test]
    fn a_zero_mass_body_is_priced_by_its_size() {
        let mudtire = zero_mass_fill(&[Vec3::ZERO, Vec3::new(0.947, 0.947, 0.340)]);
        let mini = zero_mass_fill(&[Vec3::ZERO, Vec3::new(0.652, 0.652, 0.135)]);
        assert!(
            (mini - 8.0).abs() < 0.4,
            "the mini wheel should stay at 8 kg, got {mini}"
        );
        assert!(mudtire > 4.0 * mini, "{mudtire} vs {mini}");
        // A part with no geometry at all still gets something.
        assert!(zero_mass_fill(&[]) >= 1.0);
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
        // No category: the constructor's 100, not a per-role guess.
        assert_eq!(hp("name = x\n"), 100.0);
        assert_eq!(hp("type = whatever\n"), 100.0);
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
    /// `body, shaft, 0, 0, 1, 0.0, -0.60, .8`. A servo piston slides from
    /// -0.60 to 0.80 about its neutral point; a burst piston only between its
    /// lower stop and the neutral point it rests at (live, ALARM's
    /// `burstpiston2`: stops 0 and -0.73).
    #[test]
    fn piston_travel_comes_from_the_prismatic_line() {
        let d = ra2fmt::component::parse(
            "prismatic = body, shaft, 0, 0, 1, 0.0, -0.60, .8\npower = 60 100\n",
        );
        let p = piston_stats(&d, false);
        assert_eq!(p.axis, Vec3::Z);
        assert_eq!(p.limits, [-0.60, 0.8]);
        assert_eq!(p.speed, 60.0);
        assert_eq!(p.force, 100.0 * torque_unit());
        assert!(!p.burst);
        let p = piston_stats(&d, true);
        assert_eq!(p.limits, [-0.60, 0.0]);
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
            rotor_local: local,
            hinge: None,
            items: Vec::new(),
            rotor_items: Vec::new(),
            hull_points: Vec::new(),
            havok: HavokParts::default(),
            min: Vec3::ZERO,
            max: Vec3::ZERO,
            mass: 1.0,
            hitpoints: 1.0,
            power: crate::energy::PowerDef::default(),
            fracture: 0.0,
            material: None,
            material_scale: ra2fmt::component::DEFAULT_MATERIAL_SCALE,
            damageable: true,
            sounds: ComponentSounds::default(),
            decal: None,
            role: if motor { Role::Motor } else { Role::Structure },
            weapon: None,
            motor: motor.then_some(MotorStats {
                servo: false,
                speed: 10.0,
                torque: 400.0,
                burst: None,
                fire: None,
            }),
            wheel: None,
            piston: None,
            steer_power: None,
            buckle: Vec::new(),
            buckle_fraction: 0.0,
            rest_hull: Vec::new(),
            rest_items: Vec::new(),
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
            wiring: Wiring::default(),
            heading: 0.0,
            smart_zones: Vec::new(),
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
