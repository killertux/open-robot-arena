//! The order Havok 1.x's MOPP returns a fixed mesh's triangles in (docs/29
//! section 12v).
//!
//! A fixed mesh (an arena's floor and walls) is compiled at load into a MOPP
//! (`FUN_00536a10` → `FUN_00565fd0`): a binary tree over its triangles, one
//! triangle per leaf, flattened into byte code. A query walks the code depth
//! first, the lower side of every split first (`FUN_005661a0`/`FUN_00566ad0`),
//! so whatever box is asked for, the triangles come back in the order of the
//! tree's leaves. That order is all this module computes; the code itself (its
//! quantised bounds) is not built.
//!
//! The tree (recovered from `FUN_0056a150` and its callees, checked against
//! every tree the original built for the eight stock arenas of the Arena list,
//! `mopp-...__live__000`, leaf for leaf). The game runs the compiler with the
//! x87 unit in single precision, so every step here is `f32` arithmetic.
//!
//! * The splitting directions are the axes and ten diagonals, each with a cost
//!   ([`DIRECTIONS`]; the last two are the same direction, as in the binary).
//! * A node holding one triangle is a leaf. Otherwise every direction is tried
//!   in turn: the three axes first, the longest of the parent's extents first
//!   (an extent the parent was split along counts 0.66 of its length), the
//!   other two by length, then the diagonals in table order. An axis is
//!   skipped (with the rest of the axes) once the best cost so far is below
//!   its own cost plus `0.8 (1 − len/longest)³`, a diagonal (with the rest)
//!   once it is below the diagonal's cost.
//! * A direction is tried by projecting the triangles on it (each one's
//!   `[min, max]` of its corners), sorting them by `min` (Havok's quicksort:
//!   middle pivot, not stable) and sweeping them in that order. Before
//!   triangle `i` is considered, the "open" triangles (those before it, kept
//!   by decreasing `max`, at most 51) whose `max` lies below its `min` are
//!   closed. Triangle `i`'s `min` is a candidate for the upper child's lower
//!   bound; the lower child's upper bound is the `max` of open triangle `j`
//!   (`j` open triangles would straddle the split) or, with none open, of the
//!   last closed one. `j` is tried in a window of five around the last best.
//!   The cost of a candidate is the direction's cost (less 0.03 when the
//!   triangle numbers seen so far span less than 16), penalties for an
//!   off-centre split (both sides, `2.9 x² + 3 x⁴ + 1500 x⁸`), for unbalanced
//!   counts, for straddlers (`5 j / n`) and for the overlap of the two sides
//!   (relative to the node's extent); the cheapest candidate of all wins
//!   (strictly cheaper only, so the first of equals).
//! * The triangles are then dealt out by the winner: below the upper child's
//!   bound and within the lower child's go lower, the reverse upper, those
//!   that fit both are sorted along the node's longest axis (not the split's)
//!   and handed out to keep the two counts in a 4:1 balance; the way the
//!   original moves them in its buffer decides the order the children start
//!   from (and so, through the unstable sort, which of two equal triangles
//!   comes first), and is followed exactly. A triangle that fits neither side
//!   would be cut in two by the original; no stock arena's tree has one, and
//!   here it goes to the upper side.
//! * The upper child is built before the lower one; the walk visits the lower
//!   one first.
use glam::Vec3;

/// The splitting directions (unnormalised) and their costs, `0x71cef8`.
const DIRECTIONS: [([f32; 3], f32); 13] = [
    ([1.0, 0.0, 0.0], 0.0),
    ([0.0, 1.0, 0.0], 0.0),
    ([0.0, 0.0, 1.0], 0.0),
    ([0.0, 1.0, 1.0], 0.2),
    ([1.0, 0.0, 1.0], 0.2),
    ([1.0, 1.0, 0.0], 0.2),
    ([0.0, 1.0, -1.0], 0.25),
    ([1.0, 0.0, -1.0], 0.25),
    ([1.0, -1.0, 0.0], 0.25),
    ([1.0, 1.0, 1.0], 0.3),
    ([1.0, 1.0, -1.0], 0.32),
    ([1.0, -1.0, -1.0], 0.32),
    ([1.0, -1.0, -1.0], 0.34),
];

/// The compiler's settings as the game sets them (`FUN_00536a10`): at most
/// 50 open triangles are weighed (51 kept), the window around the last best
/// straddle count is 2 either way, room for 1000 cut pieces.
const MAX_OPEN: usize = 50;
const WINDOW: i32 = 2;
const SPARE: usize = 1000;

#[derive(Clone, Copy, Debug)]
struct Entry {
    tri: u32,
    min: f32,
    max: f32,
}

/// A run of the entry buffer: `len` entries from `start`, then `spare` free
/// slots the node may use.
#[derive(Clone, Copy, Debug)]
struct Span {
    start: usize,
    len: usize,
    spare: usize,
}

#[derive(Clone, Copy, Debug)]
struct Choice {
    cost: f32,
    dir: Option<usize>,
    /// The upper child's lower bound and the lower child's upper bound.
    upper_from: f32,
    lower_to: f32,
}

struct Node {
    /// Extents along the three axes.
    extents: [[f32; 2]; 3],
    choice: Choice,
}

/// What a candidate's cost depends on (`FUN_00569840`'s record).
#[derive(Clone, Copy, Default)]
struct Sweep {
    depth: i32,
    min: f32,
    max: f32,
    inv_width: f32,
    count: i32,
    inv_count: f32,
    dir_cost: f32,
    i: i32,
    at: f32,
    first_tri: i64,
    last_tri: i64,
    open: i32,
    upper_count: i32,
    bound: f32,
}

struct Compiler {
    tris: Vec<[Vec3; 3]>,
    dirs: [(Vec3, f32); 13],
    /// Bit length of the triangle count: shallower nodes are held to a
    /// centred split.
    bits: i32,
    buf: Vec<Entry>,
    order: Vec<u32>,
}

fn directions() -> [(Vec3, f32); 13] {
    DIRECTIONS.map(|(d, c)| {
        let v = Vec3::from(d);
        let len = ((v.x * v.x + v.y * v.y) + v.z * v.z).sqrt();
        let inv = 1.0 / len;
        (Vec3::new(v.x * inv, v.y * inv, v.z * inv), c)
    })
}

/// Havok's quicksort by `min` (`FUN_0056a8a0`): middle pivot, Hoare scan,
/// the lower part recursed into, the upper one looped on.
fn quicksort(a: &mut [Entry], mut lo: usize, hi: usize) {
    loop {
        let pivot = a[(lo + hi) >> 1].min;
        let (mut i, mut j) = (lo as isize, hi as isize);
        loop {
            while a[i as usize].min < pivot {
                i += 1;
            }
            while pivot < a[j as usize].min {
                j -= 1;
            }
            if j < i {
                break;
            }
            if j != i {
                a.swap(i as usize, j as usize);
            }
            j -= 1;
            i += 1;
            if i > j {
                break;
            }
        }
        if (lo as isize) < j {
            quicksort(a, lo, j as usize);
        }
        if i >= hi as isize {
            return;
        }
        lo = i as usize;
    }
}

/// A corner's projection, `(x nx + y ny) + z nz` (`FUN_00565680`).
fn project(p: Vec3, n: Vec3) -> f32 {
    (p.x * n.x + p.y * n.y) + p.z * n.z
}

/// The penalty for a split off the node's centre, `x` the offset.
fn off_centre(x: f32) -> f32 {
    let x2 = x * x;
    let x4 = x2 * x2;
    let x8 = x4 * x4;
    (x8 * 1500.0 + x4 * 3.0) + x2 * 2.9
}

impl Compiler {
    fn bounds(&mut self, dir: usize, span: Span, write: bool) -> [f32; 2] {
        let n = self.dirs[dir].0;
        let (mut lo, mut hi) = (1.0e7f32, -1.0e7f32);
        for e in &mut self.buf[span.start..span.start + span.len] {
            let t = self.tris[e.tri as usize];
            let d = [project(t[0], n), project(t[1], n), project(t[2], n)];
            let (mut mn, mut mx) = if d[0] > d[1] {
                (d[1], d[0])
            } else {
                (d[0], d[1])
            };
            if d[2] > mx {
                mx = d[2];
            }
            if d[2] < mn {
                mn = d[2];
            }
            if write {
                e.min = mn;
                e.max = mx;
            }
            lo = lo.min(mn);
            hi = hi.max(mx);
        }
        [lo, hi]
    }

    fn sort_span(&mut self, base: usize, len: usize) {
        if len > 1 {
            quicksort(&mut self.buf[base..base + len], 0, len - 1);
        }
    }

    /// The direction's own share of a candidate's cost (`FUN_0056aa40`):
    /// its table cost, the bonus for a narrow run of triangle numbers, and
    /// the penalty for a split left of centre seen from the upper side.
    fn direction_cost(&self, s: &Sweep) -> f32 {
        let below = s.at - s.min;
        let slack = if s.depth < self.bits {
            let w = s.max - s.min;
            if 0.5 >= w {
                w
            } else {
                0.5
            }
        } else {
            0.0
        };
        let x = (below + slack) * s.inv_width - 0.5;
        let skew = if x >= 0.0 { off_centre(x) } else { 0.0 };
        let mut c = s.dir_cost;
        if s.last_tri - s.first_tri < 16 {
            c += -0.03;
        }
        c + skew
    }

    /// The candidate's own cost (`FUN_0056ab10`).
    fn split_cost(&self, s: &Sweep) -> f32 {
        let below = s.at - s.min;
        let shallow = s.depth < self.bits;
        let slack = if shallow {
            let w = s.max - s.min;
            if 0.5 >= w {
                w
            } else {
                0.5
            }
        } else {
            0.0
        };
        let x = (below - slack) * s.inv_width - 0.5;
        let skew = if x <= 0.0 { off_centre(x) } else { 0.0 };
        let b = (s.i - s.upper_count).abs() as f32 * s.inv_count;
        let balance = if shallow {
            let b = (b - 0.75) * 6.0;
            if b >= 0.0 {
                ((b * b) * b) * b
            } else {
                0.0
            }
        } else {
            let b = b * b;
            ((b * b) * b) * b
        };
        let straddle = (s.open as f32 * s.inv_count) * 5.0;
        let overlap = (s.bound - s.at) * s.inv_width;
        skew + ((overlap + straddle) + balance)
    }

    /// One direction swept (`FUN_00569840` with `FUN_00569670`).
    fn sweep(&mut self, node: &mut Node, dir: usize, span: Span, ext: [f32; 2], depth: i32) {
        let width = ext[1] - ext[0];
        let mut s = Sweep {
            depth,
            min: ext[0],
            max: ext[1],
            inv_width: if width >= 0.001 { 1.0 / width } else { 1.0 },
            count: span.len as i32,
            inv_count: 1.0 / span.len as f32,
            dir_cost: self.dirs[dir].1,
            first_tri: i32::MAX as i64,
            last_tri: (i32::MIN + 1) as i64,
            ..Default::default()
        };
        let limit = span.spare.min(MAX_OPEN);
        // Open triangles by decreasing max (indices into the buffer), and the
        // last one closed.
        let mut open: Vec<usize> = Vec::new();
        let mut closed: Option<usize> = None;
        let mut best_open: i32 = 0;
        for i in 0..span.len {
            let at = span.start + i;
            let e = self.buf[at];
            s.i = i as i32;
            s.at = e.min;
            s.first_tri = s.first_tri.min(e.tri as i64);
            s.last_tri = s.last_tri.max(e.tri as i64);
            while let Some(&k) = open.last() {
                if self.buf[k].max >= e.min {
                    break;
                }
                closed = open.pop();
            }
            while open.len() > limit + 1 {
                closed = open.pop();
            }
            self.weigh(node, dir, &mut s, &open, closed, &mut best_open);
            let at_max = e.max;
            let place = open
                .iter()
                .rposition(|&k| self.buf[k].max >= at_max)
                .map_or(0, |p| p + 1);
            open.insert(place, at);
        }
    }

    /// The candidates of triangle `s.i` (`FUN_00569670`).
    fn weigh(
        &self,
        node: &mut Node,
        dir: usize,
        s: &mut Sweep,
        open: &[usize],
        closed: Option<usize>,
        best_open: &mut i32,
    ) {
        let dcost = self.direction_cost(s);
        let mut local = 1.0e7f32;
        if open.is_empty() {
            if let Some(k) = closed {
                let bound = self.buf[k].max;
                self.candidate(node, dir, s, 0, bound, dcost, &mut local, best_open);
                *best_open -= 1;
            }
            return;
        }
        if dcost > node.choice.cost {
            return;
        }
        let hi = (*best_open + WINDOW).min(open.len() as i32);
        let mut lo = (*best_open - WINDOW).max(0);
        let mut j = hi;
        loop {
            j -= 1;
            if j < lo {
                *best_open -= 1;
                return;
            }
            let bound = self.buf[open[j as usize]].max;
            if self.candidate(node, dir, s, j, bound, dcost, &mut local, best_open)
                && j - WINDOW < lo
            {
                lo = (j - WINDOW).max(0);
            }
        }
    }

    /// One candidate: `j` open triangles straddling, the lower child up to
    /// `bound`. True when it became the node's best.
    #[allow(clippy::too_many_arguments)]
    fn candidate(
        &self,
        node: &mut Node,
        dir: usize,
        s: &mut Sweep,
        j: i32,
        bound: f32,
        dcost: f32,
        local: &mut f32,
        best_open: &mut i32,
    ) -> bool {
        s.open = j;
        s.upper_count = s.count - s.i + j;
        s.bound = bound;
        let c = self.split_cost(s) + dcost;
        if c < *local {
            *local = c;
            *best_open = j;
            if c < node.choice.cost {
                node.choice = Choice {
                    cost: c,
                    dir: Some(dir),
                    upper_from: s.at,
                    lower_to: bound,
                };
                return true;
            }
        }
        false
    }

    /// The order the three axes are tried in, and the cost each must leave
    /// room for (`FUN_00569fe0`).
    fn axis_order(parent: Option<&Node>) -> ([usize; 3], [f32; 3]) {
        let Some(p) = parent else {
            return ([0, 1, 2], [0.0; 3]);
        };
        let mut w = [0.0f32; 3];
        for (k, wk) in w.iter_mut().enumerate() {
            *wk = p.extents[k][1] - p.extents[k][0];
            if p.choice.dir == Some(k) {
                *wk *= 0.66;
            }
        }
        let (mut best, mut a) = (0.0f32, 0);
        for (k, &wk) in w.iter().enumerate() {
            if best < wk {
                best = wk;
                a = k;
            }
        }
        let (b, c) = ((a + 1) % 3, (a + 2) % 3);
        let inv = 1.0 / best;
        let room = |x: f32| {
            let y = (best - x) * inv;
            (((y * y) * y) * 16.0) * 0.05
        };
        if w[b] >= w[c] {
            ([a, b, c], [0.0, room(w[b]), room(w[c])])
        } else {
            ([a, c, b], [0.0, room(w[c]), room(w[b])])
        }
    }

    fn build(&mut self, parent: Option<&Node>, span: Span, depth: i32) -> Tree {
        if span.len == 1 {
            return Tree::Leaf(self.buf[span.start].tri);
        }
        let mut node = Node {
            extents: parent.map_or([[0.0; 2]; 3], |p| p.extents),
            choice: Choice {
                cost: 1.0e6,
                dir: None,
                upper_from: 0.0,
                lower_to: 0.0,
            },
        };
        let (order, room) = Self::axis_order(parent);
        let mut projected = None;
        for idx in 0..3 {
            let k = order[idx];
            if room[idx] + self.dirs[k].1 > node.choice.cost {
                for &k in &order[idx..] {
                    node.extents[k] = self.bounds(k, span, false);
                }
                break;
            }
            node.extents[k] = self.bounds(k, span, true);
            self.sort_span(span.start, span.len);
            let ext = node.extents[k];
            self.sweep(&mut node, k, span, ext, depth);
            projected = Some(k);
        }
        for k in 3..DIRECTIONS.len() {
            if self.dirs[k].1 > node.choice.cost {
                break;
            }
            let ext = self.bounds(k, span, true);
            self.sort_span(span.start, span.len);
            self.sweep(&mut node, k, span, ext, depth);
            projected = Some(k);
        }
        let Some(dir) = node.choice.dir else {
            // No candidate at all (every triangle in one point): keep the
            // order as it is.
            return Tree::Inner(
                (span.start..span.start + span.len)
                    .map(|i| Tree::Leaf(self.buf[i].tri))
                    .collect(),
            );
        };
        if projected != Some(dir) {
            self.bounds(dir, span, true);
        }
        let (lower, upper) = self.deal(&node, dir, span);
        let up = self.build(Some(&node), upper, depth + 1);
        let low = self.build(Some(&node), lower, depth + 1);
        Tree::Inner(vec![low, up])
    }

    /// The triangles dealt to the two children (`FUN_005699f0`), moved in the
    /// buffer the way the original moves them.
    fn deal(&mut self, node: &Node, dir: usize, span: Span) -> (Span, Span) {
        let (from, to) = (node.choice.upper_from, node.choice.lower_to);
        let start = span.start;
        let end = start + span.len + span.spare;
        // [start, lower_end) lower; [lower_end, unseen_end) not yet seen;
        // [upper_start, both_start) upper; [both_start, end) either side.
        let mut lower_end = start;
        let mut unseen_end = start + span.len;
        let mut upper_start = end;
        let mut both_start = end;
        let b = &mut self.buf;
        while lower_end < unseen_end {
            let e = b[lower_end];
            let fits_lower = e.max <= to;
            if e.min < from {
                // In the lower child (or across both: see the module's notes).
                if fits_lower {
                    lower_end += 1;
                    continue;
                }
            } else if fits_lower {
                // Either side.
                unseen_end -= 1;
                let next = b[unseen_end];
                both_start -= 1;
                upper_start -= 1;
                b[upper_start] = b[both_start];
                b[both_start] = e;
                if lower_end == unseen_end {
                    break;
                }
                b[lower_end] = next;
                continue;
            }
            // The upper child.
            upper_start -= 1;
            unseen_end -= 1;
            if lower_end == upper_start {
                continue;
            }
            let next = b[unseen_end];
            b[upper_start] = b[lower_end];
            b[lower_end] = next;
        }
        // Those that fit both, along the node's longest axis (other than the
        // split's own), dealt out to keep the counts balanced.
        let (mut longest, mut axis) = (0.0f32, 0);
        for k in 0..3 {
            if k == dir {
                continue;
            }
            let w = node.extents[k][1] - node.extents[k][0];
            if w > longest {
                longest = w;
                axis = k;
            }
        }
        let both = Span {
            start: both_start,
            len: end - both_start,
            spare: 0,
        };
        self.bounds(axis, both, true);
        self.sort_span(both.start, both.len);
        let b = &mut self.buf;
        let mut lower_n = lower_end - start;
        let mut upper_end = both_start;
        let mut last_lower = lower_n < both_start - upper_start;
        let mut cur = both_start;
        while cur < end {
            let (mut l, mut u) = (lower_n, upper_end - upper_start);
            if last_lower {
                u <<= 2;
            } else {
                l <<= 2;
            }
            if l < u {
                let tail = b[end - 1];
                b[end - 1] = b[cur];
                let first_upper = b[upper_start];
                b[lower_end] = tail;
                b[cur] = first_upper;
                lower_end += 1;
                lower_n += 1;
                upper_start += 1;
                upper_end += 1;
                cur += 1;
                last_lower = true;
            } else {
                upper_end += 1;
                cur += 1;
                last_lower = false;
            }
        }
        let lower_len = lower_end - start;
        let lower_spare = lower_len * span.spare / span.len;
        let upper_len = upper_end - upper_start;
        let upper_spare = span.spare - lower_spare;
        let upper_at = lower_end + lower_spare;
        if upper_spare != 0 && upper_start < upper_end {
            for q in 0..upper_len {
                b[upper_at + q] = b[upper_start + q];
            }
        }
        (
            Span {
                start,
                len: lower_len,
                spare: lower_spare,
            },
            Span {
                start: upper_at,
                len: upper_len,
                spare: upper_spare,
            },
        )
    }
}

enum Tree {
    Leaf(u32),
    Inner(Vec<Tree>),
}

fn leaves(t: &Tree, out: &mut Vec<u32>) {
    match t {
        Tree::Leaf(i) => out.push(*i),
        Tree::Inner(c) => c.iter().for_each(|c| leaves(c, out)),
    }
}

/// The triangles of a fixed mesh in the order its MOPP returns them, the
/// mesh given as the original's geometry holds it (its own frame, its
/// triangles in their order).
pub fn leaf_order(vertices: &[Vec3], triangles: &[[u32; 3]]) -> Vec<u32> {
    let tris: Vec<[Vec3; 3]> = triangles
        .iter()
        .map(|t| t.map(|i| vertices[i as usize]))
        .collect();
    let n = tris.len();
    if n == 0 {
        return Vec::new();
    }
    let mut c = Compiler {
        bits: (usize::BITS - n.leading_zeros()) as i32,
        dirs: directions(),
        buf: (0..n as u32)
            .map(|tri| Entry {
                tri,
                min: 0.0,
                max: 0.0,
            })
            .chain(std::iter::repeat_n(
                Entry {
                    tri: u32::MAX,
                    min: 0.0,
                    max: 0.0,
                },
                SPARE,
            ))
            .collect(),
        tris,
        order: Vec::new(),
    };
    let tree = c.build(
        None,
        Span {
            start: 0,
            len: n,
            spare: SPARE,
        },
        0,
    );
    leaves(&tree, &mut c.order);
    c.order
}

/// Per triangle, its place in [`leaf_order`].
pub fn rank(vertices: &[Vec3], triangles: &[[u32; 3]]) -> Vec<u32> {
    let mut rank = vec![0; triangles.len()];
    for (place, t) in leaf_order(vertices, triangles).into_iter().enumerate() {
        rank[t as usize] = place as u32;
    }
    rank
}

/// A triangle list given as corner triples (`[a, b, c, a, b, c, ...]`) as
/// shared vertices (equal corners, by their bits, become one) and triangles.
pub fn shared_vertices(points: &[Vec3]) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let mut index = std::collections::HashMap::new();
    let mut verts = Vec::new();
    let mut tris = Vec::new();
    for t in points.as_chunks::<3>().0 {
        tris.push([0, 1, 2].map(|k| {
            *index
                .entry(t[k].to_array().map(f32::to_bits))
                .or_insert_with(|| {
                    verts.push(t[k]);
                    verts.len() as u32 - 1
                })
        }));
    }
    (verts, tris)
}
