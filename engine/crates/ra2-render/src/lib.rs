//! `ra2-render` — offscreen wgpu renderer for Robot Arena 2 assets.
//!
//! Renders a `.gmf` mesh, a fully assembled `.bot` robot, or an arena `.py`
//! scene. Core of the M1 asset-viewer milestone, the headless regression
//! harness, and the frame source for the M2 physics sandbox.
//!
//! [`Renderer`] holds the wgpu device so a caller (e.g. the sandbox) can render
//! many frames without re-initialising the GPU.
//!
//! Env: RA2_NO_CHASSIS=1  skip the chassis mesh
//!      RA2_DEBUG=1       print per-part bounds
//!      RA2_CULL          off disables per-material backface culling
//!      RA2_HIDE          comma-separated object-name substrings to skip
//!      RA2_OBSTACLE      arena prop filter (name|all|none)
//!      RA2_CAM           ex,ey,ez,tx,ty,tz  or  @<n|name>

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::{Mat4, Vec3};
use ra2fmt::{bot as botfmt, component as compfmt, gmi, mount};

pub use ra2fmt::mount::{component_local, tm_mat4, MountMode};

pub mod effects;
pub mod material;
mod renderer;
pub mod ui;

pub use material::{scene_lights, Image, Light, Material, Stage, StageKind};
pub use renderer::{DrawItem, RenderGroup, Renderer, Vertex};

// ---------------------------------------------------------------------------
// scene collection
// ---------------------------------------------------------------------------

/// Axis-aligned bounds of every vertex in `items`.
pub fn bounds(items: &[DrawItem]) -> (Vec3, Vec3) {
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for it in items {
        for v in &it.vertices {
            let p = Vec3::from(v.pos);
            min = min.min(p);
            max = max.max(p);
        }
    }
    if min.x > max.x {
        return (Vec3::ZERO, Vec3::ZERO);
    }
    (min, max)
}

/// Print per-part bounds when `RA2_DEBUG` is set.
pub fn debug_print_items(items: &[DrawItem]) {
    if std::env::var("RA2_DEBUG").is_err() {
        return;
    }
    for it in items {
        let (mut a, mut b) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for v in &it.vertices {
            let p = Vec3::from(v.pos);
            a = a.min(p);
            b = b.max(p);
        }
        eprintln!(
            "  part {:26} verts={:5} bbox=({:.3},{:.3},{:.3})..({:.3},{:.3},{:.3}) tex={}x{}{}",
            it.label,
            it.vertices.len(),
            a.x,
            a.y,
            a.z,
            b.x,
            b.y,
            b.z,
            it.tex_size().0,
            it.tex_size().1,
            if it.material.transparent {
                if it.material.additive {
                    " additive"
                } else if it.material.no_alpha_test {
                    " blended"
                } else {
                    " alpha-test"
                }
            } else {
                ""
            }
        );
    }
}

/// The scene background colour stored in an arena's `.gmf` (via its `.py`
/// script's model, or the `.gmf` itself); the clear colour when the arena
/// script does not call `setBackColor`.
pub fn scene_background(input: &str) -> Option<[f32; 3]> {
    let lower = input.to_ascii_lowercase();
    let data = if lower.ends_with(".py") {
        let script = ra2fmt::arena::parse_path(Path::new(input)).ok()?;
        let root = find_game_root(Path::new(input))?;
        std::fs::read(resolve_ci(&root, script.model.as_ref()?)?).ok()?
    } else {
        std::fs::read(input).ok()?
    };
    Some(gmi::parse(&data).ok()?.background)
}

/// Physics-proxy triangles of an arena (`.gmf` or `.py`), if it has any.
///
/// RA2 marks its collision proxies with a negative material reference or names
/// like `*Collision*` / `*havok*`; those are the shapes the game actually
/// collides against, and they exclude the decorative geometry a robot would
/// otherwise high-centre on. Arenas that ship none (the practice arena, for
/// instance) return `None` so callers can fall back to the visible geometry.
pub fn collect_collision(
    input: &str,
) -> Result<Option<Vec<[Vec3; 3]>>, Box<dyn std::error::Error>> {
    let lower = input.to_ascii_lowercase();
    let mut out = Vec::new();
    if lower.ends_with(".py") {
        let script = ra2fmt::arena::parse_path(Path::new(input))?;
        if let Some(root) = find_game_root(Path::new(input)) {
            if let Some(m) = &script.model {
                if let Some(p) = resolve_ci(&root, m) {
                    let data = std::fs::read(&p)?;
                    let scene = gmi::parse(&data)?;
                    out.extend(scene_collision_triangles(&scene, Mat4::IDENTITY));
                }
            }
        }
    } else {
        let data = std::fs::read(input)?;
        let scene = gmi::parse(&data)?;
        out.extend(scene_collision_triangles(&scene, Mat4::IDENTITY));
    }
    if out.len() < 4 {
        Ok(None)
    } else {
        Ok(Some(out))
    }
}

/// Triangles of a scene's physics-proxy objects, in `world` space.
pub fn scene_collision_triangles(scene: &gmi::GmiScene, world: Mat4) -> Vec<[Vec3; 3]> {
    let mut out = Vec::new();
    for o in &scene.objects {
        let Some(mesh) = &o.mesh else { continue };
        if mesh.points.is_empty() || !(mesh.is_collision() || is_collision_name(&o.name)) {
            continue;
        }
        let obj = world * tm_mat4(&o.transform);
        let pts: Vec<Vec3> = mesh
            .points
            .iter()
            .map(|p| obj.transform_point3(Vec3::from(*p)))
            .collect();
        for f in &mesh.faces {
            let (a, b, c) = (f.v[0] as usize, f.v[1] as usize, f.v[2] as usize);
            if a >= pts.len() || b >= pts.len() || c >= pts.len() {
                continue;
            }
            out.push([pts[a], pts[b], pts[c]]);
        }
    }
    out
}

/// One rigid body declared by an arena's Havok data.
#[derive(Clone)]
pub struct HavokBody {
    /// Rigid body name (the object the game simulates).
    pub name: String,
    /// kg; 0 means the body is fixed.
    pub mass: f32,
    pub friction: f32,
    pub elasticity: f32,
    pub unyielding: bool,
    pub simulation_geometry: i32,
    /// `USE_DISPLAY_PROXY`: the geometry object named by `display_object` is
    /// what this body simulates (the exporter's display/collision decoupling).
    pub use_display_proxy: bool,
    /// `HAVOK_GEO_TYPE` (0 `Standard`, 1 `Sphere`/`Box` in shipped arenas).
    pub geo_type: i32,
    /// The body is a trigger zone: it has no collision response.
    pub disable_collisions: bool,
    /// Placement of the body (its `NODE_TM`).
    pub transform: Mat4,
    /// Collision geometry in body-local space.
    pub points: Vec<Vec3>,
    /// The geometry object used for simulation.
    pub collision_object: String,
    /// The geometry object to draw.
    pub display_object: String,
    /// Visual batches in body-local space.
    pub items: Vec<DrawItem>,
}

impl HavokBody {
    /// A body with zero mass (or `unyielding`) never moves.
    pub fn is_static(&self) -> bool {
        self.unyielding || self.mass <= 0.0
    }

    /// Trigger zones (`DISABLE_COLLISIONS`) must not become colliders.
    pub fn collides(&self) -> bool {
        !self.disable_collisions
    }

    /// A body whose simulation geometry is an endless plane rather than a mesh
    /// (see [`gmi::RigidBody::is_plane`]).
    pub fn is_plane(&self) -> bool {
        self.geo_type == gmi::GEO_TYPE_PLANE
    }
}

/// Everything an arena's `.gmf` says about its physics.
#[derive(Clone, Default)]
pub struct HavokArena {
    /// World gravity the file asks Havok for (`[0, -9.80998, 0]` stock).
    pub gravity: Option<Vec3>,
    /// Triangles of the fixed collision geometry, in world space.
    pub static_triangles: Vec<[Vec3; 3]>,
    /// Every rigid body in the file (static and movable).
    pub bodies: Vec<HavokBody>,
}

impl HavokArena {
    /// Fixed collision triangles belonging to a single static body.
    pub fn static_triangles_of(&self, name: &str) -> Vec<[Vec3; 3]> {
        let mut out = Vec::new();
        for b in self
            .bodies
            .iter()
            .filter(|b| b.is_static() && b.name == name)
        {
            let w = b.transform;
            for tri in b.points.as_chunks::<3>().0 {
                if tri.len() == 3 {
                    out.push([
                        w.transform_point3(tri[0]),
                        w.transform_point3(tri[1]),
                        w.transform_point3(tri[2]),
                    ]);
                }
            }
        }
        out
    }
}

/// An arena's declared spawn points, in `ID` order: `(position, facing)`.
///
/// Arenas carry these as `type = startpoint` attachment objects in their model
/// (four in every stock arena). The game drops a robot at one, and they are
/// where the fight actually starts — the Octagon's are 8 m out on the *play
/// surface*, not out at the 3.7 m drop around it, which is where an invented
/// spread puts them. The facing is the point's Z axis, which always looks at the
/// arena centre (Point01 at `(-5.94, 1.87, 5.69)` has `(0.707, 0, -0.707)`).
pub fn start_points(input: &str) -> Result<Vec<(Vec3, Vec3)>, Box<dyn std::error::Error>> {
    let Some(path) = arena_model_path(input)? else {
        return Ok(Vec::new());
    };
    let scene = gmi::parse(&std::fs::read(&path)?)?;
    let mut found: Vec<(i32, (Vec3, Vec3))> = Vec::new();
    for o in scene.attachments() {
        if !o.data.to_ascii_lowercase().contains("startpoint") {
            continue;
        }
        let Some(a) = gmi::Attachment::from_object(o) else {
            continue;
        };
        let tm = tm_mat4(&a.transform);
        found.push((
            a.id,
            (
                tm.transform_point3(Vec3::ZERO),
                tm.transform_vector3(Vec3::Z).normalize_or_zero(),
            ),
        ));
    }
    found.sort_by_key(|(id, _)| *id);
    Ok(found.into_iter().map(|(_, v)| v).collect())
}

/// Resolve the mesh an arena path stands for (the `.gmf` for a `.gmf`, the
/// script's main model for a `.py`).
pub fn arena_model_path(input: &str) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    if input.to_ascii_lowercase().ends_with(".py") {
        let script = ra2fmt::arena::parse_path(Path::new(input))?;
        let Some(root) = find_game_root(Path::new(input)) else {
            return Ok(None);
        };
        Ok(script.model.as_deref().and_then(|m| resolve_ci(&root, m)))
    } else {
        Ok(Some(PathBuf::from(input)))
    }
}

/// Does `name` name one of `roots`, or sit underneath one? Group nodes carry
/// no geometry of their own, so their children have to be walked in.
fn under_any(
    ancestry: &std::collections::HashMap<String, String>,
    name: &str,
    roots: &[&str],
) -> bool {
    let mut current = name.to_ascii_lowercase();
    let mut guard = 0;
    loop {
        if roots.iter().any(|r| r.eq_ignore_ascii_case(&current)) {
            return true;
        }
        match ancestry.get(&current) {
            Some(parent) if !parent.is_empty() && guard < 64 => {
                current = parent.clone();
                guard += 1;
            }
            _ => return false,
        }
    }
}

/// The geometry objects a body simulates (see
/// [`gmi::RigidBody::simulation_objects`]), plus the children of any group node
/// among them.
fn simulation_objects<'a>(
    scene: &'a gmi::GmiScene,
    rb: &gmi::RigidBody,
    ancestry: &std::collections::HashMap<String, String>,
) -> Vec<&'a gmi::SceneObject> {
    let roots = rb.simulation_objects();
    scene
        .objects
        .iter()
        .filter(|o| under_any(ancestry, &o.name, &roots))
        .collect()
}

/// Half-width of the quad a plane body becomes. Shipped arenas are ~50 m
/// across, so this is comfortably beyond any edge.
const PLANE_HALF: f32 = 200.0;

/// Two triangles covering an infinite plane placed by `transform`, whose normal
/// is the matrix's Y row.
fn plane_triangles(transform: &Mat4) -> [[Vec3; 3]; 2] {
    let at = transform.transform_point3(Vec3::ZERO);
    let x = transform.transform_vector3(Vec3::X).normalize_or_zero();
    let z = transform.transform_vector3(Vec3::Z).normalize_or_zero();
    let (x, z) = if x == Vec3::ZERO || z == Vec3::ZERO {
        (Vec3::X, Vec3::Z)
    } else {
        (x, z)
    };
    let (a, b) = (x * PLANE_HALF, z * PLANE_HALF);
    [
        [at - a - b, at + a - b, at + a + b],
        [at - a - b, at + a + b, at - a + b],
    ]
}

/// Read an arena's Havok simulation data: gravity, the fixed collision geometry
/// the game actually collides against, and the movable rigid bodies it spawns.
/// Returns `None` when the file has no `RBCollection` (some arenas rely on the
/// visible geometry instead, and the practice arena's props come from its
/// script).
pub fn collect_havok(input: &str) -> Result<Option<HavokArena>, Box<dyn std::error::Error>> {
    let Some(path) = arena_model_path(input)? else {
        return Ok(None);
    };
    let data = std::fs::read(&path)?;
    let scene = gmi::parse(&data)?;
    if scene.objects.iter().all(|o| o.rigid_bodies.is_empty()) {
        return Ok(None);
    }
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();

    let mut arena = HavokArena {
        gravity: scene.simulation.as_ref().map(|s| Vec3::from(s.gravity)),
        ..Default::default()
    };

    let ancestry: std::collections::HashMap<String, String> = scene
        .objects
        .iter()
        .map(|o| (o.name.to_ascii_lowercase(), o.parent.to_ascii_lowercase()))
        .collect();

    for rb in scene.rigid_bodies() {
        let collision = scene.find_object(rb.collision_object());
        let display = scene.find_object(rb.display_object());
        let transform = tm_mat4(&rb.transform);
        let to_body = transform.inverse();
        // Collision geometry is a triangle soup built from the mesh's faces (a
        // mesh's vertex list is not a triangle list). Which objects that comes
        // from is `SIMULATION_GEOMETRY`'s job, and a display-proxy body names a
        // *group* node whose children hold the real geometry — the Octagon's
        // `main_collision` names `main_display`, which is 57 child objects.
        //
        // A *standalone* proxy is a shape the body reuses: one shared object,
        // named by every body that wants it, and the exporters leave it sitting
        // wherever the first user was. The Combat Arena has a single
        // `spike_collision` at spikes1's position, named by *both* spike bodies,
        // so placing it absolutely puts spikes2's collider 8.6 m away at
        // spikes1's. Such a proxy's mesh is already the shape in the body's own
        // frame, so it is used as written. A proxy that names a *group* (the
        // Octagon's `main_collision` names `main_display`) keeps the object
        // placements its children carry.
        let standalone = rb.simulation_objects().into_iter().find(|name| {
            !name.eq_ignore_ascii_case(&rb.name)
                && !name.eq_ignore_ascii_case("(null)")
                && scene
                    .find_object(name)
                    .map(|o| o.mesh.is_some())
                    .unwrap_or(false)
                && !ancestry.values().any(|p| p.eq_ignore_ascii_case(name))
        });
        let mut points = Vec::new();
        for obj in simulation_objects(&scene, rb, &ancestry) {
            let Some(mesh) = &obj.mesh else { continue };
            let place = match &standalone {
                Some(name) if name.eq_ignore_ascii_case(&obj.name) => Mat4::IDENTITY,
                _ => to_body * tm_mat4(&obj.transform),
            };
            for f in &mesh.faces {
                let (a, b, c) = (f.v[0] as usize, f.v[1] as usize, f.v[2] as usize);
                if a < mesh.points.len() && b < mesh.points.len() && c < mesh.points.len() {
                    points.push(place.transform_point3(Vec3::from(mesh.points[a])));
                    points.push(place.transform_point3(Vec3::from(mesh.points[b])));
                    points.push(place.transform_point3(Vec3::from(mesh.points[c])));
                }
            }
        }
        // A plane body has no mesh; the transform places an infinite plane
        // whose normal is the matrix's Y row. Emit it as two large triangles
        // (both windings are added to the collider below).
        if rb.is_plane() {
            for tri in plane_triangles(&transform) {
                for v in tri {
                    points.push(to_body.transform_point3(v));
                }
            }
        }
        // The body is *drawn* as its display proxy, which is normally a group
        // node, so its children are the real batches (`main_display`,
        // `floor_display`, `Arena`).
        let mut items = Vec::new();
        if let Some(disp) = display {
            let roots = [disp.name.as_str()];
            for o in &scene.objects {
                if under_any(&ancestry, &o.name, &roots) && o.mesh.is_some() {
                    items.extend(object_items(&scene, &o.name, &dir));
                }
            }
        }
        let body = HavokBody {
            name: rb.name.clone(),
            mass: rb.mass,
            friction: rb.friction,
            elasticity: rb.elasticity,
            unyielding: rb.unyielding != 0,
            simulation_geometry: rb.simulation_geometry,
            use_display_proxy: rb.use_display_proxy,
            geo_type: rb.geo_type,
            disable_collisions: rb.disable_collisions,
            transform: tm_mat4(&rb.transform),
            points,
            collision_object: collision.map(|o| o.name.clone()).unwrap_or_default(),
            display_object: display.map(|o| o.name.clone()).unwrap_or_default(),
            items,
        };
        if body.is_static() && body.collides() {
            for tri in body.points.as_chunks::<3>().0 {
                if tri.len() == 3 {
                    arena.static_triangles.push([
                        body.transform.transform_point3(tri[0]),
                        body.transform.transform_point3(tri[1]),
                        body.transform.transform_point3(tri[2]),
                    ]);
                }
            }
        }
        arena.bodies.push(body);
    }
    Ok(Some(arena))
}

pub fn collect_items(input: &str) -> Result<Vec<DrawItem>, Box<dyn std::error::Error>> {
    let lower = input.to_ascii_lowercase();
    if lower.ends_with(".bot") {
        collect_bot(input)
    } else if lower.ends_with(".py") {
        collect_arena(input)
    } else {
        let mut items = Vec::new();
        add_gmi(Path::new(input), Mat4::IDENTITY, &mut items)?;
        Ok(items)
    }
}

/// Assemble an arena scene from its Python script: the main `.gmf` model plus
/// the script's extra props. Select props with `RA2_OBSTACLE=<name|all|none>`.
fn collect_arena(input: &str) -> Result<Vec<DrawItem>, Box<dyn std::error::Error>> {
    let script = ra2fmt::arena::parse_path(Path::new(input))?;
    let root = find_game_root(Path::new(input))
        .ok_or("could not locate game root (Arenas/Components dir)")?;
    let obstacle = std::env::var("RA2_OBSTACLE").unwrap_or_else(|_| "none".into());
    println!(
        "arena {:?}: model={:?} xtras={} (obstacle filter: {obstacle:?})",
        script.name,
        script.model.as_deref().unwrap_or("(none)"),
        script.xtras.len()
    );

    let mut items = Vec::new();
    if let Some(m) = &script.model {
        match resolve_ci(&root, m) {
            Some(p) => add_gmi(&p, Mat4::IDENTITY, &mut items)?,
            None => eprintln!("  model not found: {m}"),
        }
    }
    for x in script.xtras_matching(&obstacle) {
        match resolve_ci(&root, &x.gmf) {
            Some(p) => {
                let before = items.len();
                add_gmi(&p, Mat4::IDENTITY, &mut items)?;
                if items.len() > before {
                    println!("  xtra {:?} ({}) -> {}", x.name, x.material, p.display());
                }
            }
            None => eprintln!("  xtra not found: {}", x.gmf),
        }
    }
    Ok(items)
}

/// Resolve a script-relative path under `root`, case-insensitively.
pub fn resolve_ci(root: &Path, rel: &str) -> Option<PathBuf> {
    ra2fmt::find_ci(root, rel)
}

/// Objects whose name contains any `RA2_HIDE` substring are skipped.
fn is_hidden(name: &str) -> bool {
    use std::sync::OnceLock;
    static HIDE: OnceLock<Vec<String>> = OnceLock::new();
    let list = HIDE.get_or_init(|| {
        std::env::var("RA2_HIDE")
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().to_ascii_lowercase())
                    .filter(|x| !x.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    });
    if list.is_empty() {
        return false;
    }
    let n = name.to_ascii_lowercase();
    list.iter().any(|h| n.contains(h))
}

/// RA2 marks physics proxies with `-1` material refs, names like `*Collision*`
/// or `*traction*` (the component def's `contact`/`passthru` objects), or
/// materials named `havok` / `collision` / `collision proxy`. These are not
/// meant to be drawn.
fn is_collision_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("havok") || n.contains("collision") || n.contains("proxy") || n.contains("traction")
}

/// A component's parsed definition plus its loaded `.gmf` mesh.
pub struct BotComponent {
    pub label: String,
    pub def: compfmt::ComponentDef,
    pub scene: gmi::GmiScene,
    /// Directory holding the component's mesh (for resolving its textures).
    pub dir: PathBuf,
}

/// A parsed robot plus its per-component definitions and meshes.
pub type LoadedBot = (botfmt::Bot, PathBuf, Vec<Option<BotComponent>>);

/// Parse a `.bot` and load the definition + mesh of every attached component.
///
/// Index `0` (the chassis) is always `None`: the chassis mesh is embedded in the
/// `.bot` itself, see [`chassis_items`].
pub fn load_bot_components(input: &str) -> Result<LoadedBot, Box<dyn std::error::Error>> {
    let data = std::fs::read(input)?;
    let bot = botfmt::parse(&data)?;
    let root =
        find_game_root(Path::new(input)).ok_or("could not locate game root (Components dir)")?;

    // Component definitions live in `Components.cfz`; prefer the archive and
    // fall back to loose extracted `.txt` files when present.
    let cfl = ra2fmt::Cfl::open_path(&root.join("Components.cfz")).ok();

    let mut loaded: Vec<Option<BotComponent>> = Vec::with_capacity(bot.components.len());
    for (i, c) in bot.components.iter().enumerate() {
        if i == 0 || c.class == "SmartZone" || c.file.is_empty() {
            loaded.push(None);
            continue;
        }
        let text = cfl
            .as_ref()
            .and_then(|cf| cf.get_text(&c.file))
            .or_else(|| std::fs::read_to_string(root.join(c.file.replace('\\', "/"))).ok());
        let Some(text) = text else {
            eprintln!("  skip {:?}: no definition {}", c.class, c.file);
            loaded.push(None);
            continue;
        };
        let def = compfmt::parse(&text);
        let mesh_path = root.join("Components").join(def.mesh_rel_path());
        match std::fs::read(&mesh_path).ok().map(|b| gmi::parse(&b)) {
            Some(Ok(scene)) => {
                let dir = mesh_path.parent().unwrap_or(Path::new(".")).to_path_buf();
                loaded.push(Some(BotComponent {
                    label: format!("{}({})", c.class, def.name),
                    def,
                    scene,
                    dir,
                }));
            }
            other => {
                eprintln!(
                    "  skip {:?}: {} ({:?})",
                    c.class,
                    mesh_path.display(),
                    other.map(|r| r.err())
                );
                loaded.push(None);
            }
        }
    }
    Ok((bot, root, loaded))
}

/// Visual batches for a scene in `world` space (skipping collision proxies).
pub fn scene_items(scene: &gmi::GmiScene, world: Mat4, dir: &Path, label: &str) -> Vec<DrawItem> {
    let mut items = Vec::new();
    push_scene(scene, world, dir, label, &mut items);
    items
}

/// Visual batches for the chassis mesh embedded in a `.bot`, placed by `world`.
pub fn chassis_items(bot: &botfmt::Bot, world: Mat4) -> Vec<DrawItem> {
    let Some(ch) = bot.components.first().and_then(|c| c.chassis.as_ref()) else {
        return Vec::new();
    };
    let Some(mesh) = &ch.mesh else {
        return Vec::new();
    };
    if std::env::var("RA2_DEBUG").is_ok() {
        let ymin = mesh.points.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        let ymax = mesh.points.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        eprintln!(
            "  raw chassis mesh: {} pts, y={:.3}..{:.3}",
            mesh.points.len(),
            ymin,
            ymax
        );
    }
    let verts = chassis_mesh_vertices(ch, world);
    if verts.is_empty() {
        return Vec::new();
    }
    let image = ch
        .texture_tga
        .as_deref()
        .and_then(decode_image)
        .map(|(rgba, w, h)| Image::from_rgba(rgba, w, h))
        .unwrap_or_else(|| Image::solid([120, 120, 125, 255]));
    vec![DrawItem::new(verts, Material::textured(image), "chassis")]
}

pub fn collect_bot(input: &str) -> Result<Vec<DrawItem>, Box<dyn std::error::Error>> {
    let (bot, root, loaded) = load_bot_components(input)?;
    println!(
        "bot {} v{}: {} components (root={})",
        bot.name,
        bot.version,
        bot.components.len(),
        root.display()
    );

    // Component world transforms, chained through parents. RA2 places a component
    // by its stored translation + quaternion, then a mount yaw (`angle`) and a
    // vertical offset (`height`) about/along the model up axis (Y); at a
    // component-to-component mount it can instead align the attachment frames
    // (see `MountMode` and `docs/06-deferred-fidelity.md` F1).
    let world = component_worlds_mounted(&bot, &loaded, MountMode::from_env());

    let mut items = Vec::new();

    // chassis (mesh embedded in the bot file)
    if std::env::var("RA2_NO_CHASSIS").is_err() {
        items.extend(chassis_items(&bot, world[0]));
    }

    // attached components
    for (i, lc) in loaded.iter().enumerate().skip(1) {
        let Some(lc) = lc else { continue };
        push_scene(&lc.scene, world[i], &lc.dir, &lc.label, &mut items);
    }
    Ok(items)
}

/// Chained bot-local transform for every component using the stored transforms
/// alone (no `.gmf` mount frames). Prefer [`component_worlds_mounted`] with the
/// loaded components, which is what the renderer and the sim both do.
pub fn component_worlds(bot: &botfmt::Bot) -> Vec<Mat4> {
    mount::worlds(
        bot,
        &mount::MountFrames::empty(bot.components.len()),
        mount::MountOptions {
            mode: MountMode::Stored,
            chassis_plane: 0.0,
        },
    )
}

/// Height of the chassis' mounting plane, from `RA2_CHASSIS_PLANE` (default 0).
fn chassis_plane() -> f32 {
    std::env::var("RA2_CHASSIS_PLANE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

/// Chained bot-local transform for every component (parents before children).
///
/// `loaded` supplies the components' `.gmf` scenes, which is where the mount
/// frames live; with it, `mode` can place component-to-component mounts by
/// aligning attachment frames instead of by the stored transform alone. The
/// rule itself lives in [`ra2fmt::mount`] and is shared with the workshop.
pub fn component_worlds_mounted(
    bot: &botfmt::Bot,
    loaded: &[Option<BotComponent>],
    mode: MountMode,
) -> Vec<Mat4> {
    let tables: Vec<Option<std::collections::BTreeMap<i32, gmi::Attachment>>> = loaded
        .iter()
        .map(|lc| lc.as_ref().map(|lc| lc.scene.attachment_table()))
        .collect();
    mount::worlds(
        bot,
        &mount::MountFrames::from_tables(&tables),
        mount::MountOptions {
            mode,
            chassis_plane: chassis_plane(),
        },
    )
}

/// Build the chassis' render vertices from the file's **face groups** (F10).
///
/// The bot file dresses each side of the chassis from its own patch of the
/// 256×256 skin: a group lists the triangles of that side and, for its vertices
/// in ascending index order, the skin coordinate each one uses. That ordering is
/// the rule the stock bots agree on — every group's coordinates land inside the
/// group's own rectangle, and they reproduce the model's own per-point UV table
/// exactly on a chassis whose sides do not share vertices.
///
/// Using the model's per-point table instead is what makes a chassis decal come
/// out rotated and mirrored: a mesh point shared by two sides can only carry one
/// coordinate there, so one of the two sides samples the wrong patch of skin.
fn chassis_mesh_vertices(ch: &ra2fmt::bot::Chassis, world: Mat4) -> Vec<Vertex> {
    let Some(mesh) = ch.mesh.as_ref() else {
        return Vec::new();
    };
    // `(group, vertex) -> skin coordinate`.
    let groups: Vec<std::collections::HashMap<u32, [f32; 2]>> = ch
        .face_groups
        .iter()
        .enumerate()
        .map(|(gi, fg)| {
            let mut seen: Vec<u32> = Vec::new();
            for f in ch.faces.iter().filter(|f| f.group == gi as i32) {
                for &i in &f.points {
                    if !seen.contains(&i) {
                        seen.push(i);
                    }
                }
            }
            seen.sort_unstable();
            seen.iter()
                .zip(fg.coords.iter())
                .map(|(&i, c)| (i, [c[0] / 256.0, c[1] / 256.0]))
                .collect()
        })
        .collect();

    let mut out = Vec::new();
    let pts: Vec<Vec3> = mesh
        .points
        .iter()
        .map(|p| world.transform_point3(Vec3::from(*p)))
        .collect();
    for tri in mesh.indices.as_chunks::<3>().0 {
        let (a, b, cc) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        if a >= pts.len() || b >= pts.len() || cc >= pts.len() {
            continue;
        }
        let (pa, pb, pc) = (pts[a], pts[b], pts[cc]);
        let n = (pb - pa).cross(pc - pa).normalize_or_zero();
        // The first group that owns all three vertices dresses this triangle.
        let group_uv = groups
            .iter()
            .find_map(|g| {
                Some([
                    *g.get(&(a as u32))?,
                    *g.get(&(b as u32))?,
                    *g.get(&(cc as u32))?,
                ])
            })
            .unwrap_or([
                mesh.uvs.get(a).copied().unwrap_or([0.0, 0.0]),
                mesh.uvs.get(b).copied().unwrap_or([0.0, 0.0]),
                mesh.uvs.get(cc).copied().unwrap_or([0.0, 0.0]),
            ]);
        // The chassis builder's triangles are the opposite way round to the
        // arenas' data; swap two corners so the one culling rule holds for
        // robots too.
        for (pos, uv) in [(pa, group_uv[0]), (pc, group_uv[2]), (pb, group_uv[1])] {
            out.push(Vertex {
                pos: pos.into(),
                nrm: n.into(),
                uv,
                uv2: uv,
                color: [1.0, 1.0, 1.0, 1.0],
            });
        }
    }
    out
}

/// One material batch: its material and vertices.
type SceneGroup = (Material, Vec<Vertex>);

/// An object's transform in scene space: its node transform composed with its
/// parents' (a `.gmf` stores linked nodes' transforms relative to the parent, so
/// the bridge arena's tanks sit at their `sidegroup` helper's origin until the
/// chain is applied).
fn node_world(scene: &gmi::GmiScene, o: &gmi::SceneObject) -> Mat4 {
    let mut m = tm_mat4(&o.transform);
    let mut parent = o.parent.as_str();
    for _ in 0..32 {
        if parent.is_empty() || parent.eq_ignore_ascii_case("(null)") {
            break;
        }
        let Some(p) = scene.find_object(parent) else {
            break;
        };
        m = tm_mat4(&p.transform) * m;
        parent = p.parent.as_str();
    }
    m
}

/// Lower-cased names of the objects the original never draws as scenery: the
/// scene's rigid-body collision and trigger geometry.
///
/// A rigid body with `use_display_proxy` is displayed as its `display_proxy`
/// group (or as nothing when that is empty), never as its own mesh; that covers
/// the arenas' collision hulls (`main_collision`), damage and trigger zones
/// (`saw_zone`, `hazardzone1`, `Hammer_Damage_Zone`) and the `h_*` stand-ins.
/// A body's `geometry_proxy` is its physics mesh and is not drawn either. This
/// replaces the old "drop faces with transparency >= 0.5" heuristic, which was
/// really aimed at those volumes' red/green debug materials but also removed
/// real surfaces such as the skull arena's `pit shadow` (transparency 0.5).
fn physics_only_objects(scene: &gmi::GmiScene) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for rb in scene.rigid_bodies() {
        if rb.use_display_proxy {
            out.insert(rb.name.trim().to_ascii_lowercase());
        }
        for proxy in rb.geometry_proxy.split(',') {
            let proxy = proxy.trim();
            if !proxy.is_empty() && !proxy.eq_ignore_ascii_case("(null)") {
                out.insert(proxy.to_ascii_lowercase());
            }
        }
    }
    out
}

/// Build render vertices for a `.gmf` scene, grouped by resolved material so
/// each group can use the correct texture (needed for `Multi/Sub-Object` arenas
/// where each face selects a sub-material).
fn scene_material_groups(scene: &gmi::GmiScene, world: Mat4, dir: &Path) -> Vec<SceneGroup> {
    scene_material_groups_opts(scene, world, dir, None, false)
}

/// Like [`scene_material_groups`] but optionally limited to one object and/or
/// emitted in object-local space (for props that move independently).
fn scene_material_groups_opts(
    scene: &gmi::GmiScene,
    world: Mat4,
    dir: &Path,
    only_object: Option<&str>,
    object_local: bool,
) -> Vec<SceneGroup> {
    let mut index: HashMap<u64, usize> = HashMap::new();
    let mut groups: Vec<SceneGroup> = Vec::new();
    let physics_only = physics_only_objects(scene);
    for o in &scene.objects {
        let Some(mesh) = &o.mesh else { continue };
        if let Some(name) = only_object {
            if !o.name.eq_ignore_ascii_case(name) {
                continue;
            }
        }
        if mesh.is_collision()
            || is_hidden(&o.name)
            || is_collision_name(&o.name)
            || (only_object.is_none() && physics_only.contains(&o.name.to_ascii_lowercase()))
        {
            continue;
        }
        let top = scene.materials.get(mesh.material_ref.max(0) as usize);
        // Skip collision-proxy meshes whose material is tagged "havok"/"collision".
        if top.map(|m| is_collision_name(&m.name)).unwrap_or(false) {
            continue;
        }
        let obj_world = if object_local {
            world
        } else {
            world * node_world(scene, o)
        };
        let pts: Vec<Vec3> = mesh
            .points
            .iter()
            .map(|p| obj_world.transform_point3(Vec3::from(*p)))
            .collect();
        let linear = glam::Mat3::from_mat4(obj_world);
        let mirrored = linear.determinant() < 0.0;
        let normal_mat = if linear.determinant().abs() > 1e-12 {
            linear.inverse().transpose()
        } else {
            glam::Mat3::IDENTITY
        };
        // The second UV set: the mesh's `*MESH_MAPPINGCHANNEL 2` (lightmaps).
        let channel2 = mesh.channels.iter().find(|c| c.index == 2);
        for (fi, f) in mesh.faces.iter().enumerate() {
            let (ia, ib, ic) = (f.v[0] as usize, f.v[1] as usize, f.v[2] as usize);
            if ia >= pts.len() || ib >= pts.len() || ic >= pts.len() {
                continue;
            }
            let eff = top.map(|m| m.resolve_face(f.material));
            // A face may select a collision sub-material.
            if eff.map(|m| is_collision_name(&m.name)).unwrap_or(false) {
                continue;
            }
            let key = ((mesh.material_ref as i64 as u64) << 32)
                | ((mesh.backface_cull as u64 & 0xff) << 24)
                | ((!mesh.colors.is_empty() as u64) << 23)
                | (f.material as u32 as u64 & 0x7f_ffff);
            let gi = *index.entry(key).or_insert_with(|| {
                let material = match eff {
                    Some(m) => Material::from_gmi(m, mesh, dir),
                    None => Material::from_gmi(&gmi::Material::default(), mesh, dir),
                };
                groups.push((material, Vec::new()));
                groups.len() - 1
            });
            let (pa, pb, pc) = (pts[ia], pts[ib], pts[ic]);
            // The stored per-corner normals (`normals[4*face + corner]`, the
            // fourth being the face normal) point out of the solid; fall back
            // to the geometric normal (the data is clockwise-outward, so the
            // right-handed cross product points inwards).
            let fallback = -(pb - pa).cross(pc - pa).normalize_or_zero();
            let corner_normal = |corner: usize| -> Vec3 {
                mesh.normals
                    .get(fi * 4 + corner)
                    .map(|n| (normal_mat * Vec3::from(*n)).normalize_or_zero())
                    .filter(|n| *n != Vec3::ZERO)
                    .unwrap_or(fallback)
            };
            let normals = [corner_normal(0), corner_normal(1), corner_normal(2)];
            let uv_of = |texcoords: &[[f32; 3]], texfaces: &[[u32; 3]], idx: usize| {
                let ti = texfaces.get(fi).map(|t| t[idx] as usize)?;
                // GMF texture coordinates are 3ds Max's (v up); the bitmaps are
                // stored top row first, so v is inverted.
                texcoords.get(ti).map(|t| [t[0], 1.0 - t[1]])
            };
            let uv = |idx: usize| uv_of(&mesh.texcoords, &mesh.texfaces, idx).unwrap_or([0.0, 0.0]);
            let uv2 = |idx: usize| {
                channel2
                    .and_then(|c| uv_of(&c.texcoords, &c.texfaces, idx))
                    .unwrap_or_else(|| uv(idx))
            };
            let color = |idx: usize| -> [f32; 4] {
                mesh.colorfaces
                    .get(fi)
                    .and_then(|cf| mesh.colors.get(cf[idx] as usize))
                    .map(|c| [c[0], c[1], c[2], 1.0])
                    .unwrap_or([1.0; 4])
            };
            let g = &mut groups[gi].1;
            // A mirroring transform flips the winding; swap two corners back.
            let order = if mirrored { [0, 2, 1] } else { [0, 1, 2] };
            let ps = [pa, pb, pc];
            for k in order {
                g.push(Vertex {
                    pos: ps[k].into(),
                    nrm: normals[k].into(),
                    uv: uv(k),
                    uv2: uv2(k),
                    color: color(k),
                });
            }
        }
    }
    groups
}

/// Push one [`DrawItem`] per material group.
fn push_scene(
    scene: &gmi::GmiScene,
    world: Mat4,
    dir: &Path,
    label: &str,
    items: &mut Vec<DrawItem>,
) {
    let lights = scene_lights(scene, world);
    let lights = (!lights.is_empty()).then(|| Arc::new(lights));
    for (material, verts) in scene_material_groups(scene, world, dir) {
        if verts.is_empty() {
            continue;
        }
        let mut item = DrawItem::new(verts, material, label);
        item.scene_lights = lights.clone();
        items.push(item);
    }
}

/// Visual batches for one geometry object, in object-local space, so a physics
/// body's transform can place it (used for movable arena props).
pub fn object_items(scene: &gmi::GmiScene, object_name: &str, dir: &Path) -> Vec<DrawItem> {
    object_items_opts(scene, object_name, dir, true)
}

/// Visual batches for one geometry object, in `.gmf`/scene space.
pub fn object_scene_items(scene: &gmi::GmiScene, object_name: &str, dir: &Path) -> Vec<DrawItem> {
    object_items_opts(scene, object_name, dir, false)
}

fn object_items_opts(
    scene: &gmi::GmiScene,
    object_name: &str,
    dir: &Path,
    object_local: bool,
) -> Vec<DrawItem> {
    scene_material_groups_opts(scene, Mat4::IDENTITY, dir, Some(object_name), object_local)
        .into_iter()
        .filter(|(_, verts)| !verts.is_empty())
        .map(|(material, verts)| DrawItem::new(verts, material, object_name))
        .collect()
}

/// Every mesh point of a scene in scene space (object transforms applied).
/// `include_collision` keeps the invisible physics proxies (for colliders).
pub fn scene_points(scene: &gmi::GmiScene, include_collision: bool) -> Vec<Vec3> {
    let mut out = Vec::new();
    for o in &scene.objects {
        let Some(mesh) = &o.mesh else { continue };
        if mesh.points.is_empty() {
            continue;
        }
        if !include_collision
            && (mesh.is_collision() || is_hidden(&o.name) || is_collision_name(&o.name))
        {
            continue;
        }
        let world = tm_mat4(&o.transform);
        out.extend(
            mesh.points
                .iter()
                .map(|p| world.transform_point3(Vec3::from(*p))),
        );
    }
    out
}

/// Object-local `(name, points)` for every geometry object with a mesh.
pub fn object_points(scene: &gmi::GmiScene) -> Vec<(String, Vec<Vec3>)> {
    scene
        .objects
        .iter()
        .filter_map(|o| {
            let m = o.mesh.as_ref()?;
            if m.points.is_empty() {
                return None;
            }
            Some((
                o.name.clone(),
                m.points.iter().map(|p| Vec3::from(*p)).collect(),
            ))
        })
        .collect()
}

/// The transform of a geometry object within its `.gmf`.
pub fn object_transform(object: &gmi::SceneObject) -> Mat4 {
    tm_mat4(&object.transform)
}

fn add_gmi(
    path: &Path,
    world: Mat4,
    items: &mut Vec<DrawItem>,
) -> Result<(), Box<dyn std::error::Error>> {
    let data = std::fs::read(path)?;
    let scene = gmi::parse(&data)?;
    let dir = path.parent().unwrap_or(Path::new("."));
    push_scene(&scene, world, dir, &path.display().to_string(), items);
    Ok(())
}

pub fn find_game_root(start: &Path) -> Option<PathBuf> {
    // The same override the tests and the workshop library use: point the
    // engine at a game directory from anywhere, including a saved bot in a
    // temporary folder.
    if let Some(root) = std::env::var_os("RA2_GAME") {
        let root = PathBuf::from(root);
        if root.join("Components").is_dir() {
            return Some(root);
        }
    }
    let mut dir = start.parent()?.to_path_buf();
    for _ in 0..6 {
        if dir.join("Components").is_dir() {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

pub(crate) fn decode_image(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    // TGA has no magic bytes, so the image crate cannot guess it; fall back to
    // an explicit TGA decode (the chassis skin and many textures are TGA).
    let img = image::load_from_memory(bytes)
        .or_else(|_| image::load_from_memory_with_format(bytes, image::ImageFormat::Tga))
        .ok()?
        .to_rgba8();
    let (w, h) = (img.width(), img.height());
    Some((img.into_raw(), w, h))
}

// ---------------------------------------------------------------------------
// camera
// ---------------------------------------------------------------------------

/// A camera override resolved from `RA2_CAM`.
pub struct CamOverride {
    pub eye: Vec3,
    pub target: Option<Vec3>,
}

/// Parse `RA2_CAM`: either `ex,ey,ez,tx,ty,tz`, or `@<n>` / `@<name>` to use a
/// static camera declared in an arena script (aimed at the scene centre).
pub fn camera_override(input: &str) -> Option<CamOverride> {
    let spec = std::env::var("RA2_CAM").ok()?;
    let spec = spec.trim();
    if let Some(rest) = spec.strip_prefix('@') {
        let script = ra2fmt::arena::parse_path(Path::new(input)).ok()?;
        let cam = rest
            .parse::<usize>()
            .ok()
            .and_then(|i| script.static_cameras.get(i))
            .or_else(|| {
                script
                    .static_cameras
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(rest))
            })?;
        return Some(CamOverride {
            eye: Vec3::from(cam.position),
            target: None,
        });
    }
    let f: Vec<f32> = spec
        .split(',')
        .filter_map(|x| x.trim().parse().ok())
        .collect();
    if f.len() >= 6 {
        Some(CamOverride {
            eye: Vec3::new(f[0], f[1], f[2]),
            target: Some(Vec3::new(f[3], f[4], f[5])),
        })
    } else {
        None
    }
}

pub fn mvp_matrix(center: Vec3, radius: f32, w: u32, h: u32, cam: Option<CamOverride>) -> Mat4 {
    let aspect = w as f32 / h as f32;
    if let Some(cam) = cam {
        let target = cam.target.unwrap_or(center);
        let dist = (cam.eye - target).length().max(0.001);
        let view = look_at_lh(cam.eye, target, up_for(cam.eye - target));
        let proj = perspective_lh(
            50f32.to_radians(),
            aspect,
            (dist * 0.01).max(0.01),
            dist * 100.0,
        );
        return proj * view;
    }
    // RA2 assets are Y-up. Frame a three-quarter view from above/front/right.
    let dir = Vec3::new(0.75, 0.95, -1.0).normalize();
    let eye = center + dir * (radius * 3.0);
    let view = look_at_lh(eye, center, Vec3::Y);
    let proj = perspective_lh(45f32.to_radians(), aspect, radius * 0.01, radius * 100.0);
    proj * view
}

/// Choose an up vector that is not parallel to the view direction.
fn up_for(dir: Vec3) -> Vec3 {
    if dir.normalize_or_zero().dot(Vec3::Y).abs() > 0.999 {
        Vec3::Z
    } else {
        Vec3::Y
    }
}

/// Left-handed look-at (`D3DXMatrixLookAtLH`). RA2's data is Direct3D
/// left-handed (+X right, +Y up, +Z forward), so the view is built left-handed
/// too; a right-handed look-at mirror-images the world. This and
/// [`perspective_lh`] are the single conversion point for every 3D view.
pub fn look_at_lh(eye: Vec3, center: Vec3, up: Vec3) -> Mat4 {
    let f = (center - eye).normalize();
    let s = up.cross(f).normalize();
    let u = f.cross(s);
    Mat4::from_cols(
        glam::Vec4::new(s.x, u.x, f.x, 0.0),
        glam::Vec4::new(s.y, u.y, f.y, 0.0),
        glam::Vec4::new(s.z, u.z, f.z, 0.0),
        glam::Vec4::new(-s.dot(eye), -u.dot(eye), -f.dot(eye), 1.0),
    )
}

/// Left-handed perspective (`D3DXMatrixPerspectiveFovLH`) with 0..1 depth.
pub fn perspective_lh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    Mat4::from_cols(
        glam::Vec4::new(f / aspect, 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, f, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, far / (far - near), 1.0),
        glam::Vec4::new(0.0, 0.0, -near * far / (far - near), 0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every stock `.bot`, when the (uncommitted) game data is present.
    fn stock_bots() -> Vec<std::path::PathBuf> {
        if let Some(root) = std::env::var_os("RA2_GAME") {
            let root = std::path::PathBuf::from(root);
            if root.join("Components").is_dir() {
                return bots_under(&root);
            }
        }
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let game = loop {
            let candidate = dir.join("local/game");
            if candidate.is_dir() {
                break candidate;
            }
            if !dir.pop() {
                return Vec::new();
            }
        };
        bots_under(&game)
    }

    fn bots_under(game: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![game.to_path_buf()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "bot") {
                    out.push(p);
                }
            }
        }
        out.sort();
        out
    }

    /// The (uncommitted) game directory, when present.
    fn game_root() -> Option<std::path::PathBuf> {
        stock_bots()
            .first()
            .and_then(|p| p.ancestors().find(|a| a.ends_with("game")).map(Into::into))
    }

    /// The arenas' own data drives the material mapping: the box arena's glass
    /// is a blended, lightmapped (unlit) material, its floor carries vertex
    /// colours and a second UV set, and its one omni light sits where the
    /// `.gmf` puts it. Collision and trigger volumes are not drawn.
    #[test]
    fn box_arena_materials_and_lights_come_from_the_gmf() {
        let Some(root) = game_root() else { return };
        let path = root.join("Arenas/box/boxarena.gmf");
        let scene = gmi::parse(&std::fs::read(&path).unwrap()).unwrap();
        let lights = scene_lights(&scene, Mat4::IDENTITY);
        assert_eq!(lights.len(), 1);
        assert!((lights[0].position - Vec3::new(-9.382856, 11.261452, -3.519421)).length() < 1e-4);
        let items = scene_items(&scene, Mat4::IDENTITY, path.parent().unwrap(), "box");
        // The glass: diffuse + lightmap stages, blended, lighting off.
        let glass: Vec<_> = items
            .iter()
            .filter(|i| {
                i.material
                    .stages
                    .iter()
                    .any(|s| s.kind == StageKind::LightMap)
            })
            .collect();
        assert!(glass
            .iter()
            .any(|i| i.material.blended() && !i.material.lighting));
        // The floor's vertex colours reach the vertices.
        assert!(items.iter().any(|i| i.material.has_vertex_colors));
        // Zones and collision hulls are physics only.
        let hidden = physics_only_objects(&scene);
        for name in ["saw_zone", "spike_zone1", "arena_collision"] {
            assert!(hidden.contains(name), "{name}");
        }
        // Everything drawn carries the scene's lights.
        assert!(items.iter().all(|i| i.scene_lights.is_some()));
    }

    /// F5: an arena's main collider simulates its *display proxy*.
    ///
    /// The Octagon's `main_collision` has a 65-vertex mesh of its own and
    /// declares `SIMULATION_GEOMETRY 5` with display proxy `main_display` — a
    /// group of 57 child objects. Taking the body's own mesh instead throws most
    /// of the arena away.
    #[test]
    fn arena_collider_follows_the_display_proxy() {
        let Some(game) = game_root() else { return };
        let path = game.join("Arenas/octagon.py");
        let Some(havok) = collect_havok(&path.to_string_lossy()).unwrap() else {
            return;
        };
        let main = havok
            .bodies
            .iter()
            .find(|b| b.name == "main_collision")
            .expect("the Octagon declares main_collision");
        assert_eq!(main.simulation_geometry, 5);
        assert!(main.use_display_proxy);
        assert_eq!(main.display_object, "main_display");
        // Its own mesh is 108 triangles; the group it names is far bigger.
        assert!(
            main.points.len() / 3 > 350,
            "only {} triangles from the display proxy",
            main.points.len() / 3
        );

        // `topplane_collision` is a plane primitive: no mesh in the file, and
        // its transform puts a downward-facing plane 15.6 m up.
        let plane = havok
            .bodies
            .iter()
            .find(|b| b.name == "topplane_collision")
            .expect("the Octagon declares topplane_collision");
        assert!(plane.is_plane());
        assert_eq!(plane.points.len() / 3, 2);
        assert!(plane.transform.transform_point3(Vec3::ZERO).y > 15.0);
        assert!(plane.transform.transform_vector3(Vec3::Y).y < -0.99);
    }

    /// F11: arenas declare their own spawn points, and they look at the centre.
    ///
    /// The duel used to invent a spread at `(max-min)*0.12`, which for the
    /// Octagon lands on the rim of the 3.7 m drop around its play platform.
    #[test]
    fn start_points_sit_on_the_play_surface() {
        let Some(game) = game_root() else { return };
        let path = game.join("Arenas/octagon.py");
        let starts = start_points(&path.to_string_lossy()).unwrap();
        assert_eq!(starts.len(), 4);
        for (pos, facing) in &starts {
            let radius = pos.x.hypot(pos.z);
            assert!((radius - 8.0).abs() < 0.6, "at {pos:?}, r = {radius}");
            let to_centre = Vec3::new(-pos.x, 0.0, -pos.z).normalize_or_zero();
            assert!(
                facing.dot(to_centre) > 0.99,
                "{facing:?} does not face {to_centre:?}"
            );
        }
    }

    /// F1: the shipped rule must actually *snap* each chain link.
    ///
    /// A `.bot` names a point on the child and the point it mates with on the
    /// parent; the placement has to bring those two points together, which also
    /// constrains how `angle` and a burst motor's start angle may be applied:
    /// both must turn about the parent point's Z, through its origin. The test
    /// also proves it has teeth — under `stored` the same links miss badly.
    #[test]
    fn chain_links_snap_onto_the_parent_point() {
        let bots = stock_bots();
        if bots.is_empty() {
            return; // game data not available
        }
        let mut checked = 0usize;
        let mut stored_misses = 0usize;
        for path in &bots {
            let (bot, _root, loaded) = load_bot_components(path.to_str().unwrap()).unwrap();
            let shipped = component_worlds_mounted(&bot, &loaded, MountMode::FramesFlipped);
            let stored = component_worlds_mounted(&bot, &loaded, MountMode::Stored);
            for (i, lc) in loaded.iter().enumerate() {
                let Some(lc) = lc else { continue };
                let c = &bot.components[i];
                let parent = c.parent.max(0) as usize;
                if parent == 0 || parent >= i {
                    continue; // the chassis has no `.gmf`: nothing to snap to
                }
                let Some(parent_scene) = loaded.get(parent).and_then(|p| p.as_ref()) else {
                    continue;
                };
                let own = lc.scene.attachment_table();
                let on_parent = parent_scene.scene.attachment_table();
                let (Some(own), Some(on_parent)) =
                    (own.get(&c.own_mount), on_parent.get(&c.parent_mount))
                else {
                    continue;
                };
                let gap = |world: &[Mat4]| {
                    let child = (world[i] * tm_mat4(&own.transform)).transform_point3(Vec3::ZERO);
                    let par = (world[parent] * tm_mat4(&on_parent.transform))
                        .transform_point3(Vec3::ZERO);
                    (child - par).length()
                };
                let g = gap(&shipped);
                assert!(
                    g < 1e-3,
                    "{}: component {i} ({}) misses its parent point by {g}",
                    path.display(),
                    lc.label
                );
                if gap(&stored) > 0.05 {
                    stored_misses += 1;
                }
                checked += 1;
            }
        }
        assert!(checked > 100, "only checked {checked} links");
        assert!(
            stored_misses > 100,
            "the test has no teeth: only {stored_misses} links miss under `stored`"
        );
    }

    /// F1: `angle` must turn each chain link about the **mount axis**.
    ///
    /// `AI/Team14/Bot2` is the clean case: four identical anglers hang on the
    /// disc's four rim points, which are 90° apart but share their axes, and
    /// their `angle` values are exactly the rim azimuths (0, pi/2, pi, 3pi/2).
    /// Undoing the spin by that angle — one way round or the other — must make
    /// all four arms coincide. That is what proves the rotation is about the
    /// mount normal rather than the point's Y or a world axis; a wrong axis
    /// leaves the four arms scattered either way.
    ///
    /// Exactly one winding may work. Which one is a convention, settled by the
    /// bots' own previews (see F1), so it is deliberately *not* asserted here.
    #[test]
    fn the_mount_spin_turns_about_the_mount_axis() {
        let path = {
            let bots = stock_bots();
            match bots
                .iter()
                .find(|p| p.ends_with("Team14/Bot2.bot") || p.ends_with("Team14\\Bot2.bot"))
            {
                Some(p) => p.clone(),
                None => return, // game data not available
            }
        };
        let (bot, _root, loaded) = load_bot_components(path.to_str().unwrap()).unwrap();
        let world = component_worlds_mounted(&bot, &loaded, MountMode::FramesFlipped);
        let label = |i: usize| {
            loaded
                .get(i)
                .and_then(|l| l.as_ref())
                .map(|l| l.label.clone())
                .unwrap_or_default()
        };
        let disc = world
            .iter()
            .enumerate()
            .find(|(i, _)| label(*i).contains("Disc"))
            .map(|(i, m)| (i, m.transform_point3(Vec3::ZERO)))
            .expect("no disc in Bot2");
        let m = world[disc.0];
        let axes = [
            m.transform_vector3(Vec3::X).normalize(),
            m.transform_vector3(Vec3::Y).normalize(),
            m.transform_vector3(Vec3::Z).normalize(),
        ];
        let to_disc = |v: Vec3| Vec3::new(v.dot(axes[0]), v.dot(axes[1]), v.dot(axes[2]));
        let spin_z = |v: Vec3, t: f32| {
            Vec3::new(
                v.x * t.cos() - v.y * t.sin(),
                v.x * t.sin() + v.y * t.cos(),
                v.z,
            )
        };

        // (angle, arm in the disc's frame) for each angler.
        let mut links: Vec<(f32, Vec3)> = Vec::new();
        for (i, m) in world.iter().enumerate() {
            if !label(i).contains("Angle Connector") {
                continue;
            }
            let parent = bot.components[i].parent.max(0) as usize;
            if parent != disc.0 {
                continue;
            }
            let angler = m.transform_point3(Vec3::ZERO);
            let Some((_, claw_world)) = world
                .iter()
                .enumerate()
                .find(|(j, _)| bot.components[*j].parent == i as i32)
            else {
                continue;
            };
            let claw = claw_world.transform_point3(Vec3::ZERO);
            links.push((bot.components[i].angle, to_disc(claw - angler).normalize()));
        }
        assert_eq!(links.len(), 4, "expected four anglers, saw {}", links.len());

        let spread = |winding: f32| {
            let v: Vec<Vec3> = links
                .iter()
                .map(|(a, arm)| spin_z(*arm, winding * a))
                .collect();
            let mut worst = 0.0f32;
            for i in 0..v.len() {
                for j in (i + 1)..v.len() {
                    worst = worst.max((v[i] - v[j]).length());
                }
            }
            worst
        };
        let (plus, minus) = (spread(1.0), spread(-1.0));
        assert!(
            (plus < 1e-2) ^ (minus < 1e-2),
            "expected exactly one winding to line the arms up, got +1:{plus} -1:{minus}"
        );
    }
}
