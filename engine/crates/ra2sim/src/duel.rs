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

        // Spawn positions: spread along X around the arena centre.
        let centre = arena.center;
        let spread = ((arena.max - arena.min).length() * 0.12).clamp(2.0, 9.0);
        let n = bots.len().max(1);
        let mut robots = Vec::new();
        for (i, setup) in bots.iter().enumerate() {
            let rig = rig::load(&setup.path)?;
            let angle = i as f32 * std::f32::consts::TAU / n as f32;
            let (x, z) = (
                centre.x + angle.cos() * spread,
                centre.z + angle.sin() * spread,
            );
            let floor = Self::floor_at(&*physics, &arena, x, z);
            let position = Vec3::new(x, floor + 0.06 - rig.min.y, z);

            // Face the centre of the arena.
            let to_centre = Vec3::new(centre.x - x, 0.0, centre.z - z);
            let dir = to_centre.normalize_or_zero();
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

    /// Advance one step: motors, AI, physics, then combat.
    pub fn step(&mut self, dt: f32) {
        // Motors and AI.
        for i in 0..self.robots.len() {
            if !self.robots[i].is_alive() {
                continue;
            }
            let opponent = (i + 1) % self.robots.len();
            let contact = self
                .physics
                .in_contact(self.robots[i].body(), self.robots[opponent].body());
            let input = self.ai_input(i, opponent, dt, contact);
            // Drive through the motors: the wheel hinges push the robot.
            let tuning = self.tuning;
            self.robots[i].drive(&mut *self.physics, input, &tuning);
        }

        let info = self.physics.step(dt);
        if !info.complete {
            eprintln!("ra2sim: step at t={:.2}s dropped contacts", self.time);
        }

        self.resolve_combat(dt);
        self.time += dt;
    }

    /// Test every robot against every other and apply the resulting damage.
    fn resolve_combat(&mut self, dt: f32) {
        let n = self.robots.len();
        let mut impacts = Vec::new();
        for a in 0..n {
            for b in 0..n {
                if a == b || !self.robots[a].is_alive() || !self.robots[b].is_alive() {
                    continue;
                }
                if let Some(imp) =
                    combat::detect(&*self.physics, &self.robots, a, b, dt, &self.combat)
                {
                    impacts.push(imp);
                }
            }
        }
        for imp in &impacts {
            self.damage_taken[imp.target] += imp.damage;
            if self.robots[imp.target].damage(imp.component, imp.damage) {
                self.break_off(imp.target, imp.component, imp.point);
            }
        }
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
