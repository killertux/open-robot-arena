//! The M3 duel: two robots fight in an arena, weapons spin, parts fly off.

use glam::{Mat4, Quat, Vec3};
use ra2_render::{DrawItem, RenderGroup, Renderer};

use crate::backend::{BodyDesc, BodyHandle, MotionType, PhysicsBackend, ShapeDesc};
use crate::combat::{self, CombatConfig};
use crate::jolt::JoltBackend;
use crate::rig;
use crate::robot::Robot;
use crate::scene::{self, ArenaScene, USER_ARENA, USER_PROP};
use crate::sim::{DriveInput, DriveTuning, PropEntity, BOT_FORWARD};

/// A part that has been knocked off a robot.
pub struct Debris {
    pub body: BodyHandle,
    pub items: Vec<DrawItem>,
    pub label: String,
}

/// `BodyDesc::user_data` tag for flying parts.
pub const USER_DEBRIS: u64 = 4;

/// How to spawn one robot.
#[derive(Clone, Debug)]
pub struct BotSetup {
    pub path: String,
    /// Spawn heading in degrees (0 = facing the opponent for 2-robot duels).
    pub yaw_deg: Option<f32>,
}

/// A duel: physics world, arena, two robots, debris and the damage model.
pub struct Duel {
    pub physics: Box<dyn PhysicsBackend>,
    pub arena: ArenaScene,
    pub props: Vec<PropEntity>,
    pub robots: Vec<Robot>,
    pub debris: Vec<Debris>,
    pub time: f32,
    pub tuning: DriveTuning,
    pub combat: CombatConfig,
    /// Scales each robot's natural mass.
    pub mass_scale: f32,
    /// Human-readable event log (hits that broke something, KOs, ...).
    pub log: Vec<String>,
    /// Last damage dealt, per robot (for reporting).
    pub damage_taken: Vec<f32>,
    /// Which robot pairs were touching at the end of the last step, so a blow is
    /// dealt when contact *begins* instead of every step it lasts.
    contact_pairs: Vec<bool>,
    /// Per-robot AI state.
    ai: Vec<AiState>,
}

/// What the AI is doing right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AiMode {
    /// Line up on the opponent and drive into it.
    Charge,
    /// Back off for another run-up.
    BackOff,
}

#[derive(Clone, Copy, Debug)]
struct AiState {
    mode: AiMode,
    timer: f32,
    /// How long the hulls have been touching during this charge.
    engaged: f32,
    /// `press` grinds continuously; `cycle` (the default) disengages briefly so
    /// the robots reposition instead of welding together.
    press: bool,
}

impl AiState {
    fn new() -> Self {
        AiState {
            mode: AiMode::Charge,
            timer: 4.0,
            engaged: 0.0,
            press: std::env::var("RA2_AI")
                .map(|v| v.eq_ignore_ascii_case("press"))
                .unwrap_or(false),
        }
    }
}

impl Duel {
    /// Build the world and spawn the robots facing each other.
    pub fn new(arena_path: &str, bots: &[BotSetup]) -> Result<Self, Box<dyn std::error::Error>> {
        let obstacle = std::env::var("RA2_OBSTACLE").unwrap_or_else(|_| "none".to_string());
        // Static arena geometry only: script props become movable bodies below.
        std::env::set_var("RA2_OBSTACLE", "none");
        let arena = scene::load_arena(arena_path)?;
        std::env::remove_var("RA2_OBSTACLE");
        let mut prop_defs = scene::load_props(arena_path, &obstacle)?;
        prop_defs.extend(scene::load_havok_props(arena_path)?);

        let mut physics: Box<dyn PhysicsBackend> = Box::new(JoltBackend::new(arena.gravity)?);
        let (verts, indices) = arena.triangle_soup();
        physics.create_body(
            &BodyDesc::new(
                MotionType::Static,
                ShapeDesc::TriMesh {
                    vertices: verts,
                    indices,
                },
            )
            .with_friction(arena.friction)
            .with_restitution(arena.restitution)
            .with_user_data(USER_ARENA),
        )?;

        let mut props = Vec::new();
        for p in prop_defs {
            let desc = BodyDesc::new(
                MotionType::Dynamic,
                ShapeDesc::ConvexHull {
                    points: p.hull_points.clone(),
                },
            )
            .at(p.position)
            .with_rotation(p.rotation)
            .with_friction(p.friction)
            .with_restitution(p.restitution)
            .with_mass(p.mass)
            .with_user_data(USER_PROP);
            if let Ok(body) = physics.create_body(&desc) {
                props.push(PropEntity { body, prop: p });
            }
        }

        let mass_scale = std::env::var("RA2_MASS_SCALE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0);

        // Spawn on the arena's own start points when it declares them — the game
        // puts a robot on one, and they sit on the play surface. The Octagon's
        // are the case that matters: an invented spread lands on its rim, from
        // which a robot drives off a 3.7 m drop and cannot return. Otherwise
        // spread around the arena centre.
        let starts = ra2_render::start_points(arena_path).unwrap_or_default();
        if !starts.is_empty() {
            println!("  arena start points: {}", starts.len());
        }
        let centre = arena.center;
        let spread = ((arena.max - arena.min).length() * 0.12).clamp(2.0, 9.0);
        let n = bots.len().max(1);
        let mut robots = Vec::new();
        for (i, setup) in bots.iter().enumerate() {
            let rig = rig::load(&setup.path)?;
            let angle = i as f32 * std::f32::consts::TAU / n as f32;
            let (x, z, spawn_facing) = match starts.get(i) {
                Some((p, f)) => (p.x, p.z, Some(*f)),
                None => (
                    centre.x + angle.cos() * spread,
                    centre.z + angle.sin() * spread,
                    None,
                ),
            };
            let floor = Self::floor_at(&*physics, &arena, x, z);
            let position = Vec3::new(x, floor + 0.06 - rig.min.y, z);

            // Face where the start point says, else the centre of the arena.
            let to_centre = Vec3::new(centre.x - x, 0.0, centre.z - z);
            let mut dir = to_centre.normalize_or_zero();
            if let Some(f) = spawn_facing {
                if f.length_squared() > 0.5 {
                    dir = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
                }
            }
            let yaw = setup
                .yaw_deg
                .map(f32::to_radians)
                .unwrap_or_else(|| f32::atan2(-dir.x, -dir.z));

            let robot = Robot::spawn(
                &mut *physics,
                rig,
                i,
                position,
                Quat::from_rotation_y(yaw),
                mass_scale,
            )?;
            println!(
                "  robot {i}: {:?} at ({:.2},{:.2},{:.2}) facing {:.0}deg, {:.0} kg, {} parts",
                robot.rig.name,
                position.x,
                position.y,
                position.z,
                yaw.to_degrees(),
                robot.mass,
                robot.rig.components.len()
            );
            robots.push(robot);
        }

        let damage_taken = vec![0.0; robots.len()];
        let contact_pairs = vec![false; robots.len() * robots.len()];
        Ok(Duel {
            physics,
            arena,
            props,
            robots,
            debris: Vec::new(),
            time: 0.0,
            tuning: DriveTuning::default(),
            combat: CombatConfig::default(),
            mass_scale,
            log: Vec::new(),
            damage_taken,
            contact_pairs,
            ai: bots.iter().map(|_| AiState::new()).collect(),
        })
    }

    /// Highest floor point under `(x, z)`.
    fn floor_at(physics: &dyn PhysicsBackend, arena: &ArenaScene, x: f32, z: f32) -> f32 {
        let mut starts = vec![arena.center.y];
        let top = arena.max.y + 10.0;
        if top > arena.center.y {
            starts.push(top);
        }
        for sy in starts {
            if let Some(hit) = physics.ray_cast(Vec3::new(x, sy, z), Vec3::NEG_Y, 500.0) {
                return hit.point.y;
            }
        }
        arena.min.y
    }

    /// Whether any body of robot `a` touches any body of robot `b`.
    ///
    /// A robot is several hinged bodies, so the two root bodies are not enough:
    /// a spinner arm or a wheel can be the thing that meets the opponent.
    pub fn robots_touching(&self, a: usize, b: usize) -> bool {
        let (ra, rb) = (&self.robots[a], &self.robots[b]);
        ra.parts.iter().any(|pa| {
            pa.alive
                && rb
                    .parts
                    .iter()
                    .any(|pb| pb.alive && self.physics.in_contact(pa.body, pb.body))
        })
    }

    /// Advance one step: motors, AI, physics, then combat.
    pub fn step(&mut self, dt: f32) {
        // Motors and AI.
        for i in 0..self.robots.len() {
            if !self.robots[i].is_alive() {
                continue;
            }
            let opponent = (i + 1) % self.robots.len();
            let contact = self.robots_touching(i, opponent);
            let input = self.ai_input(i, opponent, dt, contact);
            // Drive through the motors: the wheel hinges push the robot.
            let tuning = self.tuning;
            self.robots[i].drive(&mut *self.physics, input, &tuning);
        }

        self.advance(dt);
    }

    /// Advance one step driving `inputs` (one per robot) instead of the AI.
    ///
    /// Deterministic driving, for validation and tests: the duel binary uses it
    /// for a head-on ram, which is the only way to *observe* the damage model
    /// reliably — the stock bot's own approach seldom closes to contact at all.
    pub fn step_direct(&mut self, inputs: &[DriveInput], dt: f32) {
        for (i, input) in inputs.iter().enumerate() {
            if i >= self.robots.len() || !self.robots[i].is_alive() {
                continue;
            }
            let tuning = self.tuning;
            self.robots[i].drive(&mut *self.physics, *input, &tuning);
        }

        self.advance(dt);
    }

    /// Step the world and resolve combat; the driving is already done.
    fn advance(&mut self, dt: f32) {
        // The velocities the robots *arrive* with. After the step the collision
        // has already absorbed them, so a blow has to be measured before it
        // lands, and it is the *relative* velocity that sets its size.
        let approach: Vec<Vec3> = (0..self.robots.len())
            .map(|i| self.robots[i].velocity(&*self.physics))
            .collect();

        let info = self.physics.step(dt);
        if !info.complete {
            eprintln!("ra2sim: step at t={:.2}s dropped contacts", self.time);
        }

        self.resolve_combat(dt, &approach);
        self.time += dt;
    }

    /// Test every robot against every other and apply the resulting damage.
    ///
    /// `approach` holds each robot's velocity before the step, so a blow is
    /// priced by the relative speed it arrived with.
    fn resolve_combat(&mut self, dt: f32, approach: &[Vec3]) {
        let n = self.robots.len();
        let mut impacts = Vec::new();
        let mut touching = vec![false; n * n];
        for a in 0..n {
            for b in 0..n {
                if a == b || !self.robots[a].is_alive() || !self.robots[b].is_alive() {
                    continue;
                }
                let approaching = (approach[a] - approach[b]).length();
                let first = !self.contact_pairs[a * n + b];
                if let Some(imp) = combat::detect(
                    &*self.physics,
                    &self.robots,
                    a,
                    b,
                    dt,
                    combat::Contact {
                        approach: approaching,
                        first,
                    },
                    &self.combat,
                ) {
                    touching[a * n + b] = true;
                    impacts.push(imp);
                } else if combat::contacting_parts(&*self.physics, &self.robots[a], &self.robots[b])
                    .is_some()
                {
                    touching[a * n + b] = true;
                }
            }
        }
        self.contact_pairs = touching;
        for imp in &impacts {
            self.damage_taken[imp.target] += imp.damage;
            if self.robots[imp.target].damage(imp.component, imp.damage) {
                self.break_off(imp.target, imp.component, imp.point);
            } else if imp.component == 0 {
                // A hurt chassis caves in before it dies (F9).
                self.buckle_chassis(imp.target);
            }
        }
    }

    /// Deform a robot's chassis to match its damage, and rebake its collider.
    fn buckle_chassis(&mut self, robot: usize) {
        let physics = &mut *self.physics;
        self.robots[robot].buckle(physics);
    }

    /// A component died: detach its subtree as flying debris and rebuild the
    /// robot without it. The chassis ends the fight instead.
    fn break_off(&mut self, robot: usize, component: usize, point: Vec3) {
        if component == 0 {
            self.robots[robot].destroyed = true;
        }
        let velocity = self.robots[robot].velocity(&*self.physics);
        let removed = {
            let physics = &mut *self.physics;
            self.robots[robot].detach(physics, component)
        };

        let mut created = Vec::new();
        for (i, world) in removed {
            let c = &self.robots[robot].rig.components[i];
            if c.hull_points.is_empty() || c.items.is_empty() {
                continue;
            }
            let (_, rotation, position) = world.to_scale_rotation_translation();
            let desc = BodyDesc::new(
                MotionType::Dynamic,
                ShapeDesc::ConvexHull {
                    points: c.hull_points.clone(),
                },
            )
            .at(position)
            .with_rotation(rotation)
            .with_friction(0.5)
            .with_restitution(0.25)
            .with_mass(c.mass.max(0.5))
            .with_user_data(USER_DEBRIS);
            let Ok(body) = self.physics.create_body(&desc) else {
                continue;
            };
            // Fling the part away from the hit and give it a little lift.
            let away = (position - point).normalize_or_zero();
            let impulse = away * c.mass * (1.2 + velocity.length() * 0.25) + Vec3::Y * c.mass * 0.5;
            self.physics.apply_impulse(body, impulse);
            created.push(Debris {
                body,
                items: c.items.clone(),
                label: c.name.clone(),
            });
        }
        self.debris.extend(created);

        let name = self.robots[robot].rig.name.clone();
        let part = self.robots[robot].rig.components[component].name.clone();
        if component == 0 {
            self.log
                .push(format!("t={:.1}s  {name} chassis destroyed", self.time));
        } else {
            self.log
                .push(format!("t={:.1}s  {name}: {part} knocked off", self.time));
        }
    }

    /// Smallest surface gap between the robots' bounding spheres (diagnostics).
    pub fn closest_gap(&self) -> f32 {
        if self.robots.len() < 2 {
            return f32::MAX;
        }
        let (a, b) = (&self.robots[0], &self.robots[1]);
        let wa = a.component_worlds(&*self.physics);
        let wb = b.component_worlds(&*self.physics);
        let mut best = f32::MAX;
        for (i, ca) in a.rig.components.iter().enumerate() {
            if !a.attached[i] || ca.hull_points.is_empty() {
                continue;
            }
            let pa = wa[i].transform_point3(ca.center());
            for (j, cb) in b.rig.components.iter().enumerate() {
                if !b.attached[j] || cb.hull_points.is_empty() {
                    continue;
                }
                let pb = wb[j].transform_point3(cb.center());
                let gap = (pa - pb).length() - ca.radius() - cb.radius();
                if gap < best {
                    best = gap;
                }
            }
        }
        best
    }

    /// Midpoint of the robots still fighting (for the camera).
    pub fn focus(&self) -> Vec3 {
        let alive: Vec<Vec3> = self
            .robots
            .iter()
            .filter(|r| r.is_alive())
            .map(|r| r.position(&*self.physics))
            .collect();
        if alive.is_empty() {
            return self.arena.center;
        }
        alive.iter().copied().sum::<Vec3>() / alive.len() as f32
    }

    /// Render the current frame.
    pub fn render(
        &self,
        renderer: &Renderer,
        camera: &DuelCamera,
        w: u32,
        h: u32,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let (eye, aim) = self.camera_pose(camera);
        let view = ra2_render::look_at_rh(eye, aim, Vec3::Y);
        let proj = ra2_render::perspective_rh(55f32.to_radians(), w as f32 / h as f32, 0.05, 900.0);
        let view_proj = proj * view;

        let mut groups = vec![RenderGroup {
            items: &self.arena.items,
            model: Mat4::IDENTITY,
        }];
        for pe in &self.props {
            let model = self.body_model(pe.body);
            groups.push(RenderGroup {
                items: &pe.prop.items,
                model,
            });
        }
        for d in &self.debris {
            groups.push(RenderGroup {
                items: &d.items,
                model: self.body_model(d.body),
            });
        }
        for r in &self.robots {
            if r.parts[0].alive {
                let worlds = r.component_worlds(&*self.physics);
                for (i, c) in r.rig.components.iter().enumerate() {
                    if !r.attached[i] || c.items.is_empty() {
                        continue;
                    }
                    groups.push(RenderGroup {
                        items: &c.items,
                        model: worlds[i],
                    });
                }
            }
        }

        renderer.render_groups(&groups, view_proj, w, h)
    }

    fn body_model(&self, body: BodyHandle) -> Mat4 {
        self.physics
            .state(body)
            .map(|s| Mat4::from_rotation_translation(s.rotation, s.position))
            .unwrap_or(Mat4::IDENTITY)
    }

    fn camera_pose(&self, camera: &DuelCamera) -> (Vec3, Vec3) {
        match *camera {
            DuelCamera::Auto => {
                let focus = self.focus();
                let separation = match (self.robots.first(), self.robots.get(1)) {
                    (Some(a), Some(b)) => {
                        (a.position(&*self.physics) - b.position(&*self.physics)).length()
                    }
                    _ => 4.0,
                };
                let dist = (separation * 0.85 + 4.0).clamp(5.0, 40.0);
                let dir = Vec3::new(0.72, 0.55, 0.72).normalize();
                (focus + dir * dist, focus)
            }
            DuelCamera::Wide { offset } => {
                let focus = self.focus();
                (focus + offset, focus)
            }
            DuelCamera::Chase {
                robot,
                distance,
                height,
            } => {
                let r = &self.robots[robot.min(self.robots.len() - 1)];
                let target = r.position(&*self.physics);
                let fwd = self
                    .physics
                    .state(r.body())
                    .map(|s| (s.rotation * BOT_FORWARD).normalize_or_zero())
                    .unwrap_or(BOT_FORWARD);
                (
                    target - fwd * distance + Vec3::Y * height,
                    target + fwd * 0.4,
                )
            }
            DuelCamera::Top { height } => {
                let focus = self.focus();
                (focus + Vec3::new(0.02, height, 0.02), focus)
            }
        }
    }
}

/// How a duel frame is framed.
#[derive(Clone, Copy, Debug)]
pub enum DuelCamera {
    /// Frame both robots, from a raised three-quarter view.
    Auto,
    /// A fixed offset from the midpoint of the fight.
    Wide { offset: Vec3 },
    /// Behind one robot's heading.
    Chase {
        robot: usize,
        distance: f32,
        height: f32,
    },
    /// Looking straight down.
    Top { height: f32 },
}

/// A simple driver.
///
/// The default behaviour is to press into the opponent: RA2 fights are grinds,
/// and damage accrues while the hulls are engaged. `RA2_AI=joust` switches to
/// charge/back-off passes instead (more dramatic run-ups, less contact time).
fn ai_drive(
    me: &Robot,
    target: &Robot,
    physics: &dyn PhysicsBackend,
    state: &mut AiState,
    dt: f32,
    contact: bool,
) -> DriveInput {
    let from = me.position(physics);
    let to = target.position(physics);
    let delta = Vec3::new(to.x - from.x, 0.0, to.z - from.z);
    let _distance = delta.length();

    let rot = physics
        .state(me.body())
        .map(|s| s.rotation)
        .unwrap_or(Quat::IDENTITY);
    let fwd = (rot * BOT_FORWARD).normalize_or_zero();
    let fwd = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
    let to = delta.normalize_or_zero();

    let dot = fwd.dot(to).clamp(-1.0, 1.0);
    let cross = fwd.cross(to).y;
    let angle = cross.atan2(dot);
    // The controller applies yaw = -steer * rate, so negate the heading error.
    let steer = (-angle * 2.2).clamp(-1.0, 1.0);

    if state.press {
        return DriveInput {
            throttle: 1.0,
            steer,
        };
    }

    // The physics engine tells us when the hulls actually meet (estimating it
    // from bounding radii made the robot flinch away before ever touching).
    state.timer -= dt;
    match state.mode {
        AiMode::Charge => {
            state.engaged = if contact { state.engaged + dt } else { 0.0 };
            // Grind for a bit, then pull out for another run so the fight moves.
            if state.engaged > 1.6 || state.timer <= 0.0 {
                state.mode = AiMode::BackOff;
                state.timer = 0.8;
                state.engaged = 0.0;
            }
            DriveInput {
                throttle: 1.0,
                steer,
            }
        }
        AiMode::BackOff => {
            if state.timer <= 0.0 {
                state.mode = AiMode::Charge;
                state.timer = 3.0;
            }
            DriveInput {
                throttle: -1.0,
                steer: 0.0,
            }
        }
    }
}

impl Duel {
    fn ai_input(&mut self, i: usize, opponent: usize, dt: f32, contact: bool) -> DriveInput {
        let mut state = self.ai[i];
        let input = ai_drive(
            &self.robots[i],
            &self.robots[opponent],
            &*self.physics,
            &mut state,
            dt,
            contact,
        );
        self.ai[i] = state;
        input
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The (uncommitted) game directory, when present.
    fn game_dir() -> Option<std::path::PathBuf> {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join("local/game");
            if candidate.is_dir() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// F7: a ram is an *event*, sized by the speed the robots arrive with.
    ///
    /// Two identical bots are driven straight ahead from the arena's own start
    /// points, which face the centre, so they meet head-on. The blow has to land
    /// in the range the original's own scripts show — its hazards apply a flat
    /// 25 and its AI calls anything over 10 "a good solid hit" — and then, once
    /// the two are resting against each other, contact must stop doing damage
    /// (integrating the arrival speed over the contact, as the model used to,
    /// erodes both robots from a standstill).
    #[test]
    fn a_ram_is_a_blow_and_resting_contact_does_nothing() {
        let Some(game) = game_dir() else { return };
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = game.join("AI/Team1/Bot0.bot").to_string_lossy().to_string();
        let setups = [
            BotSetup {
                path: bot.clone(),
                yaw_deg: None,
            },
            BotSetup {
                path: bot,
                yaw_deg: None,
            },
        ];
        let mut duel = Duel::new(&arena, &setups).expect("the box arena builds a duel");
        let dt = 1.0 / 60.0;

        // Run at each other until they touch.
        let ram = vec![
            DriveInput {
                throttle: 1.0,
                steer: 0.0,
            };
            2
        ];
        let mut blow = 0.0;
        for _ in 0..(6.0 / dt) as usize {
            duel.step_direct(&ram, dt);
            if blow == 0.0 && duel.damage_taken[0] > 0.0 {
                blow = duel.damage_taken[0];
            }
        }
        assert!(blow > 10.0, "the opening ram was only {blow} damage");
        assert!(blow < 400.0, "the opening ram was {blow} damage");

        // Now stand still on each other for two seconds.
        let rest = vec![DriveInput::default(); 2];
        let settled = duel.damage_taken[0];
        for _ in 0..(2.0 / dt) as usize {
            duel.step_direct(&rest, dt);
        }
        let after = duel.damage_taken[0] - settled;
        assert!(
            after < blow * 0.25,
            "resting contact added {after} damage after a {blow} blow"
        );
    }

    /// F9: a hurt chassis caves in — its drawn shape *and* its collider shrink —
    /// instead of only ever dying whole.
    ///
    /// The bot file marks which points may move ("down and in"); the floor plate
    /// is pinned by the lowest of them, and the motion grows with height above
    /// it. All 57 stock bots ship uncrushed, so this is a change we make, not one
    /// we read back.
    #[test]
    fn a_hurt_chassis_buckles() {
        let Some(game) = game_dir() else { return };
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = game.join("AI/Team1/Bot0.bot").to_string_lossy().to_string();
        let setups = [BotSetup {
            path: bot,
            yaw_deg: None,
        }];
        let mut duel = Duel::new(&arena, &setups).expect("the box arena builds a duel");
        assert!(
            !duel.robots[0].rig.components[0].buckle.is_empty(),
            "the bot carries a buckle model"
        );

        // What the chassis looks like from outside, over the region the file
        // marks as movable: the floor plate is pinned and the sides above it
        // cave, so the measures that can move are the movable footprint and the
        // top.
        let floor = duel.robots[0].rig.components[0]
            .buckle
            .iter()
            .map(|p| p.y)
            .fold(f32::MAX, f32::min);
        // `(movable footprint, top, lowest point)` over the region above the
        // buckle floor.
        let extent_of = |points: &[Vec3]| {
            let mut above = (0.0f32, f32::MIN);
            let mut planted = f32::MAX;
            for p in points {
                planted = planted.min(p.y);
                if p.y > floor + 1e-3 {
                    above.0 = above.0.max(p.x.abs()).max(p.z.abs());
                    above.1 = above.1.max(p.y);
                }
            }
            (above.0, above.1, planted)
        };
        let hull_of = |d: &Duel| d.robots[0].rig.components[0].hull_points.clone();
        let draw_of = |d: &Duel| -> Vec<Vec3> {
            d.robots[0].rig.components[0]
                .items
                .iter()
                .flat_map(|i| i.vertices.iter())
                .map(|v| Vec3::from(v.pos))
                .collect()
        };
        let (draw0, hull0) = (extent_of(&draw_of(&duel)), extent_of(&hull_of(&duel)));

        // Halfway to a destroyed chassis.
        duel.robots[0].health[0] *= 0.5;
        duel.robots[0].buckle(&mut *duel.physics);
        let (draw1, hull1) = (extent_of(&draw_of(&duel)), extent_of(&hull_of(&duel)));

        assert!(
            (duel.robots[0].rig.components[0].buckle_fraction - 0.5).abs() < 0.01,
            "buckled to {:?}",
            duel.robots[0].rig.components[0].buckle_fraction
        );
        for (name, before, after) in [("drawn", draw0, draw1), ("collider", hull0, hull1)] {
            assert!(
                after.0 < before.0 - 1e-3,
                "{name} footprint did not shrink: {} -> {}",
                before.0,
                after.0
            );
            assert!(
                after.1 < before.1 - 1e-3,
                "{name} top did not come down: {} -> {}",
                before.1,
                after.1
            );
            assert!(
                (after.2 - before.2).abs() < 1e-3,
                "{name} floor moved: {} -> {}",
                before.2,
                after.2
            );
        }
        // And it is monotone: more damage, more crush.
        let before = duel.robots[0].rig.components[0].items[0].vertices[0].pos;
        duel.robots[0].health[0] *= 0.5;
        duel.robots[0].buckle(&mut *duel.physics);
        let after = duel.robots[0].rig.components[0].items[0].vertices[0].pos;
        assert!(after[1] <= before[1], "the crush must not reverse");
    }

    /// F7: contact is tested between every pair of the robots' bodies, not just
    /// their root bodies.
    #[test]
    fn contact_spans_a_robots_own_bodies() {
        let Some(game) = game_dir() else { return };
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = game.join("AI/Team1/Bot0.bot").to_string_lossy().to_string();
        let setups = [
            BotSetup {
                path: bot.clone(),
                yaw_deg: None,
            },
            BotSetup {
                path: bot,
                yaw_deg: None,
            },
        ];
        let mut duel = Duel::new(&arena, &setups).expect("the box arena builds a duel");
        assert!(!duel.robots_touching(0, 1), "start points are apart");
        let ram = vec![
            DriveInput {
                throttle: 1.0,
                steer: 0.0,
            };
            2
        ];
        let dt = 1.0 / 60.0;
        let mut touched = false;
        for _ in 0..(8.0 / dt) as usize {
            duel.step_direct(&ram, dt);
            if duel.robots_touching(0, 1) {
                touched = true;
                break;
            }
        }
        assert!(touched, "a head-on ram never registered contact");
    }
}
