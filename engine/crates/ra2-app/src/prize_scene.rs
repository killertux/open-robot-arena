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
//! a timer: the prize does not turn. The scene is a still but for the team's
//! robot, a free physics body (see [`PrizeScene::advance`]). Lighting
//! is the arena model's two omni lights, read by the renderer from the first
//! batch.

use std::path::Path;
use std::sync::Arc;

use glam::{Mat4, Vec3};
use ra2_render::{DrawItem, Image, RenderGroup, Renderer};
use ra2sim::camera::{SceneCamera, SceneCameraKind};
use ra2sim::{BotSetup, DriveInput, Duel};

/// The physics step of the standing robot (the duel's own, 60 Hz).
const STEP: f32 = 1.0 / 60.0;
/// How much higher the original drops the pad robot from than the duel's own
/// spawn (docs/36 section 9.23): live, the chassis starts 0.226 m above where
/// it rests, ours 0.104 m (the same measure; the match spawn is lower by about
/// as much, which is open).
const SPAWN_LIFT: f32 = 0.122;

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
    /// The team's robot on the winner's pad: a free body of the running arena
    /// (see [`PrizeScene::advance`]).
    duel: Option<Box<Duel>>,
    /// Wall-clock seconds not yet stepped.
    pending: f32,
    camera: SceneCamera,
}

impl PrizeScene {
    /// Build the scene from the game install. `prize` is the tier's model path
    /// as `Events.txt` writes it (`Arenas\event_results\prizes\...gmf`); a tier
    /// without a model (the lowest tier of an event names only the directory)
    /// leaves the spot empty, as an `AddXtra` of a missing file does. `logo` is
    /// the team's logo path (`Teams\...bmp`).
    pub fn load(
        game: &Path,
        prize: &str,
        logo: &str,
        robot: Option<&Path>,
    ) -> Result<PrizeScene, String> {
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
        let duel = robot.and_then(|path| drop_robot(&script, path));
        Ok(PrizeScene {
            arena,
            prize: prize_items,
            duel,
            pending: 0.0,
            camera,
        })
    }

    /// Advance the pad robot by wall-clock seconds. The robot is a player of the
    /// running `Event Results` arena (`FUN_0046e710`); nothing drives it, so it
    /// drops onto the pad and then rocks on its free wheels, its yaw wandering
    /// by a few hundredths of a radian a minute: live, 50 s samples of the
    /// award robot's heading moved by 0.0 to 0.1 rad (docs/36 section 9.23).
    pub fn advance(&mut self, seconds: f32) {
        let Some(duel) = self.duel.as_mut() else {
            return;
        };
        self.pending += seconds.clamp(0.0, 0.5);
        while self.pending >= STEP {
            self.pending -= STEP;
            duel.step_direct(&[DriveInput::default()], STEP);
        }
    }

    /// The robot's chassis heading (`atan2(x, z)` of its forward axis), as
    /// `plus.getHeading`, and its position; `None` without a robot.
    pub fn robot_pose(&self) -> Option<(f32, Vec3)> {
        let duel = self.duel.as_ref()?;
        let robot = duel.robots.first()?;
        let state = duel.physics.state(robot.body())?;
        let forward = state.rotation * Vec3::Z;
        Some((forward.x.atan2(forward.z), state.position))
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
        let mut groups = vec![
            RenderGroup {
                items: &self.arena,
                model: Mat4::IDENTITY,
            },
            RenderGroup {
                items: &self.prize,
                model: Mat4::IDENTITY,
            },
        ];
        if let Some(duel) = &self.duel {
            groups.extend(duel.robot_groups());
        }
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

/// The team's robot dropped onto the arena's first start point: `FUN_0046e710`
/// adds the entered robot as a real player of the running arena (the match's
/// own spawn, `FUN_00439be0`), so it stands at `startpoint` 0 facing along the
/// point's Z axis (live: heading 2.7925 = 160 degrees at the first frame) and
/// then lives in the arena's physics. `None` when the robot cannot be built.
fn drop_robot(script: &Path, bot: &Path) -> Option<Box<Duel>> {
    let mut duel = Duel::new(
        &script.to_string_lossy(),
        &[BotSetup {
            path: bot.to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        }],
    )
    .ok()?;
    let bodies: Vec<_> = duel.robots.first()?.parts.iter().map(|p| p.body).collect();
    for body in bodies {
        let state = duel.physics.state(body)?;
        duel.physics.set_transform(
            body,
            state.position + Vec3::new(0.0, SPAWN_LIFT, 0.0),
            state.rotation,
        );
    }
    Some(Box::new(duel))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn game_dir() -> Option<PathBuf> {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join("local/game");
            if candidate.is_dir() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// Live (docs/36 section 9.23): the pad robot starts at the start point's
    /// heading (2.7925 rad, 160 degrees), falls onto the pad about 0.22 m and
    /// settles; nothing drives it, so it creeps on its free wheels: live the
    /// heading of C.R.U.S.H. moved by 0.0 to 0.1 rad in 50 s and Atom Smasher's by
    /// -0.0145 rad/s steadily. It never turns fast and stays on the pad.
    #[test]
    fn the_award_robot_drops_settles_and_creeps() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let bot = game.join("Robot Designs/XTRA_Atom Smasher.bot");
        if !bot.is_file() {
            return;
        }
        let mut scene = PrizeScene::load(&game, "Arenas\\event_results\\prizes", "", Some(&bot))
            .expect("the scene loads");
        let (heading, start) = scene.robot_pose().expect("a robot is on the pad");
        assert!((heading - 2.7925).abs() < 0.001, "start heading {heading}");
        for _ in 0..180 {
            scene.advance(1.0 / 60.0);
        }
        let (_, rest) = scene.robot_pose().unwrap();
        assert!(
            start.y - rest.y > 0.15,
            "it drops about 0.22 m: {} -> {}",
            start.y,
            rest.y
        );
        let before = scene.robot_pose().unwrap().0;
        let seconds = 30;
        for _ in 0..(seconds * 60) {
            scene.advance(1.0 / 60.0);
        }
        // The turn over the window, wrapped (the heading sits near pi).
        let turned = (scene.robot_pose().unwrap().0 - before + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let creep = turned.abs() / seconds as f32;
        assert!(creep < 0.03, "it creeps by {creep} rad/s at most");
        let (_, end) = scene.robot_pose().unwrap();
        assert!((end.y - rest.y).abs() < 0.05, "it stays on the pad");
        assert!(
            (Vec3::new(end.x, 0.0, end.z) - Vec3::new(rest.x, 0.0, rest.z)).length() < 0.5,
            "it stays on the spot"
        );
    }
}
