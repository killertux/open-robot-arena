//! The original's distance query between a convex hull and a triangle (`FUN_0055c390` and the
//! functions it calls), in the original's own arithmetic (docs/29 section 12an).
//!
//! Section 12ak recovered the algorithm and `closest_havok` ran it in `f64` with Gram determinants:
//! 97.7 % of the 43 689 live queries of `gjkquery-sentinel-wideload-compressor__live__000` came out
//! with the same simplex. The rest are degenerate simplexes (four coplanar points, an edge parallel
//! to the triangle) whose Johnson tests (`delta <= 1e-5`) the original decides with the rounding of
//! its own evaluation: the cofactors come from cached dot products that are stored as `f32` between
//! the tests, the weights are products of stored `f32`s, and the candidate norms mix stored and
//! register values. The x87 of the game's thread runs with a 53-bit significand (control word
//! `0x27f`), so a register is an `f64` and a store to a float slot is a rounding to `f32`.
//! [`reduce`] keeps that schedule block by block (the stack slots as a table, [`Slots`]) and [`query`]
//! runs the support searches in the hull's own frame and the world points from the 3x3 and the
//! translation, in the original's order of operations.
//!
//! Checked against the original's own code run natively (the PE image mapped at its addresses, the
//! control word `0x27f`): 172 738 random, flat, collinear and duplicate-point simplexes for [`reduce`]
//! and all 43 689 queries of the capture for [`query`], bit for bit (simplex, weights, distance,
//! points, climb vertex). `tools/oracle/gjknative/` has the harness.

// ---------------------------------------------------------------------------
// FUN_0055f100: the Johnson reduction of the simplex, one round
// ---------------------------------------------------------------------------

/// The tolerance of every Johnson test: the literal `1e-5` (as an `f32`).
pub const EPS: f64 = 9.999999747378752e-6;
/// `FLT_MAX`, the first "best" norm of the fallback.
pub const FLT_MAX: f64 = 3.4028234663852886e38;

/// A store to (or a load from) a 32-bit float slot.
#[inline]
pub fn r(x: f64) -> f64 {
    x as f32 as f64
}

type P = [f64; 3];

#[inline]
fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `(d . p)` summed z, then y, then x.
#[inline]
fn zyx(d: P, p: P) -> f64 {
    ((d[2] * p[2]) + (d[1] * p[1])) + (d[0] * p[0])
}

/// What the reduction leaves behind.
#[derive(Clone, Debug, PartialEq)]
pub struct Reduced {
    /// `false` when the tetrahedron holds the origin (the original returns 0).
    pub ok: bool,
    /// The number of points kept.
    pub count: usize,
    /// The weights of the kept points (`f32` values), in their new order.
    pub lambda: [f64; 4],
    /// `order[k]` is the old index of the point now at position `k` (all four positions).
    pub order: [usize; 4],
}

fn swapped(order: &mut [usize; 4], a: usize, b: usize) {
    order.swap(a, b);
}

/// The reduction of `w[..n]`.
pub fn reduce(w: &[P], n: usize) -> Reduced {
    let mut out = Reduced {
        ok: true,
        count: n,
        lambda: [0.0; 4],
        order: [0, 1, 2, 3],
    };
    match n {
        1 => {
            out.lambda[0] = 1.0;
        }
        2 => reduce2(w, &mut out),
        3 => reduce3(w, &mut out),
        4 => reduce4(w, &mut out),
        _ => {}
    }
    out
}

fn reduce2(w: &[P], out: &mut Reduced) {
    let e01 = r(zyx(sub(w[0], w[1]), w[0]));
    if e01 <= EPS {
        out.count = 1;
        out.lambda[0] = 1.0;
        return;
    }
    let d1 = zyx(sub(w[1], w[0]), w[1]); // stays in a register
    if d1 <= EPS {
        out.count = 1;
        out.lambda[0] = 1.0;
        swapped(&mut out.order, 0, 1);
        return;
    }
    let s = 1.0 / (e01 + d1);
    out.lambda[0] = r(s * d1);
    out.lambda[1] = r(s * e01);
}

fn reduce3(w: &[P], out: &mut Reduced) {
    let e = |i: usize, j: usize| r(zyx(sub(w[i], w[j]), w[i]));
    let e01 = e(0, 1);
    let e02 = e(0, 2);
    if e01 <= EPS && e02 <= EPS {
        out.count = 1;
        out.lambda[0] = 1.0;
        return;
    }
    let e10 = e(1, 0);
    let e12 = e(1, 2);
    if e10 <= EPS && e12 <= EPS {
        out.count = 1;
        out.lambda[0] = 1.0;
        swapped(&mut out.order, 0, 1);
        return;
    }
    let e20 = e(2, 0);
    let e21 = e(2, 1);
    if e20 <= EPS && e21 <= EPS {
        out.count = 1;
        out.lambda[0] = 1.0;
        swapped(&mut out.order, 0, 2);
        return;
    }
    // The fallback: the last thing seen, vertex 2 to the front to start with.
    let mut fb_lambda = [1.0, 0.0, 0.0, 0.0];
    let mut fb_count = 1usize;
    let (mut fb_b, mut fb_c) = (2usize, 0usize);

    // Edge 01.
    let t01 = zyx(sub(w[0], w[2]), w[1]);
    let f2 = r((t01 * e01) + (e10 * e02));
    if e10 > 0.0 && e01 > 0.0 {
        let s = 1.0 / (e10 + e01);
        if f2 <= EPS {
            out.count = 2;
            out.lambda[0] = r(e10 * s);
            out.lambda[1] = r(s * e01);
            return;
        }
        fb_lambda[0] = r(e10 * s);
        fb_lambda[1] = r(s * e01);
        fb_count = 2;
    }
    // Edge 02.
    let t02 = zyx(sub(w[0], w[1]), w[2]);
    let f1 = r((t02 * e02) + (e20 * e01));
    if e20 > 0.0 && e02 > 0.0 {
        let s = 1.0 / (e20 + e02);
        if f1 <= EPS {
            out.count = 2;
            out.lambda[0] = r(e20 * s);
            out.lambda[1] = r(s * e02);
            swapped(&mut out.order, 1, 2);
            return;
        }
        fb_lambda[0] = r(e20 * s);
        fb_lambda[1] = r(s * e02);
        fb_count = 2;
        fb_b = 1;
        fb_c = 2;
    }
    // Edge 12.
    let t12 = zyx(sub(w[1], w[0]), w[2]);
    let f0 = r((t12 * e12) + (e21 * e10));
    if e21 > 0.0 && e12 > 0.0 {
        let s = 1.0 / (e21 + e12);
        if f0 <= EPS {
            out.count = 2;
            out.lambda[0] = r(s * e12);
            out.lambda[1] = r(e21 * s);
            swapped(&mut out.order, 0, 2);
            return;
        }
        fb_lambda[0] = r(s * e12);
        fb_lambda[1] = r(e21 * s);
        fb_count = 2;
        fb_b = 0;
        fb_c = 2;
    }
    // The face.
    if f0 > 0.0 && f1 > 0.0 && f2 > 0.0 {
        let s = 1.0 / ((f0 + f1) + f2);
        out.count = 3;
        out.lambda[0] = r(f0 * s);
        out.lambda[1] = r(f1 * s);
        out.lambda[2] = r(s * f2);
        return;
    }
    out.count = fb_count;
    out.lambda = fb_lambda;
    swapped(&mut out.order, fb_b, fb_c);
}

/// `((dy * py) + (dx * px)) + (dz * pz)`.
#[inline]
fn yxz(d: P, p: P) -> f64 {
    ((d[1] * p[1]) + (d[0] * p[0])) + (d[2] * p[2])
}

/// `((dx * px) + (dz * pz)) + (dy * py)`.
#[inline]
fn xzy(d: P, p: P) -> f64 {
    ((d[0] * p[0]) + (d[2] * p[2])) + (d[1] * p[1])
}

/// `((px * px) + (py * py)) + (pz * pz)`.
#[inline]
fn norm_xyz(p: P) -> f64 {
    ((p[0] * p[0]) + (p[1] * p[1])) + (p[2] * p[2])
}

/// The four-point case. The original keeps its values in 32-bit stack slots between the tests, and a
/// later test may read a slot an earlier block overwrote, so the slots are kept by their stack offsets.
struct Slots {
    m: [f64; 0x48],
}

impl Slots {
    fn g(&self, off: usize) -> f64 {
        self.m[off / 4]
    }
    fn set(&mut self, off: usize, v: f64) -> f64 {
        let v = r(v);
        self.m[off / 4] = v;
        v
    }
}

fn reduce4(w: &[P], out: &mut Reduced) {
    let mut s = Slots { m: [0.0; 0x48] };
    // The best candidate so far: weights, count, and the swaps that bring its points to the front.
    let mut best_lambda = [0.0f64; 4];
    let mut best_count = 0usize;
    let mut swaps: Vec<(usize, usize)> = Vec::new();
    let one_swap = |swaps: &mut Vec<(usize, usize)>, a: usize, b: usize| {
        *swaps = vec![(a, b)];
    };

    // Vertex 0.
    s.set(0x84, zyx(sub(w[0], w[1]), w[0]));
    s.set(0x80, zyx(sub(w[0], w[2]), w[0]));
    s.set(0x7c, zyx(sub(w[0], w[3]), w[0]));
    if s.g(0x84) <= EPS && s.g(0x80) <= EPS && s.g(0x7c) <= EPS {
        out.count = 1;
        out.lambda[0] = 1.0;
        return;
    }
    let n0 = (w[0][2] * w[0][2] + w[0][1] * w[0][1]) + w[0][0] * w[0][0];
    if n0 < FLT_MAX {
        s.set(0x18, n0);
        best_lambda[0] = 1.0;
        best_count = 1;
    }
    // Vertex 1.
    s.set(0x88, yxz(sub(w[1], w[0]), w[1]));
    s.set(0x50, yxz(sub(w[1], w[2]), w[1]));
    s.set(0x4c, yxz(sub(w[1], w[3]), w[1]));
    if s.g(0x88) <= EPS && s.g(0x50) <= EPS && s.g(0x4c) <= EPS {
        out.count = 1;
        out.lambda[0] = 1.0;
        swapped(&mut out.order, 0, 1);
        return;
    }
    let n1 = norm_xyz(w[1]);
    if n1 < s.g(0x18) {
        s.set(0x18, n1);
        best_lambda = [1.0, 0.0, 0.0, 0.0];
        best_count = 1;
        one_swap(&mut swaps, 1, 0);
    }
    // Vertex 2.
    s.set(0x48, yxz(sub(w[2], w[0]), w[2]));
    s.set(0x1c, yxz(sub(w[2], w[1]), w[2]));
    s.set(0x3c, yxz(sub(w[2], w[3]), w[2]));
    if s.g(0x48) <= EPS && s.g(0x1c) <= EPS && s.g(0x3c) <= EPS {
        out.count = 1;
        out.lambda[0] = 1.0;
        swapped(&mut out.order, 0, 2);
        return;
    }
    let n2 = norm_xyz(w[2]);
    if n2 < s.g(0x18) {
        s.set(0x18, n2);
        best_lambda = [1.0, 0.0, 0.0, 0.0];
        best_count = 1;
        one_swap(&mut swaps, 2, 0);
    }
    // Vertex 3.
    s.set(0x58, zyx(sub(w[3], w[0]), w[3]));
    s.set(0x38, zyx(sub(w[3], w[1]), w[3]));
    s.set(0x54, zyx(sub(w[3], w[2]), w[3]));
    if s.g(0x58) <= EPS && s.g(0x38) <= EPS && s.g(0x54) <= EPS {
        out.count = 1;
        out.lambda[0] = 1.0;
        swapped(&mut out.order, 0, 3);
        return;
    }
    let n3 = norm_xyz(w[3]);
    if n3 < s.g(0x18) {
        s.set(0x18, n3);
        best_lambda = [1.0, 0.0, 0.0, 0.0];
        best_count = 1;
        one_swap(&mut swaps, 3, 0);
    }

    macro_rules! accept {
        ($count:expr, $lam:expr, $swaps:expr) => {{
            out.count = $count;
            for (k, v) in $lam.iter().enumerate() {
                out.lambda[k] = *v;
            }
            for &(a, b) in $swaps.iter() {
                swapped(&mut out.order, a, b);
            }
            return;
        }};
    }
    // The components of an edge candidate's point `v`, from the stored products, and its norm.
    // Edge 01.
    s.set(0x100, zyx(sub(w[0], w[2]), w[1]));
    s.set(0xf0, zyx(sub(w[0], w[3]), w[1]));
    s.set(0x44, (s.g(0x100) * s.g(0x84)) + (s.g(0x88) * s.g(0x80)));
    s.set(0x40, (s.g(0xf0) * s.g(0x84)) + (s.g(0x88) * s.g(0x7c)));
    if s.g(0x88) > 0.0 && s.g(0x84) > 0.0 {
        let inv = s.set(0x10, 1.0 / (s.g(0x88) + s.g(0x84)));
        if s.g(0x44) <= EPS && s.g(0x40) <= EPS {
            accept!(2, [r(inv * s.g(0x88)), r(inv * s.g(0x84))], []);
        }
        let l = s.set(0x10, inv * s.g(0x84));
        let a4 = s.set(0xa4, l * w[1][2]);
        let y = s.set(0x2c, (inv * s.g(0x88)) * w[0][1]);
        let z = s.set(0x30, (inv * s.g(0x88)) * w[0][2]);
        let x = s.set(
            0x6c,
            ((inv * s.g(0x88)) * w[0][0]) + ((inv * s.g(0x84)) * w[1][0]),
        );
        let vy = y + (l * w[1][1]);
        let norm = ((x * x) + ((z + a4) * (z + a4))) + (vy * vy);
        if norm < s.g(0x18) {
            s.set(0x18, norm);
            best_lambda[0] = r(inv * s.g(0x88));
            best_lambda[1] = l;
            best_count = 2;
        }
    }
    // Edge 02.
    s.set(0xf4, xzy(sub(w[0], w[1]), w[2]));
    s.set(0xec, xzy(sub(w[0], w[3]), w[2]));
    s.set(0xc0, (s.g(0xf4) * s.g(0x80)) + (s.g(0x48) * s.g(0x84)));
    s.set(0x20, (s.g(0xec) * s.g(0x80)) + (s.g(0x48) * s.g(0x7c)));
    if s.g(0x48) > 0.0 && s.g(0x80) > 0.0 {
        let inv = s.set(0x10, 1.0 / (s.g(0x48) + s.g(0x80)));
        if s.g(0xc0) <= EPS && s.g(0x20) <= EPS {
            accept!(2, [r(inv * s.g(0x48)), r(inv * s.g(0x80))], [(1, 2)]);
        }
        let l = s.set(0x10, inv * s.g(0x80));
        let a4 = s.set(0xa4, l * w[2][2]);
        let y = s.set(0x2c, (inv * s.g(0x48)) * w[0][1]);
        let z = s.set(0x30, (inv * s.g(0x48)) * w[0][2]);
        let x = s.set(
            0x6c,
            ((inv * s.g(0x48)) * w[0][0]) + ((inv * s.g(0x80)) * w[2][0]),
        );
        let vy = y + (l * w[2][1]);
        let norm = ((x * x) + ((z + a4) * (z + a4))) + (vy * vy);
        if norm < s.g(0x18) {
            s.set(0x18, norm);
            best_lambda[0] = r(inv * s.g(0x48));
            best_lambda[1] = l;
            best_count = 2;
            swaps = vec![(1, 2)];
        }
    }
    // Edge 03.
    s.set(0x104, zyx(sub(w[0], w[1]), w[3]));
    s.set(0xf8, zyx(sub(w[0], w[2]), w[3]));
    s.set(0xbc, (s.g(0x104) * s.g(0x7c)) + (s.g(0x58) * s.g(0x84)));
    s.set(0x64, (s.g(0xf8) * s.g(0x7c)) + (s.g(0x58) * s.g(0x80)));
    if s.g(0x58) > 0.0 && s.g(0x7c) > 0.0 {
        let inv = s.set(0x10, 1.0 / (s.g(0x58) + s.g(0x7c)));
        if s.g(0xbc) <= EPS && s.g(0x64) <= EPS {
            accept!(2, [r(inv * s.g(0x58)), r(inv * s.g(0x7c))], [(1, 3)]);
        }
        let l = s.set(0x10, inv * s.g(0x7c));
        let a4 = s.set(0xa4, l * w[3][2]);
        let y = s.set(0x2c, (inv * s.g(0x58)) * w[0][1]);
        let z = s.set(0x30, (inv * s.g(0x58)) * w[0][2]);
        let x = s.set(
            0x6c,
            ((inv * s.g(0x58)) * w[0][0]) + ((inv * s.g(0x7c)) * w[3][0]),
        );
        let vy = y + (l * w[3][1]);
        let norm = ((x * x) + ((z + a4) * (z + a4))) + (vy * vy);
        if norm < s.g(0x18) {
            s.set(0x18, norm);
            best_lambda[0] = r(inv * s.g(0x58));
            best_lambda[1] = l;
            best_count = 2;
            swaps = vec![(1, 3)];
        }
    }
    // Edge 12.
    s.set(0x108, xzy(sub(w[1], w[0]), w[2]));
    s.set(0x58, (s.g(0x108) * s.g(0x50)) + (s.g(0x1c) * s.g(0x88)));
    s.set(
        0x10,
        (xzy(sub(w[1], w[3]), w[2]) * s.g(0x50)) + (s.g(0x1c) * s.g(0x4c)),
    );
    if s.g(0x1c) > 0.0 && s.g(0x50) > 0.0 {
        let inv = s.set(0x5c, 1.0 / (s.g(0x1c) + s.g(0x50)));
        if s.g(0x58) <= EPS && s.g(0x10) <= EPS {
            accept!(2, [r(inv * s.g(0x50)), r(inv * s.g(0x1c))], [(0, 2)]);
        }
        let a4 = s.set(0xa4, (inv * s.g(0x50)) * w[2][2]);
        let y = s.set(0x2c, (inv * s.g(0x1c)) * w[1][1]);
        let z = s.set(0x30, (inv * s.g(0x1c)) * w[1][2]);
        let x = s.set(
            0x6c,
            ((inv * s.g(0x1c)) * w[1][0]) + ((inv * s.g(0x50)) * w[2][0]),
        );
        let vy = y + ((inv * s.g(0x50)) * w[2][1]);
        let norm = ((x * x) + ((z + a4) * (z + a4))) + (vy * vy);
        if norm < s.g(0x18) {
            s.set(0x18, norm);
            best_lambda[0] = r(inv * s.g(0x50));
            best_lambda[1] = r(inv * s.g(0x1c));
            best_count = 2;
            swaps = vec![(2, 0)];
        }
    }
    // Edge 13.
    s.set(0xfc, xzy(sub(w[1], w[0]), w[3]));
    s.set(0x5c, (s.g(0xfc) * s.g(0x4c)) + (s.g(0x38) * s.g(0x88)));
    s.set(
        0x50,
        (xzy(sub(w[1], w[2]), w[3]) * s.g(0x4c)) + (s.g(0x38) * s.g(0x50)),
    );
    if s.g(0x38) > 0.0 && s.g(0x4c) > 0.0 {
        let inv = s.set(0x14, 1.0 / (s.g(0x38) + s.g(0x4c)));
        if s.g(0x5c) <= EPS && s.g(0x50) <= EPS {
            accept!(2, [r(inv * s.g(0x4c)), r(inv * s.g(0x38))], [(0, 3)]);
        }
        let a4 = s.set(0xa4, (inv * s.g(0x4c)) * w[3][2]);
        let y = s.set(0x2c, (inv * s.g(0x38)) * w[1][1]);
        let z = s.set(0x30, (inv * s.g(0x38)) * w[1][2]);
        let x = s.set(
            0x6c,
            ((inv * s.g(0x38)) * w[1][0]) + ((inv * s.g(0x4c)) * w[3][0]),
        );
        let vy = y + ((inv * s.g(0x4c)) * w[3][1]);
        let norm = ((x * x) + ((z + a4) * (z + a4))) + (vy * vy);
        if norm < s.g(0x18) {
            s.set(0x18, norm);
            best_lambda[0] = r(inv * s.g(0x4c));
            best_lambda[1] = r(inv * s.g(0x38));
            best_count = 2;
            swaps = vec![(3, 0)];
        }
    }
    // Edge 23.
    let ac = s.set(0xac, w[2][0] - w[1][0]);
    let b0 = s.set(0xb0, w[2][1] - w[1][1]);
    let b4 = s.set(0xb4, w[2][2] - w[1][2]);
    s.set(
        0x38,
        (xzy(sub(w[2], w[0]), w[3]) * s.g(0x3c)) + (s.g(0x54) * s.g(0x48)),
    );
    s.set(
        0x1c,
        ((((ac * w[3][0]) + (b4 * w[3][2])) + (b0 * w[3][1])) * s.g(0x3c))
            + (s.g(0x54) * s.g(0x1c)),
    );
    if s.g(0x54) > 0.0 && s.g(0x3c) > 0.0 {
        let inv = s.set(0x14, 1.0 / (s.g(0x54) + s.g(0x3c)));
        if s.g(0x38) <= EPS && s.g(0x1c) <= EPS {
            accept!(
                2,
                [r(inv * s.g(0x54)), r(inv * s.g(0x3c))],
                [(1, 3), (0, 2)]
            );
        }
        let l = s.set(0x14, inv * s.g(0x3c));
        let a4 = s.set(0xa4, l * w[3][2]);
        let y = s.set(0x2c, (inv * s.g(0x54)) * w[2][1]);
        let z = s.set(0x30, (inv * s.g(0x54)) * w[2][2]);
        let x = s.set(
            0x6c,
            ((inv * s.g(0x54)) * w[2][0]) + ((inv * s.g(0x3c)) * w[3][0]),
        );
        let vy = y + (l * w[3][1]);
        let norm = ((x * x) + ((z + a4) * (z + a4))) + (vy * vy);
        if norm < s.g(0x18) {
            s.set(0x18, norm);
            best_lambda[0] = r(inv * s.g(0x54));
            best_lambda[1] = l;
            best_count = 2;
            swaps = vec![(2, 0), (1, 3)];
        }
    }
    // Face 012.
    if s.g(0x58) > 0.0 && s.g(0xc0) > 0.0 && s.g(0x44) > 0.0 {
        let inv = s.set(0x3c, 1.0 / ((s.g(0x58) + s.g(0xc0)) + s.g(0x44)));
        if (((s.g(0xec) * s.g(0x44)) + (s.g(0xc0) * s.g(0xf0))) + (s.g(0x58) * s.g(0x7c))) <= EPS {
            accept!(
                3,
                [r(inv * s.g(0x58)), r(inv * s.g(0xc0)), r(inv * s.g(0x44))],
                []
            );
        }
        let m14 = s.set(0x14, inv * s.g(0xc0));
        let c2 = inv * s.g(0x44);
        let m44n = s.set(0x44, c2);
        let e4 = s.set(0xe4, m44n * w[2][2]);
        let m74 = s.set(0x74, m14 * w[1][2]);
        let x2c = s.set(0x2c, (inv * s.g(0x58)) * w[0][1]);
        let x30 = s.set(0x30, (inv * s.g(0x58)) * w[0][2]);
        let m9c = s.set(0x9c, ((inv * s.g(0x58)) * w[0][0]) + (m14 * w[1][0]));
        let a4 = s.set(0xa4, x30 + m74);
        let m6c = s.set(0x6c, m9c + (c2 * w[2][0]));
        let vy = (x2c + (m14 * w[1][1])) + (m44n * w[2][1]);
        let vz = a4 + e4;
        let norm = ((m6c * m6c) + (vz * vz)) + (vy * vy);
        if norm < s.g(0x18) {
            s.set(0x18, norm);
            best_lambda = [r(inv * s.g(0x58)), m14, m44n, 0.0];
            best_count = 3;
            swaps = vec![];
        }
    }
    // Face 013.
    if s.g(0x5c) > 0.0 && s.g(0xbc) > 0.0 && s.g(0x40) > 0.0 {
        let inv = s.set(0x44, 1.0 / ((s.g(0x5c) + s.g(0xbc)) + s.g(0x40)));
        if (((s.g(0x5c) * s.g(0x80)) + (s.g(0xf8) * s.g(0x40))) + (s.g(0xbc) * s.g(0x100))) <= EPS {
            accept!(
                3,
                [r(inv * s.g(0x5c)), r(inv * s.g(0xbc)), r(inv * s.g(0x40))],
                [(2, 3)]
            );
        }
        let m14 = s.set(0x14, inv * s.g(0xbc));
        let c3 = inv * s.g(0x40);
        let m40n = s.set(0x40, c3);
        let e4 = s.set(0xe4, m40n * w[3][2]);
        let m74 = s.set(0x74, m14 * w[1][2]);
        let x2c = s.set(0x2c, (inv * s.g(0x5c)) * w[0][1]);
        let x30 = s.set(0x30, (inv * s.g(0x5c)) * w[0][2]);
        let m9c = s.set(0x9c, ((inv * s.g(0x5c)) * w[0][0]) + (m14 * w[1][0]));
        let a4 = s.set(0xa4, x30 + m74);
        let m6c = s.set(0x6c, m9c + (c3 * w[3][0]));
        let vy = (x2c + (m14 * w[1][1])) + (m40n * w[3][1]);
        let vz = a4 + e4;
        let norm = ((m6c * m6c) + (vz * vz)) + (vy * vy);
        if norm < s.g(0x18) {
            s.set(0x18, norm);
            best_lambda = [r(inv * s.g(0x5c)), m14, m40n, 0.0];
            best_count = 3;
            swaps = vec![(2, 3)];
        }
    }
    // Face 023.
    if s.g(0x38) > 0.0 && s.g(0x64) > 0.0 && s.g(0x20) > 0.0 {
        let inv = s.set(0x40, 1.0 / ((s.g(0x38) + s.g(0x64)) + s.g(0x20)));
        if (((s.g(0x104) * s.g(0x20)) + (s.g(0x64) * s.g(0xf4))) + (s.g(0x38) * s.g(0x84))) <= EPS {
            accept!(
                3,
                [r(inv * s.g(0x38)), r(inv * s.g(0x20)), r(inv * s.g(0x64))],
                [(1, 3)]
            );
        }
        let m14 = s.set(0x14, inv * s.g(0x64));
        let c1 = inv * s.g(0x20);
        let m20n = s.set(0x20, c1);
        let e4 = s.set(0xe4, m20n * w[3][2]);
        let m74 = s.set(0x74, m14 * w[2][2]);
        let x2c = s.set(0x2c, (inv * s.g(0x38)) * w[0][1]);
        let x30 = s.set(0x30, (inv * s.g(0x38)) * w[0][2]);
        let m9c = s.set(0x9c, ((inv * s.g(0x38)) * w[0][0]) + (m14 * w[2][0]));
        let a4 = s.set(0xa4, x30 + m74);
        let m6c = s.set(0x6c, m9c + (c1 * w[3][0]));
        let vy = (x2c + (m14 * w[2][1])) + (m20n * w[3][1]);
        let vz = a4 + e4;
        let norm = ((vz * vz) + (vy * vy)) + (m6c * m6c);
        if norm < s.g(0x18) {
            s.set(0x18, norm);
            best_lambda = [r(inv * s.g(0x38)), m20n, m14, 0.0];
            best_count = 3;
            swaps = vec![(1, 3)];
        }
    }
    // Face 123.
    if s.g(0x1c) > 0.0 && s.g(0x50) > 0.0 && s.g(0x10) > 0.0 {
        let inv = s.set(0x20, 1.0 / ((s.g(0x1c) + s.g(0x50)) + s.g(0x10)));
        if (((s.g(0xfc) * s.g(0x10)) + (s.g(0x50) * s.g(0x108))) + (s.g(0x1c) * s.g(0x88))) <= EPS {
            accept!(
                3,
                [r(inv * s.g(0x10)), r(inv * s.g(0x1c)), r(inv * s.g(0x50))],
                [(0, 3)]
            );
        }
        let m14 = s.set(0x14, inv * s.g(0x50));
        let c0 = inv * s.g(0x10);
        let m10n = s.set(0x10, c0);
        let e4 = s.set(0xe4, m10n * w[3][2]);
        let m74 = s.set(0x74, m14 * w[2][2]);
        let x2c = s.set(0x2c, (inv * s.g(0x1c)) * w[1][1]);
        let x30 = s.set(0x30, (inv * s.g(0x1c)) * w[1][2]);
        let m9c = s.set(0x9c, ((inv * s.g(0x1c)) * w[1][0]) + (m14 * w[2][0]));
        let a4 = s.set(0xa4, x30 + m74);
        let m6c = s.set(0x6c, m9c + (c0 * w[3][0]));
        let vy = (x2c + (m14 * w[2][1])) + (m10n * w[3][1]);
        let vz = a4 + e4;
        let norm = ((vz * vz) + (vy * vy)) + (m6c * m6c);
        if norm < s.g(0x18) {
            s.set(0x18, norm);
            best_lambda = [m10n, r(inv * s.g(0x1c)), m14, 0.0];
            best_count = 3;
            swaps = vec![(0, 3)];
        }
    }
    // The tetrahedron: the origin is inside when the four face planes put it on the same side. The
    // planes' normals are crossed from differences, some of which the original keeps as 32-bit
    // slots (`dx`, `dy` of the second edge) and some in registers.
    let df = |i: usize, j: usize, k: usize| w[i][k] - w[j][k];
    let (ac1, b01) = (r(df(1, 0, 0)), r(df(1, 0, 1)));
    let n0x = r((b01 * df(2, 0, 2)) - (df(1, 0, 2) * df(2, 0, 1)));
    let n0y = r((df(1, 0, 2) * df(2, 0, 0)) - (df(2, 0, 2) * ac1));
    let n0z = (df(2, 0, 1) * ac1) - (b01 * df(2, 0, 0));
    let (ac2, b02) = (r(df(2, 0, 0)), r(df(2, 0, 1)));
    let n1x = r((b02 * df(3, 0, 2)) - (df(2, 0, 2) * df(3, 0, 1)));
    let n1y = r((df(2, 0, 2) * df(3, 0, 0)) - (df(3, 0, 2) * ac2));
    let n1z = r((df(3, 0, 1) * ac2) - (b02 * df(3, 0, 0)));
    let (ac3, b03) = (r(df(3, 0, 0)), r(df(3, 0, 1)));
    let n2x = r((b03 * df(1, 0, 2)) - (df(3, 0, 2) * df(1, 0, 1)));
    let n2y = r((df(3, 0, 2) * df(1, 0, 0)) - (df(1, 0, 2) * ac3));
    let n2z = r((df(1, 0, 1) * ac3) - (b03 * df(1, 0, 0)));
    let (ac4, b04) = (r(df(3, 1, 0)), r(df(3, 1, 1)));
    let n3x = r((b04 * df(2, 1, 2)) - (df(3, 1, 2) * df(2, 1, 1)));
    let n3y = r((df(3, 1, 2) * df(2, 1, 0)) - (df(2, 1, 2) * ac4));
    let n3z = r((df(2, 1, 1) * ac4) - (b04 * df(2, 1, 0)));
    let side = [
        ((n0z * w[0][2]) + (n0y * w[0][1])) + (n0x * w[0][0]) > 0.0,
        ((n1z * w[0][2]) + (n1y * w[0][1])) + (n1x * w[0][0]) > 0.0,
        ((n2z * w[0][2]) + (n2y * w[0][1])) + (n2x * w[0][0]) > 0.0,
        ((n3z * w[2][2]) + (n3y * w[2][1])) + (n3x * w[2][0]) > 0.0,
    ];
    if side[1] == side[0] && side[2] == side[0] && side[3] == side[0] {
        out.ok = false;
        out.count = 4;
        return;
    }
    out.count = best_count;
    out.lambda = best_lambda;
    for &(a, b) in swaps.iter() {
        swapped(&mut out.order, a, b);
    }
}

// ---------------------------------------------------------------------------
// The query
// ---------------------------------------------------------------------------

/// What the query leaves behind.
#[derive(Clone, Debug)]
pub struct QueryResult {
    /// The simplex is the whole tetrahedron around the origin (the original answers `-1`).
    pub overlap: bool,
    /// The reported distance (`sqrt` of the round before the last one's `|v|^2`, as an `f32`).
    pub distance: f32,
    /// The kept points: indices into the hull and into the triangle.
    pub hull: Vec<usize>,
    pub tri: Vec<usize>,
    /// The point on the hull in its local frame, in the world, and on the triangle.
    pub on_a_local: [f32; 3],
    pub on_a: [f32; 3],
    pub on_b: [f32; 3],
}

pub struct Hull<'a> {
    pub vertices: &'a [[f32; 3]],
    pub adjacency: Option<&'a [Vec<u16>]>,
    /// The rotation as it sits in the collidable: three columns of three.
    pub rotation: [f32; 9],
    pub translation: [f32; 3],
}

fn p32(v: [f32; 3]) -> P {
    [v[0] as f64, v[1] as f64, v[2] as f64]
}

impl Hull<'_> {
    /// `FUN_0051e650`: a local point in the world.
    fn world(&self, v: P) -> P {
        let m: Vec<f64> = self.rotation.iter().map(|&x| x as f64).collect();
        let t = self.translation;
        let x = r(((m[6] * v[2]) + (m[3] * v[1])) + (v[0] * m[0]) + t[0] as f64);
        let y = r(r(((m[7] * v[2]) + (m[4] * v[1])) + (m[1] * v[0])) + t[1] as f64);
        let z = r(r(((m[8] * v[2]) + (m[5] * v[1])) + (m[2] * v[0])) + t[2] as f64);
        [x, y, z]
    }

    /// `FUN_0051e600`: a world direction in the hull's frame (the inverse is the transpose).
    fn local(&self, d: P) -> P {
        let m: Vec<f64> = self.rotation.iter().map(|&x| x as f64).collect();
        [
            r(((d[2] * m[2]) + (d[1] * m[1])) + (d[0] * m[0])),
            r(((d[2] * m[5]) + (d[1] * m[4])) + (d[0] * m[3])),
            r(((d[2] * m[8]) + (d[1] * m[7])) + (d[0] * m[6])),
        ]
    }

    /// `FUN_0055b000` / `FUN_0055b130`: the hull's support vertex along the world direction `d`.
    fn support(&self, d: P, cur: &mut usize) -> usize {
        let dl = self.local(d);
        let dot = |i: usize| {
            let v = p32(self.vertices[i]);
            ((dl[2] * v[2]) + (dl[1] * v[1])) + (dl[0] * v[0])
        };
        match self.adjacency {
            None => {
                let n = self.vertices.len();
                let mut best = r(dot(n - 1));
                let mut at = n - 1;
                for i in (0..n - 1).rev() {
                    let x = dot(i);
                    if x > best {
                        best = r(x);
                        at = i;
                    }
                }
                at
            }
            Some(adj) => {
                let mut best = r(dot(*cur));
                loop {
                    let mut moved = false;
                    let list = &adj[*cur];
                    for &n in list {
                        let x = dot(n as usize);
                        if x > best && (r(x) as f32).to_bits() != (best as f32).to_bits() {
                            moved = true;
                            *cur = n as usize;
                            best = r(x);
                        }
                    }
                    if !moved {
                        return *cur;
                    }
                    best = r(dot(*cur));
                }
            }
        }
    }
}

/// `FUN_0055b200`: the triangle's support vertex along `e`; ties go to the highest index.
fn tri_support(tri: &[P; 3], e: P) -> usize {
    let dot = |v: P| ((v[1] * e[1]) + (v[2] * e[2])) + (e[0] * v[0]);
    let mut best = r(dot(tri[2]));
    let mut at = 2;
    for i in (0..2).rev() {
        let x = dot(tri[i]);
        if x > best {
            best = r(x);
            at = i;
        }
    }
    at
}

/// `FUN_00555bdc0`: the triangle's centroid.
fn centroid(t: &[P; 3]) -> P {
    let k = 0.3333333432674408f64;
    let cx = r(r((t[1][0] + t[0][0]) + t[2][0]) * k);
    let cy = r(((t[1][1] + t[0][1]) + t[2][1]) * k);
    let cz = r(r(r(t[1][2] + t[0][2]) + t[2][2]) * k);
    [cx, cy, cz]
}

/// The query of hull `hull` (climb start `cur`, left at the vertex the search ends on) against the
/// triangle `tri` (world vertices).
pub fn query(hull: &Hull, tri: [[f32; 3]; 3], cur: &mut usize) -> QueryResult {
    let t = [p32(tri[0]), p32(tri[1]), p32(tri[2])];
    let c = centroid(&t);
    let pos = p32(hull.translation);
    let u = [r(pos[0] - c[0]), r(pos[1] - c[1]), r(pos[2] - c[2])];
    let d = [-u[0], -u[1], -u[2]];
    let mut a = [0usize; 4];
    let mut b = [0usize; 4];
    a[0] = hull.support(d, cur);
    b[0] = tri_support(&t, u);
    let mut count = 1usize;
    let mut w = [[0.0f64; 3]; 4];
    let wpoint = |ia: usize, ib: usize| -> P {
        let aw = hull.world(p32(hull.vertices[ia]));
        [
            r(aw[0] - t[ib][0]),
            r(aw[1] - t[ib][1]),
            r(aw[2] - t[ib][2]),
        ]
    };
    w[0] = wpoint(a[0], b[0]);
    let mut prev = FLT_MAX;
    let eps2 = EPS * EPS;
    let overlap = |a: &[usize; 4], b: &[usize; 4], count: usize| QueryResult {
        overlap: true,
        distance: 0.0,
        hull: a[..count].to_vec(),
        tri: b[..count].to_vec(),
        on_a_local: [0.0; 3],
        on_a: [0.0; 3],
        on_b: [0.0; 3],
    };
    let (old_prev, lam) = loop {
        let red = reduce(&w, count);
        if !red.ok {
            return overlap(&a, &b, count);
        }
        // The reduction swapped the points in place.
        let (na, nb, nw) = (a, b, w);
        for (k, &o) in red.order.iter().enumerate() {
            a[k] = na[o];
            b[k] = nb[o];
            w[k] = nw[o];
        }
        count = red.count;
        let lam = red.lambda;
        // v = sum of lambda w, from the last point to the first.
        let (mut vx, mut vy, mut vz) = (0.0f64, 0.0f64, 0.0f64);
        for i in (0..count).rev() {
            vx += lam[i] * w[i][0];
            vy += r(lam[i] * w[i][1]);
            vz += r(lam[i] * w[i][2]);
        }
        let norm = ((vz * vz) + (vy * vy)) + (vx * vx);
        let old = prev;
        let stored = r(norm);
        prev = stored;
        // No progress: the norm did not fall (or fell to the same `f32`).
        if norm.partial_cmp(&old) != Some(std::cmp::Ordering::Less)
            || (stored as f32).to_bits() == (old as f32).to_bits()
        {
            break (old, lam);
        }
        if eps2 > stored {
            return overlap(&a, &b, count);
        }
        let da = [r(-vx), r(-vy), r(-vz)];
        let eb = [r(vx), r(vy), r(vz)];
        let ia = hull.support(da, cur);
        let ib = tri_support(&t, eb);
        a[count] = ia;
        b[count] = ib;
        w[count] = wpoint(ia, ib);
        count += 1;
    };
    let distance = r(old_prev.sqrt()) as f32;
    // The points on the hull and on the triangle, from the last point to the first.
    let mut oa = [0.0f64; 3];
    let mut ob = [0.0f64; 3];
    for i in (0..count).rev() {
        let l = lam[i];
        let pa = p32(hull.vertices[a[i]]);
        oa[0] = r((l * pa[0]) + oa[0]);
        oa[1] = r(r(l * pa[1]) + oa[1]);
        oa[2] = r(r(l * pa[2]) + oa[2]);
        let pb = t[b[i]];
        ob[0] = r((l * pb[0]) + ob[0]);
        ob[1] = r(r(l * pb[1]) + ob[1]);
        ob[2] = r(r(l * pb[2]) + ob[2]);
    }
    let wa = hull.world(oa);
    QueryResult {
        overlap: false,
        distance,
        hull: a[..count].to_vec(),
        tri: b[..count].to_vec(),
        on_a_local: [oa[0] as f32, oa[1] as f32, oa[2] as f32],
        on_a: [wa[0] as f32, wa[1] as f32, wa[2] as f32],
        on_b: [ob[0] as f32, ob[1] as f32, ob[2] as f32],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reduction against the original's own answers (its code run natively under the control
    /// word `0x27f`): 105 simplexes of two to four points, among them flat ones with repeated points
    /// and the four-point cases whose tests the rounding decides, and 12 tetrahedra around the
    /// origin. Every kept point, its weight (bit for bit) and the order the points are left in.
    #[test]
    fn the_reduction_is_the_originals_bit_for_bit() {
        let mut n_cases = 0;
        for line in include_str!("x87gjk_reduce.tsv").lines() {
            let t: Vec<&str> = line.split('\t').collect();
            let n: usize = t[0].parse().unwrap();
            let w: Vec<P> = (0..n)
                .map(|i| [1, 2, 3].map(|k| t[3 * i + k].parse::<f32>().unwrap() as f64))
                .collect();
            let e = &t[2 + 3 * n..];
            let ok: i32 = e[0].parse().unwrap();
            let count: usize = e[1].parse().unwrap();
            let order: Vec<usize> = e[2..6].iter().map(|s| s.parse().unwrap()).collect();
            let lam: Vec<f32> = e[10..14].iter().map(|s| s.parse().unwrap()).collect();
            let got = reduce(&w, n);
            n_cases += 1;
            assert_eq!(got.ok, ok != 0, "{line}");
            assert_eq!(got.count, count, "{line}");
            if got.ok {
                assert_eq!(&got.order[..count], &order[..count], "{line}");
                for (g, l) in got.lambda.iter().zip(&lam).take(count) {
                    assert_eq!((*g as f32).to_bits(), l.to_bits(), "{line}");
                }
            }
        }
        assert_eq!(n_cases, 105);
    }

    /// The whole query against the original's own answers for 60 of the live queries of
    /// `gjkquery-sentinel-wideload-compressor__live__000` (the 8, 10 and 12 vertex hulls of the
    /// Compressor's robots, with their adjacency lists and climb starts): simplex, distance, point on
    /// the hull (bits) and the vertex the climb ends on; six of them overlaps.
    #[test]
    fn the_query_is_the_originals_bit_for_bit() {
        type Shape = (Vec<[f32; 3]>, Option<Vec<Vec<u16>>>);
        let mut shapes: std::collections::HashMap<usize, Shape> = std::collections::HashMap::new();
        let mut n_cases = 0;
        let mut overlaps = 0;
        for line in include_str!("x87gjk_query.tsv").lines() {
            let t: Vec<&str> = line.split('\t').collect();
            if t[0] == "S" {
                let n: usize = t[2].parse().unwrap();
                let v: Vec<[f32; 3]> = (0..n)
                    .map(|i| [3, 4, 5].map(|k| t[3 * i + k].parse().unwrap()))
                    .collect();
                let rest = 3 + 3 * n;
                let adj = (t[rest] == "adj").then(|| {
                    let lens: Vec<usize> =
                        (0..n).map(|i| t[rest + 1 + i].parse().unwrap()).collect();
                    let mut k = rest + 1 + n;
                    lens.iter()
                        .map(|&l| {
                            let list = (0..l).map(|j| t[k + j].parse().unwrap()).collect();
                            k += l;
                            list
                        })
                        .collect()
                });
                shapes.insert(t[1].parse().unwrap(), (v, adj));
                continue;
            }
            let si: usize = t[1].parse().unwrap();
            let start: usize = t[2].parse().unwrap();
            let f = |i: usize| -> f32 { t[3 + i].parse().unwrap() };
            let rotation: [f32; 9] = std::array::from_fn(f);
            let tri: [[f32; 3]; 3] =
                std::array::from_fn(|k| [f(12 + 3 * k), f(13 + 3 * k), f(14 + 3 * k)]);
            let e = &t[25..];
            let ret: f64 = e[0].parse().unwrap();
            let count: usize = e[1].parse().unwrap();
            let mut k = 2;
            let mut ia = vec![];
            while e[k] != "/" {
                ia.push(e[k].parse::<usize>().unwrap());
                k += 1;
            }
            k += 1;
            let mut ib = vec![];
            while e[k] != "/" {
                ib.push(e[k].parse::<usize>().unwrap());
                k += 1;
            }
            k += 1;
            let on_a: Vec<f32> = (0..3).map(|j| e[k + j].parse().unwrap()).collect();
            let dist: f32 = e[k + 3].parse().unwrap();
            let after: usize = e[k + 4].parse().unwrap();
            let (verts, adj) = &shapes[&si];
            let hull = Hull {
                vertices: verts,
                adjacency: adj.as_deref(),
                rotation,
                translation: [f(9), f(10), f(11)],
            };
            let mut cur = start;
            let got = query(&hull, tri, &mut cur);
            n_cases += 1;
            assert_eq!(got.overlap, ret < 0.0, "{line}");
            if got.overlap {
                overlaps += 1;
                continue;
            }
            assert_eq!(got.hull.len(), count, "{line}");
            assert_eq!(got.hull, ia, "{line}");
            assert_eq!(got.tri, ib, "{line}");
            assert_eq!(got.distance.to_bits(), dist.to_bits(), "{line}");
            for (g, e) in got.on_a_local.iter().zip(&on_a) {
                assert_eq!(g.to_bits(), e.to_bits(), "{line}");
            }
            assert_eq!(cur, after, "{line}");
        }
        assert_eq!((n_cases, overlaps), (60, 6));
    }

    /// A box resting on the floor under a triangle: the plain case, in the engine's terms (no
    /// adjacency lists, the highest index wins ties).
    #[test]
    fn a_box_over_a_triangle_reports_the_gap_and_the_nearest_face() {
        let v: Vec<[f32; 3]> = (0..8)
            .map(|i| {
                [
                    if i & 1 == 0 { -0.5 } else { 0.5 },
                    if i & 2 == 0 { -0.5 } else { 0.5 },
                    if i & 4 == 0 { -0.5 } else { 0.5 },
                ]
            })
            .collect();
        let hull = Hull {
            vertices: &v,
            adjacency: None,
            rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            translation: [0.0, 1.0, 0.0],
        };
        let tri = [[-5.0, 0.0, -5.0], [5.0, 0.0, -5.0], [0.0, 0.0, 5.0]];
        let q = query(&hull, tri, &mut 0);
        assert!(!q.overlap);
        assert!((q.distance - 0.5).abs() < 1e-6, "{}", q.distance);
        // The nearest corners are the four of the bottom face (y = 0.5 above the floor).
        assert!(q.hull.iter().all(|&i| v[i][1] < 0.0), "{:?}", q.hull);
        assert!((q.on_a[1] - 0.5).abs() < 1e-6 && q.on_b[1].abs() < 1e-6);
    }
}
