//! Chassis step 2's point drag: what the original checks when a top-outline point is moved.
//!
//! Recovered from `Robot Arena 2.exe` and checked against live drags (docs/36 section 9.27e,
//! `botlab-adjust-*`). Two layers judge a moved point:
//!
//! * the **editing area** (the widget class at vtable `0x6a7c6c`; its move handler is `0x478c20`)
//!   asks the outline validator `FUN_004a10d0` whether the polygon with the new position is
//!   acceptable: [`check_move`]. A refusal leaves the point where it was and raises one of three
//!   orange alerts (`FUN_004a0df0` -> `FUN_0043e7c0`), **but only once any earlier move has been
//!   accepted** (`DAT_006f52e0`, set by every accepted move, never cleared by the step 2 screen);
//! * the **lab** (`FUN_0043d820` -> `FUN_0044d1b0`) then moves the mesh vertices at that corner and
//!   recomputes every face normal. For each of the two sides next to the point the two triangles of
//!   the side are compared: when `n1 . n2 < 0.75` (`0x6a69d8`) the move is undone and the alert
//!   *Can't move that point because it will bend one side too much* (`0x6f2118`) is raised: [`bends`].

use ra2fmt::bot::Chassis;

/// Points closer than this to the moved one refuse it (`FUN_004a1290`: `ftol(length) < 100`).
pub const MIN_GAP: f64 = 100.0;
/// Half-width of the strip around an edge no other point may enter (`0x42c60000` = 99.0).
pub const EDGE_GAP: f64 = 99.0;
/// Two consecutive edge directions whose dot product is below this make a corner too sharp
/// (`FUN_004a1660`: `-0.95`, the float `0x3f733333` at `0x6f52e4`).
pub const SHARP_DOT: f64 = -0.95;
/// The side's two face normals must agree at least this much (`0x6a69d8`).
pub const BEND_DOT: f32 = 0.75;

/// The editing area's three refusals, in the order `FUN_004a10d0` tries them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleAlert {
    /// `0x6f533c`.
    TooClose,
    /// `0x6f5310`.
    TooSharp,
    /// `0x6f52ec`.
    Crossing,
}

impl RuleAlert {
    pub fn text(self) -> &'static str {
        match self {
            RuleAlert::TooClose => "Points should not be too close to other points or lines",
            RuleAlert::TooSharp => "Corners of outline should not be too sharp",
            RuleAlert::Crossing => "Lines should not cross each other",
        }
    }
}

/// What one move of a top point came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopMove {
    /// The point is on its new place, the mesh follows.
    Moved,
    /// The outline rules accepted the place, which is the point's own already.
    Unchanged,
    /// The editing area refused the place (it arms nothing and moves nothing).
    Refused(RuleAlert),
    /// The outline rules accepted it and the lab undid it: a side would bend too much.
    Bend,
}

impl TopMove {
    /// Whether the outline rules passed (`FUN_004a10d0` returned 1 and set `DAT_006f52e0`).
    pub fn validated(self) -> bool {
        !matches!(self, TopMove::Refused(_))
    }
}

/// The alert of a move the lab refuses after the mesh was built (`0x6f2118`).
pub const BEND_TEXT: &str = "Can't move that point because it will bend one side too much";

type P = [f64; 2];

fn pt(p: [i32; 2]) -> P {
    [p[0] as f64, p[1] as f64]
}

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: P, b: P) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn length(a: P) -> f64 {
    dot(a, a).sqrt()
}

/// Whether the segments `p`-`q` and `r`-`s` meet, as `FUN_0061b080` answers: `Some((kind, t))`,
/// kind 1 a single point at parameter `t` along `p`-`q`, kind 2 an overlap.
fn meet(p: P, q: P, r: P, s: P) -> Option<(u8, f64)> {
    let d = sub(q, p);
    let e = sub(s, r);
    let denom = cross(d, e);
    let w = sub(r, p);
    if denom.abs() > 1e-9 {
        let t = cross(w, e) / denom;
        let u = cross(w, d) / denom;
        return ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some((1, t));
    }
    if cross(w, d).abs() > 1e-9 * length(d).max(1.0) {
        return None;
    }
    let len2 = dot(d, d);
    if len2 == 0.0 {
        return None;
    }
    let (a, b) = (dot(w, d) / len2, dot(sub(s, p), d) / len2);
    let (lo, hi) = (a.min(b).max(0.0), a.max(b).min(1.0));
    if lo > hi {
        None
    } else if hi - lo > 1e-9 {
        Some((2, lo))
    } else {
        Some((1, lo))
    }
}

/// Whether `v` lies within [`EDGE_GAP`] of the edge `a`-`b` across it: the perpendicular from `v`
/// of that length, either way, meets the edge (`FUN_0061b080` on `v +- 99 n`).
fn beside(v: P, a: P, b: P) -> bool {
    let d = sub(b, a);
    let len = length(d);
    if len == 0.0 {
        return false;
    }
    let along = dot(sub(v, a), d) / (len * len);
    let across = cross(d, sub(v, a)) / len;
    (0.0..=1.0).contains(&along) && across.abs() <= EDGE_GAP
}

/// `FUN_004a10d0`: whether moving point `index` of the closed outline `points` to `to` is
/// acceptable, and if not which refusal comes first.
pub fn check_move(points: &[[i32; 2]], index: usize, to: [i32; 2]) -> Result<(), RuleAlert> {
    let n = points.len();
    if index >= n {
        return Ok(());
    }
    let mut poly: Vec<P> = points.iter().map(|p| pt(*p)).collect();
    poly[index] = pt(to);
    let moved = poly[index];

    // `FUN_004a1290`: other points, then the edges not touching the moved point.
    for (j, p) in poly.iter().enumerate() {
        if j != index && length(sub(*p, moved)).trunc() < MIN_GAP {
            return Err(RuleAlert::TooClose);
        }
    }
    if n > 1 {
        for i in 0..n {
            let touches = i == index || i + 1 == index || (index == 0 && i == n - 1);
            if !touches && beside(moved, poly[i], poly[(i + 1) % n]) {
                return Err(RuleAlert::TooClose);
            }
        }
    }

    // `FUN_004a1660`: consecutive edge directions.
    if n > 2 {
        let dir = |i: usize| {
            let d = sub(poly[(i + 1) % n], poly[i]);
            let l = length(d);
            if l > 1e-6 {
                [d[0] / l, d[1] / l]
            } else {
                d
            }
        };
        for i in 0..n {
            if dot(dir(i), dir((i + 1) % n)) < SHARP_DOT {
                return Err(RuleAlert::TooSharp);
            }
        }
    }

    // `FUN_004a1a50`: edges that meet away from their ends, then the points beside the two edges
    // of the moved one. (Two points are one edge both ways round, which is not a crossing: live,
    // the second point of a new outline is taken.)
    if n > 2 {
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            for j in 0..n {
                if j == i {
                    continue;
                }
                let (c, d) = (poly[j], poly[(j + 1) % n]);
                if let Some((kind, t)) = meet(a, b, c, d) {
                    let at_an_end = (-0.001..0.001).contains(&t) || (0.999..1.001).contains(&t);
                    if kind != 1 || !at_an_end {
                        return Err(RuleAlert::Crossing);
                    }
                }
            }
        }
        for e in [index, (index + n - 1) % n] {
            for (u, v) in poly.iter().enumerate() {
                if u == e || u == (e + 1) % n {
                    continue;
                }
                if beside(*v, poly[e], poly[(e + 1) % n]) {
                    return Err(RuleAlert::Crossing);
                }
            }
        }
    }
    Ok(())
}

/// The same three rules for a point about to be added to the outline (chassis step 1): the polygon
/// with the new point last, judged as a moved point `points.len()` is.
pub fn check_add(points: &[[i32; 2]], to: [i32; 2]) -> Result<(), RuleAlert> {
    let mut ring = points.to_vec();
    ring.push(to);
    check_move(&ring, points.len(), to)
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The unit normal of the triangle `(a, b, c)` as `FUN_0044d1b0` takes it: `(b - a) x (c - a)`,
/// normalised (a degenerate triangle keeps a zero normal).
pub fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let n = cross3(sub3(b, a), sub3(c, a));
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if l > 1e-12 {
        // `+ 0.0`: no live file has a `-0` (the original's printer never shows a negative zero here)
        [n[0] / l + 0.0, n[1] / l + 0.0, n[2] / l + 0.0]
    } else {
        [0.0; 3]
    }
}

/// `FUN_0044d1b0`'s bend test on a chassis whose mesh is already the extrusion of its outlines:
/// whether either of the two sides next to top point `index` has two face normals that disagree
/// (dot below [`BEND_DOT`]). A side is the quad `b_i b_i+1 t_i+1 t_i` after the two rings, its
/// triangles `(v1 v2 v0)` and `(v2 v3 v0)`.
pub fn bends(ch: &Chassis, index: usize) -> bool {
    let Some(mesh) = ch.mesh.as_ref() else {
        return false;
    };
    let n = ch.bottom.len();
    if n < 3 || index >= n || mesh.points.len() < 6 * n {
        return false;
    }
    [index, (index + n - 1) % n].into_iter().any(|side| {
        let base = 2 * n + 4 * side;
        let v = |k: usize| mesh.points[base + k];
        let a = face_normal(v(1), v(2), v(0));
        let b = face_normal(v(2), v(3), v(0));
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2] < BEND_DOT
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<[i32; 2]> {
        vec![[200, 200], [800, 200], [800, 800], [200, 800]]
    }

    #[test]
    fn a_point_may_slide_inside_the_outline() {
        assert_eq!(check_move(&square(), 0, [300, 200]), Ok(()));
        assert_eq!(check_move(&square(), 0, [400, 300]), Ok(()));
    }

    #[test]
    fn a_point_onto_another_or_beside_an_edge_is_too_close() {
        // live `botlab-adjust-alert-close`: (800,300) lies on the edge (800,200)-(800,800)
        assert_eq!(
            check_move(&square(), 0, [800, 300]),
            Err(RuleAlert::TooClose)
        );
        assert_eq!(
            check_move(&square(), 0, [750, 200]),
            Err(RuleAlert::TooClose)
        );
        // 100 units from the edge is allowed, 99 is not
        assert_eq!(check_move(&square(), 0, [700, 500]), Ok(()));
        assert_eq!(
            check_move(&square(), 0, [701, 500]),
            Err(RuleAlert::TooClose)
        );
    }

    #[test]
    fn a_spike_is_too_sharp_and_a_crossing_is_refused() {
        // live `botlab-adjust-alert-sharp` and `-cross`
        assert_eq!(
            check_move(&square(), 0, [900, 100]),
            Err(RuleAlert::TooSharp)
        );
        assert_eq!(
            check_move(&square(), 0, [900, 500]),
            Err(RuleAlert::Crossing)
        );
    }

    #[test]
    fn a_whole_triangle_has_no_sharp_corner_to_begin_with() {
        let tri = [[200, 200], [800, 200], [500, 800]];
        assert_eq!(check_move(&tri, 0, [200, 200]), Ok(()));
        assert_eq!(check_move(&tri, 1, [800, 400]), Ok(()));
    }

    #[test]
    fn the_rule_alerts_read_as_the_exe_does() {
        assert_eq!(
            RuleAlert::TooClose.text(),
            "Points should not be too close to other points or lines"
        );
        assert_eq!(
            RuleAlert::TooSharp.text(),
            "Corners of outline should not be too sharp"
        );
        assert_eq!(
            RuleAlert::Crossing.text(),
            "Lines should not cross each other"
        );
    }

    /// The two dot products the original computed for the second point of the live triangle at
    /// (800,500) (`botlab-adjust-trace`, `dot` rows): 0.8250 for the side towards the third point and
    /// 0.6984 for the first side, which is below 0.75.
    #[test]
    fn the_face_normals_dot_as_the_original_traced() {
        use crate::workshop::extrude::{extrude, DEFAULT_HEIGHT};
        let bottom = [[200, 200], [800, 200], [500, 800]];
        let top = [[200, 200], [800, 500], [500, 800]];
        let e = extrude(&bottom, &top, DEFAULT_HEIGHT).unwrap();
        let ch = Chassis {
            bottom: bottom.to_vec(),
            top: top.to_vec(),
            height: DEFAULT_HEIGHT,
            mesh: Some(e.mesh),
            ..Default::default()
        };
        let mesh = ch.mesh.as_ref().unwrap();
        let dot_of = |side: usize| {
            let base = 6 + 4 * side;
            let v = |k: usize| mesh.points[base + k];
            let a = face_normal(v(1), v(2), v(0));
            let b = face_normal(v(2), v(3), v(0));
            a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
        };
        assert!((dot_of(1) - 0.8250).abs() < 1e-3, "{}", dot_of(1));
        assert!((dot_of(0) - 0.6984).abs() < 1e-3, "{}", dot_of(0));
        assert!(bends(&ch, 1));
    }

    /// Chassis step 1 asks the same three questions of a point about to be added (live
    /// `botlab-step1-refused-*`: a point on an edge, one that crosses, one that makes a sharp corner).
    #[test]
    fn a_point_added_in_step_one_is_judged_by_the_same_rules() {
        let square = [[200, 200], [800, 200], [800, 800], [200, 800]];
        assert_eq!(check_add(&square, [500, 200]), Err(RuleAlert::TooClose));
        assert_eq!(check_add(&square, [900, 500]), Err(RuleAlert::Crossing));
        assert_eq!(check_add(&square, [700, 700]), Err(RuleAlert::TooSharp));
        assert_eq!(check_add(&square[..3], [200, 800]), Ok(()));
        assert_eq!(check_add(&[], [200, 200]), Ok(()));
        assert_eq!(
            check_add(&[[200, 200]], [800, 200]),
            Ok(()),
            "the second point is taken"
        );
    }
}
