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
//!
//! ## The arena (docs/29 §12o)
//!
//! The arena manager registers every colliding body of the arena `.gmf` with
//! the event filter under one component of its own (`FUN_00415140`, an
//! `Arena_callback` with an `Arenium` material, float 0.5, piercing 0,
//! concussion 1). A robot's contact with one of them is priced by that
//! component as a hit on the robot's part, with no attacker: `0.5 · M`, where a
//! fixed body's mass 0 counts as an inverse mass of `0.08` and adds no
//! rotation term. Floor and walls alike; a wheel is `Rubber`, which Arenium
//! does nothing to.
//!
//! That component's own `computeDamage` (`0x405090`) first looks the arena's
//! collidable up among the sub-materials the script registered
//! (`SetSubMaterialSound(name, material, factor, sound)`: the hammers, saws,
//! spikes and smash arms). Such a collidable prices the hit at `factor · M`,
//! whatever the victim is made of (a wheel too); only the others go through
//! `FUN_00489470` with the Arenium (docs/29 §12aa).

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

/// [`Impact::attacker`] of a hit the arena landed: the robot listener's
/// branch for a collidable that is not a robot component (`0x414f62`) hands
/// `0x417330` the player `-1` and no component.
pub const NO_ATTACKER: usize = usize::MAX;

/// One hit: a component of `attacker` struck a component of `target`.
#[derive(Clone, Copy, Debug)]
pub struct Impact {
    /// The attacking robot, [`NO_ATTACKER`] for the arena.
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

/// A robot part touching an arena collidable with a sub-material: what the
/// arena's sub-material sound rule reads (`0x405090`,
/// `docs/35-audio-events.md`). The robot-versus-robot [`resolve`] ignores it.
#[derive(Clone, Debug)]
pub struct MaterialContact {
    /// The collidable's `.gmf` body name, as the arena script spells its
    /// sub-material (`SetSubMaterialSound(name, ...)`).
    pub prop: String,
    pub robot: usize,
    pub point: Vec3,
    /// `FUN_0045a5a0` for this contact, the `M` the hit's damage `factor · M`
    /// is made of: both bodies' masses and rotation terms (docs/29 §12aa).
    pub magnitude: f32,
}

/// The contacts of one step between a robot's body and an arena collidable
/// the script gave a sub-material.
///
/// The sound and the damage are one computation: the arena's
/// `computeDamage` (`0x405090`) finds the struck collidable among the
/// registrations `SetSubMaterialSound` made (`0x404cc0`: every collidable of
/// the named `.gmf` body, the static arena's named pieces included), forms
/// `v = factor · M` and both prices the hit with it and, above 0.5, plays the
/// registration's sound (call site `0x4051b0`). So a contact makes a material
/// sound exactly when [`arena_hit`] prices it with a sub-material.
pub fn material_contacts(
    robots: &[Robot],
    arena: &ArenaBodies,
    events: &[ContactEvent],
    props: &MassProps,
) -> Vec<MaterialContact> {
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
        let (robot_side, ab, robot_is_a) = match (
            owner.get(&e.a),
            owner.get(&e.b),
            arena.get(&e.a),
            arena.get(&e.b),
        ) {
            (Some(&o), None, None, Some(ab)) => (o, ab, true),
            (None, Some(&o), Some(ab), None) => (o, ab, false),
            _ => continue,
        };
        let k = if ab.parts.is_empty() {
            0
        } else {
            ab.part_at(e.point)
        };
        if ab.sub_materials.get(k).copied().flatten().is_none() {
            continue;
        }
        let Some((_, m, _)) = arena_record(robots, robot_side, e, robot_is_a, props) else {
            continue;
        };
        out.push(MaterialContact {
            prop: ab.names.get(k).cloned().unwrap_or_default(),
            robot: robot_side.0,
            point: e.point,
            magnitude: m,
        });
    }
    out
}

/// The arena collidable each robot-against-arena contact of this step
/// touched: `(point, robot, the `.gmf` body name)`, for diagnostics
/// (`fidtrace`'s hit rows say which piece struck).
pub fn arena_pieces(
    robots: &[Robot],
    arena: &ArenaBodies,
    events: &[ContactEvent],
) -> Vec<(Vec3, usize, String)> {
    let mut owner: HashMap<BodyHandle, usize> = HashMap::new();
    for (r, robot) in robots.iter().enumerate() {
        for part in robot.parts.iter().filter(|p| p.alive) {
            owner.insert(part.body, r);
        }
    }
    events
        .iter()
        .filter_map(|e| {
            let (r, ab) = match (
                owner.get(&e.a),
                owner.get(&e.b),
                arena.get(&e.a),
                arena.get(&e.b),
            ) {
                (Some(&r), None, None, Some(ab)) | (None, Some(&r), Some(ab), None) => (r, ab),
                _ => return None,
            };
            let k = if ab.parts.is_empty() {
                0
            } else {
                ab.part_at(e.point)
            };
            Some((e.point, r, ab.names.get(k).cloned().unwrap_or_default()))
        })
        .collect()
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
pub fn touched_component(
    robot: &Robot,
    part: usize,
    state: &BodyState,
    p: Vec3,
    tag: u32,
) -> Option<usize> {
    if let Some(i) = tagged_component(robot, part, tag) {
        return Some(i);
    }
    // A collidable the filter has no entry for (a battery, an air tank, a control board) is
    // struck without an event: nothing is priced on it or by it (docs/29 12at).
    if unregistered_piece(robot, part, tag) {
        return None;
    }
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

/// Whether a contact names the component whose collidable made it: the original's
/// event carries the two colliding collidables, one per component
/// (`FUN_00414d80` reads the component off the struck one), where ours finds the
/// component nearest the contact point, which hands a point on the seam of two
/// touching components (the Scout's extender and the fork arm behind it) to the
/// larger. Docs/29 section 12as. `RA2_TAGS=0` is the nearest-box rule.
fn use_tags() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("RA2_TAGS").map_or(true, |v| v != "0"))
}

/// The component `tag` names, if it is an attached member of `part` that has a hull and is
/// priced: the collidables of batteries, air tanks and control boards are not
/// ([`unregistered_piece`]).
fn tagged_component(robot: &Robot, part: usize, tag: u32) -> Option<usize> {
    if tag == crate::backend::NO_TAG || !use_tags() {
        return None;
    }
    let i = tag as usize;
    (robot.rig.parts.get(part)?.members.contains(&i)
        && *robot.attached.get(i)?
        && !robot.rig.components[i].hull_points.is_empty()
        && !unregistered_piece(robot, part, tag))
    .then_some(i)
}

/// Whether `tag` names an attached member of `part` whose collidable has no entry in the
/// event filter: the original's batteries, air tank and control boards (live,
/// `eventreg-sentinel-wideload-compressor__live__000`; none of 2500 priced hits has one as
/// victim or attacker) take contacts, so they hold other bodies off, but raise no event.
/// `RA2_INTERNALS=0` has no such pieces.
fn unregistered_piece(robot: &Robot, part: usize, tag: u32) -> bool {
    if tag == crate::backend::NO_TAG || !use_tags() || !crate::robot::internals_collide() {
        return false;
    }
    let i = tag as usize;
    robot
        .rig
        .parts
        .get(part)
        .is_some_and(|p| p.members.contains(&i))
        && robot.attached.get(i).copied().unwrap_or(false)
        && matches!(robot.rig.components[i].role, Role::Power | Role::Control)
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
/// Registrations are per **collidable**, and every component has its own: the
/// robot set-up (`FUN_00414a60`) walks the robot's components and registers
/// each one's collidables at 1 Hz (`FUN_00414cd0` → `FUN_0045ab90` →
/// `FUN_004ad500(c, 2, 1.0)` → `FUN_0052d850`, which appends an entry to that
/// collidable's list), and a wheel's collidable gets a second 1 Hz entry
/// (`0x507ed6`); a weapon that declares `frequency` registers at that rate
/// instead (`0x502f7a` .. `0x503038`). Live
/// (`eventreg-sentinel-wideload-compressor__live__000`): 48 registrations, one
/// per component collidable and two on each of the eight wheels, all with
/// partner 0, interval 1 s, threshold 0, type 2. A contact point becomes an
/// event if one of its two collidables has a due entry (a full interval since
/// it last let one through; `a`'s list first, then `b`'s) and the bodies'
/// normal relative speed is at least the entry's threshold (`FUN_0052d740`
/// takes it as `|Δv · n|`, so with threshold 0 every point passes this test);
/// the entry that let it through is reset. All other points are dropped before
/// the game prices anything. So each of Wide Load's four spikes is priced up
/// to once a second, not their shaft body once between them.
#[derive(Clone, Debug)]
pub struct EventGate {
    /// Per (body, component): the last time each of its entries let a point
    /// through.
    last: HashMap<(BodyHandle, usize), Vec<Option<f32>>>,
    /// `RA2_PAIR_GATE=1`: at most one event per collidable pair and call (a
    /// step) instead of one per resolver point (docs/29 §12ag, §12ak). The
    /// filter itself is asked for every point (`0x549c30`), but live none of
    /// 1727 firings shared its collidable pair and step with another (ours
    /// 2.5-8 %). Off by default: with the distance query of §12ak it makes the
    /// Compressor's robots end up at the end walls (arena hp 337 / 326 against
    /// 178 / 146 live), through the wheels' callbacks it feeds (`RA2_WHEEL_GATE=1`
    /// gives them a gate without the pair rule: 128 / 149).
    pub pair_gate: bool,
    /// `RA2_PAIR_SINGLE=1` (experiment, off): a collidable pair with two or
    /// more points in the step lets none through. Live (docs/29 section 12ak)
    /// none of 1727 firings was on such a pair, 15 300 pairs were, but the
    /// mechanism is not known.
    pub pair_single: bool,
}

impl EventGate {
    /// A gate with the pair rule set explicitly (the environment otherwise).
    pub fn with_pair_gate(pair_gate: bool) -> Self {
        EventGate {
            last: HashMap::new(),
            pair_gate,
            pair_single: false,
        }
    }
}

impl Default for EventGate {
    fn default() -> Self {
        EventGate {
            last: HashMap::new(),
            pair_gate: std::env::var("RA2_PAIR_GATE").is_ok_and(|v| v != "0" && !v.is_empty()),
            pair_single: std::env::var("RA2_PAIR_SINGLE").is_ok_and(|v| v != "0" && !v.is_empty()),
        }
    }
}

/// A component's registrations with the event filter: the interval of each
/// entry (see [`EventGate`]).
///
/// Batteries, air tanks, control boards and SmartZones have no collidable and
/// no registration (live: the five components of the two robots without one
/// are their batteries, Wide Load's air tank and their control boards; the
/// SmartZones' phantoms are not registered either).
pub fn event_registrations(c: &crate::rig::Component) -> Vec<f32> {
    match (c.role, c.weapon.and_then(|w| w.frequency)) {
        (Role::Power | Role::Control, _) => Vec::new(),
        (Role::Weapon, Some(f)) => vec![1.0 / f],
        (Role::Wheel, _) => vec![1.0, 1.0],
        _ => vec![1.0],
    }
}

/// The registered component of `part` nearest the point (as
/// [`touched_component`], over the components that have a collidable).
fn registered_component(
    robot: &Robot,
    part: usize,
    state: &BodyState,
    p: Vec3,
    tag: u32,
) -> Option<usize> {
    if let Some(i) = tagged_component(robot, part, tag) {
        if !robot.rig.smart_zones.iter().any(|z| z.component == i)
            && !event_registrations(&robot.rig.components[i]).is_empty()
        {
            return Some(i);
        }
    }
    if unregistered_piece(robot, part, tag) {
        return None;
    }
    let frame = Mat4::from_rotation_translation(state.rotation, state.position);
    let zones: Vec<usize> = robot.rig.smart_zones.iter().map(|z| z.component).collect();
    let mut best: Option<(usize, f32)> = None;
    for &i in &robot.rig.parts[part].members {
        let c = &robot.rig.components[i];
        if !robot.attached[i]
            || c.hull_points.is_empty()
            || zones.contains(&i)
            || event_registrations(c).is_empty()
        {
            continue;
        }
        let world = frame * robot.rig.part_local[i];
        let local = world.inverse().transform_point3(p);
        let d = (local - local.clamp(c.min, c.max)).length_squared();
        if best.is_none_or(|(_, bd)| d < bd) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i)
}

/// How the arena prices what it strikes: the component the arena manager
/// registers its collidables under (`FUN_00415140`), or an `AddXtra` prop's
/// own (`FUN_00403aa0`). Where the struck collidable has no sub-material its
/// `computeDamage` (`0x405090`) falls through to every component's
/// (`FUN_00489470`), so such a contact is priced as a hit by it.
#[derive(Clone, Debug, PartialEq)]
pub struct ArenaStriker {
    /// Material class, lower case.
    pub material: String,
    /// The material's float (`material + 4`).
    pub scale: f32,
    pub piercing: f32,
    pub concussion: f32,
}

impl ArenaStriker {
    /// The arena manager's own component: an `Arenium` component with the
    /// constructor's defaults (piercing 0, concussion 1; see docs/29 §12o).
    pub fn arena() -> Self {
        ArenaStriker {
            material: "arenium".to_string(),
            scale: ARENIUM_SCALE,
            piercing: 0.0,
            concussion: 1.0,
        }
    }

    /// An `AddXtra` prop's component: the script's material by name, with the
    /// constructor's defaults.
    pub fn of_material(material: &str) -> Self {
        let class = material_class(Some(material)).to_ascii_lowercase();
        let scale = ra2fmt::component::material_class_scale(&class).unwrap_or(0.1);
        ArenaStriker {
            material: class,
            scale,
            piercing: 0.0,
            concussion: 1.0,
        }
    }

    fn striker(&self) -> Striker<'_> {
        Striker {
            piercing: self.piercing,
            concussion: self.concussion,
            scale: self.scale,
            material: &self.material,
        }
    }
}

/// `Arenium`'s material float (`0x402c50`).
pub const ARENIUM_SCALE: f32 = 0.5;

/// One arena body of the physics world.
#[derive(Clone, Debug)]
pub struct ArenaBody {
    pub striker: ArenaStriker,
    /// The original's collidables this body stands for, as world triangles:
    /// ours merges every fixed arena body into one static body, the original
    /// registers each with the event filter. Empty for a body that is one
    /// collidable (a movable arena body, a prop).
    pub parts: Vec<Vec<[Vec3; 3]>>,
    /// The `.gmf` rigid body name of each collidable: one per [`parts`]
    /// entry, or the one name of a body that is one collidable.
    ///
    /// [`parts`]: ArenaBody::parts
    pub names: Vec<String>,
    /// The factor of the sub-material the arena's script gave each of
    /// [`names`] (`SetSubMaterialSound(name, material, factor, sound)`), if
    /// any; see [`ArenaBody::sub_material_at`].
    ///
    /// [`names`]: ArenaBody::names
    pub sub_materials: Vec<Option<f32>>,
}

impl ArenaBody {
    /// A body with no sub-materials yet: `names` are its collidables'.
    pub fn new(striker: ArenaStriker, parts: Vec<Vec<[Vec3; 3]>>, names: Vec<String>) -> Self {
        let sub_materials = vec![None; names.len()];
        ArenaBody {
            striker,
            parts,
            names,
            sub_materials,
        }
    }

    /// Give each collidable named in `registered` (`(name, factor)`, as the
    /// script registered them, a later one for the same name winning) its
    /// sub-material factor.
    pub fn set_sub_materials(&mut self, registered: &[(String, f32)]) {
        self.sub_materials = self
            .names
            .iter()
            .map(|n| {
                registered
                    .iter()
                    .rev()
                    .find(|(r, _)| r.eq_ignore_ascii_case(n))
                    .map(|&(_, f)| f)
            })
            .collect();
    }

    /// The sub-material factor of the collidable the world point `p` lies on.
    ///
    /// The arena component's `computeDamage` (`0x405090`, slot `+0x18` of
    /// `Arena_callback`'s vtable `0x6aa52c`) looks the arena side's collidable
    /// up in the sub-materials the script registered (`0x404cc0`: a material
    /// made by name, `FUN_004893c0`, its float set to the factor, for every
    /// collidable of the named body). When it finds one the hit is
    /// `factor · M` (`FUN_0045a5a0`), whatever the victim is made of; only
    /// otherwise does it fall through to every component's `computeDamage`
    /// (`FUN_00489470`) with its Arenium (docs/29 §12aa).
    pub fn sub_material_at(&self, p: Vec3) -> Option<f32> {
        if self.parts.is_empty() {
            return self.sub_materials.first().copied().flatten();
        }
        self.sub_materials.get(self.part_at(p)).copied().flatten()
    }

    /// The collidable of this body the world point `p` lies on.
    pub fn part_at(&self, p: Vec3) -> usize {
        let mut best = (0, f32::MAX);
        for (i, tris) in self.parts.iter().enumerate() {
            for t in tris {
                let d = point_triangle_distance_sq(p, t);
                if d < best.1 {
                    best = (i, d);
                }
            }
        }
        best.0
    }
}

/// The arena's bodies, by physics handle: what strikes a robot that touches
/// them, and their registrations with the event filter (one 1 s entry per
/// collidable: live, the box's four and the Compressor's nine arena
/// collidables, `0x415227`).
pub type ArenaBodies = HashMap<BodyHandle, ArenaBody>;

/// The interval of an arena collidable's one registration.
pub const ARENA_INTERVAL: f32 = 1.0;

/// Squared distance from `p` to the triangle `t`.
fn point_triangle_distance_sq(p: Vec3, t: &[Vec3; 3]) -> f32 {
    let [a, b, c] = *t;
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return ap.length_squared();
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return bp.length_squared();
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return (p - (a + ab * v)).length_squared();
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return cp.length_squared();
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return (p - (a + ac * w)).length_squared();
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (p - (b + (c - b) * w)).length_squared();
    }
    let denom = 1.0 / (va + vb + vc);
    let (v, w) = (vb * denom, vc * denom);
    (p - (a + ab * v + ac * w)).length_squared()
}

impl EventGate {
    /// The events of one step that get through, in order (robots only).
    pub fn admit(
        &mut self,
        robots: &[Robot],
        events: Vec<ContactEvent>,
        now: f32,
    ) -> Vec<ContactEvent> {
        self.admit_with(robots, &ArenaBodies::new(), events, now)
    }

    /// [`EventGate::admit`] with the arena's registrations too: an arena
    /// collidable's one entry lets a point through as a robot component's do.
    pub fn admit_with(
        &mut self,
        robots: &[Robot],
        arena: &ArenaBodies,
        events: Vec<ContactEvent>,
        now: f32,
    ) -> Vec<ContactEvent> {
        let mut owner: HashMap<BodyHandle, (usize, usize)> = HashMap::new();
        for (r, robot) in robots.iter().enumerate() {
            for (p, part) in robot.parts.iter().enumerate() {
                if part.alive {
                    owner.insert(part.body, (r, p));
                }
            }
        }
        let mut out = Vec::new();
        // The filter is asked once per collidable pair and step, not once per
        // resolver point: live the same pair is priced twice in one step for
        // about 1 % of its hits (ours 2.5-8 % with a point each, where several
        // points of one pair would each find another due entry), and its two
        // entries fire a step apart (docs/29 §12ag).
        let mut passed: std::collections::HashSet<[(BodyHandle, usize); 2]> =
            std::collections::HashSet::new();
        let pair_of = |e: &ContactEvent| {
            let side = |body: BodyHandle, state: &BodyState| -> (BodyHandle, usize) {
                if let Some(ab) = arena.get(&body) {
                    return (body, ab.part_at(e.point));
                }
                let tag = if body == e.a { e.tag_a } else { e.tag_b };
                let c = owner
                    .get(&body)
                    .and_then(|&(r, p)| registered_component(&robots[r], p, state, e.point, tag));
                (body, c.unwrap_or(usize::MAX))
            };
            let mut key = [side(e.a, &e.state_a), side(e.b, &e.state_b)];
            key.sort_unstable();
            key
        };
        let mut points: HashMap<[(BodyHandle, usize); 2], usize> = HashMap::new();
        if self.pair_single {
            for e in &events {
                *points.entry(pair_of(e)).or_default() += 1;
            }
        }
        for e in events {
            let side = |body: BodyHandle, state: &BodyState| -> (BodyHandle, usize) {
                if let Some(ab) = arena.get(&body) {
                    return (body, ab.part_at(e.point));
                }
                let tag = if body == e.a { e.tag_a } else { e.tag_b };
                let c = owner
                    .get(&body)
                    .and_then(|&(r, p)| registered_component(&robots[r], p, state, e.point, tag));
                (body, c.unwrap_or(usize::MAX))
            };
            let mut key = [side(e.a, &e.state_a), side(e.b, &e.state_b)];
            key.sort_unstable();
            if self.pair_gate && passed.contains(&key) {
                continue;
            }
            if self.pair_single && points.get(&key).is_some_and(|&n| n > 1) {
                continue;
            }
            let mut through = false;
            for (body, state, tag) in [(e.a, &e.state_a, e.tag_a), (e.b, &e.state_b, e.tag_b)] {
                if let Some(ab) = arena.get(&body) {
                    let last = self.last.entry((body, ab.part_at(e.point))).or_default();
                    last.resize(1, None);
                    if last[0].is_none_or(|t| now - t >= ARENA_INTERVAL - 1e-4) {
                        last[0] = Some(now);
                        through = true;
                        break;
                    }
                    continue;
                }
                let Some(&(r, p)) = owner.get(&body) else {
                    continue;
                };
                let robot = &robots[r];
                let Some(c) = registered_component(robot, p, state, e.point, tag) else {
                    continue;
                };
                let iv = event_registrations(&robot.rig.components[c]);
                let last = self.last.entry((body, c)).or_default();
                last.resize(iv.len(), None);
                if let Some(k) =
                    (0..iv.len()).find(|&k| last[k].is_none_or(|t| now - t >= iv[k] - 1e-4))
                {
                    last[k] = Some(now);
                    through = true;
                    break;
                }
            }
            if through {
                passed.insert(key);
                out.push(e);
            }
        }
        out
    }
}

/// The normals the wheels' own collision callbacks keep from the events the
/// filter let through ([`MagicAction::contact`](crate::drive::MagicAction::contact)):
/// for every event one of whose bodies is a robot part whose registered
/// component at the point is a wheel, `(robot, part, normal toward the wheel)`.
/// The game's listener hands an event to the callbacks of both its collidables,
/// so a wheel hears the events its own entries let through and the ones its
/// partner's did (live, the wheels' callbacks run in pairs a step apart once a
/// second, their two entries, with an odd one between when the floor's entry
/// let a point through).
pub fn wheel_contacts(robots: &[Robot], events: &[ContactEvent]) -> Vec<(usize, usize, Vec3)> {
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
        // `normal` points from `a` toward `b`.
        for (body, state, toward, tag) in [
            (e.a, &e.state_a, -e.normal, e.tag_a),
            (e.b, &e.state_b, e.normal, e.tag_b),
        ] {
            let Some(&(r, p)) = owner.get(&body) else {
                continue;
            };
            let robot = &robots[r];
            let Some(c) = registered_component(robot, p, state, e.point, tag) else {
                continue;
            };
            if robot.rig.components[c].role == Role::Wheel {
                out.push((r, p, toward));
            }
        }
    }
    out
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
    resolve_with(robots, events, cfg, &MassProps::new(), &ArenaBodies::new())
}

/// [`resolve`] with the physics backend's own mass properties ([`mass_props`])
/// and the arena's bodies: a robot's contact with one of those is priced as a
/// hit on the robot by the arena's component ([`arena_hit`]).
pub fn resolve_with(
    robots: &[Robot],
    events: &[ContactEvent],
    cfg: &CombatConfig,
    props: &MassProps,
    arena: &ArenaBodies,
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
        let robot_side = |body: BodyHandle| owner.get(&body).copied();
        match (
            robot_side(e.a),
            robot_side(e.b),
            arena.get(&e.a),
            arena.get(&e.b),
        ) {
            (Some(r), None, None, Some(ab)) => {
                out.extend(arena_hit(robots, r, ab, e, true, cfg, props));
                continue;
            }
            (None, Some(r), Some(ab), None) => {
                out.extend(arena_hit(robots, r, ab, e, false, cfg, props));
                continue;
            }
            _ => {}
        }
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
            touched_component(robot_a, pa, &e.state_a, e.point, e.tag_a),
            touched_component(robot_b, pb, &e.state_b, e.point, e.tag_b),
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

/// The hit of one contact between robot part `(r, part)` and an arena body
/// (`robot_is_a`: the robot is the event's `a`).
///
/// The robot listener (`0x414d80`) gets the event on both collidables. On the
/// robot's, the other side is not a robot component (`0x414e7d` → `0x414fb2`):
/// the robot's part prices the arena and nothing is applied. On the arena's,
/// the arena's component prices the robot's part through the same
/// `computeDamage` (`FUN_00489470`) and the hit reaches `0x417330` with the
/// player `-1` and no attacking component (`0x414f62`): the same `>= 10` gate,
/// the victim component's own damage (`FUN_00489620`), no score, no "last
/// damage done". The record is the usual one: `rec0` along the normal, the
/// robot body's Havok mass, centre and inverse inertia, the arena body's own.
/// A fixed body has mass 0 and no inverse inertia, which `FUN_0045a5a0` counts
/// as an inverse mass of `0.08` (docs/29 §12o). A collidable with a
/// sub-material is priced at its factor times `M` instead
/// ([`ArenaBody::sub_material_at`], docs/29 §12aa).
#[allow(clippy::too_many_arguments)]
fn arena_record(
    robots: &[Robot],
    (r, part): (usize, usize),
    e: &ContactEvent,
    robot_is_a: bool,
    props: &MassProps,
) -> Option<(f32, f32, usize)> {
    let robot = &robots[r];
    let (body_r, state_r, body_x, state_x) = if robot_is_a {
        (e.a, &e.state_a, e.b, &e.state_b)
    } else {
        (e.b, &e.state_b, e.a, &e.state_a)
    };
    let dyn_r = dynamics_of(robot, part, body_r, state_r, props);
    let n = e.normal.normalize_or_zero();
    if n == Vec3::ZERO || dyn_r.mass <= 0.0 {
        return None;
    }
    // The arena body: a movable one has the backend's mass properties, a
    // fixed one none (mass 0, no rotation term).
    let (mass_x, com_x, inv_x) = match props.get(&body_x) {
        Some(&(mass, com_local, inv_local)) => {
            let rot = Mat3::from_quat(state_x.rotation);
            (
                mass,
                state_x.position + state_x.rotation * com_local,
                rot * inv_local * rot.transpose(),
            )
        }
        None => (0.0, state_x.position, Mat3::ZERO),
    };
    let v_r = point_velocity(
        state_r.linear_velocity,
        state_r.angular_velocity,
        dyn_r.com,
        e.point,
    );
    let v_x = point_velocity(
        state_x.linear_velocity,
        state_x.angular_velocity,
        com_x,
        e.point,
    );
    let rec0 = (v_r - v_x).dot(n).abs();
    let m = magnitude(
        rec0,
        dyn_r.mass,
        mass_x,
        angular_term(e.point - dyn_r.com, n, dyn_r.inv_inertia),
        angular_term(e.point - com_x, n, inv_x),
    );
    let tag = if robot_is_a { e.tag_a } else { e.tag_b };
    let component = touched_component(robot, part, state_r, e.point, tag)?;
    Some((rec0, m, component))
}

/// The hit of one contact between robot part `(r, part)` and an arena body
/// (`robot_is_a`: the robot is the event's `a`): see [`arena_record`].
fn arena_hit(
    robots: &[Robot],
    (r, part): (usize, usize),
    arena: &ArenaBody,
    e: &ContactEvent,
    robot_is_a: bool,
    cfg: &CombatConfig,
    props: &MassProps,
) -> Option<Impact> {
    let robot = &robots[r];
    let (rec0, m, component) = arena_record(robots, (r, part), e, robot_is_a, props)?;
    let n = e.normal.normalize_or_zero();
    let damage = match arena.sub_material_at(e.point) {
        Some(factor) => factor * m,
        None => hit_damage(
            arena.striker.striker(),
            material_class(robot.rig.components[component].material.as_deref()),
            rec0,
            m,
            cfg,
        ),
    };
    (damage > 0.0 && damage.is_finite()).then_some(Impact {
        attacker: NO_ATTACKER,
        target: r,
        component,
        by: 0,
        damage,
        point: e.point,
        // `n` points from `a` toward `b`: out of the robot when it is `a`.
        normal: if robot_is_a { n } else { -n },
        speed: rec0,
    })
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
