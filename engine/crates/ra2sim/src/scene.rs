//! Scene assembly: turn RA2 assets into physics shapes + visual geometry.

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
    /// World gravity the arena's `.gmf` asks Havok for.
    pub gravity: Vec3,
    /// Collision friction of the arena's fixed geometry (its rigid bodies).
    pub friction: f32,
    /// Restitution of the arena's fixed geometry.
    pub restitution: f32,
    /// The `.gmf`'s static background colour (RGB 0..1); the clear colour
    /// unless the arena script calls `plus.setBackColor`.
    pub background: [f32; 3],
}

/// Gravity every shipped arena but the practice arena declares.
pub const STOCK_GRAVITY: Vec3 = Vec3::new(0.0, -9.809_98, 0.0);

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

/// Does a downward ray at `(x, z)`, cast from above everything (`above`),
/// land on something lower than `below` (the arena's vertical middle, so a
/// ceiling does not count as a floor)?
///
/// It used to start 5 m above the lowest *visible* point, which for an arena
/// with a deep pit (Hilltop's is 20 m down, Electric's 8 m) is under its own
/// floor, so the ray never met it.
fn has_floor(triangles: &[[Vec3; 3]], x: f32, z: f32, above: f32, below: f32) -> bool {
    let origin = Vec3::new(x, above, z);
    triangles.iter().any(|t| {
        ray_triangle(origin, Vec3::NEG_Y, t[0], t[1], t[2])
            .map(|d| origin.y - d < below)
            .unwrap_or(false)
    })
}

/// Whether every start point has collision under it (within a few metres): the
/// Metal Skull's decoded rigid-body geometry omits its left plateau, and a robot
/// spawned there would fall into the pit before the bell.
fn supports_starts(triangles: &[[Vec3; 3]], starts: &[(Vec3, Vec3)]) -> bool {
    starts.iter().all(|(p, _)| {
        let origin = *p + Vec3::Y * 0.01;
        triangles.iter().any(|t| {
            ray_triangle(origin, Vec3::NEG_Y, t[0], t[1], t[2])
                .map(|d| d < 6.0)
                .unwrap_or(false)
        })
    })
}

/// The fixed collision triangles plus those of the movable bodies the file's
/// constraints hold up, where they rest: FlexTop's only fixed collidable is an
/// implicit plane 9.8 m down (`colliders-flextop__live__000`), and its start
/// points stand on the table, a 400 kg body hung from a point-to-point and two
/// angular dashpots (docs/29 section 12j).
fn with_suspended(h: &ra2_render::HavokArena) -> Vec<[Vec3; 3]> {
    let mut out = h.static_triangles.clone();
    for b in h
        .bodies
        .iter()
        .filter(|b| !b.is_static() && b.collides() && h.is_suspended(&b.name))
    {
        for t in b.points.as_chunks::<3>().0 {
            out.push([
                b.transform.transform_point3(t[0]),
                b.transform.transform_point3(t[1]),
                b.transform.transform_point3(t[2]),
            ]);
        }
    }
    out
}

/// Load an arena (`.py` script or `.gmf`) into visuals + collision triangles.
///
/// The collider prefers the arena's **own Havok rigid bodies** (the geometry the
/// game actually simulates, identified by a zero mass and an enabled
/// `DISABLE_COLLISIONS`), then the `*collision*`-named proxy meshes, then the
/// visible geometry. A candidate set that does not cover the arena centre is
/// rejected so robots can never drop through the world. Override with
/// `RA2_COLLISION=rb|proxy|visible`.
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

    let starts = ra2_render::start_points(path).unwrap_or_default();
    let havok = ra2_render::collect_havok(path)?;
    let gravity = havok
        .as_ref()
        .and_then(|h| h.gravity)
        .unwrap_or(STOCK_GRAVITY);
    // The game gives each fixed body its own friction; a single merged mesh
    // collider can only carry one, so take the strongest (the floor's).
    let strongest = |pick: fn(&ra2_render::HavokBody) -> f32| {
        havok
            .as_ref()
            .map(|h| {
                h.bodies
                    .iter()
                    .filter(|b| b.is_static() && b.collides())
                    .map(pick)
                    .fold(0.0f32, f32::max)
            })
            .unwrap_or(0.0)
    };
    let friction = {
        let f = strongest(|b| b.friction);
        if f > 0.0 {
            f
        } else {
            0.9
        }
    };
    let restitution = {
        let e = strongest(|b| b.elasticity);
        if e > 0.0 {
            e
        } else {
            0.05
        }
    };

    let mode = std::env::var("RA2_COLLISION").unwrap_or_else(|_| "auto".into());
    let mode = mode.to_ascii_lowercase();
    let mut source = "visible";
    let mut raw = visible.clone();
    if mode != "visible" {
        let mut taken = false;
        if mode == "auto" || mode == "rb" {
            if let Some(h) = &havok {
                if !h.static_triangles.is_empty()
                    && has_floor(
                        &h.static_triangles,
                        centre.x,
                        centre.z,
                        max.y + 1.0,
                        centre.y,
                    )
                    && (mode == "rb" || supports_starts(&with_suspended(h), &starts))
                {
                    source = "rigid bodies";
                    raw = h.static_triangles.clone();
                    taken = true;
                } else if !h.static_triangles.is_empty() {
                    eprintln!("  rigid-body collider does not cover the floor");
                }
            }
        }
        if !taken && mode != "rb" {
            let proxy = ra2_render::collect_collision(path)?;
            // Some arenas ship a proxy set that misses the floor entirely; if the
            // centre has nothing under it we would drop robots through the world,
            // so fall back rather than trust it.
            match proxy {
                Some(t) if has_floor(&t, centre.x, centre.z, max.y + 1.0, centre.y) => {
                    source = "proxy";
                    raw = t;
                }
                Some(_) => {
                    eprintln!("  proxy collider does not cover the floor; using visible geometry")
                }
                None => {}
            }
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
        "arena collider: {source} geometry, {} triangles, gravity ({:.3}, {:.3}, {:.3}), friction {:.2}",
        triangles.len(),
        gravity.x,
        gravity.y,
        gravity.z,
        friction
    );
    Ok(ArenaScene {
        items,
        triangles,
        center: centre,
        min,
        max,
        gravity,
        friction,
        restitution,
        background: ra2_render::scene_background(path).unwrap_or([0.0; 3]),
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
    /// Collide as this triangle mesh over `hull_points` (Havok's `GEOMETRIC`
    /// collidable) instead of their hull: a body the file's constraints hold,
    /// FlexTop's table, which is a ring with the hole in the middle a robot
    /// can fall through.
    pub mesh: Option<Vec<[u32; 3]>>,
    /// Mass properties read off the original where no rule is recovered:
    /// the centre of mass in the object's frame and the inertia tensor about
    /// it (see [`LIVE_MASS`]).
    pub live_mass: Option<(Vec3, glam::Mat3)>,
}

/// Arena bodies whose mass properties are taken from the original as measured
/// (body `+0x30`, `+0x19c`), because the rule Havok derives them by is not
/// recovered: `(arena body, centre in the object frame, inertia diagonal,
/// inertia xz)`.
///
/// FlexTop's table (`flextable-scout-alarm-flextop__live__000`): 400 kg, its
/// centre 0.8805 m above the pivot, i.e. `(0, 1.0000, 0)` in its own frame,
/// and an inertia of (15550.73, 31099.06, 15550.73) kg·m² with an xz term of
/// -6.48: a flat distribution (`Iyy = Ixx + Izz` to 0.01 %). Its mesh's hull
/// gives (0, 0.974, 0) and (15025, 29869, 14950); the open mesh's own solid,
/// the area of its faces and its vertices do not fit either (docs/29 §12j).
pub const LIVE_MASS: &[(&str, [f32; 3], [f32; 3], f32)] = &[(
    "floor_collision",
    [0.0, 1.0000, 0.0],
    [15550.73, 31099.06, 15550.73],
    -6.484,
)];

fn live_mass(name: &str, mass: f32) -> Option<(Vec3, glam::Mat3)> {
    let (_, com, diag, xz) = LIVE_MASS
        .iter()
        .find(|(n, ..)| n.eq_ignore_ascii_case(name))?;
    if (mass - 400.0).abs() > 1e-3 {
        return None;
    }
    let i = glam::Mat3::from_cols(
        Vec3::new(diag[0], 0.0, *xz),
        Vec3::new(0.0, diag[1], 0.0),
        Vec3::new(*xz, 0.0, diag[2]),
    );
    Some((Vec3::from(*com), i))
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
                    mesh: None,
                    live_mass: None,
                });
            }
        }
    }
    Ok(props)
}

/// Movable bodies the arena's **own `.gmf`** declares (ovals' start blocks,
/// hammers, floor saws, crates). Trigger zones (`DISABLE_COLLISIONS`) and fixed
/// bodies are skipped — the latter go into the static collider instead.
pub fn load_havok_props(path: &str) -> Result<Vec<Prop>, Box<dyn std::error::Error>> {
    let Some(havok) = ra2_render::collect_havok(path)? else {
        return Ok(Vec::new());
    };
    Ok(havok
        .bodies
        .iter()
        .filter(|b| !b.is_static() && b.collides() && !b.points.is_empty() && !b.items.is_empty())
        .map(|b| {
            let (_, rotation, position) = b.transform.to_scale_rotation_translation();
            // `points` is a triangle soup: three per face.
            let mesh = havok.is_suspended(&b.name).then(|| {
                (0..b.points.len() as u32 / 3)
                    .map(|i| [3 * i, 3 * i + 1, 3 * i + 2])
                    .collect()
            });
            Prop {
                name: b.name.clone(),
                hull_points: b.points.clone(),
                position,
                rotation,
                mass: b.mass,
                friction: b.friction.max(0.05),
                restitution: b.elasticity.clamp(0.0, 1.0),
                items: b.items.clone(),
                live_mass: mesh.as_ref().and_then(|_| live_mass(&b.name, b.mass)),
                mesh,
            }
        })
        .collect())
}
