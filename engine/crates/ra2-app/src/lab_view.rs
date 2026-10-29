//! What the original draws into the Bot Lab's 3D view besides the robot (live `DrawPrimitive`
//! dumps of the Components and Wiring tabs, docs/36 section 9.27): the chassis' face outlines as
//! thin grey lines, and for the selected control a floating icon, a blue stem and one green wire
//! per bound part. Every line is a camera-facing ribbon of constant pixel width.

use std::collections::HashMap;
use std::path::Path;

use glam::{Mat4, Vec3};
use ra2_render::material::{load_image, Material};
use ra2_render::{DrawItem, Vertex};

/// The chassis outline lines: `(130, 130, 130)`, 2 px wide.
pub const EDGE_COLOR: [f32; 4] = [130.0 / 255.0, 130.0 / 255.0, 130.0 / 255.0, 1.0];
pub const EDGE_PIXELS: f32 = 2.0;
/// The control wires and the stem: pure green and blue, 4 px wide.
pub const WIRE_COLOR: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
pub const STEM_COLOR: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
pub const WIRE_PIXELS: f32 = 4.0;
/// Where a control's icon floats (x and z of the robot, y above it) and its size, as drawn live.
pub const ICON_Y: f32 = 1.6;
pub const WIRE_Y: f32 = 1.0;
/// The wires end just above the baseplate (live they end on it; a hair of height keeps them out
/// of its depth).
pub const FLOOR_Y: f32 = 0.004;
pub const ICON_SIZE: [f32; 2] = [0.262, 0.23];

/// The camera of a lab frame, for ribbons.
#[derive(Clone, Copy, Debug)]
pub struct Cam {
    pub eye: Vec3,
    pub fwd: Vec3,
    /// World size of one pixel at one unit of depth.
    pub per_pixel: f32,
}

impl Cam {
    pub fn new(eye: Vec3, target: Vec3, fov_y: f32, height_px: f32) -> Cam {
        Cam {
            eye,
            fwd: (target - eye).normalize_or_zero(),
            per_pixel: 2.0 * (fov_y * 0.5).tan() / height_px.max(1.0),
        }
    }

    fn depth(&self, p: Vec3) -> f32 {
        (p - self.eye).dot(self.fwd)
    }
}

fn flat_material() -> Material {
    Material {
        lighting: false,
        has_vertex_colors: true,
        cull: false,
        ..Material::default()
    }
}

fn vertex(pos: Vec3, color: [f32; 4], uv: [f32; 2]) -> Vertex {
    let mut v = Vertex::new(pos, Vec3::Z, uv);
    v.color = color;
    v
}

/// The triangles of a ribbon along the segments `a -> b`, `pixels` wide on screen.
pub fn ribbon(cam: &Cam, segments: &[(Vec3, Vec3)], color: [f32; 4], pixels: f32) -> Vec<Vertex> {
    let mut out = Vec::new();
    for &(a, b) in segments {
        let dir = (b - a).normalize_or_zero();
        if dir == Vec3::ZERO {
            continue;
        }
        let side = dir.cross(cam.fwd).normalize_or_zero();
        if side == Vec3::ZERO {
            continue;
        }
        let half = |p: Vec3| side * (cam.depth(p).max(0.0) * cam.per_pixel * pixels * 0.5);
        // Square caps: each end runs on by half a width, so two ribbons meeting at a corner
        // overlap instead of leaving a notch.
        let ea = a - dir * (cam.depth(a).max(0.0) * cam.per_pixel * pixels * 0.5);
        let eb = b + dir * (cam.depth(b).max(0.0) * cam.per_pixel * pixels * 0.5);
        let (ha, hb) = (half(a), half(b));
        let q = [ea + ha, ea - ha, eb + hb, eb - hb];
        for i in [0usize, 1, 2, 2, 1, 3] {
            out.push(vertex(q[i], color, [0.0, 0.0]));
        }
    }
    out
}

/// The outline edges of a mesh's faces, in model space: an edge belongs to the outline when only
/// one triangle owns it or the triangles on its two sides differ in normal (a quad's own
/// diagonal does not show).
pub fn outline_edges(items: &[DrawItem]) -> Vec<(Vec3, Vec3)> {
    type Key = [i64; 3];
    let key = |p: Vec3| -> Key {
        [
            (p.x * 1000.0).round() as i64,
            (p.y * 1000.0).round() as i64,
            (p.z * 1000.0).round() as i64,
        ]
    };
    // edge (ordered pair of keys) -> (points, normals seen)
    let mut edges: HashMap<(Key, Key), (Vec3, Vec3, Vec<Vec3>)> = HashMap::new();
    for item in items {
        for tri in item.vertices.as_chunks::<3>().0.iter() {
            let p: Vec<Vec3> = tri.iter().map(|v| Vec3::from(v.pos)).collect();
            let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (ka, kb) = (key(p[i]), key(p[j]));
                if ka == kb {
                    continue;
                }
                let (k, a, b) = if ka < kb {
                    ((ka, kb), p[i], p[j])
                } else {
                    ((kb, ka), p[j], p[i])
                };
                edges.entry(k).or_insert((a, b, Vec::new())).2.push(n);
            }
        }
    }
    let mut keys: Vec<_> = edges.keys().copied().collect();
    keys.sort();
    let mut out = Vec::new();
    for k in keys {
        let (a, b, normals) = &edges[&k];
        // Parallel normals (either side up: a face seen from behind is the same plane).
        let first = normals[0];
        let flat = normals.len() >= 2 && normals.iter().all(|n| n.dot(first).abs() > 0.9999);
        if !flat {
            out.push((*a, *b));
        }
    }
    out
}

/// The baseplate the Components and Wiring tabs draw under the ghost chassis: the bottom face's
/// triangles on the floor, normal up, texture coordinates 8 per metre, the shipped
/// `baseplate_grid.bmp`, diffuse 0.5, no culling (live `DrawIndexedPrimitive`: 8 vertices, 6
/// triangles on the `C.R.U.S.H.` chassis).
pub fn baseplate(items: &[DrawItem], grid: &Path) -> Option<DrawItem> {
    let mut vertices = Vec::new();
    for item in items {
        for tri in item.vertices.as_chunks::<3>().0.iter() {
            if tri
                .iter()
                .all(|v| v.pos[1].abs() < 1e-4 && v.nrm[1] < -0.99)
            {
                for v in tri {
                    let mut w = *v;
                    w.nrm = [0.0, 1.0, 0.0];
                    w.uv = [v.pos[0] * 8.0, v.pos[2] * 8.0];
                    w.uv2 = w.uv;
                    w.color = [1.0; 4];
                    vertices.push(w);
                }
            }
        }
    }
    if vertices.is_empty() {
        return None;
    }
    let mut material = match load_image(grid) {
        Some(image) => Material::textured(image),
        None => Material::default(),
    };
    material.cull = false;
    material.diffuse = [0.5, 0.5, 0.5, 1.0];
    material.early = true;
    Some(DrawItem::new(vertices, material, "lab baseplate"))
}

/// One control's graphics: the icon quad, the stem and a wire to each bound part's `(x, z)`.
pub struct WireScene {
    pub icon: DrawItem,
    pub lines: DrawItem,
}

/// `anchor` is the `(x, z)` the icon floats over; `parts` are the `(x, z)` of the bound parts.
pub fn wire_scene(cam: &Cam, anchor: [f32; 2], parts: &[[f32; 2]], icon_file: &Path) -> WireScene {
    let [ax, az] = anchor;
    let mut vertices = Vec::new();
    let stem = [(Vec3::new(ax, FLOOR_Y, az), Vec3::new(ax, WIRE_Y, az))];
    vertices.extend(ribbon(cam, &stem, STEM_COLOR, WIRE_PIXELS));
    for &[px, pz] in parts {
        let path = [
            (Vec3::new(ax, ICON_Y, az), Vec3::new(ax, WIRE_Y, az)),
            (Vec3::new(ax, WIRE_Y, az), Vec3::new(px, WIRE_Y, pz)),
            (Vec3::new(px, WIRE_Y, pz), Vec3::new(px, FLOOR_Y, pz)),
        ];
        vertices.extend(ribbon(cam, &path, WIRE_COLOR, WIRE_PIXELS));
    }
    let lines = DrawItem::new(vertices, flat_material(), "lab wires");
    let image = load_image(icon_file);
    let mut material = flat_material();
    if let Some(image) = image {
        material = Material::textured(image);
        material.lighting = false;
        material.cull = false;
    }
    let (hw, hh) = (ICON_SIZE[0] * 0.5, ICON_SIZE[1] * 0.5);
    // Seen from the front (the camera looks along +z at the start), image up is +y.
    let corner = |sx: f32, sy: f32, u: f32, v: f32| {
        vertex(
            Vec3::new(ax + sx * hw, ICON_Y + sy * hh, az),
            [1.0; 4],
            [u, v],
        )
    };
    let tl = corner(-1.0, 1.0, 0.0, 0.0);
    let tr = corner(1.0, 1.0, 1.0, 0.0);
    let bl = corner(-1.0, -1.0, 0.0, 1.0);
    let br = corner(1.0, -1.0, 1.0, 1.0);
    let icon = DrawItem::new(vec![tl, tr, bl, bl, tr, br], material, "lab control icon");
    WireScene { icon, lines }
}

/// The world position of a model matrix.
pub fn origin_of(m: Mat4) -> Vec3 {
    m.transform_point3(Vec3::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Vec<Vertex> {
        [a, b, c]
            .iter()
            .map(|p| Vertex::new(Vec3::from(*p), Vec3::Y, [0.0; 2]))
            .collect()
    }

    #[test]
    fn a_quads_diagonal_is_not_an_outline_edge() {
        let mut v = tri([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        v.extend(tri([1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]));
        let item = DrawItem::new(v, Material::default(), "q");
        let edges = outline_edges(&[item]);
        assert_eq!(edges.len(), 4, "{edges:?}");
    }

    #[test]
    fn ribbons_are_constant_in_pixels() {
        let cam = Cam::new(
            Vec3::new(0.0, 0.0, -5.0),
            Vec3::ZERO,
            45f32.to_radians(),
            471.0,
        );
        // The live vertical wire at z = -0.354: 0.032 wide; at z = 0.391 the horizontal is 0.038.
        let near = ribbon(
            &cam,
            &[(Vec3::new(0.412, 1.6, -0.354), Vec3::new(0.412, 1.0, -0.354))],
            WIRE_COLOR,
            WIRE_PIXELS,
        );
        let w = (near[0].pos[0] - near[1].pos[0]).abs();
        assert!((w - 0.032).abs() < 0.002, "{w}");
    }
}
