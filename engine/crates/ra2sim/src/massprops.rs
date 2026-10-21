//! Mass properties of a robot's rigid bodies.
//!
//! The original gives every component its own mass (the `.gmf` rigid bodies'
//! `MASS`) and Havok integrates the assembly, so a robot's centre of mass is the
//! **mass-weighted** mean of its components. Jolt derives a compound's centre
//! from the *volume* of its collision hulls (every child at one density), which
//! puts a robot's centre inside its big hollow chassis hull and ignores a heavy
//! battery or motor that has no collider of its own. This module computes both
//! centres so the backend can move the body's centre from the one to the other.

use glam::Vec3;

/// Volume and volume-centroid of the convex hull of `points`, or `None` for a
/// degenerate (flat or tiny) point set.
pub fn hull_volume_centroid(points: &[Vec3]) -> Option<(f32, Vec3)> {
    let faces = quickhull(points)?;
    let reference = points.iter().copied().sum::<Vec3>() / points.len() as f32;
    let mut volume = 0.0f64;
    let mut moment = glam::DVec3::ZERO;
    for [a, b, c] in faces {
        let (a, b, c) = (
            (points[a] - reference).as_dvec3(),
            (points[b] - reference).as_dvec3(),
            (points[c] - reference).as_dvec3(),
        );
        let v = a.dot(b.cross(c)) / 6.0;
        volume += v;
        moment += v * (a + b + c) / 4.0;
    }
    if volume.abs() < 1e-9 {
        return None;
    }
    Some((volume as f32, reference + (moment / volume).as_vec3()))
}

/// Volume and volume-centroid of the solid a triangle mesh bounds, by the
/// divergence theorem over its triangles as they are wound (`None` when the
/// enclosed volume is not positive).
pub fn mesh_volume_centroid(points: &[Vec3], triangles: &[[u32; 3]]) -> Option<(f32, Vec3)> {
    let mut volume = 0.0f64;
    let mut moment = glam::DVec3::ZERO;
    for t in triangles {
        let get = |i: u32| points.get(i as usize).map(|p| p.as_dvec3());
        let (Some(a), Some(b), Some(c)) = (get(t[0]), get(t[1]), get(t[2])) else {
            continue;
        };
        let v = a.dot(b.cross(c)) / 6.0;
        volume += v;
        moment += v * (a + b + c) / 4.0;
    }
    (volume.abs() > 1e-9).then(|| (volume.abs() as f32, (moment / volume).as_vec3()))
}

/// Volume, volume-centroid and inertia tensor (about that centroid, at unit
/// density) of the convex hull of `points`, or `None` for a degenerate set.
pub fn hull_inertia(points: &[Vec3]) -> Option<(f32, Vec3, glam::Mat3)> {
    let faces = quickhull(points)?;
    let tris: Vec<[u32; 3]> = faces
        .iter()
        .map(|f| [f[0] as u32, f[1] as u32, f[2] as u32])
        .collect();
    solid_inertia(points, &tris)
}

/// [`hull_inertia`] for the solid a closed triangle mesh bounds, as Havok
/// takes a mesh body's mass properties from its own triangles. A mesh wound
/// inside out (the Scout's fork arm) bounds the same solid: the signs cancel.
pub fn mesh_inertia(points: &[Vec3], triangles: &[[u32; 3]]) -> Option<(f32, Vec3, glam::Mat3)> {
    if triangles
        .iter()
        .flatten()
        .any(|&i| i as usize >= points.len())
    {
        return None;
    }
    solid_inertia(points, triangles)
}

/// The solid's volume, centroid and inertia: the triangles split into
/// tetrahedra from a reference point; each contributes its covariance
/// `det(A) A C0 Aᵀ` (with `C0` the unit tetrahedron's), and the inertia is
/// `trace(C) I - C` once the covariance is moved to the centroid.
fn solid_inertia(points: &[Vec3], triangles: &[[u32; 3]]) -> Option<(f32, Vec3, glam::Mat3)> {
    if points.is_empty() {
        return None;
    }
    let reference = points.iter().copied().sum::<Vec3>() / points.len() as f32;
    solid_inertia_from(points, triangles, reference)
}

/// A rigid body's mass properties from its geometry's own triangles: volume,
/// centroid and inertia (about the centroid, unit density) of the tetrahedra
/// from `reference` to every triangle as wound. For a closed surface the
/// reference does not matter and this is the solid the surface bounds; for an
/// open one (the stock motors' rotor proxies, boxes without their inboard end
/// face) it is the solid the surface and the cone from `reference` bound.
pub fn triangles_mass_from(
    points: &[Vec3],
    triangles: &[[u32; 3]],
    reference: Vec3,
) -> Option<(f32, Vec3, glam::Mat3)> {
    if triangles.is_empty()
        || triangles
            .iter()
            .flatten()
            .any(|&i| i as usize >= points.len())
    {
        return None;
    }
    solid_inertia_from(points, triangles, reference)
}

fn solid_inertia_from(
    points: &[Vec3],
    triangles: &[[u32; 3]],
    reference: Vec3,
) -> Option<(f32, Vec3, glam::Mat3)> {
    use glam::{DMat3, DVec3};
    let reference = reference.as_dvec3();
    let c0 = DMat3::from_cols(
        DVec3::new(2.0, 1.0, 1.0),
        DVec3::new(1.0, 2.0, 1.0),
        DVec3::new(1.0, 1.0, 2.0),
    ) * (1.0 / 120.0);
    let mut volume = 0.0f64;
    let mut moment = DVec3::ZERO;
    let mut cov = DMat3::ZERO;
    for t in triangles {
        let (a, b, c) = (
            points[t[0] as usize].as_dvec3() - reference,
            points[t[1] as usize].as_dvec3() - reference,
            points[t[2] as usize].as_dvec3() - reference,
        );
        let m = DMat3::from_cols(a, b, c);
        let det = m.determinant();
        volume += det / 6.0;
        moment += det / 6.0 * (a + b + c) / 4.0;
        cov += m * c0 * m.transpose() * det;
    }
    if volume.abs() < 1e-9 {
        return None;
    }
    let centre = moment / volume;
    let (volume, cov) = if volume < 0.0 {
        (-volume, cov * -1.0)
    } else {
        (volume, cov)
    };
    // Move the covariance from the reference point to the centroid.
    let shift = DMat3::from_cols(centre * centre.x, centre * centre.y, centre * centre.z);
    let cov = cov - shift * volume;
    let trace = cov.x_axis.x + cov.y_axis.y + cov.z_axis.z;
    let inertia = DMat3::IDENTITY * trace - cov;
    Some((
        volume as f32,
        (reference + centre).as_vec3(),
        inertia.as_mat3(),
    ))
}

/// The points of `points`' convex hull pushed out by `d` along every face's
/// normal: a hull whose faces sit exactly `d` outside the original's. This is a
/// collision *skin* — Havok's convex shapes carry a radius that their hull is
/// grown by — and the hull of the returned points is the grown shape. Falls back
/// to `points` unchanged when they have no hull.
pub fn skinned(points: &[Vec3], d: f32) -> Vec<Vec3> {
    let Some(faces) = quickhull(points) else {
        return points.to_vec();
    };
    let mut out = Vec::with_capacity(faces.len() * 3);
    for [a, b, c] in faces {
        let n = (points[b] - points[a])
            .cross(points[c] - points[a])
            .normalize_or_zero();
        out.extend([points[a] + n * d, points[b] + n * d, points[c] + n * d]);
    }
    out
}

/// The distinct corners of the convex hull of `points` (points closer than a
/// millimetre are one corner). Empty for a degenerate set.
pub fn hull_vertices(points: &[Vec3]) -> Vec<Vec3> {
    let Some(faces) = quickhull(points) else {
        return Vec::new();
    };
    let mut out: Vec<Vec3> = Vec::new();
    for i in faces.iter().flatten().copied() {
        let p = points[i];
        if !out.iter().any(|q| q.distance_squared(p) < 1e-6) {
            out.push(p);
        }
    }
    out
}

/// Outward-wound triangles of the convex hull (indices into `points`).
fn quickhull(points: &[Vec3]) -> Option<Vec<[usize; 3]>> {
    let n = points.len();
    if n < 4 {
        return None;
    }
    let scale = points
        .iter()
        .fold(0.0f32, |m, p| m.max(p.abs().max_element()))
        .max(1e-6);
    let eps = 1e-5 * scale;

    // Initial tetrahedron: the two farthest apart along an axis, then the point
    // farthest from their line, then farthest from that plane.
    let mut lo = [0usize; 3];
    let mut hi = [0usize; 3];
    for (i, p) in points.iter().enumerate() {
        for k in 0..3 {
            if p[k] < points[lo[k]][k] {
                lo[k] = i;
            }
            if p[k] > points[hi[k]][k] {
                hi[k] = i;
            }
        }
    }
    let (mut i0, mut i1, mut best) = (0, 0, -1.0f32);
    for k in 0..3 {
        let d = points[hi[k]][k] - points[lo[k]][k];
        if d > best {
            best = d;
            i0 = lo[k];
            i1 = hi[k];
        }
    }
    if best <= eps {
        return None;
    }
    let line = (points[i1] - points[i0]).normalize();
    let i2 = (0..n).max_by(|&a, &b| {
        let da = (points[a] - points[i0]).cross(line).length();
        let db = (points[b] - points[i0]).cross(line).length();
        da.total_cmp(&db)
    })?;
    if (points[i2] - points[i0]).cross(line).length() <= eps {
        return None;
    }
    let normal = (points[i1] - points[i0])
        .cross(points[i2] - points[i0])
        .normalize();
    let i3 = (0..n).max_by(|&a, &b| {
        let da = (points[a] - points[i0]).dot(normal).abs();
        let db = (points[b] - points[i0]).dot(normal).abs();
        da.total_cmp(&db)
    })?;
    if (points[i3] - points[i0]).dot(normal).abs() <= eps {
        return None;
    }

    let centre = (points[i0] + points[i1] + points[i2] + points[i3]) / 4.0;
    let mut faces: Vec<[usize; 3]> = Vec::new();
    let add = |faces: &mut Vec<[usize; 3]>, a: usize, b: usize, c: usize| {
        let n = (points[b] - points[a]).cross(points[c] - points[a]);
        if n.dot(points[a] - centre) >= 0.0 {
            faces.push([a, b, c]);
        } else {
            faces.push([a, c, b]);
        }
    };
    add(&mut faces, i0, i1, i2);
    add(&mut faces, i0, i1, i3);
    add(&mut faces, i0, i2, i3);
    add(&mut faces, i1, i2, i3);

    let dist = |f: &[usize; 3], p: Vec3| {
        let n = (points[f[1]] - points[f[0]])
            .cross(points[f[2]] - points[f[0]])
            .normalize_or_zero();
        n.dot(p - points[f[0]])
    };
    for (pi, &p) in points.iter().enumerate() {
        if [i0, i1, i2, i3].contains(&pi) {
            continue;
        }
        let visible: Vec<bool> = faces.iter().map(|f| dist(f, p) > eps).collect();
        if !visible.iter().any(|&v| v) {
            continue;
        }
        // Horizon: directed edges of visible faces whose reverse is not on a
        // visible face.
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for (f, _) in faces.iter().zip(&visible).filter(|(_, &v)| v) {
            for (a, b) in [(f[0], f[1]), (f[1], f[2]), (f[2], f[0])] {
                edges.push((a, b));
            }
        }
        let horizon: Vec<(usize, usize)> = edges
            .iter()
            .copied()
            .filter(|&(a, b)| !edges.contains(&(b, a)))
            .collect();
        let mut kept: Vec<[usize; 3]> = faces
            .iter()
            .zip(&visible)
            .filter(|(_, &v)| !v)
            .map(|(f, _)| *f)
            .collect();
        for (a, b) in horizon {
            kept.push([a, b, pi]);
        }
        faces = kept;
    }
    Some(faces)
}

#[cfg(test)]
mod tests {

    /// A box without its top face, integrated from the middle of its bottom,
    /// puts its mass 3/8 of the way up (the solid is the box less the cone
    /// from that point to the missing face); closed, it is the box's centre.
    #[test]
    fn an_open_box_from_its_closed_end_weighs_at_three_eighths() {
        let p: Vec<Vec3> = (0..8)
            .map(|i| {
                Vec3::new(
                    if i & 1 == 0 { -0.5 } else { 0.5 },
                    if i & 2 == 0 { -0.5 } else { 0.5 },
                    if i & 4 == 0 { 0.0 } else { 2.0 },
                )
            })
            .collect();
        // Outward-wound faces of the box, the z = 2 end last.
        let sides = vec![
            [0, 2, 3],
            [3, 1, 0], // z = 0
            [0, 1, 5],
            [5, 4, 0], // y = -0.5
            [1, 3, 7],
            [7, 5, 1], // x = 0.5
            [3, 2, 6],
            [6, 7, 3], // y = 0.5
            [2, 0, 4],
            [4, 6, 2], // x = -0.5
        ];
        let open = triangles_mass_from(&p, &sides, Vec3::ZERO).expect("open");
        assert!(
            (open.1 - Vec3::new(0.0, 0.0, 0.75)).length() < 1e-5,
            "{:?}",
            open.1
        );
        let mut closed = sides.clone();
        closed.extend([[4, 5, 7], [7, 6, 4]]);
        let shut = triangles_mass_from(&p, &closed, Vec3::ZERO).expect("closed");
        assert!(
            (shut.1 - Vec3::new(0.0, 0.0, 1.0)).length() < 1e-5,
            "{:?}",
            shut.1
        );
        assert!((shut.0 - 2.0).abs() < 1e-5);
    }

    use super::*;

    #[test]
    fn a_box_hull_has_the_boxs_volume_and_centre() {
        let mut pts = Vec::new();
        for x in [0.0, 2.0] {
            for y in [1.0, 4.0] {
                for z in [-1.0, 0.5] {
                    pts.push(Vec3::new(x, y, z));
                }
            }
        }
        // Interior points must not change anything.
        pts.push(Vec3::new(1.0, 2.0, 0.0));
        let (v, c) = hull_volume_centroid(&pts).expect("a hull");
        assert!((v - 2.0 * 3.0 * 1.5).abs() < 1e-3, "{v}");
        assert!((c - Vec3::new(1.0, 2.5, -0.25)).length() < 1e-3, "{c}");
    }

    #[test]
    fn a_box_hull_has_the_boxs_inertia() {
        let (w, h, d) = (2.0f32, 3.0f32, 1.5f32);
        let mut pts = Vec::new();
        for x in [0.0, w] {
            for y in [1.0, 1.0 + h] {
                for z in [-1.0, -1.0 + d] {
                    pts.push(Vec3::new(x, y, z));
                }
            }
        }
        let (v, c, i) = hull_inertia(&pts).expect("a hull");
        assert!((v - w * h * d).abs() < 1e-3);
        assert!((c - Vec3::new(1.0, 2.5, -0.25)).length() < 1e-3, "{c}");
        let ix = v * (h * h + d * d) / 12.0;
        let iy = v * (w * w + d * d) / 12.0;
        let iz = v * (w * w + h * h) / 12.0;
        assert!((i.x_axis.x - ix).abs() < 1e-3, "{i:?}");
        assert!((i.y_axis.y - iy).abs() < 1e-3, "{i:?}");
        assert!((i.z_axis.z - iz).abs() < 1e-3, "{i:?}");
        assert!(i.x_axis.y.abs() < 1e-3 && i.x_axis.z.abs() < 1e-3 && i.y_axis.z.abs() < 1e-3);
    }

    #[test]
    fn a_flat_set_has_no_hull() {
        let pts = [
            Vec3::ZERO,
            Vec3::X,
            Vec3::Y,
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.5, 0.5, 0.0),
        ];
        assert!(hull_volume_centroid(&pts).is_none());
    }
}
