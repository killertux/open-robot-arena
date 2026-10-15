//! A simulated robot: its rig assembled into jointed rigid bodies.
//!
//! The original builds a robot out of one Havok body per rigidly-mounted group
//! of components, with the motors exporting as Havok constraints between them
//! (see `docs/05-validation-report.md`). We do the same: `rig::assemble` groups
//! the components into bodies, each body gets a compound shape from its members'
//! geometry, and every motor becomes a hinge the engine drives.

use glam::{Mat4, Quat, Vec3};

use crate::backend::{
    BackendError, BodyDesc, BodyHandle, CompoundChild, HingeDesc, JointHandle, MotionType,
    PhysicsBackend, ShapeDesc, SliderDesc,
};
use crate::rig::{JointInfo, JointUse, RobotRig};
use crate::scene::USER_BOT;
use crate::sim::{DriveInput, DriveTuning};

/// Stock component friction/restitution (see the validation report).
const STOCK_FRICTION: f32 = 0.30;
const STOCK_RESTITUTION: f32 = 0.30;

/// Live state of one assembled body.
pub struct PartState {
    pub body: BodyHandle,
    /// The hinge to the parent body, when a motor sits between them.
    pub joint: Option<JointHandle>,
    pub alive: bool,
}

/// One robot in the world.
pub struct Robot {
    pub rig: RobotRig,
    pub team: usize,
    /// One entry per [`crate::rig::Part`].
    pub parts: Vec<PartState>,
    /// Remaining hitpoints per component.
    pub health: Vec<f32>,
    /// Whether each component is still on the robot.
    pub attached: Vec<bool>,
    /// Natural mass × `mass_scale`.
    pub mass: f32,
    pub mass_scale: f32,
    /// The chassis body is gone: the robot is out of the fight.
    pub destroyed: bool,
}

impl Robot {
    /// Assemble and spawn the robot at `position`/`rotation`.
    ///
    /// Bodies are created first, then the joints between them (a joint needs both
    /// bodies to exist).
    pub fn spawn(
        physics: &mut dyn PhysicsBackend,
        rig: RobotRig,
        team: usize,
        position: Vec3,
        rotation: Quat,
        mass_scale: f32,
    ) -> Result<Self, BackendError> {
        let bot = Mat4::from_rotation_translation(rotation, position);
        let n = rig.components.len();

        let mut parts = Vec::with_capacity(rig.parts.len());
        for part in &rig.parts {
            let world = bot * rig.components[part.root].local;
            let (_, rot, pos) = world.to_scale_rotation_translation();
            let shape = Self::part_shape(&rig, &part.members);
            let mass = (part
                .members
                .iter()
                .map(|&i| rig.components[i].mass)
                .sum::<f32>()
                * mass_scale)
                .max(0.2);
            let friction = Self::part_friction(&rig, part.members.iter().copied());
            let desc = BodyDesc::new(MotionType::Dynamic, shape)
                .at(pos)
                .with_rotation(rot)
                .with_friction(friction)
                .with_restitution(STOCK_RESTITUTION)
                .with_mass(mass)
                // Driven / hit every step: never let the backend park a body.
                .with_sleeping(false)
                .with_user_data(USER_BOT);
            let body = physics.create_body(&desc)?;
            parts.push(PartState {
                body,
                joint: None,
                alive: true,
            });
        }

        // Joints, now that every body exists.
        for p in 0..rig.parts.len() {
            let (Some(parent), Some(info)) = (rig.parts[p].parent, rig.parts[p].joint) else {
                continue;
            };
            let joint = if info.slider {
                Self::make_slider(physics, &rig, &parts, parent, p, info)?
            } else {
                Self::make_hinge(physics, &rig, &parts, parent, p, info)?
            };
            parts[p].joint = Some(joint);
        }

        Ok(Robot {
            health: rig.components.iter().map(|c| c.hitpoints).collect(),
            attached: vec![true; n],
            parts,
            mass: rig.mass * mass_scale,
            mass_scale,
            destroyed: false,
            team,
            rig,
        })
    }

    /// The compound shape of a body made of `members`.
    fn part_shape(rig: &RobotRig, members: &[usize]) -> ShapeDesc {
        let children: Vec<CompoundChild> = members
            .iter()
            .filter(|&&i| !rig.components[i].hull_points.is_empty())
            .map(|&i| {
                let m = rig.part_local[i];
                let (_, local_rotation, local_position) = m.to_scale_rotation_translation();
                CompoundChild {
                    local_position,
                    local_rotation,
                    shape: ShapeDesc::ConvexHull {
                        points: rig.components[i].hull_points.clone(),
                    },
                    tag: i as u32,
                }
            })
            .collect();
        match children.len() {
            0 => ShapeDesc::Sphere { radius: 0.1 },
            1 => children
                .into_iter()
                .next()
                .map(|c| c.shape)
                .unwrap_or(ShapeDesc::Sphere { radius: 0.1 }),
            _ => ShapeDesc::Compound(children),
        }
    }

    /// Friction of a body: a wheel gets the `grip` its definition declares for the
    /// `contact` surface it rolls on, everything else the stock component value.
    fn part_friction(rig: &RobotRig, members: impl Iterator<Item = usize>) -> f32 {
        let members: Vec<usize> = members.collect();
        members
            .iter()
            .filter_map(|&i| rig.components[i].wheel.as_ref())
            .map(|w| w.grip)
            .next()
            .unwrap_or(STOCK_FRICTION)
    }

    /// Build the hinge for part `p` on `parent`.
    fn make_hinge(
        physics: &mut dyn PhysicsBackend,
        rig: &RobotRig,
        parts: &[PartState],
        parent: usize,
        p: usize,
        info: JointInfo,
    ) -> Result<JointHandle, BackendError> {
        // The joint's pivot and axis are the parent's mount point, in bot space;
        // both bodies express them in their own space (they coincide in the rest
        // pose, which is where the hinge is created).
        let (pivot_bot, axis_bot) = (info.pivot, info.axis);
        let frame_a = rig.components[rig.parts[parent].root].local;
        let frame_b = rig.components[rig.parts[p].root].local;

        let max_torque = match info.use_kind {
            JointUse::Passive => 0.0,
            _ => info.max_torque,
        };
        let desc = HingeDesc {
            body_a: parts[parent].body,
            body_b: parts[p].body,
            anchor_a: frame_a.inverse().transform_point3(pivot_bot),
            anchor_b: frame_b.inverse().transform_point3(pivot_bot),
            axis_a: frame_a.inverse().transform_vector3(axis_bot),
            axis_b: frame_b.inverse().transform_vector3(axis_bot),
            limits: info.limits,
            max_torque,
            friction_torque: info.friction_torque,
        };
        physics.create_hinge(&desc)
    }

    /// The same placement rule as a hinge, on a prismatic joint (F8): a piston's
    /// rod slides along the axis its mount point gives, with the travel its
    /// definition asks for. The motor's "torque" is a force here.
    fn make_slider(
        physics: &mut dyn PhysicsBackend,
        rig: &RobotRig,
        parts: &[PartState],
        parent: usize,
        p: usize,
        info: JointInfo,
    ) -> Result<JointHandle, BackendError> {
        let (pivot_bot, axis_bot) = (info.pivot, info.axis);
        let frame_a = rig.components[rig.parts[parent].root].local;
        let frame_b = rig.components[rig.parts[p].root].local;

        let max_force = match info.use_kind {
            JointUse::Passive => 0.0,
            _ => info.max_torque,
        };
        let desc = SliderDesc {
            body_a: parts[parent].body,
            body_b: parts[p].body,
            anchor_a: frame_a.inverse().transform_point3(pivot_bot),
            anchor_b: frame_b.inverse().transform_point3(pivot_bot),
            axis_a: frame_a.inverse().transform_vector3(axis_bot),
            axis_b: frame_b.inverse().transform_vector3(axis_bot),
            limits: info.limits,
            max_force,
            friction_force: info.friction_torque,
        };
        physics.create_slider(&desc)
    }

    /// The bot's world frame (derived from the chassis body).
    pub fn transform(&self, physics: &dyn PhysicsBackend) -> Mat4 {
        let root = self.rig.components[self.rig.parts[0].root].local;
        self.part_world(physics, 0) * root.inverse()
    }

    /// World transform of one assembled body.
    fn part_world(&self, physics: &dyn PhysicsBackend, part: usize) -> Mat4 {
        match physics.state(self.parts[part].body) {
            Some(s) => Mat4::from_rotation_translation(s.rotation, s.position),
            None => Mat4::IDENTITY,
        }
    }

    /// World transform of every component, in component index order. A body that
    /// has been shot off keeps its last-known frame.
    pub fn component_worlds(&self, physics: &dyn PhysicsBackend) -> Vec<Mat4> {
        let part_worlds: Vec<Mat4> = (0..self.rig.parts.len())
            .map(|p| self.part_world(physics, p))
            .collect();
        self.rig
            .part_of
            .iter()
            .enumerate()
            .map(|(i, &p)| part_worlds[p] * self.rig.part_local[i])
            .collect()
    }

    pub fn position(&self, physics: &dyn PhysicsBackend) -> Vec3 {
        self.transform(physics).w_axis.truncate()
    }

    pub fn velocity(&self, physics: &dyn PhysicsBackend) -> Vec3 {
        physics
            .state(self.parts[0].body)
            .map(|s| s.linear_velocity)
            .unwrap_or(Vec3::ZERO)
    }

    /// The chassis body — the one the fight revolves around.
    pub fn body(&self) -> BodyHandle {
        self.parts[0].body
    }

    /// Whether the robot can still fight.
    pub fn is_alive(&self) -> bool {
        self.parts[0].alive && !self.destroyed
    }

    /// Drive the motors: wheels from the input, spinners at their rated speed.
    ///
    /// `input.throttle` is a signed fraction of the *motor's* speed: each wheel's
    /// target is its own motor's rated angular velocity (the definition's `power`
    /// speed, in rad/s), so a robot's top speed is a property of its parts, not a
    /// global. `input.steer` adds a yaw rate to the desired hub velocity, which
    /// makes the two sides differ — a tank-style differential. The original
    /// steers through its own `Steering` components and the control wiring, which
    /// is not modelled yet.
    pub fn drive(
        &mut self,
        physics: &mut dyn PhysicsBackend,
        input: DriveInput,
        tuning: &DriveTuning,
    ) {
        let throttle = input.throttle.clamp(-1.0, 1.0);
        let yaw = -input.steer.clamp(-1.0, 1.0) * tuning.max_yaw_rate;
        let forward = Vec3::new(0.0, 0.0, -1.0);
        let yaw_axis = Vec3::Y;

        for p in 0..self.rig.parts.len() {
            if !self.parts[p].alive {
                continue;
            }
            let (Some(info), Some(joint)) = (self.rig.parts[p].joint, self.parts[p].joint) else {
                continue;
            };
            let target = match info.use_kind {
                JointUse::Spun => info.rate,
                // Parked until something triggers it (see `JointUse::Burst`);
                // its hold is the joint's friction, not a motor. A servo piston
                // is parked the same way until the control layer moves it (F8).
                JointUse::Burst | JointUse::Passive | JointUse::Piston => continue,
                JointUse::Wheel => {
                    let root = self.rig.parts[p].root;
                    let c = &self.rig.components[root];
                    // The wheel rolls perpendicular to the joint axis.
                    let roll = info.axis.cross(Vec3::Y).normalize_or_zero();
                    if roll == Vec3::ZERO {
                        continue;
                    }
                    // The front of the vehicle moves at whatever the wheel's own
                    // motor can spin it to.
                    let v = throttle * info.rate * c.wheel_radius();
                    let hub = c.local.transform_point3(Vec3::ZERO);
                    let v_hub = forward * v + yaw_axis.cross(hub) * yaw;
                    v_hub.dot(roll) / c.wheel_radius().max(0.05)
                }
            };
            if info.slider {
                physics.drive_slider(joint, target);
            } else {
                physics.drive_hinge(joint, target);
            }
        }
    }

    /// Cave the chassis in as it takes damage (F9).
    ///
    /// The bot file marks which points of the chassis model the game may move:
    /// it subdivides the sides and top, and "every hit then will shift these
    /// 'buckling' points down and in, causing the sides of the robot to buckle"
    /// (`tools/vendor/PyRA2/.../Bot file format analysis.txt` §"3D model of bot
    /// with buckling points"). Until now a chassis only ever died whole.
    ///
    /// Both the drawn chassis and its collider follow, so a hurt robot is
    /// crushed *and* smaller to hit. The movable region is the file's own: the
    /// lowest buckle point pins the floor plate (the Octagon-sized robots keep
    /// everything below the mid-height rigid), and the motion grows with height
    /// above it, so the top caves in furthest.
    ///
    /// The direction is the file's ("down and in"); the *rate* is ours — linear
    /// in the damage the chassis has taken, reaching half its height and half
    /// its footprint as the last hitpoint goes. Nothing in the shipped bots (all
    /// 57 are uncrushed: their mesh footprint equals the bottom plate) or in the
    /// binary's strings gives the original's curve.
    pub fn buckle(&mut self, physics: &mut dyn PhysicsBackend) {
        const DOWN: f32 = 0.5;
        const IN: f32 = 0.5;
        if self.health.is_empty() || self.rig.components[0].buckle.is_empty() {
            return;
        }
        let max_hp = self.rig.components[0].hitpoints.max(1.0);
        let fraction = (1.0 - self.health[0] / max_hp).clamp(0.0, 1.0);
        let c = &mut self.rig.components[0];
        let step = fraction - c.buckle_fraction;
        if step <= 1e-4 {
            return;
        }
        c.buckle_fraction = fraction;

        let height = (c.max.y - c.min.y).max(1e-3);
        let floor = c
            .buckle
            .iter()
            .map(|p| p.y)
            .fold(f32::MAX, f32::min)
            .min(c.max.y);
        let span = (c.max.y - floor).max(1e-3);
        let crush = |p: &mut Vec3| {
            let w = ((p.y - floor) / span).clamp(0.0, 1.0);
            if w <= 0.0 {
                return;
            }
            p.y -= step * DOWN * height * w;
            // "In": toward the chassis' own centre line, which in component
            // space is the origin's XZ.
            let shrink = 1.0 - step * IN * w;
            p.x *= shrink;
            p.z *= shrink;
        };

        for item in c.items.iter_mut() {
            for v in item.vertices.iter_mut() {
                let mut p = Vec3::from(v.pos);
                crush(&mut p);
                v.pos = p.into();
            }
        }
        for p in c.hull_points.iter_mut() {
            crush(p);
        }
        self.rebuild_part(physics, 0);
    }

    /// Apply damage to a component; returns `true` if it just broke off.
    pub fn damage(&mut self, index: usize, amount: f32) -> bool {
        if index >= self.health.len() || !self.attached[index] {
            return false;
        }
        if !self.rig.components[index].damageable {
            return false;
        }
        self.health[index] -= amount;
        if self.health[index] <= 0.0 {
            self.health[index] = 0.0;
            return true;
        }
        false
    }

    /// Break `component` (and anything welded past it) off the robot.
    ///
    /// Returns each detached component with the world transform it had at the
    /// moment it came away, so the caller can spawn debris.
    pub fn detach(
        &mut self,
        physics: &mut dyn PhysicsBackend,
        component: usize,
    ) -> Vec<(usize, Mat4)> {
        let mut out = Vec::new();
        if component >= self.attached.len() || !self.attached[component] {
            return out;
        }
        let part = self.rig.part_of[component];
        if !self.parts[part].alive {
            return out;
        }
        let worlds = self.component_worlds(physics);
        let root = self.rig.parts[part].root;

        // What comes away with it: itself plus its welded descendants.
        let members = self.rig.parts[part].members.clone();
        let removed: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&i| {
                let mut j = i;
                loop {
                    if j == component {
                        return true;
                    }
                    if j == root {
                        return false;
                    }
                    match self.rig.components[j].parent {
                        Some(p) if self.rig.part_of[p] == part => j = p,
                        _ => return false,
                    }
                }
            })
            .collect();
        if removed.is_empty() {
            return out;
        }

        if removed.len() == members.len() {
            self.remove_part(physics, part, &worlds, &mut out);
        } else {
            for &i in &removed {
                self.attached[i] = false;
                self.health[i] = 0.0;
                out.push((i, worlds[i]));
            }
            self.rebuild_part(physics, part);
        }
        out
    }

    /// Remove a whole body (and its children, whose joints die with it).
    fn remove_part(
        &mut self,
        physics: &mut dyn PhysicsBackend,
        part: usize,
        worlds: &[Mat4],
        out: &mut Vec<(usize, Mat4)>,
    ) {
        if !self.parts[part].alive {
            return;
        }
        let children: Vec<usize> = (0..self.rig.parts.len())
            .filter(|&q| self.rig.parts[q].parent == Some(part))
            .collect();
        for q in children {
            self.remove_part(physics, q, worlds, out);
        }
        if let Some(j) = self.parts[part].joint.take() {
            physics.remove_joint(j);
        }
        physics.remove_body(self.parts[part].body);
        self.parts[part].alive = false;
        for &i in &self.rig.parts[part].members {
            if self.attached[i] {
                self.attached[i] = false;
                self.health[i] = 0.0;
                out.push((i, worlds[i]));
            }
        }
    }

    /// Re-shape a body after some of its members came off. The body keeps its id,
    /// pose and joints, so nothing else has to be rebuilt.
    fn rebuild_part(&mut self, physics: &mut dyn PhysicsBackend, part: usize) {
        let members: Vec<usize> = self.rig.parts[part]
            .members
            .iter()
            .copied()
            .filter(|&i| self.attached[i])
            .collect();
        let shape = Self::part_shape(&self.rig, &members);
        let mass = (members
            .iter()
            .map(|&i| self.rig.components[i].mass)
            .sum::<f32>()
            * self.mass_scale)
            .max(0.2);
        if let Err(e) = physics.set_body_shape(self.parts[part].body, &shape, Some(mass)) {
            eprintln!("ra2sim: rebuild failed: {e}");
        }
    }

    /// The mass the chassis body itself carries.
    ///
    /// The original is one rigid body per robot, so a force on the robot is a
    /// force on all of it; ours is a body per component, and the chassis is the
    /// one the fight revolves around. Anything that pushes the *robot* has to go
    /// through this ratio to accelerate it as one thing.
    pub fn chassis_mass(&self) -> f32 {
        // The same sum `new` gives the body: a part's own mass is its members'.
        (self.rig.parts[0]
            .members
            .iter()
            .map(|&i| self.rig.components[i].mass)
            .sum::<f32>()
            * self.mass_scale)
            .max(0.2)
    }

    /// Mass of the components still attached.
    pub fn remaining_mass(&self) -> f32 {
        self.rig
            .components
            .iter()
            .enumerate()
            .filter(|(i, _)| self.attached[*i])
            .map(|(_, c)| c.mass)
            .sum::<f32>()
            * self.mass_scale
    }

    /// World-space centre and radius of an attached component.
    pub fn component_sphere(&self, worlds: &[Mat4], index: usize) -> (Vec3, f32) {
        let c = &self.rig.components[index];
        (worlds[index].transform_point3(c.center()), c.radius())
    }
}
