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

struct Cell {
    x: f32,
    /// Distance from `v = 246` to the cell's top.
    y: f32,
    w: f32,
    h: f32,
}

/// Shelf-pack `sizes` in order at skin scale (`w`, `h` in px).
fn pack(sizes: &[(f32, f32)]) -> Option<Vec<Cell>> {
    let mut cells: Vec<Cell> = Vec::new();
    let mut cursor = MARGIN;
    for &(w, h) in sizes {
        let skyline = |x: f32, w: f32, cells: &[Cell]| {
            cells
                .iter()
                .filter(|c| c.x < x + w && x < c.x + c.w)
                .map(|c| c.y + ceil_px(c.h) + GAP)
                .fold(0.0_f32, f32::max)
        };
        let (x, y) = if cells.is_empty() {
            (MARGIN, 0.0)
        } else if cursor + w <= SKIN - MARGIN {
            (cursor, skyline(cursor, w, &cells))
        } else {
            (MARGIN, skyline(MARGIN, w, &cells))
        };
        if y + ceil_px(h) > FIT {
            return None;
        }
        cursor = x + w + GAP;
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
    let n = bottom.len();
    if n < 3 || height <= 0.0 {
        return None;
    }
    let top = if top.len() == n { top } else { bottom };
    let b: Vec<[f32; 3]> = bottom.iter().map(|p| world(*p, 0.0)).collect();
    let t: Vec<[f32; 3]> = top.iter().map(|p| world(*p, height)).collect();
    let area = signed_area(&b);
    if area == 0.0 {
        return None;
    }
    let cw = area < 0.0;
    let plate = ear_clip(&b, cw);

    // Plate extents in metres, side widths and the skin scale that fits.
    let ext = |ring: &[[f32; 3]]| {
        let mut e = [f32::MAX, f32::MIN, f32::MAX, f32::MIN];
        for p in ring {
            e = [
                e[0].min(p[0]),
                e[1].max(p[0]),
                e[2].min(p[2]),
                e[3].max(p[2]),
            ];
        }
        e
    };
    let (eb, et) = (ext(&b), ext(&t));
    let side_len = |i: usize| {
        let (p, q) = (b[i], b[(i + 1) % n]);
        ((q[0] - p[0]).powi(2) + (q[2] - p[2]).powi(2)).sqrt()
    };
    let mut scale = PX_PER_UNIT / UNIT;
    let cells = loop {
        let mut sizes = vec![
            ((eb[1] - eb[0]) * scale, (eb[3] - eb[2]) * scale),
            ((et[1] - et[0]) * scale, (et[3] - et[2]) * scale),
        ];
        sizes.extend((0..n).map(|i| (side_len(i) * scale, height * scale)));
        if let Some(cells) = pack(&sizes) {
            break cells;
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

    // Bottom plate: mirrored in u, v runs with -z.
    let c0 = &cells[0];
    let bottom_uv: Vec<[f32; 2]> = b
        .iter()
        .map(|p| {
            [
                c0.x + c0.w - (p[0] - eb[0]) * scale,
                v_of(c0.y + c0.h) + (eb[3] - p[2]) * scale,
            ]
        })
        .collect();
    // Top plate: mirrored in u, v runs with +z.
    let c1 = &cells[1];
    let top_uv: Vec<[f32; 2]> = t
        .iter()
        .map(|p| {
            [
                c1.x + c1.w - (p[0] - et[0]) * scale,
                v_of(c1.y + c1.h) + (p[2] - et[2]) * scale,
            ]
        })
        .collect();
    for (ring, normal, uv) in [
        (&b, [0.0, -1.0, 0.0], &bottom_uv),
        (&t, [0.0, 1.0, 0.0], &top_uv),
    ] {
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
        let cell = &cells[2 + i];
        let len = side_len(i) * scale;
        let (hi, lo) = (v_of(cell.y), v_of(cell.y) - height * scale);
        let right = cell.x + cell.w;
        let uv = [
            [right, hi],
            [right - len, hi],
            [right - len, lo],
            [right, lo],
        ];
        let quad = [b[i], b[j], t[j], t[i]];
        let d = [b[j][0] - b[i][0], b[j][1] - b[i][1], b[j][2] - b[i][2]];
        let up = [t[i][0] - b[i][0], t[i][1] - b[i][1], t[i][2] - b[i][2]];
        let mut normal = [
            d[1] * up[2] - d[2] * up[1],
            d[2] * up[0] - d[0] * up[2],
            d[0] * up[1] - d[1] * up[0],
        ];
        if !cw {
            normal = [-normal[0], -normal[1], -normal[2]];
        }
        let l = (normal[0].powi(2) + normal[1].powi(2) + normal[2].powi(2)).sqrt();
        let normal = if l > 0.0 {
            [normal[0] / l, normal[1] / l, normal[2] / l]
        } else {
            [0.0, 0.0, 1.0]
        };
        for k in 0..4 {
            mesh.points.push(quad[k]);
            mesh.normals.push(normal);
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
    let Some(e) = extrude(&ch.bottom, &ch.top, ch.height) else {
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
}
