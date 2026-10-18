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
