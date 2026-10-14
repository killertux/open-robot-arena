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

/// Load an arena (`.py` script or `.gmf`) into visuals + collision triangles.
pub fn load_arena(path: &str) -> Result<ArenaScene, Box<dyn std::error::Error>> {
    let items = ra2_render::collect_items(path)?;
    let mut triangles = Vec::with_capacity(items.iter().map(|i| i.vertices.len() / 3).sum());
    for it in &items {
        for tri in it.vertices.as_chunks::<3>().0 {
            // RA2/D3D8 geometry is wound clockwise as seen from outside, which
            // makes `(b-a)x(c-a)` point *inward*. Jolt's mesh collider treats the
            // counter-clockwise (front) side as the solid one, so rewind.
            triangles.push([
                Vec3::from(tri[0].pos),
                Vec3::from(tri[2].pos),
                Vec3::from(tri[1].pos),
            ]);
        }
    }
    let (min, max) = ra2_render::bounds(&items);
    Ok(ArenaScene {
        items,
        triangles,
        center: (min + max) * 0.5,
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
