//! Geometry queries over the bodies' collidables: how deep two shapes overlap
//! and which way to push them apart, and where a ray first meets a body.
//!
//! The backend needs these three ways beyond the collision agents of
//! [`super::narrow`] (which report nothing for shapes that overlap):
//!
//! * the deep-overlap contacts of a pair a bad step let through
//!   ([`collide`]: one hit per overlapping pair of collidables, and per
//!   triangle of a mesh, as the Jolt query world this replaced reported);
//! * the surface a wheel's phantom (its traction ring) overlaps ([`collide`]
//!   again, the deepest hit's normal);
//! * ray casts ([`ray_piece`]).
//!
//! Everything is written over the pieces [`super::narrow::pieces_of`] makes
//! from a body's shape: a convex piece is the convex hull of its vertices grown
//! by a radius, a mesh is a set of triangles. A piece built from a `MeshHull`
//! is seen as its convex hull here ([`Piece::hull`]), as it was in the Jolt
//! world.
//!
//! Overlap depth is the penetration depth of the Minkowski difference (an
//! expanding-polytope search over the support functions, started from the
//! directions of the axes), plus the two radii; shapes whose cores are apart but
//! closer than the radii overlap by the difference. Ray casts are the
//! Gilbert-Bergen-Wiseman conservative advancement on the same support
//! functions, and a ray against a mesh walks its bounding boxes.

use glam::{DVec3, Vec3};

use super::narrow::{closest, reduce, support, Aabb, Mesh, Piece, PieceShape, Pose, SimplexPoint};

/// The convex hull of `verts` grown by `radius`, or a triangle mesh.
#[derive(Clone, Copy)]
pub enum Shape<'a> {
    Convex { verts: &'a [Vec3], radius: f32 },
    Mesh(&'a Mesh),
}

/// A piece as the queries see it.
pub fn shape_of(p: &Piece) -> Shape<'_> {
    if let Some(h) = &p.hull {
        return Shape::Convex {
            verts: h,
            radius: 0.0,
        };
    }
    match &p.shape {
        PieceShape::Convex { vertices, radius } => Shape::Convex {
            verts: vertices,
            radius: *radius,
        },
        PieceShape::Mesh(m) => Shape::Mesh(m),
    }
}

/// One overlap, in world space. `normal` is the unit direction the query shape
/// moves to come out of the obstacle (from the obstacle toward the query),
/// `depth` how far it must move.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub depth: f32,
    pub normal: Vec3,
    /// The deepest point of the query shape inside the obstacle.
    pub on_query: Vec3,
    /// The deepest point of the obstacle inside the query shape.
    pub on_obstacle: Vec3,
}

/// The penetration of two convex shapes, in one frame; `axis` points from `a`
/// toward `b`.
struct Penetration {
    depth: f64,
    axis: DVec3,
    on_a: DVec3,
    on_b: DVec3,
}

fn dv(v: &[Vec3]) -> Vec<DVec3> {
    v.iter().map(|p| p.as_dvec3()).collect()
}

/// How deep the convex hulls of `a` and `b` (grown by `ra` and `rb`) overlap.
fn penetration(a: &[DVec3], ra: f64, b: &[DVec3], rb: f64) -> Option<Penetration> {
    let af: Vec<Vec3> = a.iter().map(|p| p.as_vec3()).collect();
    let bf: Vec<Vec3> = b.iter().map(|p| p.as_vec3()).collect();
    let c = closest(&af, &bf);
    let radii = ra + rb;
    if !c.overlap && c.distance > 1e-9 {
        let d = c.distance as f64;
        if d >= radii {
            return None;
        }
        let axis = (c.on_b.as_dvec3() - c.on_a.as_dvec3()) / d;
        return Some(Penetration {
            depth: radii - d,
            axis,
            on_a: c.on_a.as_dvec3() + axis * ra,
            on_b: c.on_b.as_dvec3() - axis * rb,
        });
    }
    // The cores touch or overlap.
    let Some((depth, axis, on_a, on_b)) = epa(a, b) else {
        // Degenerate cores (a point or segment on a plane): they only touch.
        let ca = a.iter().sum::<DVec3>() / a.len() as f64;
        let cb = b.iter().sum::<DVec3>() / b.len() as f64;
        let axis = (cb - ca).try_normalize().unwrap_or(DVec3::Y);
        return (radii > 0.0).then_some(Penetration {
            depth: radii,
            axis,
            on_a: ca + axis * ra,
            on_b: cb - axis * rb,
        });
    };
    Some(Penetration {
        depth: depth + radii,
        axis,
        on_a: on_a + axis * ra,
        on_b: on_b - axis * rb,
    })
}

#[derive(Clone, Copy)]
struct V {
    w: DVec3,
    a: DVec3,
    b: DVec3,
}

fn sup(a: &[DVec3], b: &[DVec3], d: DVec3) -> V {
    let pa = a[support(a, d)];
    let pb = b[support(b, -d)];
    V {
        w: pa - pb,
        a: pa,
        b: pb,
    }
}

struct Face {
    i: [usize; 3],
    n: DVec3,
    d: f64,
    alive: bool,
}

struct Polytope {
    v: Vec<V>,
    f: Vec<Face>,
}

impl Polytope {
    fn face(&mut self, i: [usize; 3], inside: DVec3) -> bool {
        let (p0, p1, p2) = (self.v[i[0]].w, self.v[i[1]].w, self.v[i[2]].w);
        let mut n = (p1 - p0).cross(p2 - p0);
        let len = n.length();
        if len < 1e-15 {
            return false;
        }
        n /= len;
        let mut i = i;
        if n.dot(inside - p0) > 0.0 {
            n = -n;
            i.swap(1, 2);
        }
        let d = n.dot(p0);
        self.f.push(Face {
            i,
            n,
            d,
            alive: true,
        });
        true
    }

    /// Grow the polytope to `p`; false when `p` is already inside.
    fn add(&mut self, p: V) -> bool {
        let visible: Vec<usize> = (0..self.f.len())
            .filter(|&k| {
                let f = &self.f[k];
                f.alive && f.n.dot(p.w - self.v[f.i[0]].w) > 1e-12
            })
            .collect();
        if visible.is_empty() {
            return false;
        }
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for &k in &visible {
            let i = self.f[k].i;
            for (u, v) in [(i[0], i[1]), (i[1], i[2]), (i[2], i[0])] {
                if let Some(pos) = edges.iter().position(|&e| e == (v, u)) {
                    edges.swap_remove(pos);
                } else {
                    edges.push((u, v));
                }
            }
            self.f[k].alive = false;
        }
        let at = self.v.len();
        self.v.push(p);
        for (u, v) in edges {
            let (p0, p1, p2) = (self.v[u].w, self.v[v].w, p.w);
            let mut n = (p1 - p0).cross(p2 - p0);
            let len = n.length();
            if len < 1e-15 {
                continue;
            }
            n /= len;
            let d = n.dot(p0);
            self.f.push(Face {
                i: [u, v, at],
                n,
                d,
                alive: true,
            });
        }
        true
    }
}

/// The expanding-polytope search on `a - b`: the depth the cores overlap by,
/// the axis from `a` toward `b` and the deepest point of each. `None` when the
/// difference is flat (no starting tetrahedron).
fn epa(a: &[DVec3], b: &[DVec3]) -> Option<(f64, DVec3, DVec3, DVec3)> {
    let mut dirs = vec![
        DVec3::X,
        DVec3::NEG_X,
        DVec3::Y,
        DVec3::NEG_Y,
        DVec3::Z,
        DVec3::NEG_Z,
    ];
    for x in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            for z in [-1.0, 1.0] {
                dirs.push(DVec3::new(x, y, z).normalize());
            }
        }
    }
    let mut pts: Vec<V> = Vec::new();
    for d in dirs {
        let s = sup(a, b, d);
        if !pts.iter().any(|p| p.w.distance_squared(s.w) < 1e-18) {
            pts.push(s);
        }
    }
    if pts.len() < 4 {
        return None;
    }
    // The starting tetrahedron: the farthest pair, the point farthest from
    // their line, the point farthest from that plane.
    let mut best = (0usize, 1usize, -1.0f64);
    for i in 0..pts.len() {
        for j in i + 1..pts.len() {
            let d = pts[i].w.distance_squared(pts[j].w);
            if d > best.2 {
                best = (i, j, d);
            }
        }
    }
    let (i0, i1) = (best.0, best.1);
    let (p0, p1) = (pts[i0].w, pts[i1].w);
    let line = (p1 - p0).try_normalize()?;
    let i2 = (0..pts.len())
        .max_by(|&x, &y| {
            let dx = (pts[x].w - p0).cross(line).length_squared();
            let dy = (pts[y].w - p0).cross(line).length_squared();
            dx.total_cmp(&dy)
        })
        .unwrap();
    let plane = (pts[i2].w - p0).cross(line).try_normalize()?;
    let i3 = (0..pts.len())
        .max_by(|&x, &y| {
            (pts[x].w - p0)
                .dot(plane)
                .abs()
                .total_cmp(&(pts[y].w - p0).dot(plane).abs())
        })
        .unwrap();
    let scale = best.2.sqrt().max(1e-12);
    if (pts[i3].w - p0).dot(plane).abs() < 1e-9 * scale {
        return None;
    }
    let idx = [i0, i1, i2, i3];
    let inside = idx.iter().map(|&k| pts[k].w).sum::<DVec3>() / 4.0;
    let mut poly = Polytope {
        v: idx.iter().map(|&k| pts[k]).collect(),
        f: Vec::new(),
    };
    for tri in [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
        if !poly.face(tri, inside) {
            return None;
        }
    }
    for (k, p) in pts.iter().enumerate() {
        if !idx.contains(&k) {
            poly.add(*p);
        }
    }
    let mut chosen = None;
    for _ in 0..128 {
        let (k, face) = poly
            .f
            .iter()
            .enumerate()
            .filter(|(_, f)| f.alive)
            .min_by(|x, y| x.1.d.total_cmp(&y.1.d))?;
        chosen = Some(k);
        let (n, d) = (face.n, face.d);
        let s = sup(a, b, n);
        if s.w.dot(n) - d < 1e-9 * scale.max(1.0) {
            break;
        }
        if !poly.add(s) {
            break;
        }
    }
    let k = chosen?;
    // `chosen` may have been replaced by the last `add`; take the nearest alive.
    let face = if poly.f[k].alive {
        &poly.f[k]
    } else {
        poly.f
            .iter()
            .filter(|f| f.alive)
            .min_by(|x, y| x.d.total_cmp(&y.d))?
    };
    let (va, vb, vc) = (poly.v[face.i[0]], poly.v[face.i[1]], poly.v[face.i[2]]);
    let p = face.n * face.d;
    let (e0, e1, e2) = (vb.w - va.w, vc.w - va.w, p - va.w);
    let (d00, d01, d11) = (e0.dot(e0), e0.dot(e1), e1.dot(e1));
    let (d20, d21) = (e2.dot(e0), e2.dot(e1));
    let den = d00 * d11 - d01 * d01;
    let (u, v) = if den.abs() > 1e-300 {
        ((d11 * d20 - d01 * d21) / den, (d00 * d21 - d01 * d20) / den)
    } else {
        (0.0, 0.0)
    };
    let (u, v) = (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    let w = (1.0 - u - v).max(0.0);
    let on_a = va.a * w + vb.a * u + vc.a * v;
    let on_b = va.b * w + vb.b * u + vc.b * v;
    Some((face.d.max(0.0), face.n, on_a, on_b))
}

/// Hits shallower than this are dropped (the Jolt query world this replaced had
/// a collision tolerance of `1e-4` and reported nothing for touching shapes).
pub const MIN_DEPTH: f32 = 1e-4;

/// Every overlap of the convex piece `query` (at `qpose`) with `obstacle` (at
/// `opose`), appended to `out`: one for a convex obstacle, one per overlapped
/// triangle for a mesh. A mesh as the query yields nothing (meshes are static).
pub fn collide(
    query: &Piece,
    qpose: &Pose,
    obstacle: &Piece,
    opose: &Pose,
    scratch: &mut Vec<u32>,
    out: &mut Vec<Hit>,
) {
    let Shape::Convex {
        verts: qv,
        radius: qr,
    } = shape_of(query)
    else {
        return;
    };
    // Everything in the obstacle's frame.
    let inv = opose.rotation.inverse();
    let rel_rot = inv * qpose.rotation;
    let rel_pos = inv * (qpose.position - opose.position);
    let q: Vec<DVec3> = qv
        .iter()
        .map(|p| (rel_pos + rel_rot * *p).as_dvec3())
        .collect();
    let to_world = |p: DVec3| opose.apply(p.as_vec3());
    let push = |pen: Penetration, out: &mut Vec<Hit>| {
        if (pen.depth as f32) < MIN_DEPTH {
            return;
        }
        let n = (-pen.axis).as_vec3();
        out.push(Hit {
            depth: pen.depth as f32,
            normal: opose.rotation * n,
            on_query: to_world(pen.on_a),
            on_obstacle: to_world(pen.on_b),
        });
    };
    match shape_of(obstacle) {
        Shape::Convex {
            verts: ov,
            radius: or,
        } => {
            let o = dv(ov);
            if let Some(p) = penetration(&q, qr as f64, &o, or as f64) {
                push(p, out);
            }
        }
        Shape::Mesh(mesh) => {
            let bounds = Aabb::of(q.iter().map(|p| p.as_vec3())).grown(qr + 1e-4);
            mesh.query(&bounds, scratch);
            for &t in scratch.iter() {
                let tri = mesh.triangles[t as usize].map(|i| mesh.vertices[i as usize].as_dvec3());
                if let Some(p) = penetration(&q, qr as f64, &tri, 0.0) {
                    push(p, out);
                }
            }
        }
    }
}

/// Where the ray from `origin` along `dir` (unit, world space) first meets
/// `piece` (at `pose`) within `max`: its distance and the surface normal
/// there, facing the ray. A ray that starts inside a convex piece hits it at 0.
pub fn ray_piece(
    piece: &Piece,
    pose: &Pose,
    origin: Vec3,
    dir: Vec3,
    max: f32,
) -> Option<(f32, Vec3)> {
    let inv = pose.rotation.inverse();
    let o = inv * (origin - pose.position);
    let d = inv * dir;
    match shape_of(piece) {
        Shape::Mesh(mesh) => {
            let (t, tri) = mesh.ray(o, d, max)?;
            let v = mesh.triangles[tri as usize].map(|i| mesh.vertices[i as usize]);
            let mut n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
            if n.dot(d) > 0.0 {
                n = -n;
            }
            Some((t, pose.rotation * n))
        }
        Shape::Convex { verts, radius } => {
            let (t, n) = ray_convex(verts, radius, o, d, max)?;
            Some((t, pose.rotation * n))
        }
    }
}

/// A ray against the convex hull of `verts` grown by `radius` (the piece's
/// frame): conservative advancement on the support function.
fn ray_convex(
    verts: &[Vec3],
    radius: f32,
    origin: Vec3,
    dir: Vec3,
    max: f32,
) -> Option<(f32, Vec3)> {
    let pts = dv(verts);
    let r = dir.as_dvec3() * max as f64;
    let from = origin.as_dvec3();
    let radius = radius as f64;
    let mut lambda = 0.0f64;
    let mut x = from;
    let mut normal = DVec3::ZERO;
    let mut kept: Vec<DVec3> = Vec::new();
    let mut v = x - pts[0];
    for _ in 0..100 {
        let vv = v.length_squared();
        if vv < 1e-12 {
            break;
        }
        let vn = v / vv.sqrt();
        let p = pts[support(&pts, v)] + vn * radius;
        let w = x - p;
        let vw = v.dot(w);
        if vw > 0.0 {
            let vr = v.dot(r);
            if vr >= 0.0 {
                return None;
            }
            lambda -= vw / vr;
            if lambda > 1.0 {
                return None;
            }
            x = from + r * lambda;
            normal = v;
        }
        if !kept.iter().any(|q| q.distance_squared(p) < 1e-18) {
            kept.push(p);
        }
        let mut s: Vec<SimplexPoint> = kept
            .iter()
            .map(|p| SimplexPoint {
                w: x - *p,
                a: *p,
                b: DVec3::ZERO,
                ia: 0,
                ib: 0,
            })
            .collect();
        let mut lam = Vec::new();
        match reduce(&mut s, &mut lam) {
            Some(next) => {
                v = next;
                kept = s.iter().map(|q| q.a).collect();
            }
            None => {
                v = DVec3::ZERO;
                break;
            }
        }
    }
    let _ = v;
    let n = normal.try_normalize().map_or(-dir, |n| n.as_vec3());
    Some(((lambda * max as f64) as f32, n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::ShapeDesc;
    use glam::Quat;

    fn piece(shape: ShapeDesc) -> Piece {
        super::super::narrow::pieces_of(&shape).remove(0)
    }

    fn at(p: Vec3) -> Pose {
        Pose {
            position: p,
            rotation: Quat::IDENTITY,
        }
    }

    fn hits(q: &Piece, qp: Pose, o: &Piece, op: Pose) -> Vec<Hit> {
        let mut out = Vec::new();
        collide(q, &qp, o, &op, &mut Vec::new(), &mut out);
        out
    }

    #[test]
    fn a_box_sunk_into_a_box_overlaps_by_the_smaller_face_gap() {
        let a = piece(ShapeDesc::Box {
            half_extents: Vec3::splat(0.5),
        });
        let b = piece(ShapeDesc::Box {
            half_extents: Vec3::new(5.0, 0.5, 5.0),
        });
        // a's bottom face 0.2 below b's top face.
        let h = hits(&a, at(Vec3::new(0.3, 0.8, 0.1)), &b, at(Vec3::ZERO));
        assert_eq!(h.len(), 1);
        assert!((h[0].depth - 0.2).abs() < 1e-5, "{:?}", h[0]);
        assert!((h[0].normal - Vec3::Y).length() < 1e-5, "{:?}", h[0]);
    }

    #[test]
    fn two_spheres_overlap_by_the_radii_less_the_gap() {
        let a = piece(ShapeDesc::Sphere { radius: 0.5 });
        let b = piece(ShapeDesc::Sphere { radius: 0.25 });
        let h = hits(&a, at(Vec3::new(0.6, 0.0, 0.0)), &b, at(Vec3::ZERO));
        assert_eq!(h.len(), 1);
        assert!((h[0].depth - 0.15).abs() < 1e-5);
        assert!((h[0].normal - Vec3::X).length() < 1e-5);
        assert!(hits(&a, at(Vec3::new(0.8, 0.0, 0.0)), &b, at(Vec3::ZERO)).is_empty());
    }

    #[test]
    fn a_sphere_with_its_centre_inside_a_box_is_pushed_out_the_near_face() {
        let a = piece(ShapeDesc::Sphere { radius: 0.1 });
        let b = piece(ShapeDesc::Box {
            half_extents: Vec3::splat(1.0),
        });
        let h = hits(&a, at(Vec3::new(0.9, 0.1, 0.0)), &b, at(Vec3::ZERO));
        assert_eq!(h.len(), 1);
        assert!((h[0].depth - 0.2).abs() < 1e-5, "{:?}", h[0]);
        assert!((h[0].normal - Vec3::X).length() < 1e-5);
    }

    #[test]
    fn a_box_on_a_mesh_floor_hits_each_triangle_it_overlaps() {
        let floor = piece(ShapeDesc::TriMesh {
            vertices: vec![
                Vec3::new(-5.0, 0.0, -5.0),
                Vec3::new(5.0, 0.0, -5.0),
                Vec3::new(5.0, 0.0, 5.0),
                Vec3::new(-5.0, 0.0, 5.0),
            ],
            indices: vec![[0, 2, 1], [0, 3, 2]],
        });
        let b = piece(ShapeDesc::Box {
            half_extents: Vec3::splat(0.5),
        });
        let h = hits(&b, at(Vec3::new(1.0, 0.4, 1.0)), &floor, at(Vec3::ZERO));
        assert!(!h.is_empty());
        assert!(h.iter().all(|h| (h.depth - 0.1).abs() < 1e-4));
        assert!(h.iter().all(|h| (h.normal - Vec3::Y).length() < 1e-4));
    }

    #[test]
    fn rays_meet_boxes_spheres_capsules_and_meshes() {
        let b = piece(ShapeDesc::Box {
            half_extents: Vec3::splat(1.0),
        });
        let (t, n) = ray_piece(
            &b,
            &at(Vec3::ZERO),
            Vec3::new(0.2, 5.0, 0.3),
            Vec3::NEG_Y,
            20.0,
        )
        .unwrap();
        assert!(
            (t - 4.0).abs() < 1e-4 && (n - Vec3::Y).length() < 1e-4,
            "{t} {n:?}"
        );
        assert!(ray_piece(
            &b,
            &at(Vec3::ZERO),
            Vec3::new(2.0, 5.0, 0.0),
            Vec3::NEG_Y,
            20.0
        )
        .is_none());
        assert!(ray_piece(
            &b,
            &at(Vec3::ZERO),
            Vec3::new(0.0, 5.0, 0.0),
            Vec3::NEG_Y,
            3.0
        )
        .is_none());
        let s = piece(ShapeDesc::Sphere { radius: 0.5 });
        let (t, n) = ray_piece(
            &s,
            &at(Vec3::ZERO),
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::NEG_X,
            20.0,
        )
        .unwrap();
        assert!((t - 2.5).abs() < 1e-4 && (n - Vec3::X).length() < 1e-4);
        let c = piece(ShapeDesc::Capsule {
            half_height: 1.0,
            radius: 0.25,
        });
        let (t, _) = ray_piece(
            &c,
            &at(Vec3::ZERO),
            Vec3::new(3.0, 0.7, 0.0),
            Vec3::NEG_X,
            20.0,
        )
        .unwrap();
        assert!((t - 2.75).abs() < 1e-4);
        let (t, _) = ray_piece(
            &c,
            &at(Vec3::ZERO),
            Vec3::new(0.0, 5.0, 0.0),
            Vec3::NEG_Y,
            20.0,
        )
        .unwrap();
        assert!((t - 3.75).abs() < 1e-4);
        let m = piece(ShapeDesc::TriMesh {
            vertices: vec![
                Vec3::new(-5.0, 0.0, -5.0),
                Vec3::new(5.0, 0.0, -5.0),
                Vec3::new(5.0, 0.0, 5.0),
                Vec3::new(-5.0, 0.0, 5.0),
            ],
            indices: vec![[0, 2, 1], [0, 3, 2]],
        });
        let (t, n) = ray_piece(
            &m,
            &at(Vec3::ZERO),
            Vec3::new(1.0, 3.0, 1.0),
            Vec3::NEG_Y,
            20.0,
        )
        .unwrap();
        assert!((t - 3.0).abs() < 1e-4 && (n - Vec3::Y).length() < 1e-4);
        let (t, n) = ray_piece(
            &m,
            &at(Vec3::ZERO),
            Vec3::new(1.0, -2.0, 1.0),
            Vec3::Y,
            20.0,
        )
        .unwrap();
        assert!((t - 2.0).abs() < 1e-4 && (n - Vec3::NEG_Y).length() < 1e-4);
    }

    #[test]
    fn a_ray_that_starts_inside_hits_at_once() {
        let b = piece(ShapeDesc::Box {
            half_extents: Vec3::splat(1.0),
        });
        let (t, _) =
            ray_piece(&b, &at(Vec3::ZERO), Vec3::new(0.1, 0.0, 0.0), Vec3::Y, 5.0).unwrap();
        assert!(t < 1e-4);
    }

    /// The depth is the true minimum translation: the shapes touch after
    /// moving by it and no sampled direction needs less.
    #[test]
    fn the_overlap_depth_of_random_hulls_is_the_minimum_translation() {
        let mut seed = 0x1234_5678_9abc_def1u64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            ((seed >> 40) as f32) / (1u64 << 24) as f32
        };
        let cloud = |n: usize, scale: f32, at: Vec3, rnd: &mut dyn FnMut() -> f32| {
            (0..n)
                .map(|_| at + Vec3::new(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5) * scale)
                .collect::<Vec<_>>()
        };
        let mut checked = 0;
        for _ in 0..300 {
            let a = cloud(12, 1.0, Vec3::ZERO, &mut rnd);
            let off = Vec3::new(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5) * 0.8;
            let b = cloud(10, 1.2, off, &mut rnd);
            let (ad, bd) = (dv(&a), dv(&b));
            let Some(p) = penetration(&ad, 0.0, &bd, 0.0) else {
                continue;
            };
            if p.depth < 1e-3 {
                continue;
            }
            checked += 1;
            let moved: Vec<Vec3> = a
                .iter()
                .map(|v| *v - p.axis.as_vec3() * (p.depth as f32 * 1.01))
                .collect();
            assert!(!closest(&moved, &b).overlap, "not clear: {}", p.depth);
            let moved: Vec<Vec3> = a
                .iter()
                .map(|v| *v - p.axis.as_vec3() * (p.depth as f32 * 0.99))
                .collect();
            assert!(closest(&moved, &b).overlap, "clear too early: {}", p.depth);
            // No sampled direction clears sooner (radial extent of A - B).
            let diffs: Vec<Vec3> = a
                .iter()
                .flat_map(|x| b.iter().map(move |y| *x - *y))
                .collect();
            for _ in 0..200 {
                let u = Vec3::new(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5).normalize_or_zero();
                if u == Vec3::ZERO {
                    continue;
                }
                let (t, _) = ray_convex(&diffs, 0.0, u * 50.0, -u, 100.0).unwrap();
                assert!(
                    50.0 - t + 1e-3 >= p.depth as f32,
                    "{} < {}",
                    50.0 - t,
                    p.depth
                );
            }
        }
        assert!(checked > 100, "only {checked} overlapping pairs");
    }
}
