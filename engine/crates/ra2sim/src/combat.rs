//! Collision damage, as the original computes it.
//!
//! Havok fires a collision event for **every contact point on every step** while
//! two bodies touch (world step `FUN_0052aa60` → event manager `FUN_00549c30`).
//! The robot's listener (`FUN_00414d80`) turns each event into a hit in *both*
//! directions: the component on one side asks `computeDamage` (`FUN_00489470`)
//! about the component on the other, using its **own** `piercing`,
//! `concussion` and material, and the damage lands on that exact component.
//! This module is that pipeline, fed by the physics backend's contact points
//! ([`ContactEvent`]).
//!
//! ## The record (`FUN_0045a430`)
//!
//! `rec0` is the **normal closing speed at the contact point**, measured on the
//! bodies' state at the start of the step, before the solver touches them:
//! `|(v_a(p) − v_b(p)) · n|` with `v(p) = v + ω × (p − com)`.
//!
//! ## The magnitude (`FUN_0045a5a0`)
//!
//! ```text
//! M = 0.5 · rec0 / (1/m_a + 1/m_b + (r_a×n)·I_a⁻¹(r_a×n) + (r_b×n)·I_b⁻¹(r_b×n))
//! ```
//!
//! A mass of zero or less counts as an inverse mass of `0.08`, and no inverse
//! mass is smaller than `0.01`, so a body is never heavier than 100 kg to the
//! formula.
//!
//! ## The damage (`FUN_00489470`)
//!
//! ```text
//! damage = (piercing · rec0 · 10 + concussion · M) · scale
//! ```
//!
//! `piercing`/`concussion` and `scale` (the material float) belong to the
//! *attacking* component. A component that declares neither has the
//! constructor's defaults: piercing `0`, concussion `1`. Two material classes
//! zero the hit outright: an **Arenium** victim (class `0x715dc8`) takes
//! nothing, and an Arenium attacker does nothing to a **Rubber** victim
//! (class `0x715de4`).
//!
//! ## The gates (`FUN_00417330`)
//!
//! A component that has already come off (`+0xe0 > 0`, the detach state
//! machine of `FUN_0045f4e0`/`FUN_0045f710`) on either side ends the event.
//! Below `10.0` the amount only drives effects (`FUN_00417510` → component
//! vtable `+0x6c` = `FUN_0045f3a0`: sparks and sound, themselves gated on
//! `> 10`); from `10.0` up it is applied to the victim (`FUN_004175b0` →
//! `FUN_00489620` → vtable `+0x1c`) and scored. There is no damage from a
//! sub-threshold bump, however long it lasts.

use std::collections::HashMap;

use glam::{Mat3, Mat4, Vec3};

use crate::backend::{BodyHandle, BodyState, ContactEvent};
use crate::rig::Role;
use crate::robot::Robot;

/// The coefficients of the hit formula.
#[derive(Clone, Copy, Debug)]
pub struct CombatConfig {
    /// `piercing`'s coefficient — **10** (`FUN_00489470`).
    pub piercing_weight: f32,
    /// `concussion`'s coefficient — **1**; it multiplies [`magnitude`].
    pub concussion_weight: f32,
    /// Damage below this only produces effects and is never applied or scored
    /// (`FUN_00417330`: `10.0 <= dmg`).
    pub apply_threshold: f32,
}

impl Default for CombatConfig {
    fn default() -> Self {
        CombatConfig {
            piercing_weight: 10.0,
            concussion_weight: 1.0,
            apply_threshold: APPLY_THRESHOLD,
        }
    }
}

/// The smallest hit that is applied to hitpoints and scored.
pub const APPLY_THRESHOLD: f32 = 10.0;

/// `FUN_0045a5a0`'s inverse mass for a body with no mass.
const INV_MASS_UNKNOWN: f32 = 0.08;
/// ... and the floor under every inverse mass.
const INV_MASS_FLOOR: f32 = 0.01;

/// One hit: a component of `attacker` struck a component of `target`.
#[derive(Clone, Copy, Debug)]
pub struct Impact {
    pub attacker: usize,
    pub target: usize,
    /// Component index on the target.
    pub component: usize,
    /// Component index on the attacker that did the damage.
    pub by: usize,
    pub damage: f32,
    pub point: Vec3,
    /// The contact normal out of the struck component's body (the direction
    /// the sparks of `FUN_004a2a50` leave along).
    pub normal: Vec3,
    /// The record's closing speed (`rec0`), m/s.
    pub speed: f32,
}

/// `1/m` the way `FUN_0045a5a0` forms it.
pub fn inverse_mass_term(mass: f32) -> f32 {
    let inv = if mass <= 0.0 {
        INV_MASS_UNKNOWN
    } else {
        1.0 / mass
    };
    inv.max(INV_MASS_FLOOR)
}

/// The angular term of one body: `(r × n) · I⁻¹ (r × n)`.
pub fn angular_term(r: Vec3, n: Vec3, inv_inertia_world: Mat3) -> f32 {
    let rn = r.cross(n);
    rn.dot(inv_inertia_world * rn)
}

/// `FUN_0045a5a0`: half the closing speed over the effective inverse mass.
pub fn magnitude(rec0: f32, mass_a: f32, mass_b: f32, ang_a: f32, ang_b: f32) -> f32 {
    let denom = inverse_mass_term(mass_a) + inverse_mass_term(mass_b) + ang_a + ang_b;
    rec0 / denom * 0.5
}

/// What a component does to another, as `FUN_00489470` reads it.
#[derive(Clone, Copy, Debug)]
pub struct Striker<'a> {
    pub piercing: f32,
    pub concussion: f32,
    /// The attacker's material float (`material + 4`).
    pub scale: f32,
    /// The attacker's material class, lower case (`"metal"`, `"arenium"`, …).
    pub material: &'a str,
}

/// `FUN_00489470`: the damage `attacker` does to a component of material
/// `victim_material`, for a record with closing speed `rec0` and magnitude `m`.
pub fn hit_damage(
    attacker: Striker<'_>,
    victim_material: &str,
    rec0: f32,
    m: f32,
    cfg: &CombatConfig,
) -> f32 {
    // Arenium shrugs everything off, and does nothing to rubber.
    if victim_material == "arenium" {
        return 0.0;
    }
    if attacker.material == "arenium" && victim_material == "rubber" {
        return 0.0;
    }
    (attacker.piercing * rec0 * cfg.piercing_weight
        + m * attacker.concussion * cfg.concussion_weight)
        * attacker.scale
}

/// The material class named by a definition's `material = metal(.4)` line.
pub fn material_class(material: Option<&str>) -> &str {
    match material {
        Some(m) => m.split('(').next().unwrap_or("metal").trim(),
        None => "metal",
    }
}

/// `v(p) = v + ω × (p − com)`.
pub fn point_velocity(v: Vec3, w: Vec3, com: Vec3, p: Vec3) -> Vec3 {
    v + w.cross(p - com)
}

/// A robot body as the formula sees it: mass, centre of mass and the inverse
/// inertia tensor in world space, built from the components welded into it.
#[derive(Clone, Copy, Debug)]
pub struct BodyDynamics {
    pub mass: f32,
    pub com: Vec3,
    pub inv_inertia: Mat3,
}

/// The dynamics of part `part` of `robot` at pre-step state `state`.
///
/// Jolt does not expose a body's inertia tensor, so it is rebuilt from what the
/// body is made of: each attached component is a solid box of its own extent at
/// its own mass, about the common centre of mass.
pub fn body_dynamics(robot: &Robot, part: usize, state: &BodyState) -> BodyDynamics {
    let members: Vec<usize> = robot.rig.parts[part]
        .members
        .iter()
        .copied()
        .filter(|&i| robot.attached[i])
        .collect();
    let frame = Mat4::from_rotation_translation(state.rotation, state.position);
    let scale = robot.mass_scale;
    let mut mass = 0.0;
    let mut com_local = Vec3::ZERO;
    for &i in &members {
        let c = &robot.rig.components[i];
        let m = c.mass * scale;
        mass += m;
        com_local += robot.rig.part_local[i].transform_point3(c.center()) * m;
    }
    if mass <= 0.0 {
        return BodyDynamics {
            mass: 0.0,
            com: frame.transform_point3(Vec3::ZERO),
            inv_inertia: Mat3::ZERO,
        };
    }
    com_local /= mass;
    let mut inertia = Mat3::ZERO;
    for &i in &members {
        let c = &robot.rig.components[i];
        let m = c.mass * scale;
        let local = robot.rig.part_local[i];
        let h = c.half_extents();
        let diag = Vec3::new(
            m / 3.0 * (h.y * h.y + h.z * h.z),
            m / 3.0 * (h.x * h.x + h.z * h.z),
            m / 3.0 * (h.x * h.x + h.y * h.y),
        );
        let rot = Mat3::from_mat4(local);
        let own = rot * Mat3::from_diagonal(diag) * rot.transpose();
        let d = local.transform_point3(c.center()) - com_local;
        let shift = Mat3::from_diagonal(Vec3::splat(d.length_squared()))
            - Mat3::from_cols(d * d.x, d * d.y, d * d.z);
        inertia += own + shift * m;
    }
    let inv_local = if inertia.determinant().abs() > 1e-9 {
        inertia.inverse()
    } else {
        Mat3::ZERO
    };
    let r = Mat3::from_quat(state.rotation);
    BodyDynamics {
        mass: mass.max(0.2),
        com: frame.transform_point3(com_local),
        inv_inertia: r * inv_local * r.transpose(),
    }
}

/// What the physics backend holds for a body: mass, centre of mass in the
/// body's frame and inverse inertia in the body's frame.
pub type MassProps = HashMap<BodyHandle, (f32, Vec3, Mat3)>;

/// The backend's own mass properties of every live robot body (empty for a
/// backend that does not expose them).
pub fn mass_props(physics: &dyn crate::backend::PhysicsBackend, robots: &[Robot]) -> MassProps {
    let mut out = MassProps::new();
    for robot in robots {
        for part in robot.parts.iter().filter(|p| p.alive) {
            if let Some(m) = physics.mass_properties(part.body) {
                out.insert(part.body, m);
            }
        }
    }
    out
}

/// The dynamics `FUN_0045a5a0` prices with: the Havok bodies' own mass
/// (`body + 0x190`, read by `FUN_0045a430` into the record), centre of mass
/// (`+0x30`) and world inverse inertia (`+0x10c`). Without the backend's
/// numbers (Jolt), [`body_dynamics`] rebuilds them from the components.
fn dynamics_of(
    robot: &Robot,
    part: usize,
    body: BodyHandle,
    state: &BodyState,
    props: &MassProps,
) -> BodyDynamics {
    match props.get(&body) {
        Some(&(mass, com_local, inv_local)) => {
            let r = Mat3::from_quat(state.rotation);
            BodyDynamics {
                mass,
                com: state.position + state.rotation * com_local,
                inv_inertia: r * inv_local * r.transpose(),
            }
        }
        None => body_dynamics(robot, part, state),
    }
}

/// The attached component of `part` closest to the world point `p`, by distance
/// to its own bounding box. This is the "exact touched component": the compound
/// body is built from one hull per component, so the contact point lies on one
/// of them.
pub fn touched_component(robot: &Robot, part: usize, state: &BodyState, p: Vec3) -> Option<usize> {
    let frame = Mat4::from_rotation_translation(state.rotation, state.position);
    let mut best: Option<(usize, f32)> = None;
    for &i in &robot.rig.parts[part].members {
        let c = &robot.rig.components[i];
        if !robot.attached[i] || c.hull_points.is_empty() {
            continue;
        }
        let world = frame * robot.rig.part_local[i];
        let local = world.inverse().transform_point3(p);
        let clamped = local.clamp(c.min, c.max);
        let d = (local - clamped).length_squared();
        if best.map(|(_, bd)| d < bd).unwrap_or(true) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i)
}

/// The attacking parameters of a component.
fn striker(robot: &Robot, index: usize) -> Striker<'_> {
    let c = &robot.rig.components[index];
    let (piercing, concussion) = match (c.role, c.weapon) {
        (Role::Weapon, Some(w)) => (w.piercing, w.concussion),
        // The component constructor's defaults (`FUN_00489110`).
        _ => (0.0, 1.0),
    };
    Striker {
        piercing,
        concussion,
        scale: c.material_scale,
        material: material_class(c.material.as_deref()),
    }
}

/// The collision-event filter the robots register with (`FUN_0052d4f0`, the
/// world's event manager ahead of the game's listener).
///
/// Every robot body is registered once at 1 Hz (`FUN_0045ab90` →
/// `FUN_004ad500(body, 2, 1.0)` → `FUN_0052d850`, from the robot set-up
/// `0x414d66` and `0x507ed6`); a weapon that declares `frequency` registers its
/// bodies at that rate instead (`0x502f7a` .. `0x503038`). A contact point
/// becomes an event only if one of its two bodies' registrations is due
/// (a full interval since it last let one through) and the bodies close at the
/// point (`FUN_0052d740`: the normal relative velocity against the
/// registration's threshold, 0); the registration that let it through is then
/// reset. All other points are dropped before the game prices anything. Live
/// (`fightcontacts-scout-alarm-box__live__000`): the robots touch at up to 13
/// points a step, yet only 16 % of those steps price a hit, one per component
/// pair, and a pair that stays in contact is priced again about 1.0 s later.
#[derive(Clone, Debug, Default)]
pub struct EventGate {
    last: HashMap<BodyHandle, f32>,
}

impl EventGate {
    /// The events of one step that get through, in order.
    pub fn admit(
        &mut self,
        robots: &[Robot],
        events: Vec<ContactEvent>,
        now: f32,
        props: &MassProps,
    ) -> Vec<ContactEvent> {
        let mut interval: HashMap<BodyHandle, f32> = HashMap::new();
        let mut owner: HashMap<BodyHandle, (usize, usize)> = HashMap::new();
        for (r, robot) in robots.iter().enumerate() {
            for (p, part) in robot.parts.iter().enumerate() {
                if part.alive {
                    owner.insert(part.body, (r, p));
                }
            }
        }
        let com = |body: BodyHandle, state: &BodyState| match owner.get(&body) {
            Some(&(r, p)) => dynamics_of(&robots[r], p, body, state, props).com,
            None => state.position,
        };
        for robot in robots {
            for (p, part) in robot.parts.iter().enumerate() {
                if !part.alive {
                    continue;
                }
                let rate = robot.rig.parts[p]
                    .members
                    .iter()
                    .filter_map(|&i| robot.rig.components[i].weapon.and_then(|w| w.frequency))
                    .fold(None, |m: Option<f32>, f| Some(m.map_or(f, |m| m.max(f))))
                    .unwrap_or(1.0);
                interval.insert(part.body, 1.0 / rate);
            }
        }
        let mut out = Vec::new();
        for e in events {
            let n = e.normal.normalize_or_zero();
            let va = point_velocity(
                e.state_a.linear_velocity,
                e.state_a.angular_velocity,
                com(e.a, &e.state_a),
                e.point,
            );
            let vb = point_velocity(
                e.state_b.linear_velocity,
                e.state_b.angular_velocity,
                com(e.b, &e.state_b),
                e.point,
            );
            if (va - vb).dot(n) <= 0.0 {
                continue;
            }
            let due = |gate: &Self, body: BodyHandle| {
                interval
                    .get(&body)
                    .is_some_and(|iv| gate.last.get(&body).is_none_or(|t| now - t >= *iv - 1e-4))
            };
            let through = if due(self, e.a) {
                Some(e.a)
            } else if due(self, e.b) {
                Some(e.b)
            } else {
                None
            };
            if let Some(body) = through {
                self.last.insert(body, now);
                out.push(e);
            }
        }
        out
    }
}

/// How squarely a weapon's striking faces meet the contact
/// (`Weapon::computeDamage`, `FUN_005032f0`): the contact normal toward the
/// victim, in the weapon's frame, against its `normal` faces; 1 for anything
/// that is not a weapon or lists no face.
fn face_alignment(
    robot: &Robot,
    index: usize,
    part: usize,
    state: &BodyState,
    toward: Vec3,
) -> f32 {
    let c = &robot.rig.components[index];
    let Some(w) = c.weapon.filter(|_| c.role == Role::Weapon) else {
        return 1.0;
    };
    let _ = part;
    let (_, rotation, _) = robot.rig.part_local[index].to_scale_rotation_translation();
    let world = state.rotation * rotation;
    w.alignment(world.inverse() * toward)
}

/// Every hit of one step's contact events, in both directions.
///
/// `events` come from [`crate::backend::PhysicsBackend::take_contacts`]; only
/// contacts between two different robots' bodies are dealt with. A hit is
/// returned whatever its size, so the caller can apply the
/// [`CombatConfig::apply_threshold`] gate the way `FUN_00417330` does.
pub fn resolve(robots: &[Robot], events: &[ContactEvent], cfg: &CombatConfig) -> Vec<Impact> {
    resolve_with(robots, events, cfg, &MassProps::new())
}

/// [`resolve`] with the physics backend's own mass properties ([`mass_props`]).
pub fn resolve_with(
    robots: &[Robot],
    events: &[ContactEvent],
    cfg: &CombatConfig,
    props: &MassProps,
) -> Vec<Impact> {
    let mut owner: HashMap<BodyHandle, (usize, usize)> = HashMap::new();
    for (r, robot) in robots.iter().enumerate() {
        for (p, part) in robot.parts.iter().enumerate() {
            if part.alive {
                owner.insert(part.body, (r, p));
            }
        }
    }
    let mut out = Vec::new();
    for e in events {
        let (Some(&(ra, pa)), Some(&(rb, pb))) = (owner.get(&e.a), owner.get(&e.b)) else {
            continue;
        };
        if ra == rb {
            continue;
        }
        let (robot_a, robot_b) = (&robots[ra], &robots[rb]);
        let dyn_a = dynamics_of(robot_a, pa, e.a, &e.state_a, props);
        let dyn_b = dynamics_of(robot_b, pb, e.b, &e.state_b, props);
        let n = e.normal.normalize_or_zero();
        if n == Vec3::ZERO || dyn_a.mass <= 0.0 || dyn_b.mass <= 0.0 {
            continue;
        }
        let va = point_velocity(
            e.state_a.linear_velocity,
            e.state_a.angular_velocity,
            dyn_a.com,
            e.point,
        );
        let vb = point_velocity(
            e.state_b.linear_velocity,
            e.state_b.angular_velocity,
            dyn_b.com,
            e.point,
        );
        let rec0 = (va - vb).dot(n).abs();
        let m = magnitude(
            rec0,
            dyn_a.mass,
            dyn_b.mass,
            angular_term(e.point - dyn_a.com, n, dyn_a.inv_inertia),
            angular_term(e.point - dyn_b.com, n, dyn_b.inv_inertia),
        );
        let (Some(ca), Some(cb)) = (
            touched_component(robot_a, pa, &e.state_a, e.point),
            touched_component(robot_b, pb, &e.state_b, e.point),
        ) else {
            continue;
        };
        for (att, att_c, vic, vic_c, toward, state, part) in [
            (ra, ca, rb, cb, n, &e.state_a, pa),
            (rb, cb, ra, ca, -n, &e.state_b, pb),
        ] {
            let (attacker, victim) = (&robots[att], &robots[vic]);
            let damage = hit_damage(
                striker(attacker, att_c),
                material_class(victim.rig.components[vic_c].material.as_deref()),
                rec0,
                m,
                cfg,
            ) * face_alignment(attacker, att_c, part, state, toward);
            if damage > 0.0 && damage.is_finite() {
                out.push(Impact {
                    attacker: att,
                    target: vic,
                    component: vic_c,
                    by: att_c,
                    damage,
                    point: e.point,
                    // `n` points from `a` toward `b`.
                    normal: if vic == ra { n } else { -n },
                    speed: rec0,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metal(piercing: f32, concussion: f32, scale: f32) -> Striker<'static> {
        Striker {
            piercing,
            concussion,
            scale,
            material: "metal",
        }
    }

    #[test]
    fn inverse_mass_follows_the_clamps() {
        assert_eq!(inverse_mass_term(0.0), 0.08);
        assert_eq!(inverse_mass_term(-3.0), 0.08);
        assert!((inverse_mass_term(20.0) - 0.05).abs() < 1e-6);
        // Heavier than 100 kg is never heavier than 100 kg.
        assert_eq!(inverse_mass_term(250.0), 0.01);
    }

    #[test]
    fn magnitude_is_half_the_speed_over_the_effective_inverse_mass() {
        // 10 m/s, 20 kg against 50 kg, no rotation:
        // 0.5 * 10 / (0.05 + 0.02) = 71.428...
        let m = magnitude(10.0, 20.0, 50.0, 0.0, 0.0);
        assert!((m - 5.0 / 0.07).abs() < 1e-3, "{m}");
        // The angular terms add to the denominator.
        let m = magnitude(10.0, 20.0, 50.0, 0.03, 0.0);
        assert!((m - 5.0 / 0.10).abs() < 1e-3, "{m}");
    }

    #[test]
    fn angular_term_uses_the_lever_arm_perpendicular_to_the_normal() {
        let inv = Mat3::from_diagonal(Vec3::splat(0.5));
        // r = (0,1,0), n = (1,0,0): r×n = (0,0,-1), |r×n|² = 1, times 0.5.
        let t = angular_term(Vec3::Y, Vec3::X, inv);
        assert!((t - 0.5).abs() < 1e-6);
        // A contact along the normal through the centre adds nothing.
        assert_eq!(angular_term(Vec3::X, Vec3::X, inv), 0.0);
    }

    #[test]
    fn damage_is_piercing_times_speed_times_ten_plus_concussion_times_magnitude() {
        let cfg = CombatConfig::default();
        // (0.3 * 8 * 10 + 0.8 * 40) * 0.1 = (24 + 32) * 0.1 = 5.6
        let d = hit_damage(metal(0.3, 0.8, 0.1), "metal", 8.0, 40.0, &cfg);
        assert!((d - 5.6).abs() < 1e-4, "{d}");
        // A plain chassis: piercing 0, concussion 1, scale 0.1.
        let d = hit_damage(metal(0.0, 1.0, 0.1), "metal", 8.0, 40.0, &cfg);
        assert!((d - 4.0).abs() < 1e-4, "{d}");
        // The ram plate overrides the scale to 0.4.
        let d = hit_damage(metal(0.0, 1.0, 0.4), "metal", 8.0, 40.0, &cfg);
        assert!((d - 16.0).abs() < 1e-4, "{d}");
    }

    #[test]
    fn arenium_is_immune_and_does_nothing_to_rubber() {
        let cfg = CombatConfig::default();
        let a = Striker {
            material: "arenium",
            ..metal(0.0, 1.0, 0.5)
        };
        assert_eq!(hit_damage(a, "rubber", 8.0, 40.0, &cfg), 0.0);
        assert!(hit_damage(a, "metal", 8.0, 40.0, &cfg) > 0.0);
        // Anything that strikes an arenium component does nothing.
        assert_eq!(
            hit_damage(metal(0.9, 1.0, 0.1), "arenium", 8.0, 40.0, &cfg),
            0.0
        );
        // Rubber attacks with a scale of zero.
        assert_eq!(
            hit_damage(
                Striker {
                    material: "rubber",
                    ..metal(0.0, 1.0, 0.0)
                },
                "metal",
                8.0,
                40.0,
                &cfg
            ),
            0.0
        );
    }

    #[test]
    fn material_class_reads_the_name_before_the_override() {
        assert_eq!(material_class(Some("metal(.4)")), "metal");
        assert_eq!(material_class(Some("rubber(0)")), "rubber");
        assert_eq!(material_class(Some("arenium")), "arenium");
        assert_eq!(material_class(None), "metal");
    }

    #[test]
    fn point_velocity_adds_the_spin() {
        let v = point_velocity(Vec3::X, Vec3::Y * 2.0, Vec3::ZERO, Vec3::Z);
        // ω × r = (0,2,0) × (0,0,1) = (2,0,0)
        assert!((v - Vec3::new(3.0, 0.0, 0.0)).length() < 1e-6);
    }
}
