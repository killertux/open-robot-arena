//! The original's material model, as the engine's own material code builds and
//! applies it.
//!
//! RA2 draws through Direct3D 8's fixed-function pipeline. Per batch it builds a
//! material from the GMF (`FUN_005ae630`) and sets the device state for it
//! (`FUN_005b24c0`, through the cached `SetRenderState` wrapper `FUN_00590f30`
//! and the texture-stage wrapper `FUN_00590f80`). This module holds that state
//! and the GMF-to-state mapping; `lib.rs` turns it into a pipeline and shader.
//!
//! Exe-derived facts (addresses are in `Robot Arena 2.exe`):
//!
//! * Map slots (`GMTexture+0x34`): 1 diffuse, 5 self-illumination, 6 opacity,
//!   9 reflect; any other slot is ignored (`FUN_005ae630`, the `switch`).
//! * A diffuse map becomes a *modulate* stage, a self-illumination map a
//!   *lightmap* stage and a reflect map a *sphere-environment* stage, in file
//!   order, each on UV set `BITMAP_MAP_CHANNEL - 1` (`FUN_005b1e60`).
//! * An opacity map loads no bitmap of its own: it marks the material
//!   transparent, and the diffuse bitmap's own alpha channel is the opacity.
//!   It alpha-tests (`ALPHAREF 0x10`, `GREATEREQUAL`) only when its
//!   `BITMAP_INVERT` is 1; otherwise the batch is blended and depth-sorted.
//! * `MATERIAL_TRANSPARENCY > 0` makes the material blended with diffuse alpha
//!   `1 - transparency` (no alpha test). The diffuse colour is the GMF diffuse
//!   unless a diffuse map exists, then white. Ambient is the GMF ambient.
//!   Specular is `shine strength x specular colour`, power `shine x 100`;
//!   `D3DRS_SPECULARENABLE` is on iff the power is positive.
//!   Emissive is never set (zero); `MATERIAL_SELFILLUM` is never read.
//! * A bitmap that cannot be loaded still takes its stage and samples black.
//! * `BITMAP_FILTER` 0 (Pyramidal) samples with point filtering, 1 (SAT) with
//!   bilinear; lightmap and environment stages are always bilinear. Mipmapping
//!   is never enabled (no `D3DTSS_MIPFILTER`).
//! * Lighting is on unless the mesh carries vertex colours or a lightmap stage
//!   is present. `MATERIAL_XP_TYPE` 3 is additive (`SRCALPHA, ONE`).
//! * Back faces are culled (`CULL_CCW`) iff the mesh's `BACKFACE_CULL` is 1.
//! * Bitmaps are shared by name for the whole session ([`session_bitmap`]).
//! * A face id above the sub-material count uses the last sub-material; one
//!   equal to the count is not drawn (`gmi::Material::resolve_face`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use glam::{Mat4, Vec3};
use ra2fmt::gmi;

/// A decoded bitmap (RGBA8, top row first). Shared by `Arc`; the renderer keys
/// its GPU texture cache on the allocation.
pub struct Image {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

impl Image {
    /// A 1x1 image of one colour.
    pub fn solid(rgba: [u8; 4]) -> Arc<Image> {
        Arc::new(Image {
            rgba: rgba.to_vec(),
            w: 1,
            h: 1,
        })
    }

    /// An image from raw RGBA bytes.
    pub fn from_rgba(rgba: Vec<u8>, w: u32, h: u32) -> Arc<Image> {
        Arc::new(Image { rgba, w, h })
    }
}

fn image_cache() -> &'static Mutex<HashMap<PathBuf, Option<Arc<Image>>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<Arc<Image>>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Load (and cache by path) a bitmap; `None` when missing or undecodable.
pub fn load_image(path: &Path) -> Option<Arc<Image>> {
    if let Some(hit) = image_cache().lock().ok()?.get(path) {
        return hit.clone();
    }
    let img = std::fs::read(path)
        .ok()
        .and_then(|b| crate::decode_image(&b))
        .map(|(rgba, w, h)| Arc::new(Image { rgba, w, h }));
    if let Ok(mut c) = image_cache().lock() {
        c.insert(path.to_path_buf(), img.clone());
    }
    img
}

fn session_bitmaps() -> &'static Mutex<HashMap<String, Arc<Image>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Image>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A GMF material's bitmap as the original's texture manager hands it out.
///
/// Every model's stage asks the manager (`FUN_00591030`) for the string
/// `<texture path> + <bitmap as written in the GMF>` (`FUN_005ae630` builds it
/// from the global `DAT_0071f574`, which only the bot lab's `FUN_00593a40`
/// changes) and gets back the first live texture whose name equals it
/// (`FUN_0058e390`, an exact compare), loading a file only on a miss. Live
/// (`tools/oracle/capture_arena_states.py`, docs/34 section 9) the path is the
/// empty string for every one of 573 arena-load requests, so the arena key is
/// the bare name, and 8 arenas played twice in one session loaded each shared
/// name's file exactly once: the first file loaded under a name is what every
/// later model with that name gets, whatever its own directory holds
/// (`bars.tga` compressor then electric/skull, `sawhazard.bmp` and
/// `triple_spike.bmp` from box). Nothing cleared the list (the clear-all
/// functions `0x592300`/`0x5924e0` run only from the graphics shutdown).
/// A file that fails to load is not cached (the manager deletes it).
///
/// The key carries the current texture path ([`set_texture_path`]), so a
/// bot-lab request (path set by `FUN_00593a40`) never shares an entry with an
/// arena's bare name.
pub fn session_bitmap(dir: &Path, bitmap: &str) -> Option<Arc<Image>> {
    let key = format!("{}{bitmap}", texture_path());
    if let Some(hit) = session_bitmaps().lock().ok()?.get(&key) {
        return Some(hit.clone());
    }
    let img = resolve_bitmap(dir, bitmap).and_then(|p| load_image(&p))?;
    if let Ok(mut c) = session_bitmaps().lock() {
        return Some(c.entry(key).or_insert(img).clone());
    }
    Some(img)
}

/// The default texture path: empty (`DAT_0071f574` is a NULL `std::string`
/// pointer in every arena load seen live).
pub const DEFAULT_TEXTURE_PATH: &str = "";

fn texture_path_cell() -> &'static Mutex<String> {
    static PATH: OnceLock<Mutex<String>> = OnceLock::new();
    PATH.get_or_init(|| Mutex::new(DEFAULT_TEXTURE_PATH.to_string()))
}

/// The global texture path the session cache is keyed with (`DAT_0071f574`).
pub fn texture_path() -> String {
    texture_path_cell()
        .lock()
        .map(|p| p.clone())
        .unwrap_or_else(|_| DEFAULT_TEXTURE_PATH.to_string())
}

/// Change the global texture path as the bot lab does (`FUN_00593a40`, called
/// from `0x42e650` with `UI\\BotLab\\Maps`). Bitmaps requested under another path
/// never share an entry with the arena's, even when the bare names match.
pub fn set_texture_path(path: &str) {
    if let Ok(mut p) = texture_path_cell().lock() {
        *p = path.to_string();
    }
}

/// What a texture stage does (`FUN_005b24c0`, texture type 0/1/2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageKind {
    /// `COLOROP = MODULATE` of texture and the previous stage.
    Modulate,
    /// A baked lightmap (`MAP_SELFILLUM`): modulate, and lighting is switched off.
    LightMap,
    /// A sphere map (`MAP_REFLECT`) from camera-space normals, combined with
    /// `MODULATEALPHA_ADDCOLOR`.
    Env,
}

/// One texture stage.
#[derive(Clone)]
pub struct Stage {
    pub image: Arc<Image>,
    pub kind: StageKind,
    /// Bilinear (`true`) or point sampling.
    pub linear: bool,
    /// Which vertex UV set feeds the stage (`TEXCOORDINDEX`): 0 or 1.
    pub uv_set: u32,
}

/// Everything the original sets on the device for one batch.
#[derive(Clone)]
pub struct Material {
    /// Up to three stages (the data never exceeds two).
    pub stages: Vec<Stage>,
    /// Alpha blending (and, with [`Material::no_alpha_test`] false, alpha test).
    pub transparent: bool,
    /// Additive blending (`SRCALPHA, ONE`) instead of `SRCALPHA, INVSRCALPHA`.
    pub additive: bool,
    /// Blend and depth-sort instead of alpha-testing.
    pub no_alpha_test: bool,
    /// `D3DRS_AMBIENTMATERIALSOURCE = COLOR1` (the vertex colours).
    pub vertex_color_ambient: bool,
    /// The mesh carries vertex colours (a `D3DFVF_DIFFUSE` vertex).
    pub has_vertex_colors: bool,
    /// `D3DRS_LIGHTING`.
    pub lighting: bool,
    pub z_write: bool,
    pub z_test: bool,
    /// Drawn first, in order, with its blend state, instead of joining the sorted blended pass
    /// (the lab's chassis ghost goes down before the parts inside it, which then cover it).
    pub early: bool,
    /// `CULL_CCW` (true) or none.
    pub cull: bool,
    /// `D3DMATERIAL8` diffuse, ambient, specular (RGBA) and power.
    pub diffuse: [f32; 4],
    pub ambient: [f32; 4],
    pub specular: [f32; 4],
    pub power: f32,
}

impl Material {
    /// A plain lit, single-textured, opaque, back-face-culled material: the
    /// engine's defaults (`FUN_005b1a80` and `FUN_005b22f0`) plus one stage.
    pub fn textured(image: Arc<Image>) -> Material {
        Material {
            stages: vec![Stage {
                image,
                kind: StageKind::Modulate,
                linear: true,
                uv_set: 0,
            }],
            ..Material::default()
        }
    }

    /// Alpha-tested materials draw as whole sorted batches.
    pub fn alpha_tested(&self) -> bool {
        self.transparent && !self.no_alpha_test
    }

    /// Blended materials draw as globally depth-sorted triangles.
    pub fn blended(&self) -> bool {
        self.transparent && self.no_alpha_test
    }

    /// Whether the specular term is on (`D3DRS_SPECULARENABLE`: power > 0).
    pub fn specular_enabled(&self) -> bool {
        self.power > 0.0
    }

    /// Build the original's material for a mesh using GMF material `m`.
    ///
    /// `dir` is the model's directory, where bitmaps are searched (also under
    /// `maps/`). The mesh is needed for its vertex-colour and culling flags.
    pub fn from_gmi(m: &gmi::Material, mesh: &gmi::Mesh, dir: &Path) -> Material {
        let has_vcol = !mesh.colors.is_empty();
        let mut mat = Material {
            cull: mesh.backface_cull == 1,
            has_vertex_colors: has_vcol,
            ..Material::default()
        };
        let mut has_diffuse_map = false;
        for t in &m.textures {
            let uv_set = (t.channel - 1).clamp(0, 1) as u32;
            match t.kind {
                gmi::MAP_DIFFUSE | gmi::MAP_SELFILLUM | gmi::MAP_REFLECT => {
                    if t.bitmap.is_empty() {
                        continue;
                    }
                    // A bitmap that fails to load still takes its stage (a NULL
                    // texture): `FUN_005b1e60` queues the entry whatever the
                    // loader returned, and the stage then samples black. The
                    // shipped octagon's `light_flare_diffuse.tga` is such a
                    // file; the flare quads vanish under additive blending.
                    let image = session_bitmap(dir, &t.bitmap).unwrap_or_else(|| {
                        if std::env::var("RA2_DEBUG").is_ok() {
                            eprintln!("  missing bitmap {:?} (black stage)", t.bitmap);
                        }
                        Image::solid([0, 0, 0, 255])
                    });
                    let (kind, linear) = match t.kind {
                        gmi::MAP_DIFFUSE => {
                            has_diffuse_map = true;
                            (StageKind::Modulate, t.filter == 1)
                        }
                        gmi::MAP_SELFILLUM => (StageKind::LightMap, true),
                        _ => (StageKind::Env, true),
                    };
                    if mat.stages.len() < 3 {
                        mat.stages.push(Stage {
                            image,
                            kind,
                            linear,
                            uv_set,
                        });
                    }
                }
                gmi::MAP_OPACITY => {
                    mat.transparent = true;
                    mat.no_alpha_test = t.invert != 1;
                }
                _ => {}
            }
        }
        mat.additive = m.xp_type == 3;
        let alpha = 1.0 - m.transparency;
        let diffuse = if has_diffuse_map {
            [1.0, 1.0, 1.0]
        } else {
            m.diffuse
        };
        mat.diffuse = [diffuse[0], diffuse[1], diffuse[2], alpha];
        mat.power = m.shine * 100.0;
        let s = m.shine_strength;
        mat.specular = [s * m.specular[0], s * m.specular[1], s * m.specular[2], 0.0];
        if has_vcol {
            // Vertex colours are the ambient source and light the batch
            // themselves: lighting is off (`FUN_005ae630`, `batch+0x1a`).
            mat.vertex_color_ambient = true;
            mat.lighting = false;
        } else {
            mat.ambient = [m.ambient[0], m.ambient[1], m.ambient[2], 1.0];
        }
        if alpha < 1.0 {
            mat.transparent = true;
            mat.no_alpha_test = true;
        }
        if mat.stages.iter().any(|s| s.kind == StageKind::LightMap) {
            mat.lighting = false;
        }
        mat
    }
}

impl Default for Material {
    fn default() -> Self {
        Material {
            stages: Vec::new(),
            transparent: false,
            additive: false,
            no_alpha_test: false,
            vertex_color_ambient: false,
            has_vertex_colors: false,
            lighting: true,
            z_write: true,
            z_test: true,
            early: false,
            cull: true,
            diffuse: [1.0; 4],
            ambient: [1.0; 4],
            specular: [1.0; 4],
            power: 0.0,
        }
    }
}

/// Locate a bitmap referenced by a material, under `dir` or `dir/maps`.
pub fn resolve_bitmap(dir: &Path, bitmap: &str) -> Option<PathBuf> {
    let flat = bitmap.replace('\\', "/");
    let file = flat.rsplit('/').next().unwrap_or(&flat).to_string();
    [
        dir.join(bitmap),
        dir.join("maps").join(bitmap),
        dir.join("maps").join(bitmap.to_lowercase()),
        dir.join(&flat),
        dir.join("maps").join(&file),
        dir.join("maps").join(file.to_lowercase()),
    ]
    .into_iter()
    .find(|c| c.exists())
    .or_else(|| ra2fmt::find_ci(&dir.join("maps"), &file))
    .or_else(|| ra2fmt::find_ci(dir, &file))
}

/// The original's global ambient: `SetRenderState(D3DRS_AMBIENT, 0xff262626)`
/// at device setup (`FUN_0058f390`), regardless of the GMF's ambient.
pub const GLOBAL_AMBIENT: [f32; 3] = [38.0 / 255.0; 3];

/// A scene light as the original submits it to `SetLight`.
#[derive(Clone, Debug)]
pub struct Light {
    /// World-space position (the node's translation).
    pub position: Vec3,
    /// `D3DLIGHT8.Range`.
    pub range: f32,
    /// Diffuse colour (the GMF colour times its intensity).
    pub diffuse: [f32; 3],
    /// Specular colour: the GMF colour *not* scaled by the intensity
    /// (`FUN_005ba1e0` writes `D3DLIGHT8.Specular` = colour, `.Diffuse` =
    /// colour x intensity, `.Ambient` = 0).
    pub specular: [f32; 3],
    /// `D3DLIGHT8.Attenuation0..2`.
    pub attenuation: [f32; 3],
}

impl Light {
    /// Convert a GMF light the way `FUN_005ba1e0` does. Only omni lights exist
    /// in the shipped data and only they become `D3DLIGHT_POINT`.
    pub fn from_gmi(object: &gmi::SceneObject, world: Mat4) -> Option<Light> {
        let l = object.light.as_ref()?;
        if l.kind != 0 || l.use_light == 0 {
            return None;
        }
        let pos = world.transform_point3(Vec3::from(object.transform[3]));
        let (range, attenuation) = if l.far_atten && l.attn_end > 0.0 {
            let r = l.attn_end;
            (r, [0.4, 3.0 / r, 40.0 / (r * r)])
        } else {
            (10000.0, [1.0, 0.0, 0.0])
        };
        Some(Light {
            position: pos,
            range,
            diffuse: [
                l.color[0] * l.intensity,
                l.color[1] * l.intensity,
                l.color[2] * l.intensity,
            ],
            specular: l.color,
            attenuation,
        })
    }
}

impl Light {
    /// A distant white key light from above-left-front, for frames that carry
    /// no scene lights. Not part of the original, which lights robots with the
    /// arena's lights; used by the asset viewers and the workshop.
    pub fn viewer_key() -> Light {
        Light {
            position: Vec3::new(-0.4, 0.7, -0.6).normalize() * 5000.0,
            range: 10000.0,
            diffuse: [1.0; 3],
            specular: [0.0; 3],
            attenuation: [1.0, 0.0, 0.0],
        }
    }
}

/// The lights of a scene, in file order.
pub fn scene_lights(scene: &gmi::GmiScene, world: Mat4) -> Vec<Light> {
    scene
        .lights()
        .filter_map(|o| Light::from_gmi(o, world))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-pixel bitmap in a fresh temp directory, returning the directory.
    fn dir_with_bitmaps(names: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ra2-material-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for n in names {
            image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]))
                .save(dir.join(n))
                .unwrap();
        }
        dir
    }

    fn tex(bitmap: &str, kind: i32, filter: i32, channel: i32, invert: i32) -> gmi::Texture {
        gmi::Texture {
            bitmap: bitmap.into(),
            kind,
            filter,
            channel,
            invert,
            ..Default::default()
        }
    }

    fn mesh() -> gmi::Mesh {
        gmi::Mesh {
            backface_cull: 1,
            ..Default::default()
        }
    }

    #[test]
    fn diffuse_filter_flag_picks_point_or_bilinear() {
        let dir = dir_with_bitmaps(&["a.png"]);
        let mut gm = gmi::Material {
            textures: vec![tex("a.png", gmi::MAP_DIFFUSE, 0, 1, 0)],
            diffuse: [0.2, 0.4, 0.6],
            ..Default::default()
        };
        let m = Material::from_gmi(&gm, &mesh(), &dir);
        assert_eq!(m.stages.len(), 1);
        assert!(
            !m.stages[0].linear,
            "Pyramidal samples with point filtering"
        );
        gm.textures = vec![tex("a.png", gmi::MAP_DIFFUSE, 1, 1, 0)];
        let m = Material::from_gmi(&gm, &mesh(), &dir);
        assert!(m.stages[0].linear, "SAT samples bilinearly");
        // A diffuse map turns the material diffuse white.
        assert_eq!(m.diffuse, [1.0; 4]);
        let flat = Material::from_gmi(
            &gmi::Material {
                diffuse: [0.2, 0.4, 0.6],
                ..Default::default()
            },
            &mesh(),
            &dir,
        );
        assert_eq!(flat.diffuse, [0.2, 0.4, 0.6, 1.0]);
    }

    #[test]
    fn opacity_map_alpha_tests_only_when_inverted() {
        let dir = dir_with_bitmaps(&["a.png"]);
        let mut gm = gmi::Material {
            textures: vec![
                tex("a.png", gmi::MAP_DIFFUSE, 1, 1, 0),
                tex("a.png", gmi::MAP_OPACITY, 1, 1, 1),
            ],
            ..Default::default()
        };
        let m = Material::from_gmi(&gm, &mesh(), &dir);
        assert!(m.transparent && m.alpha_tested() && !m.blended());
        // The opacity map loads no bitmap of its own.
        assert_eq!(m.stages.len(), 1);
        gm.textures[1].invert = 0;
        let m = Material::from_gmi(&gm, &mesh(), &dir);
        assert!(m.blended() && !m.alpha_tested());
    }

    #[test]
    fn material_transparency_blends_with_diffuse_alpha() {
        let gm = gmi::Material {
            transparency: 0.65,
            xp_type: 3,
            ..Default::default()
        };
        let m = Material::from_gmi(&gm, &mesh(), Path::new("."));
        assert!(m.blended() && m.additive);
        assert!((m.diffuse[3] - 0.35).abs() < 1e-6);
    }

    #[test]
    fn lightmap_stages_and_vertex_colours_switch_lighting_off() {
        let dir = dir_with_bitmaps(&["a.png", "lm.png"]);
        let gm = gmi::Material {
            textures: vec![
                tex("a.png", gmi::MAP_DIFFUSE, 1, 1, 0),
                tex("lm.png", gmi::MAP_SELFILLUM, 0, 2, 0),
            ],
            ..Default::default()
        };
        let m = Material::from_gmi(&gm, &mesh(), &dir);
        assert!(!m.lighting);
        assert_eq!(m.stages[1].kind, StageKind::LightMap);
        assert_eq!(m.stages[1].uv_set, 1);
        assert!(m.stages[1].linear, "lightmaps are always bilinear");

        let plain = Material::from_gmi(&gmi::Material::default(), &mesh(), &dir);
        assert!(plain.lighting && plain.cull);
        let coloured = gmi::Mesh {
            colors: vec![[1.0; 3]],
            ..mesh()
        };
        let m = Material::from_gmi(&gmi::Material::default(), &coloured, &dir);
        assert!(!m.lighting && m.has_vertex_colors && m.vertex_color_ambient);
    }

    /// docs/34 section 10: the Combat Arena's floor is its tile bitmap times a baked
    /// lightmap on UV set 2, lighting off (the original's draw state, dumped live with
    /// `ra2_d3dstate.js`: `LIGHTING` 0, stage 0 MODULATE, stage 1 MODULATE on set 1, no fog).
    /// The lightmap is bright and neutral at the arena's centre and blue at the corners, so
    /// the floor reads lavender grey in the middle and saturated blue-purple towards the
    /// corners; the same colour at the same place is what the live captures show.
    #[test]
    fn the_box_floor_is_tile_times_a_lightmap_with_lighting_off() {
        let Some(game) = std::env::var_os("RA2_GAME").map(PathBuf::from).or_else(|| {
            let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            loop {
                if d.join("local/game").is_dir() {
                    return Some(d.join("local/game"));
                }
                if !d.pop() {
                    return None;
                }
            }
        }) else {
            return;
        };
        let dir = game.join("Arenas/box");
        let Ok(bytes) = std::fs::read(dir.join("boxarena.gmf")) else {
            return;
        };
        let scene = gmi::parse(&bytes).expect("box arena parses");
        let floor = scene
            .materials
            .iter()
            .find(|m| m.name == "floor with lm")
            .expect("floor material");
        let tile = floor
            .sub_materials
            .iter()
            .find(|m| m.name == "tile")
            .unwrap();
        let mat = Material::from_gmi(tile, &mesh(), &dir);
        assert!(!mat.lighting, "lightmapped floor is unlit");
        assert_eq!(mat.stages.len(), 2);
        assert_eq!(mat.stages[0].kind, StageKind::Modulate);
        assert_eq!(mat.stages[1].kind, StageKind::LightMap);
        assert_eq!(mat.stages[1].uv_set, 1);
        let lm = &mat.stages[1].image;
        let at = |fx: f32, fy: f32| {
            let (x, y) = ((lm.w as f32 * fx) as usize, (lm.h as f32 * fy) as usize);
            let i = (y * lm.w as usize + x) * 4;
            [
                lm.rgba[i] as i32,
                lm.rgba[i + 1] as i32,
                lm.rgba[i + 2] as i32,
            ]
        };
        let centre = at(0.5, 0.5);
        assert!(
            centre[0] > 245 && centre[1] > 245 && centre[2] > 220,
            "{centre:?}"
        );
        let corner = at(0.04, 0.04);
        assert!(corner[2] - corner[0] > 60, "{corner:?}");
    }

    #[test]
    fn an_unloadable_bitmap_keeps_its_stage_and_samples_black() {
        let gm = gmi::Material {
            textures: vec![tex("no_such_file.tga", gmi::MAP_DIFFUSE, 1, 1, 0)],
            ..Default::default()
        };
        let m = Material::from_gmi(&gm, &mesh(), Path::new("."));
        assert_eq!(m.stages.len(), 1);
        assert_eq!(m.stages[0].image.rgba, [0, 0, 0, 255]);
        assert_eq!(m.diffuse[..3], [1.0; 3]);
    }

    #[test]
    fn specular_follows_shine_and_strength() {
        let gm = gmi::Material {
            shine: 0.1,
            shine_strength: 0.5,
            specular: [0.8; 3],
            ..Default::default()
        };
        let m = Material::from_gmi(&gm, &mesh(), Path::new("."));
        assert!((m.power - 10.0).abs() < 1e-4);
        assert!(m.specular_enabled());
        assert!((m.specular[0] - 0.4).abs() < 1e-6);
    }

    #[test]
    fn omni_lights_become_d3d_point_lights() {
        let mut o = gmi::SceneObject::default();
        o.transform[3] = [1.0, 2.0, 3.0];
        o.light = Some(gmi::Light {
            use_light: 1,
            color: [1.0, 0.5, 0.25],
            intensity: 2.0,
            ..Default::default()
        });
        let l = Light::from_gmi(&o, Mat4::IDENTITY).unwrap();
        assert_eq!(l.position, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(l.diffuse, [2.0, 1.0, 0.5]);
        assert_eq!(
            l.specular,
            [1.0, 0.5, 0.25],
            "specular is the colour, unscaled"
        );
        assert_eq!((l.range, l.attenuation), (10000.0, [1.0, 0.0, 0.0]));
        let far = gmi::Light {
            far_atten: true,
            attn_end: 40.0,
            use_light: 1,
            ..Default::default()
        };
        o.light = Some(far);
        let l = Light::from_gmi(&o, Mat4::IDENTITY).unwrap();
        assert_eq!(l.range, 40.0);
        assert_eq!(l.attenuation, [0.4, 3.0 / 40.0, 40.0 / 1600.0]);
    }

    /// Serialises the tests that read or change the global texture path.
    static PATH_LOCK: Mutex<()> = Mutex::new(());

    /// The texture manager hands every later model the first file loaded under
    /// a bitmap name (`FUN_00591030` / `FUN_0058e390`).
    #[test]
    fn the_first_bitmap_loaded_under_a_name_is_the_sessions() {
        let _g = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let base = std::env::temp_dir().join(format!("ra2-session-bitmap-{}", std::process::id()));
        let (a, b) = (base.join("a"), base.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let name = format!("zz_session_{}.png", std::process::id());
        image::RgbaImage::from_pixel(1, 1, image::Rgba([200, 0, 0, 255]))
            .save(a.join(&name))
            .unwrap();
        image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 200, 255]))
            .save(b.join(&name))
            .unwrap();
        let first = session_bitmap(&a, &name).unwrap();
        let second = session_bitmap(&b, &name).unwrap();
        assert_eq!(&second.rgba[..3], &[200, 0, 0]);
        assert!(Arc::ptr_eq(&first, &second));
        assert!(session_bitmap(&b, "zz_missing_bitmap.png").is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The cache key carries the texture path, so a bot-lab request for a bare
    /// name the arena already loaded is a separate entry.
    #[test]
    fn the_session_key_includes_the_texture_path() {
        let _g = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let base = std::env::temp_dir().join(format!("ra2-session-key-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let name = format!("zz_key_{}.png", std::process::id());
        image::RgbaImage::from_pixel(1, 1, image::Rgba([9, 9, 9, 255]))
            .save(base.join(&name))
            .unwrap();
        let other = base.join("lab");
        std::fs::create_dir_all(&other).unwrap();
        image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255]))
            .save(other.join(&name))
            .unwrap();
        let arena = session_bitmap(&base, &name).unwrap();
        set_texture_path("UI\\BotLab\\Maps\\");
        let lab = session_bitmap(&other, &name).unwrap();
        set_texture_path(DEFAULT_TEXTURE_PATH);
        let arena_again = session_bitmap(&base, &name).unwrap();
        assert_eq!(&lab.rgba[..3], &[1, 2, 3]);
        assert_eq!(&arena.rgba[..3], &[9, 9, 9]);
        assert!(Arc::ptr_eq(&arena, &arena_again));
        let _ = std::fs::remove_dir_all(&base);
    }
}
