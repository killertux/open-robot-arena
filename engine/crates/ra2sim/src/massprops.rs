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

/// The outward-wound triangles of the convex hull of `points` (indices into `points`), for ray casts.
pub fn hull_triangles(points: &[Vec3]) -> Option<Vec<[u32; 3]>> {
    quickhull(points).map(|f| {
        f.into_iter()
            .map(|t| [t[0] as u32, t[1] as u32, t[2] as u32])
            .collect()
    })
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

/// A rigid body's mass properties the way the original computes them
/// (`FUN_0053c990`, docs/29 section 12bf): Mirtich's polyhedral integrals
/// (`volInt.c`) over the body's own triangles, one face at a time, in `f32`.
/// The vertices are first moved by minus the centre of their bounding box
/// (`FUN_0053c810`) for accuracy and the centre is added back at the end; a
/// face is integrated in the projection along its normal's largest component
/// (`FUN_0053c400`, `FUN_0053c180`, `FUN_0053bdc0`); a face with no area is
/// skipped. The integrals are those of the surface as it is wound, so an open
/// surface and an inside-out one give what the formulas give, not a hull's
/// solid. With the volume `V` (signed) the density is `mass / V`
/// (`FUN_0053c6e0`).
///
/// Returns `(V, centre of mass, inertia tensor about it)` for the given mass,
/// or `None` for no triangles or no volume.
pub fn original_mass_props(
    points: &[Vec3],
    triangles: &[[u32; 3]],
    mass: f32,
) -> Option<(f32, Vec3, glam::Mat3)> {
    if points.is_empty()
        || triangles.is_empty()
        || triangles
            .iter()
            .flatten()
            .any(|&i| i as usize >= points.len())
    {
        return None;
    }
    // FUN_0053c810: minus the centre of the bounding box.
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in points {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let centre = Vec3::new(
        (hi.x + lo.x) * 0.5,
        (hi.y + lo.y) * 0.5,
        (hi.z + lo.z) * 0.5,
    );
    let off = -centre;
    let v: Vec<[f32; 3]> = points
        .iter()
        .map(|p| [p.x + off.x, off.y + p.y, off.z + p.z])
        .collect();

    let (mut t0, mut t1, mut t2, mut tp) = (0.0f32, [0.0f32; 3], [0.0f32; 3], [0.0f32; 3]);
    for t in triangles {
        let p = [v[t[0] as usize], v[t[1] as usize], v[t[2] as usize]];
        // FUN_0053ba90: the face normal and plane offset.
        let d1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
        let d2 = [p[2][0] - p[1][0], p[2][1] - p[1][1], p[2][2] - p[1][2]];
        let nx = d2[2] * d1[1] - d2[1] * d1[2];
        let ny = d2[0] * d1[2] - d2[2] * d1[0];
        let nz = d2[1] * d1[0] - d2[0] * d1[1];
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        if len == 0.0 {
            continue;
        }
        let k = 1.0 / len;
        let n = [nx * k, k * ny, k * nz];
        let w = (-(n[0] * p[0][0]) - n[1] * p[0][1]) - n[2] * p[0][2];
        // FUN_0053c400: the projection axes.
        let c = if n[0].abs() <= n[1].abs() || n[0].abs() <= n[2].abs() {
            if n[1].abs() <= n[2].abs() {
                2
            } else {
                1
            }
        } else {
            0
        };
        let a = (c + 1) % 3;
        let b = (a + 1) % 3;
        // FUN_0053bdc0: the integrals over the projection.
        let (mut p1, mut pa, mut pb, mut paa, mut pab, mut pbb) = (0.0f32, 0.0, 0.0, 0.0, 0.0, 0.0);
        let (mut paaa, mut paab, mut pabb, mut pbbb) = (0.0f32, 0.0, 0.0, 0.0);
        for i in 0..3 {
            let (j, kx) = (i, (i + 1) % 3);
            let a0 = p[j][a];
            let b0 = p[j][b];
            let a1 = p[kx][a];
            let b1 = p[kx][b];
            let da = a1 - a0;
            let db = b1 - b0;
            let a0_2 = a0 * a0;
            let a0_3 = a0_2 * a0;
            let b0_2 = b0 * b0;
            let b0_3 = b0_2 * b0;
            let a1_2 = a1 * a1;
            let b1_3 = b1 * b1 * b1;
            let c1 = a1 + a0;
            let ca = a1 * c1 + a0_2;
            let caa = ca * a1 + a0_3;
            let cb = (b1 + b0) * b1 + b0_2;
            let cbb = cb * b1 + b0_3;
            let a1a0 = a1 * a0 + a1 * a0;
            let kab = a1_2 + a1a0 + a0_2 * 3.0;
            let cab = a1_2 * 3.0 + a1a0 + a0_2;
            p1 += c1 * db;
            pa += ca * db;
            paa += caa * db;
            paaa += (caa * a1 + a0_3 * a0) * db;
            pb += cb * da;
            pbb += cbb * da;
            pbbb += (cbb * b1 + b0_2 * b0_2) * da;
            pab += (cab * b1 + kab * b0) * db;
            paab += ((cab * a0 + a1_2 * a1 * 4.0) * b1 + (kab * a1 + a0_3 * 4.0) * b0) * db;
            let b1_2b0 = b1 * b1 * b0;
            let b1b0_2 = b0_2 * b1;
            pabb += ((b1_3 * 4.0 + b1_2b0 * 3.0 + b1b0_2 + b1b0_2 + b0_3) * a1
                + (b0_3 * 4.0 + b1_2b0 + b1_2b0 + b1b0_2 * 3.0 + b1_3) * a0)
                * da;
        }
        let (p1, pa, paa, paaa) = (
            p1 * 0.5,
            pa * 0.166_666_67,
            paa * 0.083_333_336,
            paaa * 0.05,
        );
        let (pb, pbb, pbbb) = (pb * -0.166_666_67, pbb * -0.083_333_336, pbbb * -0.05);
        let (pab, paab, pabb) = (
            pab * 0.041_666_668,
            paab * 0.016_666_668,
            pabb * -0.016_666_668,
        );
        // FUN_0053c180: the face integrals.
        let k1 = 1.0 / n[c];
        let k2 = k1 * k1;
        let k3 = k2 * k1;
        let (na, nb) = (n[a], n[b]);
        let fa = k1 * pa;
        let fb = k1 * pb;
        let fc = -((na * pa + nb * pb + w * p1) * k2);
        let faa = k1 * paa;
        let fbb = k1 * pbb;
        let fcc = (na * nb * pab * 2.0
            + na * na * paa
            + nb * nb * pbb
            + ((na * pa + nb * pb) * 2.0 + w * p1) * w)
            * k3;
        let faaa = k1 * paaa;
        let fbbb = k1 * pbbb;
        let fccc = -(k3
            * k1
            * (na * na * na * paaa
                + nb * nb * nb * pbbb
                + (nb * nb * na * pabb + na * na * nb * paab) * 3.0
                + ((na * na * paa + nb * nb * pbb + na * nb * pab * 2.0) * 3.0
                    + ((na * pa + nb * pb) * 3.0 + w * p1) * w)
                    * w));
        let faab = k1 * paab;
        let fbbc = -((na * pabb + nb * pbbb + w * pbb) * k2);
        let fcca = (na * na * paaa
            + na * nb * paab * 2.0
            + nb * nb * pabb
            + (na * paa * 2.0 + nb * pab * 2.0 + w * pa) * w)
            * k3;
        // FUN_0053c400: the volume integrals.
        t0 += n[0]
            * if a == 0 {
                fa
            } else if b == 0 {
                fb
            } else {
                fc
            };
        t1[a] += faa * n[a];
        t1[b] += fbb * n[b];
        t1[c] += fcc * n[c];
        t2[a] += faaa * n[a];
        t2[b] += fbbb * n[b];
        t2[c] += fccc * n[c];
        tp[a] += faab * n[a];
        tp[b] += fbbc * n[b];
        tp[c] += fcca * n[c];
    }
    let t1 = t1.map(|x| x * 0.5);
    let t2 = t2.map(|x| x * 0.333_333_34);
    let tp = tp.map(|x| x * 0.5);
    if t0 == 0.0 || !t0.is_finite() {
        return None;
    }
    // FUN_0053c6e0: the centre and the inertia tensor about it.
    let density = mass / t0;
    let r = [t1[0] / t0, t1[1] / t0, t1[2] / t0];
    let mut j = [[0.0f32; 3]; 3];
    j[0][0] = (t2[2] + t2[1]) * density;
    j[1][1] = (t2[0] + t2[2]) * density;
    j[2][2] = (t2[0] + t2[1]) * density;
    j[0][1] = -(tp[0] * density);
    j[1][0] = j[0][1];
    j[1][2] = -(tp[1] * density);
    j[2][1] = j[1][2];
    j[2][0] = -(tp[2] * density);
    j[0][2] = j[2][0];
    j[0][0] -= (r[2] * r[2] + r[1] * r[1]) * mass;
    j[1][1] -= (r[2] * r[2] + r[0] * r[0]) * mass;
    j[2][2] -= (r[1] * r[1] + r[0] * r[0]) * mass;
    let xy = mass * r[0] * r[1] + j[0][1];
    let yz = mass * r[2] * r[1] + j[1][2];
    let zx = r[2] * r[0] * mass + j[2][0];
    j[0][1] = xy;
    j[1][0] = xy;
    j[1][2] = yz;
    j[2][1] = yz;
    j[2][0] = zx;
    j[0][2] = zx;
    let com = Vec3::new(r[0], r[1], r[2]) + centre;
    // Rows to columns (the tensor is symmetric).
    let m = glam::Mat3::from_cols_array_2d(&j);
    Some((t0, com, m))
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

#[cfg(test)]
mod original_tests {
    use super::*;

    fn cube(h: f32) -> (Vec<Vec3>, Vec<[u32; 3]>) {
        let p: Vec<Vec3> = (0..8)
            .map(|i| {
                Vec3::new(
                    if i & 1 == 0 { -h } else { h },
                    if i & 2 == 0 { -h } else { h },
                    if i & 4 == 0 { -h } else { h },
                )
            })
            .collect();
        let quads = [
            [0u32, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ];
        let t = quads
            .iter()
            .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
            .collect();
        (p, t)
    }

    #[test]
    fn a_turned_box_agrees_with_the_tetrahedra_rule() {
        let (p, t) = cube(0.5);
        let q = glam::Quat::from_euler(glam::EulerRot::XYZ, 0.4, 0.9, -0.3);
        let p: Vec<Vec3> = p
            .iter()
            .map(|v| q * (*v * Vec3::new(0.3, 1.0, 0.2)) + Vec3::new(0.7, 0.1, -0.4))
            .collect();
        let (v, c, i) = original_mass_props(&p, &t, 8.0).expect("a box");
        let (v2, c2, i2) = mesh_inertia(&p, &t).expect("a box");
        assert!((v.abs() - v2).abs() < 1e-4, "{v} {v2}");
        assert!((c - c2).length() < 1e-4, "{c} {c2}");
        let i2 = i2 * (8.0 / v2);
        assert!(i.abs_diff_eq(i2, 2e-4), "{i:?} {i2:?}");
    }

    #[test]
    fn a_cube_off_the_origin_has_the_textbook_inertia() {
        let (p, t) = cube(0.5);
        let p: Vec<Vec3> = p.iter().map(|q| *q + Vec3::new(3.0, -2.0, 1.0)).collect();
        let (v, c, i) = original_mass_props(&p, &t, 6.0).expect("a cube");
        assert!((v.abs() - 1.0).abs() < 1e-4, "{v}");
        assert!((c - Vec3::new(3.0, -2.0, 1.0)).length() < 1e-4, "{c}");
        // m (a^2 + a^2) / 12 = 6 * 2 / 12 = 1
        for k in 0..3 {
            assert!((i.col(k)[k] - 1.0).abs() < 1e-3, "{i:?}");
        }
        assert!(
            i.col(0)[1].abs() < 1e-3 && i.col(1)[2].abs() < 1e-3,
            "{i:?}"
        );
    }
}
