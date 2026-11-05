//! Narrowphase: the contact points Havok 1.7's collision agents report, written
//! from their recovered behaviour (`docs/29-havok17-backend.md`, section 10).
//!
//! A body is a set of collidables ("pieces"): convex pieces (hulls, boxes, and
//! the round primitives as a point or segment core with a radius) and triangle
//! meshes. Havok dispatches a pair of collidables on the sum of their shape
//! classes (`FUN_00543370`); the pairs *Robot Arena 2* meets are
//!
//! * convex × mesh (`CXGEOMETRIC_GEOMETRIC`, `FUN_00545b50`): one closest-point
//!   pair per triangle near the convex piece, plus the pair's persistent
//!   manifold ([`MeshManifold`]);
//! * convex × convex (`CXGEOMETRIC_CXGEOMETRIC`, `FUN_00544820` and
//!   `FUN_0055b880`): one closest-point pair, plus the pair's persistent
//!   manifold ([`ConvexManifold`]).
//!
//! Closest points come from a distance query on the Minkowski difference
//! (GJK; `FUN_0055c390` / `FUN_0055b2c0`) that also reports which vertices of
//! each shape the final simplex was built from. Those "features" decide whether
//! a new point joins the manifold (an edge or a vertex against a face, or an
//! edge against an edge) or is reported for this detection only (a vertex
//! against a vertex, a face against a face or an edge).
//!
//! A pair whose shapes overlap (distance below zero) reports no point at all:
//! Havok puts it on the penetration list and backsteps instead.

use glam::{DVec3, Quat, Vec3};

use crate::backend::ShapeDesc;

use super::x87gjk;

/// The sides of the polygon that stands in for a cylinder's round faces.
const CYLINDER_SIDES: usize = 24;

/// One collidable of a body, in the body's frame (relative to its origin).
#[derive(Clone, Debug)]
pub struct Piece {
    /// The compound child's tag (the caller's component index; `u32::MAX` for a
    /// shape that is not a compound's child). Reported on the contacts the piece
    /// makes: the original's contact event names the colliding *collidable*, one per
    /// component, which is how the game knows which component was struck.
    pub tag: u32,
    pub position: Vec3,
    pub rotation: Quat,
    pub shape: PieceShape,
    /// The smallest sphere around the piece (its centre in the piece's
    /// frame, its radius): a moving collidable's broadphase box is this
    /// sphere's (docs/29 section 12u).
    pub sphere: (Vec3, f32),
    /// For a mesh built from a `MeshHull`: the corners of its convex hull.
    /// The geometry queries of [`super::query`] (overlap depths, phantoms,
    /// ray casts) see such a piece as that hull, as the Jolt query world they
    /// replace did; the agents above collide the mesh itself.
    pub hull: Option<Vec<Vec3>>,
}

#[derive(Clone, Debug)]
pub enum PieceShape {
    /// The convex hull of `vertices` grown by `radius` (zero for polytopes; a
    /// sphere is one vertex, a capsule two).
    Convex {
        vertices: Vec<Vec3>,
        radius: f32,
    },
    Mesh(Mesh),
}

/// The pieces of `shape`, in the frame `shape` is given in.
pub fn pieces_of(shape: &ShapeDesc) -> Vec<Piece> {
    let mut out = Vec::new();
    collect(shape, Vec3::ZERO, Quat::IDENTITY, u32::MAX, &mut out);
    out
}

fn collect(shape: &ShapeDesc, position: Vec3, rotation: Quat, tag: u32, out: &mut Vec<Piece>) {
    let convex = |vertices: Vec<Vec3>, radius: f32| PieceShape::Convex { vertices, radius };
    let mut desc_hull: Option<&[Vec3]> = None;
    let shape = match shape {
        ShapeDesc::Compound(children) => {
            for c in children {
                collect(
                    &c.shape,
                    position + rotation * c.local_position,
                    rotation * c.local_rotation,
                    c.tag,
                    out,
                );
            }
            return;
        }
        ShapeDesc::OffsetCenterOfMass { shape, .. } => {
            collect(shape, position, rotation, tag, out);
            return;
        }
        ShapeDesc::Box { half_extents: h } => {
            let mut v = Vec::with_capacity(8);
            for x in [-h.x, h.x] {
                for y in [-h.y, h.y] {
                    for z in [-h.z, h.z] {
                        v.push(Vec3::new(x, y, z));
                    }
                }
            }
            convex(v, 0.0)
        }
        ShapeDesc::Sphere { radius } => convex(vec![Vec3::ZERO], *radius),
        ShapeDesc::Capsule {
            half_height,
            radius,
        } => convex(
            vec![
                Vec3::new(0.0, -half_height, 0.0),
                Vec3::new(0.0, *half_height, 0.0),
            ],
            *radius,
        ),
        ShapeDesc::Cylinder {
            half_height,
            radius,
        } => {
            let mut v = Vec::with_capacity(2 * CYLINDER_SIDES);
            for k in 0..CYLINDER_SIDES {
                let a = k as f32 * std::f32::consts::TAU / CYLINDER_SIDES as f32;
                let (s, c) = a.sin_cos();
                for y in [-half_height, *half_height] {
                    v.push(Vec3::new(c * radius, y, s * radius));
                }
            }
            convex(v, 0.0)
        }
        ShapeDesc::ConvexHull { points } => {
            let mut v = crate::massprops::hull_vertices(points);
            if v.is_empty() {
                // A flat or degenerate hull: its own points.
                for p in points {
                    if !v.iter().any(|q: &Vec3| q.distance_squared(*p) < 1e-10) {
                        v.push(*p);
                    }
                }
            }
            if v.is_empty() {
                return;
            }
            convex(v, 0.0)
        }
        ShapeDesc::TriMesh { vertices, indices } => {
            PieceShape::Mesh(Mesh::new(vertices.clone(), indices.clone()))
        }
        ShapeDesc::MeshHull {
            points,
            indices,
            raw,
        } => {
            desc_hull = Some(points);
            PieceShape::Mesh(Mesh::with_frame(points.clone(), indices.clone(), raw))
        }
    };
    let hull = match desc_hull {
        Some(points) => {
            let v = crate::massprops::hull_vertices(points);
            (v.len() >= 2).then_some(v)
        }
        None => None,
    };
    let sphere = match &shape {
        PieceShape::Convex { vertices, radius } => {
            let (c, r) = bounding_sphere(vertices);
            (c, r + radius)
        }
        PieceShape::Mesh(m) => bounding_sphere(&m.vertices),
    };
    out.push(Piece {
        tag,
        position,
        rotation,
        shape,
        sphere,
        hull,
    });
}

/// An axis-aligned box.
#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub const EMPTY: Aabb = Aabb {
        min: Vec3::splat(f32::MAX),
        max: Vec3::splat(f32::MIN),
    };

    pub fn of(points: impl IntoIterator<Item = Vec3>) -> Aabb {
        let mut b = Aabb::EMPTY;
        for p in points {
            b.min = b.min.min(p);
            b.max = b.max.max(p);
        }
        b
    }

    pub fn grown(self, by: f32) -> Aabb {
        Aabb {
            min: self.min - Vec3::splat(by),
            max: self.max + Vec3::splat(by),
        }
    }

    pub fn union(self, o: Aabb) -> Aabb {
        Aabb {
            min: self.min.min(o.min),
            max: self.max.max(o.max),
        }
    }

    pub fn overlaps(&self, o: &Aabb) -> bool {
        self.min.cmple(o.max).all() && o.min.cmple(self.max).all()
    }

    fn corners(&self) -> [Vec3; 8] {
        let (a, b) = (self.min, self.max);
        [
            Vec3::new(a.x, a.y, a.z),
            Vec3::new(b.x, a.y, a.z),
            Vec3::new(a.x, b.y, a.z),
            Vec3::new(b.x, b.y, a.z),
            Vec3::new(a.x, a.y, b.z),
            Vec3::new(b.x, a.y, b.z),
            Vec3::new(a.x, b.y, b.z),
            Vec3::new(b.x, b.y, b.z),
        ]
    }

    /// This box (in the frame `position`/`rotation` maps from) as a box around
    /// its transformed corners.
    pub fn transformed(&self, position: Vec3, rotation: Quat) -> Aabb {
        Aabb::of(self.corners().map(|c| position + rotation * c))
    }
}

/// A triangle mesh with a bounding-volume tree over its triangles (the role
/// Havok's MOPP plays: which triangles are near a box).
#[derive(Clone, Debug)]
pub struct Mesh {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
    nodes: Vec<Node>,
    /// Triangle indices in tree order.
    order: Vec<u32>,
    pub bounds: Aabb,
    /// Havok's own tree over the triangles (the order its walks meet them in).
    pub tree: super::obbtree::ObbTree,
    /// Per triangle, its place in the order a MOPP compiled from this mesh
    /// returns it (a fixed mesh's queries, docs/29 section 12v).
    pub mopp: Vec<u32>,
}

#[derive(Clone, Copy, Debug)]
struct Node {
    bounds: Aabb,
    /// Leaf: `count > 0`, triangles `order[first..first + count]`; inner:
    /// children at `first` and `first + 1`.
    first: u32,
    count: u32,
}

const LEAF_SIZE: usize = 4;

impl Mesh {
    pub fn new(vertices: Vec<Vec3>, triangles: Vec<[u32; 3]>) -> Mesh {
        Mesh::with_frame(vertices, triangles, &[])
    }

    /// A mesh whose vertices the original keeps as `raw` (the same vertices
    /// in its geometry's own frame; empty: as `vertices`): Havok's tree is
    /// built there.
    pub fn with_frame(vertices: Vec<Vec3>, triangles: Vec<[u32; 3]>, raw: &[Vec3]) -> Mesh {
        // Each triangle once: the arena soup carries every triangle in both
        // windings (Jolt's mesh is solid from the front only), while Havok's
        // closest-point agent is two-sided and the game's meshes hold each
        // triangle once (no repeated point in the live contact lists).
        let mut seen = std::collections::HashSet::new();
        let triangles: Vec<[u32; 3]> = triangles
            .into_iter()
            .filter(|t| t.iter().all(|&i| (i as usize) < vertices.len()))
            .filter(|t| {
                let mut key = t.map(|i| vertices[i as usize].to_array().map(f32::to_bits));
                key.sort_unstable();
                seen.insert(key)
            })
            .collect();
        let tri_box = |t: &[u32; 3]| Aabb::of(t.iter().map(|&i| vertices[i as usize]));
        let boxes: Vec<Aabb> = triangles.iter().map(tri_box).collect();
        let mut order: Vec<u32> = (0..triangles.len() as u32).collect();
        let mut nodes = vec![Node {
            bounds: Aabb::EMPTY,
            first: 0,
            count: 0,
        }];
        // Iterative build: (node, range of `order`).
        let mut stack = vec![(0usize, 0usize, order.len())];
        while let Some((node, lo, hi)) = stack.pop() {
            let bounds = order[lo..hi]
                .iter()
                .fold(Aabb::EMPTY, |b, &t| b.union(boxes[t as usize]));
            if hi - lo <= LEAF_SIZE {
                nodes[node] = Node {
                    bounds,
                    first: lo as u32,
                    count: (hi - lo) as u32,
                };
                continue;
            }
            let extent = bounds.max - bounds.min;
            let axis = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };
            let centre = |t: u32| {
                let b = boxes[t as usize];
                (b.min[axis] + b.max[axis]) * 0.5
            };
            order[lo..hi].sort_by(|a, b| centre(*a).total_cmp(&centre(*b)));
            let mid = (lo + hi) / 2;
            let left = nodes.len();
            nodes.push(Node {
                bounds: Aabb::EMPTY,
                first: 0,
                count: 0,
            });
            nodes.push(Node {
                bounds: Aabb::EMPTY,
                first: 0,
                count: 0,
            });
            nodes[node] = Node {
                bounds,
                first: left as u32,
                count: 0,
            };
            stack.push((left, lo, mid));
            stack.push((left + 1, mid, hi));
        }
        let bounds = nodes[0].bounds;
        let raw = if raw.len() == vertices.len() {
            raw
        } else {
            &vertices[..]
        };
        let tree = super::obbtree::ObbTree::build(raw, &vertices, &triangles);
        let mopp = super::mopp::rank(raw, &triangles);
        Mesh {
            vertices,
            triangles,
            nodes,
            order,
            bounds,
            tree,
            mopp,
        }
    }

    /// The nearest triangle a ray meets, from either side: its distance
    /// along `dir` (unit, mesh frame) within `max` and its index.
    pub fn ray(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<(f32, u32)> {
        if self.triangles.is_empty() {
            return None;
        }
        let (o, d) = (origin.as_dvec3(), dir.as_dvec3());
        let inv = DVec3::new(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
        let mut best = max as f64;
        let mut found = None;
        let slab = |b: &Aabb, best: f64| -> bool {
            let (lo, hi) = (b.min.as_dvec3(), b.max.as_dvec3());
            let mut t0 = 0.0f64;
            let mut t1 = best;
            for k in 0..3 {
                if d[k] == 0.0 {
                    if o[k] < lo[k] || o[k] > hi[k] {
                        return false;
                    }
                    continue;
                }
                let (mut a, mut b2) = ((lo[k] - o[k]) * inv[k], (hi[k] - o[k]) * inv[k]);
                if a > b2 {
                    std::mem::swap(&mut a, &mut b2);
                }
                t0 = t0.max(a);
                t1 = t1.min(b2);
                if t0 > t1 {
                    return false;
                }
            }
            true
        };
        let mut stack = vec![0usize];
        while let Some(n) = stack.pop() {
            let node = self.nodes[n];
            if !slab(&node.bounds.grown(1e-5), best) {
                continue;
            }
            if node.count > 0 {
                let (a, b) = (node.first as usize, (node.first + node.count) as usize);
                for &t in &self.order[a..b] {
                    let tri = self.triangles[t as usize];
                    let v = tri.map(|i| self.vertices[i as usize].as_dvec3());
                    // Moller-Trumbore, two-sided.
                    let (e1, e2) = (v[1] - v[0], v[2] - v[0]);
                    let p = d.cross(e2);
                    let det = e1.dot(p);
                    if det.abs() < 1e-14 {
                        continue;
                    }
                    let s = o - v[0];
                    let u = s.dot(p) / det;
                    if !(-1e-9..=1.0 + 1e-9).contains(&u) {
                        continue;
                    }
                    let q = s.cross(e1);
                    let w = d.dot(q) / det;
                    if w < -1e-9 || u + w > 1.0 + 1e-9 {
                        continue;
                    }
                    let t_hit = e2.dot(q) / det;
                    if t_hit >= 0.0 && t_hit <= best {
                        best = t_hit;
                        found = Some((t_hit as f32, t));
                    }
                }
            } else {
                stack.push(node.first as usize);
                stack.push(node.first as usize + 1);
            }
        }
        found
    }

    /// The triangles whose bounds overlap `query` (mesh frame), ascending
    /// (the callers put them in the order Havok's query gives).
    pub fn query(&self, query: &Aabb, out: &mut Vec<u32>) {
        out.clear();
        if self.triangles.is_empty() {
            return;
        }
        let mut stack = vec![0usize];
        while let Some(n) = stack.pop() {
            let node = self.nodes[n];
            if !node.bounds.overlaps(query) {
                continue;
            }
            if node.count > 0 {
                let (a, b) = (node.first as usize, (node.first + node.count) as usize);
                for &t in &self.order[a..b] {
                    let tri = self.triangles[t as usize];
                    let tb = Aabb::of(tri.iter().map(|&i| self.vertices[i as usize]));
                    if tb.overlaps(query) {
                        out.push(t);
                    }
                }
            } else {
                stack.push(node.first as usize);
                stack.push(node.first as usize + 1);
            }
        }
        out.sort_unstable();
    }
}

/// The result of a distance query between two convex vertex sets.
#[derive(Clone, Copy, Debug)]
pub struct Closest {
    /// Distance between the two hulls (0 when they overlap).
    pub distance: f32,
    /// The closest points on A and on B.
    pub on_a: Vec3,
    pub on_b: Vec3,
    /// The final simplex: per point, the vertex of A and of B it came from.
    pub simplex_a: [u16; 3],
    pub simplex_b: [u16; 3],
    pub size: usize,
    pub overlap: bool,
}

impl Closest {
    /// Whether the closest features join the pair's manifold (`FUN_0055b880`,
    /// the `switch` in `FUN_00545b50`): a two-point simplex (an edge on either
    /// side), or a three-point one with a single vertex on one side (vertex
    /// against face) or a repeated vertex on both (edge against edge). A vertex
    /// against a vertex, or a face against an edge or a face, is reported for
    /// this detection only.
    pub fn persists(&self) -> bool {
        let (a, b) = (self.simplex_a, self.simplex_b);
        match self.size {
            2 => a[0] != a[1] || b[0] != b[1],
            3 => {
                let all = |s: [u16; 3]| s[0] == s[1] && s[0] == s[2];
                let dup = |s: [u16; 3]| s[0] == s[1] || s[0] == s[2] || s[1] == s[2];
                all(a) || all(b) || (dup(a) && dup(b))
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct SimplexPoint {
    pub(super) w: DVec3,
    pub(super) a: DVec3,
    pub(super) b: DVec3,
    pub(super) ia: u16,
    pub(super) ib: u16,
}

fn dv(v: Vec3) -> DVec3 {
    DVec3::new(v.x as f64, v.y as f64, v.z as f64)
}

fn fv(v: DVec3) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}

pub(super) fn support(points: &[DVec3], d: DVec3) -> usize {
    let mut best = 0;
    let mut best_dot = f64::MIN;
    for (k, p) in points.iter().enumerate() {
        let x = p.dot(d);
        if x > best_dot {
            best_dot = x;
            best = k;
        }
    }
    best
}

/// The point of segment / triangle / tetrahedron `s` closest to the origin,
/// keeping in `s` only the points that support it (with their weights).
pub(super) fn reduce(s: &mut Vec<SimplexPoint>, lambda: &mut Vec<f64>) -> Option<DVec3> {
    match s.len() {
        1 => {
            *lambda = vec![1.0];
            Some(s[0].w)
        }
        2 => Some(segment(s, lambda)),
        3 => Some(triangle(s, lambda)),
        _ => tetrahedron(s, lambda),
    }
}

fn segment(s: &mut Vec<SimplexPoint>, lambda: &mut Vec<f64>) -> DVec3 {
    let (a, b) = (s[0].w, s[1].w);
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 0.0 {
        (-a.dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if t <= 0.0 {
        s.truncate(1);
        *lambda = vec![1.0];
        a
    } else if t >= 1.0 {
        s.remove(0);
        *lambda = vec![1.0];
        b
    } else {
        *lambda = vec![1.0 - t, t];
        a + ab * t
    }
}

/// The closest point of triangle `s` to the origin by its Voronoi regions.
fn triangle(s: &mut Vec<SimplexPoint>, lambda: &mut Vec<f64>) -> DVec3 {
    let (a, b, c) = (s[0].w, s[1].w, s[2].w);
    let (ab, ac, ap) = (b - a, c - a, -a);
    let keep = |s: &mut Vec<SimplexPoint>, idx: &[usize]| {
        let kept: Vec<SimplexPoint> = idx.iter().map(|&i| s[i]).collect();
        *s = kept;
    };
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        keep(s, &[0]);
        *lambda = vec![1.0];
        return a;
    }
    let bp = -b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        keep(s, &[1]);
        *lambda = vec![1.0];
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        keep(s, &[0, 1]);
        *lambda = vec![1.0 - v, v];
        return a + ab * v;
    }
    let cp = -c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        keep(s, &[2]);
        *lambda = vec![1.0];
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        keep(s, &[0, 2]);
        *lambda = vec![1.0 - w, w];
        return a + ac * w;
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        keep(s, &[1, 2]);
        *lambda = vec![1.0 - w, w];
        return b + (c - b) * w;
    }
    let denom = va + vb + vc;
    if denom.abs() < 1e-300 {
        // Degenerate (collinear): the best of its edges.
        let mut best: Option<(f64, Vec<SimplexPoint>, Vec<f64>, DVec3)> = None;
        for pair in [[0usize, 1], [0, 2], [1, 2]] {
            let mut e = vec![s[pair[0]], s[pair[1]]];
            let mut l = Vec::new();
            let p = segment(&mut e, &mut l);
            let d = p.length_squared();
            if best.as_ref().is_none_or(|b| d < b.0) {
                best = Some((d, e, l, p));
            }
        }
        let (_, e, l, p) = best.expect("three edges");
        *s = e;
        *lambda = l;
        return p;
    }
    let v = vb / denom;
    let w = vc / denom;
    *lambda = vec![1.0 - v - w, v, w];
    a + ab * v + ac * w
}

/// The closest point of tetrahedron `s` to the origin, or `None` when the
/// origin is inside it.
fn tetrahedron(s: &mut Vec<SimplexPoint>, lambda: &mut Vec<f64>) -> Option<DVec3> {
    let faces = [[0usize, 1, 2, 3], [0, 1, 3, 2], [0, 2, 3, 1], [1, 2, 3, 0]];
    let mut best: Option<(f64, Vec<SimplexPoint>, Vec<f64>, DVec3)> = None;
    for f in faces {
        let (p0, p1, p2, opp) = (s[f[0]].w, s[f[1]].w, s[f[2]].w, s[f[3]].w);
        let n = (p1 - p0).cross(p2 - p0);
        let side_origin = n.dot(-p0);
        let side_opp = n.dot(opp - p0);
        let scale = n.length() * (opp - p0).length();
        let flat = side_opp.abs() <= 1e-12 * scale.max(1e-300);
        if !(flat || side_origin * side_opp < 0.0) {
            continue;
        }
        let mut t = vec![s[f[0]], s[f[1]], s[f[2]]];
        let mut l = Vec::new();
        let p = triangle(&mut t, &mut l);
        let d = p.length_squared();
        if best.as_ref().is_none_or(|b| d < b.0) {
            best = Some((d, t, l, p));
        }
    }
    let (_, t, l, p) = best?;
    *s = t;
    *lambda = l;
    Some(p)
}

/// The closest points of the convex hulls of `a` and `b` (GJK on `A - B`).
pub fn closest(a: &[Vec3], b: &[Vec3]) -> Closest {
    let pa: Vec<DVec3> = a.iter().copied().map(dv).collect();
    let pb: Vec<DVec3> = b.iter().copied().map(dv).collect();
    let point = |ia: usize, ib: usize| SimplexPoint {
        w: pa[ia] - pb[ib],
        a: pa[ia],
        b: pb[ib],
        ia: ia as u16,
        ib: ib as u16,
    };
    let mut s: Vec<SimplexPoint> = vec![point(0, 0)];
    let mut lambda = vec![1.0];
    let mut v = s[0].w;
    let mut overlap = false;
    for _ in 0..64 {
        let vv = v.length_squared();
        if vv < 1e-14 {
            overlap = true;
            break;
        }
        let (ia, ib) = (support(&pa, -v), support(&pb, v));
        let w = point(ia, ib);
        // No progress toward the origin: v is the closest point.
        if vv - v.dot(w.w) <= 1e-12 * vv.max(1e-6) {
            break;
        }
        if s.iter().any(|p| p.ia == w.ia && p.ib == w.ib) {
            break;
        }
        s.push(w);
        match reduce(&mut s, &mut lambda) {
            Some(next) => {
                let stalled = next.length_squared() >= vv;
                v = next;
                if stalled {
                    break;
                }
            }
            None => {
                overlap = true;
                break;
            }
        }
    }
    let mut on_a = DVec3::ZERO;
    let mut on_b = DVec3::ZERO;
    for (p, l) in s.iter().zip(&lambda) {
        on_a += p.a * *l;
        on_b += p.b * *l;
    }
    let mut simplex_a = [0u16; 3];
    let mut simplex_b = [0u16; 3];
    for (k, p) in s.iter().take(3).enumerate() {
        simplex_a[k] = p.ia;
        simplex_b[k] = p.ib;
    }
    Closest {
        distance: if overlap {
            0.0
        } else {
            (on_a - on_b).length() as f32
        },
        on_a: fv(on_a),
        on_b: fv(on_b),
        simplex_a,
        simplex_b,
        size: s.len().min(3),
        overlap,
    }
}

/// The support vertex the original's convex shapes return (`FUN_0055b130`, the
/// shape without adjacency; a triangle's `FUN_0055b200` is the same scan): from
/// the last vertex down, a vertex replaces the best only when strictly
/// further along `d`, so a tie goes to the highest index.
fn support_high(points: &[DVec3], d: DVec3) -> usize {
    let mut best = points.len() - 1;
    let mut best_dot = points[best].dot(d);
    for k in (0..best).rev() {
        let x = points[k].dot(d);
        if x > best_dot {
            best_dot = x;
            best = k;
        }
    }
    best
}

/// The determinant of the Gram matrix of `w[idx]` with column `col` replaced
/// by ones: the cofactor `delta_X(i)` of Johnson's distance algorithm.
fn delta(w: &[DVec3], idx: &[usize], col: usize) -> f64 {
    let n = idx.len();
    let mut m = [[0.0f64; 4]; 4];
    for (r, &i) in idx.iter().enumerate() {
        for (c, &j) in idx.iter().enumerate() {
            m[r][c] = if c == col { 1.0 } else { w[i].dot(w[j]) };
        }
    }
    #[allow(clippy::needless_range_loop)]
    fn det(m: &[[f64; 4]; 4], n: usize) -> f64 {
        match n {
            1 => m[0][0],
            2 => m[0][0] * m[1][1] - m[0][1] * m[1][0],
            _ => {
                let mut d = 0.0;
                for c in 0..n {
                    let mut sub = [[0.0f64; 4]; 4];
                    for r in 1..n {
                        let mut cc = 0;
                        for k in 0..n {
                            if k != c {
                                sub[r - 1][cc] = m[r][k];
                                cc += 1;
                            }
                        }
                    }
                    let s = if c % 2 == 0 { 1.0 } else { -1.0 };
                    d += s * m[0][c] * det(&sub, n - 1);
                }
                d
            }
        }
    }
    det(&m, n)
}

/// The subsets of `0..n` in the order the original tries them: its vertices,
/// then its edges, its faces, last the whole simplex.
fn subsets(n: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    for size in 1..=n {
        let mut idx: Vec<usize> = (0..size).collect();
        loop {
            out.push(idx.clone());
            let mut k = size;
            loop {
                if k == 0 {
                    break;
                }
                k -= 1;
                if idx[k] < n - size + k {
                    idx[k] += 1;
                    for m in k + 1..size {
                        idx[m] = idx[m - 1] + 1;
                    }
                    k = usize::MAX;
                    break;
                }
            }
            if k != usize::MAX {
                break;
            }
        }
    }
    out
}

/// Where the original leaves the kept vertices of a reduced simplex (the swaps
/// at the ends of `FUN_0055f100`'s branches): ascending, except that a pair
/// `{1,2}` of three or four points comes out `(2,1)`, `{1,3}` of four `(3,1)`,
/// and the triples `{0,2,3}` and `{1,2,3}` of four `(0,3,2)` and `(3,1,2)`.
fn kept_order(n: usize, x: &[usize]) -> Vec<usize> {
    match (n, x) {
        (3 | 4, [1, 2]) => vec![2, 1],
        (4, [1, 3]) => vec![3, 1],
        (4, [0, 2, 3]) => vec![0, 3, 2],
        (4, [1, 2, 3]) => vec![3, 1, 2],
        _ => x.to_vec(),
    }
}

/// The tolerance on Johnson's tests (the literal `1e-5` in `FUN_0055f100`).
const JOHNSON_EPS: f64 = 1e-5;

/// The point of the simplex `w` closest to the origin and the subset (with its
/// weights) that supports it, as `FUN_0055f100` picks it: the first subset in
/// [`subsets`]' order whose weights are all positive and which no other vertex
/// of the simplex improves by more than [`JOHNSON_EPS`] (Johnson's test); if
/// none passes, the candidate with the smallest norm. `None` when the whole
/// simplex holds the origin.
fn johnson(w: &[DVec3]) -> Option<(Vec<usize>, Vec<f64>)> {
    let n = w.len();
    if n == 1 {
        return Some((vec![0], vec![1.0]));
    }
    let mut best: Option<(f64, Vec<usize>, Vec<f64>)> = None;
    for x in subsets(n) {
        let d: Vec<f64> = (0..x.len()).map(|c| delta(w, &x, c)).collect();
        if d.iter().any(|&v| v <= 0.0) {
            continue;
        }
        let sum: f64 = d.iter().sum();
        let lambda: Vec<f64> = d.iter().map(|v| v / sum).collect();
        if x.len() == n {
            // The origin is inside the full simplex when it has four points.
            if n == 4 {
                return None;
            }
            return Some(reorder(n, x, lambda));
        }
        let mut accepted = true;
        for j in (0..n).filter(|j| !x.contains(j)) {
            let mut xj = x.clone();
            xj.push(j);
            if delta(w, &xj, xj.len() - 1) > JOHNSON_EPS {
                accepted = false;
                break;
            }
        }
        if accepted {
            return Some(reorder(n, x, lambda));
        }
        let mut v = DVec3::ZERO;
        for (&i, l) in x.iter().zip(&lambda) {
            v += w[i] * *l;
        }
        let norm = v.length_squared();
        if best.as_ref().is_none_or(|b| norm < b.0) {
            best = Some((norm, x, lambda));
        }
    }
    best.map(|(_, x, l)| reorder(n, x, l))
}

fn reorder(n: usize, x: Vec<usize>, lambda: Vec<f64>) -> (Vec<usize>, Vec<f64>) {
    let order = kept_order(n, &x);
    let l = order
        .iter()
        .map(|i| lambda[x.iter().position(|j| j == i).expect("kept")])
        .collect();
    (order, l)
}

/// The distance query the original runs between a convex piece `a` and a
/// triangle (or convex) `b` (`FUN_0055c390`, docs/29 section 12ak): Johnson's
/// algorithm on `A - B` started from one support pair in the direction from
/// `a`'s collidable `origin` to `b`'s centroid, one more support pair per
/// round along `-v` / `v` until `|v|^2` stops falling; it reports the distance
/// of the round before the last and the points of the last round's simplex.
/// The supports break ties toward the highest vertex ([`support_high`]), the
/// tests carry an absolute tolerance, and a simplex that is a degenerate edge
/// keeps the point its tests leave on it (not a vertex).
pub fn closest_havok(a: &[Vec3], b: &[Vec3], origin: Vec3) -> Closest {
    closest_havok_with(a, b, origin, None, &mut 0)
}

/// The vertex a convex shape with adjacency returns for direction `d`
/// (`FUN_0055b000`): from the vertex `cur` (left where the last query ended),
/// move to a neighbour strictly further along `d`, scanning the old vertex's
/// whole list against the best so far, until no list improves. `cur` keeps the
/// result for the next query.
fn support_climb(points: &[DVec3], adj: &[Vec<u16>], cur: &mut usize, d: DVec3) -> usize {
    let mut best = points[*cur].dot(d);
    loop {
        let mut moved = false;
        let list = &adj[*cur];
        let mut next = *cur;
        for &n in list {
            let x = points[n as usize].dot(d);
            if x > best {
                moved = true;
                best = x;
                next = n as usize;
            }
        }
        *cur = next;
        if !moved {
            return *cur;
        }
    }
}

/// [`closest_havok`] for a convex piece with the original's adjacency lists
/// `adj` (neighbours per vertex) and its climb's start vertex `cur`.
pub fn closest_havok_with(
    a: &[Vec3],
    b: &[Vec3],
    origin: Vec3,
    adj: Option<&[Vec<u16>]>,
    cur: &mut usize,
) -> Closest {
    let pa: Vec<DVec3> = a.iter().copied().map(dv).collect();
    let pb: Vec<DVec3> = b.iter().copied().map(dv).collect();
    let centroid = pb.iter().sum::<DVec3>() / pb.len() as f64;
    let first = centroid - dv(origin);
    let mut s: Vec<SimplexPoint> = Vec::new();
    let push = |s: &mut Vec<SimplexPoint>, d: DVec3, cur: &mut usize| {
        let ia = match adj {
            Some(adj) => support_climb(&pa, adj, cur, d),
            None => support_high(&pa, d),
        };
        let ib = support_high(&pb, -d);
        s.push(SimplexPoint {
            w: pa[ia] - pb[ib],
            a: pa[ia],
            b: pb[ib],
            ia: ia as u16,
            ib: ib as u16,
        });
    };
    push(&mut s, first, cur);
    let mut prev = f64::MAX;
    let mut lambda = vec![1.0];
    let mut overlap = false;
    let mut v;
    let mut dist2 = prev;
    let mut rounds = 0;
    loop {
        let w: Vec<DVec3> = s.iter().map(|p| p.w).collect();
        let Some((keep, l)) = johnson(&w) else {
            overlap = true;
            v = DVec3::ZERO;
            break;
        };
        s = keep.iter().map(|&i| s[i]).collect();
        lambda = l;
        v = DVec3::ZERO;
        for (p, l) in s.iter().zip(&lambda) {
            v += p.w * *l;
        }
        let now = v.length_squared();
        rounds += 1;
        if prev <= now || rounds > 64 {
            dist2 = prev;
            break;
        }
        if now < 1e-10 {
            overlap = true;
            break;
        }
        prev = now;
        dist2 = now;
        push(&mut s, -v, cur);
    }
    let _ = v;
    let mut on_a = DVec3::ZERO;
    let mut on_b = DVec3::ZERO;
    for (p, l) in s.iter().zip(&lambda) {
        on_a += p.a * *l;
        on_b += p.b * *l;
    }
    let mut simplex_a = [0u16; 3];
    let mut simplex_b = [0u16; 3];
    for (k, p) in s.iter().take(3).enumerate() {
        simplex_a[k] = p.ia;
        simplex_b[k] = p.ib;
    }
    Closest {
        distance: if overlap { 0.0 } else { dist2.sqrt() as f32 },
        on_a: fv(on_a),
        on_b: fv(on_b),
        simplex_a,
        simplex_b,
        size: s.len().min(3),
        overlap,
    }
}

/// The point of triangle `t` closest to `p`, by its Voronoi regions.
fn closest_on_triangle(p: DVec3, t: &[DVec3; 3]) -> DVec3 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = va + vb + vc;
    if denom.abs() < 1e-300 {
        return a;
    }
    a + ab * (vb / denom) + ac * (vc / denom)
}

/// The closest points of segments `p1 q1` and `p2 q2`.
fn closest_segments(p1: DVec3, q1: DVec3, p2: DVec3, q2: DVec3) -> (DVec3, DVec3) {
    let (d1, d2, r) = (q1 - p1, q2 - p2, p1 - p2);
    let (a, e, f) = (d1.length_squared(), d2.length_squared(), d2.dot(r));
    let eps = 1e-18;
    let (s, t);
    if a <= eps && e <= eps {
        return (p1, p2);
    }
    if a <= eps {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= eps {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut ss = if denom > eps {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut tt = (b * ss + f) / e;
            if tt < 0.0 {
                tt = 0.0;
                ss = (-c / a).clamp(0.0, 1.0);
            } else if tt > 1.0 {
                tt = 1.0;
                ss = ((b - c) / a).clamp(0.0, 1.0);
            }
            s = ss;
            t = tt;
        }
    }
    (p1 + d1 * s, p2 + d2 * t)
}

/// Whether segment `p q` passes through triangle `t`.
fn segment_hits_triangle(p: DVec3, q: DVec3, t: &[DVec3; 3]) -> bool {
    let (e1, e2) = (t[1] - t[0], t[2] - t[0]);
    let d = q - p;
    let h = d.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1e-18 {
        return false;
    }
    let inv = 1.0 / det;
    let s = p - t[0];
    let u = s.dot(h) * inv;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let qv = s.cross(e1);
    let v = d.dot(qv) * inv;
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    let w = e2.dot(qv) * inv;
    (0.0..=1.0).contains(&w)
}

/// The closest points of two triangles (`FUN_0055e950` → `FUN_00561410`):
/// the distance and the points on `a` and on `b`; the distance is zero when
/// they intersect. Candidates are taken in a fixed order — `b`'s corners
/// against `a`, `a`'s against `b`, then the nine edge pairs — and a later one
/// wins only when strictly closer.
pub fn triangle_closest(a: &[Vec3; 3], b: &[Vec3; 3]) -> (f32, Vec3, Vec3) {
    let ta = a.map(dv);
    let tb = b.map(dv);
    for k in 0..3 {
        let (p, q) = (ta[k], ta[(k + 1) % 3]);
        let (r, s) = (tb[k], tb[(k + 1) % 3]);
        if segment_hits_triangle(p, q, &tb) || segment_hits_triangle(r, s, &ta) {
            return (0.0, fv(p), fv(p));
        }
    }
    let mut best = (f64::MAX, DVec3::ZERO, DVec3::ZERO);
    let mut take = |pa: DVec3, pb: DVec3| {
        let d = pa.distance_squared(pb);
        if d < best.0 {
            best = (d, pa, pb);
        }
    };
    for &v in &tb {
        take(closest_on_triangle(v, &ta), v);
    }
    for &v in &ta {
        take(v, closest_on_triangle(v, &tb));
    }
    for i in 0..3 {
        for j in 0..3 {
            let (pa, pb) = closest_segments(ta[i], ta[(i + 1) % 3], tb[j], tb[(j + 1) % 3]);
            take(pa, pb);
        }
    }
    (best.0.sqrt() as f32, fv(best.1), fv(best.2))
}

/// Points of a mesh × mesh pair closer than this to one already reported are
/// dropped (`tol² · 0.001` squared, `FUN_0054a2a0`).
pub const MESH_DUPLICATE: f32 = 0.001;

/// One contact point an agent reports: `normal` from B toward A, `point` the
/// midpoint of the closest points, `distance` their separation (`0x60`-byte
/// record: `+0` normal, `+0x10` point, `+0x20`/`+0x30` the points on A and B,
/// `+0x44` distance).
#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub normal: Vec3,
    pub point: Vec3,
    pub distance: f32,
    /// The closest points in A's and B's body frames (what the manifold keeps).
    pub local_a: Vec3,
    pub local_b: Vec3,
}

/// The smallest sphere holding every point (Welzl's algorithm), as its
/// centre and radius. The original bounds a moving collidable so: live
/// (`ord-scout-alarm-box__live__000`, `X` rows) every moving collidable's
/// broadphase box is a cube of half side this radius plus half the
/// tolerance, about a point fixed in its body.
pub fn bounding_sphere(points: &[Vec3]) -> (Vec3, f32) {
    fn sphere_of(r: &[DVec3]) -> (DVec3, f64) {
        match r.len() {
            0 => (DVec3::ZERO, -1.0),
            1 => (r[0], 0.0),
            2 => ((r[0] + r[1]) * 0.5, r[0].distance(r[1]) * 0.5),
            3 => {
                let (a, b, c) = (r[0], r[1], r[2]);
                let (ab, ac) = (b - a, c - a);
                let n = ab.cross(ac);
                let nn = n.length_squared();
                if nn < 1e-24 {
                    let pairs = [(a, b), (a, c), (b, c)];
                    let (p, q) = pairs
                        .into_iter()
                        .max_by(|x, y| x.0.distance(x.1).total_cmp(&y.0.distance(y.1)))
                        .unwrap();
                    return ((p + q) * 0.5, p.distance(q) * 0.5);
                }
                let o = (n.cross(ab) * ac.length_squared() + ac.cross(n) * ab.length_squared())
                    / (2.0 * nn);
                (a + o, o.length())
            }
            _ => {
                let a = r[0];
                let m = glam::DMat3::from_cols(r[1] - a, r[2] - a, r[3] - a).transpose();
                let rhs = DVec3::new(
                    (r[1] - a).length_squared(),
                    (r[2] - a).length_squared(),
                    (r[3] - a).length_squared(),
                ) * 0.5;
                if m.determinant().abs() < 1e-24 {
                    return sphere_of(&r[..3]);
                }
                let o = m.inverse() * rhs;
                (a + o, o.length())
            }
        }
    }
    fn welzl(p: &[DVec3], r: &mut Vec<DVec3>) -> (DVec3, f64) {
        if p.is_empty() || r.len() == 4 {
            return sphere_of(r);
        }
        let (last, rest) = p.split_last().unwrap();
        let s = welzl(rest, r);
        if s.1 >= 0.0 && s.0.distance(*last) <= s.1 * (1.0 + 1e-12) + 1e-12 {
            return s;
        }
        r.push(*last);
        let s = welzl(rest, r);
        r.pop();
        s
    }
    if points.is_empty() {
        return (Vec3::ZERO, 0.0);
    }
    let p: Vec<DVec3> = points.iter().map(|v| v.as_dvec3()).collect();
    let (c, r) = welzl(&p, &mut Vec::new());
    (c.as_vec3(), r as f32)
}

/// A rigid pose: body origin and orientation.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub position: Vec3,
    pub rotation: Quat,
}

impl Pose {
    pub fn apply(&self, p: Vec3) -> Vec3 {
        self.position + self.rotation * p
    }
    pub fn inverse_apply(&self, p: Vec3) -> Vec3 {
        self.rotation.inverse() * (p - self.position)
    }
}

/// The closest points of two convex pieces given in world space, with their
/// radii. `None` when they overlap.
pub fn convex_point(
    a: &[Vec3],
    ra: f32,
    b: &[Vec3],
    rb: f32,
    pose_a: &Pose,
    pose_b: &Pose,
) -> Option<(Point, Closest)> {
    point_from(closest(a, b), ra, rb, pose_a, pose_b, false)
}

/// [`convex_point`] with the original's own distance query ([`closest_havok`]);
/// `origin` is the convex piece's collidable position.
pub fn convex_point_havok(
    a: &[Vec3],
    ra: f32,
    b: &[Vec3],
    rb: f32,
    pose_a: &Pose,
    pose_b: &Pose,
    origin: Vec3,
) -> Option<(Point, Closest)> {
    point_from(closest_havok(a, b, origin), ra, rb, pose_a, pose_b, true)
}

/// [`convex_point_havok`] in the original's own arithmetic ([`super::x87gjk::query`], docs/29
/// section 12an): `local` are the hull's vertices in its frame, `hull` its world pose (the
/// collidable's 3x3 and translation); the support searches run on the local direction and the
/// world points are made from the 3x3, as the original does.
pub fn convex_point_x87(
    local: &[Vec3],
    ra: f32,
    b: &[Vec3],
    rb: f32,
    pose_a: &Pose,
    pose_b: &Pose,
    hull: &Pose,
) -> Option<(Point, Closest)> {
    if local.is_empty() || b.len() < 3 {
        return None;
    }
    let verts: Vec<[f32; 3]> = local.iter().map(|v| v.to_array()).collect();
    let q = x87gjk::Hull {
        vertices: &verts,
        adjacency: None,
        rotation: glam::Mat3::from_quat(hull.rotation).to_cols_array(),
        translation: hull.position.to_array(),
    };
    let tri = [b[0].to_array(), b[1].to_array(), b[2].to_array()];
    let mut climb = 0usize;
    let r = x87gjk::query(&q, tri, &mut climb);
    let mut simplex_a = [0u16; 3];
    let mut simplex_b = [0u16; 3];
    for (k, (a, t)) in r.hull.iter().zip(&r.tri).take(3).enumerate() {
        simplex_a[k] = *a as u16;
        simplex_b[k] = *t as u16;
    }
    let c = Closest {
        distance: r.distance,
        on_a: Vec3::from_array(r.on_a),
        on_b: Vec3::from_array(r.on_b),
        simplex_a,
        simplex_b,
        size: r.hull.len().min(3),
        overlap: r.overlap,
    };
    point_from(c, ra, rb, pose_a, pose_b, true)
}

/// The convex x convex pair in the original's own arithmetic ([`super::x87gjk::query_hulls`],
/// `FUN_0055b2c0`, docs/29 section 12ax): `local_a` / `local_b` are the hulls' vertices in their own
/// frames, `hull_a` / `hull_b` their world poses (the collidables' 3x3 and translation), `pose_a` /
/// `pose_b` the bodies'. The normal, the distance and the point are the query's own record; the
/// radii (ours, a rounded piece's) move the points along the normal as in [`point_from`].
#[allow(clippy::too_many_arguments)]
pub fn convex_pair_x87(
    local_a: &[Vec3],
    ra: f32,
    local_b: &[Vec3],
    rb: f32,
    pose_a: &Pose,
    pose_b: &Pose,
    hull_a: &Pose,
    hull_b: &Pose,
) -> Option<(Point, Closest)> {
    if local_a.is_empty() || local_b.is_empty() {
        return None;
    }
    let va: Vec<[f32; 3]> = local_a.iter().map(|v| v.to_array()).collect();
    let vb: Vec<[f32; 3]> = local_b.iter().map(|v| v.to_array()).collect();
    fn hull<'a>(v: &'a [[f32; 3]], p: &Pose) -> x87gjk::Hull<'a> {
        x87gjk::Hull {
            vertices: v,
            adjacency: None,
            rotation: glam::Mat3::from_quat(p.rotation).to_cols_array(),
            translation: p.position.to_array(),
        }
    }
    let (ha, hb) = (hull(&va, hull_a), hull(&vb, hull_b));
    let (mut ca, mut cb) = (0usize, 0usize);
    let r = x87gjk::query_hulls(&ha, &hb, &mut ca, &mut cb);
    let mut simplex_a = [0u16; 3];
    let mut simplex_b = [0u16; 3];
    for (k, (a, b)) in r.a.iter().zip(&r.b).take(3).enumerate() {
        simplex_a[k] = *a as u16;
        simplex_b[k] = *b as u16;
    }
    let c = Closest {
        distance: r.distance,
        on_a: Vec3::from_array(r.on_a),
        on_b: Vec3::from_array(r.on_b),
        simplex_a,
        simplex_b,
        size: r.a.len().min(4),
        overlap: r.overlap,
    };
    if c.overlap || c.distance <= 1e-6 {
        return None;
    }
    let distance = c.distance - ra - rb;
    if distance < 0.0 {
        return None;
    }
    let normal = Vec3::from_array(r.normal);
    let on_a = c.on_a - normal * ra;
    let on_b = c.on_b + normal * rb;
    let point = if ra == 0.0 && rb == 0.0 {
        Vec3::from_array(r.point)
    } else {
        (on_a + on_b) * 0.5
    };
    Some((
        Point {
            normal,
            point,
            distance,
            local_a: pose_a.inverse_apply(on_a),
            local_b: pose_b.inverse_apply(on_b),
        },
        c,
    ))
}

fn point_from(
    c: Closest,
    ra: f32,
    rb: f32,
    pose_a: &Pose,
    pose_b: &Pose,
    from_points: bool,
) -> Option<(Point, Closest)> {
    if c.overlap || c.distance <= 1e-6 {
        return None;
    }
    let distance = c.distance - ra - rb;
    if distance < 0.0 {
        return None;
    }
    // The original's normal is the last simplex's point over its length, not
    // the previous round's distance ([`closest_havok`]).
    let normal = if from_points {
        (c.on_a - c.on_b).try_normalize().unwrap_or(Vec3::Y)
    } else {
        (c.on_a - c.on_b) / c.distance
    };
    let on_a = c.on_a - normal * ra;
    let on_b = c.on_b + normal * rb;
    Some((
        Point {
            normal,
            point: (on_a + on_b) * 0.5,
            distance,
            local_a: pose_a.inverse_apply(on_a),
            local_b: pose_b.inverse_apply(on_b),
        },
        c,
    ))
}

/// The persistent manifold of a convex × mesh pair (`FUN_00545b50`).
///
/// Each detection: every kept point is re-measured from its two body-fixed
/// points and dropped once they are a tolerance apart (in any direction); its
/// normal stays the one it was found with and its distance is measured along
/// it. Then each triangle near the convex piece gives its closest-point pair;
/// one whose features persist replaces a kept point within the tolerance of it
/// whose normal is within `acos 0.8` of its own, or joins the manifold; the
/// others are reported once. Last, every kept point is reported.
#[derive(Clone, Debug, Default)]
pub struct MeshManifold {
    pub points: Vec<Point>,
    /// The square of the tolerance when the pair was first seen (`+0x14`).
    pub tol2: f32,
    pub seen: u64,
}

/// The normal agreement for a new point to replace a kept one (`0.8`, the
/// literal in `FUN_00545b50`).
pub const SAME_NORMAL: f32 = 0.8;

impl MeshManifold {
    /// Re-measure the kept points with the bodies where they are now.
    pub fn refresh(&mut self, pose_a: &Pose, pose_b: &Pose) {
        let tol2 = self.tol2;
        let mut k = self.points.len();
        while k > 0 {
            k -= 1;
            let p = &mut self.points[k];
            let wa = pose_a.apply(p.local_a);
            let wb = pose_b.apply(p.local_b);
            let d = wa - wb;
            if d.length_squared() >= tol2 {
                self.points.swap_remove(k);
            } else {
                p.point = (wa + wb) * 0.5;
                p.distance = d.dot(p.normal);
            }
        }
    }

    /// Offer one triangle's closest point whose features persist.
    pub fn offer(&mut self, new: Point) {
        for p in &mut self.points {
            if p.point.distance_squared(new.point) < self.tol2
                && p.normal.dot(new.normal) > SAME_NORMAL
            {
                *p = new;
                return;
            }
        }
        self.points.push(new);
    }
}

/// The persistent manifold of a convex × convex pair (`FUN_0055b880`).
///
/// The new closest point is always reported. A kept point is dropped once its
/// two body-fixed points are a tolerance apart; otherwise it is reported again
/// — with the *new* point's normal and its own separation `|p_A - p_B|` — if it
/// lies at least four tolerances from the new point, and if it lies closer it
/// keeps the new point out of the manifold.
#[derive(Clone, Debug, Default)]
pub struct ConvexManifold {
    pub points: Vec<Point>,
    pub seen: u64,
}

/// How far a kept convex point must be from the new one to be reported (in
/// tolerances; `param_5² · 16` in `FUN_0055b880`).
pub const CONVEX_SPACING: f32 = 4.0;

impl ConvexManifold {
    /// The points to report besides `new` (whose features `persists` says
    /// whether it may join the manifold).
    pub fn update(
        &mut self,
        new: Point,
        persists: bool,
        tol: f32,
        pose_a: &Pose,
        pose_b: &Pose,
    ) -> Vec<Point> {
        let mut out = Vec::new();
        let mut join = persists;
        let mut k = self.points.len();
        let spacing2 = (CONVEX_SPACING * tol) * (CONVEX_SPACING * tol);
        while k > 0 {
            k -= 1;
            let p = self.points[k];
            let wa = pose_a.apply(p.local_a);
            let wb = pose_b.apply(p.local_b);
            let d2 = wa.distance_squared(wb);
            if d2 >= tol * tol {
                self.points.swap_remove(k);
                continue;
            }
            let mid = (wa + wb) * 0.5;
            if mid.distance_squared(new.point) >= spacing2 {
                out.push(Point {
                    normal: new.normal,
                    point: mid,
                    distance: d2.sqrt(),
                    local_a: p.local_a,
                    local_b: p.local_b,
                });
            } else {
                join = false;
            }
        }
        if join {
            self.points.push(new);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(c: Vec3, h: f32) -> Vec<Vec3> {
        let mut v = Vec::new();
        for x in [-h, h] {
            for y in [-h, h] {
                for z in [-h, h] {
                    v.push(c + Vec3::new(x, y, z));
                }
            }
        }
        v
    }

    /// The smallest sphere, not the box's: a long thin triangle's is the
    /// circle on its longest side; the live boxes of the Scout's chassis
    /// (1.0377 m) and ALARM's (1.1072 m, more than half its longest span)
    /// come out so too.
    #[test]
    fn the_bounding_sphere_is_the_smallest() {
        let (c, r) = bounding_sphere(&[
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 0.2, 0.0),
        ]);
        assert!(
            c.distance(Vec3::ZERO) < 1e-6 && (r - 1.0).abs() < 1e-6,
            "{c} {r}"
        );
        let cube = cube(Vec3::new(1.0, 2.0, 3.0), 0.5);
        let (c, r) = bounding_sphere(&cube);
        assert!(c.distance(Vec3::new(1.0, 2.0, 3.0)) < 1e-5, "{c}");
        assert!((r - 0.75f32.sqrt()).abs() < 1e-5, "{r}");
    }

    #[test]
    fn the_distance_between_two_boxes_is_the_gap_between_their_faces() {
        let a = cube(Vec3::new(0.0, 1.3, 0.0), 0.5);
        let b = cube(Vec3::ZERO, 0.5);
        let c = closest(&a, &b);
        assert!(!c.overlap);
        assert!((c.distance - 0.3).abs() < 1e-5, "{c:?}");
        assert!((c.on_a.y - 0.8).abs() < 1e-5 && (c.on_b.y - 0.5).abs() < 1e-5);
    }

    #[test]
    fn overlapping_hulls_report_an_overlap() {
        let a = cube(Vec3::new(0.2, 0.7, 0.1), 0.5);
        let b = cube(Vec3::ZERO, 0.5);
        assert!(closest(&a, &b).overlap);
    }

    /// A corner over a face: the simplex holds one vertex of the corner's
    /// shape against the face (three of the triangle's), so it persists.
    #[test]
    fn a_corner_over_a_face_persists_and_a_corner_over_a_corner_does_not() {
        let tri = vec![
            Vec3::new(-5.0, 0.0, -5.0),
            Vec3::new(5.0, 0.0, -5.0),
            Vec3::new(0.0, 0.0, 5.0),
        ];
        let rot = Quat::from_rotation_x(0.6) * Quat::from_rotation_z(0.5);
        let corner: Vec<Vec3> = cube(Vec3::ZERO, 0.5)
            .into_iter()
            .map(|p| rot * p + Vec3::new(0.3, 2.0, 0.2))
            .collect();
        let low = corner.iter().map(|p| p.y).fold(f32::MAX, f32::min);
        let c = closest(&corner, &tri);
        assert!((c.distance - low).abs() < 1e-4, "{c:?} vs {low}");
        assert!(c.persists(), "{c:?}");
        // Apex to apex: one vertex of each, reported once.
        let near = vec![
            Vec3::ZERO,
            Vec3::new(-1.0, 1.0, 0.0),
            Vec3::new(-1.0, -1.0, 0.3),
            Vec3::new(-1.0, 0.0, -1.0),
        ];
        let far: Vec<Vec3> = near
            .iter()
            .map(|p| Vec3::new(1.0 - p.x, p.y, p.z))
            .collect();
        let c = closest(&near, &far);
        assert!((c.distance - 1.0).abs() < 1e-5, "{c:?}");
        assert_eq!(c.size, 1, "{c:?}");
        assert!(!c.persists());
    }

    #[test]
    fn two_triangles_meet_at_their_closest_features() {
        let floor = [
            Vec3::new(-5.0, 0.0, -5.0),
            Vec3::new(5.0, 0.0, -5.0),
            Vec3::new(0.0, 0.0, 5.0),
        ];
        // A corner 0.07 above the floor.
        let tilted = [
            Vec3::new(0.0, 0.07, 0.0),
            Vec3::new(0.3, 0.2, 0.0),
            Vec3::new(0.0, 0.25, 0.3),
        ];
        let (d, on_floor, on_tri) = triangle_closest(&floor, &tilted);
        assert!((d - 0.07).abs() < 1e-5, "{d}");
        assert!(on_floor.y.abs() < 1e-6 && (on_tri.y - 0.07).abs() < 1e-6);
        // Crossing edges: an edge-edge pair is the closest.
        let a = [
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
        ];
        let b = [
            Vec3::new(0.0, 0.1, -1.0),
            Vec3::new(0.0, 0.1, 1.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let (d, pa, pb) = triangle_closest(&a, &b);
        assert!((d - 0.1).abs() < 1e-5 && pa.length() < 1e-5 && (pb.y - 0.1).abs() < 1e-5);
        // Interpenetrating: zero.
        let c = [
            Vec3::new(0.0, -0.5, -1.0),
            Vec3::new(0.0, -0.5, 1.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        assert_eq!(triangle_closest(&a, &c).0, 0.0);
    }

    #[test]
    fn the_tree_finds_the_triangles_under_a_box() {
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for i in 0..20 {
            for j in 0..20 {
                let base = vertices.len() as u32;
                let (x, z) = (i as f32, j as f32);
                vertices.extend([
                    Vec3::new(x, 0.0, z),
                    Vec3::new(x + 1.0, 0.0, z),
                    Vec3::new(x, 0.0, z + 1.0),
                ]);
                triangles.push([base, base + 1, base + 2]);
            }
        }
        let mesh = Mesh::new(vertices, triangles);
        let mut out = Vec::new();
        mesh.query(
            &Aabb {
                min: Vec3::new(4.2, -0.1, 7.2),
                max: Vec3::new(4.8, 0.1, 7.6),
            },
            &mut out,
        );
        assert_eq!(out, vec![4 * 20 + 7]);
    }

    #[test]
    fn a_kept_mesh_point_is_dropped_once_its_body_points_part_by_the_tolerance() {
        let still = Pose {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        };
        let mut m = MeshManifold {
            tol2: 0.01,
            ..Default::default()
        };
        m.offer(Point {
            normal: Vec3::Y,
            point: Vec3::new(0.0, 0.04, 0.0),
            distance: 0.08,
            local_a: Vec3::new(0.0, 0.08, 0.0),
            local_b: Vec3::ZERO,
        });
        // Slid 0.05 m: still kept, distance along the old normal.
        m.refresh(
            &Pose {
                position: Vec3::new(0.05, 0.0, 0.0),
                rotation: Quat::IDENTITY,
            },
            &still,
        );
        assert_eq!(m.points.len(), 1);
        assert!((m.points[0].distance - 0.08).abs() < 1e-6);
        // Slid 0.07 m: |(0.07, 0.08, 0)| > 0.1, dropped.
        m.refresh(
            &Pose {
                position: Vec3::new(0.07, 0.0, 0.0),
                rotation: Quat::IDENTITY,
            },
            &still,
        );
        assert!(m.points.is_empty());
    }

    #[test]
    fn a_new_mesh_point_replaces_a_kept_one_nearby_with_the_same_normal() {
        let mut m = MeshManifold {
            tol2: 0.01,
            ..Default::default()
        };
        let at = |x: f32, n: Vec3| Point {
            normal: n,
            point: Vec3::new(x, 0.0, 0.0),
            distance: 0.05,
            local_a: Vec3::ZERO,
            local_b: Vec3::ZERO,
        };
        m.offer(at(0.0, Vec3::Y));
        m.offer(at(0.05, Vec3::Y));
        assert_eq!(m.points.len(), 1);
        assert_eq!(m.points[0].point.x, 0.05);
        // A different normal (cos 0.6) or a point a tolerance away joins.
        m.offer(at(0.06, Vec3::new(0.8, 0.6, 0.0)));
        m.offer(at(0.3, Vec3::Y));
        assert_eq!(m.points.len(), 3);
    }

    #[test]
    fn a_kept_convex_point_near_the_new_one_is_not_reported_and_blocks_it() {
        let pose = Pose {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        };
        let p = |x: f32| Point {
            normal: Vec3::Y,
            point: Vec3::new(x, 0.025, 0.0),
            distance: 0.05,
            local_a: Vec3::new(x, 0.05, 0.0),
            local_b: Vec3::new(x, 0.0, 0.0),
        };
        let mut m = ConvexManifold::default();
        assert!(m.update(p(0.0), true, 0.1, &pose, &pose).is_empty());
        assert_eq!(m.points.len(), 1);
        // 0.3 m away (< 4 tol): not reported, and the new point stays out.
        assert!(m.update(p(0.3), true, 0.1, &pose, &pose).is_empty());
        assert_eq!(m.points.len(), 1);
        // 0.5 m away: the kept point is reported with the new normal, and the
        // new one joins.
        let out = m.update(
            Point {
                normal: Vec3::new(0.0, 1.0, 0.0),
                ..p(0.5)
            },
            true,
            0.1,
            &pose,
            &pose,
        );
        assert_eq!(out.len(), 1);
        assert!((out[0].distance - 0.05).abs() < 1e-6);
        assert_eq!(m.points.len(), 2);
    }
}

#[cfg(test)]
mod havok_gjk_tests {
    use super::*;
    use glam::Mat3;

    // A live distance query (`gjkquery-sentinel-wideload-compressor__live__000`,
    // seq 12371): the Wide Load's N-12 wheel (the 26-vertex hull, axle along z)
    // over a floor triangle, a tenth of a second before the bell landing. The
    // original's simplex was hull vertices (15, 3, 15) against the triangle's
    // (0, 1, 2): a point on the wheel's axial edge, z = 0.0625 between the rings
    // at -0.155 and +0.155, not a ring vertex.
    const HULL: [f32; 78] = [
        -4.636496e-09,
        -1.8475019e-08,
        -0.15503708,
        -0.32593724,
        3.0740143e-08,
        -0.15503706,
        -0.2822699,
        0.16296865,
        -0.1550371,
        -0.16296858,
        0.28226995,
        -0.15503713,
        5.8825837e-08,
        0.3259372,
        -0.15503715,
        0.16296868,
        0.2822699,
        -0.15503715,
        0.28226998,
        0.16296855,
        -0.15503712,
        0.32593724,
        -9.618452e-08,
        -0.15503709,
        0.2822699,
        -0.16296871,
        -0.15503703,
        0.16296855,
        -0.28227,
        -0.15503702,
        -9.659317e-08,
        -0.32593727,
        -0.155037,
        -0.16296871,
        -0.2822699,
        -0.155037,
        -0.28226998,
        -0.16296855,
        -0.15503702,
        -0.32593724,
        6.770386e-08,
        0.15503709,
        -0.2822699,
        0.16296868,
        0.15503703,
        -0.16296856,
        0.28227,
        0.15503702,
        6.806677e-08,
        0.32593727,
        0.155037,
        0.16296868,
        0.28226992,
        0.155037,
        0.28226998,
        0.16296858,
        0.15503702,
        0.32593724,
        -5.9220802e-08,
        0.15503706,
        0.2822699,
        -0.16296868,
        0.1550371,
        0.16296855,
        -0.28226995,
        0.15503713,
        -8.7352234e-08,
        -0.3259372,
        0.15503715,
        -0.1629687,
        -0.28226987,
        0.15503715,
        -0.28226998,
        -0.16296852,
        0.15503712,
        4.60444e-09,
        1.84887e-08,
        0.15503708,
    ];
    const XF: [f32; 12] = [
        2.2454674e-06,
        0.2834564,
        0.9589854,
        9.3105893e-07,
        -0.9589851,
        0.2834563,
        1.0000002,
        2.806676e-08,
        -2.2846357e-06,
        -18.216131,
        5.447981,
        -14.062784,
    ];
    const TRI: [f32; 9] = [
        -19.961012, 4.9526205, -11.271542, -13.905169, 4.9526205, -17.55698, -19.961012, 4.9526205,
        -17.55698,
    ];

    fn world() -> (Vec<Vec3>, [Vec3; 3], Mat3, Vec3) {
        let rot = Mat3::from_cols_slice(&XF[..9]);
        let t = Vec3::new(XF[9], XF[10], XF[11]);
        let hull = HULL
            .chunks(3)
            .map(|v| rot * Vec3::new(v[0], v[1], v[2]) + t)
            .collect();
        let tri = [0, 1, 2].map(|k| Vec3::new(TRI[3 * k], TRI[3 * k + 1], TRI[3 * k + 2]));
        (hull, tri, rot, t)
    }

    #[test]
    fn a_wheels_axial_edge_over_the_floor_gives_the_original_edge_point() {
        let (hull, tri, rot, t) = world();
        let c = closest_havok(&hull, &tri, t);
        let mut ring: Vec<u16> = c.simplex_a[..c.size].to_vec();
        ring.sort_unstable();
        ring.dedup();
        assert_eq!(ring, vec![3, 15], "{c:?}");
        let local = rot.transpose() * (c.on_a - t);
        assert!(
            (local.z - 0.0625).abs() < 2e-3 && (local.x + 0.163).abs() < 2e-3,
            "{local}"
        );
        assert!((c.distance - 0.178_473).abs() < 1e-4, "{}", c.distance);
        // An edge against a face is reported for this detection only.
        assert!(!c.persists());
    }

    /// The supports break ties toward the highest vertex and a triangle's three
    /// vertices are all equally low over a floor: the simplex's triangle side
    /// is 2, 1, 0 in the order the rounds reach them.
    #[test]
    fn a_tie_goes_to_the_highest_vertex() {
        let pts: Vec<DVec3> = vec![DVec3::ZERO, DVec3::X, DVec3::new(0.0, 0.0, 1.0)];
        assert_eq!(support_high(&pts, DVec3::NEG_Y), 2);
        assert_eq!(support_high(&pts, DVec3::X), 1);
    }

    #[test]
    fn johnsons_test_keeps_an_edge_when_no_other_point_improves_it_by_more_than_the_tolerance() {
        // Origin projects inside the segment w0-w1.
        let w = [DVec3::new(-1.0, 0.5, 0.0), DVec3::new(1.0, 0.5, 0.0)];
        let (keep, l) = johnson(&w).unwrap();
        assert_eq!(keep, vec![0, 1]);
        assert!((l[0] - 0.5).abs() < 1e-12);
        // Origin beyond the end: the nearer vertex alone.
        let w = [DVec3::new(2.0, 0.5, 0.0), DVec3::new(3.0, 0.5, 0.0)];
        let (keep, _) = johnson(&w).unwrap();
        assert_eq!(keep, vec![0]);
        // The pair {1,2} of a triangle is kept in the order (2, 1).
        let w = [
            DVec3::new(-5.0, 4.0, 0.0),
            DVec3::new(-1.0, 0.5, 0.0),
            DVec3::new(1.0, 0.5, 0.0),
        ];
        let (keep, _) = johnson(&w).unwrap();
        assert_eq!(keep, vec![2, 1]);
    }

    /// Four points around the origin: it is inside, and the query reports an overlap.
    #[test]
    fn a_tetrahedron_around_the_origin_is_an_overlap() {
        let w = [
            DVec3::new(1.0, 1.0, 1.0),
            DVec3::new(-1.0, -1.0, 1.0),
            DVec3::new(-1.0, 1.0, -1.0),
            DVec3::new(1.0, -1.0, -1.0),
        ];
        assert!(johnson(&w).is_none());
    }
}

#[cfg(test)]
mod cxcx_tests {
    use super::*;

    fn cube(h: f32) -> Vec<Vec3> {
        (0..8)
            .map(|i| {
                Vec3::new(
                    if i & 1 == 0 { -h } else { h },
                    if i & 2 == 0 { -h } else { h },
                    if i & 4 == 0 { -h } else { h },
                )
            })
            .collect()
    }

    fn at(p: Vec3) -> Pose {
        Pose {
            position: p,
            rotation: Quat::IDENTITY,
        }
    }

    /// Two boxes a hand apart on x: the gap, the normal from B toward A, the point halfway, the
    /// persisting features (a face against a face is reported for this detection only) and the
    /// body-fixed points.
    #[test]
    fn two_boxes_a_gap_apart_give_the_gap_the_normal_and_the_midpoint() {
        let (pa, pb) = (at(Vec3::new(1.05, 0.0, 0.0)), at(Vec3::ZERO));
        let a = cube(0.5);
        let (p, c) =
            convex_pair_x87(&a, 0.0, &a, 0.0, &pa, &pb, &pa, &pb).expect("apart, not overlapping");
        assert!((p.distance - 0.05).abs() < 1e-6, "{}", p.distance);
        assert!((p.normal - Vec3::X).length() < 1e-6, "{:?}", p.normal);
        assert!((p.point.x - 0.525).abs() < 1e-6, "{:?}", p.point);
        assert!(c.size >= 1);
        assert!((pb.apply(p.local_b).x - 0.5).abs() < 1e-6);
        assert!((pa.apply(p.local_a).x - 0.55).abs() < 1e-6);
        // Overlapping boxes report nothing.
        let pa = at(Vec3::new(0.5, 0.0, 0.0));
        assert!(convex_pair_x87(&a, 0.0, &a, 0.0, &pa, &pb, &pa, &pb).is_none());
    }
}
