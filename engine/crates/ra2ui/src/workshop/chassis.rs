//! The chassis data the workshop's drawing screens edit: the two outline
//! polygons, the extruded mesh, and the face groups that dress it.
//!
//! ## What the shipped screens are
//!
//! `botlab/chassis_structure1.gib` draws the **baseplate outline** (*"This is
//! the bottom of the robot"*), `chassis_structure2.gib` edits the **top panel**
//! and the extrusion height, and `ch_stepbuttons.gib` carries the `Snap to
//! grid` checkbox, the `clear outline button`, the height/radius/length/width
//! sliders and the two step buttons. `Basic_Tutorial.htm` states the rules:
//! at most **16** points, no two closer than one grid unit, no intersecting
//! lines, and the last point joins the first.
//!
//! The file stores both the outlines (`Chassis::bottom` / `Chassis::top`, in
//! `0..1000`) and the mesh those outlines are extruded into. Measured on all
//! 57 shipped bots, the mesh's rings are **exactly** the outlines:
//!
//! ```text
//! mesh[0..bottom.len()]  = bottom, x = (u-500)·0.0022, z = -(v-500)·0.0022, y = 0
//! mesh[bottom.len()..]   = top,    the same in x/z, y = Chassis::height
//! ```
//!
//! The rest of the mesh is one four-vertex quad per side. The **UV atlas is
//! not recovered**: `face_groups` carry a skin rectangle per side, and the
//! game's extrusion chooses the atlas. So this module edits:
//!
//! - the outlines (add/remove/clear/generate, and a point move, which also
//!   moves the matching ring vertex because the data says it is the same
//!   point);
//! - the mesh directly (add/remove a point or a face), which the writer
//!   reproduces exactly;
//! - the extrusion height, as a uniform y scale of the existing mesh, which is
//!   what "extrude the bottom outline to this height" means for a mesh that is
//!   already an extrusion.
//!
//! It does **not** regenerate the mesh or its face groups from a changed
//! outline: that is the game's extrusion and atlas packing, not recovered from
//! the shipped data. A new outline point is stored and saved, but the preview
//! keeps the existing mesh until the player also edits the mesh. That boundary
//! is named in `docs/15-workshop-model.md`.

use std::collections::BTreeMap;

use ra2fmt::bot::{Chassis, Face};

use ra2fmt::mount::{baseplate_to_world, world_to_baseplate};

/// The outline coordinate range the blueprint uses.
pub const OUTLINE_MAX: i32 = 1000;
/// The blueprint grid: `Unit Scale AP = .22 M`, so one grid square is 100
/// outline units (0.22 m).
pub const GRID_UNIT: i32 = 100;
/// `Basic_Tutorial.htm`: "You have a maximum of 16 points to use."
pub const MAX_OUTLINE_POINTS: usize = 16;

/// Which of the two outlines a drawing operation edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outline {
    /// Chassis step 1, the baseplate.
    Bottom,
    /// Chassis step 2, the top panel.
    Top,
}

impl Outline {
    pub fn label(self) -> &'static str {
        match self {
            Outline::Bottom => "baseplate",
            Outline::Top => "top",
        }
    }
}

/// Why a chassis edit was refused.
#[derive(Clone, Debug, PartialEq)]
pub enum ChassisError {
    /// The point would make an outline longer than the tutorial's 16.
    TooManyPoints,
    /// "No two points can be closer than one grid unit."
    TooClose {
        unit: i32,
    },
    /// "No lines can ever intersect."
    SelfIntersecting,
    NoPoint {
        index: usize,
        count: usize,
    },
    NoFace {
        index: usize,
        count: usize,
    },
    NoGroup {
        index: usize,
        count: usize,
    },
    /// A face names a mesh point that does not exist.
    NoVertex {
        index: usize,
        count: usize,
    },
    /// A face repeats a vertex.
    DegenerateFace,
    /// The bot's chassis has no embedded mesh to edit.
    NoMesh,
    /// Height must be positive and finite.
    BadHeight(f32),
    /// The outline has fewer than three points and cannot be closed.
    TooFewPoints,
}

impl std::fmt::Display for ChassisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChassisError::TooManyPoints => {
                write!(
                    f,
                    "a chassis outline holds at most {MAX_OUTLINE_POINTS} points"
                )
            }
            ChassisError::TooClose { unit } => write!(
                f,
                "two outline points must be at least one grid unit ({unit}) apart"
            ),
            ChassisError::SelfIntersecting => write!(f, "chassis outline lines cannot intersect"),
            ChassisError::NoPoint { index, count } => {
                write!(f, "outline point {index} does not exist ({count} points)")
            }
            ChassisError::NoFace { index, count } => {
                write!(f, "chassis face {index} does not exist ({count} faces)")
            }
            ChassisError::NoGroup { index, count } => {
                write!(f, "face group {index} does not exist ({count} groups)")
            }
            ChassisError::NoVertex { index, count } => {
                write!(f, "mesh point {index} does not exist ({count} points)")
            }
            ChassisError::DegenerateFace => write!(f, "a face needs three different points"),
            ChassisError::NoMesh => write!(f, "the chassis has no embedded mesh"),
            ChassisError::BadHeight(h) => write!(f, "chassis height {h} is not positive"),
            ChassisError::TooFewPoints => write!(f, "an outline needs at least three points"),
        }
    }
}

impl std::error::Error for ChassisError {}

fn outline(ch: &Chassis, which: Outline) -> &[[i32; 2]] {
    match which {
        Outline::Bottom => &ch.bottom,
        Outline::Top => &ch.top,
    }
}

fn outline_mut(ch: &mut Chassis, which: Outline) -> &mut Vec<[i32; 2]> {
    match which {
        Outline::Bottom => &mut ch.bottom,
        Outline::Top => &mut ch.top,
    }
}

/// Round to the blueprint grid and clamp to the `0..1000` sheet.
pub fn snap(point: [i32; 2]) -> [i32; 2] {
    let round = |v: i32| ((v + GRID_UNIT / 2) / GRID_UNIT * GRID_UNIT).clamp(0, OUTLINE_MAX);
    [round(point[0]), round(point[1])]
}

fn clamp(point: [i32; 2]) -> [i32; 2] {
    [
        point[0].clamp(0, OUTLINE_MAX),
        point[1].clamp(0, OUTLINE_MAX),
    ]
}

fn cross(a: [i32; 2], b: [i32; 2], c: [i32; 2]) -> i64 {
    (b[0] - a[0]) as i64 * (c[1] - a[1]) as i64 - (b[1] - a[1]) as i64 * (c[0] - a[0]) as i64
}

fn on_segment(a: [i32; 2], b: [i32; 2], p: [i32; 2]) -> bool {
    p[0] >= a[0].min(b[0])
        && p[0] <= a[0].max(b[0])
        && p[1] >= a[1].min(b[1])
        && p[1] <= a[1].max(b[1])
}

/// Whether segments `a-b` and `c-d` meet (including touching endpoints).
fn segments_intersect(a: [i32; 2], b: [i32; 2], c: [i32; 2], d: [i32; 2]) -> bool {
    let (d1, d2, d3, d4) = (
        cross(c, d, a),
        cross(c, d, b),
        cross(a, b, c),
        cross(a, b, d),
    );
    if ((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0)) {
        return true;
    }
    (d1 == 0 && on_segment(c, d, a))
        || (d2 == 0 && on_segment(c, d, b))
        || (d3 == 0 && on_segment(a, b, c))
        || (d4 == 0 && on_segment(a, b, d))
}

/// Every edge of a closed outline: `(last, first)`, then consecutive pairs.
fn edges(points: &[[i32; 2]]) -> Vec<([i32; 2], [i32; 2])> {
    let mut out = Vec::new();
    if points.len() < 2 {
        return out;
    }
    for i in 0..points.len() {
        out.push((points[i], points[(i + 1) % points.len()]));
    }
    out
}

fn too_close(points: &[[i32; 2]], point: [i32; 2], skip: Option<usize>) -> bool {
    let unit = (GRID_UNIT as i64).pow(2);
    points.iter().enumerate().any(|(i, p)| {
        if Some(i) == skip {
            return false;
        }
        let dx = (p[0] - point[0]) as i64;
        let dy = (p[1] - point[1]) as i64;
        dx * dx + dy * dy < unit
    })
}

/// Refuse an outline that intersects itself. Adjacent edges share a vertex by
/// construction and are skipped; every other pair may only stay apart.
fn self_intersects(points: &[[i32; 2]]) -> bool {
    let edges = edges(points);
    let n = edges.len();
    for i in 0..n {
        for j in (i + 1)..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let (a, b) = edges[i];
            let (c, d) = edges[j];
            if segments_intersect(a, b, c, d) {
                return true;
            }
        }
    }
    false
}

/// Check a whole outline against the tutorial's rules.
pub fn validate_outline(points: &[[i32; 2]]) -> Result<(), ChassisError> {
    if points.len() < 3 {
        return Err(ChassisError::TooFewPoints);
    }
    if points.len() > MAX_OUTLINE_POINTS {
        return Err(ChassisError::TooManyPoints);
    }
    for i in 0..points.len() {
        for j in (i + 1)..points.len() {
            let (a, b) = (points[i], points[j]);
            let dx = (a[0] - b[0]) as i64;
            let dy = (a[1] - b[1]) as i64;
            if dx * dx + dy * dy < (GRID_UNIT as i64).pow(2) {
                return Err(ChassisError::TooClose { unit: GRID_UNIT });
            }
        }
    }
    if self_intersects(points) {
        return Err(ChassisError::SelfIntersecting);
    }
    Ok(())
}

/// Add a point to an outline.
pub fn add_outline_point(
    ch: &mut Chassis,
    which: Outline,
    point: [i32; 2],
    snap_to_grid: bool,
) -> Result<usize, ChassisError> {
    let point = clamp(if snap_to_grid { snap(point) } else { point });
    let points = outline(ch, which);
    if points.len() >= MAX_OUTLINE_POINTS {
        return Err(ChassisError::TooManyPoints);
    }
    if too_close(points, point, None) {
        return Err(ChassisError::TooClose { unit: GRID_UNIT });
    }
    let mut next = points.to_vec();
    next.push(point);
    if next.len() >= 4 && self_intersects(&next) {
        return Err(ChassisError::SelfIntersecting);
    }
    outline_mut(ch, which).push(point);
    Ok(next.len() - 1)
}

/// Remove an outline point.
pub fn remove_outline_point(
    ch: &mut Chassis,
    which: Outline,
    index: usize,
) -> Result<[i32; 2], ChassisError> {
    let points = outline(ch, which);
    let Some(point) = points.get(index).copied() else {
        return Err(ChassisError::NoPoint {
            index,
            count: points.len(),
        });
    };
    outline_mut(ch, which).remove(index);
    Ok(point)
}

/// "clear outline button": erase the outline, leaving the mesh alone.
pub fn clear_outline(ch: &mut Chassis, which: Outline) {
    outline_mut(ch, which).clear();
}

/// The automatic circle tool: a regular 12-gon around `centre` in outline
/// units. Twelve is inside the tutorial's 16-point limit.
pub fn set_circle(ch: &mut Chassis, which: Outline, centre: [i32; 2], radius: i32) {
    let radius = radius.clamp(GRID_UNIT, OUTLINE_MAX / 2) as f32;
    let mut points = Vec::with_capacity(12);
    for i in 0..12 {
        let angle = std::f32::consts::TAU * i as f32 / 12.0;
        let x = centre[0] as f32 + radius * angle.cos();
        let y = centre[1] as f32 + radius * angle.sin();
        points.push(clamp([x.round() as i32, y.round() as i32]));
    }
    *outline_mut(ch, which) = points;
}

/// The automatic rectangle tool: four corners around `centre`.
pub fn set_rectangle(ch: &mut Chassis, which: Outline, centre: [i32; 2], length: i32, width: i32) {
    let (l, w) = (
        (length / 2).clamp(GRID_UNIT / 2, OUTLINE_MAX / 2),
        (width / 2).clamp(GRID_UNIT / 2, OUTLINE_MAX / 2),
    );
    let points = vec![
        clamp([centre[0] - l, centre[1] - w]),
        clamp([centre[0] + l, centre[1] - w]),
        clamp([centre[0] + l, centre[1] + w]),
        clamp([centre[0] - l, centre[1] + w]),
    ];
    *outline_mut(ch, which) = points;
}

/// The mesh ring index a moved outline point maps to, when the mesh is the
/// corpus' extrusion of these two outlines.
pub fn ring_index(ch: &Chassis, which: Outline, index: usize) -> Option<usize> {
    let mesh = ch.mesh.as_ref()?;
    let n = ch.bottom.len();
    let (outline, offset, y) = match which {
        Outline::Bottom => (&ch.bottom, 0, 0.0),
        Outline::Top => (&ch.top, n, ch.height),
    };
    let point = *outline.get(index)?;
    let at = offset + index;
    let want = world_point(point, y);
    let got = *mesh.points.get(at)?;
    ((0..3).all(|i| (want[i] - got[i]).abs() < 1e-3)).then_some(at)
}

fn world_point(point: [i32; 2], y: f32) -> [f32; 3] {
    let world = baseplate_to_world([point[0] as f32, point[1] as f32]);
    [world[0], y, world[1]]
}

/// Move one outline point, and the mesh ring vertex it names when the corpus'
/// ring layout is present.
///
/// This is chassis step 2's *"select points and move them"*: moving a top point
/// slants the side, and the embedded mesh's top ring is the same point. The lab's own step 2
/// does not use it any more: the drag is `Build::adjust_top_point`, which judges the move as the
/// original does (the editing area's rules, the bend test) and rebuilds the extrusion (docs/36
/// section 9.27e).
pub fn move_outline_point(
    ch: &mut Chassis,
    which: Outline,
    index: usize,
    point: [i32; 2],
    snap_to_grid: bool,
) -> Result<(), ChassisError> {
    let point = clamp(if snap_to_grid { snap(point) } else { point });
    {
        let points = outline(ch, which);
        if index >= points.len() {
            return Err(ChassisError::NoPoint {
                index,
                count: points.len(),
            });
        }
        if too_close(points, point, Some(index)) {
            return Err(ChassisError::TooClose { unit: GRID_UNIT });
        }
        let mut next = points.to_vec();
        next[index] = point;
        if next.len() >= 4 && self_intersects(&next) {
            return Err(ChassisError::SelfIntersecting);
        }
    }
    let ring = ring_index(ch, which, index);
    outline_mut(ch, which)[index] = point;
    if let (Some(at), Some(mesh)) = (ring, ch.mesh.as_mut()) {
        let y = match which {
            Outline::Bottom => 0.0,
            Outline::Top => ch.height,
        };
        mesh.points[at] = world_point(point, y);
    }
    Ok(())
}

/// Set the extrusion height, scaling the existing mesh's `y` (the bottom ring
/// stays at 0) and its buckle points. The outlines are unchanged.
pub fn set_height(ch: &mut Chassis, height: f32) -> Result<(), ChassisError> {
    if !height.is_finite() || height <= 0.0 {
        return Err(ChassisError::BadHeight(height));
    }
    let old = ch.height;
    if old <= 0.0 || (old - height).abs() < f32::EPSILON {
        ch.height = height;
        return Ok(());
    }
    let ratio = height / old;
    ch.height = height;
    if let Some(mesh) = ch.mesh.as_mut() {
        for p in &mut mesh.points {
            p[1] *= ratio;
        }
    }
    for point in &mut ch.buckling {
        point.position[1] *= ratio;
    }
    Ok(())
}

/// Add a mesh point. `uv` is in the mesh's `0..1` convention; the face group
/// copies it as skin pixels when a face first uses the point.
pub fn add_mesh_point(
    ch: &mut Chassis,
    point: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
) -> Result<usize, ChassisError> {
    let Some(mesh) = ch.mesh.as_mut() else {
        return Err(ChassisError::NoMesh);
    };
    let index = mesh.points.len();
    mesh.points.push(point);
    mesh.normals.push(normal);
    mesh.uvs.push(uv);
    Ok(index)
}

/// Remove a mesh point, the triangles that use it, and the face rows that name
/// it. Every later index shifts down by one. The group coordinates of the
/// remaining points are preserved.
pub fn remove_mesh_point(ch: &mut Chassis, index: usize) -> Result<[f32; 3], ChassisError> {
    let Some(points) = ch.mesh.as_ref().map(|m| m.points.len()) else {
        return Err(ChassisError::NoMesh);
    };
    if index >= points {
        return Err(ChassisError::NoVertex {
            index,
            count: points,
        });
    }
    let old_coords: Vec<BTreeMap<u32, [f32; 2]>> = coords_by_point(ch)
        .into_iter()
        .map(|map| {
            map.into_iter()
                .filter_map(|(i, coord)| {
                    if i as usize == index {
                        return None;
                    }
                    Some((if i as usize > index { i - 1 } else { i }, coord))
                })
                .collect()
        })
        .collect();
    let mesh = ch.mesh.as_mut().expect("checked above");
    let removed = mesh.points.remove(index);
    if index < mesh.normals.len() {
        mesh.normals.remove(index);
    }
    if index < mesh.uvs.len() {
        mesh.uvs.remove(index);
    }
    let mut indices = Vec::with_capacity(mesh.indices.len());
    for tri in mesh.indices.as_chunks::<3>().0 {
        if tri.iter().any(|i| *i as usize == index) {
            continue;
        }
        for i in tri {
            indices.push(if *i as usize > index { *i - 1 } else { *i });
        }
    }
    mesh.indices = indices;

    let mut faces = Vec::with_capacity(ch.faces.len());
    for face in &ch.faces {
        if face.points.iter().any(|p| *p as usize == index) {
            continue;
        }
        let mut face = *face;
        for p in &mut face.points {
            if *p as usize > index {
                *p -= 1;
            }
        }
        faces.push(face);
    }
    ch.faces = faces;
    rebuild_all_groups(ch, &old_coords);
    ch.face_count = ch
        .mesh
        .as_ref()
        .map(|m| m.indices.len() as i32 / 3)
        .unwrap_or(0);
    Ok(removed)
}

/// Add a triangle to a face group. `points` are mesh indices in winding order.
///
/// The group's `coords` list is kept in the renderer's order — one coordinate
/// per unique point, ascending by point index. A point new to the group takes
/// the mesh point's own UV.
pub fn add_face(ch: &mut Chassis, group: usize, points: [u32; 3]) -> Result<usize, ChassisError> {
    if group >= ch.face_groups.len() {
        return Err(ChassisError::NoGroup {
            index: group,
            count: ch.face_groups.len(),
        });
    }
    let Some(mesh) = ch.mesh.as_ref() else {
        return Err(ChassisError::NoMesh);
    };
    for p in points {
        if p as usize >= mesh.points.len() {
            return Err(ChassisError::NoVertex {
                index: p as usize,
                count: mesh.points.len(),
            });
        }
    }
    if points[0] == points[1] || points[1] == points[2] || points[0] == points[2] {
        return Err(ChassisError::DegenerateFace);
    }
    let old_coords = coords_by_point(ch);
    let index = ch.faces.len();
    ch.faces.push(Face {
        group: group as i32,
        points,
    });
    let mesh = ch.mesh.as_mut().expect("checked above");
    mesh.indices.extend_from_slice(&points);
    rebuild_group(ch, group, &old_coords);
    ch.face_count = ch
        .mesh
        .as_ref()
        .map(|m| m.indices.len() as i32 / 3)
        .unwrap_or(0);
    Ok(index)
}

/// Remove a face row and the triangle that names the same three points.
pub fn remove_face(ch: &mut Chassis, index: usize) -> Result<Face, ChassisError> {
    let Some(face) = ch.faces.get(index).copied() else {
        return Err(ChassisError::NoFace {
            index,
            count: ch.faces.len(),
        });
    };
    let old_coords = coords_by_point(ch);
    ch.faces.remove(index);
    if let Some(mesh) = ch.mesh.as_mut() {
        if let Some(tri) = mesh
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .position(|tri| tri == &face.points)
        {
            mesh.indices.drain(tri * 3..tri * 3 + 3);
        }
    }
    rebuild_group(ch, face.group.max(0) as usize, &old_coords);
    ch.face_count = ch
        .mesh
        .as_ref()
        .map(|m| m.indices.len() as i32 / 3)
        .unwrap_or(0);
    Ok(face)
}

fn coords_by_point(ch: &Chassis) -> Vec<BTreeMap<u32, [f32; 2]>> {
    let mut out = Vec::with_capacity(ch.face_groups.len());
    for (gi, group) in ch.face_groups.iter().enumerate() {
        let seen = group_points(ch, gi);
        out.push(
            seen.iter()
                .copied()
                .zip(group.coords.iter().copied())
                .collect(),
        );
    }
    out
}

fn group_points(ch: &Chassis, group: usize) -> Vec<u32> {
    let mut seen = Vec::new();
    for face in ch.faces.iter().filter(|f| f.group == group as i32) {
        for p in face.points {
            if !seen.contains(&p) {
                seen.push(p);
            }
        }
    }
    seen.sort_unstable();
    seen
}

fn rebuild_all_groups(ch: &mut Chassis, old: &[BTreeMap<u32, [f32; 2]>]) {
    for group in 0..ch.face_groups.len() {
        let previous = old.get(group).cloned().unwrap_or_default();
        rebuild_group(ch, group, &[previous]);
    }
}

fn rebuild_group(ch: &mut Chassis, group: usize, old: &[BTreeMap<u32, [f32; 2]>]) {
    if group >= ch.face_groups.len() {
        return;
    }
    let previous = old.get(group).cloned().unwrap_or_default();
    let seen = group_points(ch, group);
    let uvs: Vec<[f32; 2]> = ch.mesh.as_ref().map(|m| m.uvs.clone()).unwrap_or_default();
    let coords = seen
        .iter()
        .map(|i| {
            previous.get(i).copied().unwrap_or_else(|| {
                uvs.get(*i as usize)
                    .map(|uv| [uv[0] * 256.0, uv[1] * 256.0])
                    .unwrap_or([0.0, 0.0])
            })
        })
        .collect();
    ch.face_groups[group].coords = coords;
}

/// The skin coordinates of a triangle's three vertices in one group, or `None`
/// when the group does not own all three.
///
/// This is the renderer's own lookup: one coordinate per unique point, sorted
/// by point index (`ra2_render::chassis_mesh_vertices`).
pub fn group_uvs(ch: &Chassis, group: usize, points: [u32; 3]) -> Option<[[f32; 2]; 3]> {
    let fg = ch.face_groups.get(group)?;
    let seen = group_points(ch, group);
    let mut out = [[0.0; 2]; 3];
    for (slot, point) in points.iter().enumerate() {
        let rank = seen.iter().position(|p| p == point)?;
        out[slot] = *fg.coords.get(rank)?;
    }
    Some(out)
}

/// The mesh's ring layout matches the corpus' extrusion: `bottom` then `top`
/// share the point table, and the side quads follow.
pub fn rings_match(ch: &Chassis) -> bool {
    let Some(mesh) = ch.mesh.as_ref() else {
        return false;
    };
    let n = ch.bottom.len();
    let t = ch.top.len();
    if mesh.points.len() < n + t || n == 0 || t == 0 {
        return false;
    }
    ch.bottom.iter().enumerate().all(|(i, p)| {
        let want = world_point(*p, 0.0);
        (0..3).all(|k| (want[k] - mesh.points[i][k]).abs() < 1e-3)
    }) && ch.top.iter().enumerate().all(|(i, p)| {
        let want = world_point(*p, ch.height);
        (0..3).all(|k| (want[k] - mesh.points[n + i][k]).abs() < 1e-3)
    })
}

/// The outline's points as bot-local metres, for a 2D panel or a test.
pub fn outline_world(points: &[[i32; 2]], y: f32) -> Vec<[f32; 3]> {
    points
        .iter()
        .map(|p| {
            let w = baseplate_to_world([p[0] as f32, p[1] as f32]);
            [w[0], y, w[1]]
        })
        .collect()
}

/// A point's outline coordinate, for a 2D panel.
pub fn point_to_outline(world: [f32; 2]) -> [i32; 2] {
    let p = world_to_baseplate(world);
    [p[0].round() as i32, p[1].round() as i32]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2fmt::bot::{Armour, Bot, BotMesh, Component, FaceGroup};

    fn mesh() -> BotMesh {
        BotMesh {
            points: vec![
                [-0.5, 0.0, -0.5],
                [0.5, 0.0, -0.5],
                [0.5, 0.0, 0.5],
                [-0.5, 0.0, 0.5],
            ],
            normals: vec![[0.0, 1.0, 0.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }

    fn chassis() -> Chassis {
        Chassis {
            height: 0.5,
            face_count: 2,
            mesh: Some(mesh()),
            bottom: vec![[0, 0], [1000, 0], [1000, 1000], [0, 1000]],
            top: vec![[0, 0], [1000, 0], [1000, 1000], [0, 1000]],
            face_groups: vec![FaceGroup {
                coords: vec![[0.0, 0.0], [256.0, 0.0], [256.0, 256.0], [0.0, 256.0]],
                bounds: [0, 0, 256, 256],
            }],
            faces: vec![
                Face {
                    group: 0,
                    points: [0, 1, 2],
                },
                Face {
                    group: 0,
                    points: [0, 2, 3],
                },
            ],
            armour: Some(Armour::default()),
            ..Default::default()
        }
    }

    fn bot(chassis: Chassis) -> Bot {
        Bot {
            name: "Drawn".into(),
            components: vec![Component {
                parent: -1,
                class: "Chassis".into(),
                rotation: [0.0, 0.0, 0.0, 1.0],
                chassis: Some(chassis),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn game_root() -> Option<std::path::PathBuf> {
        if let Some(dir) = std::env::var_os("RA2_GAME") {
            let dir = std::path::PathBuf::from(dir);
            return dir.join("Components").is_dir().then_some(dir);
        }
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        while dir.pop() {
            let candidate = dir.join("local/game");
            if candidate.join("Components").is_dir() {
                return Some(candidate);
            }
        }
        None
    }

    fn bots(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                bots(&path, out);
            } else if path.extension().is_some_and(|e| e == "bot") {
                out.push(path);
            }
        }
    }

    #[test]
    fn outline_points_snap_to_the_blueprint_grid_and_follow_the_tutorial_rules() {
        let mut ch = chassis();
        ch.bottom.clear();
        assert_eq!(snap([131, 962]), [100, 1000]);
        add_outline_point(&mut ch, Outline::Bottom, [120, 120], true).unwrap();
        add_outline_point(&mut ch, Outline::Bottom, [420, 120], true).unwrap();
        add_outline_point(&mut ch, Outline::Bottom, [420, 420], true).unwrap();
        assert_eq!(ch.bottom, vec![[100, 100], [400, 100], [400, 400]]);
        // Closer than one grid unit: refused.
        assert_eq!(
            add_outline_point(&mut ch, Outline::Bottom, [440, 120], true),
            Err(ChassisError::TooClose { unit: GRID_UNIT })
        );
        // A bow tie: the fourth point makes the closing edges cross.
        let mut bow = Chassis::default();
        for p in [[0, 0], [1000, 0], [0, 1000]] {
            add_outline_point(&mut bow, Outline::Bottom, p, false).unwrap();
        }
        assert_eq!(
            add_outline_point(&mut bow, Outline::Bottom, [1000, 1000], false),
            Err(ChassisError::SelfIntersecting)
        );
        // The tutorial's 16-point ceiling.
        let mut many = Chassis {
            bottom: (0..MAX_OUTLINE_POINTS)
                .map(|i| {
                    let a = std::f32::consts::TAU * i as f32 / MAX_OUTLINE_POINTS as f32;
                    [
                        (500.0 + 300.0 * a.cos()).round() as i32,
                        (500.0 + 300.0 * a.sin()).round() as i32,
                    ]
                })
                .collect(),
            ..Default::default()
        };
        assert_eq!(
            add_outline_point(&mut many, Outline::Bottom, [500, 0], false),
            Err(ChassisError::TooManyPoints)
        );
        assert!(validate_outline(&[[0, 0], [1000, 0], [0, 1000]]).is_ok());
        assert_eq!(
            validate_outline(&[[0, 0], [1000, 0]]),
            Err(ChassisError::TooFewPoints)
        );
    }

    #[test]
    fn the_circle_and_rectangle_tools_write_outlines_the_writer_keeps() {
        let mut ch = chassis();
        set_circle(&mut ch, Outline::Top, [500, 500], 300);
        assert_eq!(ch.top.len(), 12);
        set_rectangle(&mut ch, Outline::Bottom, [500, 500], 600, 400);
        assert_eq!(
            ch.bottom,
            vec![[200, 300], [800, 300], [800, 700], [200, 700]]
        );
        let bytes = bot(ch.clone()).write();
        let back = ra2fmt::bot::parse(&bytes).unwrap();
        let drawn = back.components[0].chassis.as_ref().unwrap();
        assert_eq!(drawn.bottom, ch.bottom);
        assert_eq!(drawn.top, ch.top);
        assert_eq!(back.write(), bytes);
    }

    #[test]
    fn a_mesh_point_and_face_round_trip_and_keep_the_group_coordinates_in_order() {
        let mut ch = chassis();
        let p = add_mesh_point(&mut ch, [0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.5, 0.5]).unwrap();
        assert_eq!(p, 4);
        let f = add_face(&mut ch, 0, [0, 1, 4]).unwrap();
        assert_eq!(ch.face_count, 3);
        assert_eq!(ch.mesh.as_ref().unwrap().indices.len(), 9);
        assert_eq!(ch.face_groups[0].coords.len(), 5);
        assert_eq!(ch.face_groups[0].coords[4], [128.0, 128.0]);
        remove_face(&mut ch, f).unwrap();
        assert_eq!(ch.face_count, 2);
        assert_eq!(ch.face_groups[0].coords.len(), 4);
        let removed = remove_mesh_point(&mut ch, 4).unwrap();
        assert_eq!(removed, [0.0, 0.0, 0.0]);
        assert_eq!(ch.mesh.as_ref().unwrap().points.len(), 4);

        let bytes = bot(ch).write();
        let back = ra2fmt::bot::parse(&bytes).unwrap();
        let written = back.components[0].chassis.as_ref().unwrap();
        assert_eq!(written.face_count, 2);
        assert_eq!(written.faces, chassis().faces);
    }

    #[test]
    fn removing_a_shared_mesh_point_removes_its_faces_and_shifts_the_rest() {
        let mut ch = chassis();
        remove_mesh_point(&mut ch, 1).unwrap();
        let m = ch.mesh.as_ref().unwrap();
        assert_eq!(m.points.len(), 3);
        assert_eq!(m.indices, vec![0, 1, 2]);
        assert_eq!(ch.faces.len(), 1);
        assert_eq!(ch.faces[0].points, [0, 1, 2]);
        assert_eq!(ch.face_count, 1);
        assert_eq!(ch.face_groups[0].coords.len(), 3);
        // The coordinates of the points above the removed index keep their
        // skin positions, not the removed point's.
        assert_eq!(
            ch.face_groups[0].coords,
            vec![[0.0, 0.0], [256.0, 256.0], [0.0, 256.0]]
        );
    }

    #[test]
    fn the_height_slider_scales_the_extrusion_and_keeps_the_outlines() {
        let mut ch = chassis();
        set_height(&mut ch, 1.0).unwrap();
        assert_eq!(ch.height, 1.0);
        assert!(ch
            .mesh
            .as_ref()
            .unwrap()
            .points
            .iter()
            .all(|p| p[1] == 0.0 || p[1] == 1.0));
        /* the synthetic mesh is flat; use a corpus-shaped ring below */
        let mut corpus = chassis();
        corpus.mesh.as_mut().unwrap().points[2][1] = 0.5;
        corpus.mesh.as_mut().unwrap().points[3][1] = 0.5;
        set_height(&mut corpus, 0.25).unwrap();
        assert_eq!(corpus.mesh.as_ref().unwrap().points[2][1], 0.25);
        assert_eq!(corpus.height, 0.25);
        assert_eq!(
            set_height(&mut corpus, 0.0),
            Err(ChassisError::BadHeight(0.0))
        );
    }

    /// All 57 shipped chassis obey the ring rule the outline move relies on.
    #[test]
    fn every_shipped_chassis_mesh_ring_is_its_own_outline() {
        let Some(root) = game_root() else { return };
        let mut files = Vec::new();
        for sub in ["Robot Designs", "AI", "Teams"] {
            let dir = root.join(sub);
            if dir.is_dir() {
                bots(&dir, &mut files);
            }
        }
        files.sort();
        assert_eq!(files.len(), 57);
        let mut points = 0usize;
        for path in &files {
            let data = std::fs::read(path).unwrap();
            let bot = ra2fmt::bot::parse(&data).unwrap();
            let ch = bot.components[0].chassis.as_ref().unwrap();
            assert!(rings_match(ch), "{}", path.display());
            points += ch.mesh.as_ref().map(|m| m.points.len()).unwrap_or(0);
        }
        eprintln!(
            "{} chassis: every bottom/top ring is its outline ({points} mesh points)",
            files.len()
        );
    }

    /// The corpus' ring layout: the two outlines are the first mesh points, so
    /// step 2's point move moves the mesh vertex with the same coordinate.
    #[test]
    fn moving_a_top_point_moves_the_mesh_ring_vertex() {
        let mut ch = chassis();
        ch.height = 0.5;
        // Build the corpus layout: bottom at y=0, top at y=height.
        let mut m = mesh();
        m.points = vec![
            [-1.1, 0.0, 1.1],
            [1.1, 0.0, 1.1],
            [1.1, 0.0, -1.1],
            [-1.1, 0.0, -1.1],
            [-1.1, 0.5, 1.1],
            [1.1, 0.5, 1.1],
            [1.1, 0.5, -1.1],
            [-1.1, 0.5, -1.1],
        ];
        m.normals = vec![[0.0, 1.0, 0.0]; 8];
        m.uvs = vec![[0.0, 0.0]; 8];
        m.indices = vec![0, 1, 2, 4, 5, 6];
        ch.mesh = Some(m);
        assert!(rings_match(&ch));
        let at = ring_index(&ch, Outline::Top, 2).unwrap();
        move_outline_point(&mut ch, Outline::Top, 2, [600, 600], false).unwrap();
        let p = ch.mesh.as_ref().unwrap().points[at];
        assert!((p[0] - 0.22).abs() < 1e-5);
        assert!((p[1] - 0.5).abs() < 1e-5);
        assert!((p[2] + 0.22).abs() < 1e-5);
    }
}
