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

    /// Whether a convex body — `points` in the frame `world` puts in the world
    /// (a component's collision pieces) — comes within `tolerance` of the zone.
    ///
    /// The original's SmartZone is a Havok phantom: a body is in it when the
    /// collision detection finds it within the world's tolerance (0.1). This is
    /// a separating-axis test on the zone's three axes and on 24 directions
    /// around its vertical, which is exact for the upright boxes the stock
    /// zones are and within a few millimetres for the robots' convex pieces. A
    /// bounding sphere, or even a component's box, is too generous: ALARM's
    /// chassis box grazed the Octagon's `hellzone2` as it drove past where the
    /// original never raises the hellraiser (live, `HellRaiser.timer` never
    /// reset); its hull, chamfered at the corners, stays clear.
    pub fn touches_points(&self, world: Mat4, points: &[Vec3], tolerance: f32) -> bool {
        if points.is_empty() {
            return false;
        }
        let to_zone = self.inv * world;
        let local: Vec<Vec3> = points
            .iter()
            .map(|p| to_zone.transform_point3(*p))
            .collect();
        let centre = (self.min + self.max) * 0.5;
        let half = (self.max - self.min) * 0.5;
        let mut axes = vec![Vec3::X, Vec3::Y, Vec3::Z];
        for k in 1..24 {
            let a = k as f32 * std::f32::consts::PI / 24.0;
            axes.push(Vec3::new(a.cos(), 0.0, a.sin()));
        }
        axes.iter().all(|l| {
            let c = centre.dot(*l);
            let r = half.x * l.x.abs() + half.y * l.y.abs() + half.z * l.z.abs();
            let (lo, hi) = local.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
                let d = p.dot(*l);
                (lo.min(d), hi.max(d))
            });
            lo <= c + r + tolerance && hi >= c - r - tolerance
        })
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
    // The caller hands us whatever the match was given, which is normally the
    // arena's *script*; the hinges are in the model it names.
    let model = ra2_render::arena_model_path(arena_path)
        .ok()
        .flatten()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| arena_path.to_string());
    if let Ok(bytes) = std::fs::read(&model) {
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

/// The travel a hinge is given, from the model's own `*ANGLE_LIMITS`.
///
/// Jolt measures a hinge's angle from where the two frames coincide and insists
/// on `-π ≤ min ≤ 0 ≤ max ≤ π`. The models state theirs in their own frame, with
/// the two sides exported at the hinge's *lower stop* — the Octagon's hellraiser
/// arms ship `[1°, 70°]`, which Jolt refuses outright — so the travel is taken
/// from that stop: `[0, max - min]`. The Compressor's smash arms already say
/// `[0, 0.27]` and are unchanged. A range a full turn wide is Jolt's "no limits".
pub fn hinge_limits(model: [f32; 2]) -> [f32; 2] {
    use std::f32::consts::PI;
    let (lo, hi) = (model[0].min(model[1]), model[0].max(model[1]));
    if hi - lo >= 2.0 * PI {
        return [-PI, PI];
    }
    let (lo, hi) = if lo > 0.0 { (0.0, hi - lo) } else { (lo, hi) };
    [lo.clamp(-PI, 0.0), hi.clamp(0.0, PI)]
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
    /// The drive strength and friction the joint currently carries, so the
    /// script's changes reach the motor only when it actually changes its mind.
    pub strength: f32,
    pub friction: f32,
    /// The joint's own travel, for `SetAutoLock`'s "has it arrived" test.
    pub limits: [f32; 2],
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    /// The models state their hinge travel in their own frame and Jolt insists on
    /// a range containing zero, so the travel is taken from the model's lower
    /// stop. The Octagon's hellraiser arms ship `[1°, 70°]`, which Jolt refuses
    /// outright; the Compressor's smash arms already say `[0, 0.27]`.
    /// The Octagon's `hellzone2` (x 3.1..4.0, z -1.4..0.9) and ALARM's chassis
    /// (about 1.5 x 1.1 m) at (2.56, -2.28) heading the diagonal, as it drives
    /// past live without raising the hellraiser: its bounding sphere reached the
    /// zone, its box does not.
    #[test]
    fn a_zone_is_touched_by_a_body_not_its_bounding_sphere() {
        let zone = Zone {
            name: "hellzone2".into(),
            id: 2,
            inv: Mat4::IDENTITY,
            min: Vec3::new(3.1, -0.1, -1.4),
            max: Vec3::new(4.0, 0.6, 0.9),
        };
        // A 1.5 x 0.4 x 1.1 m box.
        let (min, max) = (Vec3::new(-0.75, -0.2, -0.55), Vec3::new(0.75, 0.2, 0.55));
        let corners: Vec<Vec3> = (0..8)
            .map(|k| {
                Vec3::new(
                    if k & 1 == 0 { min.x } else { max.x },
                    if k & 2 == 0 { min.y } else { max.y },
                    if k & 4 == 0 { min.z } else { max.z },
                )
            })
            .collect();
        let at = |x: f32, z: f32, yaw: f32| {
            Mat4::from_rotation_translation(glam::Quat::from_rotation_y(yaw), Vec3::new(x, 0.2, z))
        };
        // Driving past diagonally, a metre off: the sphere reaches, the body
        // does not.
        let past = at(2.56, -2.28, -0.77);
        assert!(zone.touches(
            past.transform_point3(Vec3::ZERO),
            ((max - min) * 0.5).length()
        ));
        assert!(!zone.touches_points(past, &corners, 0.1));
        // On the plate it is in.
        assert!(zone.touches_points(at(3.5, -0.2, 0.3), &corners, 0.1));
        // Within the tolerance of a face counts; beyond it does not.
        assert!(zone.touches_points(at(2.31, 0.0, 0.0), &corners, 0.1));
        assert!(!zone.touches_points(at(2.2, 0.0, 0.0), &corners, 0.1));
        assert!(!zone.touches_points(at(3.5, 0.0, 0.0), &[], 0.1));
    }

    #[test]
    fn hinge_travel_is_taken_from_the_models_lower_stop() {
        assert_eq!(hinge_limits([0.0, 0.27052602]), [0.0, 0.27052602]);

        let hellraiser = hinge_limits([1.0f32.to_radians(), 70.0f32.to_radians()]);
        assert_eq!(hellraiser[0], 0.0);
        assert!(
            (hellraiser[1] - 69.0f32.to_radians()).abs() < 1e-5,
            "{hellraiser:?}"
        );

        // A full turn is Jolt's "no limits", and nothing can express more.
        assert_eq!(hinge_limits([-PI, PI]), [-PI, PI]);
        assert_eq!(hinge_limits([-4.0, 4.0]), [-PI, PI]);
        assert_eq!(hinge_limits([-PI, 5.0]), [-PI, PI]);
    }
}
