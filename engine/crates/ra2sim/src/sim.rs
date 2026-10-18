//! The headless robot sandbox: one jointed robot in an arena, scripted input.
//!
//! The robot is assembled exactly the way `ra2-duel` assembles one: `rig::load`
//! splits the `.bot` into one rigid body per rigidly-mounted group, every motor
//! becomes a hinge, and the wheels are driven through those hinges. There is no
//! arcade velocity controller any more — see F2 in
//! `docs/06-deferred-fidelity.md`.

use glam::{Mat4, Quat, Vec3};
use ra2_render::{RenderGroup, Renderer};

use crate::backend::{BodyDesc, BodyHandle, MotionType, PhysicsBackend, ShapeDesc};
use crate::jolt::JoltBackend;
use crate::rig::{self, RobotRig};
use crate::robot::Robot;
use crate::scene::{self, ArenaScene, Prop, USER_ARENA, USER_PROP};

/// Forward is -Z in RA2 model space.
pub const BOT_FORWARD: Vec3 = Vec3::new(0.0, 0.0, -1.0);

/// Driver input for one instant.
///
/// `throttle` and `steer` (each in `[-1, 1]`) are a stand-in driver's
/// `Forward`/`LeftRight` keys; `keys` are the DirectInput scan codes a human
/// holds, which the robot's own wiring maps to controls. Both reach the motors
/// only through the bot's wiring (see [`crate::controls`]).
#[derive(Clone, Copy, Debug, Default)]
pub struct DriveInput {
    pub throttle: f32,
    pub steer: f32,
    pub keys: crate::controls::KeySet,
}

impl DriveInput {
    /// A stand-in driver's `throttle`/`steer` with no keys held.
    pub fn new(throttle: f32, steer: f32) -> Self {
        DriveInput {
            throttle,
            steer,
            keys: Default::default(),
        }
    }
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
                return DriveInput::new(k.throttle, k.steer);
            }
        }
        DriveInput::default()
    }
}

/// Tuning for the drive that is not a property of the parts themselves.
///
/// Nothing is left: a robot's speed is its motors' (`power` in the component
/// definitions) and it turns because its wiring drives the sides differently.
/// The struct stays so a future tuning has a place to go.
#[derive(Clone, Copy, Debug, Default)]
pub struct DriveTuning {}

/// A movable prop (crate, barrel, ...) placed in the world.
pub struct PropEntity {
    pub body: BodyHandle,
    pub prop: Prop,
}

/// A live sandbox: physics world, arena, props and one jointed robot.
pub struct Sandbox {
    pub physics: Box<dyn PhysicsBackend>,
    pub arena: ArenaScene,
    pub props: Vec<PropEntity>,
    pub robot: Robot,
    pub tuning: DriveTuning,
    pub time: f32,
}

impl Sandbox {
    /// Assemble the world: static arena collider, then the robot above the floor.
    pub fn new(arena_path: &str, bot_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let obstacle = std::env::var("RA2_OBSTACLE").unwrap_or_else(|_| "none".to_string());
        // Load the arena's static geometry only: props from the script's
        // `AddXtra` meshes become independent movable bodies below.
        std::env::set_var("RA2_OBSTACLE", "none");
        let arena = scene::load_arena(arena_path)?;
        std::env::remove_var("RA2_OBSTACLE");
        let mut prop_defs = scene::load_props(arena_path, &obstacle)?;
        // The arena's own rigid bodies are movable props too (start blocks,
        // hammers, floor saws); they use the game's own mass/friction/elasticity.
        prop_defs.extend(scene::load_havok_props(arena_path)?);

        let rig = rig::load(bot_path)?;
        let mass_scale = mass_scale(rig.mass);
        let (components, parts) = (rig.components.len(), rig.parts.len());

        let mut physics: Box<dyn PhysicsBackend> = Box::new(JoltBackend::new(arena.gravity)?);

        // Static arena geometry.
        let (verts, indices) = arena.triangle_soup();
        let arena_desc = BodyDesc::new(
            MotionType::Static,
            ShapeDesc::TriMesh {
                vertices: verts,
                indices,
            },
        )
        .with_friction(arena.friction)
        .with_restitution(arena.restitution)
        .with_user_data(USER_ARENA);
        physics.create_body(&arena_desc)?;

        // Spawn the robot with its lowest point just above the floor.
        let spawn = Self::find_spawn(&*physics, &arena, &rig);
        // Optional spawn heading (degrees about +Y), e.g. to face a prop.
        let yaw = std::env::var("RA2_YAW")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.0);
        let robot = Robot::spawn(
            &mut *physics,
            rig,
            0,
            spawn,
            Quat::from_rotation_y(yaw.to_radians()),
            mass_scale,
        )?;

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
            "sandbox: backend={} arena_tris={} props={} components={} parts={} mass={:.0}kg spawn={:.2},{:.2},{:.2}",
            physics.name(),
            arena.triangles.len(),
            props.len(),
            components,
            parts,
            robot.mass,
            spawn.x,
            spawn.y,
            spawn.z,
        );

        Ok(Sandbox {
            physics,
            arena,
            props,
            robot,
            tuning: DriveTuning::default(),
            time: 0.0,
        })
    }

    /// Highest floor point under a few candidate spawn locations.
    fn find_spawn(physics: &dyn PhysicsBackend, arena: &ArenaScene, rig: &RobotRig) -> Vec3 {
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
        // Place the robot's lowest point 6 cm above the floor, so it settles
        // onto its wheels rather than starting in contact.
        Vec3::new(chosen.0, floor + 0.06 - rig.min.y, chosen.1)
    }

    /// Apply one step of scripted input, then advance the world.
    pub fn step(&mut self, input: DriveInput, dt: f32) {
        self.robot.drive(&mut *self.physics, input, &self.tuning);
        let info = self.physics.step(dt);
        if !info.complete {
            eprintln!("ra2sim: step at t={:.2}s dropped contacts", self.time);
        }
        self.time += dt;
    }

    /// World-space transform of the robot.
    pub fn bot_transform(&self) -> Mat4 {
        self.robot.transform(&*self.physics)
    }

    pub fn bot_position(&self) -> Vec3 {
        self.robot.position(&*self.physics)
    }

    pub fn bot_speed(&self) -> f32 {
        self.robot.velocity(&*self.physics).length()
    }

    /// Bounding radius of the robot in its rest pose, for framing the camera.
    pub fn bot_size(&self) -> f32 {
        ((self.robot.rig.max - self.robot.rig.min) * 0.5).length()
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
            .bot_transform()
            .transform_vector3(self.robot.rig.forward())
            .normalize_or_zero();

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
        // The robot's parts come from the same source the duel renders them
        // from: each component's own batches at its simulated world transform.
        if self.robot.parts[0].alive {
            let worlds = self.robot.component_worlds(&*self.physics);
            for (i, c) in self.robot.rig.components.iter().enumerate() {
                if !self.robot.attached[i] || c.items.is_empty() {
                    continue;
                }
                groups.push(RenderGroup {
                    items: &c.items,
                    model: worlds[i],
                });
            }
        }
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

/// How far to scale the robot's natural mass.
///
/// `RA2_MASS_SCALE` multiplies it (the same knob `ra2-duel` uses); the older
/// absolute `RA2_MASS=<kg>` is still honoured by turning it back into a scale.
fn mass_scale(natural: f32) -> f32 {
    if let Some(s) = std::env::var("RA2_MASS_SCALE")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
    {
        return s;
    }
    if let Some(kg) = std::env::var("RA2_MASS")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
    {
        if natural > 0.0 {
            return kg / natural;
        }
    }
    1.0
}
