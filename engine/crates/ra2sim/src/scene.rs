//! Scene assembly: turn RA2 assets into physics shapes + visual geometry.

use std::collections::HashSet;
use std::path::Path;

use glam::{Quat, Vec3};
use ra2_render::DrawItem;

/// `BodyDesc::user_data` tags.
pub const USER_ARENA: u64 = 1;
pub const USER_BOT: u64 = 2;
pub const USER_PROP: u64 = 3;

/// A loaded arena: visual batches (world space) plus its collision triangles.
pub struct ArenaScene {
    pub items: Vec<DrawItem>,
    pub triangles: Vec<[Vec3; 3]>,
    pub center: Vec3,
    pub min: Vec3,
    pub max: Vec3,
}

impl ArenaScene {
    /// Non-indexed triangle soup for a mesh collider.
    pub fn triangle_soup(&self) -> (Vec<Vec3>, Vec<[u32; 3]>) {
        let mut verts = Vec::with_capacity(self.triangles.len() * 3);
        let mut idx = Vec::with_capacity(self.triangles.len());
        for (i, t) in self.triangles.iter().enumerate() {
            verts.extend_from_slice(t);
            let b = (i * 3) as u32;
            idx.push([b, b + 1, b + 2]);
        }
        (verts, idx)
    }
}

/// Möller–Trumbore ray/triangle intersection; returns the distance along `dir`.
fn ray_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-8 {
        return None;
    }
    let inv = 1.0 / det;
    let t = origin - a;
    let u = t.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let dist = e2.dot(q) * inv;
    (dist > 1e-4).then_some(dist)
}

/// Does a downward ray at `(x, z)` hit these triangles?
fn has_floor(triangles: &[[Vec3; 3]], x: f32, z: f32, top: f32) -> bool {
    let origin = Vec3::new(x, top + 5.0, z);
    triangles.iter().any(|t| {
        ray_triangle(origin, Vec3::NEG_Y, t[0], t[1], t[2])
            .map(|d| d < top + 10.0)
            .unwrap_or(false)
    })
}

/// Load an arena (`.py` script or `.gmf`) into visuals + collision triangles.
///
/// Colliders come from the arena's own physics proxies when it has them (clean,
/// no decoration), otherwise from the visible geometry. A proxy set that does
/// not actually cover the floor is rejected. Override with
/// `RA2_COLLISION=proxy|visible`.
pub fn load_arena(path: &str) -> Result<ArenaScene, Box<dyn std::error::Error>> {
    let items = ra2_render::collect_items(path)?;
    let (min, max) = ra2_render::bounds(&items);
    let centre = (min + max) * 0.5;

    let visible: Vec<[Vec3; 3]> = {
        let mut t = Vec::with_capacity(items.iter().map(|i| i.vertices.len() / 3).sum());
        for it in &items {
            for tri in it.vertices.as_chunks::<3>().0 {
                t.push([
                    Vec3::from(tri[0].pos),
                    Vec3::from(tri[1].pos),
                    Vec3::from(tri[2].pos),
                ]);
            }
        }
        t
    };

    let mode = std::env::var("RA2_COLLISION").unwrap_or_else(|_| "auto".into());
    let mut source = "visible";
    let mut raw = visible.clone();
    if !mode.eq_ignore_ascii_case("visible") {
        let proxy = ra2_render::collect_collision(path)?;
        // Some arenas ship a proxy set that misses the floor entirely; if the
        // centre has nothing under it we would drop robots through the world,
        // so fall back rather than trust it.
        let usable = proxy
            .as_ref()
            .map(|t| has_floor(t, centre.x, centre.z, min.y))
            .unwrap_or(false);
        match proxy {
            Some(t) if usable => {
                source = "proxy";
                raw = t;
            }
            Some(_) => {
                eprintln!("  proxy collider does not cover the floor; using visible geometry")
            }
            None => {}
        }
    }

    // RA2/D3D8 display geometry is wound clockwise as seen from outside, which
    // makes `(b-a)x(c-a)` point *inward*, while Jolt's mesh collider treats the
    // counter-clockwise (front) side as solid. Proxies in the wild are not
    // consistently wound either way, so emit both faces and the collider is
    // solid from either side (the meshes have no volume, so this is free of
    // ambiguity for us and costs only triangles).
    let mut triangles: Vec<[Vec3; 3]> = Vec::with_capacity(raw.len() * 2);
    for t in raw {
        triangles.push([t[0], t[2], t[1]]);
        triangles.push([t[0], t[1], t[2]]);
    }

    println!(
        "arena collider: {source} geometry, {} triangles",
        triangles.len()
    );
    Ok(ArenaScene {
        items,
        triangles,
        center: centre,
        min,
        max,
    })
}

/// A loaded robot: visual batches (bot-local) plus a hull of its geometry.
pub struct BotRig {
    pub items: Vec<DrawItem>,
    /// Bot-local points fed to the convex-hull collider.
    pub hull_points: Vec<Vec3>,
    pub min: Vec3,
    pub max: Vec3,
    pub mass: f32,
}

impl BotRig {
    /// Half-extents of the robot's bounding box.
    pub fn half_extent(&self) -> Vec3 {
        (self.max - self.min) * 0.5
    }
}

/// Load a `.bot` into visuals + a collider hull. `mass` is in kg.
pub fn load_bot(path: &str, mass: f32) -> Result<BotRig, Box<dyn std::error::Error>> {
    let items = ra2_render::collect_items(path)?;
    if items.is_empty() {
        return Err("robot has no renderable geometry".into());
    }

    // Convex-hull input: every vertex, thinned on a ~1 cm grid.
    let mut seen: HashSet<(i32, i32, i32)> = HashSet::new();
    let mut hull_points = Vec::new();
    for it in &items {
        for v in &it.vertices {
            let p = Vec3::from(v.pos);
            let key = (
                (p.x * 100.0).round() as i32,
                (p.y * 100.0).round() as i32,
                (p.z * 100.0).round() as i32,
            );
            if seen.insert(key) {
                hull_points.push(p);
            }
        }
    }

    let (min, max) = ra2_render::bounds(&items);
    Ok(BotRig {
        items,
        hull_points,
        min,
        max,
        mass,
    })
}

/// A movable arena prop, backed by a `.gmf` rigid body (e.g. a crate).
pub struct Prop {
    pub name: String,
    /// Object-local hull input points.
    pub hull_points: Vec<Vec3>,
    /// Initial world transform (arena space).
    pub position: Vec3,
    pub rotation: Quat,
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    /// Visual batches, object-local.
    pub items: Vec<DrawItem>,
}

/// Load movable props from the arena script's `AddXtra` meshes. Props whose
/// object names match a rigid body in the mesh's `RBCollection` become
/// dynamic bodies using the game's own mass/friction/elasticity.
pub fn load_props(
    arena_path: &str,
    obstacle_filter: &str,
) -> Result<Vec<Prop>, Box<dyn std::error::Error>> {
    if !arena_path.to_ascii_lowercase().ends_with(".py") {
        return Ok(Vec::new());
    }
    let script = ra2fmt::arena::parse_path(Path::new(arena_path))?;
    let Some(root) = ra2_render::find_game_root(Path::new(arena_path)) else {
        return Ok(Vec::new());
    };

    let mut props = Vec::new();
    for xtra in script.xtras_matching(obstacle_filter) {
        let Some(gmf) = ra2_render::resolve_ci(&root, &xtra.gmf) else {
            continue;
        };
        let Ok(data) = std::fs::read(&gmf) else {
            continue;
        };
        let Ok(scene) = ra2fmt::gmi::parse(&data) else {
            continue;
        };
        let dir = gmf.parent().unwrap_or(Path::new(".")).to_path_buf();

        for coll in &scene.objects {
            if coll.rigid_bodies.is_empty() {
                continue;
            }
            for rb in &coll.rigid_bodies {
                let Some(obj) = scene
                    .objects
                    .iter()
                    .find(|o| o.name.eq_ignore_ascii_case(&rb.name) && o.mesh.is_some())
                else {
                    continue;
                };
                let items = ra2_render::object_items(&scene, &obj.name, &dir);
                if items.is_empty() {
                    continue;
                }
                let hull_points: Vec<Vec3> = obj
                    .mesh
                    .as_ref()
                    .map(|m| m.points.iter().map(|p| Vec3::from(*p)).collect())
                    .unwrap_or_default();
                let tm = ra2_render::object_transform(obj);
                let (_, rotation, position) = tm.to_scale_rotation_translation();
                props.push(Prop {
                    name: rb.name.clone(),
                    hull_points,
                    position,
                    rotation,
                    mass: if rb.mass > 0.0 { rb.mass } else { 20.0 },
                    friction: rb.friction.max(0.05),
                    restitution: rb.elasticity.clamp(0.0, 1.0),
                    items,
                });
            }
        }
    }
    Ok(props)
}
