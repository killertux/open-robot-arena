//! The bounding-volume tree Havok 1.x keeps over a moving mesh's triangles
//! (collidable `+0x1c8`, built by `FUN_0053a700` from the triangles in index
//! order), and the orders its walks visit the triangles in (docs/29 section
//! 12u). The order is what matters here: it is the order the agents meet the
//! triangles in, so the order of a pair's contact points and of the
//! resolver's impulses.
//!
//! The build (recovered): a set of one triangle is a leaf. Otherwise the
//! triangles' centroids (`(a + b + c) · 0.33333334`) give a mean and a variance
//! per axis of the mesh's frame (`FUN_0053ac00`, single precision: the sums in
//! order, then times `1/n`, the variance `Σc²/n − mean²`); the split axis is the
//! one with the largest variance (x only when strictly larger than both, then y
//! when strictly larger than both, else z), and the triangles whose centroid
//! lies below the mean on it go to the first child, the others to the second,
//! each keeping its order; when one side is empty the other's last triangle
//! moves over. The first child is visited first by every walk (`FUN_00550560`,
//! `FUN_0054a080`, `FUN_00550100`). Each node also gets an oriented box fitted
//! to its triangles (`FUN_00557320`: the area-weighted covariance of the
//! triangles, `FUN_00680650`, its eigenvectors as axes, the vertices'
//! extents along them); only its volume matters to the order (the two-tree
//! walk splits the bigger box), and ours is fitted the same way.
use glam::{DMat3, DVec3, Mat3, Vec3};

use super::narrow::Aabb;

#[derive(Clone, Debug)]
struct Node {
    /// Children (first, second), or `None` for a leaf.
    children: Option<(u32, u32)>,
    /// The leaf's triangle.
    tri: u32,
    /// The triangles' box in the mesh's frame (ours, for the walks' pruning).
    bounds: Aabb,
    /// The fitted oriented box's volume, `8 hx hy hz`.
    volume: f32,
}

#[derive(Clone, Debug, Default)]
pub struct ObbTree {
    nodes: Vec<Node>,
    /// Per triangle: its leaf's place in the depth-first order.
    pub rank: Vec<u32>,
}

fn centroid(t: [Vec3; 3]) -> Vec3 {
    Vec3::new(
        (t[0].x + t[1].x + t[2].x) * 0.333_333_34,
        (t[0].y + t[1].y + t[2].y) * 0.333_333_34,
        (t[0].z + t[1].z + t[2].z) * 0.333_333_34,
    )
}

/// The axis `FUN_0053a700` splits along and the mean it splits at.
fn split(tris: &[[Vec3; 3]], set: &[u32]) -> (usize, f32) {
    let mut sum = Vec3::ZERO;
    let mut sq = Vec3::ZERO;
    for &t in set {
        let c = centroid(tris[t as usize]);
        sum += c;
        sq += c * c;
    }
    let inv = 1.0 / set.len() as f32;
    let mean = sum * inv;
    let var = sq * inv - mean * mean;
    let axis = if var.x > var.y && var.x > var.z {
        0
    } else if var.y > var.x && var.y > var.z {
        1
    } else {
        2
    };
    (axis, mean[axis])
}

/// The volume of the oriented box `FUN_00557320` fits to the triangles.
fn obb_volume(tris: &[[Vec3; 3]], set: &[u32]) -> f32 {
    let mut area = 0.0f64;
    let mut mean = DVec3::ZERO;
    let mut c = DMat3::ZERO;
    for &t in set {
        let [p, q, r] = tris[t as usize].map(|v| v.as_dvec3());
        let a = (p - q).cross(p - r).length() * 0.5;
        let m = (p + q + r) / 3.0;
        let outer = |u: DVec3| DMat3::from_cols(u * u.x, u * u.y, u * u.z);
        area += a;
        mean += m * a;
        c += (outer(m) * 9.0 + outer(p) + outer(q) + outer(r)) * a;
    }
    let axes = if area > 0.0 {
        mean /= area;
        let cov =
            c * (1.0 / 12.0) - DMat3::from_cols(mean * mean.x, mean * mean.y, mean * mean.z) * area;
        jacobi(cov)
    } else {
        DMat3::IDENTITY
    };
    let axes = Mat3::from_cols(
        axes.x_axis.as_vec3(),
        axes.y_axis.as_vec3(),
        axes.z_axis.as_vec3(),
    );
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for &t in set {
        for v in tris[t as usize] {
            let p = axes.transpose() * v;
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    let h = (hi - lo) * 0.5;
    8.0 * h.x * h.y * h.z
}

/// The eigenvectors (columns) of a symmetric matrix, by Jacobi rotations.
fn jacobi(mut a: DMat3) -> DMat3 {
    let mut v = DMat3::IDENTITY;
    for _ in 0..50 {
        let off = a.x_axis.y.powi(2) + a.x_axis.z.powi(2) + a.y_axis.z.powi(2);
        if off < 1e-30 {
            break;
        }
        for (p, q) in [(0usize, 1usize), (0, 2), (1, 2)] {
            let apq = a.col(q)[p];
            if apq.abs() < 1e-300 {
                continue;
            }
            let (app, aqq) = (a.col(p)[p], a.col(q)[q]);
            let theta = (aqq - app) / (2.0 * apq);
            let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
            let t = if theta == 0.0 { 1.0 } else { t };
            let cs = 1.0 / (t * t + 1.0).sqrt();
            let sn = t * cs;
            let mut j = DMat3::IDENTITY;
            *j.col_mut(p) = {
                let mut c = DVec3::ZERO;
                c[p] = cs;
                c[q] = -sn;
                c
            };
            *j.col_mut(q) = {
                let mut c = DVec3::ZERO;
                c[p] = sn;
                c[q] = cs;
                c
            };
            a = j.transpose() * a * j;
            v *= j;
        }
    }
    v
}

impl ObbTree {
    /// The tree over `triangles`: split as their vertices `raw` lie in the
    /// frame the original keeps the geometry in, its boxes and volumes from
    /// the same vertices as `placed` (the piece's frame, where the walks
    /// measure).
    pub fn build(raw: &[Vec3], placed: &[Vec3], triangles: &[[u32; 3]]) -> ObbTree {
        let tris: Vec<[Vec3; 3]> = triangles
            .iter()
            .map(|t| t.map(|i| raw[i as usize]))
            .collect();
        let boxes: Vec<[Vec3; 3]> = triangles
            .iter()
            .map(|t| t.map(|i| placed[i as usize]))
            .collect();
        let mut tree = ObbTree {
            nodes: Vec::new(),
            rank: vec![0; tris.len()],
        };
        if tris.is_empty() {
            return tree;
        }
        let all: Vec<u32> = (0..tris.len() as u32).collect();
        tree.node(&tris, &boxes, all);
        // Depth first, the first child first.
        let mut stack = vec![0u32];
        let mut k = 0;
        while let Some(n) = stack.pop() {
            let node = &tree.nodes[n as usize];
            match node.children {
                Some((a, b)) => {
                    stack.push(b);
                    stack.push(a);
                }
                None => {
                    tree.rank[node.tri as usize] = k;
                    k += 1;
                }
            }
        }
        tree
    }

    fn node(&mut self, tris: &[[Vec3; 3]], boxes: &[[Vec3; 3]], set: Vec<u32>) -> u32 {
        let bounds = set
            .iter()
            .fold(Aabb::EMPTY, |b, &t| b.union(Aabb::of(boxes[t as usize])));
        let index = self.nodes.len() as u32;
        self.nodes.push(Node {
            children: None,
            tri: set[0],
            bounds,
            volume: obb_volume(boxes, &set),
        });
        if set.len() == 1 {
            return index;
        }
        let (axis, mean) = split(tris, &set);
        let (mut below, mut above): (Vec<u32>, Vec<u32>) = set
            .iter()
            .partition(|&&t| centroid(tris[t as usize])[axis] < mean);
        if above.is_empty() {
            above.push(below.pop().unwrap());
        }
        if below.is_empty() {
            below.push(above.pop().unwrap());
        }
        let a = self.node(tris, boxes, below);
        let b = self.node(tris, boxes, above);
        self.nodes[index as usize].children = Some((a, b));
        index
    }

    /// The pairs of triangles, one of each tree, whose boxes come within
    /// `margin` of each other, in the order the two-tree walk (`FUN_00550100`)
    /// meets them: at each pair of nodes the one with the bigger box is split
    /// (the first tree's when the second's is a leaf, the second's when the
    /// first's is), the first child first. `b_to_a` maps the second mesh's
    /// frame into the first's.
    pub fn pairs(
        a: &ObbTree,
        b: &ObbTree,
        b_to_a: (Mat3, Vec3),
        margin: f32,
        out: &mut Vec<(u32, u32)>,
    ) {
        out.clear();
        if a.nodes.is_empty() || b.nodes.is_empty() {
            return;
        }
        let (rot, pos) = b_to_a;
        let in_a = |bx: &Aabb| {
            let c = (bx.min + bx.max) * 0.5;
            let h = (bx.max - bx.min) * 0.5;
            let ar = Mat3::from_cols(rot.x_axis.abs(), rot.y_axis.abs(), rot.z_axis.abs());
            let e = ar * h;
            let cc = rot * c + pos;
            Aabb {
                min: cc - e,
                max: cc + e,
            }
        };
        let mut stack = vec![(0u32, 0u32)];
        while let Some((i, j)) = stack.pop() {
            let (na, nb) = (&a.nodes[i as usize], &b.nodes[j as usize]);
            if !na.bounds.grown(margin).overlaps(&in_a(&nb.bounds)) {
                continue;
            }
            match (na.children, nb.children) {
                (None, None) => out.push((na.tri, nb.tri)),
                (Some((a0, a1)), None) => {
                    stack.push((a1, j));
                    stack.push((a0, j));
                }
                (Some((a0, a1)), Some(_)) if nb.volume < na.volume => {
                    stack.push((a1, j));
                    stack.push((a0, j));
                }
                (_, Some((b0, b1))) => {
                    stack.push((i, b1));
                    stack.push((i, b0));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A strip of four triangles along x, listed out of order: the walk
    /// meets them by position along the strip, lowest first.
    #[test]
    fn the_leaves_come_by_the_split_below_the_mean_first() {
        let v: Vec<Vec3> = (0..6)
            .flat_map(|i| [Vec3::new(i as f32, 0.0, 0.0), Vec3::new(i as f32, 0.0, 0.3)])
            .collect();
        let quad = |i: u32| [2 * i, 2 * i + 2, 2 * i + 1];
        let tris = vec![quad(3), quad(0), quad(4), quad(1)];
        let t = ObbTree::build(&v, &v, &tris);
        assert_eq!(t.rank, vec![2, 0, 3, 1]);
    }
}
