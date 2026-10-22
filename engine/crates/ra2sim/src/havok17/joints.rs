//! Havok 1.x's constraint solver for the joints Robot Arena 2 uses: the hinge
//! (type 2, every motor and axle), the prismatic (type 4, pistons and hazards)
//! and the point-to-point (type 1).
//!
//! It runs as an action, after integration and collision detection
//! (`FUN_00523bd0` → `FUN_00534480`, "Actions/Constraint"). Every constraint is
//! first *set up* (vtable `+0x24`; the hinge's is `FUN_00535a50`): its motor and
//! limits act once as a single impulse about the free axis, then the locked
//! directions are solved as one dense block. Then four solve passes
//! (`+0x28`, `FUN_005361e0`) re-solve every block Gauss-Seidel style with the
//! pass factors `(tau, damping) = (1, 1), (1, 1), (1, 1), (0.8, 0.8)`. A block's
//! right-hand side is `tau · error / dt - damping · J v` and its impulse
//! `λ = K⁻¹ b` (dense LDL, `FUN_00556380`).
//!
//! The motor is a position servo (`0x535a50`): the target angle advances by
//! `dt · targetVelocity` per call, the impulse is
//! `m_eff (0.8 · err / dt - ω_rel)` (`DAT_006ab658`), capped at
//! `maxTorque · dt`, and when capped the target slips back so the error cannot
//! wind up. `dt` is the substep (`world + 0x6c`), never a backstepped piece.

use glam::{Quat, Vec3};

use super::body::Body;

/// The servo's position gain (`DAT_006ab658`).
pub const SERVO_GAIN: f32 = 0.8;
/// The solve passes' `(tau, damping)` (`FUN_00534480`).
pub const PASSES: [(f32, f32); 4] = [(1.0, 1.0), (1.0, 1.0), (1.0, 1.0), (0.8, 0.8)];

/// What kind of joint, with its own state.
#[derive(Clone, Debug)]
pub enum Kind {
    Hinge {
        /// Free axis in each body's frame.
        axis_a: Vec3,
        axis_b: Vec3,
        /// A direction perpendicular to the axis in each body's frame; the
        /// two coincide in world space at the rest pose, which is angle 0.
        ref_a: Vec3,
        ref_b: Vec3,
    },
    Slider {
        axis_a: Vec3,
        /// B's orientation relative to A at the rest pose.
        rest: Quat,
    },
    PointToPoint,
}

/// One joint between bodies `a` and `b`.
#[derive(Clone, Debug)]
pub struct Joint {
    pub alive: bool,
    pub a: usize,
    pub b: usize,
    /// The shared point in each body's own frame, from its origin.
    pub anchor_a: Vec3,
    pub anchor_b: Vec3,
    pub kind: Kind,
    /// Travel limits (radians for a hinge, metres for a slider), measured from
    /// the rest pose; `limited` is the hinge's `*IS_LIMITED`.
    pub limits: [f32; 2],
    pub limited: bool,
    /// Motor block (`hinge + 0x90`): on/off, cap, target velocity, and the
    /// solver atom's target angle (`atom + 0x98`).
    pub motor: bool,
    pub max_torque: f32,
    pub target_velocity: f32,
    pub target: f32,
    /// Ours: what the joint resists an undriven motion with (N·m or N). The
    /// game parks a motor with `Lock` (`FUN_00480e20`); this holds it the way
    /// the Jolt backend's motor friction does.
    pub friction: f32,
    /// The game's `Lock` (`FUN_00480e20` / `FUN_004b50c0`): while set, the
    /// limits and `limited` flag the joint had before it was locked; the
    /// joint's own limits are then both its position at the lock.
    pub saved_limits: Option<([f32; 2], bool)>,
    /// A steered hinge's free axis and reference in A's frame as built
    /// (Havok's wheel constraint keeps the unsteered axle, `+0xb8`, and
    /// rebuilds the steered one from it, `FUN_00553af0`).
    pub steer_rest: Option<(Vec3, Vec3)>,
}

/// A row of a block: the Jacobian against each body's linear and angular
/// velocity, and the position error it corrects.
#[derive(Clone, Copy, Default)]
struct Row {
    lin_a: Vec3,
    ang_a: Vec3,
    lin_b: Vec3,
    ang_b: Vec3,
    error: f32,
}

pub(crate) fn wrap(angle: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let a = (angle + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
    if a.is_finite() {
        a
    } else {
        0.0
    }
}

/// A unit vector perpendicular to `v`.
pub fn perpendicular(v: Vec3) -> Vec3 {
    let other = if v.x.abs() < 0.6 { Vec3::X } else { Vec3::Y };
    v.cross(other).normalize_or_zero()
}

impl Joint {
    fn anchors(&self, bodies: &[Body]) -> (Vec3, Vec3) {
        let (a, b) = (&bodies[self.a], &bodies[self.b]);
        (
            a.origin() + a.rotation * self.anchor_a,
            b.origin() + b.rotation * self.anchor_b,
        )
    }

    /// The hinge angle (B relative to A about A's axis) or the slider offset.
    pub fn position(&self, bodies: &[Body]) -> f32 {
        let (a, b) = (&bodies[self.a], &bodies[self.b]);
        match &self.kind {
            Kind::Hinge {
                axis_a,
                ref_a,
                ref_b,
                ..
            } => {
                let axis = a.rotation * *axis_a;
                let (ra, rb) = (a.rotation * *ref_a, b.rotation * *ref_b);
                ra.cross(rb).dot(axis).atan2(ra.dot(rb))
            }
            Kind::Slider { axis_a, .. } => {
                let (pa, pb) = self.anchors(bodies);
                (pb - pa).dot(a.rotation * *axis_a)
            }
            Kind::PointToPoint => 0.0,
        }
    }

    /// The free direction in world space, and whether it is angular.
    fn free_axis(&self, bodies: &[Body]) -> Option<(Vec3, bool)> {
        let a = &bodies[self.a];
        match &self.kind {
            Kind::Hinge { axis_a, .. } => Some(((a.rotation * *axis_a).normalize(), true)),
            Kind::Slider { axis_a, .. } => Some(((a.rotation * *axis_a).normalize(), false)),
            Kind::PointToPoint => None,
        }
    }

    /// The locked directions as rows.
    fn rows(&self, bodies: &[Body]) -> Vec<Row> {
        let (a, b) = (&bodies[self.a], &bodies[self.b]);
        let (pa, pb) = self.anchors(bodies);
        let (ra, rb) = (pa - a.com, pb - b.com);
        let linear = |dir: Vec3, error: f32| Row {
            lin_a: -dir,
            ang_a: -ra.cross(dir),
            lin_b: dir,
            ang_b: rb.cross(dir),
            error,
        };
        let angular = |dir: Vec3, error: f32| Row {
            lin_a: Vec3::ZERO,
            ang_a: -dir,
            lin_b: Vec3::ZERO,
            ang_b: dir,
            error,
        };
        let gap = pb - pa;
        match &self.kind {
            Kind::Hinge { axis_a, axis_b, .. } => {
                let axis = (a.rotation * *axis_a).normalize();
                let along_b = (b.rotation * *axis_b).normalize();
                let b1 = perpendicular(along_b);
                let b2 = along_b.cross(b1);
                let mut rows: Vec<Row> = [Vec3::X, Vec3::Y, Vec3::Z]
                    .into_iter()
                    .map(|e| linear(e, gap.dot(e)))
                    .collect();
                // C = a · b_k, dC/dt = (ωB - ωA) · (b_k × a).
                for bk in [b1, b2] {
                    let dir = bk.cross(axis);
                    rows.push(angular(dir, axis.dot(bk)));
                }
                rows
            }
            Kind::Slider { axis_a, rest } => {
                let axis = (a.rotation * *axis_a).normalize();
                let t1 = perpendicular(axis);
                let t2 = axis.cross(t1);
                // Orientation error: the small rotation that takes B from where
                // the rest pose puts it to where it is.
                let want = a.rotation * *rest;
                let delta = b.rotation * want.inverse();
                let delta = if delta.w < 0.0 { -delta } else { delta };
                let err = Vec3::new(delta.x, delta.y, delta.z) * 2.0;
                let mut rows: Vec<Row> = [Vec3::X, Vec3::Y, Vec3::Z]
                    .into_iter()
                    .map(|e| angular(e, err.dot(e)))
                    .collect();
                rows.push(linear(t1, gap.dot(t1)));
                rows.push(linear(t2, gap.dot(t2)));
                rows
            }
            Kind::PointToPoint => [Vec3::X, Vec3::Y, Vec3::Z]
                .into_iter()
                .map(|e| linear(e, gap.dot(e)))
                .collect(),
        }
    }

    /// The motor and limit impulse about (or along) the free axis, once per
    /// solver call (`0x535a50`).
    fn drive(&mut self, bodies: &mut [Body], dt: f32) {
        let Some((axis, angular)) = self.free_axis(bodies) else {
            return;
        };
        if !self.motor && !self.limited && self.friction <= 0.0 {
            return;
        }
        let inv_dt = 1.0 / dt;
        let (a, b) = (&bodies[self.a], &bodies[self.b]);
        let (pa, pb) = self.anchors(bodies);
        let (ra, rb) = (pa - a.com, pb - b.com);
        // k: the free direction's own response; m = 1/k.
        let (k, rel) = if angular {
            (
                axis.dot(a.inv_inertia_world * axis) * a.is_dynamic() as i32 as f32
                    + axis.dot(b.inv_inertia_world * axis) * b.is_dynamic() as i32 as f32,
                axis.dot(b.angular_velocity - a.angular_velocity),
            )
        } else {
            let part = |body: &Body, r: Vec3| {
                if body.is_dynamic() {
                    body.inv_mass + r.cross(axis).dot(body.inv_inertia_world * r.cross(axis))
                } else {
                    0.0
                }
            };
            (
                part(a, ra) + part(b, rb),
                axis.dot(b.point_velocity(pb) - a.point_velocity(pa)),
            )
        };
        if k <= 1e-12 {
            return;
        }
        let m = 1.0 / k;
        // The measured angle, in (-π, π]: what the limits are checked against.
        // The servo measures its error the short way round (a target that has
        // run more than half a turn ahead is a whole turn behind), but that
        // turn is the target's, not the hinge's: a burst motor's target runs
        // on at its speed past the stop it never reaches within the lock
        // tolerance (Sentinel's wedge, 50 rad/s against a 1.67 rad travel),
        // and moving the angle with it put the hinge a turn below its lower
        // limit, whose correction then spun the wedge round at 160 rad/s
        // (docs/29 section 12l).
        let position = self.position(bodies);
        let mut lambda = 0.0f32;
        if self.motor {
            let mut err = self.target - position;
            if angular {
                if err > std::f32::consts::PI {
                    err -= std::f32::consts::TAU;
                }
                if err < -std::f32::consts::PI {
                    err += std::f32::consts::TAU;
                }
            }
            let next = self.target + dt * self.target_velocity;
            lambda = (SERVO_GAIN * inv_dt * err - rel) * m;
            self.target = next;
            if inv_dt * lambda.abs() > self.max_torque {
                let s = dt / lambda.abs() * self.max_torque;
                lambda *= s;
                self.target = next - (1.0 - s) * err;
            }
            if angular {
                self.target = wrap(self.target);
            }
        } else if self.friction > 0.0 {
            // Ours: a parked joint resists its own motion up to its friction.
            lambda = (-rel * m).clamp(-self.friction * dt, self.friction * dt);
        }
        if self.limited && self.saved_limits.is_some() && angular {
            // Locked at one angle: measure the miss the short way round, so a
            // hinge locked near ±π does not see a full turn of error.
            let bound = self.limits[0];
            let miss = wrap((lambda * k + rel) * dt + position - bound);
            lambda -= miss * inv_dt * m;
        } else if self.limited {
            let predicted = (lambda * k + rel) * dt + position;
            let bound = if predicted > self.limits[1] {
                Some(self.limits[1])
            } else if predicted < self.limits[0] {
                Some(self.limits[0])
            } else {
                None
            };
            if let Some(bound) = bound {
                lambda -= (predicted - bound) * inv_dt * m;
            }
        }
        if lambda == 0.0 {
            return;
        }
        if angular {
            bodies[self.b].apply_angular_impulse(axis * lambda);
            bodies[self.a].apply_angular_impulse(-axis * lambda);
        } else {
            bodies[self.b].apply_impulse_at(axis * lambda, pb);
            bodies[self.a].apply_impulse_at(-axis * lambda, pa);
        }
    }

    /// A slider's limit again, in each solve pass, before its block
    /// (`0x555960` → `0x555cc0`, the prismatic atom's pass, vtable
    /// `0x6ac524 + 0x28`): with the pass's `tau`, an impulse along the axis
    /// that brings the predicted position `position + rel · dt` back onto the
    /// stop it would pass, `-(predicted - bound) / dt · tau · m` (the atom's
    /// limit factor `+0x30` is 1). The hinge's pass (`0x5361e0`) has no such
    /// step: its limit acts once, at the set-up. The pass also re-runs the
    /// atom's motor, which our sliders do not use (the game drives a piston's
    /// rod and the hazards' sliders with forces, docs/29 section 12r).
    fn limit_pass(&self, bodies: &mut [Body], dt: f32, tau: f32) {
        if !self.limited || !matches!(self.kind, Kind::Slider { .. }) {
            return;
        }
        let Some((axis, false)) = self.free_axis(bodies) else {
            return;
        };
        let (a, b) = (&bodies[self.a], &bodies[self.b]);
        let (pa, pb) = self.anchors(bodies);
        let (ra, rb) = (pa - a.com, pb - b.com);
        let part = |body: &Body, r: Vec3| {
            if body.is_dynamic() {
                body.inv_mass + r.cross(axis).dot(body.inv_inertia_world * r.cross(axis))
            } else {
                0.0
            }
        };
        let k = part(a, ra) + part(b, rb);
        if k <= 1e-12 {
            return;
        }
        let rel = axis.dot(b.point_velocity(pb) - a.point_velocity(pa));
        let predicted = rel * dt + self.position(bodies);
        let bound = if predicted > self.limits[1] {
            self.limits[1]
        } else if predicted < self.limits[0] {
            self.limits[0]
        } else {
            return;
        };
        let lambda = -(predicted - bound) / dt * tau / k;
        bodies[self.b].apply_impulse_at(axis * lambda, pb);
        bodies[self.a].apply_impulse_at(-axis * lambda, pa);
    }

    /// Solve the locked block once with the pass factors.
    fn solve_block(&self, bodies: &mut [Body], rows: &[Row], dt: f32, tau: f32, damping: f32) {
        let (a, b) = (&bodies[self.a], &bodies[self.b]);
        let (da, db) = (a.is_dynamic(), b.is_dynamic());
        if !da && !db {
            return;
        }
        let n = rows.len();
        // M⁻¹ Jᵀ for every row, then K = J M⁻¹ Jᵀ.
        let response = |r: &Row| {
            let (la, aa) = if da {
                (r.lin_a * a.inv_mass, a.inv_inertia_world * r.ang_a)
            } else {
                (Vec3::ZERO, Vec3::ZERO)
            };
            let (lb, ab) = if db {
                (r.lin_b * b.inv_mass, b.inv_inertia_world * r.ang_b)
            } else {
                (Vec3::ZERO, Vec3::ZERO)
            };
            (la, aa, lb, ab)
        };
        let resp: Vec<_> = rows.iter().map(response).collect();
        let mut k = [[0.0f32; 6]; 6];
        let mut rhs = [0.0f32; 6];
        for i in 0..n {
            for j in 0..n {
                let (la, aa, lb, ab) = resp[j];
                k[i][j] = rows[i].lin_a.dot(la)
                    + rows[i].ang_a.dot(aa)
                    + rows[i].lin_b.dot(lb)
                    + rows[i].ang_b.dot(ab);
            }
            let r = &rows[i];
            let jv = r.lin_a.dot(a.velocity)
                + r.ang_a.dot(a.angular_velocity)
                + r.lin_b.dot(b.velocity)
                + r.ang_b.dot(b.angular_velocity);
            rhs[i] = -tau * r.error / dt - damping * jv;
        }
        let Some(lambda) = solve_dense(&mut k, &mut rhs, n) else {
            return;
        };
        let (mut dva, mut dwa, mut dvb, mut dwb) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);
        for (i, (la, aa, lb, ab)) in resp.iter().enumerate() {
            dva += *la * lambda[i];
            dwa += *aa * lambda[i];
            dvb += *lb * lambda[i];
            dwb += *ab * lambda[i];
        }
        if da {
            bodies[self.a].velocity += dva;
            bodies[self.a].angular_velocity += dwa;
        }
        if db {
            bodies[self.b].velocity += dvb;
            bodies[self.b].angular_velocity += dwb;
        }
    }
}

/// Gaussian elimination with partial pivoting on the leading `n×n` block.
fn solve_dense(k: &mut [[f32; 6]; 6], rhs: &mut [f32; 6], n: usize) -> Option<[f32; 6]> {
    for col in 0..n {
        let pivot = (col..n).max_by(|&x, &y| k[x][col].abs().total_cmp(&k[y][col].abs()))?;
        if k[pivot][col].abs() < 1e-12 {
            return None;
        }
        k.swap(col, pivot);
        rhs.swap(col, pivot);
        for row in col + 1..n {
            let f = k[row][col] / k[col][col];
            if f != 0.0 {
                let pivot_row = k[col];
                for (dst, src) in k[row][col..n].iter_mut().zip(&pivot_row[col..n]) {
                    *dst -= f * src;
                }
                rhs[row] -= f * rhs[col];
            }
        }
    }
    let mut x = [0.0f32; 6];
    for row in (0..n).rev() {
        let mut s = rhs[row];
        for c in row + 1..n {
            s -= k[row][c] * x[c];
        }
        x[row] = s / k[row][row];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// One constraint-solver action: set up every joint (motor, limits, one block
/// solve), then the four passes. `dt` is the substep length.
pub fn solve(bodies: &mut [Body], joints: &mut [Joint], dt: f32) {
    let live = |j: &Joint, bodies: &[Body]| {
        j.alive
            && bodies[j.a].alive
            && bodies[j.b].alive
            && (bodies[j.a].is_dynamic() || bodies[j.b].is_dynamic())
    };
    let mut blocks: Vec<(usize, Vec<Row>)> = Vec::new();
    for (i, joint) in joints.iter_mut().enumerate() {
        if !live(joint, bodies) {
            continue;
        }
        joint.drive(bodies, dt);
        let rows = joint.rows(bodies);
        joint.solve_block(bodies, &rows, dt, 1.0, 1.0);
        blocks.push((i, rows));
    }
    for (tau, damping) in PASSES {
        for (i, rows) in &blocks {
            joints[*i].limit_pass(bodies, dt, tau);
            joints[*i].solve_block(bodies, rows, dt, tau, damping);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MotionType;
    use glam::Mat3;

    fn body(motion: MotionType, mass: f32, inertia: f32) -> Body {
        let dynamic = motion == MotionType::Dynamic;
        let mut b = Body {
            alive: true,
            motion,
            sensor: false,
            group: None,
            com: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            com_local: Vec3::ZERO,
            mass,
            inv_mass: if dynamic { 1.0 / mass } else { 0.0 },
            inv_inertia_local: if dynamic {
                Mat3::from_diagonal(Vec3::splat(1.0 / inertia))
            } else {
                Mat3::ZERO
            },
            inv_inertia_world: Mat3::ZERO,
            force: Vec3::ZERO,
            torque: Vec3::ZERO,
            friction: 0.5,
            restitution: 0.2,
        };
        b.refresh_inertia();
        b
    }

    fn hinge(max_torque: f32, velocity: f32) -> Joint {
        Joint {
            alive: true,
            a: 0,
            b: 1,
            anchor_a: Vec3::ZERO,
            anchor_b: Vec3::ZERO,
            kind: Kind::Hinge {
                axis_a: Vec3::X,
                axis_b: Vec3::X,
                ref_a: Vec3::Y,
                ref_b: Vec3::Y,
            },
            limits: [-std::f32::consts::PI, std::f32::consts::PI],
            limited: false,
            motor: true,
            max_torque,
            target_velocity: velocity,
            target: 0.0,
            friction: 0.0,
            saved_limits: None,
            steer_rest: None,
        }
    }

    /// The live Scout drive hinge (havok-step__live__018): target 17 rad/s,
    /// cap 675 N·m, so at most 11.25 N·m·s per 1/60 s call; spinning free (the
    /// wheel in the air) it settles at the target with λ near zero.
    #[test]
    fn a_free_wheel_servo_runs_at_its_target_velocity() {
        let mut bodies = vec![
            body(MotionType::Static, 0.0, 1.0),
            body(MotionType::Dynamic, 27.0, 0.45),
        ];
        let mut joints = vec![hinge(675.0, 17.0)];
        let dt = 1.0 / 60.0;
        let mut largest = 0.0f32;
        let mut late = Vec::new();
        for step in 0..240 {
            let before = bodies[1].angular_velocity.x;
            solve(&mut bodies, &mut joints, dt);
            // Integrate the orientation the way the world does.
            let w = bodies[1].angular_velocity;
            let q = bodies[1].rotation;
            bodies[1].rotation =
                (q + Quat::from_xyzw(w.x, w.y, w.z, 0.0) * q * (0.5 * dt)).normalize();
            bodies[1].refresh_inertia();
            largest = largest.max((bodies[1].angular_velocity.x - before).abs());
            if step >= 180 {
                late.push(bodies[1].angular_velocity.x);
            }
        }
        let (lo, hi) = late
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), &w| (a.min(w), b.max(w)));
        // No call gives more than 11.25 N·m·s on 0.45 kg·m².
        assert!(largest <= 11.25 / 0.45 + 1e-3, "{largest}");
        // First-order quaternion integration turns a step by 2 atan(ωh/2), a
        // little less than ωh, so the servo holds ω a little above its target:
        // 17.11 here, and live ω_rel = 17.13..17.16 at the 17 rad/s target.
        let mean = late.iter().sum::<f32>() / late.len() as f32;
        assert!((mean - 17.11).abs() < 0.05, "{mean} in {lo}..{hi}");
    }

    #[test]
    fn a_hinge_holds_its_pivot_together() {
        let mut bodies = vec![
            body(MotionType::Dynamic, 100.0, 10.0),
            body(MotionType::Dynamic, 10.0, 0.5),
        ];
        bodies[1].com = Vec3::new(0.0, 0.0, 0.5);
        let mut j = hinge(0.0, 0.0);
        j.motor = false;
        j.anchor_a = Vec3::new(0.0, 0.0, 0.5);
        joints_drift(&mut bodies, j);
    }

    fn joints_drift(bodies: &mut [Body], j: Joint) {
        let mut joints = vec![j];
        bodies[1].velocity = Vec3::new(0.0, 3.0, 0.0);
        solve(bodies, &mut joints, 1.0 / 60.0);
        let (pa, pb) = joints[0].anchors(bodies);
        let va = bodies[0].point_velocity(pa);
        let vb = bodies[1].point_velocity(pb);
        assert!((va - vb).length() < 1e-3, "{va:?} {vb:?}");
    }
}
