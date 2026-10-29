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
        box_touches_points(self.inv * world, self.min, self.max, points, tolerance)
    }
}

/// Whether the convex hull of `points` (carried into the box's frame by
/// `to_box`) overlaps the box `min..max` within `tolerance`: separating axes
/// along the box's own axes and 24 horizontal directions. Exact for the box's
/// faces and within a few millimetres for convex pieces.
pub fn box_touches_points(
    to_box: Mat4,
    min: Vec3,
    max: Vec3,
    points: &[Vec3],
    tolerance: f32,
) -> bool {
    if points.is_empty() {
        return false;
    }
    let local: Vec<Vec3> = points.iter().map(|p| to_box.transform_point3(*p)).collect();
    let centre = (min + max) * 0.5;
    let half = (max - min) * 0.5;
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

/// Whether a collision piece is in a zone the way Havok's phantom finds it.
///
/// The phantom's list (`det + 0x100`, cleared with the contacts every
/// detection, `0x542660`) only takes a pair whose closest-point query comes
/// out *negative*: the convex agent (`0x544820`) and the convex-mesh agent
/// (`0x545b50`) push a pair with a phantom on either side there when the GJK
/// distance is below zero, and report nothing for one merely within the
/// tolerance. The convex-mesh agent runs that query per *triangle* the MOPP
/// finds near the convex piece, so a mesh piece (a chassis, a
/// `SIMULATION_GEOMETRY 6` part) is in the zone only while one of its
/// triangles crosses it: a zone wholly inside a mesh body's volume sees
/// nothing of it. A convex piece is its hull.
pub fn box_touches_piece(
    to_box: Mat4,
    min: Vec3,
    max: Vec3,
    points: &[Vec3],
    mesh: Option<&[[u32; 3]]>,
) -> bool {
    match mesh {
        Some(tris) if !tris.is_empty() => box_touches_triangles(to_box, min, max, points, tris),
        _ => box_touches_points(to_box, min, max, points, 0.0),
    }
}

/// Whether any of the triangles (indices into `points`, carried into the
/// box's frame by `to_box`) overlaps the box `min..max`: the separating-axis
/// test on the box's three axes, the triangle's normal and the nine edge
/// crossings, exact for a triangle against a box.
pub fn box_touches_triangles(
    to_box: Mat4,
    min: Vec3,
    max: Vec3,
    points: &[Vec3],
    tris: &[[u32; 3]],
) -> bool {
    let centre = (min + max) * 0.5;
    let half = (max - min) * 0.5;
    let local: Vec<Vec3> = points
        .iter()
        .map(|p| to_box.transform_point3(*p) - centre)
        .collect();
    let separated = |axis: Vec3, v: &[Vec3; 3]| -> bool {
        if axis.length_squared() < 1e-12 {
            return false;
        }
        let r = half.x * axis.x.abs() + half.y * axis.y.abs() + half.z * axis.z.abs();
        let d = [axis.dot(v[0]), axis.dot(v[1]), axis.dot(v[2])];
        let lo = d[0].min(d[1]).min(d[2]);
        let hi = d[0].max(d[1]).max(d[2]);
        lo > r || hi < -r
    };
    tris.iter().any(|t| {
        let Some(v) = (|| {
            Some([
                *local.get(t[0] as usize)?,
                *local.get(t[1] as usize)?,
                *local.get(t[2] as usize)?,
            ])
        })() else {
            return false;
        };
        let e = [v[1] - v[0], v[2] - v[1], v[0] - v[2]];
        if [Vec3::X, Vec3::Y, Vec3::Z]
            .iter()
            .any(|&a| separated(a, &v))
        {
            return false;
        }
        if separated(e[0].cross(e[1]), &v) {
            return false;
        }
        for a in [Vec3::X, Vec3::Y, Vec3::Z] {
            for ed in e {
                if separated(a.cross(ed), &v) {
                    return false;
                }
            }
        }
        true
    })
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

/// How an arena hinge's controller and Havok's hinge see the joint ours
/// builds between the motionless anchor (`a`) and the part that swings (`b`),
/// whose angle grows with `(ω_b - ω_a) · axis`, the model's `*SPIN_AXIS` in
/// the world (docs/29 section 12z).
///
/// * The controller's angle (`0x481190`, what its limits `+0x70`/`+0x74` and
///   its "arrived" test read) grows the other way when the part that swings is
///   Havok's body B (the model's `*BODY2`, `0x4a7340` → `0x5257a0(BODY1,
///   BODY2)`): `sense` is `-1` then, `+1` when it is body A (the Parking
///   Lot's trap doors, `*BODY1`).
/// * The motor (`0x481450`): `trunc(p · 0.01 · S)`, negated when any
///   component of the axis in body B's frame (`+0x60..+0x68`) is below `-0.5`
///   (`axis_flip`), drives `(ω_B - ω_A) · axis` to that value. Live
///   (`hazard-octagon__live__000`, `hazard-compressor__live__000`): the
///   hellraisers (axis `(0, 0, -1)` in B) rise with `ω_z` +25 rad/s for
///   `p = 100`, the hammers (`(-1, 0, 0)` in B) swing toward the centre, the
///   Compressor's right arms (`(0, 1, 0)`, speed `-95`) and left arms
///   (`(0, -1, 0)`, `95`) close alike, each with its controller angle
///   growing from 0 to its upper limit.
///
/// Returns `(sense, motor)`: the controller's angle is `sense ·` ours, and
/// ours drives at `motor · trunc(p · 0.01 · S)`.
pub fn arena_hinge_sense(moving_is_b: bool, axis_in_b: Vec3) -> (f32, f32) {
    let flip = if axis_in_b.x < -0.5 || axis_in_b.y < -0.5 || axis_in_b.z < -0.5 {
        -1.0
    } else {
        1.0
    };
    if moving_is_b {
        (-1.0, flip)
    } else {
        (1.0, -flip)
    }
}

/// An arena hinge's travel in ours ([`arena_hinge_sense`]): the model's
/// `*ANGLE_LIMITS` are the controller's (and Havok's) angles, 0 where the
/// model was exported; ours measures from the same pose, with `sense`. The
/// rest pose is kept inside the travel: live the hellraisers lie at a
/// controller angle of -0.013 to 0.006 against their 1 degree lower limit.
/// An unlimited hinge is free.
pub fn arena_hinge_limits(model: Option<[f32; 2]>, sense: f32) -> [f32; 2] {
    use std::f32::consts::PI;
    let Some([lo, hi]) = model else {
        return [-PI, PI];
    };
    let (a, b) = (sense * lo, sense * hi);
    [a.min(b).clamp(-PI, 0.0), a.max(b).clamp(0.0, PI)]
}

/// The travel a slider is given: `AddPrismatic`'s two stops, as the original
/// keeps them (`0x4b47b0`: the first in the controller's `+0x6c`, the second in
/// `+0x70`, the joint's travel `[min, max]` of the two in `+0x64`/`+0x68`),
/// measured from the pose the joint is created in. Jolt also requires `min <=
/// 0 <= max`, which every stock arena's stops satisfy.
pub fn slider_limits(first: f32, second: f32) -> [f32; 2] {
    [first.min(second).min(0.0), first.max(second).max(0.0)]
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
    /// A slider's stops: where a negative direction drives it (the
    /// controller's `+0x6c`) and where a positive one does (`+0x70`). A
    /// hinge's: its controller's limits (`+0x70`, `+0x74`), in the
    /// controller's own angle.
    pub stops: [f32; 2],
    /// A hinge's controller angle is `sense ·` ours, and its motor drives
    /// ours at `motor ·` the controller's velocity ([`arena_hinge_sense`]);
    /// `1` for a slider.
    pub sense: f32,
    pub motor: f32,
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

    /// Havok's phantom sees a mesh piece only where one of its triangles
    /// crosses the zone (`0x545b50`): a zone wholly inside a closed mesh sees
    /// nothing of it, while the same piece as a convex hull fills the zone.
    #[test]
    fn a_zone_inside_a_mesh_body_sees_none_of_its_triangles() {
        let (lo, hi) = (Vec3::splat(-1.0), Vec3::splat(1.0));
        let pts: Vec<Vec3> = (0..8)
            .map(|k| {
                Vec3::new(
                    if k & 1 == 0 { lo.x } else { hi.x },
                    if k & 2 == 0 { lo.y } else { hi.y },
                    if k & 4 == 0 { lo.z } else { hi.z },
                )
            })
            .collect();
        // The cube's twelve triangles.
        let quads = [
            [0, 1, 3, 2],
            [4, 6, 7, 5],
            [0, 4, 5, 1],
            [2, 3, 7, 6],
            [0, 2, 6, 4],
            [1, 5, 7, 3],
        ];
        let tris: Vec<[u32; 3]> = quads
            .iter()
            .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
            .collect();
        let (zmin, zmax) = (Vec3::splat(-0.3), Vec3::splat(0.3));
        let at = |x: f32| Mat4::from_translation(Vec3::new(x, 0.0, 0.0));
        // Inside: the hull fills the zone, no triangle reaches it.
        assert!(box_touches_piece(at(0.0), zmin, zmax, &pts, None));
        assert!(!box_touches_piece(at(0.0), zmin, zmax, &pts, Some(&tris)));
        // Across a face: both.
        assert!(box_touches_piece(at(1.1), zmin, zmax, &pts, Some(&tris)));
        assert!(box_touches_piece(at(1.1), zmin, zmax, &pts, None));
        // Clear of it: neither, also not within a tolerance (a pair only
        // enters the list overlapping).
        assert!(!box_touches_piece(at(1.35), zmin, zmax, &pts, Some(&tris)));
        assert!(!box_touches_piece(at(1.35), zmin, zmax, &pts, None));
    }

    /// docs/29 section 12z, live `hazard-octagon__live__000` and
    /// `hazard-compressor__live__000`: every arena hinge's controller angle
    /// grows from 0 toward its upper limit as `p = 100` drives it, so ours
    /// turns the part the way live does and keeps its travel on that side.
    #[test]
    fn an_arena_hinge_turns_the_way_its_controller_drives_it() {
        // The hellraisers and hammers: axis negative in body B, which swings.
        assert_eq!(
            arena_hinge_sense(true, Vec3::new(0.0, 0.0, -1.0)),
            (-1.0, -1.0)
        );
        assert_eq!(
            arena_hinge_sense(true, Vec3::new(-1.0, 0.0, 0.0)),
            (-1.0, -1.0)
        );
        // The Compressor's right arms: axis +y in B.
        assert_eq!(arena_hinge_sense(true, Vec3::Y), (-1.0, 1.0));
        // The Parking Lot's trap doors swing as body A.
        assert_eq!(
            arena_hinge_sense(false, Vec3::new(-1.0, 0.0, 0.0)),
            (1.0, 1.0)
        );

        let hellraiser = arena_hinge_limits(Some([1f32.to_radians(), 70f32.to_radians()]), -1.0);
        assert!(
            (hellraiser[0] + 70f32.to_radians()).abs() < 1e-6,
            "{hellraiser:?}"
        );
        assert_eq!(hellraiser[1], 0.0);
        assert_eq!(
            arena_hinge_limits(Some([0.0, 0.270526]), -1.0),
            [-0.270526, 0.0]
        );
        assert_eq!(
            arena_hinge_limits(Some([-PI / 2.0, PI / 2.0]), 1.0),
            [-PI / 2.0, PI / 2.0]
        );
        assert_eq!(arena_hinge_limits(None, -1.0), [-PI, PI]);
    }
}
