//! A simulated robot: its rig plus live physics, motor spin and damage state.

use std::collections::HashSet;

use glam::{Mat4, Quat, Vec3};

use crate::backend::{BackendError, BodyDesc, BodyHandle, MotionType, PhysicsBackend, ShapeDesc};
use crate::rig::RobotRig;
use crate::scene::USER_BOT;

/// One robot in the world.
pub struct Robot {
    pub rig: RobotRig,
    pub team: usize,
    pub body: BodyHandle,
    /// Accumulated rotor angle per component (only motors use it).
    pub thetas: Vec<f32>,
    /// Remaining hitpoints per component.
    pub health: Vec<f32>,
    /// Whether each component is still bolted to the chassis.
    pub attached: Vec<bool>,
    /// Convex-hull input for the current (attached) body, bot-local.
    pub hull_points: Vec<Vec3>,
    /// Radius of the robot's own hull (for contact-distance estimates).
    pub hull_radius: f32,
    pub mass: f32,
    /// Whether the robot can still fight.
    pub destroyed: bool,
}

/// Collapse a point cloud onto a ~2 cm grid (keeps hull construction fast).
fn thin(points: &[Vec3]) -> Vec<Vec3> {
    let mut seen: HashSet<(i32, i32, i32)> = HashSet::new();
    let mut out = Vec::new();
    for p in points {
        let key = (
            (p.x * 50.0).round() as i32,
            (p.y * 50.0).round() as i32,
            (p.z * 50.0).round() as i32,
        );
        if seen.insert(key) {
            out.push(*p);
        }
    }
    out
}

impl Robot {
    /// Spawn a robot at `position`/`rotation`, keeping `mass_scale * rig.mass` kg.
    pub fn spawn(
        physics: &mut dyn PhysicsBackend,
        rig: RobotRig,
        team: usize,
        position: Vec3,
        rotation: Quat,
        mass_scale: f32,
    ) -> Result<Self, BackendError> {
        let n = rig.components.len();
        let hull_points = Self::hull_of(&rig, &vec![true; n]);
        let hull_radius = hull_points.iter().fold(0.0f32, |m, p| m.max(p.length()));
        let mass = (rig.mass * mass_scale).max(1.0);

        let desc = BodyDesc::new(
            MotionType::Dynamic,
            ShapeDesc::ConvexHull {
                points: hull_points.clone(),
            },
        )
        .at(position)
        .with_rotation(rotation)
        // Stock RA2 component physics: friction 0.30, elasticity 0.30, and no
        // drag at all (Havok 1.x drag is a world setting that RA2 leaves at 0).
        .with_friction(0.30)
        .with_restitution(0.30)
        .with_mass(mass)
        // Driven every step: never let the backend park it.
        .with_sleeping(false)
        .with_user_data(USER_BOT);
        let body = physics.create_body(&desc)?;

        Ok(Robot {
            health: rig.components.iter().map(|c| c.hitpoints).collect(),
            thetas: vec![0.0; n],
            attached: vec![true; n],
            hull_points,
            hull_radius,
            mass,
            destroyed: false,
            body,
            team,
            rig,
        })
    }

    /// Convex-hull input for the components still attached.
    pub fn hull_of(rig: &RobotRig, attached: &[bool]) -> Vec<Vec3> {
        let mut points = Vec::new();
        for (i, c) in rig.components.iter().enumerate() {
            if !attached.get(i).copied().unwrap_or(false) || c.hull_points.is_empty() {
                continue;
            }
            for p in &c.hull_points {
                points.push(c.local.transform_point3(*p));
            }
        }
        thin(&points)
    }

    /// Rebuild the collider after parts have been knocked off. Preserves the
    /// body's pose and velocity.
    pub fn rebuild(&mut self, physics: &mut dyn PhysicsBackend) -> Result<(), BackendError> {
        let state = physics.state(self.body);
        physics.remove_body(self.body);

        self.hull_points = Self::hull_of(&self.rig, &self.attached);
        self.hull_radius = self
            .hull_points
            .iter()
            .fold(0.0f32, |m, p| m.max(p.length()));
        let (position, rotation, lin, ang) = match state {
            Some(s) => (
                s.position,
                s.rotation,
                s.linear_velocity,
                s.angular_velocity,
            ),
            None => (Vec3::ZERO, Quat::IDENTITY, Vec3::ZERO, Vec3::ZERO),
        };
        let desc = BodyDesc::new(
            MotionType::Dynamic,
            ShapeDesc::ConvexHull {
                points: self.hull_points.clone(),
            },
        )
        .at(position)
        .with_rotation(rotation)
        .with_friction(0.30)
        .with_restitution(0.30)
        .with_mass(self.remaining_mass().max(1.0))
        // Driven every step: never let the backend park it.
        .with_sleeping(false)
        .with_user_data(USER_BOT);
        self.body = physics.create_body(&desc)?;
        physics.set_velocity(self.body, lin, ang);
        Ok(())
    }

    /// Mass of the parts still attached.
    pub fn remaining_mass(&self) -> f32 {
        let base: f32 = self
            .rig
            .components
            .iter()
            .map(|c| c.mass)
            .sum::<f32>()
            .max(0.001);
        let left: f32 = self
            .rig
            .components
            .iter()
            .enumerate()
            .filter(|(i, _)| self.attached[*i])
            .map(|(_, c)| c.mass)
            .sum();
        self.mass * (left / base).clamp(0.05, 1.0)
    }

    pub fn transform(&self, physics: &dyn PhysicsBackend) -> Mat4 {
        match physics.state(self.body) {
            Some(s) => Mat4::from_rotation_translation(s.rotation, s.position),
            None => Mat4::IDENTITY,
        }
    }

    pub fn position(&self, physics: &dyn PhysicsBackend) -> Vec3 {
        physics
            .state(self.body)
            .map(|s| s.position)
            .unwrap_or(Vec3::ZERO)
    }

    pub fn velocity(&self, physics: &dyn PhysicsBackend) -> Vec3 {
        physics
            .state(self.body)
            .map(|s| s.linear_velocity)
            .unwrap_or(Vec3::ZERO)
    }

    /// Animated bot-local transform of every component.
    pub fn worlds(&self) -> Vec<Mat4> {
        self.rig.worlds(&self.thetas)
    }

    /// World transform of a component.
    pub fn world_of(&self, worlds: &[Mat4], body: Mat4, index: usize) -> Mat4 {
        body * worlds[index]
    }

    /// Advance every motor's rotor angle.
    pub fn advance_motors(&mut self, dt: f32) {
        for i in 0..self.rig.components.len() {
            if !self.attached[i] {
                continue;
            }
            if let Some(m) = self.rig.components[i].motor {
                self.thetas[i] += m.rate * dt;
            }
        }
    }

    /// Apply damage to a component; returns `true` if it just broke off.
    pub fn damage(&mut self, index: usize, amount: f32) -> bool {
        if index >= self.health.len() || !self.attached[index] {
            return false;
        }
        let c = &self.rig.components[index];
        if !c.damageable {
            return false;
        }
        self.health[index] -= amount;
        if self.health[index] <= 0.0 {
            self.health[index] = 0.0;
            return true;
        }
        false
    }

    /// Mark a component and its whole subtree as detached. Returns the indices.
    pub fn detach(&mut self, index: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = vec![index];
        while let Some(i) = stack.pop() {
            if !self.attached.get(i).copied().unwrap_or(false) {
                continue;
            }
            self.attached[i] = false;
            out.push(i);
            stack.extend(self.rig.components[i].children.iter().copied());
        }
        out
    }

    /// True while the chassis survives.
    pub fn is_alive(&self) -> bool {
        self.attached[0] && !self.destroyed
    }

    /// World-space centre and radius of an attached component.
    pub fn component_sphere(&self, worlds: &[Mat4], body: Mat4, index: usize) -> (Vec3, f32) {
        let c = &self.rig.components[index];
        let w = body * worlds[index];
        (w.transform_point3(c.center()), c.radius())
    }
}
