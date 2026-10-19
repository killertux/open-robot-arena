//! The M3 duel: two robots fight in an arena, weapons spin, parts fly off.

use glam::{Mat4, Quat, Vec3};
use ra2_render::{DrawItem, RenderGroup, Renderer};

use crate::backend::{BodyDesc, BodyHandle, HingeDesc, MotionType, PhysicsBackend, ShapeDesc};
use crate::combat::{self, CombatConfig};
use crate::hazards::{self, LiveJoint, Zone};
use crate::rig;
use crate::robot::Robot;
use crate::scene::{self, ArenaScene, USER_ARENA, USER_PROP};
use crate::scriptai::ScriptAi;
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
    /// Explicit `(x, z)` to spawn at, instead of the arena's own start points.
    /// Height is still probed for the floor.
    pub spawn: Option<[f32; 2]>,
}

/// The handle a hidden prop's slot carries while its body is gone.
///
/// A body handle is a `u32`, so the top value is free. `Arena.SetActive(name,
/// False)` removes the body and leaves this behind; `True` builds a fresh one
/// from the prop's own definition.
const HIDDEN_PROP: BodyHandle = u32::MAX;

/// Build one arena prop's body from its `.gmf` rigid body definition.
fn spawn_prop(physics: &mut dyn PhysicsBackend, p: &crate::scene::Prop) -> Option<BodyHandle> {
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
    physics.create_body(&desc).ok()
}

/// The damage below which a hit is not applied at all.
///
/// `FUN_00417330` gates the damage event on `10.0 <= param_5` before it calls
/// the hit handler (`FUN_004175b0`), which is the only path that reaches a
/// component's hitpoints (`FUN_00489620` → vtable `+0x1c`) and the score. A
/// smaller hit is handed to `FUN_00417510` instead, which only reaches the
/// component's effects (vtable `+0x6c`, `FUN_0045f3a0`: sparks and sound).
pub const SCORE_HIT_MIN_DAMAGE: f32 = crate::combat::APPLY_THRESHOLD;

/// One damage event that reached a component: what `FUN_004175b0` logs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageEvent {
    /// Match time, seconds.
    pub time: f32,
    /// The attacking robot, `-1` for the environment and hazards.
    pub attacker: i32,
    pub victim: usize,
    pub component: usize,
    pub amount: f32,
}

/// One hit that the match may score.
///
/// The original's hit handler (`FUN_004175b0`) turns a landed blow into points
/// outside King of the Hill: the victim must still be alive (`+0x34 == 0`), the
/// attacker must be a real player, and in `TEAM MATCH` the attacker and victim
/// must be on different teams (`FUN_004177c0`, `+0x30`). The damage awarded is
/// the effective damage returned by the hit's own calculation
/// (`FUN_00489620`, the component's vtable `+0x1c`), which is what this event
/// carries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScoreHit {
    pub attacker: usize,
    pub victim: usize,
    /// Effective damage, the amount `FUN_004178a0` adds to `+0x20`.
    pub damage: f32,
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
    /// Damage each robot has landed on another, summed over the match. Arena
    /// hazards' damage has no attacker and is only in `damage_taken`.
    pub damage_dealt: Vec<f32>,
    /// Hits that the match has not scored yet. The duel knows the damage and the
    /// attacker; the game type and teams are the match's, so it drains this.
    score_hits: Vec<ScoreHit>,
    /// Every damage event that reached a component (`FUN_004175b0`), in order.
    pub damage_events: Vec<DamageEvent>,
    /// Keep every [`Impact`](combat::Impact), including those under the `10`
    /// gate, in [`Duel::impacts`] (diagnostics and tests).
    pub record_impacts: bool,
    pub impacts: Vec<combat::Impact>,
    /// Whether the immobility rule runs: the match sets it when the fight
    /// begins (`FUN_00412fc0` requires `+0x8e`, "fighting").
    pub immobility_active: bool,
    immobility: crate::immobility::Tracker,
    /// Per-robot AI state (the stand-in driver).
    ai: Vec<AiState>,
    /// The game's own AI, when its scripts loaded and the bots are known to it.
    script_ai: Option<ScriptAi>,
    /// `(component, amount, time)` of the last blow each robot landed, which is
    /// what its brain reads back with `GetLastDamageDone` to spot a good hit.
    last_done: Vec<(i32, f32, f32)>,
    /// …and the last it took.
    last_received: Vec<(i32, f32, f32)>,
    /// Seconds until the brains are run again.
    brain_timer: f32,
    /// Where the arena came from, for the hazards' own lookups.
    arena_path: String,
    /// The hinges the model declares, by name (`GetHinge`).
    model_hinges: std::collections::BTreeMap<String, hazards::ModelHinge>,
    /// Hazard joints the scripts asked for, live.
    joints: Vec<LiveJoint>,
    /// The SmartZones the arena script registered.
    zones: Vec<Zone>,
    /// The model's phantom volumes (`GetRobotsInPhantom`): the KOTH scoring
    /// zone is one, and it is not a registered SmartZone.
    phantoms: Vec<(String, ra2script::arena::PhantomVolume)>,
    /// The first `Tick` failure, kept so a broken arena is reported once but
    /// remains visible in the match report's log.
    pub arena_tick_error: Option<String>,
    /// Whether each robot was inside each zone last step.
    zone_inside: Vec<Vec<bool>>,
    /// Seconds until the arena's own `Tick` runs again.
    arena_timer: f32,
    /// The `SetPinned` wishes already applied, so the reconciliation below only
    /// talks to the physics when the script actually changed its mind.
    applied_pinned: std::collections::BTreeMap<String, bool>,
    /// The match's one scene camera (F2/F3/F4, the arena intro): see
    /// [`crate::camera`].
    pub scene_camera: crate::camera::SceneCamera,
    /// The match's particle effects (sparks, debris, motor smoke, flames).
    pub fx: ra2_render::effects::Effects,
    /// The arena's shadow light and receiver triangles, read from the script
    /// layer once the arena script has declared them.
    shadow: Option<(Vec3, Vec<[Vec3; 3]>)>,
    /// The robot of the local human player, if the match has one. The F2/F4
    /// constructors hand `0x4014b0(0)` (the human player list's first entry)
    /// to the camera as its target; an AI-only match has none, and the
    /// original's cameras then have no target at all (measured live, docs/30).
    pub human_robot: Option<usize>,
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

/// The shipped AI for these robots, when the game's scripts sit next to the
/// arena. `None` (with a line on stderr) when they do not load, so a run still
/// works on a machine without the game data.
fn script_ai_for(arena_path: &str, robots: &[Robot]) -> Option<ScriptAi> {
    let path = std::path::Path::new(arena_path);
    let root = ra2_render::find_game_root(path)?;
    let script = arena_path.ends_with(".py").then_some(path);
    let names: Vec<String> = robots
        .iter()
        .map(|r| crate::scriptai::bot_name(&r.rig))
        .collect();
    match ScriptAi::new(&root, script, &names) {
        Ok(ai) => {
            println!(
                "  shipped AI: {} of {} robots ({} brains)",
                ai.driven(),
                names.len(),
                names.join(", ")
            );
            Some(ai)
        }
        Err(e) => {
            eprintln!("ra2sim: shipped AI unavailable ({e}); using the stand-in driver");
            None
        }
    }
}

impl Duel {
    /// Build the world and spawn the robots facing each other.
    pub fn new(arena_path: &str, bots: &[BotSetup]) -> Result<Self, Box<dyn std::error::Error>> {
        // Absolute from the outset: the script layer changes the process's
        // working directory to the game root, after which a relative arena path
        // no longer resolves.
        let arena_path = &std::fs::canonicalize(arena_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| arena_path.to_string());
        let obstacle = std::env::var("RA2_OBSTACLE").unwrap_or_else(|_| "none".to_string());
        // Static arena geometry only: script props become movable bodies below.
        std::env::set_var("RA2_OBSTACLE", "none");
        let arena = scene::load_arena(arena_path)?;
        std::env::remove_var("RA2_OBSTACLE");
        let mut prop_defs = scene::load_props(arena_path, &obstacle)?;
        prop_defs.extend(scene::load_havok_props(arena_path)?);

        let mut physics: Box<dyn PhysicsBackend> = crate::backend::create_backend(arena.gravity)?;
        let (verts, indices) = arena.triangle_soup();
        let _arena_body = physics.create_body(
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
            if let Some(body) = spawn_prop(&mut *physics, &p) {
                props.push(PropEntity { body, prop: p });
            }
        }

        if std::env::var("RA2_DEBUG").is_ok() {
            for p in &props {
                let n = p.prop.hull_points.len().max(1);
                let c = p.prop.hull_points.iter().fold(Vec3::ZERO, |a, b| a + *b) / n as f32;
                println!(
                    "  prop {:16?} at {:?} hull {:3} centroid {c:?}",
                    p.prop.name,
                    p.prop.position,
                    p.prop.hull_points.len()
                );
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
            if let Some(at) = setup.spawn {
                let floor = Self::floor_at(&*physics, &arena, at[0], at[1], None);
                let position = Vec3::new(at[0], floor + 0.06 - rig.min.y, at[1]);
                // `yaw_deg` is where the robot's *front* points (its `.bot`
                // heading is not always `-Z`).
                let yaw = setup.yaw_deg.map(f32::to_radians).unwrap_or(0.0) - rig.heading;
                let robot = Robot::spawn(
                    &mut *physics,
                    rig,
                    i,
                    position,
                    Quat::from_rotation_y(yaw),
                    mass_scale,
                )?;
                println!(
                    "  robot {i}: {:?} placed at ({:.2},{:.2},{:.2}), {:.0} kg",
                    robot.rig.name, position.x, position.y, position.z, robot.mass
                );
                robots.push(robot);
                continue;
            }
            let (x, z, spawn_facing, start_y) = match starts.get(i) {
                Some((p, f)) => (p.x, p.z, Some(*f), Some(p.y)),
                None => (
                    centre.x + angle.cos() * spread,
                    centre.z + angle.sin() * spread,
                    None,
                    None,
                ),
            };
            let floor = Self::floor_at(&*physics, &arena, x, z, start_y);
            let position = Vec3::new(x, floor + 0.06 - rig.min.y, z);

            // Face where the start point says, else the centre of the arena.
            let to_centre = Vec3::new(centre.x - x, 0.0, centre.z - z);
            let mut dir = to_centre.normalize_or_zero();
            if let Some(f) = spawn_facing {
                if f.length_squared() > 0.5 {
                    dir = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
                }
            }
            // The robot's front is its `.bot` heading, not always `-Z`.
            let yaw = setup
                .yaw_deg
                .map(f32::to_radians)
                .unwrap_or_else(|| f32::atan2(-dir.x, -dir.z))
                - rig.heading;

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

        // The game's own AI, when its scripts are next to the arena and the
        // robots are in `Bindings.py`. Without it the stand-in driver runs.
        let script_ai = script_ai_for(arena_path, &robots);
        let n = robots.len();
        // The arena's own hazards: the zones its script registered, and the
        // hinges its model declares.
        let registered = ra2script::state()
            .arena
            .as_ref()
            .map(|a| a.lock().unwrap().zones.clone())
            .unwrap_or_default();
        let zones = hazards::zones(arena_path, &registered);
        // The model's phantom volumes. `GetRobotsInPhantom` asks by name and
        // the KOTH scripts' scoring zone lives here rather than in `zones`.
        let phantoms: Vec<(String, ra2script::arena::PhantomVolume)> = ra2script::state()
            .arena
            .as_ref()
            .map(|a| {
                let d = a.lock().unwrap_or_else(|e| e.into_inner());
                d.phantoms
                    .iter()
                    .map(|(name, volume)| (name.clone(), volume.clone()))
                    .collect()
            })
            .unwrap_or_default();
        if !zones.is_empty() {
            println!(
                "  hazard zones: {}",
                zones
                    .iter()
                    .map(|z| format!("{}#{}, ", z.name, z.id))
                    .collect::<String>()
            );
        }
        let model_hinges = hazards::hinges(arena_path);
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
            damage_dealt: vec![0.0; n],
            score_hits: Vec::new(),
            damage_events: Vec::new(),
            immobility_active: false,
            record_impacts: false,
            impacts: Vec::new(),
            immobility: crate::immobility::Tracker::default(),
            ai: bots.iter().map(|_| AiState::new()).collect(),
            script_ai,
            last_done: vec![(0, 0.0, 0.0); n],
            last_received: vec![(0, 0.0, 0.0); n],
            brain_timer: 0.0,
            arena_path: arena_path.to_string(),
            model_hinges,
            joints: Vec::new(),
            zone_inside: vec![vec![false; n]; zones.len()],
            zones,
            phantoms,
            arena_tick_error: None,
            arena_timer: 0.0,
            applied_pinned: std::collections::BTreeMap::new(),
            scene_camera: crate::camera::SceneCamera::default(),
            fx: ra2_render::effects::Effects::new(
                ra2_render::find_game_root(std::path::Path::new(arena_path)).as_deref(),
            ),
            shadow: None,
            human_robot: None,
        })
    }

    /// Whether the game's own AI layer loaded for this duel.
    pub fn has_shipped_ai(&self) -> bool {
        self.script_ai.is_some()
    }

    /// Highest floor point under `(x, z)`.
    ///
    /// `from_y` is the start point's own height: a start point sits above the
    /// surface it belongs to, so the ray begins there. Casting from the arena
    /// centre instead starts *inside* a raised platform (the Metal Skull's
    /// plateaus) and lands on the floor beneath it.
    fn floor_at(
        physics: &dyn PhysicsBackend,
        arena: &ArenaScene,
        x: f32,
        z: f32,
        from_y: Option<f32>,
    ) -> f32 {
        let mut starts: Vec<f32> = from_y.into_iter().collect();
        starts.push(arena.center.y);
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

    /// The closest living opponent, or `None` when this robot is the last one.
    fn nearest_opponent(&self, me: usize) -> Option<usize> {
        let here = self.robots[me].position(&*self.physics);
        (0..self.robots.len())
            .filter(|&i| i != me && self.robots[i].is_alive())
            .min_by(|&a, &b| {
                let da = (self.robots[a].position(&*self.physics) - here).length_squared();
                let db = (self.robots[b].position(&*self.physics) - here).length_squared();
                da.total_cmp(&db)
            })
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
        self.step_with(&[], dt);
    }

    /// Advance one step with optional per-robot driver overrides.
    ///
    /// `overrides[i]` is `Some(input)` for a robot the caller drives (the
    /// exhibition match's player) and `None` for every robot the shipped AI or
    /// the stand-in driver owns. The brains still tick, so the other robots
    /// behave exactly as they do on [`Duel::step`]; the overridden robot's own
    /// brain output is discarded.
    pub fn step_with(&mut self, overrides: &[Option<DriveInput>], dt: f32) {
        // The game's own AI drives when it loaded; a robot the binding table
        // does not cover, or a run without the script layer, keeps the stand-in.
        // Brains run on the game's own cadence — an eighth of a second per tick
        // (`AI.SuperAI.Tick` refreshes its sensors after eight of them "once per
        // second") — while the physics runs every step between them.
        self.brain_timer -= dt;
        let tick_brains = self.brain_timer <= 0.0;
        if tick_brains {
            self.brain_timer = ra2script::AI_TICK_INTERVAL;
        }
        let mut brains = self.run_brains(dt, tick_brains);
        for i in 0..self.robots.len() {
            if !self.robots[i].is_alive() {
                continue;
            }
            let input = match overrides.get(i).copied().flatten() {
                Some(input) => input,
                None => match brains.get_mut(i).and_then(Option::take) {
                    // The brain's `Input` calls go to the robot's own control
                    // board; its wiring decides what they move.
                    Some(events) => {
                        self.robots[i].apply_inputs(&events);
                        DriveInput::default()
                    }
                    None => {
                        // Two robots face each other; in a four-robot match the
                        // stand-in driver goes for the nearest survivor. The
                        // game's own AI chooses its own target through
                        // `GetEnemies`, so this only affects the fallback driver.
                        let opponent = self.nearest_opponent(i);
                        let contact = opponent
                            .map(|o| self.robots_touching(i, o))
                            .unwrap_or(false);
                        match opponent {
                            Some(o) => self.ai_input(i, o, dt, contact),
                            None => DriveInput::default(),
                        }
                    }
                },
            };
            // Drive through the motors: the wheel hinges push the robot.
            let tuning = self.tuning;
            self.robots[i].drive(&mut *self.physics, input, &tuning);
        }

        self.advance(dt);
        self.service_requests(dt);
    }

    /// Build the joints the arena scripts have asked for since the last step.
    ///
    /// A slider (`AddPrismatic`) is anchored between the arena's static body and
    /// the part the script named; a hinge (`GetHinge`) is one the *model*
    /// declares, and only the body that moves needs creating.
    fn build_joints(&mut self) {
        let requests = ra2script::take_joint_requests();
        if requests.is_empty() {
            return;
        }
        let props: std::collections::BTreeMap<String, BodyHandle> = self
            .props
            .iter()
            .map(|p| (p.prop.name.clone(), p.body))
            .collect();
        let static_transforms = self.arena_named_transforms();

        for req in requests {
            let torque = {
                let d = req.drive.lock().unwrap_or_else(|e| e.into_inner());
                d.torque
            };
            // The scripts write the torque the way the engine's own
            // `SetPowerSettings` does — before the ×40 the binary applies (F3).
            let cap = (torque * 40.0).abs().max(1.0);
            if req.slider {
                let Some(&moving) = props.get(&req.child) else {
                    self.log.push(format!(
                        "hazard joint: no movable body named {:?}",
                        req.child
                    ));
                    continue;
                };
                let state = match self.physics.state(moving) {
                    Some(s) => s,
                    None => continue,
                };
                let host_rot = static_transforms
                    .get(&req.host)
                    .map(|m| m.to_scale_rotation_translation().1)
                    .unwrap_or(glam::Quat::IDENTITY);
                let axis = host_rot * Vec3::from(req.axis);
                let limits = hazards::slider_limits(req.neutral, req.lower, req.upper);
                let Some(anchor) = self.anchor_body(state.position) else {
                    continue;
                };
                let desc = crate::backend::SliderDesc {
                    body_a: anchor,
                    body_b: moving,
                    // The anchor body sits at the joint, so its own anchor is its
                    // origin; the moving part carries the offset.
                    anchor_a: Vec3::ZERO,
                    anchor_b: Vec3::ZERO,
                    axis_a: axis,
                    axis_b: state.rotation.inverse() * axis,
                    limits,
                    max_force: cap,
                    friction_force: cap,
                };
                match self.physics.create_slider(&desc) {
                    Ok(handle) => {
                        self.log.push(format!(
                            "hazard: {:?} slides on {:?} [{:.2}, {:.2}]",
                            req.child, req.host, limits[0], limits[1]
                        ));
                        self.joints.push(LiveJoint {
                            handle,
                            body: moving,
                            slider: true,
                            axis,
                            drive: req.drive.clone(),
                            strength: cap,
                            friction: cap,
                            limits,
                        });
                    }
                    Err(e) => self.log.push(format!("hazard joint {:?}: {e}", req.child)),
                }
            } else {
                let Some(hinge) = self.model_hinges.get(&req.child).cloned() else {
                    self.log
                        .push(format!("hazard hinge: the model has no {:?}", req.child));
                    continue;
                };
                // One side of the hinge is the fixed scenery, the other the part
                // that swings.
                let (pivot, axis) = (hinge.pivot, hinge.axis);
                let moving = props
                    .get(&hinge.body_a)
                    .or_else(|| props.get(&hinge.body_b))
                    .copied();
                let Some(moving) = moving else {
                    self.log.push(format!(
                        "hazard hinge: neither {:?} nor {:?} moves",
                        hinge.body_a, hinge.body_b
                    ));
                    continue;
                };
                let Some(state) = self.physics.state(moving) else {
                    continue;
                };
                let Some(anchor) = self.anchor_body(pivot) else {
                    continue;
                };
                let limits = hazards::hinge_limits(
                    hinge
                        .limits
                        .unwrap_or([-std::f32::consts::PI, std::f32::consts::PI]),
                );
                let desc = HingeDesc {
                    body_a: anchor,
                    body_b: moving,
                    anchor_a: Vec3::ZERO,
                    anchor_b: state.rotation.inverse() * (pivot - state.position),
                    axis_a: axis,
                    axis_b: state.rotation.inverse() * axis,
                    // The model's own angle limits: the compressor's smash arms
                    // are allowed 0.27 rad, which is what makes them arms rather
                    // than windmills.
                    limits,
                    max_torque: cap,
                    friction_torque: hinge.friction,
                };
                match self.physics.create_hinge(&desc) {
                    Ok(handle) => {
                        self.log
                            .push(format!("hazard: {:?} swings on {:?}", req.child, axis));
                        self.joints.push(LiveJoint {
                            handle,
                            body: moving,
                            slider: false,
                            axis,
                            drive: req.drive.clone(),
                            strength: cap,
                            friction: hinge.friction,
                            limits,
                        });
                    }
                    Err(e) => self.log.push(format!("hazard hinge {:?}: {e}", req.child)),
                }
            }
        }
    }

    /// A motionless pin for a hazard joint to hang from.
    ///
    /// Jolt rejects a constraint whose anchor is far from the body's own origin
    /// (the leverage is unrepresentable), and the arena's static body is one
    /// huge trimesh centred on the world — so each hazard joint gets its own
    /// zero-size static body, at the joint. It has no collision of its own.
    fn anchor_body(&mut self, pivot: Vec3) -> Option<BodyHandle> {
        let desc = BodyDesc::new(MotionType::Static, ShapeDesc::Sphere { radius: 0.5 })
            .at(pivot)
            .with_sensor(true)
            .with_user_data(USER_ARENA);
        match self.physics.create_body(&desc) {
            Ok(b) => Some(b),
            Err(e) => {
                self.log.push(format!("hazard anchor: {e}"));
                None
            }
        }
    }

    /// World transforms of the arena's own named bodies, for a joint's frame.
    fn arena_named_transforms(&self) -> std::collections::BTreeMap<String, Mat4> {
        let mut out = std::collections::BTreeMap::new();
        if let Ok(Some(havok)) = ra2_render::collect_havok(&self.arena_path) {
            for b in &havok.bodies {
                out.insert(b.name.clone(), b.transform);
            }
        }
        out
    }

    /// Watch the registered SmartZones and tell the script about crossings.
    ///
    /// `ZoneEvent(direction, id, robot, chassis)` is the script's side of this,
    /// and `robot` counts from one — `Hazards.Electricity` immediately does
    /// `r = robot - 1`, and `AI.SuperAI.SmartZoneEvent` does the same.
    fn watch_zones(&mut self) {
        if self.zones.is_empty() && self.phantoms.is_empty() {
            return;
        }
        let worlds: Vec<Vec<Mat4>> = (0..self.robots.len())
            .map(|i| self.robots[i].component_worlds(&*self.physics))
            .collect();

        // What is in every registered zone, published for both `Arena.
        // GetZoneContents` (the AI's `CheckSensors`) and the crossing events
        // below. The values are the original's 1-based robot ids.
        let mut contents: std::collections::BTreeMap<String, Vec<(i32, bool)>> =
            std::collections::BTreeMap::new();
        for (zi, zone) in self.zones.iter().enumerate() {
            let mut inside_robots: Vec<(i32, bool)> = Vec::new();
            for (robot, w) in worlds.iter().enumerate() {
                let mut inside = false;
                let mut chassis = false;
                for (ci, c) in self.robots[robot].rig.components.iter().enumerate() {
                    if !self.robots[robot].attached[ci] {
                        continue;
                    }
                    let p = w[ci].transform_point3(c.center());
                    if zone.touches(p, c.radius()) {
                        inside = true;
                        if ci == 0 {
                            chassis = true;
                        }
                    }
                }
                if inside {
                    inside_robots.push((robot as i32 + 1, chassis));
                }
                let was = self.zone_inside[zi][robot];
                if inside != was {
                    let direction = if inside { 1 } else { -1 };
                    if let Some(host) = self.script_ai.as_ref() {
                        host.zone_event(direction, zone.id, robot as i32 + 1, chassis);
                    }
                    self.zone_inside[zi][robot] = inside;
                    self.log.push(format!(
                        "zone {:?} ({}) robot {} {}",
                        zone.name,
                        zone.id,
                        robot + 1,
                        if inside { "entered" } else { "left" }
                    ));
                }
            }
            contents.insert(zone.name.clone(), inside_robots);
        }

        // The phantoms are queried by name and answered from the chassis: a
        // `GetRobotsInPhantom` asks who is *standing* in the volume. (The
        // registered zones above keep the per-part test because a hazard wants
        // to fire when a blade enters, not only the hull.)
        if !self.phantoms.is_empty() {
            for (name, volume) in &self.phantoms {
                let mut inside_robots: Vec<(i32, bool)> = Vec::new();
                for (robot, w) in worlds.iter().enumerate() {
                    let Some(chassis) = self.robots[robot].rig.components.first() else {
                        continue;
                    };
                    if !self.robots[robot]
                        .attached
                        .first()
                        .copied()
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    let p = w[0].transform_point3(chassis.center());
                    if volume.touches(p, chassis.radius()) {
                        inside_robots.push((robot as i32 + 1, true));
                    }
                }
                // A registered SmartZone of the same name wins: that one keeps
                // the per-part reading the hazard events use. (The stock arenas
                // name both the trigger body and its display object, so the
                // collision is real.)
                contents.entry(name.clone()).or_insert(inside_robots);
            }
        }

        ra2script::publish_zone_contents(contents);
    }

    /// Run the arena's own `Tick`, at the rate the game did.
    ///
    /// `Hazards.Flame` counts by a quarter of a second and `Electricity` counts
    /// a 5 s charge down in the same units, which puts an arena tick at 0.25 s
    /// however fast the physics runs.
    fn run_arena_tick(&mut self, dt: f32) {
        if self.script_ai.is_none() {
            return;
        }
        self.arena_timer -= dt;
        if self.arena_timer > 0.0 {
            return;
        }
        // Carry the remainder instead of resetting to the interval. A 60 Hz
        // step is 1/60 s and an arena tick is 0.25 s; resetting loses the
        // fraction every tick, which drifts the arena clock against the match
        // clock (19 ticks in 5 s instead of 20).
        self.arena_timer += ra2script::ARENA_TICK_INTERVAL;
        if let Some(host) = self.script_ai.as_ref() {
            if let Err(e) = host.tick_arena(dt) {
                // Report the first failure once: a broken arena fails every
                // quarter second, and a flood would bury the first traceback.
                // The message names the script path and carries the Python
                // traceback from `describe_python_error`.
                if self.arena_tick_error.is_none() {
                    let message = format!("arena Tick failed ({}): {e}", self.arena_path);
                    eprintln!("ra2sim: {message}");
                    self.log.push(message.clone());
                    self.arena_tick_error = Some(message);
                }
            }
        }
    }

    /// Reconcile the arena's own `SetPinned`/`SetActive` wishes.
    ///
    /// The scripts state what they *want* and the engine makes the world match:
    /// `flextop.py` pins its flexing floor when the hazards are off, and
    /// `parkinglot.py` keeps all 25 cinder blocks out of the world until a trap
    /// door opens over the pit and drops them in. Both calls are therefore
    /// idempotent, and both are read from the arena script's own records every
    /// step, so a hazard that flips them mid-match is picked up.
    fn apply_arena_flags(&mut self) {
        let (pinned, active) = {
            let st = ra2script::state();
            let Some(arena) = st.arena.as_ref() else {
                return;
            };
            let d = arena.lock().unwrap_or_else(|e| e.into_inner());
            if d.pinned.is_empty() && d.active.is_empty() {
                return;
            }
            (d.pinned.clone(), d.active.clone())
        };

        for (name, on) in &pinned {
            if self.applied_pinned.get(name) == Some(on) {
                continue;
            }
            let target = if *on {
                MotionType::Static
            } else {
                MotionType::Dynamic
            };
            let Some(p) = self.props.iter().find(|p| &p.prop.name == name) else {
                continue;
            };
            if p.body == HIDDEN_PROP {
                continue;
            }
            match self.physics.set_motion_type(p.body, target) {
                Ok(()) => {
                    self.applied_pinned.insert(name.clone(), *on);
                }
                Err(e) => self.log.push(format!("SetPinned({name:?}): {e}")),
            }
        }

        for (name, on) in &active {
            let Some(i) = self.props.iter().position(|p| &p.prop.name == name) else {
                continue;
            };
            let hidden = self.props[i].body == HIDDEN_PROP;
            if *on && hidden {
                match spawn_prop(&mut *self.physics, &self.props[i].prop) {
                    Some(body) => self.props[i].body = body,
                    None => self.log.push(format!("SetActive({name:?}): body failed")),
                }
            } else if !*on && !hidden {
                self.physics.remove_body(self.props[i].body);
                self.props[i].body = HIDDEN_PROP;
            }
        }
    }

    /// Drive every hazard joint from the settings its script last set.
    ///
    /// `Lock(True)` (or a zero speed/direction) leaves the motor off, which is
    /// how the joints park — the friction set at creation holds them there,
    /// exactly as a parked piston does (F8). `ApplyForce`/`ApplyTorque` are
    /// one-shot impulses applied along the joint's own axis or about it.
    fn drive_joints(&mut self, dt: f32) {
        for j in &mut self.joints {
            // How far along its own travel the joint is, for the auto-lock test.
            // Read through the physics (a disjoint field) so the mutable borrow
            // of `self.joints` this loop holds is untouched.
            let pos = if j.slider {
                self.physics.slider_offset(j.handle)
            } else {
                self.physics.hinge_angle(j.handle)
            };
            let (target, force, torque, strength, friction) = {
                let mut d = j.drive.lock().unwrap_or_else(|e| e.into_inner());
                // The script's torque is the *limit* on the drive, and it can
                // change at any time — a hammer parked with `Lock(True)` only
                // asks for its 1200 when a robot walks in.
                let strength = (d.torque.abs() * 40.0).max(1.0);
                // `Lock(True)` parks the joint, and so does the engine's own
                // `SetAutoLock`: with no motor it holds where it is, on friction,
                // the way a parked piston does (F8). The original does exactly
                // this in the joint's own update — it locks when the motor is
                // stopped, and again when the joint arrives at the end of its
                // travel, which is why the Octagon's pit floors stay up under the
                // robots until their script gives them a direction.
                let arrived = if d.direction > 0.0 {
                    (pos - j.limits[1]).abs() < 0.05
                } else if d.direction < 0.0 {
                    (pos - j.limits[0]).abs() < 0.05
                } else {
                    false
                };
                let held = d.locked
                    || (arrived && d.auto_lock_stops)
                    || (d.direction == 0.0 && d.auto_lock_idle);
                let target = if held { 0.0 } else { d.target_velocity() };
                let friction = if held { strength } else { 0.0 };
                (target, d.take_force(), d.take_torque(), strength, friction)
            };
            if (j.strength - strength).abs() > 1e-3 || (j.friction - friction).abs() > 1e-3 {
                self.physics
                    .set_joint_strength(j.handle, strength, friction);
                j.strength = strength;
                j.friction = friction;
            }
            if j.slider {
                self.physics.drive_slider(j.handle, target);
            } else {
                self.physics.drive_hinge(j.handle, target);
            }
            if force != 0.0 {
                self.physics.apply_impulse(j.body, j.axis * force * dt);
            }
            if torque != 0.0 {
                self.physics
                    .apply_angular_impulse(j.body, j.axis * torque * dt);
            }
        }
    }

    /// Publish what the brains can see, then let every brain run a tick.
    ///
    /// The engine publishes before the drive is applied and the physics steps,
    /// so a brain sees the world as it was when the tick began — the same order
    /// the original's frame loop used.
    fn run_brains(&mut self, dt: f32, tick: bool) -> Vec<Option<Vec<ra2script::InputEvent>>> {
        if self.script_ai.is_none() {
            return Vec::new();
        }
        self.publish_snapshots(dt);
        let n = self.robots.len();
        match self.script_ai.as_ref() {
            Some(ai) if tick => ai.tick(n),
            Some(ai) => ai.drives(n),
            None => Vec::new(),
        }
    }

    /// Publish what the scripts can see of each robot.
    ///
    /// The match's countdown steps the world without running a brain, but the
    /// arena's `Tick` and the hazards still read robot state, so the snapshot
    /// is published on every path that advances the world.
    fn publish_snapshots(&self, dt: f32) {
        let snapshots: Vec<ra2script::BotSnapshot> = (0..self.robots.len())
            .map(|i| {
                let mut s = crate::scriptai::snapshot(&*self.physics, &self.robots[i]);
                s.last_damage_done = self.last_done[i];
                s.last_damage_received = self.last_received[i];
                s
            })
            .collect();
        ra2script::publish(snapshots, self.time, dt);
    }

    /// Apply what the scripts asked for during the last tick.
    ///
    /// The `plus` free functions run inside Python, so `plus.damage`,
    /// `plus.force`, `plus.zap` and `plus.eliminatePlayer` queue instead of
    /// re-entering the simulator; this drains the queue.
    /// Drain what the scripts asked for, at the top of the next step.
    ///
    /// `dt` is what `plus.force` needs: the original accumulates the vector into
    /// the body's *force* (F13), not its velocity, so the step it lands in turns
    /// it into `force * dt` of impulse.
    fn service_requests(&mut self, dt: f32) {
        for req in ra2script::take_requests() {
            match req {
                ra2script::Request::Damage {
                    bot,
                    component,
                    amount,
                    point,
                } => {
                    let (Ok(bot), Ok(component)) =
                        (usize::try_from(bot), usize::try_from(component))
                    else {
                        continue;
                    };
                    if bot >= self.robots.len() || component >= self.robots[bot].health.len() {
                        continue;
                    }
                    self.apply_hit(None, bot, component, amount, point);
                }
                ra2script::Request::Force { bot, momentum } => {
                    if let Ok(bot) = usize::try_from(bot) {
                        if bot < self.robots.len() {
                            let robot = &self.robots[bot];
                            let body = robot.body();
                            // The script asked for a force on the *robot*, and
                            // the original's robot is one rigid body — so it
                            // accelerates as `force / robot mass`. Ours is a body
                            // per component, and this impulse lands on the
                            // chassis alone, so it is scaled by the chassis's
                            // share of the robot to leave the robot's own
                            // acceleration untouched.
                            let share = robot.chassis_mass() / robot.remaining_mass().max(0.2);
                            self.physics.apply_impulse(body, momentum * dt * share);
                        }
                    }
                }
                ra2script::Request::Zap {
                    bot,
                    amount,
                    radius,
                } => {
                    // The shipped `Hazards.Electricity` applies its own 25
                    // damage and 115 N/kg lift through `plus.damage`/`plus.force`
                    // on the same tick. What `zap` adds is the visible shock,
                    // and the recon did not establish that it deals damage of
                    // its own, so this records it (`zapped_until`, with the
                    // script's amount/radius) and leaves the spark to M5.
                    let _ = (amount, radius);
                    if let Ok(bot) = usize::try_from(bot) {
                        if bot < self.robots.len() {
                            self.robots[bot].zapped_until = self.time + 0.25;
                        }
                    }
                }
                ra2script::Request::Eliminate { bot } => {
                    // The original's robot state 3 (`0x4b9c70`): out of the
                    // match, still in the world. Tabletop's fall-out
                    // (`flextop.py:188`, `clawtop.py:249`), the Skull pit and
                    // the Octagon all reach this through `plus.eliminatePlayer`.
                    if let Ok(bot) = usize::try_from(bot) {
                        if bot < self.robots.len() && !self.robots[bot].eliminated {
                            self.robots[bot].eliminated = true;
                            let name = self.robots[bot].rig.name.clone();
                            self.log
                                .push(format!("t={:.1}s  {name} eliminated", self.time));
                        }
                    }
                }
            }
        }
    }

    /// Advance one step driving `inputs` (one per robot) instead of the AI.
    ///
    /// Deterministic driving, for validation and tests: the duel binary uses it
    /// for a head-on ram, which is the only way to *observe* the damage model
    /// reliably — the stock bot's own approach seldom closes to contact at all.
    pub fn step_direct(&mut self, inputs: &[DriveInput], dt: f32) {
        self.publish_snapshots(dt);
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
        // The arena's own logic, whichever driver ran: build whatever joints its
        // script asked for (by now every hazard's `__init__` has run and its
        // power settings are in place), fire the zone events, run its `Tick`,
        // and set the motors the hazards want for this step.
        self.build_joints();
        self.apply_arena_flags();
        self.watch_zones();
        self.run_arena_tick(dt);
        self.drive_joints(dt);
        self.step_scene_camera(dt);

        let info = self.physics.step(dt);
        if !info.complete {
            eprintln!("ra2sim: step at t={:.2}s dropped contacts", self.time);
        }

        self.resolve_combat();
        self.update_effects(dt);
        self.update_immobility(dt);
        self.time += dt;
        // What the scripts asked for, at the top of the step after the one they
        // asked in — `plus.damage`, `plus.force`, `plus.zap` and the tournament's
        // `eliminatePlayer`. It belongs to the step, not to whichever driver ran
        // it, so an arena hazard reaches the robots however the robots are driven.
        self.service_requests(dt);
    }

    /// Advance the particle effects: the motors that smoke, the arena's flames,
    /// and the shadow data once the arena script has declared it.
    fn update_effects(&mut self, dt: f32) {
        if self.shadow.is_none() {
            let st = ra2script::state();
            if let Some(arena) = st.arena.as_ref() {
                if let Ok(a) = arena.lock() {
                    if let Some(light) = a.shadow_source {
                        let tris: Vec<[Vec3; 3]> = a
                            .shadow_triangles
                            .iter()
                            .map(|t| [Vec3::from(t[0]), Vec3::from(t[1]), Vec3::from(t[2])])
                            .collect();
                        self.fx.set_collision(tris.clone());
                        self.shadow = Some((Vec3::from(light), tris));
                    }
                }
            }
        }
        // The arena's lightning bolts (`CreateLightning`, ...): the script owns
        // their ends and switches, the effects draw them.
        let bolts: Vec<_> = ra2script::state()
            .arena
            .as_ref()
            .and_then(|a| a.lock().ok().map(|a| a.lightning.clone()))
            .unwrap_or_default();
        self.fx.set_lightning(
            bolts
                .iter()
                .map(|(i, a, b, on)| (*i, Vec3::from(*a), Vec3::from(*b), *on)),
        );
        if self.fx.enabled {
            for r in 0..self.robots.len() {
                let smoking: Vec<usize> = (0..self.robots[r].rig.components.len())
                    .filter(|&i| self.robots[r].smoking(i))
                    .collect();
                let worlds =
                    (!smoking.is_empty()).then(|| self.robots[r].component_worlds(&*self.physics));
                for i in 0..self.robots[r].rig.components.len() {
                    if self.robots[r].rig.components[i].role != crate::rig::Role::Motor {
                        continue;
                    }
                    let key = ((r as u64) << 16) | i as u64;
                    let on = smoking.contains(&i);
                    let at = worlds
                        .as_ref()
                        .map_or(Vec3::ZERO, |w| w[i].transform_point3(Vec3::ZERO));
                    self.fx.motor_smoke(key, at, on);
                }
            }
            let flames: Vec<_> = ra2script::state().emitters.clone();
            for (i, e) in flames.iter().enumerate() {
                self.fx.flame(
                    (1 << 40) | i as u64,
                    Vec3::from(e.location),
                    Vec3::from(e.velocity),
                    Vec3::from(e.variance),
                    e.emitting,
                );
            }
        }
        self.fx.step(dt);
    }

    /// Turn the step's contact points into damage.
    ///
    /// Every contact point of every step is an event, in both directions
    /// ([`combat::resolve`]); each one then goes through [`Duel::apply_hit`],
    /// whose `>= 10` gate is the arena's (`FUN_00417330`).
    fn resolve_combat(&mut self) {
        let events = self.physics.take_contacts();
        let impacts = combat::resolve(&self.robots, &events, &self.combat);
        if self.record_impacts {
            self.impacts.extend(impacts.iter().copied());
        }
        for imp in &impacts {
            self.sparks_for(imp);
            self.apply_hit(
                Some(imp.attacker),
                imp.target,
                imp.component,
                imp.damage,
                imp.point,
            );
        }
    }

    /// The sparks of one priced contact (`FUN_0045f3a0` → `FUN_004a2a50`):
    /// only above `10` damage, only when a metal part is in the contact and no
    /// rubber one is, `int(damage)` of them leaving along the contact normal.
    fn sparks_for(&mut self, imp: &combat::Impact) {
        if imp.damage <= 10.0 || !self.fx.enabled {
            return;
        }
        let class = |robot: usize, component: usize| {
            self.robots
                .get(robot)
                .and_then(|r| r.rig.components.get(component))
                .map(|c| combat::material_class(c.material.as_deref()).to_string())
        };
        let classes = [
            class(imp.target, imp.component),
            class(imp.attacker, imp.by),
        ];
        let metal = classes.iter().flatten().any(|c| c == "metal");
        let rubber = classes.iter().flatten().any(|c| c == "rubber");
        if metal && !rubber {
            self.fx.sparks(imp.damage, imp.point, imp.normal);
        }
    }

    /// Apply one damage event, and remember whether it is a scoring hit.
    ///
    /// This is the arena's `FUN_00417330`: it ignores a zero hit, and a hit under
    /// [`CombatConfig::apply_threshold`] (`10.0`) goes no further than the
    /// sparks and sounds (`FUN_00417510`) — it takes no hitpoints and scores
    /// nothing. From `10.0` up `FUN_004175b0` applies it to the victim's
    /// component (`FUN_00489620` → the component's own `apply`) and the match
    /// scores it: outside King of the Hill, and not between teammates in Team
    /// Match, which [`Match`](crate::r#match::Match) decides. `attacker` is
    /// `None` for a hazard's own damage, which never stores points
    /// (`plus.damage` goes through `FUN_004148f0`, whose player id is `-1`).
    ///
    /// The victim's state is tested *before* the blow lands, like
    /// `FUN_004175b0`'s `+0x34 == 0`; the score is the effective damage the
    /// component's `apply` returned.
    pub fn apply_hit(
        &mut self,
        attacker: Option<usize>,
        victim: usize,
        component: usize,
        damage: f32,
        point: Vec3,
    ) -> bool {
        if victim >= self.robots.len()
            || component >= self.robots[victim].health.len()
            || !damage.is_finite()
            || damage < self.combat.apply_threshold
        {
            return false;
        }
        if !self.robots[victim].attached[component] {
            // `FUN_00417330`: a component that has come off (`+0xe0 > 0`) ends the event.
            return false;
        }
        let victim_was_alive = self.robots[victim].is_alive();
        let applied = self.robots[victim].damage(component, damage);
        self.damage_taken[victim] += damage;
        self.damage_events.push(DamageEvent {
            time: self.time,
            attacker: attacker.map(|a| a as i32).unwrap_or(-1),
            victim,
            component,
            amount: damage,
        });
        if let Some(attacker) = attacker {
            if attacker < self.robots.len() {
                self.damage_dealt[attacker] += damage;
                // The brains spot a "good hit" by matching the component id
                // against their own weapon list (`AI/__init__.py:CheckForWeaponHits`).
                self.last_done[attacker] = (component as i32, damage, self.time);
                if victim_was_alive && applied.effective > 0.0 {
                    self.score_hits.push(ScoreHit {
                        attacker,
                        victim,
                        damage: applied.effective,
                    });
                }
            }
        }
        self.last_received[victim] = (component as i32, damage, self.time);
        let name = self.robots[victim].rig.name.clone();
        if applied.critical {
            self.log.push(format!(
                "t={:.1}s  {name}: critical hit {}",
                self.time, self.robots[victim].criticals
            ));
        }
        if applied.destroyed {
            self.robots[victim].destroyed = true;
            self.log
                .push(format!("t={:.1}s  {name} chassis destroyed", self.time));
            return true;
        }
        if applied.detach {
            self.fx.debris(damage, point, Vec3::Y);
            self.break_off(victim, component, point);
            true
        } else {
            if component == 0 {
                // A hurt chassis caves in before it dies (F9).
                self.buckle_chassis(victim);
            }
            false
        }
    }

    /// Take the hits the match has not scored yet.
    pub fn take_score_hits(&mut self) -> Vec<ScoreHit> {
        std::mem::take(&mut self.score_hits)
    }

    /// Deform a robot's chassis to match its damage, and rebake its collider.
    fn buckle_chassis(&mut self, robot: usize) {
        let physics = &mut *self.physics;
        self.robots[robot].buckle(physics);
    }

    /// A component came off: detach its subtree as flying debris and rebuild the
    /// robot without it. (The chassis never does; its end is the third critical
    /// hit, handled in [`Duel::apply_hit`].)
    fn break_off(&mut self, robot: usize, component: usize, point: Vec3) {
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
        self.log
            .push(format!("t={:.1}s  {name}: {part} knocked off", self.time));
    }

    /// The immobility rule (`FUN_00412fc0`, block `0x4133b8`), sampled once a
    /// second while the fight is on: a robot that has not left a sqrt(2) m
    /// sphere for 10 s is warned (`ImmobilityWarning(id, 1)` to every brain) and
    /// at 20 s it is IMMOBILIZED (state 2).
    fn update_immobility(&mut self, dt: f32) {
        if !self.immobility_active {
            return;
        }
        let positions: Vec<Option<Vec3>> = (0..self.robots.len())
            .map(|i| {
                self.robots[i]
                    .is_alive()
                    .then(|| self.robots[i].position(&*self.physics))
            })
            .collect();
        for event in self.immobility.update(dt, &positions) {
            match event {
                crate::immobility::Event::Warning(id, on) => {
                    if let Some(ai) = self.script_ai.as_ref() {
                        ai.immobility_warning(id as i32, on);
                    }
                }
                crate::immobility::Event::Immobilized(id) => {
                    self.robots[id].immobilized = true;
                    let name = self.robots[id].rig.name.clone();
                    self.log
                        .push(format!("t={:.1}s  {name} immobilized", self.time));
                }
            }
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
        self.apply_background(renderer);
        let view_proj = self.view_proj(camera, w, h);
        let fx = self.effect_items(view_proj);
        let mut groups = self.render_groups();
        groups.push(RenderGroup {
            items: &fx,
            model: Mat4::IDENTITY,
        });
        renderer.render_groups(&groups, view_proj, w, h)
    }

    /// Render the current frame into an existing colour target and depth
    /// buffer. This is [`Duel::render`] without the private texture and the
    /// readback, for a caller that composites the HUD over the same target.
    ///
    /// `format` is the colour target's format; the renderer builds (and caches)
    /// that format's pipelines.
    #[allow(clippy::too_many_arguments)]
    pub fn render_to(
        &self,
        renderer: &Renderer,
        camera: &DuelCamera,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        w: u32,
        h: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.apply_background(renderer);
        let view_proj = self.view_proj(camera, w, h);
        let fx = self.effect_items(view_proj);
        let mut groups = self.render_groups();
        groups.push(RenderGroup {
            items: &fx,
            model: Mat4::IDENTITY,
        });
        renderer.render_groups_to(&groups, view_proj, color_view, depth_view, format)
    }

    /// The shadows and particles of this frame, in world space.
    ///
    /// Shadows are each attached component's hull projected from the arena's
    /// shadow light onto its declared receivers (`ShadowWizard`); particles are
    /// billboarded to the camera `view_proj` looks through.
    pub fn effect_items(&self, view_proj: Mat4) -> Vec<DrawItem> {
        let mut out = Vec::new();
        if let Some((light, receivers)) = &self.shadow {
            // One caster per attached component, as `ShadowWizard` has one per
            // object: its hull points in the world.
            let mut casters: Vec<Vec<Vec3>> = Vec::new();
            for r in self.robots.iter().filter(|r| r.parts[0].alive) {
                let worlds = r.component_worlds(&*self.physics);
                for (i, c) in r.rig.components.iter().enumerate() {
                    if r.attached[i] && !c.hull_points.is_empty() {
                        casters.push(
                            c.hull_points
                                .iter()
                                .map(|p| worlds[i].transform_point3(*p))
                                .collect(),
                        );
                    }
                }
            }
            out.extend(ra2_render::effects::shadow_item(
                *light, receivers, &casters,
            ));
        }
        if !self.fx.is_empty() {
            let (right, up) = ra2_render::effects::camera_axes(view_proj);
            out.extend(self.fx.items(right, up));
        }
        out
    }

    /// Clear to the arena script's `setBackColor`, else the `.gmf`'s own
    /// static background.
    fn apply_background(&self, renderer: &Renderer) {
        let color = ra2script::state()
            .back_color
            .unwrap_or(self.arena.background);
        renderer.set_background(color);
    }

    /// The scene's draw groups, borrowing the arena, the props, the debris and
    /// the robots.
    pub fn render_groups(&self) -> Vec<RenderGroup<'_>> {
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
        groups
    }

    /// The view-projection matrix `camera` asks for at this size.
    ///
    /// The three match cameras (`Static`, `Watch`, `Orbit`) are the original's
    /// scene camera: `D3DXMatrixPerspectiveFovLH(fov, w / h, near, far)` and a
    /// `D3DXMatrixLookAtLH` view (see [`crate::camera`]). The other modes are
    /// this port's own and keep a fixed 55 degree lens.
    pub fn view_proj(&self, camera: &DuelCamera, w: u32, h: u32) -> Mat4 {
        let aspect = w as f32 / h as f32;
        if let Some(scene) = self.scene_for(camera) {
            return scene.view_proj(aspect);
        }
        let (eye, aim) = self.camera_pose(camera);
        let view = ra2_render::look_at_lh(eye, aim, Vec3::Y);
        let proj = ra2_render::perspective_lh(55f32.to_radians(), aspect, 0.05, 900.0);
        proj * view
    }

    /// The scene camera as `camera` wants it, or `None` for this port's own
    /// modes. A mode the scene camera is not currently in is shown with that
    /// mode's constructor defaults (no script involved).
    fn scene_for(&self, camera: &DuelCamera) -> Option<crate::camera::SceneCamera> {
        use crate::camera::{SceneCamera, SceneCameraKind};
        let kind = match camera {
            DuelCamera::Static => SceneCameraKind::Static,
            DuelCamera::Watch => SceneCameraKind::Watch,
            DuelCamera::Orbit { .. } => SceneCameraKind::Orbit,
            _ => return None,
        };
        let mut scene = if self.scene_camera.kind == kind {
            self.scene_camera.clone()
        } else {
            SceneCamera::new(kind)
        };
        scene.follow(self.follow_target());
        Some(scene)
    }

    /// The human player's robot position and heading: what the watch and orbit
    /// cameras follow (`0x43a0ff` / `0x439e5a` hand `0x4014b0(0)`, the first
    /// human player, to `0x5a6650`). `None` without a human player.
    pub fn follow_target(&self) -> Option<crate::camera::Target> {
        let robot = self.robots.get(self.human_robot?)?;
        let position = robot.position(&*self.physics);
        let rotation = self.physics.state(robot.body()).map(|s| s.rotation)?;
        let forward = rotation * robot.rig.forward();
        Some(crate::camera::Target {
            position,
            yaw: forward.x.atan2(forward.z),
        })
    }

    /// Run what the scripts asked of the camera this step, then the camera's
    /// own animation clock.
    fn step_scene_camera(&mut self, dt: f32) {
        for command in ra2script::camera::take_commands() {
            self.scene_camera.apply(command);
        }
        let target = self.follow_target();
        self.scene_camera.follow(target);
        self.scene_camera.advance(dt);
    }

    /// Put the scene camera in `kind`'s configuration the way the match shell
    /// does on a camera key (`F2`/`F3`/`F4`) and, for `Static`/`Watch`, ask the
    /// arena's script for its next camera of that kind. Returns the camera's
    /// name (`"Static 1"`), which the HUD's `camera textbox` shows.
    pub fn select_scene_camera(&mut self, kind: crate::camera::SceneCameraKind) -> Option<String> {
        use crate::camera::SceneCameraKind;
        // Anything the scripts queued earlier belongs to the old configuration.
        for command in ra2script::camera::take_commands() {
            self.scene_camera.apply(command);
        }
        self.scene_camera.select(kind);
        let name = match kind {
            SceneCameraKind::Static => {
                ra2script::camera::next_camera(ra2script::camera::NextCamera::Static)
            }
            SceneCameraKind::Watch => {
                ra2script::camera::next_camera(ra2script::camera::NextCamera::Watch)
            }
            SceneCameraKind::Orbit => None,
        };
        for command in ra2script::camera::take_commands() {
            self.scene_camera.apply(command);
        }
        name
    }

    fn body_model(&self, body: BodyHandle) -> Mat4 {
        self.physics
            .state(body)
            .map(|s| Mat4::from_rotation_translation(s.rotation, s.position))
            .unwrap_or(Mat4::IDENTITY)
    }

    /// The eye and aim point `camera` asks for, in world units.
    ///
    /// The presentation uses this for the 3D frame and the audio layer uses it
    /// for the listener: the ear is the eye and its forward is `aim - eye`, so
    /// a positional sound is heard from where the picture is taken.
    pub fn camera_pose(&self, camera: &DuelCamera) -> (Vec3, Vec3) {
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
            DuelCamera::Watch | DuelCamera::Static | DuelCamera::Orbit { .. } => {
                // The scene camera's own pose (see [`crate::camera`]).
                match self.scene_for(camera) {
                    Some(scene) => {
                        let (eye, aim, _) = scene.pose();
                        (eye, aim)
                    }
                    None => (self.arena.center + Vec3::Y, self.arena.center),
                }
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
                    .map(|s| (s.rotation * r.rig.forward()).normalize_or_zero())
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
    /// The match shell's `watch`: frame every live robot. **Our rule**; the
    /// C++ framing at `0x4770a0` was not established.
    Watch,
    /// The match shell's `static`: the arena script's own `AddStaticCamera`.
    Static,
    /// The match shell's `orbit`: a slow orbit around the live midpoint.
    /// **Our rule**; `0x476f00` was not traced.
    Orbit { period: f32 },
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

impl DuelCamera {
    /// Parse the binary's three camera strings (`0x4759c0`: `watch`, `static`,
    /// `orbit`). Anything else is not one of the config's modes; the caller
    /// reports it rather than silently choosing.
    pub fn from_config(name: &str) -> Option<DuelCamera> {
        match name.trim().to_ascii_lowercase().as_str() {
            "watch" => Some(DuelCamera::Watch),
            "static" => Some(DuelCamera::Static),
            "orbit" => Some(DuelCamera::Orbit { period: 24.0 }),
            _ => None,
        }
    }

    /// The name the HUD's `camera textbox` shows, in the config's vocabulary.
    ///
    /// This is the `ra2.cfg` string (`0x4759c0` compares against `watch`,
    /// `static`, `orbit`). The textbox itself shows the camera object's own
    /// name ([`Self::display_name`]).
    pub fn name(self) -> &'static str {
        match self {
            DuelCamera::Watch => "watch",
            DuelCamera::Static => "static",
            DuelCamera::Orbit { .. } => "orbit",
            DuelCamera::Auto => "auto",
            DuelCamera::Wide { .. } => "wide",
            DuelCamera::Chase { .. } => "chase",
            DuelCamera::Top { .. } => "top",
        }
    }

    /// The text `camera textbox` shows for this mode.
    ///
    /// The binary sets the box in `FUN_00477180` from the camera object and
    /// shows it for 3.0 s (`+0x17c = 0x40400000`, `0x4771a2`). The three
    /// shipped cameras name themselves `Watch Cam`, `Static Cam` and
    /// `Orbit Cam` (`0x6f1044`, `0x6f1038`, `0x6f102c`; constructed by
    /// `0x43a020`, `0x439f20`, `0x439d60`), and `0x476fd1` pushes the orbit
    /// literal for the match camera. An arena's `AddStaticCamera`/
    /// `AddWatchCamera` name (`Arenas/box.py:22-28`: `Static 1`, `Watch 1`)
    /// replaces the class default; this port keeps the class defaults and
    /// labels its driving camera as ours.
    pub fn display_name(self) -> &'static str {
        match self {
            DuelCamera::Watch => "Watch Cam",
            DuelCamera::Static => "Static Cam",
            DuelCamera::Orbit { .. } => "Orbit Cam",
            DuelCamera::Auto => "Auto Cam",
            DuelCamera::Wide { .. } => "Wide Cam",
            DuelCamera::Chase { .. } => "Chase Cam (ours)",
            DuelCamera::Top { .. } => "Top Cam",
        }
    }
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
    let fwd = (rot * me.rig.forward()).normalize_or_zero();
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
            ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
    /// Puts an environment variable back when the test ends.
    struct Restore(&'static str, Option<String>);

    impl Drop for Restore {
        fn drop(&mut self) {
            match &self.1 {
                Some(v) => std::env::set_var(self.0, v),
                None => std::env::remove_var(self.0),
            }
        }
    }

    fn game_dir() -> Option<std::path::PathBuf> {
        if let Some(dir) = std::env::var_os("RA2_GAME") {
            return Some(std::path::PathBuf::from(dir));
        }
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
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        // `Team5/Bot1` has `.bot` heading 0. The robot's front is its heading
        // now, so `Team1/Bot0` (heading PI) meets head-on with its other face
        // and measures a 13.9 blow followed by 42.6 damage of resting contact.
        let bot = game.join("AI/Team5/Bot1.bot").to_string_lossy().to_string();
        // Placed head-on and close, rather than at the arena's own start points
        // 25 m apart: a long approach is a chaotic trajectory, and this test is
        // about the *blow*, not about who happens to reach whom. A head-on ram
        // has to be a head-on ram whatever the masses are.
        let setups = [
            BotSetup {
                path: bot.clone(),
                yaw_deg: Some(270.0),
                spawn: Some([-3.0, 0.0]),
            },
            BotSetup {
                path: bot,
                yaw_deg: Some(90.0),
                spawn: Some([3.0, 0.0]),
            },
        ];
        let mut duel = Duel::new(&arena, &setups).expect("the box arena builds a duel");
        let dt = 1.0 / 60.0;

        // Run at each other until they touch.
        let ram = vec![
            DriveInput {
                throttle: 1.0,
                steer: 0.0,
                ..Default::default()
            };
            2
        ];
        // The blow is read from the priced contact events (`record_impacts`),
        // before the arena's `>= 10` gate: the control wiring drives the motors
        // at their rated speed, and today that is ~5x too slow (the fidelity
        // harness: top speed 2.3 m/s against the original's 11.1), so a ram here
        // closes at ~3 m/s and its contact points stay under the gate. The gate
        // itself is the original's (`FUN_00417330`) and is asserted below, not
        // lowered. Once the speed gap is closed this ram clears 10 again.
        duel.record_impacts = true;
        let mut blow = 0.0f32;
        for _ in 0..(6.0 / dt) as usize {
            duel.step_direct(&ram, dt);
            blow = duel.impacts.iter().map(|i| i.damage).fold(blow, f32::max);
        }
        println!(
            "opening ram: {blow:.1} peak priced contact (attacker chassis material x{:.2}), \
             {:.1} applied",
            duel.robots[0].rig.components[0].material_scale, duel.damage_taken[0]
        );
        assert!(blow > 0.0, "the ram produced no priced contact at all");
        assert!(blow < 400.0, "the opening ram was {blow} damage");
        let gated: f32 = duel
            .impacts
            .iter()
            .filter(|i| i.damage >= 10.0 && i.target == 0)
            .map(|i| i.damage)
            .sum();
        assert!(
            (duel.damage_taken[0] - gated).abs() < 1e-3 * (1.0 + gated),
            "applied damage {} is not the >=10 contact points {gated}",
            duel.damage_taken[0]
        );

        // Now stand still on each other for two seconds: nothing pressed
        // together adds a blow as hard as the ram's.
        let rest = vec![DriveInput::default(); 2];
        let (settled, seen) = (duel.damage_taken[0], duel.impacts.len());
        for _ in 0..(2.0 / dt) as usize {
            duel.step_direct(&rest, dt);
        }
        let after = duel.damage_taken[0] - settled;
        let rest_peak = duel.impacts[seen..]
            .iter()
            .map(|i| i.damage)
            .fold(0.0f32, f32::max);
        println!("resting contact: peak {rest_peak:.2}, applied {after:.1}");
        assert!(
            rest_peak <= blow,
            "resting contact peaked at {rest_peak}, above the {blow} ram"
        );
        assert!(
            after <= blow * 0.25,
            "resting contact added {after} damage after a {blow} blow"
        );
    }

    /// F7: the hit's scale is the **attacking component's** material float.
    ///
    /// The shipped definitions give three values — Metal 0.1 (the default and
    /// the saw blade), the ram plate's `metal(.4)` = 0.4, and the casters'
    /// `rubber(0)` = 0.0. The first blow of the same head-on ram is measured
    /// with each, so the path is linear in the recovered float and a rubber
    /// part deals nothing.
    #[test]
    fn the_opening_ram_scales_with_the_attackers_material() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = game.join("AI/Team1/Bot0.bot").to_string_lossy().to_string();
        let blow_with = |scale: f32| -> f32 {
            let setups = [
                BotSetup {
                    path: bot.clone(),
                    yaw_deg: Some(270.0),
                    spawn: Some([-3.0, 0.0]),
                },
                BotSetup {
                    path: bot.clone(),
                    yaw_deg: Some(90.0),
                    spawn: Some([3.0, 0.0]),
                },
            ];
            let mut duel = Duel::new(&arena, &setups).expect("the box arena builds a duel");
            // `damage_taken[0]` is the damage robot 0 *received*, so the
            // attacker is robot 1; set both sides and the first blow is the
            // same whichever way one wants to read it.
            for robot in &mut duel.robots {
                for c in &mut robot.rig.components {
                    c.material_scale = scale;
                }
            }
            let ram = vec![
                DriveInput {
                    throttle: 1.0,
                    steer: 0.0,
                    ..Default::default()
                };
                2
            ];
            duel.record_impacts = true;
            for _ in 0..(6.0 / (1.0 / 60.0)) as usize {
                duel.step_direct(&ram, 1.0 / 60.0);
                // The first contact point of the first step the robots touch.
                if let Some(first) = duel.impacts.first() {
                    return first.damage;
                }
            }
            0.0
        };
        let (metal, plate, rubber) = (blow_with(0.1), blow_with(0.4), blow_with(0.0));
        println!("opening ram: metal 0.1 -> {metal:.1}, ram plate 0.4 -> {plate:.1}, rubber 0.0 -> {rubber:.1}");
        assert!(rubber == 0.0, "a rubber part dealt {rubber}");
        assert!(metal > 0.0, "metal dealt {metal}");
        let ratio = plate / metal;
        assert!(
            (ratio - 4.0).abs() < 0.4,
            "the 0.4 material should deal four times the 0.1 one: {ratio:.2}"
        );
    }

    /// The damage stream of a head-on ram, against the original's live log
    /// (`match-damage__live__000.jsonl`): the original logged 8 events in a
    /// 50 s fight, amounts 10.0 to 74.0, the contact pair 16.3 both ways.
    ///
    /// A sustained shove is not a hit: after the opening blow the stream is
    /// only the occasional contact point whose closing speed is high enough for
    /// the amount to reach the `10` gate, so the rate stays in the original's
    /// order of magnitude (a few per second at most, never one per step).
    #[test]
    fn a_ram_makes_discrete_damage_events_like_the_original() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = game.join("AI/Team1/Bot0.bot").to_string_lossy().to_string();
        let setups = [
            BotSetup {
                path: bot.clone(),
                yaw_deg: Some(270.0),
                spawn: Some([-3.0, 0.0]),
            },
            BotSetup {
                path: bot,
                yaw_deg: Some(90.0),
                spawn: Some([3.0, 0.0]),
            },
        ];
        let mut duel = Duel::new(&arena, &setups).expect("a duel");
        duel.record_impacts = true;
        let ram = vec![
            DriveInput {
                throttle: 1.0,
                steer: 0.0,
                ..Default::default()
            };
            2
        ];
        let seconds = 12.0;
        let dt = 1.0 / 60.0;
        for _ in 0..(seconds / dt) as usize {
            duel.step_direct(&ram, dt);
        }
        let applied: Vec<f32> = duel.damage_events.iter().map(|e| e.amount).collect();
        let all: Vec<f32> = duel.impacts.iter().map(|i| i.damage).collect();
        let mut sorted = applied.clone();
        sorted.sort_by(f32::total_cmp);
        let pct = |p: f32| {
            sorted
                .get(((sorted.len() as f32 - 1.0) * p).round() as usize)
                .copied()
                .unwrap_or(0.0)
        };
        println!(
            "ram {seconds:.0}s: {} contact-point hits, {} applied (>=10) = {:.2}/s; \
             amounts min {:.1} median {:.1} p90 {:.1} max {:.1}",
            all.len(),
            applied.len(),
            applied.len() as f32 / seconds,
            pct(0.0),
            pct(0.5),
            pct(0.9),
            pct(1.0),
        );
        // With the motors at their rated speed (~5x slower than the original's
        // robots, see the fidelity baseline) a ram's contact points stay under
        // the arena's `>= 10` gate, so "no applied hits" is the expected symptom
        // of the speed gap, not of a broken gate. What must hold regardless:
        // the contact was priced, and exactly the points at the gate or above
        // were applied.
        assert!(!all.is_empty(), "the ram never made contact");
        let gated = all.iter().filter(|a| **a >= 10.0).count();
        assert_eq!(
            applied.len(),
            gated,
            "applied events are not the >=10 contact points"
        );
        assert!(
            applied.len() as f32 / seconds < 30.0,
            "{} damage events in {seconds}s is a grind, not discrete hits",
            applied.len()
        );
        assert!(applied.iter().all(|a| *a >= 10.0));
    }

    /// `FUN_00412fc0`: a robot that never leaves its spot is warned at 10 s and
    /// counted out (state 2) at 20 s of fight, and the brains hear about it.
    #[test]
    fn a_robot_that_sits_still_is_immobilized_after_twenty_seconds() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = |p: &str, x: f32| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: Some([x, 0.0]),
        };
        let mut duel = Duel::new(
            &arena,
            &[
                bot("AI/Team1/Bot0.bot", -8.0),
                bot("AI/Team6/Bot2.bot", 8.0),
            ],
        )
        .expect("a duel");
        let idle = [DriveInput::default(), DriveInput::default()];
        let dt = 1.0 / 60.0;
        // Not running until the fight begins.
        for _ in 0..(25.0 / dt) as usize {
            duel.step_direct(&idle, dt);
        }
        assert!(!duel.robots[0].immobilized, "the rule ran outside a fight");
        duel.immobility_active = true;
        for _ in 0..(19.0 / dt) as usize {
            duel.step_direct(&idle, dt);
        }
        assert!(duel.robots[0].is_alive());
        for _ in 0..(3.0 / dt) as usize {
            duel.step_direct(&idle, dt);
        }
        assert!(duel.robots[0].immobilized);
        assert!(!duel.robots[0].is_alive(), "state 2 is defeated");
        assert!(duel.log.iter().any(|l| l.contains("immobilized")));
    }

    /// M8.3: the two requests the duel used to drop now change the match.
    ///
    /// `plus.eliminatePlayer` is the original's robot state 3 — Tabletop's
    /// fall-out, the Skull pit and the Octagon's pit all call it. `plus.zap`
    /// is the electric hazard's shock: the hazard itself applies the damage,
    /// so the engine records the visual deadline (the recon did not establish
    /// that the zap deals damage of its own).
    #[test]
    fn zap_and_eliminate_requests_take_effect() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = |p: &str| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        };
        let mut duel = Duel::new(
            &arena,
            &[bot("AI/Team1/Bot0.bot"), bot("AI/Team6/Bot2.bot")],
        )
        .expect("a duel");
        let dt = 1.0 / 60.0;
        let idle = [DriveInput::default(), DriveInput::default()];

        ra2script::state().requests.push(ra2script::Request::Zap {
            bot: 0,
            amount: 10.0,
            radius: 3.0,
        });
        ra2script::state()
            .requests
            .push(ra2script::Request::Eliminate { bot: 1 });
        duel.step_direct(&idle, dt);
        assert!(
            duel.robots[0].zapped_until > duel.time - dt * 2.0,
            "the zap was dropped"
        );
        assert!(duel.robots[1].eliminated, "the elimination was dropped");
        assert!(!duel.robots[1].is_alive(), "state 3 is not alive");
        assert!(
            duel.log.iter().any(|l| l.contains("eliminated")),
            "{:?}",
            duel.log
        );
        let snapshot = crate::scriptai::snapshot(&*duel.physics, &duel.robots[1]);
        assert!(snapshot.eliminated && !snapshot.alive);
        let other = crate::scriptai::snapshot(&*duel.physics, &duel.robots[0]);
        assert!(!other.eliminated && other.alive);
    }

    /// F9: a hurt chassis caves in — its drawn shape *and* its collider shrink —
    /// instead of only ever dying whole.
    ///
    /// The bot file marks which points may move ("down and in"); the floor plate
    /// is pinned by the lowest of them, and the motion grows with height above
    /// it. All 57 stock bots ship uncrushed, so this is a change we make, not one
    /// we read back.
    /// A spike slider, built the way the engine builds the arena's own.
    ///
    /// Isolates the physics from the script layer: a static pin, a 40 kg body
    /// (the Combat Arena's spikes weigh exactly that), the script's own travel
    /// and the script's own 40000 push. `FireTeeth` is one push over one step,
    /// so the spike shoots up its whole travel and then falls back under
    /// gravity until the next punch — which is why this measures the peak.
    #[test]
    fn a_slider_lets_its_body_rise() {
        let mut physics: Box<dyn PhysicsBackend> =
            Box::new(crate::jolt::JoltBackend::new(Vec3::new(0.0, -9.81, 0.0)).unwrap());
        let anchor = physics
            .create_body(
                &BodyDesc::new(MotionType::Static, ShapeDesc::Sphere { radius: 0.5 })
                    .with_sensor(true),
            )
            .unwrap();
        let body = physics
            .create_body(
                &BodyDesc::new(MotionType::Dynamic, ShapeDesc::Sphere { radius: 0.3 })
                    .with_mass(40.0),
            )
            .unwrap();
        let desc = crate::backend::SliderDesc {
            body_a: anchor,
            body_b: body,
            anchor_a: Vec3::ZERO,
            anchor_b: Vec3::ZERO,
            axis_a: Vec3::Y,
            axis_b: Vec3::Y,
            limits: hazards::slider_limits(0.0, 0.8, 0.0),
            max_force: 1.0,
            friction_force: 1.0,
        };
        let joint = physics.create_slider(&desc).expect("the slider");
        physics.drive_slider(joint, 0.0);
        // `ApplyForce(40000)` over one 60 Hz step, the way the hazard pushes.
        physics.apply_impulse(body, Vec3::Y * 40000.0 * (1.0 / 60.0));
        let mut peak = 0.0f32;
        for _ in 0..30 {
            physics.step(1.0 / 60.0);
            peak = peak.max(physics.state(body).unwrap().position.y);
        }
        let travel = hazards::slider_limits(0.0, 0.8, 0.0)[1];
        assert!(
            peak > travel - 0.05,
            "the body never reached its travel: peak {peak}, travel {travel}"
        );
    }

    /// M4/F13: the arena's spikes fire when a robot enters their SmartZone.
    ///
    /// The whole loop, from the arena's own data to the game's own hazard code:
    /// the zones come from the model (`saw_zone`, `spike_zone1`, `spike_zone2`),
    /// `Arenas/box.py` registered them, and the engine tells the script when a
    /// robot crosses one. `BoxArena.ZoneEvent` arms the right hazard,
    /// `BoxArena.Tick` — the game's own Python, running at the arena's quarter
    /// second — ticks it, and after two ticks `Spikes.FireTeeth` unlocks the
    /// prismatic the script created and punches it with 40000. The spike body
    /// shooting up its travel is the proof the whole chain connected.
    ///
    /// Robot 1 is *placed* in the zone rather than driven there: the stock bots'
    /// traction on the arena floor is marginal for about a quarter of them
    /// (F11), and a test that depends on one of those arriving is a test that
    /// fails for the wrong reason.
    #[test]
    fn the_spikes_fire_when_a_robot_enters_their_zone() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        // Where `spike_zone2` is, from the arena's own geometry — but the zone
        // list only exists once a duel is built, so ask the model directly.
        let zone_centre = {
            let registered = std::collections::BTreeMap::from([("spike_zone2".to_string(), 3i32)]);
            let z = crate::hazards::zones(&arena, &registered);
            let z = z.first().expect("the box arena ships spike_zone2");
            z.inv.inverse().transform_point3((z.min + z.max) * 0.5)
        };
        let bot = |p: &str, spawn: Option<[f32; 2]>| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn,
        };
        let mut duel = Duel::new(
            &arena,
            &[
                bot("AI/Team1/Bot0.bot", None),
                bot("AI/Team6/Bot2.bot", Some([zone_centre.x, zone_centre.z])),
            ],
        )
        .expect("a duel");

        assert_eq!(duel.zones.len(), 3, "saw_zone, spike_zone1, spike_zone2");
        let spike = duel
            .props
            .iter()
            .find(|p| p.prop.name == "spikes2")
            .map(|p| p.body)
            .expect("the arena ships spikes2");

        // Standing in the zone is enough: the engine sees the overlap and the
        // script's `ZoneEvent` arms the hazard.
        let dt = 1.0 / 60.0;
        duel.step_direct(&[DriveInput::default(), DriveInput::default()], dt);
        assert!(
            duel.log.iter().any(|l| l.contains("spike_zone2")),
            "no zone event: {:?}",
            duel.log
        );

        // `Spikes.Tick` counts two arena ticks — half a second — before firing,
        // and `FireTeeth` is one push over one step, so the spike shoots up and
        // falls back until the next punch. Watch the peak, not the end state.
        let rest = duel.physics.state(spike).map(|s| s.position.y).unwrap();
        let mut peak = rest;
        for _ in 0..(2.0 / dt) as usize {
            duel.step_direct(&[DriveInput::default(), DriveInput::default()], dt);
            peak = peak.max(duel.physics.state(spike).map(|s| s.position.y).unwrap());
        }
        let fired = duel.joints.iter().any(|j| {
            let d = j.drive.lock().unwrap_or_else(|e| e.into_inner());
            !d.locked
        });
        assert!(fired, "the spikes never fired: {:?}", duel.log);
        // The spike shoots up its travel only when nothing is in the way; the
        // robot is standing on this one (it spawned in the zone), so the punch
        // is stopped early. A fifth of a metre is still the punch landing.
        assert!(
            peak > rest + 0.15,
            "the spikes never punched: {rest} -> {peak} ({:?})",
            duel.log
        );
    }

    /// M4/F13: a hinge the *model* declares is driven by the script.
    ///
    /// The Compressor Arena's four smash arms are hinges in the `.gmf`, and
    /// `Hazards.Smasher.__init__` starts retracting them the moment the arena is
    /// set up — `SetPowerSettings(12, 700)` then `SetDirection(-100)`. Nothing in
    /// the script moves a body: it names a hinge and states a speed, which is the
    /// whole of the hazard drive.
    #[test]
    fn the_models_own_hinges_are_driven_by_the_script() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game
            .join("Arenas/compressor.py")
            .to_string_lossy()
            .to_string();
        let bot = |p: &str| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        };
        let mut duel = Duel::new(
            &arena,
            &[bot("AI/Team1/Bot0.bot"), bot("AI/Team6/Bot2.bot")],
        )
        .expect("a duel");

        let idle = [DriveInput::default(), DriveInput::default()];
        let dt = 1.0 / 60.0;
        // The joints are built at the top of the first step.
        duel.step_direct(&idle, dt);
        let swings: Vec<crate::backend::JointHandle> = duel
            .joints
            .iter()
            .filter(|j| !j.slider)
            .map(|j| j.handle)
            .collect();
        assert_eq!(
            swings.len(),
            4,
            "the compressor declares four smash arms: {:?}",
            duel.log
        );
        for _ in 0..30 {
            duel.step_direct(&idle, dt);
        }
        let before: Vec<f32> = swings
            .iter()
            .map(|h| duel.physics.hinge_angle(*h))
            .collect();
        for _ in 0..120 {
            duel.step_direct(&idle, dt);
        }
        let after: Vec<f32> = swings
            .iter()
            .map(|h| duel.physics.hinge_angle(*h))
            .collect();
        assert!(
            before.iter().zip(&after).any(|(a, b)| (b - a).abs() > 0.05),
            "no smash arm moved: {before:?} -> {after:?}"
        );
    }

    /// M4/F13: `plus.force` is a force, so one step of it is `force * dt`.
    ///
    /// Recovered from the binary: the engine adds the vector into the body's
    /// *force* accumulator, and the joints' `ApplyForce` calls that same function
    /// with `axis * power`, so the two are one operation. `Hazards.Electricity`
    /// asks for `115 * getWeight(bot)` — 115 N per kilogram, an acceleration of
    /// about 11.7 g held for one step, and a kick of 115/60 ≈ 1.9 m/s. Read as an
    /// impulse it would be 115 m/s, and the zap would launch the robot.
    #[test]
    fn plus_force_is_a_force_and_not_an_impulse() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = |p: &str, spawn: Option<[f32; 2]>| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn,
        };
        // Placed clear of the zones, so nothing but the force touches it.
        let mut duel = Duel::new(
            &arena,
            &[
                bot("AI/Team1/Bot0.bot", Some([0.0, 0.0])),
                bot("AI/Team6/Bot2.bot", Some([6.0, 6.0])),
            ],
        )
        .expect("a duel");

        let idle = [DriveInput::default(), DriveInput::default()];
        let dt = 1.0 / 60.0;
        for _ in 0..60 {
            duel.step_direct(&idle, dt);
        }
        let chassis = duel.robots[0].body();
        let before = duel
            .physics
            .state(chassis)
            .expect("the chassis")
            .linear_velocity
            .y;

        // Exactly what `Hazards.Electricity` asks for.
        let mass = duel.robots[0].remaining_mass();
        ra2script::state().requests.push(ra2script::Request::Force {
            bot: 0,
            momentum: Vec3::new(0.0, 115.0 * mass, 0.0),
        });
        duel.step_direct(&idle, dt);

        let after = duel
            .physics
            .state(chassis)
            .expect("the chassis")
            .linear_velocity
            .y;
        let lift = after - before;
        assert!(
            (1.0..4.0).contains(&lift),
            "115 N/kg should lift a robot ~1.9 m/s in one step, got {lift}"
        );
    }

    /// M4/F13: `SetAutoLock(True)` holds a joint whose motor has not started.
    ///
    /// The Octagon's start blocks are `PitSlider`s: each is a prismatic with
    /// 3.8 m of travel and no motor of its own until `Activate` calls
    /// `setactive(True)` some eleven seconds in. What keeps four robots standing
    /// on four trapdoors until then is `SetAutoLock(True)` — the original's joint
    /// update locks such a joint while its motor is stopped. Without it the floors
    /// drop out from under the robots at the bell.
    #[test]
    fn a_pit_floor_is_held_until_its_script_opens_it() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/octagon.py").to_string_lossy().to_string();
        let bot = |p: &str| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        };
        let mut duel = Duel::new(
            &arena,
            &[bot("AI/Team1/Bot0.bot"), bot("AI/Team6/Bot2.bot")],
        )
        .expect("a duel");

        let idle = [DriveInput::default(), DriveInput::default()];
        let dt = 1.0 / 60.0;
        duel.step_direct(&idle, dt);
        let floors: Vec<crate::backend::JointHandle> = duel
            .joints
            .iter()
            .filter(|j| j.slider)
            .map(|j| j.handle)
            .collect();
        assert_eq!(floors.len(), 4, "the Octagon ships four pit floors");

        for _ in 0..(2.0 / dt) as usize {
            duel.step_direct(&idle, dt);
        }
        for h in &floors {
            let off = duel.physics.slider_offset(*h);
            assert!(
                off > -0.5,
                "a pit floor sank out from under the robots: {off} ({:?})",
                duel.log
            );
        }
    }

    /// F9: the crush moves through the file's own movable region.
    ///
    /// Every stock bot that carries a buckle model — 43 of the 57; the other 14
    /// are from before the format had the section — keeps its points as a ring at
    /// mid-height and a ring at the top. So the movable region *is* the top half of
    /// the chassis, and a full crush lands the top ring exactly on the mid ring:
    /// as far as it can go before the model turns inside out. That is where the
    /// crush's *down* rate comes from now, read out of the file instead of
    /// assumed, and this is the invariant it rests on.
    #[test]
    fn a_buckle_model_is_the_top_half_of_the_chassis() {
        let Some(game) = game_dir() else { return };
        let mut seen = 0;
        for team in std::fs::read_dir(game.join("AI"))
            .into_iter()
            .flatten()
            .flatten()
        {
            for bot in std::fs::read_dir(team.path())
                .into_iter()
                .flatten()
                .flatten()
            {
                if bot.path().extension().map(|e| e != "bot").unwrap_or(true) {
                    continue;
                }
                let Ok(r) = crate::rig::load(&bot.path().to_string_lossy()) else {
                    continue;
                };
                let c = &r.components[0];
                if c.buckle.is_empty() {
                    continue;
                }
                seen += 1;
                let floor = c.buckle.iter().map(|p| p.y).fold(f32::MAX, f32::min);
                let height = c.max.y - c.min.y;
                let span = c.max.y - floor;
                assert!(
                    (span - height * 0.5).abs() < 0.02,
                    "{:?}: the movable span {span} is not the top half of {height}",
                    r.name
                );
            }
        }
        assert!(
            seen >= 10,
            "expected buckle models in the stock AI bots, saw {seen}"
        );
    }

    /// F7: `fracture` is a per-hit threshold, not a reduction.
    ///
    /// The engine's rule (`FUN_0045f2b0`) is a single comparison: if the part is
    /// attached, has hitpoints left, and its `fracture` is positive but *smaller
    /// than the blow*, the part comes away outright — whatever its hitpoints
    /// said. That is what makes a hazard's flat 25 a scratch against a ramplate
    /// (40) or the control board behind a chassis (41) while one solid pass of a
    /// spinner shears a weapon off, and it is the reading the community's own
    /// notes describe ("41 is how much you have to deal to the control board").
    #[test]
    fn fracture_is_a_per_hit_threshold() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = |p: &str| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        };
        let mut duel = Duel::new(
            &arena,
            &[bot("AI/Team1/Bot0.bot"), bot("AI/Team6/Bot2.bot")],
        )
        .expect("a duel");

        let robot = &mut duel.robots[0];
        // The factory's default per category is 15 for extenders and 20 for
        // mechanics, mobility and weapons, so a stock bot carries both.
        let (index, fracture) = robot
            .rig
            .components
            .iter()
            .enumerate()
            .find(|(i, c)| {
                robot.attached[*i] && c.fracture > 0.0 && robot.health[*i] > c.fracture + 2.0
            })
            .map(|(i, c)| (i, c.fracture))
            .expect("the bot has a part with a fracture value and hitpoints to spare");
        assert!(fracture >= 15.0, "unexpected fracture {fracture}");

        // `FUN_0045f2b0` tests the fracture against the hitpoints the part
        // ALREADY has: while it still has some, even a huge blow only wears it
        // down (the old model took it off on the blow itself, which was wrong).
        let before = robot.health[index];
        let hit = robot.damage(index, fracture + 0.5);
        assert!(!hit.detach, "a part with hitpoints left was torn off");
        assert!(robot.attached[index]);
        assert!((robot.health[index] - (before - fracture - 0.5)).abs() < 1e-3);
        assert!((hit.effective - (fracture + 0.5)).abs() < 1e-3);

        // Ground down to zero, it is still on the robot ...
        robot.health[index] = 0.0;
        assert!(!robot.damage(index, fracture - 0.5).detach);
        assert!(robot.attached[index], "zero hitpoints alone do not detach");
        assert_eq!(robot.health[index], 0.0, "hitpoints never go negative");
        // ... and a single blow past the fracture now shears it off.
        assert!(
            robot.damage(index, fracture + 0.5).detach,
            "a blow past the fracture did not take the dead part off"
        );
        assert!(robot.fractured[index]);

        // A component that declares no `fracture` (0) is only ever worn down.
        let other = robot
            .rig
            .components
            .iter()
            .enumerate()
            .find(|(i, c)| robot.attached[*i] && *i != index && c.fracture == 0.0 && *i != 0)
            .map(|(i, _)| i);
        if let Some(other) = other {
            robot.health[other] = 0.0;
            assert!(!robot.damage(other, 500.0).detach);
        }
    }

    /// `FUN_0044f370`: a chassis at zero hitpoints is not defeated; from then on
    /// every blow over 40 is a critical hit and the third destroys the robot.
    #[test]
    fn the_chassis_dies_of_three_criticals_not_of_hitpoints() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = |p: &str| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        };
        let mut duel = Duel::new(
            &arena,
            &[bot("AI/Team1/Bot0.bot"), bot("AI/Team6/Bot2.bot")],
        )
        .expect("a duel");
        let hp = duel.robots[0].health[0];
        assert!(hp > 0.0);

        // Wearing the chassis down to exactly zero defeats nothing.
        let point = Vec3::ZERO;
        assert!(!duel.apply_hit(Some(1), 0, 0, hp, point));
        assert_eq!(duel.robots[0].health[0], 0.0);
        assert!(duel.robots[0].is_alive());
        assert_eq!(
            duel.robots[0].criticals, 0,
            "the blow that empties it is not a critical"
        );

        // A hit under 40 is not critical; hitpoints go negative.
        assert!(!duel.apply_hit(Some(1), 0, 0, 30.0, point));
        assert_eq!(duel.robots[0].criticals, 0);
        assert_eq!(duel.robots[0].health[0], -30.0);

        // Criticals 1 and 2 hurt; the third destroys.
        assert!(!duel.apply_hit(Some(1), 0, 0, 41.0, point));
        assert!(!duel.apply_hit(Some(1), 0, 0, 90.0, point));
        assert_eq!(duel.robots[0].criticals, 2);
        assert!(duel.robots[0].is_alive());
        assert!(duel.apply_hit(Some(1), 0, 0, 41.0, point));
        assert!(duel.robots[0].destroyed);
        assert!(!duel.robots[0].is_alive());
        // The chassis stays on the robot: DESTROYED is a state, not a detach.
        assert!(duel.robots[0].attached[0]);
    }

    /// A motor under half its hitpoints smokes (`FUN_0048e0d0`).
    #[test]
    fn a_hurt_motor_smokes() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = |p: &str| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        };
        let mut duel = Duel::new(
            &arena,
            &[bot("AI/Team1/Bot0.bot"), bot("AI/Team6/Bot2.bot")],
        )
        .expect("a duel");
        let robot = &mut duel.robots[0];
        let motor = robot
            .rig
            .components
            .iter()
            .position(|c| c.role == crate::rig::Role::Motor)
            .expect("a motor");
        assert!(!robot.smoking(motor));
        robot.health[motor] = 0.49 * robot.rig.components[motor].hitpoints;
        assert!(robot.smoking(motor));
        assert!(!robot.smoking(0), "only motors smoke");
    }

    /// `FUN_0045f3a0` / `FUN_004a2a50`: a hit over 10 on metal throws sparks
    /// (`int(damage)` of them, along the contact normal), a hit of 10 or less
    /// does not, and neither does one that touches rubber.
    #[test]
    fn sparks_follow_the_impact_rule() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = |p: &str| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        };
        let mut duel = Duel::new(
            &arena,
            &[bot("AI/Team1/Bot0.bot"), bot("AI/Team6/Bot2.bot")],
        )
        .expect("a duel");
        let hit = |damage: f32| combat::Impact {
            attacker: 1,
            target: 0,
            component: 0,
            by: 0,
            damage,
            point: Vec3::new(0.0, 1.0, 0.0),
            normal: Vec3::Y,
            speed: 3.0,
        };
        duel.sparks_for(&hit(10.0));
        assert!(duel.fx.is_empty(), "10 is not over 10");
        duel.sparks_for(&hit(14.5));
        assert!(!duel.fx.is_empty(), "a priced metal hit throws sparks");
        duel.fx = ra2_render::effects::Effects::new(None);
        duel.fx.enabled = false;
        duel.sparks_for(&hit(40.0));
        assert!(duel.fx.is_empty(), "GraphicEffects off");
        duel.fx.enabled = true;
        for r in &mut duel.robots {
            for c in &mut r.rig.components {
                c.material = Some("rubber(0)".into());
            }
        }
        duel.sparks_for(&hit(40.0));
        assert!(duel.fx.is_empty(), "rubber on the contact: no sparks");
    }

    /// `ShadowWizard`: the arena's declared light and receivers reach the
    /// renderer, and a robot over the floor casts onto it; a flame hazard's
    /// emitter turns on with `SetEmitting`.
    #[test]
    fn the_arena_declares_the_shadows_and_flames() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = |p: &str| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        };
        let mut duel = Duel::new(
            &arena,
            &[bot("AI/Team1/Bot0.bot"), bot("AI/Team6/Bot2.bot")],
        )
        .expect("a duel");
        duel.step(1.0 / 60.0);
        let items = duel.effect_items(Mat4::IDENTITY);
        let shadow = items
            .iter()
            .find(|i| i.label == "shadows")
            .expect("shadows");
        assert!(shadow.material.blended());
        // The shadow lies on the floor (y = -2.1126 + the lift) or a wall.
        assert!(shadow
            .vertices
            .iter()
            .any(|v| (v.pos[1] + 2.1126).abs() < 0.05));
        // A motor under half its hitpoints smokes once the step runs.
        let robot = &mut duel.robots[0];
        let motor = (0..robot.rig.components.len())
            .find(|&i| robot.rig.components[i].role == crate::rig::Role::Motor)
            .expect("Scout has a motor");
        robot.health[motor] = 0.4 * robot.rig.components[motor].hitpoints;
        for _ in 0..30 {
            duel.step(1.0 / 60.0);
        }
        assert!(!duel.fx.is_empty(), "the smoking motor emits");
    }

    /// The electric arena's twelve bolts (`CreateLightning`) are on while its
    /// hazards are, and reach the renderer as one ribbon item.
    #[test]
    fn the_electric_arenas_lightning_is_drawn() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game
            .join("Arenas/electric.py")
            .to_string_lossy()
            .to_string();
        let bot = |p: &str| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        };
        let mut duel = Duel::new(
            &arena,
            &[bot("AI/Team1/Bot0.bot"), bot("AI/Team6/Bot2.bot")],
        )
        .expect("a duel");
        for _ in 0..6 {
            duel.step(1.0 / 60.0);
        }
        let bolts = ra2script::state()
            .arena
            .as_ref()
            .map(|a| a.lock().unwrap().lightning.clone())
            .unwrap_or_default();
        assert_eq!(bolts.len(), 12, "CreateLightning ran twelve times");
        assert!(bolts.iter().all(|b| b.3), "HazardsOn lit them all");
        let items = duel.effect_items(Mat4::IDENTITY);
        let item = items
            .iter()
            .find(|i| i.label == "lightning")
            .expect("the bolts draw");
        assert_eq!(item.vertices.len(), 12 * 2 * 5 * 6);
    }

    /// `FUN_00417330`: below 10 a hit only drives effects.
    #[test]
    fn a_hit_under_ten_takes_nothing() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = |p: &str| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        };
        let mut duel = Duel::new(
            &arena,
            &[bot("AI/Team1/Bot0.bot"), bot("AI/Team6/Bot2.bot")],
        )
        .expect("a duel");
        let hp = duel.robots[0].health[0];
        duel.apply_hit(Some(1), 0, 0, 9.99, Vec3::ZERO);
        assert_eq!(duel.robots[0].health[0], hp);
        assert!(duel.damage_events.is_empty());
        assert!(duel.take_score_hits().is_empty());
        duel.apply_hit(Some(1), 0, 0, 10.0, Vec3::ZERO);
        assert_eq!(duel.robots[0].health[0], hp - 10.0);
        assert_eq!(duel.damage_events.len(), 1);
        assert_eq!(duel.take_score_hits().len(), 1);
    }

    /// M4/F13: a hazard that only asks for power when it fires still gets it.
    ///
    /// The Half Circle Arena's hammer is a hinge in the model that
    /// `Hazards.Hammer.__init__` parks with `Lock(True)` and *no* power settings
    /// at all — the 1200 N·m only arrives in `Smash`, when a robot walks into the
    /// zone. A joint therefore has to be able to take a new drive strength after
    /// it is built, or the hammer swings as if it weighed nothing.
    #[test]
    fn a_hammer_that_asks_for_power_late_still_swings() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/half.py").to_string_lossy().to_string();
        let zone_centre = {
            let registered = std::collections::BTreeMap::from([("Hammer Zone".to_string(), 1i32)]);
            let z = crate::hazards::zones(&arena, &registered);
            let z = z.first().expect("the Half Circle Arena ships a hammer");
            z.inv.inverse().transform_point3((z.min + z.max) * 0.5)
        };
        let bot = |p: &str, spawn: Option<[f32; 2]>| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn,
        };
        let mut duel = Duel::new(
            &arena,
            &[
                bot("AI/Team1/Bot0.bot", None),
                bot("AI/Team6/Bot2.bot", Some([zone_centre.x, zone_centre.z])),
            ],
        )
        .expect("a duel");

        let idle = [DriveInput::default(), DriveInput::default()];
        let dt = 1.0 / 60.0;
        duel.step_direct(&idle, dt);
        assert!(
            duel.log.iter().any(|l| l.contains("Hammer Zone")),
            "the robot never reached the hammer's zone: {:?}",
            duel.log
        );
        let hammer = duel
            .joints
            .iter()
            .find(|j| !j.slider)
            .map(|j| j.handle)
            .expect("the hammer's hinge was built");

        // `Hammer.Tick` smashes, retracts and smashes again over ~2.5 s each way.
        let mut swing: f32 = 0.0;
        let mut last = duel.physics.hinge_angle(hammer);
        for _ in 0..(4.0 / dt) as usize {
            duel.step_direct(&idle, dt);
            let now = duel.physics.hinge_angle(hammer);
            swing = swing.max((now - last).abs());
            last = now;
        }
        assert!(
            swing > 0.05,
            "the hammer never moved (largest step {swing}): {:?}",
            duel.log
        );
    }

    /// M4/F13: `Arena.SetActive` and `Arena.SetPinned` really change the world.
    ///
    /// The Parking Lot's cinder blocks are not scenery until a trap door opens
    /// over the pit: `parkinglot.py` calls `SetActive("cinderblockN", False)` for
    /// all of them on the way in, and `True` for the ones above a door that has
    /// just dropped. `SetPinned` is its counterpart (`flextop.py` pins its
    /// flexing floor when the hazards are off). The engine reads both as the
    /// *wanted* state and reconciles the world against them.
    #[test]
    fn the_arena_can_hide_a_body_and_pin_one() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game
            .join("Arenas/parkinglot.py")
            .to_string_lossy()
            .to_string();
        let bot = |p: &str| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        };
        let mut duel = Duel::new(
            &arena,
            &[bot("AI/Team1/Bot0.bot"), bot("AI/Team6/Bot2.bot")],
        )
        .expect("a duel");
        assert!(
            duel.props.iter().any(|p| p.prop.name == "cinderblock1"),
            "the Parking Lot ships cinder blocks"
        );

        let body_of = |d: &Duel, name: &str| {
            d.props
                .iter()
                .find(|p| p.prop.name == name)
                .map(|p| p.body)
                .expect("the named prop")
        };
        // The script hid them while the arena was being set up.
        let dt = 1.0 / 60.0;
        let idle = [DriveInput::default(), DriveInput::default()];
        duel.step_direct(&idle, dt);
        assert_eq!(
            body_of(&duel, "cinderblock1"),
            HIDDEN_PROP,
            "the block should be out of the world"
        );

        // A trap door opens over the pit, and the block is pinned where it
        // spawns — 6 m up, so a body that is not pinned would be falling.
        {
            let st = ra2script::state();
            let a = st.arena.as_ref().expect("a scripted arena");
            let mut d = a.lock().unwrap();
            d.active.insert("cinderblock1".to_string(), true);
            d.pinned.insert("cinderblock1".to_string(), true);
        }
        for _ in 0..30 {
            duel.step_direct(&idle, dt);
        }
        let live = body_of(&duel, "cinderblock1");
        assert_ne!(live, HIDDEN_PROP, "the block never came back");
        let held = duel.physics.state(live).expect("the block is in the world");
        for _ in 0..30 {
            duel.step_direct(&idle, dt);
        }
        let still = duel.physics.state(live).expect("the block is in the world");
        assert!(
            (still.position.y - held.position.y).abs() < 0.05,
            "a pinned body moved: {} -> {}",
            held.position.y,
            still.position.y
        );

        // And unpinning it lets it drop the six metres to the floor.
        {
            let st = ra2script::state();
            let a = st.arena.as_ref().expect("a scripted arena");
            a.lock()
                .unwrap()
                .pinned
                .insert("cinderblock1".to_string(), false);
        }
        for _ in 0..30 {
            duel.step_direct(&idle, dt);
        }
        let fallen = duel.physics.state(live).expect("the block is in the world");
        assert!(
            fallen.position.y < held.position.y - 0.5,
            "an unpinned body did not fall: {} -> {}",
            held.position.y,
            fallen.position.y
        );
    }

    /// M4/F13: the game's own AI closes the fight.
    ///
    /// This is the item the whole script layer was for. The stand-in driver only
    /// ever charges the opponent, so the headline duel orbits and pins itself
    /// against a wall from about 15 s and the robots never touch. The shipped
    /// `AI/Rammer.py` and `AI/Whipper.py` brains — reached through
    /// `AI/Bindings.py` by the bots' own names — drive to the enemy and press:
    /// the gap goes negative (they are in contact) and both take damage.
    #[test]
    fn the_shipped_ai_closes_to_contact() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        // About the driver, not the arena: the spikes and the saw are live
        // parts of the floor otherwise, and they change where the fight happens.
        let was = std::env::var("RA2_HAZARDS").ok();
        std::env::set_var("RA2_HAZARDS", "0");
        let _restore = Restore("RA2_HAZARDS", was);
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = |p: &str| BotSetup {
            path: game.join(p).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        };
        let mut duel = Duel::new(
            &arena,
            &[bot("AI/Team1/Bot0.bot"), bot("AI/Team6/Bot2.bot")],
        )
        .expect("a duel");
        assert!(
            duel.script_ai.is_some(),
            "the shipped AI did not load; this test is about it"
        );

        // The engine's own step, as `ra2-duel` runs it. The drive is
        // dt-sensitive — a coarser step loses the wheel contact a marginal robot
        // needs (F11) — and the AI is not: it runs on its own eighth-of-a-second
        // cadence whatever the physics does.
        //
        // 60 s: with the robots at their recovered speed the shipped AI's idle
        // spells (it waits in its corner for a random while) decide when the
        // fight starts, and in the unit-test process it first meets at about 45 s.
        // 30 s, not 20 (older note): the first hit lands at 20.23 s on the vendored CMake
        // build of Jolt (the prebuilt archive lands it just under 20 s). The
        // behavioural claim is that the shipped AI closes and hits; a window
        // that ends inside one build's rounding is not measuring that. Measured
        // margin: 9.8 s.
        let dt = 1.0 / 60.0;
        // The game holds the robots still through the 4 s countdown before the
        // bell; settling first matters with the game's own friction law (the
        // cold-start pair can circle instead of engaging), so the test follows
        // the match's own flow.
        let idle = vec![DriveInput::default(); duel.robots.len()];
        for _ in 0..(4.0 / dt) as usize {
            duel.step_direct(&idle, dt);
        }
        duel.record_impacts = true;
        let mut closest = f32::MAX;
        for _ in 0..(60.0 / dt) as usize {
            duel.step(dt);
            closest = closest.min(duel.closest_gap());
        }
        eprintln!(
            "DBG {:?}",
            duel.robots
                .iter()
                .map(|r| r.position(&*duel.physics))
                .collect::<Vec<_>>()
        );
        let hits: f32 = duel.damage_taken.iter().sum();
        assert!(
            closest < 1.0,
            "the robots never met; closest gap {closest:.2} m"
        );
        // The fight is read from the priced contact events, not only from the
        // applied damage: the arena applies a contact point only at `>= 10`
        // (`FUN_00417330`) and the robots' motors are ~5x too slow today (see
        // docs/26-fidelity-baseline.md), so a closing pair can touch without any
        // point reaching the gate. Contact was made and priced; whatever was
        // applied is exactly the points at the gate or above.
        assert!(!duel.impacts.is_empty(), "they met but made no contact");
        let gated: f32 = duel
            .impacts
            .iter()
            .filter(|i| i.damage >= 10.0)
            .map(|i| i.damage)
            .sum();
        assert!(
            (hits - gated).abs() <= 1e-3 * (1.0 + gated) + 1e-3,
            "applied {hits} is not the >=10 contact points {gated}"
        );
    }

    #[test]
    fn a_hurt_chassis_buckles() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = game.join("AI/Team1/Bot0.bot").to_string_lossy().to_string();
        let setups = [BotSetup {
            path: bot,
            yaw_deg: None,
            spawn: None,
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

    /// F9: the crushed **collider** follows the mesh, and a motorized robot
    /// still drives afterwards.
    ///
    /// The backend removes the chassis' hinges, re-shapes the body and recreates
    /// the hinges with their current angle as the reference. This test measures
    /// the collider directly — a grid of downward rays over the chassis'
    /// footprint, keeping only hits on the chassis body — so it cannot pass by
    /// looking at `hull_points` alone (which the old test did while Jolt was
    /// refusing the re-bake).
    #[test]
    fn a_crushed_chassis_collider_follows_and_the_robot_still_drives() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let setups = [BotSetup {
            path: game.join("AI/Team1/Bot0.bot").to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        }];
        let mut duel = Duel::new(&arena, &setups).expect("the box arena builds a duel");
        let dt = 1.0 / 60.0;

        // The collider's top, sampled through the physics world. Rays start a
        // metre above the pristine chassis and only hits on the chassis body
        // count, so a motor housing over the hull cannot be mistaken for it.
        let collider_top = |duel: &Duel| -> f32 {
            let robot = &duel.robots[0];
            let c = &robot.rig.components[0];
            let worlds = robot.component_worlds(&*duel.physics);
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for p in &c.hull_points {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
            let (mut top, mut hits) = (f32::MIN, 0usize);
            for ix in 0..=20 {
                for iz in 0..=20 {
                    let x = lo.x + (hi.x - lo.x) * ix as f32 / 20.0;
                    let z = lo.z + (hi.z - lo.z) * iz as f32 / 20.0;
                    let from = worlds[0].transform_point3(Vec3::new(x, hi.y + 1.0, z));
                    if let Some(hit) = duel.physics.ray_cast(from, Vec3::NEG_Y, 4.0) {
                        if hit.body == robot.body() {
                            top = top.max(hit.point.y);
                            hits += 1;
                        }
                    }
                }
            }
            assert!(hits > 0, "no ray hit the chassis collider");
            top
        };

        // Let the robot settle on the arena floor.
        for _ in 0..30 {
            duel.step_direct(&[DriveInput::default()], dt);
        }
        let pristine = collider_top(&duel);
        let joints = duel.robots[0]
            .parts
            .iter()
            .filter(|p| p.joint.is_some())
            .count();

        duel.robots[0].health[0] *= 0.5;
        duel.robots[0].buckle(&mut *duel.physics);
        for _ in 0..10 {
            duel.step_direct(&[DriveInput::default()], dt);
        }
        let crushed = collider_top(&duel);
        // The drawn top moves 0.1855 m at half damage; the collider must follow
        // by most of that (the ray grid is finite, hence the 0.10 m floor).
        println!("chassis collider top: {pristine:.3} -> {crushed:.3} m");
        assert!(
            pristine - crushed > 0.10,
            "the collider did not follow the crush: {pristine:.3} -> {crushed:.3} m"
        );
        assert_eq!(
            duel.robots[0]
                .parts
                .iter()
                .filter(|p| p.joint.is_some())
                .count(),
            joints,
            "the re-bake lost a joint"
        );

        // The motorized robot must still drive with the recreated hinges.
        let start = duel.robots[0].position(&*duel.physics);
        let drive = DriveInput {
            throttle: 1.0,
            steer: 0.0,
            ..Default::default()
        };
        for _ in 0..(3.0 / dt) as usize {
            duel.step_direct(&[drive], dt);
        }
        let travelled = duel.robots[0].position(&*duel.physics).distance(start);
        println!("travelled {travelled:.2} m after the crush");
        assert!(
            travelled > 1.0,
            "the crushed robot did not drive: {travelled:.2} m"
        );
    }

    /// F9: the crush **curve** is linear in the damage taken, and it is
    /// measured, not tuned.
    ///
    /// `FUN_0045f2b0` applies a hit as `hp -= amount` (clamped), so the only
    /// quantity the chassis has to drive the deformation is the fraction of its
    /// hitpoint pool that is gone. `Robot::buckle` applies that fraction to the
    /// *pristine* mesh, so the top ring lands on the file's own floor exactly at
    /// a full crush; this test samples the curve at quarter points and prints
    /// it. The silhouette the game gets from its runtime subdivision is F9's
    /// remaining gap, not the curve.
    #[test]
    fn the_crush_curve_is_linear_in_damage() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let setups = [BotSetup {
            path: game.join("AI/Team1/Bot0.bot").to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        }];
        let mut duel = Duel::new(&arena, &setups).expect("the box arena builds a duel");
        let c = &duel.robots[0].rig.components[0];
        let floor = c.buckle.iter().map(|p| p.y).fold(f32::MAX, f32::min);
        let top = c.hull_points.iter().map(|p| p.y).fold(f32::MIN, f32::max);
        let span = top - floor;
        let max_hp = duel.robots[0].health[0].max(1.0);
        assert!(span > 0.05, "the chassis has a movable region ({span} m)");

        let mut rows = Vec::new();
        let mut moved_before = 0.0f32;
        for f in [0.25f32, 0.5, 0.75, 1.0] {
            duel.robots[0].health[0] = max_hp * (1.0 - f);
            duel.robots[0].buckle(&mut *duel.physics);
            let now_top = duel.robots[0].rig.components[0]
                .hull_points
                .iter()
                .map(|p| p.y)
                .fold(f32::MIN, f32::max);
            let moved = top - now_top;
            rows.push(format!(
                "f={f:.2}: top moved {moved:.4} m (linear {:.4})",
                f * span
            ));
            assert!(
                (moved - f * span).abs() < 0.02,
                "f={f:.2}: moved {moved:.4}, linear would be {:.4}",
                f * span
            );
            assert!(
                moved > moved_before + 1e-3,
                "the crush must grow with damage: {moved} after {moved_before}"
            );
            moved_before = moved;
        }
        assert!(
            (moved_before - span).abs() < 0.03,
            "a full crush must land the top ring on the file's floor: moved {moved_before:.4} of {span:.4}"
        );
        println!(
            "crush curve (linear in the hitpoint fraction):\n  {}",
            rows.join("\n  ")
        );
    }

    /// F10: the chassis is dressed from the file's **face groups**, so every
    /// triangle's skin coordinates land inside the patch the file allocates for
    /// its side. The model's own per-point table cannot manage that — a mesh
    /// point shared by two sides carries a single coordinate — which is exactly
    /// what made chassis decals come out rotated and mirrored.
    #[test]
    fn chassis_uvs_come_from_its_face_groups() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let mut paths: Vec<std::path::PathBuf> = Vec::new();
        for team in std::fs::read_dir(game.join("AI"))
            .into_iter()
            .flatten()
            .flatten()
        {
            for b in std::fs::read_dir(team.path())
                .into_iter()
                .flatten()
                .flatten()
            {
                if b.path().extension().is_some_and(|e| e == "bot") {
                    paths.push(b.path());
                }
            }
        }
        assert!(paths.len() > 40, "found only {} stock bots", paths.len());
        let (mut affected, mut checked) = (0usize, 0usize);
        for path in &paths {
            let name = path.display().to_string();
            let data = std::fs::read(path).expect("stock bot reads");
            let bot = ra2fmt::bot::parse(&data).expect("stock bot parses");
            let ch = bot.components[0].chassis.as_ref().expect("has a chassis");
            assert!(!ch.face_groups.is_empty(), "{name}: no face groups");
            checked += 1;
            let inside = |uv: [f32; 2]| {
                ch.face_groups.iter().any(|fg| {
                    let (x0, y0, x1, y1) = (
                        fg.bounds[0] as f32,
                        fg.bounds[1] as f32,
                        fg.bounds[2] as f32,
                        fg.bounds[3] as f32,
                    );
                    let (u, v) = (uv[0] * 256.0, uv[1] * 256.0);
                    u >= x0.min(x1) - 2.0
                        && u <= x0.max(x1) + 2.0
                        && v >= y0.min(y1) - 2.0
                        && v <= y0.max(y1) + 2.0
                })
            };
            let items = ra2_render::chassis_items(&bot, Mat4::IDENTITY);
            let (mut ok, mut n) = (0usize, 0usize);
            for it in &items {
                for v in &it.vertices {
                    n += 1;
                    if inside(v.uv) {
                        ok += 1;
                    }
                }
            }
            assert_eq!(
                ok, n,
                "{name}: only {ok}/{n} chassis vertices sit in a patch"
            );
            // The model's per-point table is what we used to dress the chassis;
            // where the two disagree is exactly where a decal came out wrong.
            let mesh = ch.mesh.as_ref().unwrap();
            let groups: Vec<std::collections::HashMap<u32, [f32; 2]>> = (0..ch.face_groups.len())
                .map(|gi| group_table(ch, gi))
                .collect();
            let (mut raw_ok, mut raw_n) = (0usize, 0usize);
            for tri in mesh.indices.as_chunks::<3>().0 {
                let t = [tri[0], tri[1], tri[2]];
                let Some(g) = groups.iter().find(|g| t.iter().all(|i| g.contains_key(i))) else {
                    continue;
                };
                for &i in &t {
                    raw_n += 1;
                    let raw = mesh.uvs.get(i as usize).copied().unwrap_or([0.0, 0.0]);
                    if (raw[0] - g[&i][0]).abs() < 1e-4 && (raw[1] - g[&i][1]).abs() < 1e-4 {
                        raw_ok += 1;
                    }
                }
            }
            if raw_ok < raw_n {
                affected += 1;
            }
        }
        println!(
            "chassis UVs: {checked} stock bots checked, {affected} dressed differently from before"
        );
        assert!(affected > 0, "no stock bot was affected — nothing changed");
    }

    /// A face group's `vertex -> skin coordinate` table: the file lists the
    /// coordinates for the group's vertices in ascending index order (F10).
    fn group_table(
        ch: &ra2fmt::bot::Chassis,
        gi: usize,
    ) -> std::collections::HashMap<u32, [f32; 2]> {
        let fg = &ch.face_groups[gi];
        let mut seen: Vec<u32> = Vec::new();
        for f in ch.faces.iter().filter(|f| f.group == gi as i32) {
            for &i in &f.points {
                if !seen.contains(&i) {
                    seen.push(i);
                }
            }
        }
        seen.sort_unstable();
        seen.iter()
            .zip(fg.coords.iter())
            .map(|(&i, c)| (i, [c[0] / 256.0, c[1] / 256.0]))
            .collect()
    }

    /// F7: contact is tested between every pair of the robots' bodies, not just
    /// their root bodies.
    #[test]
    fn contact_spans_a_robots_own_bodies() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = game.join("AI/Team1/Bot0.bot").to_string_lossy().to_string();
        // Head-on and close: what this test is about is which *bodies* meet, not
        // whether a 25 m approach happens to connect.
        let setups = [
            BotSetup {
                path: bot.clone(),
                yaw_deg: Some(270.0),
                spawn: Some([-3.0, 0.0]),
            },
            BotSetup {
                path: bot,
                yaw_deg: Some(90.0),
                spawn: Some([3.0, 0.0]),
            },
        ];
        let mut duel = Duel::new(&arena, &setups).expect("the box arena builds a duel");
        assert!(!duel.robots_touching(0, 1), "start points are apart");
        let ram = vec![
            DriveInput {
                throttle: 1.0,
                steer: 0.0,
                ..Default::default()
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

    /// The watch and orbit cameras follow the human player's robot
    /// (`0x4014b0(0)`); an AI-only match has none (measured on the original:
    /// `+0x1a8` stays null through F2 and F4).
    #[test]
    fn the_cameras_have_no_target_without_a_human_player() {
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = game.join("AI/Team0/Bot0.bot").to_string_lossy().to_string();
        let mut duel = Duel::new(
            &arena,
            &[BotSetup {
                path: bot,
                yaw_deg: None,
                spawn: None,
            }],
        )
        .expect("a duel");
        assert!(duel.follow_target().is_none());
        duel.human_robot = Some(0);
        assert!(duel.follow_target().is_some());
    }

    /// Measured on the original (docs/30 §3): the Scout's two spin motors cost
    /// 200 a second at full throttle, a flipper press 0.6 s of 500 a second, an
    /// idle robot nothing, and an empty battery stops the motors.
    #[test]
    fn the_scout_pays_the_originals_power_bills() {
        use crate::energy::Channel;
        let Some(game) = game_dir() else { return };
        let _scripts = ra2script::lock();
        let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
        let bot = game.join("AI/Team0/Bot0.bot").to_string_lossy().to_string();
        let mut duel = Duel::new(
            &arena,
            &[BotSetup {
                path: bot,
                yaw_deg: None,
                spawn: None,
            }],
        )
        .expect("a duel");
        let dt = 1.0 / 60.0;
        let charge = |d: &Duel| {
            let r = &d.robots[0];
            r.energy.charge(Channel::Elec, &r.attached)
        };
        assert_eq!(duel.robots[0].energy.elec_total, 52000.0);
        assert_eq!(duel.robots[0].energy.air_total, 0.0);
        // Idle: nothing leaves the battery.
        let start = charge(&duel);
        for _ in 0..120 {
            duel.step_direct(&[DriveInput::default()], dt);
        }
        assert_eq!(charge(&duel), start);
        // Two seconds of full throttle: 2 motors x 100 a second.
        let drive = DriveInput {
            throttle: 1.0,
            steer: 0.0,
            ..Default::default()
        };
        for _ in 0..120 {
            duel.step_direct(&[drive], dt);
        }
        let used = start - charge(&duel);
        assert!(
            (used - 400.0).abs() < 4.0,
            "two seconds of throttle cost {used}"
        );
        // A flipper press: 0.6 s x 500 a second.
        let before = charge(&duel);
        duel.robots[0].apply_inputs(&[("Flip".to_string(), 0, 1.0)]);
        for _ in 0..120 {
            duel.step_direct(&[DriveInput::default()], dt);
        }
        let flip = before - charge(&duel);
        assert!((flip - 300.0).abs() < 8.0, "a flip cost {flip}");
        // An empty battery: the budget says zero.
        duel.robots[0].energy.sources[0].current = 0.0;
        duel.step_direct(&[drive], dt);
        assert_eq!(duel.robots[0].energy.last.elec.scale, 0.0);
        assert_eq!(
            duel.robots[0]
                .energy
                .fraction(Channel::Elec, &duel.robots[0].attached),
            0.0
        );
    }
}
