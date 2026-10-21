//! Battle marks on a chassis: slashes, dents and punctures (`DecalTrigger`,
//! RTTI `0x6ef5e4`; docs/37-effects.md).
//!
//! A weapon whose definition says `decal = <file> <dx dy dz> <w> <h> <mode>` leaves a
//! mark on the chassis it hits. The chassis' hit handler (`0x44ee70`, the
//! `Chassis` override of the component hit effects) builds one `DecalTrigger`
//! (`0x1cf4` bytes, constructor `0x5a0e00`) per contact:
//!
//! * the **box**: centre `c` = the contact point in the chassis' own frame,
//!   depth axis `N` = the contact normal turned to point out of the chassis, width
//!   axis `A` = the weapon's decal direction `(dx, dy, dz)` carried by the weapon
//!   body's orientation into the chassis frame, height axis `V = N x A`; the half
//!   extents are `w / 2`, `h / 2` and `0.5` (`0x6f20c0`). Six planes, in the order
//!   `A, -A, V, -V, N, -N` (`FUN_005a1970`);
//! * every triangle of the chassis' first mesh whose unit face normal has `n . N >
//!   0.25` (`0x705f48`) is pushed `0.0005` (`0x6f20c4`) along its normal and
//!   clipped to the box (`FUN_005a1700`, `FUN_005a1a80`, Sutherland-Hodgman);
//!   a polygon that would take the decal past 89 vertices ends the walk
//!   (`FUN_005a14f0`);
//! * a clipped point keeps the indices of its source triangle and its barycentric
//!   weights (the three sub-areas over the triangle's area, so a point that was
//!   pushed off the face sums to a little over one), and the draw (`FUN_005a1ee0`)
//!   re-evaluates the position from the mesh as it is *now*, so a decal follows a
//!   chassis that buckles;
//! * texture coordinates `u = (p - c) . A / w + 0.5`, `v = (p - c) . V / h + 0.5`
//!   (not renormalised, `A` is not perpendicular to `N`);
//! * the polygons are triangle fans (`0, i - 1, i`), diffuse `(1, 1, 1, alpha)` with
//!   `alpha = clamp(damage / 100, 0, 1)` (`FUN_004194f0` over `0..100 -> 0..1`).
//!
//! A chassis keeps at most 25 decals (`0x6f20c8`): the oldest goes first.
//!
//! Draw state (`FUN_005a2240`, material `0x71fd78`): alpha blending on, depth write
//! off, lighting on, back-face culling; the weapon's mode byte (`+0xac`, the last
//! number of the definition's `decal` line) chooses `SRCALPHA, ONE` with an
//! alpha test (`>= 16`) for `1` (slashes) and `SRCALPHA, INVSRCALPHA` without one
//! for anything else (dents, punctures).

use std::path::Path;
use std::sync::Arc;

use glam::Vec3;

use crate::material::{load_image, Image, Material, Stage, StageKind};
use crate::{DrawItem, Vertex};

/// Decals a chassis keeps (`DAT_006f20c8`).
pub const MAX_DECALS: usize = 25;
/// The decal's half depth along the contact normal (`DAT_006f20c0`).
pub const DEPTH: f32 = 0.5;
/// How far a decal rides above the face it was cut from (`DAT_006f20c4`).
pub const BIAS: f32 = 0.0005;
/// `cos` limit between a face and the contact normal (`DAT_00705f48`).
pub const FACING: f32 = 0.25;
/// The most clipped points one decal holds (`FUN_005a14f0`: `0x59`).
pub const MAX_POINTS: usize = 89;

/// What a weapon's `decal =` line says (`decal = single_slash.tga 0 0 1 .5 .5 1`).
#[derive(Clone, Debug, PartialEq)]
pub struct DecalSpec {
    /// The file under `Arenas/decals/`.
    pub file: String,
    /// Direction in the weapon's frame (the width axis).
    pub direction: Vec3,
    pub width: f32,
    pub height: f32,
    /// `1` slash (additive, alpha tested), anything else alpha blended.
    pub mode: i32,
}

impl DecalSpec {
    /// Parse the value of a definition's `decal` field; `None` when it has no
    /// file name.
    pub fn parse(value: &str) -> Option<DecalSpec> {
        let mut it = value.split_whitespace();
        let file = it.next()?.to_string();
        let mut num = |default: f32| {
            it.next()
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(default)
        };
        let direction = Vec3::new(num(0.0), num(0.0), num(0.0));
        let width = num(0.5);
        let height = num(0.5);
        let mode = num(0.0) as i32;
        Some(DecalSpec {
            file,
            direction,
            width,
            height,
            mode,
        })
    }

    /// Whether the mark is drawn additively (the slashes).
    pub fn additive(&self) -> bool {
        self.mode == 1
    }
}

/// The box a decal is cut with, in the chassis' frame.
#[derive(Clone, Copy, Debug)]
pub struct Projection {
    pub center: Vec3,
    /// The depth axis (out of the chassis).
    pub normal: Vec3,
    /// The width axis.
    pub axis: Vec3,
    pub width: f32,
    pub height: f32,
}

/// One clipped point: where it sits on the mesh and where it samples the image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecalPoint {
    /// The source triangle (index into the list the decal was cut from).
    pub triangle: u32,
    pub weights: [f32; 3],
    pub uv: [f32; 2],
}

/// One mark on a chassis.
#[derive(Clone, Debug)]
pub struct Decal {
    pub spec: DecalSpec,
    pub alpha: f32,
    pub points: Vec<DecalPoint>,
    /// Triangle fans over `points`.
    pub triangles: Vec<[u16; 3]>,
}

fn area(a: Vec3, b: Vec3, c: Vec3) -> f32 {
    (b - a).cross(c - a).length() * 0.5
}

/// Sutherland-Hodgman against one plane `n . p + d >= 0` (`FUN_005a1a80`).
fn clip(poly: &[Vec3], n: Vec3, d: f32) -> Vec<Vec3> {
    let dist: Vec<f32> = poly.iter().map(|p| n.dot(*p) + d).collect();
    if dist.iter().all(|&x| x < 0.0) {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let prev = if i == 0 { poly.len() - 1 } else { i - 1 };
        if dist[i] >= 0.0 {
            if dist[prev] < 0.0 {
                let t = dist[i] / (dist[i] - dist[prev]);
                out.push(poly[i] * (1.0 - t) + poly[prev] * t);
            }
            out.push(poly[i]);
        } else if dist[prev] >= 0.0 {
            let t = dist[prev] / (dist[prev] - dist[i]);
            out.push(poly[prev] * (1.0 - t) + poly[i] * t);
        }
    }
    out
}

/// Cut a decal out of `triangles` (the chassis mesh's triangles, corners in the
/// order of its index list, in the chassis' frame). `alpha` is the diffuse alpha.
pub fn project(
    spec: &DecalSpec,
    alpha: f32,
    p: &Projection,
    triangles: impl IntoIterator<Item = [Vec3; 3]>,
) -> Decal {
    let (c, n, a) = (p.center, p.normal, p.axis);
    let v = n.cross(a);
    let planes = [
        (a, p.width * 0.5 - c.dot(a)),
        (-a, p.width * 0.5 + c.dot(a)),
        (v, p.height * 0.5 - c.dot(v)),
        (-v, p.height * 0.5 + c.dot(v)),
        (n, DEPTH - c.dot(n)),
        (-n, DEPTH + c.dot(n)),
    ];
    let mut decal = Decal {
        spec: spec.clone(),
        alpha,
        points: Vec::new(),
        triangles: Vec::new(),
    };
    for (t, tri) in triangles.into_iter().enumerate() {
        let face = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero();
        if face == Vec3::ZERO || face.dot(n) <= FACING {
            continue;
        }
        let mut poly: Vec<Vec3> = tri.iter().map(|q| *q + face * BIAS).collect();
        for (pn, pd) in planes {
            poly = clip(&poly, pn, pd);
            if poly.is_empty() {
                break;
            }
        }
        if poly.is_empty() {
            continue;
        }
        if decal.points.len() + poly.len() > MAX_POINTS {
            break;
        }
        let whole = area(tri[0], tri[1], tri[2]);
        let first = decal.points.len() as u16;
        for q in &poly {
            let weights = [
                area(*q, tri[1], tri[2]) / whole,
                area(tri[0], *q, tri[2]) / whole,
                area(tri[0], tri[1], *q) / whole,
            ];
            let d = *q - c;
            decal.points.push(DecalPoint {
                triangle: t as u32,
                weights,
                uv: [d.dot(a) / p.width + 0.5, d.dot(v) / p.height + 0.5],
            });
        }
        for i in 2..poly.len() as u16 {
            decal.triangles.push([first, first + i - 1, first + i]);
        }
    }
    decal
}

impl Decal {
    /// The decal's triangles with the corners at the mesh as it is now
    /// (`FUN_005a1ee0`). `triangle(i)` gives source triangle `i`'s corners in
    /// index order; the result is `(position, normal, uv)` per corner, three per
    /// triangle, in the chassis' frame and in the winding the renderer culls.
    pub fn corners(
        &self,
        triangle: impl Fn(u32) -> Option<[Vec3; 3]>,
    ) -> Vec<(Vec3, Vec3, [f32; 2])> {
        let mut places = Vec::with_capacity(self.points.len());
        for pt in &self.points {
            places.push(triangle(pt.triangle).map(|t| {
                let normal = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
                let w = pt.weights;
                (
                    t[0] * w[0] + t[1] * w[1] + t[2] * w[2] + normal * BIAS,
                    normal,
                )
            }));
        }
        let mut out = Vec::with_capacity(self.triangles.len() * 3);
        for tri in &self.triangles {
            let corner =
                |i: u16| places[i as usize].map(|(p, n)| (p, n, self.points[i as usize].uv));
            // The mesh data winds the other way round to the renderer's rule
            // (the chassis items swap two corners), so the fan is emitted reversed.
            let (Some(a), Some(b), Some(c)) = (corner(tri[0]), corner(tri[2]), corner(tri[1]))
            else {
                continue;
            };
            out.extend([a, b, c]);
        }
        out
    }
}

/// The material a mark of `spec` is drawn with (`FUN_005a2240` over the state
/// object at `0x71fd78`).
pub fn material(image: Arc<Image>, spec: &DecalSpec) -> Material {
    Material {
        stages: vec![Stage {
            image,
            kind: StageKind::Modulate,
            linear: true,
            uv_set: 0,
        }],
        transparent: true,
        // The slashes (`SRCALPHA, ONE`) are alpha tested (`>= 16` of the texture's
        // alpha times the vertex alpha); the dents blend with no test.
        no_alpha_test: !spec.additive(),
        additive: spec.additive(),
        has_vertex_colors: true,
        lighting: true,
        z_write: false,
        cull: true,
        ..Material::default()
    }
}

/// One draw item for the marks that share `image` and blend: `vertices` are
/// already in the world.
pub fn item(image: Arc<Image>, spec: &DecalSpec, vertices: Vec<Vertex>) -> DrawItem {
    DrawItem::new(
        vertices,
        material(image, spec),
        format!("decal:{}", spec.file),
    )
}

/// Load `Arenas/decals/<file>` under the game directory.
pub fn load(game: &Path, file: &str) -> Option<Arc<Image>> {
    let rel = format!("Arenas/decals/{file}");
    let path = crate::resolve_ci(game, &rel).unwrap_or_else(|| game.join(rel));
    load_image(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> DecalSpec {
        DecalSpec::parse("single_slash.tga 0 0 1 .5 .5 1").unwrap()
    }

    #[test]
    fn the_decal_line_parses() {
        let s = spec();
        assert_eq!(s.file, "single_slash.tga");
        assert_eq!(s.direction, Vec3::Z);
        assert_eq!((s.width, s.height, s.mode), (0.5, 0.5, 1));
        assert!(s.additive());
        let d = DecalSpec::parse("batter02.tga 1 0 0 .5 .5 0").unwrap();
        assert_eq!(d.direction, Vec3::X);
        assert!(!d.additive());
        assert_eq!(
            DecalSpec::parse("mace.tga 0 0 1 .25 .25 0").unwrap().width,
            0.25
        );
        assert!(DecalSpec::parse("").is_none());
    }

    /// Slashes are additive and alpha tested, dents blended and untested (`0x71fd78`).
    #[test]
    fn slashes_are_tested_and_dents_are_not() {
        let image = Image::solid([255, 255, 255, 255]);
        let slash = material(image.clone(), &spec());
        assert!(slash.additive && slash.alpha_tested() && !slash.blended());
        let dent = material(
            image,
            &DecalSpec::parse("batter02.tga 1 0 0 .5 .5 0").unwrap(),
        );
        assert!(!dent.additive && !dent.alpha_tested() && dent.blended());
    }

    /// A flat plate facing +Y is cut to the box: a `0.5 x 0.5` square, uv 0..1.
    #[test]
    fn a_flat_face_is_cut_to_the_box() {
        // Two triangles of a big plate, wound so the face normal is +Y.
        let a = Vec3::new(-2.0, 0.0, -2.0);
        let b = Vec3::new(-2.0, 0.0, 2.0);
        let c = Vec3::new(2.0, 0.0, 2.0);
        let d = Vec3::new(2.0, 0.0, -2.0);
        let tris = [[a, b, c], [a, c, d]];
        assert!((tris[0][1] - tris[0][0]).cross(tris[0][2] - tris[0][0]).y > 0.0);
        let p = Projection {
            center: Vec3::ZERO,
            normal: Vec3::Y,
            axis: Vec3::X,
            width: 0.5,
            height: 0.5,
        };
        let decal = project(&spec(), 0.5, &p, tris);
        assert!(!decal.points.is_empty());
        let lo = decal
            .points
            .iter()
            .map(|q| q.uv[0])
            .fold(f32::MAX, f32::min);
        let hi = decal
            .points
            .iter()
            .map(|q| q.uv[0])
            .fold(f32::MIN, f32::max);
        assert!(
            (lo - 0.0).abs() < 1e-4 && (hi - 1.0).abs() < 1e-4,
            "{lo} {hi}"
        );
        // The marks stand 0.5 mm over the plate.
        let corners = decal.corners(|i| tris.get(i as usize).copied());
        assert_eq!(corners.len(), decal.triangles.len() * 3);
        assert!(corners
            .iter()
            .all(|(pos, n, _)| (pos.y - BIAS).abs() < 0.01 && n.y > 0.99));
    }

    /// A face turned away from the contact normal takes no mark.
    #[test]
    fn faces_turned_away_take_no_mark() {
        let a = Vec3::new(-2.0, 0.0, -2.0);
        let b = Vec3::new(-2.0, 0.0, 2.0);
        let c = Vec3::new(2.0, 0.0, 2.0);
        let p = Projection {
            center: Vec3::ZERO,
            normal: -Vec3::Y,
            axis: Vec3::X,
            width: 0.5,
            height: 0.5,
        };
        assert!(project(&spec(), 1.0, &p, [[a, b, c]]).points.is_empty());
    }

    /// A box that misses the mesh leaves an empty decal (the original still counts
    /// it against the chassis' 25).
    #[test]
    fn a_box_off_the_mesh_is_empty() {
        let tri = [
            Vec3::new(-2.0, 0.0, -2.0),
            Vec3::new(-2.0, 0.0, 2.0),
            Vec3::new(2.0, 0.0, 2.0),
        ];
        let p = Projection {
            center: Vec3::new(0.0, 5.0, 0.0),
            normal: Vec3::Y,
            axis: Vec3::X,
            width: 0.5,
            height: 0.5,
        };
        assert!(project(&spec(), 1.0, &p, [tri]).points.is_empty());
    }
}
