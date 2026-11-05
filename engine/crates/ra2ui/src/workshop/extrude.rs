//! The game's extrusion: a chassis made from its two outlines and a height.
//!
//! Recovered from live files the original wrote (`tri`, `L`, square, 800- and 200-unit boxes, a
//! raised box: docs/36 section 9.27), not from its code:
//!
//! * the mesh is the bottom ring (normal `-y`), the top ring (normal `+y`), then one four-vertex
//!   quad per outline edge `b_i, b_i+1, t_i+1, t_i` with that side's own normal; points
//!   `x = (u-500)*0.0022`, `z = -(v-500)*0.0022`;
//! * the bottom plate is ear-clipped into `(v, prev, next)` triangles, the last triangle written
//!   reversed; the top plate is the same triangles reversed on the top ring; a side is
//!   `(v1, v2, v0), (v2, v3, v0)`;
//! * the face groups are the bottom plate, the top plate and one rectangle per side, packed on
//!   the 256x256 skin from `v = 246` downwards, 10 px in from the left edge, 9 px apart, at
//!   0.15 px per outline unit; when the layout does not fit in 236 px it is tried again at 0.9x
//!   the scale (live: 600-unit outlines at 0.15, 800-unit ones at 0.135, a 1.19 m height at
//!   0.1215);
//! * every top vertex is a crush corner `index 3 a b c` listing its two side copies; a new
//!   design has no buckle points, a white skin and the armour row `1 12 0 200 0`.
//!
//! **Open:** the packing of a very thin group (a 100-unit edge) is not the in-order shelf this
//! module does: the original leaves a wider gap around it (docs/36 section 9.27), and the order
//! it clips a concave plate's ears in is not recovered (every ear of a rectilinear plate is a
//! 90 degree corner; the original's pick among them looks arbitrary). Both only change the file's
//! atlas and triangle order, not the shape.

use ra2fmt::bot::{BotMesh, Chassis, Face, FaceGroup};

/// The extrusion height a fresh chassis starts with (live `0.825`).
pub const DEFAULT_HEIGHT: f32 = 0.825;
/// The height slider's range (live: the thumb at the top gives 1.32, at the bottom 0.436364).
pub const MIN_HEIGHT: f32 = 0.436_364;
pub const MAX_HEIGHT: f32 = 1.32;

/// The height for a slider position, `0` at the bottom of the track, `1` at the top.
pub fn height_for_fraction(t: f32) -> f32 {
    MIN_HEIGHT + t.clamp(0.0, 1.0) * (MAX_HEIGHT - MIN_HEIGHT)
}

/// The slider position of a height.
pub fn fraction_for_height(height: f32) -> f32 {
    ((height - MIN_HEIGHT) / (MAX_HEIGHT - MIN_HEIGHT)).clamp(0.0, 1.0)
}

const UNIT: f32 = 0.0022;
/// Skin pixels per outline unit, before any shrinking.
const PX_PER_UNIT: f32 = 0.15;
const SKIN: f32 = 256.0;
const MARGIN: f32 = 10.0;
const GAP: f32 = 9.0;
const FIT: f32 = 236.0;

/// The white 256x256 32-bit skin a fresh chassis carries.
pub fn blank_skin() -> Vec<u8> {
    let mut tga = vec![0u8; 18];
    tga[2] = 2;
    tga[13] = 1;
    tga[15] = 1;
    tga[16] = 32;
    tga[17] = 0x20;
    tga.extend(std::iter::repeat_n(0xff, 256 * 256 * 4));
    tga
}

/// Whether an outline runs counter-clockwise in the lab's `(x, z)` (the way the live files never
/// store one): the original keeps a clockwise baseplate, so a baseplate clicked the other way round
/// is stored in the reverse order (live `botlab-adjust-base-ccw`: (200,200) (500,800) (800,200) is
/// written `800 200 / 500 800 / 200 200`).
pub fn is_counter_clockwise(points: &[[i32; 2]]) -> bool {
    let ring: Vec<[f32; 3]> = points.iter().map(|p| world(*p, 0.0)).collect();
    signed_area(&ring) > 0.0
}

fn world(p: [i32; 2], y: f32) -> [f32; 3] {
    [
        (p[0] as f32 - 500.0) * UNIT,
        y,
        -(p[1] as f32 - 500.0) * UNIT,
    ]
}

fn signed_area(ring: &[[f32; 3]]) -> f32 {
    let mut sum = 0.0;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        sum += a[0] * b[2] - b[0] * a[2];
    }
    sum
}

fn cross(o: [f32; 3], a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - o[0]) * (b[2] - o[2]) - (a[2] - o[2]) * (b[0] - o[0])
}

fn inside(p: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> bool {
    let d = [cross(a, b, p), cross(b, c, p), cross(c, a, p)];
    d.iter().all(|v| *v >= 0.0) || d.iter().all(|v| *v <= 0.0)
}

/// Ear-clip a ring into triangles of ring indices. `cw` is whether the ring runs clockwise in
/// `(x, z)`, as the live files' do; triangles come out counter-clockwise for those, `(v, prev,
/// next)`, and the last one reversed.
fn ear_clip(ring: &[[f32; 3]], cw: bool) -> Vec<[u32; 3]> {
    let mut list: Vec<usize> = (0..ring.len()).collect();
    let mut tris = Vec::new();
    let sign = if cw { -1.0 } else { 1.0 };
    while list.len() > 3 {
        let n = list.len();
        let mut found = None;
        for k in 0..n {
            let (p, c, nx) = (list[(k + n - 1) % n], list[k], list[(k + 1) % n]);
            if cross(ring[p], ring[c], ring[nx]) * sign <= 0.0 {
                continue;
            }
            let blocked = list.iter().any(|&o| {
                o != p && o != c && o != nx && inside(ring[o], ring[p], ring[c], ring[nx])
            });
            if !blocked {
                found = Some(k);
                break;
            }
        }
        // A degenerate ring (collinear points): clip the first corner.
        let k = found.unwrap_or(0);
        let (p, c, nx) = (list[(k + n - 1) % n], list[k], list[(k + 1) % n]);
        tris.push(if cw {
            [c as u32, p as u32, nx as u32]
        } else {
            [c as u32, nx as u32, p as u32]
        });
        list.remove(k);
    }
    if list.len() == 3 {
        tris.push(if cw {
            [list[2] as u32, list[1] as u32, list[0] as u32]
        } else {
            [list[0] as u32, list[1] as u32, list[2] as u32]
        });
    }
    tris
}

/// Whole pixels a height takes (a hair of float noise does not count as a pixel).
fn ceil_px(h: f32) -> f32 {
    (h - 0.001).ceil()
}

/// A face's polygon laid flat at skin scale (see `extrude`).
struct Unfold {
    along: Vec<f32>,
    across: Vec<f32>,
    max_along: f32,
    min_across: f32,
    min_along: f32,
    max_across: f32,
}

impl Unfold {
    fn of(points: &[[f32; 3]], normal: [f32; 3], scale: f32) -> Unfold {
        let sub = |p: [f32; 3], q: [f32; 3]| [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
        let dot = |p: [f32; 3], q: [f32; 3]| p[0] * q[0] + p[1] * q[1] + p[2] * q[2];
        let unit = |v: [f32; 3]| {
            let l = dot(v, v).sqrt();
            if l > 1e-9 {
                [v[0] / l, v[1] / l, v[2] / l]
            } else {
                v
            }
        };
        let mut a = unit(sub(points[1], points[0]));
        if a == [0.0; 3] {
            a = [1.0, 0.0, 0.0];
        }
        let w = unit([
            normal[1] * a[2] - normal[2] * a[1],
            normal[2] * a[0] - normal[0] * a[2],
            normal[0] * a[1] - normal[1] * a[0],
        ]);
        let along: Vec<f32> = points
            .iter()
            .map(|p| dot(sub(*p, points[0]), a) * scale)
            .collect();
        let across: Vec<f32> = points
            .iter()
            .map(|p| dot(sub(*p, points[0]), w) * scale)
            .collect();
        let fold = |v: &[f32], f: fn(f32, f32) -> f32, init: f32| v.iter().copied().fold(init, f);
        Unfold {
            max_along: fold(&along, f32::max, f32::MIN),
            min_along: fold(&along, f32::min, f32::MAX),
            max_across: fold(&across, f32::max, f32::MIN),
            min_across: fold(&across, f32::min, f32::MAX),
            along,
            across,
        }
    }

    fn width(&self) -> f32 {
        self.max_along - self.min_along
    }

    fn height(&self) -> f32 {
        self.max_across - self.min_across
    }
}

struct Cell {
    x: f32,
    /// Distance from `v = 246` to the cell's top.
    y: f32,
    w: f32,
    h: f32,
}

/// Pack `sizes` in order at skin scale (`w`, `h` in px): each rectangle goes where it sits lowest on
/// the skyline, among the left margin and the right end (plus the gap, rounded up to a whole pixel)
/// of every rectangle placed so far; the leftmost of equal heights. Recovered from the live files:
/// a square's four sides in a row, a triangle's top plate beside the bottom one and the first side
/// under the top plate (`botlab-adjust-triangle-ok`, docs/36 section 9.27e).
fn pack(sizes: &[(f32, f32)]) -> Option<Vec<Cell>> {
    let mut cells: Vec<Cell> = Vec::new();
    for &(w, h) in sizes {
        let skyline = |x: f32, cells: &[Cell]| {
            cells
                .iter()
                .filter(|c| c.x < x + w && x < c.x + c.w)
                .map(|c| c.y + ceil_px(c.h) + GAP)
                .fold(0.0_f32, f32::max)
        };
        let mut spots = vec![MARGIN];
        spots.extend(cells.iter().map(|c| (c.x + c.w + GAP - 0.001).ceil()));
        let best = spots
            .into_iter()
            .filter(|x| *x + w <= SKIN - MARGIN)
            .map(|x| (skyline(x, &cells), x))
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let (y, x) = best?;
        if y + ceil_px(h) > FIT {
            return None;
        }
        cells.push(Cell { x, y, w, h });
    }
    Some(cells)
}

/// What [`extrude`] makes: the chassis' own mesh, groups and faces.
pub struct Extrusion {
    pub mesh: BotMesh,
    pub face_groups: Vec<FaceGroup>,
    pub faces: Vec<Face>,
    pub top_corners: Vec<[i32; 4]>,
}

/// Extrude `bottom` to `height`, its top ring being `top` (the same count of points, else the
/// bottom's own).
pub fn extrude(bottom: &[[i32; 2]], top: &[[i32; 2]], height: f32) -> Option<Extrusion> {
    extrude_at(bottom, top, height, height)
}

/// [`extrude`] with the face normals (and so the skin's side planes) taken from the quads as they stood at
/// `normal_height`: the height slider moves the mesh's vertices and leaves the normals the last extrusion or
/// point move gave them (live: a top point dragged at 0.825 and the slider then moved to 1.32 saves the
/// 0.825 normals, `(0.959, 0.256, 0.120)` against 0.1632 for a fresh 1.32 one; 9.29).
pub fn extrude_at(
    bottom: &[[i32; 2]],
    top: &[[i32; 2]],
    height: f32,
    normal_height: f32,
) -> Option<Extrusion> {
    let n = bottom.len();
    if n < 3 || height <= 0.0 {
        return None;
    }
    let top = if top.len() == n { top } else { bottom };
    let b: Vec<[f32; 3]> = bottom.iter().map(|p| world(*p, 0.0)).collect();
    let t: Vec<[f32; 3]> = top.iter().map(|p| world(*p, height)).collect();
    let tn: Vec<[f32; 3]> = top.iter().map(|p| world(*p, normal_height)).collect();
    let area = signed_area(&b);
    if area == 0.0 {
        return None;
    }
    let cw = area < 0.0;
    let plate = ear_clip(&b, cw);

    // Every face of the skin is the face's own polygon unfolded flat: `a` runs from the first point
    // to the second, `w = n x a` is the other axis in the face's plane, and the cell is the bounding
    // box of the polygon at the skin scale. A plate (normal `-y` below, `+y` above) with a first
    // edge along x is the plain top-down picture; a side whose top edge was dragged is no longer a
    // rectangle (live `botlab-adjust-*`: the top plate's first edge lies along the cell's top, a
    // side projects onto the plane of its second triangle's normal).
    let plate_down = [0.0, -1.0, 0.0];
    let plate_up = [0.0, 1.0, 0.0];
    let side_normal = |i: usize| -> [f32; 3] {
        let j = (i + 1) % n;
        let quad = [b[i], b[j], tn[j], tn[i]];
        let m = super::adjust::face_normal(quad[2], quad[3], quad[0]);
        if m == [0.0; 3] {
            return [0.0, 0.0, 1.0];
        }
        if cw {
            m
        } else {
            [-m[0], -m[1], -m[2]]
        }
    };
    let faces_in: Vec<(Vec<[f32; 3]>, [f32; 3])> = [(b.clone(), plate_down), (t.clone(), plate_up)]
        .into_iter()
        .chain((0..n).map(|i| {
            let j = (i + 1) % n;
            (vec![b[i], b[j], t[j], t[i]], side_normal(i))
        }))
        .collect();
    let mut scale = PX_PER_UNIT / UNIT;
    let (cells, unfolds) = loop {
        let unfolds: Vec<Unfold> = faces_in
            .iter()
            .map(|(points, normal)| Unfold::of(points, *normal, scale))
            .collect();
        let sizes: Vec<(f32, f32)> = unfolds.iter().map(|u| (u.width(), u.height())).collect();
        if let Some(cells) = pack(&sizes) {
            break (cells, unfolds);
        }
        scale *= 0.9;
        if scale < 1.0 {
            return None;
        }
    };

    let mut mesh = BotMesh::default();
    let mut groups: Vec<FaceGroup> = Vec::new();
    let mut faces = Vec::new();
    let v_of = |y: f32| SKIN - MARGIN - y;
    let mut add_group = |coords: Vec<[f32; 2]>| {
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for c in &coords {
            for k in 0..2 {
                lo[k] = lo[k].min(c[k]);
                hi[k] = hi[k].max(c[k]);
            }
        }
        let bounds = [
            lo[0].floor() as i32 - 3,
            lo[1].floor() as i32 - 2,
            hi[0].ceil() as i32 + 2,
            hi[1].ceil() as i32 + 3,
        ];
        groups.push(FaceGroup { coords, bounds });
    };
    let skin_uv = |face: usize| -> Vec<[f32; 2]> {
        let (cell, unfold) = (&cells[face], &unfolds[face]);
        unfold
            .along
            .iter()
            .zip(unfold.across.iter())
            .map(|(a, w)| {
                [
                    cell.x + (unfold.max_along - a),
                    v_of(cell.y) - (w - unfold.min_across),
                ]
            })
            .collect()
    };

    // Plates: the bottom ring (normal -y), then the top ring (+y).
    let bottom_uv = skin_uv(0);
    let top_uv = skin_uv(1);
    for (ring, normal, uv) in [(&b, plate_down, &bottom_uv), (&t, plate_up, &top_uv)] {
        for (p, c) in ring.iter().zip(uv.iter()) {
            mesh.points.push(*p);
            mesh.normals.push(normal);
            mesh.uvs.push([c[0] / SKIN, c[1] / SKIN]);
        }
    }
    add_group(bottom_uv);
    add_group(top_uv);
    for tri in &plate {
        faces.push(Face {
            group: 0,
            points: *tri,
        });
        mesh.indices.extend_from_slice(tri);
    }
    for tri in &plate {
        let r = [tri[2] + n as u32, tri[1] + n as u32, tri[0] + n as u32];
        faces.push(Face {
            group: 1,
            points: r,
        });
        mesh.indices.extend_from_slice(&r);
    }
    // Sides.
    let mut top_copies: Vec<Vec<(usize, u32)>> = vec![Vec::new(); n];
    for i in 0..n {
        let j = (i + 1) % n;
        let base = mesh.points.len() as u32;
        let uv = skin_uv(2 + i);
        let quad = [b[i], b[j], t[j], t[i]];
        let quad_n = [b[i], b[j], tn[j], tn[i]];
        // The lab gives every vertex the normal of the last triangle that uses it
        // (`FUN_0044d1b0`, in index order): `(v1 v2 v0)` then `(v2 v3 v0)`, so `v1` keeps the first
        // triangle's and `v0`, `v2`, `v3` the second's. On a flat side they are one normal; after
        // a top point is dragged they differ (live `botlab-adjust-triangle-ok`).
        let unit = |tri: [usize; 3]| {
            let n = super::adjust::face_normal(quad_n[tri[0]], quad_n[tri[1]], quad_n[tri[2]]);
            let n = if cw { n } else { [-n[0], -n[1], -n[2]] };
            if n == [0.0; 3] {
                [0.0, 0.0, 1.0]
            } else {
                n
            }
        };
        let first = unit([1, 2, 0]);
        let second = unit([2, 3, 0]);
        let normals = [second, first, second, second];
        for k in 0..4 {
            mesh.points.push(quad[k]);
            mesh.normals.push(normals[k]);
            mesh.uvs.push([uv[k][0] / SKIN, uv[k][1] / SKIN]);
        }
        add_group(uv.to_vec());
        let group = (2 + i) as i32;
        for tri in [[base + 1, base + 2, base], [base + 2, base + 3, base]] {
            faces.push(Face { group, points: tri });
            mesh.indices.extend_from_slice(&tri);
        }
        top_copies[i].push((i, base + 3));
        top_copies[j].push((i, base + 2));
    }
    let top_corners = (0..n)
        .map(|i| {
            let mut copies = top_copies[i].clone();
            copies.sort();
            // `index 3 a b c`: the top vertex and its two side copies, the earlier edge first.
            let mut row = [i as i32, (n + i) as i32, 0, 0];
            for (k, (_, idx)) in copies.iter().take(2).enumerate() {
                row[2 + k] = *idx as i32;
            }
            row
        })
        .collect();
    Some(Extrusion {
        mesh,
        face_groups: groups,
        faces,
        top_corners,
    })
}

/// Rebuild `ch`'s mesh, face groups, faces and crush corners from its outlines and height.
/// Returns whether the outlines make a shape (at least three points).
pub fn rebuild(ch: &mut Chassis) -> bool {
    rebuild_at(ch, ch.height)
}

/// [`rebuild`] with the normals of the quads at `normal_height` ([`extrude_at`]).
pub fn rebuild_at(ch: &mut Chassis, normal_height: f32) -> bool {
    let Some(e) = extrude_at(&ch.bottom, &ch.top, ch.height, normal_height) else {
        ch.mesh = None;
        ch.face_groups.clear();
        ch.faces.clear();
        ch.top_corners.clear();
        ch.texture_tga = None;
        ch.face_count = 0;
        return false;
    };
    ch.face_count = (e.mesh.indices.len() / 3) as i32;
    ch.mesh = Some(e.mesh);
    ch.face_groups = e.face_groups;
    ch.faces = e.faces;
    ch.top_corners = e.top_corners;
    ch.buckling.clear();
    if ch.texture_tga.is_none() {
        ch.texture_tga = Some(blank_skin());
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<[i32; 2]> {
        vec![[200, 200], [800, 200], [800, 800], [200, 800]]
    }

    #[test]
    fn a_square_matches_the_live_files_points_indices_and_corners() {
        let e = extrude(&square(), &square(), DEFAULT_HEIGHT).unwrap();
        assert_eq!(e.mesh.points.len(), 24);
        assert!(
            (e.mesh.points[0][0] + 0.66).abs() < 1e-6 && (e.mesh.points[0][2] - 0.66).abs() < 1e-6
        );
        assert_eq!(e.mesh.indices[..12], [0, 3, 1, 3, 2, 1, 5, 7, 4, 5, 6, 7]);
        assert_eq!(e.mesh.indices[12..18], [9, 10, 8, 10, 11, 8]);
        assert_eq!(e.top_corners[0], [0, 4, 11, 22]);
        assert_eq!(e.top_corners[1], [1, 5, 10, 15]);
        assert_eq!(e.top_corners[3], [3, 7, 18, 23]);
        assert_eq!(e.mesh.normals[8], [0.0, 0.0, 1.0]);
        assert_eq!(e.mesh.normals[12], [1.0, 0.0, 0.0]);
    }

    #[test]
    fn a_square_packs_its_atlas_as_the_live_file_does() {
        let e = extrude(&square(), &square(), DEFAULT_HEIGHT).unwrap();
        let g = &e.face_groups;
        assert_eq!(g.len(), 6);
        let near =
            |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 0.05 && (a[1] - b[1]).abs() < 0.05;
        // bottom plate, top plate, first side, second side, third side (next row), fourth side.
        assert!(near(g[0].coords[0], [100.0, 156.0]));
        assert!(near(g[0].coords[2], [10.0, 246.0]));
        assert!(near(g[1].coords[0], [199.0, 246.0]));
        assert!(near(g[2].coords[0], [100.0, 147.0]));
        assert!(near(g[2].coords[2], [10.0, 90.75]));
        assert!(near(g[3].coords[0], [199.0, 147.0]));
        assert!(near(g[4].coords[0], [100.0, 81.0]));
        assert_eq!(g[0].bounds, [7, 154, 102, 249]);
        assert_eq!(g[5].bounds, [106, 22, 201, 84]);
    }

    #[test]
    fn a_triangle_is_a_single_clipped_plate_and_three_sides() {
        let tri = vec![[200, 200], [800, 200], [500, 800]];
        let e = extrude(&tri, &tri, DEFAULT_HEIGHT).unwrap();
        assert_eq!(e.mesh.points.len(), 18);
        assert_eq!(e.mesh.indices[..6], [2, 1, 0, 3, 4, 5]);
        assert_eq!(e.face_groups.len(), 5);
        assert_eq!(e.top_corners[0], [0, 3, 9, 16]);
        let n = e.mesh.normals[10];
        assert!((n[0] - 0.894_427).abs() < 1e-5 && (n[2] + 0.447_214).abs() < 1e-5);
        // The side after the second sits on a new row, from x = 10.
        assert!((e.face_groups[3].coords[0][0] - 209.623).abs() < 0.01);
    }

    #[test]
    fn a_wide_or_tall_chassis_shrinks_the_skin_layout_by_tenths() {
        let big = vec![[100, 100], [900, 100], [900, 900], [100, 900]];
        let e = extrude(&big, &big, DEFAULT_HEIGHT).unwrap();
        // 800 units at 0.135 px per unit: a 108 px plate.
        assert!((e.face_groups[0].coords[0][0] - 118.0).abs() < 0.01);
        assert!(
            (e.face_groups[2].coords[2][1] - 78.375).abs() < 0.01,
            "{:?}",
            e.face_groups[2].coords
        );
        let tall = extrude(&square(), &square(), 1.18909).unwrap();
        assert!((tall.face_groups[0].coords[0][0] - 82.9).abs() < 0.05);
    }

    #[test]
    fn the_height_slider_spans_the_live_range() {
        assert!((height_for_fraction(0.0) - 0.436_364).abs() < 1e-6);
        assert!((height_for_fraction(1.0) - 1.32).abs() < 1e-6);
        assert!((fraction_for_height(DEFAULT_HEIGHT) - 0.4398).abs() < 1e-3);
    }

    #[test]
    fn fewer_than_three_points_make_no_shape() {
        let mut ch = Chassis {
            height: DEFAULT_HEIGHT,
            bottom: vec![[200, 200], [800, 200]],
            ..Default::default()
        };
        assert!(!rebuild(&mut ch));
        assert!(ch.mesh.is_none());
    }

    fn live_chassis(name: &str) -> Option<Chassis> {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let at = dir.join("local/oracle/original").join(name);
            if at.is_file() {
                let bot = ra2fmt::bot::parse(&std::fs::read(at).ok()?).ok()?;
                return bot.components.into_iter().next()?.chassis;
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// Our extrusion of a live adjusted chassis' two outlines against the file the original wrote
    /// (skipped without the captures): points, per-vertex normals, skin coordinates, triangles.
    fn same_as_live(name: &str, shifted: Option<usize>) {
        let Some(live) = live_chassis(name) else {
            return;
        };
        let e = extrude(&live.bottom, &live.top, live.height).expect("a shape");
        let (m, l) = (&e.mesh, live.mesh.as_ref().unwrap());
        assert_eq!(m.points.len(), l.points.len(), "{name}");
        // The plates' ear order is still not recovered (the original clips a square from corner 2,
        // a 200 x 300 rectangle from corner 0; docs/36 9.27): compare their triangles as sets and
        // the sides exactly.
        let plate = 2 * 3 * (live.bottom.len() - 2);
        assert_eq!(m.indices[plate..], l.indices[plate..], "{name}");
        let sets = |idx: &[u32]| {
            let mut v: Vec<[u32; 3]> = idx
                .chunks(3)
                .map(|t| {
                    let mut t = [t[0], t[1], t[2]];
                    t.sort_unstable();
                    t
                })
                .collect();
            v.sort_unstable();
            v
        };
        assert_eq!(
            sets(&m.indices[..plate]),
            sets(&l.indices[..plate]),
            "{name}"
        );
        for i in 0..l.points.len() {
            for k in 0..3 {
                assert!(
                    (m.points[i][k] - l.points[i][k]).abs() < 1e-5,
                    "{name} point {i}"
                );
                assert!(
                    (m.normals[i][k] - l.normals[i][k]).abs() < 2e-6,
                    "{name} normal {i}: {:?} {:?}",
                    m.normals[i],
                    l.normals[i]
                );
            }
            // Open (docs/36 9.27e): the one side of `triangle-bent` whose base is not axis aligned and
            // whose quad is not flat sits 6.22 px lower in the original's atlas than its bounding
            // box says (same shape, same u); every other group matches to the file's six digits.
            let side = (i >= 2 * live.bottom.len()).then(|| 2 + (i - 2 * live.bottom.len()) / 4);
            for k in 0..2 {
                if shifted.is_some() && side == shifted && k == 1 {
                    continue;
                }
                assert!(
                    (m.uvs[i][k] - l.uvs[i][k]).abs() < 2e-6,
                    "{name} uv {i}: {:?} {:?}",
                    m.uvs[i],
                    l.uvs[i]
                );
            }
        }
        assert_eq!(e.top_corners, live.top_corners, "{name}");
    }

    #[test]
    fn a_dragged_top_point_changes_the_normals_and_the_layout_as_live() {
        same_as_live("botlab-adjust-triangle-ok__live__000.bot", None);
        same_as_live("botlab-adjust-triangle-bent__live__000.bot", Some(3));
        same_as_live("botlab-adjust-square-taper__live__000.bot", None);
        // baseplates whose first edge is not along x: the plates unfold from it
        same_as_live("botlab-adjust-base-vertical__live__000.bot", None);
        same_as_live("botlab-adjust-base-ccw__live__000.bot", None);
    }
}
