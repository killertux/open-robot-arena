//! Weapon impacts: turning contacts into damage.
//!
//! Jolt reports whether two robot hulls are touching, but not which part
//! touched which. Rather than guess from geometry (bounding volumes miss real
//! hull contacts), we let the physics engine decide *whether* two robots are
//! engaged and then attribute the hit: the attacker contributes its best weapon
//! (peak damage factor times tip speed), and the damage lands on whichever of
//! the opponent's parts is nearest the strike.

use glam::{Mat4, Vec3};

use crate::backend::PhysicsBackend;
use crate::rig::Role;
use crate::robot::Robot;

/// Tuning for the damage model.
#[derive(Clone, Copy, Debug)]
pub struct CombatConfig {
    /// Global damage multiplier.
    pub damage_scale: f32,
    /// Damage factor for parts that are not weapons (ramming).
    pub ram_factor: f32,
    /// Impact speed (m/s) treated as the baseline.
    pub speed_floor: f32,
}

impl Default for CombatConfig {
    fn default() -> Self {
        CombatConfig {
            damage_scale: 22.0,
            ram_factor: 0.25,
            speed_floor: 1.5,
        }
    }
}

/// One part hitting another in a single step.
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
}

/// Damage factor and rotor tip speed for one attacking component. `worlds` are
/// component world transforms (the rig's bodies already place them).
fn offense(r: &Robot, worlds: &[Mat4], index: usize, cfg: &CombatConfig) -> Option<(f32, f32)> {
    let c = &r.rig.components[index];
    let factor = match c.role {
        Role::Weapon => c.weapon.map(|w| w.concussion + w.piercing).unwrap_or(0.5),
        Role::Chassis => cfg.ram_factor,
        Role::Motor | Role::Piston => cfg.ram_factor * 0.75,
        Role::Wheel => cfg.ram_factor * 0.5,
        _ => cfg.ram_factor,
    };
    if factor <= 0.0 {
        return None;
    }

    // Tip speed of the nearest motor turning this part.
    let here = worlds[index].transform_point3(c.center());
    let mut tip = 0.0;
    let mut p = c.parent;
    while let Some(pi) = p {
        if let Some(m) = r.rig.components[pi].motor {
            let mc = &r.rig.components[pi];
            let mw = worlds[pi].transform_point3(mc.center());
            tip = m.speed * (here - mw).length();
            break;
        }
        p = r.rig.components[pi].parent;
    }
    Some((factor, tip))
}

/// The attacker's best weapon this step: `(component, factor, impact speed, world point)`.
fn best_attack(
    r: &Robot,
    worlds: &[Mat4],
    closing: f32,
    cfg: &CombatConfig,
) -> Option<(usize, f32, f32, Vec3)> {
    let mut best: Option<(usize, f32, f32, Vec3)> = None;
    for (i, c) in r.rig.components.iter().enumerate() {
        if !r.attached[i] || c.hull_points.is_empty() {
            continue;
        }
        let Some((factor, tip)) = offense(r, worlds, i, cfg) else {
            continue;
        };
        let impact = cfg.speed_floor + closing + tip;
        let point = worlds[i].transform_point3(c.center());
        let better = best
            .map(|(_, f, im, _)| factor * impact > f * im)
            .unwrap_or(true);
        if better {
            best = Some((i, factor, impact, point));
        }
    }
    best
}

/// If robots `a` and `b` are in contact, the damage `a` deals to `b` this step.
pub fn detect(
    physics: &dyn PhysicsBackend,
    robots: &[Robot],
    a: usize,
    b: usize,
    dt: f32,
    cfg: &CombatConfig,
) -> Option<Impact> {
    let (ra, rb) = (&robots[a], &robots[b]);
    if !physics.in_contact(ra.body(), rb.body()) {
        return None;
    }
    let (wa, wb) = (ra.component_worlds(physics), rb.component_worlds(physics));
    let closing = (ra.velocity(physics) - rb.velocity(physics)).length();

    let (by, factor, impact, at) = best_attack(ra, &wa, closing, cfg)?;

    // Land the hit on the opponent's part nearest the strike, skipping
    // indestructible fittings (control boards, anchors) that would otherwise
    // soak the damage forever.
    let mut target = None;
    let mut best_d = f32::MAX;
    for (j, c) in rb.rig.components.iter().enumerate() {
        if !rb.attached[j] || c.hull_points.is_empty() || !c.damageable {
            continue;
        }
        let d = (wb[j].transform_point3(c.center()) - at).length();
        if d < best_d {
            best_d = d;
            target = Some(j);
        }
    }
    let component = target?;

    Some(Impact {
        attacker: a,
        target: b,
        component,
        by,
        damage: cfg.damage_scale * factor * impact * dt,
        point: at,
    })
}

/// Human-readable summary of what an attacker would do to a target right now.
pub fn report(
    physics: &dyn PhysicsBackend,
    robots: &[Robot],
    a: usize,
    b: usize,
    cfg: &CombatConfig,
) -> Vec<String> {
    let (ra, rb) = (&robots[a], &robots[b]);
    let wa = ra.component_worlds(physics);
    let closing = (ra.velocity(physics) - rb.velocity(physics)).length();
    let contact = physics.in_contact(ra.body(), rb.body());

    let mut rows = vec![format!("contact={contact} closing={closing:.2} m/s")];
    for (i, c) in ra.rig.components.iter().enumerate() {
        if !ra.attached[i] {
            continue;
        }
        let (factor, tip) = offense(ra, &wa, i, cfg).unwrap_or((0.0, 0.0));
        if factor > cfg.ram_factor {
            rows.push(format!(
                "  attacker {:2} {:24} factor={:.2} tip={:5.1} m/s",
                i, c.name, factor, tip
            ));
        }
    }
    if let Some((_, factor, impact, _)) = best_attack(ra, &wa, closing, cfg) {
        rows.push(format!(
            "  best: factor={factor:.2} impact={impact:.1} dps={:.0}",
            cfg.damage_scale * factor * impact
        ));
    }
    let _ = (ra, rb, wa);
    rows
}
