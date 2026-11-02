//! The shipped AI, driving robots through the script layer.
//!
//! `ra2sim::duel`'s own driver is a stand-in: it only ever charges the opponent,
//! which is why the headline duel orbits instead of fighting. The game's answer
//! is its own Python AI — `AI/SuperAI` plus `Tactics` plus a per-bot brain chosen
//! by `Bindings.py` — and this is the bridge that runs it.
//!
//! Each tick the simulator publishes a [`ra2script::BotSnapshot`] per robot, runs
//! every brain, and reads back the `Forward`/`LeftRight` channels the brain set.
//! A robot whose name is not in `Bindings.py` keeps the stand-in driver.

use std::path::{Path, PathBuf};

use glam::{Quat, Vec3};
use ra2script::{BotSnapshot, ScriptHost};

use crate::backend::PhysicsBackend;
use crate::robot::Robot;

/// A robot's name as the AI's binding table spells it.
///
/// The `.bot` stores it on a `Name:` line and the parser keeps the whole line,
/// while `Bindings.py` lists just the name.
pub fn bot_name(rig: &crate::rig::RobotRig) -> String {
    rig.name
        .trim()
        .strip_prefix("Name:")
        .unwrap_or(rig.name.trim())
        .trim()
        .to_string()
}

/// The `nose` a brain is built with in `AI/Bindings.py` (radians), if the table lists `name` with one.
///
/// A robot driven by a brain has its forward heading set from the brain's `nose` (the AI callback's
/// field `+0x40`, pushed into the robot's `+0xac` by the vtable call at `0x415a6e` when the match is
/// set up), whatever the `.bot` says; the loader (`0x4de450`) reads a heading from the file only for
/// versions from 1.1, so the three stock 1.07 robots (Iceberg `pi`, Tornado and LugNut `-pi/2`) get
/// theirs from here (docs/34 section 11.5, checked live on all three). Every other stock robot's
/// `nose` equals its file's heading. A bot the table lists without `nose` has `0`.
pub fn bindings_nose(root: &Path, name: &str) -> Option<f32> {
    let text = std::fs::read_to_string(root.join("AI").join("Bindings.py")).ok()?;
    let key = format!("(\"{name}\"");
    let line = text.lines().find(|l| l.contains(&key))?;
    let Some(at) = line.find("'nose'") else {
        return Some(0.0);
    };
    let rest = line[at + 6..].trim_start().strip_prefix(':')?;
    let expr = rest.split([',', '}']).next()?.trim();
    parse_angle(expr)
}

/// `math.pi`, `-math.pi / 2`, `math.pi * 0.5` or a plain number.
fn parse_angle(expr: &str) -> Option<f32> {
    let (neg, body) = match expr.strip_prefix('-') {
        Some(b) => (true, b.trim()),
        None => (false, expr),
    };
    let (head, tail) = match body.find(['/', '*']) {
        Some(i) => (
            body[..i].trim(),
            Some((body.as_bytes()[i], body[i + 1..].trim())),
        ),
        None => (body, None),
    };
    let mut v = if head == "math.pi" {
        std::f32::consts::PI
    } else {
        head.parse::<f32>().ok()?
    };
    if let Some((op, n)) = tail {
        let n: f32 = n.parse().ok()?;
        v = if op == b'/' { v / n } else { v * n };
    }
    Some(if neg { -v } else { v })
}

/// `GetDirection` and `GetSpeed` (`0x4df3d0`, `0x401c90`): the unit vector the robot faces, as it is
/// (a tilted robot's has a `y`), and that vector dotted with the body's linear velocity.
pub fn facing(forward: Vec3, velocity: Vec3) -> (Vec3, f32) {
    let forward = forward.normalize_or_zero();
    (forward, velocity.dot(forward))
}

/// `RA2_AI_FLATDIR=1`: the direction projected onto the floor, as it was before docs/29 section 12an
/// (an experiment switch; the default is the original's vector).
fn flat_direction() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("RA2_AI_FLATDIR").is_ok_and(|v| v != "0"))
}

/// `GetComponentHealth` (`0x4b2c50`): a part's health over its hitpoints.
pub fn health_fraction(health: f32, hitpoints: f32) -> f32 {
    if hitpoints > 0.0 {
        health / hitpoints
    } else {
        0.0
    }
}

/// What the scripts get to see about one robot.
pub fn snapshot(physics: &dyn PhysicsBackend, robot: &Robot) -> BotSnapshot {
    let transform = robot.transform(physics);
    // `GetDirection` (`0x4df3d0`) is `(0, 0, 1)` turned by the heading and the chassis body's
    // orientation and handed out as it is: a tilted robot's vector has a `y`. `GetHeading` is its
    // `atan2(x, z)` (`0x4df320`), and `GetSpeed` (`0x401c90`) dots it with the body's linear
    // velocity. Flattening it, as this did before, turned the speed of a robot on its tail or on a
    // ramp into the horizontal one (docs/33 section 1; docs/29 section 12an).
    let state = physics.state(robot.body());
    let velocity = state.map(|s| s.linear_velocity).unwrap_or(Vec3::ZERO);
    let mut world_forward = transform.transform_vector3(robot.rig.forward());
    if flat_direction() {
        world_forward = Vec3::new(world_forward.x, 0.0, world_forward.z);
        if world_forward == Vec3::ZERO {
            world_forward = Vec3::NEG_Z;
        }
    }
    let (forward, speed) = facing(world_forward, velocity);
    let angular = state.map(|s| s.angular_velocity).unwrap_or(Vec3::ZERO);
    let up = transform.transform_vector3(Vec3::Y);

    let worlds = robot.component_worlds(physics);
    let components = robot
        .rig
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| {
            // `GetComponentHealth` (`0x4b2c50`) is the part's health over its hitpoints: a
            // fraction (live: the Scout's chassis reads 0.836 where `plus.getHitpoints` says 1527).
            let health = robot.health.get(i).copied().unwrap_or(c.hitpoints);
            (c.class.clone(), health_fraction(health, c.hitpoints))
        })
        .collect();
    let spheres: Vec<(Vec3, f32)> = (0..robot.rig.components.len())
        .map(|i| robot.component_sphere(&worlds, i))
        .collect();
    // The same parts as oriented boxes: the components' own local extents,
    // carried by their world rotation. Exact for a box-shaped part, and far
    // tighter than the sphere around it for anything long or flat.
    let boxes: Vec<(Vec3, Vec3, Quat)> = (0..robot.rig.components.len())
        .map(|i| {
            let c = &robot.rig.components[i];
            let (_, rotation, _) = worlds[i].to_scale_rotation_translation();
            (spheres[i].0, c.half_extents(), rotation)
        })
        .collect();

    BotSnapshot {
        alive: robot.is_alive(),
        // `plus.getLocation` / `AI.GetLocation` read the chassis body's
        // position, which in Havok is its centre of mass: live the resting
        // Scout and ALARM report -1.884 / -1.811, their bot origins -2.086 /
        // -2.074 (docs/26, docs/29 §12g).
        location: physics
            .center_of_mass(robot.body())
            .unwrap_or(transform.w_axis.truncate()),
        team: robot.team as i32,
        human: false,
        controls: robot
            .rig
            .wiring
            .controls
            .iter()
            .map(|c| c.name.clone())
            .collect(),
        direction: forward,
        speed,
        turning: angular.y,
        upside_down: up.y < 0.0,
        mass: robot.remaining_mass(),
        components,
        spheres,
        boxes,
        last_damage_done: ra2script::NO_DAMAGE,
        last_damage_received: ra2script::NO_DAMAGE,
        eliminated: robot.eliminated,
    }
}

/// The game's own brains, one per robot the binding table covers.
pub struct ScriptAi {
    pub host: ScriptHost,
}

impl ScriptAi {
    /// Boot the script layer, load the shipped AI and build a brain per robot.
    ///
    /// `game_root` is the installation; `arena_script` is the arena's `.py`, when
    /// the duel was given one, so `Arenas.currentArena` — which the AI reads for
    /// its pathfinding — is the scripted arena rather than nothing.
    pub fn new(
        game_root: &Path,
        arena_script: Option<&Path>,
        names: &[String],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut host = ScriptHost::boot_with_scripts(game_root, arena_script)?;
        for (i, name) in names.iter().enumerate() {
            host.add_brain(name, i as i32);
        }
        Ok(ScriptAi { host })
    }

    /// How many robots the shipped AI is driving.
    pub fn driven(&self) -> usize {
        self.host.brain_count()
    }

    /// Run the brains of the robots `due` says, for one tick each; `Some(events)`
    /// for each robot a brain drives, `events` being the `Input` calls it made
    /// (none for a brain not due), and `None` where the robot has no brain.
    pub fn tick(&self, count: usize, due: &[bool]) -> Vec<Option<Vec<ra2script::InputEvent>>> {
        Self::collect(
            count,
            self.host
                .tick_brains(&|i| due.get(i).copied().unwrap_or(false)),
        )
    }

    /// Tell the arena script that a robot entered or left one of its zones.
    pub fn zone_event(&self, direction: i32, zone: i32, robot: i32, chassis: bool) -> bool {
        self.host.send_zone_event(direction, zone, robot, chassis)
    }

    /// The SmartZones robot `robot`'s brain registered, `(name, id)`.
    pub fn brain_smart_zones(&self, robot: usize) -> Vec<(String, i32)> {
        self.host.brain_smart_zones(robot)
    }

    /// `SmartZoneEvent(direction, id, other + 1, chassis)` to `robot`'s brain.
    pub fn smart_zone_event(
        &self,
        robot: usize,
        direction: i32,
        id: i32,
        other: i32,
        chassis: bool,
    ) {
        self.host
            .send_smart_zone_event(robot, direction, id, other, chassis);
    }

    /// `LostComponent(component)` to `robot`'s brain.
    pub fn lost_component(&self, robot: usize, component: i32) {
        self.host.send_lost_component(robot, component);
    }

    /// `PlayerOut(id)` to every brain.
    pub fn player_out(&self, id: i32) {
        self.host.send_player_out(id);
    }

    /// `ImmobilityWarning(id, on)` to every brain (`FUN_00412fc0`).
    pub fn immobility_warning(&self, id: i32, on: bool) {
        self.host.send_immobility_warning(id, on);
    }

    /// Run the arena's own `Tick`, which the game did four times a second.
    ///
    /// An error is returned, not turned into `true`: the duel logs the first
    /// one with the arena's script and the Python traceback.
    pub fn tick_arena(&self, dt: f32) -> Result<bool, String> {
        self.host.run_arena_tick(dt)
    }

    /// The robots a brain drives, without running one: the controls simply
    /// hold between ticks, so there are no new events.
    pub fn drives(&self, count: usize) -> Vec<Option<Vec<ra2script::InputEvent>>> {
        Self::collect(
            count,
            self.host
                .brain_robots()
                .into_iter()
                .map(|i| (i, Vec::new()))
                .collect(),
        )
    }

    fn collect(
        count: usize,
        raw: Vec<(usize, Vec<ra2script::InputEvent>)>,
    ) -> Vec<Option<Vec<ra2script::InputEvent>>> {
        let mut out = vec![None; count];
        for (i, events) in raw {
            if i < out.len() {
                out[i] = Some(events);
            }
        }
        out
    }
}

/// The game installation holding an arena script.
pub fn game_root_for(arena_path: &Path) -> Option<PathBuf> {
    ra2_render::find_game_root(arena_path)
}

#[cfg(test)]
mod nose_tests {
    use super::parse_angle;
    use std::f32::consts::PI;

    #[test]
    fn nose_expressions_of_bindings_py_parse() {
        assert_eq!(parse_angle("math.pi"), Some(PI));
        assert_eq!(parse_angle("-math.pi / 2"), Some(-PI / 2.0));
        assert_eq!(parse_angle("math.pi * 0.5"), Some(PI * 0.5));
        assert_eq!(parse_angle("0.0"), Some(0.0));
        assert_eq!(parse_angle("north"), None);
    }
}

#[cfg(test)]
mod facing_tests {
    use super::{facing, health_fraction};
    use glam::Vec3;

    /// Live: the Scout's chassis has 1827 hitpoints and 1527.7 left, and `GetComponentHealth` says
    /// 0.8357, not 1527.7.
    #[test]
    fn a_components_health_is_a_fraction_of_its_hitpoints() {
        assert!((health_fraction(1527.72, 1827.99) - 0.8357).abs() < 1e-3);
        assert_eq!(health_fraction(5.0, 0.0), 0.0);
    }

    /// A robot on its tail (nose up) faces mostly up: the direction keeps its `y`, and the speed is
    /// the velocity along that vector, not along its horizontal shadow (live, docs/29 section 12an).
    #[test]
    fn the_direction_and_speed_are_not_flattened() {
        let forward = Vec3::new(0.0, 0.6, 0.8);
        let velocity = Vec3::new(0.0, 0.0, 2.0);
        let (d, speed) = facing(forward, velocity);
        assert!((d.y - 0.6).abs() < 1e-6 && (d.length() - 1.0).abs() < 1e-6);
        assert!(
            (speed - 1.6).abs() < 1e-6,
            "2 m/s ahead is 1.6 m/s along a 37 degree nose: {speed}"
        );
        // Level: the same as before.
        let (d, speed) = facing(Vec3::Z, velocity);
        assert_eq!(d, Vec3::Z);
        assert!((speed - 2.0).abs() < 1e-6);
    }
}
