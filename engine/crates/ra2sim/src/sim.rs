//! The M2 physics sandbox: an arena, a robot, and an arcade drive controller.

use glam::{Mat4, Quat, Vec3};
use ra2_render::{RenderGroup, Renderer};

use crate::backend::{BodyDesc, BodyHandle, MotionType, PhysicsBackend, ShapeDesc};
use crate::jolt::JoltBackend;
use crate::scene::{self, ArenaScene, BotRig, USER_ARENA, USER_BOT, USER_PROP};

/// Driver input for one instant, each in `[-1, 1]`.
#[derive(Clone, Copy, Debug, Default)]
pub struct DriveInput {
    pub throttle: f32,
    pub steer: f32,
}

/// A single time-segment of a scripted drive.
#[derive(Clone, Copy, Debug)]
struct DriveKey {
    until: f32,
    throttle: f32,
    steer: f32,
}

/// A scripted driving timeline, e.g. `"0-2:1,0;2-4:1,1;4-6:0,0"`.
#[derive(Clone, Debug, Default)]
pub struct DriveScript {
    keys: Vec<DriveKey>,
}

impl DriveScript {
    /// Parse `"<start>-<end>:<throttle>,<steer>; ..."` (times in seconds).
    pub fn parse(spec: &str) -> DriveScript {
        let mut keys = Vec::new();
        for seg in spec.split(';') {
            let seg = seg.trim();
            if seg.is_empty() {
                continue;
            }
            let Some((range, values)) = seg.split_once(':') else {
                continue;
            };
            let end = range
                .split_once('-')
                .and_then(|(_, e)| e.trim().parse::<f32>().ok());
            let mut nums = values
                .split(',')
                .filter_map(|v| v.trim().parse::<f32>().ok());
            let (throttle, steer) = (nums.next().unwrap_or(0.0), nums.next().unwrap_or(0.0));
            if let Some(until) = end {
                keys.push(DriveKey {
                    until,
                    throttle,
                    steer,
                });
            }
        }
        keys.sort_by(|a, b| a.until.total_cmp(&b.until));
        DriveScript { keys }
    }

    /// The input active at time `t` seconds.
    pub fn at(&self, t: f32) -> DriveInput {
        for k in &self.keys {
            if t < k.until {
                return DriveInput {
                    throttle: k.throttle,
                    steer: k.steer,
                };
            }
        }
        DriveInput::default()
    }
}

/// Tuning for the arcade controller.
#[derive(Clone, Copy, Debug)]
pub struct DriveTuning {
    /// Forward acceleration at full throttle (m/s²).
    pub accel: f32,
    /// Speed the controller stops accelerating past (m/s).
    pub max_speed: f32,
    /// Lateral velocity cancelled per step (0..1).
    pub grip: f32,
    /// Yaw acceleration at full steer (rad/s²).
    pub turn_accel: f32,
    /// Cap on yaw rate (rad/s).
    pub max_yaw_rate: f32,
}

impl Default for DriveTuning {
    fn default() -> Self {
        DriveTuning {
            accel: 7.0,
            max_speed: 7.0,
            grip: 0.35,
            turn_accel: 9.0,
            max_yaw_rate: 2.2,
        }
    }
}

/// A movable prop (crate, barrel, ...) placed in the world.
pub struct PropEntity {
    pub body: BodyHandle,
    pub prop: scene::Prop,
}

/// A live sandbox: physics world, arena, robot, and controller.
pub struct Sandbox {
    pub physics: Box<dyn PhysicsBackend>,
    pub arena: ArenaScene,
    pub bot: BotRig,
    pub bot_body: BodyHandle,
    pub props: Vec<PropEntity>,
    pub tuning: DriveTuning,
    pub mass: f32,
    pub time: f32,
    /// Robot-local "forward" axis (RA2 robots face -Z).
    pub forward: Vec3,
}

/// Forward is -Z in RA2 model space.
pub const BOT_FORWARD: Vec3 = Vec3::new(0.0, 0.0, -1.0);

impl Sandbox {
    /// Assemble the world: static arena collider, then the robot above the floor.
    pub fn new(arena_path: &str, bot_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let obstacle = std::env::var("RA2_OBSTACLE").unwrap_or_else(|_| "none".to_string());
        // Load the arena's static geometry only: props from the script's
        // `AddXtra` meshes become independent movable bodies below.
        std::env::set_var("RA2_OBSTACLE", "none");
        let arena = scene::load_arena(arena_path)?;
        std::env::remove_var("RA2_OBSTACLE");
        let prop_defs = scene::load_props(arena_path, &obstacle)?;
        let mass = std::env::var("RA2_MASS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(120.0);
        let bot = scene::load_bot(bot_path, mass)?;

        let mut physics: Box<dyn PhysicsBackend> =
            Box::new(JoltBackend::new(Vec3::new(0.0, -9.81, 0.0))?);

        // Static arena geometry.
        let (verts, indices) = arena.triangle_soup();
        let arena_desc = BodyDesc::new(
            MotionType::Static,
            ShapeDesc::TriMesh {
                vertices: verts,
                indices,
            },
        )
        .with_friction(0.9)
        .with_restitution(0.05)
        .with_user_data(USER_ARENA);
        physics.create_body(&arena_desc)?;

        // Spawn the robot just above the floor under the arena centre.
        let spawn = Self::find_spawn(&*physics, &arena, &bot);
        // Optional spawn heading (degrees about +Y), e.g. to face a prop.
        let yaw = std::env::var("RA2_YAW")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.0);
        let hull = ShapeDesc::ConvexHull {
            points: bot.hull_points.clone(),
        };
        let bot_desc = BodyDesc::new(MotionType::Dynamic, hull)
            .at(spawn)
            .with_rotation(Quat::from_rotation_y(yaw.to_radians()))
            // Low ground friction: the arcade controller models tyre grip
            // explicitly (a sliding hull would otherwise be pinned by friction).
            .with_friction(0.15)
            .with_restitution(0.15)
            .with_mass(mass)
            .with_damping(0.05, 0.6)
            .with_user_data(USER_BOT);
        let bot_body = physics.create_body(&bot_desc)?;

        // Movable props (arena obstacles with embedded rigid bodies).
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
            match physics.create_body(&desc) {
                Ok(body) => props.push(PropEntity { body, prop: p }),
                Err(e) => eprintln!("ra2sim: prop {} skipped: {e}", p.name),
            }
        }

        println!(
            "sandbox: backend={} arena_tris={} props={} bot_hull_pts={} mass={}kg spawn={:.2},{:.2},{:.2}",
            physics.name(),
            arena.triangles.len(),
            props.len(),
            bot.hull_points.len(),
            mass,
            spawn.x,
            spawn.y,
            spawn.z,
        );

        Ok(Sandbox {
            physics,
            arena,
            bot,
            bot_body,
            props,
            tuning: DriveTuning::default(),
            mass,
            time: 0.0,
            forward: BOT_FORWARD,
        })
    }

    /// Highest floor point under a few candidate spawn locations.
    fn find_spawn(physics: &dyn PhysicsBackend, arena: &ArenaScene, bot: &BotRig) -> Vec3 {
        let c = arena.center;
        // Start the probe at the arena's mid-height first (rooms have a roof
        // above; an open arena has nothing up there), then try from the top.
        let mut starts = vec![c.y];
        let top = arena.max.y + 10.0;
        if top > c.y {
            starts.push(top);
        }

        let candidates: Vec<(f32, f32)> = {
            // `RA2_SPAWN=x,z` overrides the spawn point (height is still probed).
            if let Ok(spec) = std::env::var("RA2_SPAWN") {
                let v: Vec<f32> = spec
                    .split(',')
                    .filter_map(|x| x.trim().parse().ok())
                    .collect();
                if v.len() >= 2 {
                    vec![(v[0], v[1])]
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
            .into_iter()
            .chain({
                let mut v = vec![(c.x, c.z)];
                for r in [2.0f32, 4.0, 7.0] {
                    for k in 0..8 {
                        let a = k as f32 * std::f32::consts::TAU / 8.0;
                        v.push((c.x + r * a.cos(), c.z + r * a.sin()));
                    }
                }
                v
            })
            .collect()
        };

        let mut floor: Option<f32> = None;
        let mut chosen = (c.x, c.z);
        'outer: for (x, z) in candidates {
            for &sy in &starts {
                if let Some(hit) = physics.ray_cast(Vec3::new(x, sy, z), Vec3::NEG_Y, 500.0) {
                    floor = Some(hit.point.y);
                    chosen = (x, z);
                    break 'outer;
                }
            }
        }
        let floor = floor.unwrap_or(arena.min.y);
        // Place the hull bottom 5 cm above the floor.
        Vec3::new(chosen.0, floor + 0.05 - bot.min.y, chosen.1)
    }

    /// Apply one step of scripted input, then advance the world.
    pub fn step(&mut self, input: DriveInput, dt: f32) {
        self.apply_drive(input, dt);
        let info = self.physics.step(dt);
        if !info.complete {
            eprintln!("ra2sim: step at t={:.2}s dropped contacts", self.time);
        }
        self.time += dt;
    }

    /// Arcade controller: drive the horizontal velocity toward the target and
    /// set the yaw rate. Modelled as a velocity controller rather than wheel
    /// torque, so it stays predictable while the collider is a single hull.
    fn apply_drive(&mut self, input: DriveInput, dt: f32) {
        let Some(st) = self.physics.state(self.bot_body) else {
            return;
        };
        let rot = st.rotation;
        let fwd = (rot * self.forward).normalize_or_zero();
        let right = (rot * Vec3::X).normalize_or_zero();
        let up = (rot * Vec3::Y).normalize_or_zero();

        let v = st.linear_velocity;
        let (v_forward, v_lateral, v_vertical) = (v.dot(fwd), v.dot(right), v.dot(up));

        // Forward speed moves toward the throttle target at a bounded rate.
        let throttle = input.throttle.clamp(-1.0, 1.0);
        let target = throttle * self.tuning.max_speed;
        let step = self.tuning.accel * dt;
        let new_forward = v_forward + (target - v_forward).clamp(-step, step);
        // Grip bleeds off sideways velocity.
        let new_lateral = v_lateral * (1.0 - self.tuning.grip).clamp(0.0, 1.0);

        let linear = fwd * new_forward + right * new_lateral + up * v_vertical;

        // Steering: replace the yaw component, keeping the rest of the spin.
        let steer = input.steer.clamp(-1.0, 1.0);
        let speed_ratio = (v_forward.abs() / self.tuning.max_speed).clamp(0.0, 1.0);
        let authority = (0.35 + 0.65 * speed_ratio).min(1.0);
        let yaw = -steer * self.tuning.max_yaw_rate * authority;
        let spin = st.angular_velocity;
        let angular = spin - up * spin.dot(up) + up * yaw;

        self.physics.set_velocity(self.bot_body, linear, angular);
    }

    /// World-space transform of the robot.
    pub fn bot_transform(&self) -> Mat4 {
        match self.physics.state(self.bot_body) {
            Some(st) => Mat4::from_rotation_translation(st.rotation, st.position),
            None => Mat4::IDENTITY,
        }
    }

    pub fn bot_position(&self) -> Vec3 {
        self.physics
            .state(self.bot_body)
            .map(|s| s.position)
            .unwrap_or(Vec3::ZERO)
    }

    pub fn bot_speed(&self) -> f32 {
        self.physics
            .state(self.bot_body)
            .map(|s| s.linear_velocity.length())
            .unwrap_or(0.0)
    }

    /// Render the current frame with the given camera.
    pub fn render(
        &self,
        renderer: &Renderer,
        camera: &CameraMode,
        w: u32,
        h: u32,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let target = self.bot_position();
        let fwd = self
            .physics
            .state(self.bot_body)
            .map(|s| (s.rotation * self.forward).normalize_or_zero())
            .unwrap_or(self.forward);

        let (eye, aim) = match *camera {
            // Follow behind the robot's heading, so it stays in frame even when
            // it drives into a corner of a cluttered indoor arena.
            CameraMode::Chase { distance, height } => (
                target - fwd * distance + Vec3::Y * height,
                target + fwd * 0.4,
            ),
            CameraMode::Fixed(offset) => (target + offset, target),
            CameraMode::Top { height } => (target + Vec3::new(0.02, height, 0.02), target),
        };

        let view = ra2_render::look_at_rh(eye, aim, Vec3::Y);
        let proj = ra2_render::perspective_rh(55f32.to_radians(), w as f32 / h as f32, 0.05, 800.0);
        let view_proj = proj * view;

        let mut groups = vec![RenderGroup {
            items: &self.arena.items,
            model: Mat4::IDENTITY,
        }];
        for pe in &self.props {
            let model = self
                .physics
                .state(pe.body)
                .map(|s| Mat4::from_rotation_translation(s.rotation, s.position))
                .unwrap_or(Mat4::IDENTITY);
            groups.push(RenderGroup {
                items: &pe.prop.items,
                model,
            });
        }
        groups.push(RenderGroup {
            items: &self.bot.items,
            model: self.bot_transform(),
        });
        renderer.render_groups(&groups, view_proj, w, h)
    }
}

/// How the sandbox frames the robot.
#[derive(Clone, Copy, Debug)]
pub enum CameraMode {
    /// Behind the robot's heading (the default).
    Chase { distance: f32, height: f32 },
    /// A fixed world-space offset from the robot (debugging).
    Fixed(Vec3),
    /// Looking straight down, for a floor plan of the arena.
    Top { height: f32 },
}

impl CameraMode {
    /// A chase distance/height scaled to a robot of `size` (bounding radius).
    pub fn chase_for(size: f32) -> CameraMode {
        let s = size.max(0.4);
        CameraMode::Chase {
            distance: s * 2.6 + 0.8,
            height: s * 1.9 + 0.5,
        }
    }
}
