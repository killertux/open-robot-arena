//! Arena hazards: the SmartZones the scripts registered and the joints they built.
//!
//! An arena's hazards come from two places, and both are already in the game's
//! data:
//!
//! * The **zones** are trigger volumes the model carries (`saw_zone`,
//!   `spike_zone1`, …) and the script names with `RegisterZone(name, id)`. The
//!   engine watches them and tells the script when a robot enters or leaves.
//! * The **joints** are the moving parts — the spikes, the saw, the smash arms.
//!   Some the scripts create themselves (`AddPrismatic('arena_collision',
//!   'spikes1', 0, 1, 0, 0, .8, 0)`) and some the model already declares
//!   (`GetHinge('Hinge01')`), and the script then drives them with
//!   `SetPowerSettings`/`Lock`/`SetDirection`/`ApplyForce`.

use std::collections::BTreeMap;

use glam::{Mat4, Vec3};

use crate::backend::{BodyHandle, JointHandle};

/// A SmartZone, as a box in its own frame.
///
/// The stock zones are eight-vertex boxes, so testing in the zone's frame is
/// exact for them and a fair approximation for anything else.
#[derive(Clone, Debug)]
pub struct Zone {
    pub name: String,
    /// The id the script registered it under (`ZoneEvent`'s second argument).
    pub id: i32,
    pub inv: Mat4,
    pub min: Vec3,
    pub max: Vec3,
}

impl Zone {
    /// Whether a sphere of `radius` around `centre` touches the zone.
    pub fn touches(&self, centre: Vec3, radius: f32) -> bool {
        let p = self.inv.transform_point3(centre);
        let r = Vec3::splat(radius);
        (p.cmpge(self.min - r) & p.cmple(self.max + r)).all()
    }
}

/// Resolve the zones a script registered against the arena's own geometry.
///
/// A name the model does not carry is skipped: the scripts register zones for
/// hazards that only some revisions of an arena ship.
pub fn zones(arena_path: &str, registered: &BTreeMap<String, i32>) -> Vec<Zone> {
    if registered.is_empty() {
        return Vec::new();
    }
    let Ok(Some(havok)) = ra2_render::collect_havok(arena_path) else {
        return Vec::new();
    };
    registered
        .iter()
        .filter_map(|(name, id)| {
            let body = havok.bodies.iter().find(|b| &b.name == name)?;
            if body.points.is_empty() {
                return None;
            }
            let mut min = Vec3::splat(f32::MAX);
            let mut max = Vec3::splat(f32::MIN);
            for p in &body.points {
                min = min.min(*p);
                max = max.max(*p);
            }
            Some(Zone {
                name: name.clone(),
                id: *id,
                inv: body.transform.inverse(),
                min,
                max,
            })
        })
        .collect()
}

/// A hinge the model declares, as the engine needs it.
#[derive(Clone, Debug)]
pub struct ModelHinge {
    pub pivot: Vec3,
    pub axis: Vec3,
    /// The two objects it joins; the first is usually fixed scenery.
    pub body_a: String,
    pub body_b: String,
    /// Degree limits in radians, when the model limits it.
    pub limits: Option<[f32; 2]>,
    pub friction: f32,
}

/// The model's own hinges, by name (`GetHinge`).
pub fn hinges(arena_path: &str) -> BTreeMap<String, ModelHinge> {
    let mut out = BTreeMap::new();
    if let Ok(bytes) = std::fs::read(arena_path) {
        if let Ok(scene) = ra2fmt::gmi::parse(&bytes) {
            for h in &scene.hinges {
                out.insert(
                    h.name.clone(),
                    ModelHinge {
                        pivot: Vec3::from(h.pivot),
                        axis: Vec3::from(h.axis),
                        body_a: h.body_a.clone(),
                        body_b: h.body_b.clone(),
                        limits: h.limited.then_some(h.limits),
                        friction: h.friction,
                    },
                );
            }
        }
    }
    out
}

/// The travel a slider is given, in the rest-pose-relative form Jolt wants.
///
/// The scripts write the limits unordered — `box.py` asks for `(neutral, 0.8,
/// 0)`, and the spikes rise 0.8 m — so the engine takes the min and the max,
/// relative to the neutral pose the joint is created at.
pub fn slider_limits(neutral: f32, lower: f32, upper: f32) -> [f32; 2] {
    let (a, b) = (lower.min(upper), lower.max(upper));
    let lo = a - neutral;
    let hi = b - neutral;
    // Jolt requires `min <= 0 <= max`; the rest pose is the neutral one.
    [lo.min(0.0), hi.max(0.0)]
}

/// The settings a hazard drives its joint with, as the engine reads them.
///
/// Shared with the script's controller object: the script mutates one and the
/// engine reads it every step.
pub type SharedDrive = std::sync::Arc<std::sync::Mutex<ra2script::Drive>>;

/// A hazard joint the engine has built, with the settings it is driven by.
pub struct LiveJoint {
    pub handle: JointHandle,
    /// The part that moves; one-shot forces are applied to it.
    pub body: BodyHandle,
    pub slider: bool,
    /// The joint's axis in world space, for those forces.
    pub axis: Vec3,
    pub drive: SharedDrive,
}
