//! The chassis mesh the original draws: the file's faces cut into grids (docs/34 section 11.6).
//!
//! A `.bot` stores the chassis as a bare prism: a bottom plate, a top plate and one quad per outline
//! edge (Scout: 20 triangles). The game draws a finer mesh (Scout 79, Iceberg 132, ALARM 70, FlapJack
//! 116; `ra2_vbdump.js`), which is also the mesh a hit crushes. The vertex buffers and the files'
//! `buckling` points (v1.11 and later, which are the cut's interior points with the game's jitter in
//! them) fix the construction, checked on every stock robot with a vertex dump:
//!
//! * **Walls.** Each wall quad becomes a grid of 2 rows by `cols` columns; rows at 0, 1/2 and 1 of the
//!   height. `cols = 1 + floor(sqrt(bottom edge length / 0.44 m))` (fits the 276 walls of the 45 stock
//!   robots that carry buckle points: 1 for edges under 0.4 m, 2 up to 1.76 m, 3 beyond). Two triangles
//!   a cell. The texture coordinates are the quad's, interpolated evenly.
//! * **Top plate.** The outline of the top plate takes the wall grids' top-row points (so it has
//!   `n + sum(cols - 1)` points) and an inner ring of as many points, 0.05 to 0.11 m inside (the `buckling`
//!   points that are on the top plane and not on an outline edge). The plate is the strip between the
//!   two loops (2 triangles a segment) and the ring polygon (`m - 2` triangles).
//! * **Bottom plate** is the file's own.
//!
//! The interior points come from the file's `buckling` points when it has them (their positions carry
//! the game's few millimetres of jitter); otherwise they lie on the straight lines and the ring is
//! 0.08 m towards the centre.
//!
//! Not recovered: the order of the original's vertices and its choice of cell diagonals.

use glam::Vec3;
use ra2fmt::bot::Chassis;

/// 0.44 m, the edge length of one column's worth: `cols = 1 + floor(sqrt(length / CELL))`.
const CELL: f32 = 0.44;
/// How far the ring lies inside the top plate's outline when the file does not say (live, Iceberg: 0.056 to
/// 0.111, mean 0.085).
const RING_INSET: f32 = 0.08;
/// A buckle point lies on a line when it is this close to it (metres).
const ON_LINE: f32 = 0.02;

/// The chassis mesh in the chassis' own space, in the file's triangle winding.
#[derive(Debug, Clone, PartialEq)]
pub struct SubMesh {
    pub points: Vec<Vec3>,
    /// The skin coordinate of each point, in 0..1.
    pub uvs: Vec<[f32; 2]>,
    /// The face (face group) each point belongs to: points are not shared between faces.
    pub face: Vec<u32>,
    pub tris: Vec<[u32; 3]>,
}

impl SubMesh {
    fn push(&mut self, p: Vec3, uv: [f32; 2], face: u32) -> u32 {
        self.points.push(p);
        self.uvs.push(uv);
        self.face.push(face);
        (self.points.len() - 1) as u32
    }
}

/// The number of columns a wall whose bottom edge is `length` metres long is cut into.
pub fn columns(length: f32) -> usize {
    1 + (length / CELL + 1e-4).sqrt().floor() as usize
}

struct Wall {
    group: usize,
    /// File vertex indices of the bottom pair and the top pair, matched (`bottom[k]` is under `top[k]`).
    bottom: [usize; 2],
    top: [usize; 2],
    /// Where the top pair sits in the top plate's loop (the faces do not share vertices, so by position).
    top_loop: [usize; 2],
    cols: usize,
    /// The top row's points from `top[0]` to `top[1]`, ends included.
    top_row: Vec<Vec3>,
}

/// Cut the chassis into the original's grids, or `None` when the file's faces are not the prism the
/// rule describes (the caller then draws the file's mesh as it is).
pub fn tessellate(ch: &Chassis) -> Option<SubMesh> {
    let mesh = ch.mesh.as_ref()?;
    if ch.face_groups.is_empty() || mesh.points.is_empty() {
        return None;
    }
    let pts: Vec<Vec3> = mesh.points.iter().map(|p| Vec3::from(*p)).collect();
    let tris_in_file = mesh.indices.len() / 3;
    // Some files list every face twice (Razor's 12 triangles are 24 rows).
    let mut faces: Vec<&ra2fmt::bot::Face> = Vec::new();
    for f in &ch.faces {
        if !faces
            .iter()
            .any(|g| g.group == f.group && g.points == f.points)
        {
            faces.push(f);
        }
    }
    if faces.len() != tris_in_file {
        return None;
    }
    let ymin = pts.iter().map(|p| p.y).fold(f32::MAX, f32::min);
    let ymax = pts.iter().map(|p| p.y).fold(f32::MIN, f32::max);
    let height = ymax - ymin;
    if height < 1e-3 {
        return None;
    }

    // Each group's vertices (ascending, as the skin coordinates are listed) and their skin coordinates.
    let mut group_points: Vec<Vec<usize>> = Vec::new();
    let mut group_uv: Vec<std::collections::HashMap<usize, [f32; 2]>> = Vec::new();
    for (gi, fg) in ch.face_groups.iter().enumerate() {
        let mut seen: Vec<usize> = Vec::new();
        for f in faces.iter().filter(|f| f.group == gi as i32) {
            for &i in &f.points {
                let i = i as usize;
                if i >= pts.len() {
                    return None;
                }
                if !seen.contains(&i) {
                    seen.push(i);
                }
            }
        }
        seen.sort_unstable();
        if seen.len() != fg.coords.len() {
            return None;
        }
        group_uv.push(
            seen.iter()
                .zip(&fg.coords)
                .map(|(&i, c)| (i, [c[0] / 256.0, c[1] / 256.0]))
                .collect(),
        );
        group_points.push(seen);
    }
    if faces
        .iter()
        .any(|f| f.group < 0 || f.group as usize >= ch.face_groups.len())
    {
        return None;
    }

    let is_flat_at = |g: &[usize], y: f32| g.iter().all(|&i| (pts[i].y - y).abs() < 1e-3);
    let mut bottom_group = None;
    let mut top_group = None;
    let mut wall_groups = Vec::new();
    for (gi, g) in group_points.iter().enumerate() {
        if g.len() >= 3 && is_flat_at(g, ymin) {
            if bottom_group.replace(gi).is_some() {
                return None;
            }
        } else if g.len() >= 3 && is_flat_at(g, ymax) {
            if top_group.replace(gi).is_some() {
                return None;
            }
        } else if g.len() == 4 {
            wall_groups.push(gi);
        } else {
            return None;
        }
    }
    let (bottom_group, top_group) = (bottom_group?, top_group?);

    // The top plate's outline loop, in file order, and the walls under its edges.
    let loop_ids = group_points[top_group].clone();
    let n = loop_ids.len();
    let buckle: Vec<Vec3> = ch.buckling.iter().map(|b| Vec3::from(b.position)).collect();

    let mut walls: Vec<Wall> = Vec::new();
    for &gi in &wall_groups {
        let g = &group_points[gi];
        let mut low: Vec<usize> = g
            .iter()
            .copied()
            .filter(|&i| pts[i].y < ymin + 1e-3)
            .collect();
        let mut high: Vec<usize> = g
            .iter()
            .copied()
            .filter(|&i| pts[i].y > ymax - 1e-3)
            .collect();
        if low.len() != 2 || high.len() != 2 {
            return None;
        }
        // Pair each bottom corner with the top corner over it: the pairing in which the two edges run
        // the same way (the nearer corner is a tie on a corner wall).
        let flat = |i: usize| Vec3::new(pts[i].x, 0.0, pts[i].z);
        if (flat(low[1]) - flat(low[0])).dot(flat(high[1]) - flat(high[0])) < 0.0 {
            high.swap(0, 1);
        }
        // `low[0]` first: follow the outline direction (top loop order) so rows run the same way.
        let pos = |i: usize| {
            loop_ids
                .iter()
                .position(|&x| pts[x].distance(pts[i]) < 1e-3)
        };
        let (p0, p1) = (pos(high[0])?, pos(high[1])?);
        if (p0 + 1) % n != p1 {
            low.swap(0, 1);
            high.swap(0, 1);
            if (p1 + 1) % n != p0 {
                return None;
            }
        }
        let length = pts[low[0]].distance(pts[low[1]]);
        let cols = columns(length);
        // The top row: buckle points on the top edge, in order, else evenly spaced.
        let (a, b) = (pts[high[0]], pts[high[1]]);
        let mut interior: Vec<(f32, Vec3)> = buckle
            .iter()
            .filter(|p| (p.y - ymax).abs() < 1e-3 && on_segment(**p, a, b))
            .map(|p| ((*p - a).dot(b - a) / (b - a).length_squared(), *p))
            .collect();
        interior.sort_by(|x, y| x.0.total_cmp(&y.0));
        let top_row: Vec<Vec3> = if interior.len() + 1 == cols {
            std::iter::once(a)
                .chain(interior.iter().map(|x| x.1))
                .chain(std::iter::once(b))
                .collect()
        } else {
            (0..=cols)
                .map(|k| a.lerp(b, k as f32 / cols as f32))
                .collect()
        };
        let top_loop = [pos(high[0])?, pos(high[1])?];
        walls.push(Wall {
            group: gi,
            bottom: [low[0], low[1]],
            top: [high[0], high[1]],
            top_loop,
            cols,
            top_row,
        });
    }
    if walls.len() != n {
        return None;
    }

    let mut out = SubMesh {
        points: Vec::new(),
        uvs: Vec::new(),
        face: Vec::new(),
        tris: Vec::new(),
    };

    // The file's own triangles of one group, oriented as the file has them, with the group's skin
    // coordinates.
    let push_file_group = |out: &mut SubMesh, gi: usize| {
        let mut remap = std::collections::HashMap::new();
        for f in faces.iter().filter(|f| f.group == gi as i32) {
            let mut tri = [0u32; 3];
            for (k, &i) in f.points.iter().enumerate() {
                let i = i as usize;
                let id = *remap.entry(i).or_insert_with(|| {
                    out.push(
                        pts[i],
                        group_uv[gi].get(&i).copied().unwrap_or([0.0; 2]),
                        gi as u32,
                    )
                });
                tri[k] = id;
            }
            out.tris.push(tri);
        }
    };
    push_file_group(&mut out, bottom_group);

    // Walls.
    let wall_normal = |gi: usize| -> Vec3 {
        let mut n = Vec3::ZERO;
        for f in faces.iter().filter(|f| f.group == gi as i32) {
            let (a, b, c) = (
                pts[f.points[0] as usize],
                pts[f.points[1] as usize],
                pts[f.points[2] as usize],
            );
            n += (b - a).cross(c - a);
        }
        n
    };
    for w in &walls {
        let uv = |i: usize| group_uv[w.group].get(&i).copied().unwrap_or([0.0; 2]);
        let (b0, b1, t0, t1) = (
            pts[w.bottom[0]],
            pts[w.bottom[1]],
            pts[w.top[0]],
            pts[w.top[1]],
        );
        let (ub0, ub1, ut0, ut1) = (uv(w.bottom[0]), uv(w.bottom[1]), uv(w.top[0]), uv(w.top[1]));
        let lerp2 =
            |a: [f32; 2], b: [f32; 2], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        // Rows: floor, middle, top. The middle row's ends are the vertical edges' midpoints; its
        // interior points are the file's buckle points on that line, else evenly spaced.
        let (m0, m1) = (b0.lerp(t0, 0.5), b1.lerp(t1, 0.5));
        let mut mid: Vec<(f32, Vec3)> = buckle
            .iter()
            .filter(|p| (p.y - (ymin + height * 0.5)).abs() < 1e-3 && on_segment(**p, m0, m1))
            .map(|p| ((*p - m0).dot(m1 - m0) / (m1 - m0).length_squared(), *p))
            .collect();
        mid.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mid_row: Vec<Vec3> = if mid.len() + 1 == w.cols {
            std::iter::once(m0)
                .chain(mid.iter().map(|x| x.1))
                .chain(std::iter::once(m1))
                .collect()
        } else {
            (0..=w.cols)
                .map(|k| m0.lerp(m1, k as f32 / w.cols as f32))
                .collect()
        };
        let floor_row: Vec<Vec3> = (0..=w.cols)
            .map(|k| b0.lerp(b1, k as f32 / w.cols as f32))
            .collect();
        let rows = [&floor_row, &mid_row, &w.top_row];
        let mut ids = [[0u32; 8]; 3];
        if w.cols + 1 > 8 {
            return None;
        }
        for (r, row) in rows.iter().enumerate() {
            let t = r as f32 * 0.5;
            for (k, p) in row.iter().enumerate() {
                let s = k as f32 / w.cols as f32;
                let at = lerp2(lerp2(ub0, ub1, s), lerp2(ut0, ut1, s), t);
                ids[r][k] = out.push(*p, at, w.group as u32);
            }
        }
        let want = wall_normal(w.group);
        for r in 0..2 {
            for k in 0..w.cols {
                let (p00, p10, p11, p01) =
                    (ids[r][k], ids[r][k + 1], ids[r + 1][k + 1], ids[r + 1][k]);
                for tri in [[p00, p10, p11], [p00, p11, p01]] {
                    out.tris.push(orient(&out.points, tri, want));
                }
            }
        }
    }

    // Top plate.
    let top_normal = wall_normal(top_group);
    let mut outer: Vec<Vec3> = Vec::new();
    for k in 0..n {
        let w = walls
            .iter()
            .find(|w| w.top_loop == [k, (k + 1) % n])
            .or_else(|| walls.iter().find(|w| w.top_loop == [(k + 1) % n, k]))?;
        let mut row = w.top_row.clone();
        if w.top_loop == [(k + 1) % n, k] {
            row.reverse();
        }
        row.pop();
        outer.extend(row);
    }
    let m = outer.len();
    let centre = outer.iter().fold(Vec3::ZERO, |s, p| s + *p) / m as f32;
    // The ring: the buckle points on the top plane that are not on an outline edge, matched to the
    // outer points nearest them; else straight in towards the centre.
    let on_outline = |p: Vec3| {
        (0..n).any(|k| {
            let (a, b) = (pts[loop_ids[k]], pts[loop_ids[(k + 1) % n]]);
            on_segment_closed(p, a, b)
        })
    };
    let candidates: Vec<Vec3> = buckle
        .iter()
        .copied()
        .filter(|p| (p.y - ymax).abs() < 1e-3 && !on_outline(*p))
        .collect();
    let ring: Vec<Vec3> = match match_nearest(&outer, &candidates) {
        Some(r) => r,
        None => outer
            .iter()
            .map(|p| {
                let d = Vec3::new(centre.x - p.x, 0.0, centre.z - p.z).normalize_or_zero();
                *p + d * RING_INSET
            })
            .collect(),
    };
    // The plate's skin coordinates: the affine map of the group's own corners.
    let affine = fit_affine(&group_points[top_group], &pts, &group_uv[top_group])?;
    let uv_at = |p: Vec3| -> [f32; 2] {
        [
            affine[0][0] + affine[0][1] * p.x + affine[0][2] * p.z,
            affine[1][0] + affine[1][1] * p.x + affine[1][2] * p.z,
        ]
    };
    let gi = top_group as u32;
    let outer_ids: Vec<u32> = outer.iter().map(|p| out.push(*p, uv_at(*p), gi)).collect();
    let ring_ids: Vec<u32> = ring.iter().map(|p| out.push(*p, uv_at(*p), gi)).collect();
    for k in 0..m {
        let k1 = (k + 1) % m;
        for tri in [
            [outer_ids[k], outer_ids[k1], ring_ids[k1]],
            [outer_ids[k], ring_ids[k1], ring_ids[k]],
        ] {
            out.tris.push(orient(&out.points, tri, top_normal));
        }
    }
    for tri in ear_clip(&ring, &ring_ids) {
        out.tris.push(orient(&out.points, tri, top_normal));
    }
    Some(out)
}

/// `tri` wound so its normal agrees with `want`.
fn orient(points: &[Vec3], tri: [u32; 3], want: Vec3) -> [u32; 3] {
    let (a, b, c) = (
        points[tri[0] as usize],
        points[tri[1] as usize],
        points[tri[2] as usize],
    );
    if (b - a).cross(c - a).dot(want) >= 0.0 {
        tri
    } else {
        [tri[0], tri[2], tri[1]]
    }
}

/// `p` on the open segment `a..b` (not at its ends), within [`ON_LINE`].
fn on_segment(p: Vec3, a: Vec3, b: Vec3) -> bool {
    let d = b - a;
    let l2 = d.length_squared();
    if l2 < 1e-9 {
        return false;
    }
    let t = (p - a).dot(d) / l2;
    let end = ON_LINE / l2.sqrt();
    if t <= end || t >= 1.0 - end {
        return false;
    }
    (p - (a + d * t)).length() < ON_LINE
}

/// `p` on the closed segment `a..b`.
fn on_segment_closed(p: Vec3, a: Vec3, b: Vec3) -> bool {
    let d = b - a;
    let l2 = d.length_squared();
    if l2 < 1e-9 {
        return false;
    }
    let t = ((p - a).dot(d) / l2).clamp(0.0, 1.0);
    (p - (a + d * t)).length() < ON_LINE
}

/// For each of `outer` the nearest unused candidate, or `None` when there are not exactly as many.
fn match_nearest(outer: &[Vec3], candidates: &[Vec3]) -> Option<Vec<Vec3>> {
    if outer.len() != candidates.len() || outer.is_empty() {
        return None;
    }
    let mut used = vec![false; candidates.len()];
    let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
    for (i, o) in outer.iter().enumerate() {
        for (j, c) in candidates.iter().enumerate() {
            pairs.push((o.distance(*c), i, j));
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut chosen = vec![None; outer.len()];
    for (d, i, j) in pairs {
        if chosen.iter().all(|c| c.is_some()) {
            break;
        }
        if d > 0.3 {
            return None;
        }
        if chosen[i].is_none() && !used[j] {
            chosen[i] = Some(candidates[j]);
            used[j] = true;
        }
    }
    chosen.into_iter().collect()
}

/// `u = a + b x + c z`, `v = d + e x + f z` through the group's own corners (three that are not in a
/// line are enough; the rest are the plate's own and agree).
fn fit_affine(
    ids: &[usize],
    pts: &[Vec3],
    uv: &std::collections::HashMap<usize, [f32; 2]>,
) -> Option<[[f32; 3]; 2]> {
    let mut best: Option<(f32, [usize; 3])> = None;
    for a in 0..ids.len() {
        for b in a + 1..ids.len() {
            for c in b + 1..ids.len() {
                let (p, q, r) = (pts[ids[a]], pts[ids[b]], pts[ids[c]]);
                let area = ((q.x - p.x) * (r.z - p.z) - (q.z - p.z) * (r.x - p.x)).abs();
                if best.is_none_or(|(s, _)| area > s) {
                    best = Some((area, [ids[a], ids[b], ids[c]]));
                }
            }
        }
    }
    let (area, [a, b, c]) = best?;
    if area < 1e-6 {
        return None;
    }
    let (p, q, r) = (pts[a], pts[b], pts[c]);
    let det = (q.x - p.x) * (r.z - p.z) - (q.z - p.z) * (r.x - p.x);
    let mut out = [[0.0f32; 3]; 2];
    for axis in 0..2 {
        let (up, uq, ur) = (uv[&a][axis], uv[&b][axis], uv[&c][axis]);
        let dx = ((uq - up) * (r.z - p.z) - (ur - up) * (q.z - p.z)) / det;
        let dz = ((ur - up) * (q.x - p.x) - (uq - up) * (r.x - p.x)) / det;
        out[axis] = [up - dx * p.x - dz * p.z, dx, dz];
    }
    Some(out)
}

/// Ear clipping of a simple polygon in the xz plane (`ids` index the same points).
fn ear_clip(ring: &[Vec3], ids: &[u32]) -> Vec<[u32; 3]> {
    let mut rest: Vec<usize> = (0..ring.len()).collect();
    let mut out = Vec::new();
    // Which way round the polygon runs.
    let area: f32 = (0..ring.len())
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            a.x * b.z - b.x * a.z
        })
        .sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let cross =
        |a: Vec3, b: Vec3, c: Vec3| ((b.x - a.x) * (c.z - a.z) - (b.z - a.z) * (c.x - a.x)) * sign;
    let mut guard = 0;
    while rest.len() > 3 && guard < 1000 {
        guard += 1;
        let m = rest.len();
        let mut clipped = false;
        for k in 0..m {
            let (i, j, l) = (rest[(k + m - 1) % m], rest[k], rest[(k + 1) % m]);
            let (a, b, c) = (ring[i], ring[j], ring[l]);
            if cross(a, b, c) <= 1e-9 {
                continue;
            }
            let inside = rest.iter().any(|&o| {
                if o == i || o == j || o == l {
                    return false;
                }
                let p = ring[o];
                cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0
            });
            if inside {
                continue;
            }
            out.push([ids[i], ids[j], ids[l]]);
            rest.remove(k);
            clipped = true;
            break;
        }
        if !clipped {
            // A degenerate ring: fan what is left rather than lose the plate.
            break;
        }
    }
    for k in 1..rest.len().saturating_sub(1) {
        out.push([ids[rest[0]], ids[rest[k]], ids[rest[k + 1]]]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn the_column_rule_matches_the_stock_walls() {
        // Edge lengths of the shipped outlines (units of 0.0022 m) and the columns the buckle points show.
        for (units, cols) in [
            (100.0, 1),
            (141.4, 1),
            (200.0, 2),
            (500.0, 2),
            (600.0, 2),
            (700.0, 2),
            (799.0, 2),
            (800.0, 3),
            (1000.0, 3),
        ] {
            assert_eq!(columns(units * 0.0022), cols, "{units}");
        }
    }

    fn repo() -> Option<PathBuf> {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            if dir.join("local/game").is_dir() && dir.join("local/oracle/original").is_dir() {
                return Some(dir);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len() / 2)
            .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).unwrap_or(0))
            .collect()
    }

    /// The `"key": "value"` string of a dump line.
    fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
        let at = line.find(&format!("\"{key}\""))?;
        let rest = &line[at + key.len() + 2..];
        let open = rest.find('"')? + 1;
        let close = rest[open..].find('"')? + open;
        Some(&rest[open..close])
    }

    /// The chassis draws of the live vertex dumps: the same number of triangles as ours, and every live
    /// vertex has one of ours within 4 mm, with the same skin coordinate to a texel.
    #[test]
    fn the_cut_reproduces_the_live_chassis_vertex_buffers() {
        let Some(root) = repo() else { return };
        for (bot, dump, tris) in [
            ("AI/Team0/Bot0.bot", "vbdump-scout__live__000.jsonl", 79),
            ("AI/Team7/Bot1.bot", "vbdump-iceberg__live__000.jsonl", 132),
            ("AI/Team10/Bot2.bot", "vbdump-razor__live__000.jsonl", 84),
            ("AI/Team0/Bot1.bot", "vbdump-scout__live__000.jsonl", 70),
            ("AI/Team8/Bot0.bot", "vbdump-flapjack__live__000.jsonl", 116),
        ] {
            let Ok(bytes) = std::fs::read(root.join("local/game").join(bot)) else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(root.join("local/oracle/original").join(dump))
            else {
                continue;
            };
            let parsed = ra2fmt::bot::parse(&bytes).unwrap();
            let ch = parsed.components[0].chassis.as_ref().unwrap();
            let sub = tessellate(ch).unwrap_or_else(|| panic!("{bot}: not cut"));
            assert_eq!(sub.tris.len(), tris, "{bot}: triangles");
            // The live draw with that many triangles.
            let live = text
                .lines()
                .find(|l| field(l, "ib").is_some_and(|ib| ib.len() / 4 / 3 == tris))
                .unwrap_or_else(|| panic!("{bot}: no live draw of {tris} triangles"));
            let vb = hex(field(live, "vb").unwrap());
            let floats: Vec<f32> = vb
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| f32::from_le_bytes(*c))
                .collect();
            // A file without `buckling` points (v1.07 / 1.08) leaves the cut's jitter and the ring to us.
            let exact = !ch.buckling.is_empty();
            let (tol_pos, tol_uv) = if exact { (0.004, 1.5) } else { (0.07, 6.0) };
            let mut worst = 0.0f32;
            let mut uv_bad = 0;
            for v in floats.as_chunks::<8>().0 {
                let p = Vec3::new(v[0], v[1], v[2]);
                let (best, at) = sub
                    .points
                    .iter()
                    .enumerate()
                    .map(|(i, q)| (q.distance(p), i))
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .unwrap();
                worst = worst.max(best);
                // Some face of that position has the live skin coordinate.
                let ok = sub.points.iter().enumerate().any(|(i, q)| {
                    q.distance(p) < tol_pos
                        && (sub.uvs[i][0] - v[6]).abs() * 256.0 < tol_uv
                        && (sub.uvs[i][1] - v[7]).abs() * 256.0 < tol_uv
                });
                if !ok {
                    uv_bad += 1;
                    eprintln!(
                        "{bot}: live {p:?} uv {:?} nearest {:?} uv {:?}",
                        &v[6..8],
                        sub.points[at],
                        sub.uvs[at]
                    );
                }
            }
            assert!(
                worst < tol_pos,
                "{bot}: a live vertex is {worst} m from the cut"
            );
            assert_eq!(uv_bad, 0, "{bot}: skin coordinates");
        }
    }
}
