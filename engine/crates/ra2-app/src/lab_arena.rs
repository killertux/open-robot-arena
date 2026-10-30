//! The Bot Lab's `Test Robot` tab: the robot just built, alone in the practice arena, driven by
//! its own wiring (docs/36 section 9.27).
//!
//! Live (`Scripts/Practice.py`, `FUN_0043ef60`, `ra2_camera.js`): a one-robot DEATHMATCH in
//! `Arenas/practice arena/practice_arena.gmf` with no countdown and no end; the robot starts on the
//! model's start point; the camera is a static one at `(-4.2, 2.5, 9.5)` that looks at the robot
//! and zooms with the distance (`FovRange 2 .. 20`, `0.8976 .. 0.3491`); the `Obstacles` list swaps
//! the `AddXtra` props (`crates`, `cinderblocks`, `barrels`, `cones`, `ramps`); RESET puts the robot
//! and the obstacles back; leaving the tab drops the arena and entering it builds a new one.

use std::path::{Path, PathBuf};

use ra2_render::Renderer;
use ra2script::camera::CameraCommand;
use ra2sim::r#match::{GameType, Match, MatchConfig, RobotEntry};
use ra2sim::{DriveInput, DuelCamera};

use crate::match_mode::Target;

/// The practice arena's script (`AddXtraSound` obstacles), under the game folder.
pub const ARENA_SCRIPT: &str = "Scripts/Practice.py";
/// The static camera the practice mode sets up, and its distance zoom.
pub const CAMERA_EYE: [f32; 3] = [-4.2, 2.5, 9.5];
pub const CAMERA_FOV_RANGE: [f32; 4] = [2.0, 20.0, 0.897_597_9, 0.349_065_85];

/// The obstacle checkboxes of `testarena.gib`, in the order the screen lists them.
pub const OBSTACLES: [(&str, &str); 6] = [
    ("none cb", "none"),
    ("crates cb", "crates"),
    ("blocks cb", "blocks"),
    ("barrels cb", "barrels"),
    ("cones cb", "cones"),
    ("ramps cb", "ramps"),
];

/// The filter a checkbox's name stands for (`none` for no obstacle).
pub fn obstacle_for(checkbox: &str) -> Option<&'static str> {
    OBSTACLES
        .iter()
        .find(|(name, _)| *name == checkbox)
        .map(|(_, filter)| *filter)
}

pub struct LabArena {
    pub matched: Match,
    game: PathBuf,
    bot: PathBuf,
    obstacle: &'static str,
    camera: DuelCamera,
    input: DriveInput,
    accumulator: f32,
    target: Option<(Target, [u32; 2])>,
    /// What the practice arena sounds like: the robot's motors, its weapons, impacts and the
    /// obstacles' own sounds (`AddXtraSound`). No bell, announcer or crowd: the practice mode has
    /// no countdown, no end and no stands.
    sounds: crate::match_sounds::MatchSounds,
    cues: Vec<crate::match_sounds::SoundCue>,
}

impl LabArena {
    /// Build the practice arena with `obstacle` (`"none"`, `"crates"`, ...) and the robot in `bot`.
    pub fn new(game: &Path, bot: &Path, obstacle: &'static str) -> Result<LabArena, String> {
        std::env::set_var("RA2_HAZARDS", "0");
        std::env::set_var("RA2_OBSTACLE", obstacle);
        // The robot is a temporary file outside the game folder; the loaders find the game by this.
        let had_game = std::env::var_os("RA2_GAME");
        std::env::set_var("RA2_GAME", game);
        let arena = game.join(ARENA_SCRIPT);
        let config = MatchConfig::new(
            arena.to_string_lossy().to_string(),
            GameType::Deathmatch,
            vec![RobotEntry::human(0, bot.to_string_lossy().to_string())],
        )
        .with_practice();
        let built = Match::new(config).map_err(|e| e.to_string());
        std::env::remove_var("RA2_OBSTACLE");
        match had_game {
            Some(value) => std::env::set_var("RA2_GAME", value),
            None => std::env::remove_var("RA2_GAME"),
        }
        let mut matched = built?;
        // The camera follows the player's robot (`0x4014b0(0)`).
        matched.duel.human_robot = Some(0);
        {
            use ra2sim::camera::SceneCameraKind;
            let camera = &mut matched.duel.scene_camera;
            camera.select(SceneCameraKind::Watch);
            camera.apply(CameraCommand::Position(CAMERA_EYE));
            camera.apply(CameraCommand::FovRange(CAMERA_FOV_RANGE));
        }
        Ok(LabArena {
            matched,
            game: game.to_path_buf(),
            bot: bot.to_path_buf(),
            obstacle,
            camera: DuelCamera::Watch,
            input: DriveInput::default(),
            accumulator: 0.0,
            target: None,
            sounds: crate::match_sounds::MatchSounds::new(),
            cues: Vec::new(),
        })
    }

    pub fn obstacle(&self) -> &'static str {
        self.obstacle
    }

    /// The `Obstacles` list picked `obstacle`: a new arena with the robot back on its start point.
    pub fn set_obstacle(&mut self, obstacle: &'static str) -> Result<(), String> {
        let keep = self.input;
        *self = LabArena::new(&self.game, &self.bot, obstacle)?;
        self.input = keep;
        Ok(())
    }

    /// RESET: the robot and the obstacles back where they started.
    pub fn reset(&mut self) -> Result<(), String> {
        self.set_obstacle(self.obstacle)
    }

    /// The scan codes the player holds, for the robot's own wiring.
    pub fn set_drive(&mut self, input: DriveInput) {
        self.input = input;
    }

    pub fn drive(&self) -> DriveInput {
        self.input
    }

    /// Run the fixed steps `seconds` of wall time covers.
    pub fn advance(&mut self, seconds: f32) {
        let dt = self.matched.config.dt;
        self.accumulator += seconds.min(0.25);
        while self.accumulator >= dt {
            self.accumulator -= dt;
            self.matched.step_with(&[Some(self.input)]);
            let cues = self.sounds.observe(&mut self.matched);
            self.cues
                .extend(cues.into_iter().filter(Self::is_practice_cue));
        }
    }

    /// The cues the practice arena plays: the robot's and the props' sounds, not the match's
    /// ceremony (`Practice.py` starts no countdown, announcer, crowd or chat).
    fn is_practice_cue(cue: &crate::match_sounds::SoundCue) -> bool {
        use crate::match_sounds::SoundCue::*;
        !matches!(
            cue,
            Beep { .. }
                | Chat
                | Announce { .. }
                | ImmobileTick
                | CrowdStop
                | CrowdLoop { .. }
                | CrowdOneShot { .. }
                | CrowdLoopShout { .. }
                | CrowdFadeShout { .. }
        )
    }

    /// The sounds the steps since the last call made.
    pub fn take_cues(&mut self) -> Vec<crate::match_sounds::SoundCue> {
        std::mem::take(&mut self.cues)
    }

    /// The ears: the practice camera's eye, looking at the robot.
    pub fn audio_listener(&self) -> ra2_audio::Listener {
        let (eye, aim) = self.matched.duel.camera_pose(&self.camera);
        let direction = aim - eye;
        let forward = if direction.length_squared() > 1e-8 {
            direction.normalize()
        } else {
            glam::Vec3::Z
        };
        ra2_audio::Listener::new(
            [eye.x, eye.y, eye.z],
            [forward.x, forward.y, forward.z],
            [0.0, 1.0, 0.0],
        )
    }

    /// The 3D view at `size` as RGBA pixels.
    pub fn frame(&mut self, renderer: &Renderer, size: [u32; 2]) -> Result<Vec<u8>, String> {
        if self.target.as_ref().map(|(_, s)| *s) != Some(size) {
            self.target = Some((Target::new(renderer.device(), size[0], size[1]), size));
        }
        // The arena's own lights plus the lab camera's headlight (live: two `Omni` point lights of
        // the model and a third white one, range 1000, at the camera's eye).
        let mut lights: Vec<ra2_render::Light> = self
            .matched
            .duel
            .render_groups()
            .iter()
            .flat_map(|g| g.items.iter())
            .find_map(|i| i.scene_lights.as_ref())
            .map(|l| l.to_vec())
            .unwrap_or_default();
        lights.push(ra2_render::Light {
            position: glam::Vec3::from(CAMERA_EYE),
            range: 1000.0,
            diffuse: [1.0; 3],
            specular: [1.0; 3],
            attenuation: [1.0, 0.0, 0.0],
        });
        renderer.set_lights(&lights);
        let (target, _) = self.target.as_ref().expect("target");
        self.matched
            .duel
            .render_to(
                renderer,
                &self.camera,
                &target.color_view,
                &target.depth_view,
                wgpu::TextureFormat::Rgba8Unorm,
                size[0],
                size[1],
            )
            .map_err(|e| e.to_string())?;
        renderer
            .read_texture(&target.color, size[0], size[1])
            .map_err(|e| e.to_string())
    }
}
