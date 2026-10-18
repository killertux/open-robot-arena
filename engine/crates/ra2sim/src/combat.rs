//! Weapon impacts: turning contacts into damage.
//!
//! Jolt reports whether two *bodies* are touching, but not which component
//! touched which. Rather than guess from geometry (bounding volumes miss real
//! hull contacts), we let the physics engine decide which of the two robots'
//! bodies met and then attribute the hit: the attacker contributes its best
//! weapon among the bodies that made contact, and the damage lands on whichever
//! of the opponent's contacting bodies carries the nearest destructible part.
//!
//! Testing only the robots' root bodies — as this used to — misses every hit
//! that lands elsewhere. A robot is several hinged parts (a chassis group plus a
//! body per motor), so a spinner arm or a wheel meeting the opponent is a
//! different body pair, and those contacts silently did no damage.
//!
//! ## A hit is an event, and a grind is a rate
//!
//! The game reports damage in *amounts per hit* — `GetLastDamageReceived`
//! returns `(component, amount, time, by player, by component)`, the AI scripts
//! treat `amount > 10` as "a good solid hit" and hazards apply a flat 25 — so
//! the damage a ram does is set by the speed the robots *arrive* with, dealt
//! once when contact begins. Integrating an instantaneous speed over the steps a
//! contact lasts (what this used to do) gives a brief fast impact almost no
//! weight and lets two motionless robots erode each other, which is neither what
//! the game does nor what a fight looks like.
//!
//! A spinning weapon is different: it works the same joint repeatedly while it
//! turns, so its rotor **tip speed is a rate** and is applied every step.
//! Motionless contact therefore does nothing, a ram is a single blow, and a
//! spinner grinds.

use glam::{Mat4, Vec3};

use crate::backend::PhysicsBackend;
use crate::rig::Role;
use crate::robot::Robot;

/// Tuning for the damage model.
///
/// The engine's own hit formula was recovered from the binary
/// (`FUN_00489470`, called from the collision handler `FUN_00503c30`):
///
/// ```text
/// damage = (piercing * contact_speed * 10 + concussion * magnitude) * scale
/// ```
///
/// It is **linear and additive** in the two parameters the component declares,
/// with coefficients **10** and **1**, which is the shape the community's own
/// measurements fit (`D = 183.9·P + 100·C`). `magnitude` is the collision's
/// *impulse*: the engine's magnitude function (`FUN_0045a5a0`) works from the
/// contrast of the two bodies and returns `0.5 · μ · v` for the reduced mass and
/// closing speed, halving it at the end. `scale` is the **attacking
/// component's** material float (`FUN_00489470` reads
/// `*(float *)(*(int *)(this + 0x64) + 4)`): `Metal` 0.1, `Rubber` 0.0,
/// `Arenium` 0.5, overridden per definition by `material = <class>(<n>)` — the
/// ram plate is `metal(.4)`. It is no longer calibrated; F7 recovered it from
/// the `MaterialObject` constructors and the component factory.
///
/// A non-weapon part declares neither parameter, so its whole factor is the
/// impulse's coefficient — a chassis ram is priced by the collision itself,
/// which is also why there is no per-role weighting: the engine has none.
#[derive(Clone, Copy, Debug)]
pub struct CombatConfig {
    /// `piercing`'s coefficient — **10**, recovered.
    pub piercing_weight: f32,
    /// `concussion`'s coefficient — **1**, recovered; it multiplies the
    /// collision's impulse.
    pub concussion_weight: f32,
}

impl Default for CombatConfig {
    fn default() -> Self {
        CombatConfig {
            piercing_weight: 10.0,
            concussion_weight: 1.0,
        }
    }
}

/// The engine's own gate: a blow smaller than this is discarded.
///
/// `FUN_00503c30` only applies its hit when the amount comes out above `0.5`, so
/// two robots leaning on each other at rest do nothing at all.
pub const MIN_BLOW: f32 = 0.5;

/// The reduced mass of a pair of robots, `m1·m2/(m1+m2)`.
///
/// Half of it is the coefficient of the collision's impulse in the hit formula,
/// so two heavy robots hurt each other more than a heavy one hurts a light one.
pub fn reduced_mass(a: &Robot, b: &Robot) -> f32 {
    let (ma, mb) = (a.remaining_mass().max(0.2), b.remaining_mass().max(0.2));
    ma * mb / (ma + mb)
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

/// The best strike one robot can make this step.
#[derive(Clone, Copy, Debug)]
struct Strike {
    /// The component that strikes.
    index: usize,
    /// Damage factor (`concussion + piercing` for a weapon).
    factor: f32,
    /// The speed the strike arrives with: the robots' closing speed for a blow,
    /// the part's rotor tip speed for a grinding one (m/s).
    impact: f32,
    /// Damage dealt this step.
    damage: f32,
    point: Vec3,
}

/// Damage factor per m/s and rotor tip speed for one attacking component, plus
/// that tip speed. `worlds` are component world transforms (the rig's bodies
/// already place them) and `impulse` is half the reduced mass of the two robots
/// — the engine's collision magnitude without its speed term.
fn offense(
    r: &Robot,
    worlds: &[Mat4],
    index: usize,
    impulse: f32,
    cfg: &CombatConfig,
) -> Option<(f32, f32)> {
    let c = &r.rig.components[index];
    // The collision itself, plus — for a weapon — the two parameters its own
    // definition declares. A part that declares none contributes the collision
    // alone, which is how the engine prices a bare ram.
    let factor = match c.role {
        Role::Weapon => {
            let (piercing, concussion) = c
                .weapon
                .map(|w| (w.piercing, w.concussion))
                .unwrap_or((0.0, 0.0));
            cfg.piercing_weight * piercing + cfg.concussion_weight * impulse * concussion
        }
        _ => impulse,
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

/// The attacker's best strike this step, among the components on the bodies that
/// are touching the opponent.
///
/// `blow` is the closing speed the robots arrived with on the step contact
/// began, and zero while a contact simply continues; `dt` scales the rotor term,
/// which is a rate.
fn best_strike(
    r: &Robot,
    worlds: &[Mat4],
    blow: f32,
    impulse: f32,
    dt: f32,
    on_parts: &[bool],
    cfg: &CombatConfig,
) -> Option<Strike> {
    let mut best: Option<Strike> = None;
    for (i, c) in r.rig.components.iter().enumerate() {
        if !r.attached[i] || c.hull_points.is_empty() {
            continue;
        }
        if !on_parts.get(r.rig.part_of[i]).copied().unwrap_or(false) {
            continue;
        }
        let Some((factor, tip)) = offense(r, worlds, i, impulse, cfg) else {
            continue;
        };
        // F7: the hit's `scale` is the **attacking component's** material
        // float (`FUN_00489470` reads it from `this + 0x64`, and the same
        // object carries the `piercing`/`concussion` it just used). A chassis
        // that declares no material is Metal, 0.1; a ram plate is 0.4; a
        // rubber caster is 0.0.
        let damage = c.material_scale * factor * (blow + tip * dt);
        if damage <= 0.0 {
            continue;
        }
        let strike = Strike {
            index: i,
            factor,
            impact: blow.max(tip),
            damage,
            point: worlds[i].transform_point3(c.center()),
        };
        if best.as_ref().map(|b| damage > b.damage).unwrap_or(true) {
            best = Some(strike);
        }
    }
    best
}

/// Which bodies of `a` and `b` are touching, as `(a_parts, b_parts)` flags.
///
/// A robot is several hinged bodies, so a contact is between two particular
/// bodies (the chassis group, a motor's rotor, a wheel); both sides of it are
/// what the hit is attributed through.
pub fn contacting_parts(
    physics: &dyn PhysicsBackend,
    a: &Robot,
    b: &Robot,
) -> Option<(Vec<bool>, Vec<bool>)> {
    let mut a_parts = vec![false; a.parts.len()];
    let mut b_parts = vec![false; b.parts.len()];
    let mut any = false;
    for (i, pa) in a.parts.iter().enumerate() {
        if !pa.alive {
            continue;
        }
        for (j, pb) in b.parts.iter().enumerate() {
            if !pb.alive {
                continue;
            }
            if physics.in_contact(pa.body, pb.body) {
                a_parts[i] = true;
                b_parts[j] = true;
                any = true;
            }
        }
    }
    any.then_some((a_parts, b_parts))
}

/// How a pair met this step.
#[derive(Clone, Copy, Debug)]
pub struct Contact {
    /// The closing speed the two robots arrived with, measured *before* the step
    /// ran, so a blow is priced by the speed it arrived with rather than by
    /// whatever the collision left behind (m/s).
    pub approach: f32,
    /// Whether this step is the one contact began on. A blow is dealt here; a
    /// contact that simply continues only grinds.
    pub first: bool,
}

/// If robots `a` and `b` are in contact, the damage `a` deals to `b` this step.
pub fn detect(
    physics: &dyn PhysicsBackend,
    robots: &[Robot],
    a: usize,
    b: usize,
    dt: f32,
    contact: Contact,
    cfg: &CombatConfig,
) -> Option<Impact> {
    let (ra, rb) = (&robots[a], &robots[b]);
    let (a_parts, b_parts) = contacting_parts(physics, ra, rb)?;
    let (wa, wb) = (ra.component_worlds(physics), rb.component_worlds(physics));
    let blow = if contact.first { contact.approach } else { 0.0 };

    // The collision's own magnitude, without its speed: half the reduced mass.
    let impulse = 0.5 * reduced_mass(ra, rb);
    let strike = best_strike(ra, &wa, blow, impulse, dt, &a_parts, cfg)?;

    // A blow the engine would discard: it only applies a hit above `0.5`. The
    // gate is on the *blow*, not the grind — a spinner's rotor works the joint
    // as a rate, and each pass of a blade is a hit of its own.
    if contact.first && strike.damage <= MIN_BLOW {
        return None;
    }

    // Land the hit on the opponent's part nearest the strike, among the bodies
    // that are actually touching, and skipping indestructible fittings (control
    // boards, anchors) that would otherwise soak the damage forever.
    let mut target = None;
    let mut best_d = f32::MAX;
    for (j, c) in rb.rig.components.iter().enumerate() {
        if !rb.attached[j] || c.hull_points.is_empty() || !c.damageable {
            continue;
        }
        if !b_parts.get(rb.rig.part_of[j]).copied().unwrap_or(false) {
            continue;
        }
        let d = (wb[j].transform_point3(c.center()) - strike.point).length();
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
        by: strike.index,
        damage: strike.damage,
        point: strike.point,
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
    let (ra, _rb) = (&robots[a], &robots[b]);
    let wa = ra.component_worlds(physics);
    let closing = (ra.velocity(physics) - _rb.velocity(physics)).length();
    let contacts = contacting_parts(physics, ra, _rb);

    let impulse = 0.5 * reduced_mass(ra, _rb);
    let mut rows = vec![match &contacts {
        Some((ap, bp)) => format!(
            "contact=true closing={closing:.2} m/s parts a=[{}] b=[{}]",
            flag_list(ap),
            flag_list(bp)
        ),
        None => format!("contact=false closing={closing:.2} m/s"),
    }];
    if let Some((on_parts, _)) = contacts.as_ref() {
        for (i, c) in ra.rig.components.iter().enumerate() {
            if !ra.attached[i] {
                continue;
            }
            let (factor, tip) = offense(ra, &wa, i, impulse, cfg).unwrap_or((0.0, 0.0));
            if ra.rig.components[i].role == Role::Weapon {
                let touching = on_parts.get(ra.rig.part_of[i]).copied().unwrap_or(false);
                rows.push(format!(
                    "  attacker {:2} {:24} factor={:.2} tip={:5.1} m/s{}",
                    i,
                    c.name,
                    factor,
                    tip,
                    if touching { " (touching)" } else { "" }
                ));
            }
        }
        // What a blow would do, and what the grind does per second.
        if let Some(s) = best_strike(ra, &wa, closing, impulse, 1.0, on_parts, cfg) {
            rows.push(format!(
                "  blow: factor={:.2} impact={:.1} m/s amount={:.0}",
                s.factor, s.impact, s.damage
            ));
        }
        if let Some(s) = best_strike(ra, &wa, 0.0, impulse, 1.0, on_parts, cfg) {
            rows.push(format!("  grind: dps={:.0}", s.damage));
        }
    }
    rows
}

/// `"0 2 3"` for the entries that are set.
fn flag_list(flags: &[bool]) -> String {
    flags
        .iter()
        .enumerate()
        .filter(|(_, f)| **f)
        .map(|(i, _)| i.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}
