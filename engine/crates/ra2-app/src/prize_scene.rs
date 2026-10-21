//! The award screen's 3D scene: the event-results spot with the prize on it.
//!
//! `FUN_00416960` (the event-winner opener, reached from the bracket's round
//! hook `FUN_004164a0` once the event is over) builds `teamhq/event_winner.gib`
//! (`FUN_0046def0`, `EventResultsWindow`, 0x184 bytes) and then calls
//! `FUN_0046e140(1, ...)`, which starts the **`Event Results` arena** under the
//! window and fills it in, in this order:
//!
//! * `FUN_0046e610`: `AddXtra("prize", <model>, "metal")` on the running arena,
//!   `<model>` being the event record's tier string: the record keeps its
//!   `tiers + 1` model/message pairs from `+0x6c` in 0x10-byte slots
//!   (`FUN_0046e6e0`), and the window's `+0x178` picks the slot, the rounds the
//!   team won, one more (the trophy) when it won the event
//!   (`FUN_004164a0`: `FUN_0046ce50` set and `+0x70 == 0` adds 1).
//! * `FUN_0046eb70`: finds the arena object `banner Instance` and its `banner`
//!   batch, empties the batch's texture stages (`FUN_005b2270`) and adds one
//!   whose texture is the team's logo (`FUN_00494a10` is the team's `+0x28`
//!   string, `FUN_005b1e60` the stage) - the team's picture hangs on the wall.
//! * the arena itself is `Arenas/event_results.py`: `event_results.gmf`, black
//!   back colour, DEATHMATCH only, and one `AddStaticCamera("fixed cam",
//!   (0, 3.95, -11.88), (0.206, 0), 45 * .015)`.
//!
//! The prize `.gmf` files are authored in place (the node translations of
//! `Gold_Cup_Trophy.gmf` and the others sit on the arena's winner spot, 2.31,
//! 0.27, -1.89), so an `AddXtra` needs no placement of its own, and the
//! window's class overrides no per-frame function (its vtable `0x6a73ac` holds
//! only the destructor `0x46e110` of its own), the script's `Tick` only counts
//! a timer: the prize does not turn. The scene is therefore a still. Lighting
//! is the arena model's two omni lights, read by the renderer from the first
//! batch.

use std::path::Path;
use std::sync::Arc;

use glam::Mat4;
use ra2_render::{DrawItem, Image, RenderGroup, Renderer};
use ra2sim::camera::{SceneCamera, SceneCameraKind};

/// `Arenas/event_results.py`: `AddStaticCamera("fixed cam", ...)`.
pub const CAMERA_POSITION: [f32; 3] = [0.0, 3.95, -11.88];
/// `(pitch, yaw)`; `StaticCamera.apply` passes `rotation[0], rotation[1]`.
pub const CAMERA_ROTATION: [f32; 2] = [0.206, 0.0];
/// `45 * .015` radians.
pub const CAMERA_FOV: f32 = 45.0 * 0.015;

/// The event-results arena plus its prize, ready to draw.
pub struct PrizeScene {
    arena: Vec<DrawItem>,
    prize: Vec<DrawItem>,
    camera: SceneCamera,
}

impl PrizeScene {
    /// Build the scene from the game install. `prize` is the tier's model path
    /// as `Events.txt` writes it (`Arenas\event_results\prizes\...gmf`); a tier
    /// without a model (the lowest tier of an event names only the directory)
    /// leaves the spot empty, as an `AddXtra` of a missing file does. `logo` is
    /// the team's logo path (`Teams\...bmp`).
    pub fn load(game: &Path, prize: &str, logo: &str) -> Result<PrizeScene, String> {
        let script = ra2_render::resolve_ci(game, "Arenas/event_results.py")
            .ok_or("Arenas/event_results.py is not in the install")?;
        let mut arena = ra2_render::collect_items(&script.to_string_lossy())
            .map_err(|e| format!("the event-results arena: {e}"))?;
        let prize_items = if prize.to_ascii_lowercase().ends_with(".gmf") {
            match ra2_render::resolve_ci(game, prize) {
                Some(path) => ra2_render::collect_items(&path.to_string_lossy())
                    .map_err(|e| format!("{prize}: {e}"))?,
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };
        if let Some(image) =
            ra2_render::resolve_ci(game, logo).and_then(|p| ra2_render::material::load_image(&p))
        {
            set_banner(&mut arena, game, image);
        }
        let mut camera = SceneCamera::new(SceneCameraKind::Static);
        camera.position = glam::Vec3::from(CAMERA_POSITION);
        camera.rotation = [CAMERA_ROTATION[0], CAMERA_ROTATION[1], 0.0];
        camera.fov = CAMERA_FOV;
        Ok(PrizeScene {
            arena,
            prize: prize_items,
            camera,
        })
    }

    /// Whether a prize model is on the spot.
    pub fn has_prize(&self) -> bool {
        !self.prize.is_empty()
    }

    /// Draw the scene into a caller's target (the back colour is black,
    /// `plus.setBackColor(0, 0, 0)`).
    pub fn render(
        &self,
        renderer: &Renderer,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        w: u32,
        h: u32,
    ) -> Result<(), String> {
        renderer.set_background([0.0, 0.0, 0.0]);
        let view_proj = self.camera.view_proj(w as f32 / h as f32);
        let view_proj = ra2_render::d3d_pixel_centers(view_proj, w, h);
        let groups = [
            RenderGroup {
                items: &self.arena,
                model: Mat4::IDENTITY,
            },
            RenderGroup {
                items: &self.prize,
                model: Mat4::IDENTITY,
            },
        ];
        renderer
            .render_groups_to(&groups, view_proj, color, depth, format)
            .map_err(|e| e.to_string())
    }
}

/// Swap the banner's texture for the team's logo: the arena's `teamlogo`
/// material is the one drawn with `spikeball.bmp` (the placeholder skull).
fn set_banner(arena: &mut [DrawItem], game: &Path, logo: Arc<Image>) {
    let dir = ra2_render::resolve_ci(game, "Arenas/event_results/event_results.gmf")
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let placeholder = dir.and_then(|d| ra2_render::material::session_bitmap(&d, "spikeball.bmp"));
    for item in arena {
        for stage in &mut item.material.stages {
            if placeholder
                .as_ref()
                .is_some_and(|p| Arc::ptr_eq(p, &stage.image))
            {
                stage.image = logo.clone();
            }
        }
    }
}
