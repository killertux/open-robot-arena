//! GJK: whether two convex point sets overlap (the lab's collision judge, docs/36 9.29).
//!
//! The support point of a set along a direction is its farthest point; the algorithm walks a simplex of
//! Minkowski-difference support points toward the origin and says "overlap" when the simplex encloses it.

use glam::Vec3;

fn support(points: &[Vec3], dir: Vec3) -> Vec3 {
    let mut best = points[0];
    let mut best_dot = best.dot(dir);
    for p in &points[1..] {
        let d = p.dot(dir);
        if d > best_dot {
            best = *p;
            best_dot = d;
        }
    }
    best
}

fn minkowski(a: &[Vec3], b: &[Vec3], dir: Vec3) -> Vec3 {
    support(a, dir) - support(b, -dir)
}

/// Whether the convex hulls of two point sets share a point (touching counts as sharing).
pub fn overlap(a: &[Vec3], b: &[Vec3]) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let mut dir = Vec3::X;
    let mut simplex: Vec<Vec3> = vec![minkowski(a, b, dir)];
    dir = -simplex[0];
    for _ in 0..64 {
        if dir.length_squared() < 1e-14 {
            return true;
        }
        let p = minkowski(a, b, dir);
        if p.dot(dir) < 0.0 {
            return false;
        }
        simplex.push(p);
        if update(&mut simplex, &mut dir) {
            return true;
        }
    }
    true
}

/// Reduce the simplex to the feature nearest the origin and point `dir` at the origin from it. `true` when
/// the origin is inside a tetrahedron.
fn update(simplex: &mut Vec<Vec3>, dir: &mut Vec3) -> bool {
    match simplex.len() {
        2 => {
            let (a, b) = (simplex[1], simplex[0]);
            let ab = b - a;
            let ao = -a;
            if ab.dot(ao) > 0.0 {
                *dir = ab.cross(ao).cross(ab);
                if dir.length_squared() < 1e-14 {
                    // the origin lies on the segment
                    return true;
                }
            } else {
                *simplex = vec![a];
                *dir = ao;
            }
            false
        }
        3 => {
            let (a, b, c) = (simplex[2], simplex[1], simplex[0]);
            let ab = b - a;
            let ac = c - a;
            let ao = -a;
            let abc = ab.cross(ac);
            if abc.cross(ac).dot(ao) > 0.0 {
                if ac.dot(ao) > 0.0 {
                    *simplex = vec![c, a];
                    *dir = ac.cross(ao).cross(ac);
                } else {
                    *simplex = vec![b, a];
                    return update(simplex, dir);
                }
            } else if ab.cross(abc).dot(ao) > 0.0 {
                *simplex = vec![b, a];
                return update(simplex, dir);
            } else if abc.dot(ao) > 0.0 {
                *dir = abc;
            } else {
                *simplex = vec![b, c, a];
                *dir = -abc;
            }
            false
        }
        4 => {
            let (a, b, c, d) = (simplex[3], simplex[2], simplex[1], simplex[0]);
            let ao = -a;
            let ab = b - a;
            let ac = c - a;
            let ad = d - a;
            let abc = ab.cross(ac);
            let acd = ac.cross(ad);
            let adb = ad.cross(ab);
            if abc.dot(ao) > 0.0 {
                *simplex = vec![c, b, a];
                return update(simplex, dir);
            }
            if acd.dot(ao) > 0.0 {
                *simplex = vec![d, c, a];
                return update(simplex, dir);
            }
            if adb.dot(ao) > 0.0 {
                *simplex = vec![b, d, a];
                return update(simplex, dir);
            }
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(centre: Vec3, half: f32) -> Vec<Vec3> {
        let mut v = Vec::new();
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                for z in [-1.0, 1.0] {
                    v.push(centre + Vec3::new(x, y, z) * half);
                }
            }
        }
        v
    }

    #[test]
    fn cubes_overlap_when_they_share_volume_and_not_when_apart() {
        let a = cube(Vec3::ZERO, 1.0);
        assert!(overlap(&a, &cube(Vec3::new(1.5, 0.2, -0.3), 1.0)));
        assert!(!overlap(&a, &cube(Vec3::new(2.5, 0.0, 0.0), 1.0)));
        assert!(!overlap(&a, &cube(Vec3::new(2.5, 2.5, 2.5), 1.0)));
        assert!(
            overlap(&a, &cube(Vec3::new(0.2, 0.0, 0.0), 0.3)),
            "one inside the other"
        );
    }

    #[test]
    fn a_round_part_beside_a_box_corner_is_clear_where_their_boxes_overlap() {
        // a disc of radius 1 in the xy plane centred at (1.6, 1.6): its bounding box reaches (0.6, 0.6),
        // the unit box's corner is at (1, 1) which is 0.85 from the centre: inside the box, outside... check
        // a corner point at distance > 1 from the disc's centre.
        let mut disc = Vec::new();
        for i in 0..64 {
            let t = i as f32 / 64.0 * std::f32::consts::TAU;
            for z in [-0.1, 0.1] {
                disc.push(Vec3::new(1.6 + t.cos(), 1.6 + t.sin(), z));
            }
        }
        let bx = cube(Vec3::ZERO, 1.0);
        // box corner (1,1,*) is sqrt(0.72) = 0.85 from the centre: inside the disc, so they overlap
        assert!(overlap(&bx, &disc));
        let far = cube(Vec3::new(-0.2, -0.2, 0.0), 1.0);
        // corner (0.8, 0.8): distance sqrt(1.28) = 1.13 from the centre: outside the disc
        assert!(!overlap(&far, &disc));
    }
}
