//! Havok 1.x's collision resolver as Robot Arena 2 configures it: type 2,
//! sequential single-point impulses (`setResolver(2)` at `0x4ac3b5`).
//!
//! The order of operations is the engine's:
//!
//! 1. per island (`0x5586a0`), per contact, the 3×3 impulse response
//!    `K = K_A(p) + K_B(p)` and its inverse are computed once, before any
//!    impulse (`FUN_00540630`), with the bodies where the last detection left
//!    them;
//! 2. the iterative loop (`FUN_00559720`) walks the contacts round-robin and
//!    gives an impulse to the next one that is still approaching
//!    (`v_n < -0.001`, `DAT_006ac604`), until a full lap finds none or
//!    `4 × 100` impulses have been spent; after the first 100 the impulses are
//!    inelastic;
//! 3. each impulse (`FUN_00540820`) tries to stop the point dead with
//!    restitution, and falls back to the edge of the friction cone, whose
//!    limit scales with `1 + e`;
//! 4. after every impulse the contact is pushed apart (`FUN_005598f0`) if it is
//!    closer than `0.9` of the tolerance and now separating.
//!
//! Friction combines as `sqrt(µA µB)` and restitution as `eA eB` (`0x540820`).

use glam::{Mat3, Vec3};

use super::body::Body;

/// `v_n` below which a contact counts as approaching (`DAT_006ac604`).
pub const APPROACH: f32 = -0.001;
/// Impulses per round of the loop (`0x559680` sets the budget's unit to 100)
/// and the number of rounds (`0x559720` loops while `n < 4 * 100`).
pub const IMPULSES_PER_ROUND: usize = 100;
pub const ROUNDS: usize = 4;
/// Push-out starts this fraction of the tolerance from the surface
/// (`DAT_006a6070`) and its velocity is `GAIN · tol · f²` (`DAT_006a57a8`).
pub const PUSH_OUT_FRACTION: f32 = 0.9;
pub const PUSH_OUT_GAIN: f32 = 10.0;

/// One contact point of the last collision detection.
#[derive(Clone, Copy, Debug)]
pub struct Contact {
    pub a: usize,
    pub b: usize,
    /// The tags of the colliding pieces of `a` and `b` ([`crate::backend::NO_TAG`] if unknown).
    pub tag_a: u32,
    pub tag_b: u32,
    /// Unit normal, pointing from `b` toward `a` (the resolver's
    /// `v_n = n · (v_A(p) - v_B(p))` is negative when they approach).
    pub normal: Vec3,
    /// The point both bodies' velocities are measured at.
    pub point: Vec3,
    /// Separation along the normal when it was detected (`contact + 0x54`):
    /// positive apart, negative overlapping.
    pub distance: f32,
    /// `sqrt(µA µB)` and `eA eB`.
    pub friction: f32,
    pub restitution: f32,
}

impl Contact {
    /// Relative velocity of `a` against `b` at the point (`FUN_00559840`).
    fn relative_velocity(&self, bodies: &[Body]) -> Vec3 {
        bodies[self.a].point_velocity(self.point) - bodies[self.b].point_velocity(self.point)
    }
}

/// What the resolver precomputes per contact (`resolver + 0x38`, `+ 0x48`).
struct Response {
    inverse: Mat3,
    /// `n · K n`, the normal's own response.
    normal: f32,
}

/// The impulse one contact needs (`FUN_00540820`), given the relative velocity
/// `u` at its point.
fn impulse(c: &Contact, r: &Response, u: Vec3, inelastic: bool) -> Vec3 {
    let n = c.normal;
    let e = if inelastic { 0.0 } else { c.restitution };
    let mu = c.friction;
    // The frictionless impulse that stops the normal motion, ...
    let a = -n.dot(u) / r.normal;
    let p0 = n * a;
    // ... the one that stops the point dead, and the difference.
    let w = r.inverse * -u;
    let d = w - p0;
    let p = p0 * (1.0 + e) + d;
    let pn = p.dot(n);
    let pt = (p - n * pn).length();
    if mu * pn < pt {
        // Outside the cone: slide back along `d` to its edge,
        // |J_t| = µ J_n with J = (1 + e) P0 + s d.
        let wt = (w - n * w.dot(n)).length();
        let s = (1.0 + e) * a * mu / (wt - d.dot(n) * mu);
        p0 * (1.0 + e) + d * s
    } else {
        p
    }
}

/// Push a contact apart (`FUN_005598f0`): a velocity change of
/// `10 · tol · f²` along the normal for each body, with the angular change the
/// matching mass-scaled impulse at the point gives.
fn push_out(c: &Contact, bodies: &mut [Body], tol: f32) {
    let edge = tol * PUSH_OUT_FRACTION;
    let f = (edge - c.distance) / edge;
    if !(f > 0.0 && f < 1.0) {
        return;
    }
    if c.normal.dot(c.relative_velocity(bodies)) <= 0.0 {
        return;
    }
    let dv = c.normal * (tol * PUSH_OUT_GAIN * f * f);
    for (index, sign) in [(c.a, 1.0f32), (c.b, -1.0f32)] {
        let body = &mut bodies[index];
        if body.is_dynamic() {
            let j = dv * (body.mass * sign);
            body.apply_impulse_at(j, c.point);
        }
    }
}

/// Resolve every contact of the last detection, island by island.
pub fn resolve(bodies: &mut [Body], contacts: &[Contact], tol: f32) {
    for island in islands(bodies, contacts) {
        resolve_island(bodies, contacts, &island, tol);
    }
}

fn resolve_island(bodies: &mut [Body], contacts: &[Contact], island: &[usize], tol: f32) {
    let responses: Vec<Response> = island
        .iter()
        .map(|&i| {
            let c = &contacts[i];
            let k = bodies[c.a].collision_matrix(c.point) + bodies[c.b].collision_matrix(c.point);
            let inverse = if k.determinant().abs() > 1e-12 {
                k.inverse()
            } else {
                Mat3::ZERO
            };
            Response {
                inverse,
                normal: c.normal.dot(k * c.normal),
            }
        })
        .collect();
    let count = island.len();
    if count == 0 {
        return;
    }
    let mut cursor = 0usize;
    let mut spent = 0usize;
    while spent < IMPULSES_PER_ROUND * ROUNDS {
        // The next contact, round-robin, that is still approaching.
        let mut found = None;
        for _ in 0..count {
            let k = cursor;
            cursor = (cursor + 1) % count;
            let c = &contacts[island[k]];
            let u = c.relative_velocity(bodies);
            if c.normal.dot(u) < APPROACH && responses[k].normal > 1e-12 {
                found = Some((k, u));
                break;
            }
        }
        let Some((k, u)) = found else {
            return;
        };
        let inelastic = spent > IMPULSES_PER_ROUND;
        let c = &contacts[island[k]];
        let j = impulse(c, &responses[k], u, inelastic);
        bodies[c.a].apply_impulse_at(j, c.point);
        bodies[c.b].apply_impulse_at(-j, c.point);
        push_out(c, bodies, tol);
        spent += 1;
    }
}

/// Groups of contacts that share a body the resolver moves (`0x5586a0`).
/// Contacts keep their detection order within an island.
fn islands(bodies: &[Body], contacts: &[Contact]) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..bodies.len()).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for c in contacts {
        if bodies[c.a].is_dynamic() && bodies[c.b].is_dynamic() {
            let (ra, rb) = (find(&mut parent, c.a), find(&mut parent, c.b));
            if ra != rb {
                parent[ra] = rb;
            }
        }
    }
    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    for (i, c) in contacts.iter().enumerate() {
        let owner = if bodies[c.a].is_dynamic() { c.a } else { c.b };
        let root = find(&mut parent, owner);
        match groups.iter_mut().find(|(r, _)| *r == root) {
            Some((_, list)) => list.push(i),
            None => groups.push((root, vec![i])),
        }
    }
    groups.into_iter().map(|(_, list)| list).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MotionType;
    use glam::Quat;

    pub(crate) fn body(motion: MotionType, mass: f32) -> Body {
        let inv = if motion == MotionType::Dynamic {
            1.0 / mass
        } else {
            0.0
        };
        let i = 0.4 * mass * 0.25;
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
            inv_mass: inv,
            inv_inertia_local: if inv > 0.0 {
                Mat3::from_diagonal(Vec3::splat(1.0 / i))
            } else {
                Mat3::ZERO
            },
            inv_inertia_world: Mat3::ZERO,
            force: Vec3::ZERO,
            torque: Vec3::ZERO,
            friction: 0.5,
            restitution: 0.2,
            active: true,
            freeze: Default::default(),
            system: None,
        };
        b.refresh_inertia();
        b
    }

    fn contact(normal: Vec3, point: Vec3, mu: f32, e: f32) -> Contact {
        Contact {
            tag_a: crate::backend::NO_TAG,
            tag_b: crate::backend::NO_TAG,
            a: 0,
            b: 1,
            normal,
            point,
            distance: 0.095,
            friction: mu,
            restitution: e,
        }
    }

    #[test]
    fn a_falling_body_bounces_with_the_product_restitution() {
        let mut bodies = vec![
            body(MotionType::Dynamic, 10.0),
            body(MotionType::Static, 0.0),
        ];
        bodies[0].velocity = Vec3::new(0.0, -2.0, 0.0);
        let c = contact(Vec3::Y, Vec3::new(0.0, -0.5, 0.0), 0.5, 0.2 * 0.3);
        resolve(&mut bodies, &[c], 0.1);
        assert!(
            (bodies[0].velocity.y - 2.0 * 0.06).abs() < 1e-4,
            "{:?}",
            bodies[0].velocity
        );
    }

    #[test]
    fn a_sliding_body_is_held_to_the_friction_cone() {
        let mut bodies = vec![
            body(MotionType::Dynamic, 10.0),
            body(MotionType::Static, 0.0),
        ];
        // Through the centre, so no spin: approach 1 m/s, slide 10 m/s.
        bodies[0].velocity = Vec3::new(10.0, -1.0, 0.0);
        let c = contact(Vec3::Y, Vec3::ZERO, 0.5, 0.0);
        resolve(&mut bodies, &[c], 0.1);
        // Normal impulse 10 kg·m/s, so friction removes at most 0.5 m/s.
        assert!(
            bodies[0].velocity.y.abs() < 1e-4,
            "{:?}",
            bodies[0].velocity
        );
        assert!(
            (bodies[0].velocity.x - 9.5).abs() < 1e-3,
            "{:?}",
            bodies[0].velocity
        );
    }

    #[test]
    fn a_slow_body_inside_the_cone_sticks() {
        let mut bodies = vec![
            body(MotionType::Dynamic, 10.0),
            body(MotionType::Static, 0.0),
        ];
        bodies[0].velocity = Vec3::new(0.2, -1.0, 0.0);
        let c = contact(Vec3::Y, Vec3::ZERO, 0.5, 0.0);
        resolve(&mut bodies, &[c], 0.1);
        assert!(
            bodies[0].velocity.length() < 1e-4,
            "{:?}",
            bodies[0].velocity
        );
    }

    #[test]
    fn a_contact_inside_the_cushion_is_pushed_apart() {
        let mut bodies = vec![
            body(MotionType::Dynamic, 10.0),
            body(MotionType::Static, 0.0),
        ];
        bodies[0].velocity = Vec3::new(0.0, -1.0, 0.0);
        let mut c = contact(Vec3::Y, Vec3::ZERO, 0.5, 0.25);
        c.distance = 0.045; // half of the 0.09 cushion: f = 0.5
        resolve(&mut bodies, &[c], 0.1);
        // Bounce 0.25 m/s, then 10 * 0.1 * 0.25 = 0.25 m/s of push.
        assert!(
            (bodies[0].velocity.y - 0.5).abs() < 1e-4,
            "{:?}",
            bodies[0].velocity
        );
    }
}
