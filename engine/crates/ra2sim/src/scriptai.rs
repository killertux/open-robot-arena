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

/// What the scripts get to see about one robot.
pub fn snapshot(physics: &dyn PhysicsBackend, robot: &Robot) -> BotSnapshot {
    let transform = robot.transform(physics);
    let forward = transform.transform_vector3(robot.rig.forward());
    // The scripts think in a heading whose `(sin, cos)` is the facing, so the
    // direction is flattened: a robot on its back still has a heading.
    let flat = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let state = physics.state(robot.body());
    let velocity = state.map(|s| s.linear_velocity).unwrap_or(Vec3::ZERO);
    let angular = state.map(|s| s.angular_velocity).unwrap_or(Vec3::ZERO);
    let up = transform.transform_vector3(Vec3::Y);

    let worlds = robot.component_worlds(physics);
    let components = robot
        .rig
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| {
            (
                c.class.clone(),
                robot.health.get(i).copied().unwrap_or(c.hitpoints),
            )
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
        direction: if flat == Vec3::ZERO {
            Vec3::NEG_Z
        } else {
            flat
        },
        speed: velocity.dot(flat),
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

    /// Run every brain for one tick; `Some(events)` for each robot a brain
    /// drives, `events` being the `Input` calls it made, and `None` where the
    /// robot has no brain.
    pub fn tick(&self, count: usize) -> Vec<Option<Vec<ra2script::InputEvent>>> {
        Self::collect(count, self.host.tick_brains())
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
