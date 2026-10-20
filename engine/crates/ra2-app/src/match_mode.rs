//! The match on screen: `--match`, its 3D frame, HUD, camera, results, pause.
//!
//! The rules are [`ra2sim::r#match`]'s; this module is presentation only. One
//! frame is composed in the same order the original draws it: the duel's 3D
//! scene into the target, then the shipped HUD `.gib` over it, then — when the
//! match is over — `common/matchresults.gib`. The pause dialog is
//! `common/ingame_dialog.gib` (root `in game popup win`), whose own notes name
//! `resume button` and `quit button`.
//!
//! ## Offscreen frames
//!
//! `--match --seconds N --out DIR` runs the match at its own `dt` and writes
//! `countdown.png`, `fight-first.png`, `fight-mid.png`, `fight-last.png` and
//! `results.png`. The fight is a real one; if neither chassis is destroyed by
//! `N`, the **evidence script** ends it through `plus.eliminatePlayer`'s own
//! request (the state-3 path `flextop.py:188` uses) for the robot with less
//! hitpoints left. That is a script, not a game rule, and it is printed.
//!
//! `--quit-at N` opens the pause dialog at `N` seconds, writes `pause.png`,
//! presses its `quit button` through the same hit test a mouse uses, and writes
//! `menu.png` from a fresh shell — the deterministic pause/quit proof.

use std::path::{Path, PathBuf};

use ra2_render::ui::{UiAssets, UiRenderer, UiScene};
use ra2_render::Renderer;
use ra2fmt::gib::{Block, Gib, Widget};
use ra2sim::camera::SceneCameraKind;
use ra2sim::r#match::{Match, MatchConfig, MatchReport, MatchState, RobotEntry};
use ra2sim::{DriveInput, Duel, DuelCamera, GameType};

use crate::hud;
use crate::SHELL_SIZE;

/// The default arena, game type and the two shipped AI brains the match tests
/// use.
pub const ARENA: &str = "Arenas/box.py";
pub const BOTS: [&str; 2] = ["AI/Team1/Bot0.bot", "AI/Team6/Bot2.bot"];
/// The exhibition player's robot: the career/team table's team 0, robot 0.
pub const PLAYER_BOT: &str = "Teams/Team0/Bot0.bot";
/// The exhibition opponent: the first shipped AI team's first robot.
pub const OPPONENT_BOT: &str = "AI/Team1/Bot0.bot";
/// The arena picker's default when no row is selected (the game's own
/// `Events.txt` name for `box.py` is `Combat Arena`).
pub const DEFAULT_ARENA_SCRIPT: &str = "box.py";
/// How long the offscreen evidence fight runs before its scripted decision.
pub const DEFAULT_SECONDS: f32 = 30.0;

/// How long `camera textbox` stays up after a camera change, in seconds.
///
/// `FUN_00477180` shows the box and writes `+0x17c = 0x40400000` (3.0) at
/// `0x4771a2`; the per-frame clock at `0x4774f1` decrements it and hides the
/// box (`+0x178`) when it reaches zero.
pub const CAMERA_LABEL_SECONDS: f32 = 3.0;

/// What `--match` was asked to do.
#[derive(Clone, Debug)]
pub struct Options {
    pub game: PathBuf,
    pub arena: PathBuf,
    pub bots: Vec<PathBuf>,
    pub seconds: f32,
    pub out: Option<PathBuf>,
    pub quit_at: Option<f32>,
    pub size: [u32; 2],
}

impl Options {
    pub fn new(
        game: &Path,
        arena: Option<PathBuf>,
        bots: Vec<PathBuf>,
        seconds: Option<f32>,
        out: Option<PathBuf>,
        quit_at: Option<f32>,
        size: [u32; 2],
    ) -> Result<Options, String> {
        let bots = if bots.is_empty() {
            BOTS.iter().map(|p| game.join(p)).collect()
        } else {
            bots
        };
        if bots.len() != 2 {
            return Err(format!(
                "--bots wants two .bot paths (a DEATHMATCH is two robots), got {}",
                bots.len()
            ));
        }
        for bot in &bots {
            if !bot.is_file() {
                return Err(format!("{} is not a .bot file", bot.display()));
            }
        }
        let seconds = seconds.unwrap_or(DEFAULT_SECONDS);
        if !(seconds.is_finite() && seconds > 0.0) {
            return Err(format!("--seconds must be positive, got {seconds}"));
        }
        if let Some(quit) = quit_at {
            if !(quit.is_finite() && quit >= 0.0 && quit < seconds) {
                return Err(format!(
                    "--quit-at {quit} must be before the scripted decision at {seconds}s"
                ));
            }
        }
        Ok(Options {
            game: game.to_path_buf(),
            arena: arena.unwrap_or_else(|| game.join(ARENA)),
            bots,
            seconds,
            out,
            quit_at,
            size,
        })
    }

    fn match_config(&self) -> MatchConfig {
        MatchConfig::new(
            self.arena.to_string_lossy().to_string(),
            GameType::Deathmatch,
            self.bots
                .iter()
                .enumerate()
                .map(|(team, path)| RobotEntry::ai(team, path.to_string_lossy().to_string()))
                .collect(),
        )
        .with_duration(Some(self.seconds))
    }
}

/// The camera from `ra2.cfg`'s `DefaultCamera` (`watch`, `static`, `orbit`).
/// An unknown value is reported, not silently replaced.
pub fn camera_from_cfg(game: &Path) -> Result<DuelCamera, String> {
    let cfg = crate::options::read_cfg(game);
    let name = cfg.default_camera();
    DuelCamera::from_config(name).ok_or_else(|| {
        format!(
            "{name:?} in {} is not watch/static/orbit",
            crate::options::settings_path(game).display()
        )
    })
}

/// The scene-camera configuration a match camera mode asks for, if it is one of
/// the original's three.
fn scene_kind(camera: &DuelCamera) -> Option<SceneCameraKind> {
    match camera {
        DuelCamera::Static => Some(SceneCameraKind::Static),
        DuelCamera::Watch => Some(SceneCameraKind::Watch),
        DuelCamera::Orbit { .. } => Some(SceneCameraKind::Orbit),
        _ => None,
    }
}

/// The driving camera: behind the player's heading, scaled to the robot.
///
/// This is **our choice**, not one of the binary's three config cameras
/// (`watch`/`static`/`orbit`): a chase frame keeps the robot the player is
/// steering at a readable size, and `C` still reaches the shipped vocabulary.
pub fn chase_camera(duel: &Duel, robot: usize) -> DuelCamera {
    let robot = robot.min(duel.robots.len().saturating_sub(1));
    let size = duel
        .robots
        .get(robot)
        .map(|r| ((r.rig.max - r.rig.min) * 0.5).length())
        .unwrap_or(1.0)
        .max(0.4);
    DuelCamera::Chase {
        robot,
        distance: size * 2.6 + 0.8,
        height: size * 1.9 + 0.5,
    }
}

/// A live match plus its screens. The window loop and the offscreen runner
/// share this, so the pictures cannot diverge.
pub struct Session {
    pub matched: Match,
    pub camera: DuelCamera,
    pub paused: bool,
    /// Whether this match has a local player driving robot 0. The exhibition
    /// flow sets it; `--match` and the HQ events are AI-versus-AI and leave it
    /// false, so their camera sequence stays the config's three modes.
    pub player_controlled: bool,
    /// The robot the player drives (0 in this client).
    pub player_robot: usize,
    /// The keyboard input sampled at the latest tick.
    player_input: DriveInput,
    /// Accumulates wall-clock time for [`Session::advance`].
    accumulator: f32,
    /// Seconds left on `camera textbox`'s 3.0 s display window.
    camera_label_left: f32,
    /// The name the scene camera last reported (`"Static 1"`, `"Watch 1"`,
    /// from the arena's `Next*Camera`): the original puts it in the textbox
    /// instead of the class default.
    camera_name: Option<String>,
    /// The camera the match settles on once the intro is over: the config's
    /// `DefaultCamera` (or the chase frame when a player is driving).
    default_camera: DuelCamera,
    /// The chat group is open (backslash). Local only; nothing is sent.
    chat_open: bool,
    chat_entry: String,
    /// The pause dialog's widgets, loaded once.
    pub dialog: Gib,
    presentation: Presentation,
    /// What the match sounds like: impacts, countdown, motors.
    match_sounds: crate::match_sounds::MatchSounds,
    /// Cues waiting for the audio drain.
    cues: Vec<crate::match_sounds::SoundCue>,
}

/// The values the HUD needs that are not part of the match report: the camera
/// name (and its 3 s window) and the local chat state.
/// The effect of an Esc key press, see [`Session::escape`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escape {
    SkippedIntro,
    Paused,
    Resumed,
    Ignored,
}

struct ScreenBind {
    camera: String,
    show_camera: bool,
    chat_open: bool,
    chat_entry: String,
}

struct Presentation {
    ui_root: PathBuf,
    assets: UiAssets,
    hud: Gib,
    results: Option<Gib>,
    /// The screen the windows lay out on: the target's own size. The original
    /// lays every GUI window out natively at the display resolution
    /// (`docs/28` §1), in the match as in the shell, so this is the window or
    /// PNG size and the scenes need no scaling.
    layout: [f32; 2],
}

impl Presentation {
    fn open(game: &Path) -> Result<Presentation, String> {
        let ui_root = game.join("UI");
        let assets = UiAssets::open(game).map_err(|e| e.to_string())?;
        let hud = hud::load(&ui_root)?;
        Ok(Presentation {
            ui_root,
            assets,
            hud,
            results: None,
            layout: SHELL_SIZE,
        })
    }

    fn scene_size(&self) -> [u32; 2] {
        [self.layout[0].round() as u32, self.layout[1].round() as u32]
    }

    fn root_mut(&mut self) -> Option<&mut Widget> {
        self.hud.blocks.iter_mut().find_map(|b| match b {
            Block::Widget(w) => Some(w),
            _ => None,
        })
    }

    /// Bind the HUD and return its placed scene.
    fn hud_scene(
        &mut self,
        report: &MatchReport,
        state: MatchState,
        clock: f32,
        countdown_left: f32,
        bind: &ScreenBind,
    ) -> Result<(hud::Hud, UiScene), String> {
        let Some(root) = self.root_mut() else {
            return Err("gamewindow.gib has no root widget".into());
        };
        let bound = hud::hud(root, report, state, clock, countdown_left);
        hud::set_camera(root, &bind.camera, bind.show_camera);
        hud::set_chat(root, bind.chat_open, &bind.chat_entry);
        let placed = ra2ui::resolve_with(&self.hud, self.layout, Some(self.assets.faces()));
        // The scene's coordinates are the design screen, not the target: the
        // renderer's `fit` is then the identity (layout == target).
        let size = self.scene_size();
        let scene = self.assets.scene(&placed, size);
        Ok((bound, scene))
    }

    /// Build (once) and return the results screen's scene.
    fn results_scene(&mut self, report: &MatchReport) -> Result<UiScene, String> {
        if self.results.is_none() {
            let template = ra2ui::cells::template(&self.ui_root, ra2ui::results::TEMPLATE)?;
            let mut screen = ra2ui::workshop::load(&self.ui_root, "common/matchresults.gib")?;
            let root = screen
                .blocks
                .iter_mut()
                .find_map(|b| match b {
                    Block::Widget(w) => Some(w),
                    _ => None,
                })
                .ok_or_else(|| "matchresults.gib has no root widget".to_string())?;
            let results = report.results();
            let filled = ra2ui::results::fill(root, &template, &results);
            // The cells name their own snapshots by content-addressed key; put
            // every decoded picture in the asset set before the scene resolves.
            for result in &results {
                if let Some(preview) = &result.bot_preview_image {
                    self.assets.add_preview(&result.bot_preview, preview);
                }
            }
            println!(
                "results screen: {filled} of {} bot(s) placed",
                report.robots.len()
            );
            self.results = Some(screen);
        }
        let screen = self.results.as_ref().unwrap();
        let placed = ra2ui::resolve_with(screen, self.layout, Some(self.assets.faces()));
        Ok(self.assets.scene(&placed, self.scene_size()))
    }

    /// The HUD with the pause dialog over it.
    fn pause_scene(
        &mut self,
        report: &MatchReport,
        state: MatchState,
        clock: f32,
        countdown_left: f32,
        bind: &ScreenBind,
        dialog: &Gib,
    ) -> Result<UiScene, String> {
        let Some(root) = self.root_mut() else {
            return Err("gamewindow.gib has no root widget".into());
        };
        hud::hud(root, report, state, clock, countdown_left);
        hud::set_camera(root, &bind.camera, bind.show_camera);
        hud::set_chat(root, bind.chat_open, &bind.chat_entry);
        let mut placed = ra2ui::resolve_with(&self.hud, self.layout, Some(self.assets.faces()));
        let dialog_placed = ra2ui::resolve_with(dialog, self.layout, Some(self.assets.faces()));
        placed.extend(dialog_placed);
        Ok(self.assets.scene(&placed, self.scene_size()))
    }
}

impl Session {
    pub fn new(options: &Options) -> Result<Session, String> {
        // The shipped hazards move the box fight around; the duel's own tests
        // turn them off for the deterministic presentation run as well. The
        // environment is read while the world is built, so this must come
        // before `Match::new`.
        // `RA2_MATCH_HAZARDS=1` keeps them on, for the audio comparisons against
        // the live exhibition captures (hazards enabled: the intro names them).
        let hazards = std::env::var("RA2_MATCH_HAZARDS").is_ok_and(|v| v == "1");
        std::env::set_var("RA2_HAZARDS", if hazards { "1" } else { "0" });
        let matched = Match::new(options.match_config()).map_err(|e| e.to_string())?;
        let mut session = Session::with_match(&options.game, matched)?;
        session.set_layout(options.size);
        Ok(session)
    }

    /// [`Session::new`] with the player driving robot 0: the workshop's
    /// `Test Robot`, where the player tries the robot they just built with its
    /// own wiring.
    pub fn new_player(options: &Options) -> Result<Session, String> {
        std::env::set_var("RA2_HAZARDS", "0");
        let matched = Match::new(options.match_config()).map_err(|e| e.to_string())?;
        let mut session = Session::with_match_controlled(&options.game, matched, true)?;
        session.set_layout(options.size);
        Ok(session)
    }

    /// A live match and its shipped screens from an already-built [`Match`].
    /// This is the same presentation the `--match` path uses; the Team HQ's
    /// event flow supplies a match whose arena and game type come from
    /// `Events.txt` and the arena's own `game_types` instead of the fixed
    /// `--match` defaults.
    pub fn with_match(game: &Path, matched: Match) -> Result<Session, String> {
        // The shipped hazards move the box fight around; the duel's own tests
        // turn them off for the deterministic presentation run as well.
        std::env::set_var("RA2_HAZARDS", "0");
        Session::with_match_controlled(game, matched, false)
    }

    /// [`Session::with_match`] with the player driving robot 0: the Team HQ's
    /// event match, which the original plays with the player's own robot under
    /// the keyboard (chase camera, Esc pauses), exactly like an exhibition.
    pub fn with_match_player(game: &Path, matched: Match) -> Result<Session, String> {
        std::env::set_var("RA2_HAZARDS", "0");
        Session::with_match_controlled(game, matched, true)
    }

    /// Start the exhibition match the shipped setup screen chose.
    ///
    /// `entries` are the robots the four `playerN group` slots selected, with
    /// the team ids the mode assigns (slot 1 is entry 0, the driven robot).
    /// `hazards` is the arena picker's own `hazard checkbox`; `None` means the
    /// arena script did not expose one and the environment is left alone. The
    /// environment is read while the world is built, so this must happen before
    /// [`Match::new`].
    #[allow(clippy::too_many_arguments)]
    pub fn exhibition(
        game: &Path,
        arena: &Path,
        game_type: GameType,
        entries: Vec<RobotEntry>,
        seconds: Option<f32>,
        hazards: Option<bool>,
    ) -> Result<Session, String> {
        match hazards {
            Some(true) => std::env::set_var("RA2_HAZARDS", "1"),
            Some(false) => std::env::set_var("RA2_HAZARDS", "0"),
            None => {}
        }
        let config = MatchConfig::new(arena.to_string_lossy().to_string(), game_type, entries)
            .with_duration(seconds);
        let matched = Match::new(config).map_err(|e| e.to_string())?;
        Session::with_match_controlled(game, matched, true)
    }

    fn with_match_controlled(
        game: &Path,
        matched: Match,
        player_controlled: bool,
    ) -> Result<Session, String> {
        let default_camera = if player_controlled {
            chase_camera(&matched.duel, 0)
        } else {
            camera_from_cfg(game)?
        };
        let mut matched = matched;
        if player_controlled {
            matched.duel.human_robot = Some(0);
        }
        // The match shell starts the arena's own intro; `ArenaIntros` is the
        // shipped switch. The generator advances on the arena's `Tick`, so its
        // music, crowd, announcer and camera requests reach the app's audio
        // drain. The intro flies the scene camera in its static configuration
        // (the HUD label reads `Static 1`); when it ends the config's camera
        // takes over. Every priced contact is a sound (`FUN_0045f3a0`); the
        // session drains them each step into cues.
        matched.duel.record_impacts = true;
        let cfg = crate::options::read_cfg(game);
        // `FUN_0046e5e0() == 1`: sparks and motor smoke are the `GraphicEffects`
        // option's.
        matched.duel.fx.enabled = cfg.graphic_effects();
        let mut camera = default_camera;
        let mut camera_name = None;
        if cfg.arena_intros() && std::env::var("RA2_ARENA_INTROS").as_deref() != Ok("0") {
            camera = DuelCamera::Static;
            camera_name = matched.duel.select_scene_camera(SceneCameraKind::Static);
            match ra2script::arena_intro(true) {
                Ok(()) => {
                    if ra2script::camera::intro_running() {
                        matched.begin_intro();
                    }
                }
                Err(error) => eprintln!("match: {error}"),
            }
        } else {
            println!("arena intro: off (ArenaIntros=0)");
            if let Some(kind) = scene_kind(&camera) {
                camera_name = matched.duel.select_scene_camera(kind);
            }
        }
        let presentation = Presentation::open(game)?;
        let dialog = ra2ui::workshop::load(&game.join("UI"), "common/ingame_dialog.gib")?;
        let report = matched.report();
        println!(
            "match: {} — {}, {:.0}s, camera {}",
            matched.config.arena,
            report
                .robots
                .iter()
                .map(|r| r.bot_name.as_str())
                .collect::<Vec<_>>()
                .join(" vs "),
            matched.config.duration.unwrap_or(0.0),
            camera.name()
        );
        Ok(Session {
            matched,
            camera,
            paused: false,
            player_controlled,
            player_robot: 0,
            player_input: DriveInput::default(),
            accumulator: 0.0,
            camera_label_left: CAMERA_LABEL_SECONDS,
            camera_name,
            default_camera,
            chat_open: false,
            chat_entry: String::new(),
            dialog,
            presentation,
            match_sounds: crate::match_sounds::MatchSounds::new(),
            cues: Vec::new(),
        })
    }

    pub fn report(&self) -> MatchReport {
        self.matched.report()
    }

    /// The match's audio listener: the camera's eye, facing its aim point.
    ///
    /// This is the same pose [`Session::render_3d`] uses, so a positional sound
    /// is heard from where the frame is taken. `up` is world +Y; the match
    /// camera never rolls.
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

    /// Set the keyboard drive sampled at the next fixed step. A
    /// player-controlled session feeds robot 0; an AI-only session ignores it.
    pub fn set_drive(&mut self, input: DriveInput) {
        self.player_input = input;
    }

    /// The input the next step will sample.
    pub fn drive(&self) -> DriveInput {
        self.player_input
    }

    /// Where robot `robot` is, in world units. `None` when the match has no
    /// such robot.
    pub fn robot_position(&self, robot: usize) -> Option<glam::Vec3> {
        self.matched
            .duel
            .robots
            .get(robot)
            .map(|r| r.position(&*self.matched.duel.physics))
    }

    /// One fixed simulation step, with the evidence script's decision at the
    /// configured length. A natural finish (a destroyed chassis) is left alone.
    pub fn step(&mut self) {
        // `0x4774f1`: the camera-name window runs on the game's own dt.
        if self.camera_label_left > 0.0 {
            let dt = self.matched.config.dt;
            self.camera_label_left = (self.camera_label_left - dt).max(0.0);
        }
        if self.matched.state() == MatchState::Fight {
            let dt = self.matched.config.dt;
            let limit = self.matched.config.duration;
            if let Some(limit) = limit {
                if self.matched.clock() + dt >= limit
                    && self.matched.report().winners.is_empty()
                    && self.matched.duel.robots.iter().all(|r| r.is_alive())
                {
                    let loser = evidence_loser(&self.matched.report());
                    println!(
                        "evidence decision at {:.2}s: eliminate robot {loser} (no chassis destroyed \
                         in {limit:.0}s; plus.eliminatePlayer request, the tabletop path)",
                        self.matched.clock()
                    );
                    self.matched.eliminate(loser);
                }
            }
        }
        let mut inputs = vec![None; self.matched.duel.robots.len()];
        if self.player_controlled {
            if let Some(input) = inputs.get_mut(self.player_robot) {
                *input = Some(self.player_input);
            }
        }
        let before = self.matched.state();
        self.matched.step_with(&inputs);
        if before == MatchState::Intro && self.matched.state() != MatchState::Intro {
            // `plus.introComplete()`: the match camera takes over.
            let camera = self.default_camera;
            self.enter_camera(camera);
        }
        let cues = self.match_sounds.observe(&mut self.matched);
        self.cues.extend(cues);
    }

    /// Take the sounds the steps since the last call made.
    pub fn take_cues(&mut self) -> Vec<crate::match_sounds::SoundCue> {
        std::mem::take(&mut self.cues)
    }

    /// Skip the arena intro (the `Skip Intro` button): the arena's own
    /// `DoIntro(False)`, which stops its sounds and calls `plus.introComplete()`.
    pub fn skip_intro(&mut self) {
        if self.matched.state() == MatchState::Intro {
            if let Err(error) = ra2script::arena_intro(false) {
                eprintln!("match: {error}");
            }
            self.matched.end_intro();
            let camera = self.default_camera;
            self.enter_camera(camera);
        }
    }

    /// Put the match in one of the camera modes and, for the original's three,
    /// reconfigure the scene camera as its key handler does (`0x439c10`).
    pub fn enter_camera(&mut self, camera: DuelCamera) {
        self.camera = camera;
        self.camera_name =
            scene_kind(&camera).and_then(|kind| self.matched.duel.select_scene_camera(kind));
        self.camera_label_left = CAMERA_LABEL_SECONDS;
    }

    /// The camera keys: F2 orbit, F3 static (the arena's next static camera),
    /// F4 watch (its next watch camera). `0x439c10` dispatches `0x71`..`0x73`.
    pub fn camera_key(&mut self, n: u8) {
        let camera = match n {
            2 => DuelCamera::Orbit { period: 24.0 },
            3 => DuelCamera::Static,
            4 => DuelCamera::Watch,
            _ => return,
        };
        self.enter_camera(camera);
        println!("camera: {} ({})", self.camera.name(), self.camera_label());
    }

    /// The mouse wheel in orbit mode (`0x5a6447`): positive notches zoom in.
    pub fn orbit_wheel(&mut self, notches: f32) {
        if matches!(self.camera, DuelCamera::Orbit { .. }) {
            self.matched
                .duel
                .scene_camera
                .orbit_input(0.0, 0.0, notches);
        }
    }

    /// Advance by wall-clock seconds at the match's own `dt`.
    pub fn advance(&mut self, seconds: f32) {
        if self.paused {
            return;
        }
        self.accumulator += seconds;
        while self.accumulator >= self.matched.config.dt {
            self.accumulator -= self.matched.config.dt;
            self.step();
            if self.matched.state() == MatchState::Results {
                break;
            }
        }
    }

    /// What Esc does in a match (`0x475ee0`): while the arena intro plays it
    /// skips it (`0x477900`); once the match is over (`+0x1d5`) it is ignored;
    /// otherwise it opens the `in game popup win` dialog (and pauses), and with
    /// the dialog open it closes it again (resume). `quit_game` is what the
    /// dialog's `quit button` does (result 8: `0x4122d0`, the game exits);
    /// `end match button` is result 4 (back to `exhibition_setup_win`, or the
    /// Team HQ's flow for an event).
    pub fn escape(&mut self) -> Escape {
        match self.matched.state() {
            MatchState::Results | MatchState::Over => Escape::Ignored,
            MatchState::Intro => {
                self.skip_intro();
                Escape::SkippedIntro
            }
            _ => {
                self.toggle_pause();
                if self.paused {
                    Escape::Paused
                } else {
                    Escape::Resumed
                }
            }
        }
    }

    /// Lay the windows out natively on a target of `size` pixels. Windows are
    /// resolved at every frame, so a resize takes effect on the next one.
    pub fn set_layout(&mut self, size: [u32; 2]) {
        self.presentation.layout = [size[0].max(1) as f32, size[1].max(1) as f32];
    }

    /// The screen the HUD windows are laid out on.
    pub fn layout(&self) -> [f32; 2] {
        self.presentation.layout
    }

    pub fn toggle_pause(&mut self) {
        if self.matched.state() == MatchState::Results {
            return;
        }
        self.paused = !self.paused;
        ra2script::set_paused(self.paused);
        println!("pause: {}", if self.paused { "on" } else { "off" });
    }

    pub fn cycle_camera(&mut self) {
        let next = if self.player_controlled {
            // Driving starts in chase; `C` cycles chase → watch → orbit. The
            // binary's config vocabulary is watch/static/orbit, so `chase` is
            // this client's driving choice and is printed as such.
            match self.camera {
                DuelCamera::Chase { .. } => DuelCamera::Watch,
                DuelCamera::Watch => DuelCamera::Orbit { period: 24.0 },
                _ => chase_camera(&self.matched.duel, self.player_robot),
            }
        } else {
            match self.camera {
                DuelCamera::Watch => DuelCamera::Static,
                DuelCamera::Static => DuelCamera::Orbit { period: 24.0 },
                _ => DuelCamera::Watch,
            }
        };
        self.enter_camera(next);
        println!("camera: {} ({})", self.camera.name(), self.camera_label());
    }

    /// The text `camera textbox` shows: the name the scene camera reported
    /// (`Static 1`, `Watch 1`), else the camera class's own name (`Orbit Cam`).
    pub fn camera_label(&self) -> &str {
        match (&self.camera_name, scene_kind(&self.camera)) {
            (Some(name), Some(_)) => name,
            _ => self.camera.display_name(),
        }
    }

    /// Whether the 3.0 s camera-name window is still open.
    pub fn show_camera_label(&self) -> bool {
        self.camera_label_left > 0.0
    }

    /// Whether the local chat entry is open (backslash).
    pub fn chat_open(&self) -> bool {
        self.chat_open
    }

    /// Open or close the chat group. `0x475ee0` opens it on `\` during play
    /// (`+0x1b4` visible, `+0x1b8` focused). The text is local; nothing is
    /// sent.
    pub fn toggle_chat(&mut self) {
        if self.matched.state() == MatchState::Results || self.paused {
            return;
        }
        self.chat_open = !self.chat_open;
        self.chat_entry.clear();
        println!("chat: {}", if self.chat_open { "open" } else { "closed" });
    }

    /// Append a typed character to the chat entry.
    pub fn chat_type(&mut self, c: char) {
        if self.chat_open && !c.is_control() {
            self.chat_entry.push(c);
        }
    }

    /// Remove the last typed character.
    pub fn chat_backspace(&mut self) {
        if self.chat_open {
            self.chat_entry.pop();
        }
    }

    /// Close the chat entry on Enter or Escape and log the line. The file's
    /// `chat displaybox` is a `DisplayBox`; drawing its history is not part of
    /// this minimum.
    pub fn chat_submit(&mut self) {
        if !self.chat_open {
            return;
        }
        if !self.chat_entry.is_empty() {
            println!("chat (local): {}", self.chat_entry);
            // `Sounds\chat.wav`: the match object makes it with the countdown
            // beep and the critical hit (`0x415849`).
            self.cues.push(crate::match_sounds::SoundCue::Chat);
            self.chat_entry.clear();
        }
        self.chat_open = false;
    }

    fn screen_bind(&self) -> ScreenBind {
        ScreenBind {
            camera: self.camera_label().to_string(),
            show_camera: self.show_camera_label(),
            chat_open: self.chat_open,
            chat_entry: self.chat_entry.clone(),
        }
    }

    /// The scene for this instant: results at the end, otherwise HUD (plus the
    /// pause dialog when paused).
    pub fn scene(&mut self) -> Result<UiScene, String> {
        let report = self.matched.report();
        let state = self.matched.state();
        let clock = self.matched.clock();
        let countdown = self.matched.countdown_left();
        if state == MatchState::Results {
            return self.presentation.results_scene(&report);
        }
        let bind = self.screen_bind();
        if self.paused {
            let dialog = self.dialog.clone();
            self.presentation
                .pause_scene(&report, state, clock, countdown, &bind, &dialog)
        } else {
            self.presentation
                .hud_scene(&report, state, clock, countdown, &bind)
                .map(|(_, scene)| scene)
        }
    }

    /// Bind the HUD this instant and return both its values and its scene. The
    /// HUD unit tests use the same call through [`Session::scene`].
    pub fn hud_now(&mut self) -> Result<(hud::Hud, UiScene), String> {
        let report = self.matched.report();
        let state = self.matched.state();
        let clock = self.matched.clock();
        let countdown = self.matched.countdown_left();
        let bind = self.screen_bind();
        self.presentation
            .hud_scene(&report, state, clock, countdown, &bind)
    }

    /// Whether a click during the arena intro hit the HUD's `skip button`
    /// ("click to skip past the announcer intro", in `skip group`, which the
    /// HUD shows only while the intro plays).
    pub fn skip_button_at(&mut self, point: [f32; 2]) -> bool {
        if self.matched.state() != MatchState::Intro || self.paused {
            return false;
        }
        // Bind the HUD for the current moment, so the hit test sees the same
        // visibility the frame drew.
        if self.hud_now().is_err() {
            return false;
        }
        let placed = ra2ui::resolve_with(
            &self.presentation.hud,
            self.presentation.layout,
            Some(self.presentation.assets.faces()),
        );
        let Some(path) = ra2ui::input::hit_control(&placed, point[0], point[1]) else {
            return false;
        };
        names_on_path(&placed, &path)
            .iter()
            .any(|n| n == "skip button")
    }

    /// Whether a click in the pause dialog hit a named button.
    pub fn pause_button_at(&self, point: [f32; 2]) -> Option<String> {
        if !self.paused {
            return None;
        }
        let placed = ra2ui::resolve_with(
            &self.dialog,
            self.presentation.layout,
            Some(self.presentation.assets.faces()),
        );
        let path = ra2ui::input::hit_control(&placed, point[0], point[1])?;
        let names = names_on_path(&placed, &path);
        for wanted in ["resume button", "quit button", "end match button"] {
            if names.iter().any(|n| n == wanted) {
                return Some(wanted.to_string());
            }
        }
        None
    }

    /// Whether a click on the results screen hit its `continue button` or
    /// `Quit Session button`. Both return to the menu in this client; the
    /// original's continue/next-round destinations are event state (M10).
    pub fn results_button_at(&mut self, point: [f32; 2]) -> Option<String> {
        if self.matched.state() != MatchState::Results {
            return None;
        }
        if self.presentation.results.is_none() {
            let report = self.matched.report();
            let _ = self.presentation.results_scene(&report);
        }
        let screen = self.presentation.results.as_ref()?;
        let placed = ra2ui::resolve_with(
            screen,
            self.presentation.layout,
            Some(self.presentation.assets.faces()),
        );
        let path = ra2ui::input::hit_control(&placed, point[0], point[1])?;
        let names = names_on_path(&placed, &path);
        for wanted in ["continue button"] {
            if names.iter().any(|n| n == wanted) {
                return Some(wanted.to_string());
            }
        }
        None
    }

    /// Draw the 3D pass into a caller's target.
    pub fn render_3d(
        &self,
        renderer: &Renderer,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        w: u32,
        h: u32,
    ) -> Result<(), String> {
        self.matched
            .duel
            .render_to(renderer, &self.camera, color, depth, format, w, h)
            .map_err(|e| e.to_string())
    }
}

fn evidence_loser(report: &MatchReport) -> usize {
    let score = |i: usize| -> f32 {
        let r = &report.robots[i];
        r.armor + r.battery + r.air
    };
    let mut order: Vec<usize> = (0..report.robots.len()).collect();
    order.sort_by(|&a, &b| {
        score(a)
            .partial_cmp(&score(b))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                report.robots[b]
                    .damage_taken
                    .partial_cmp(&report.robots[a].damage_taken)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| b.cmp(&a))
    });
    order[0]
}

fn names_on_path(placed: &[ra2ui::Placement<'_>], path: &[usize]) -> Vec<String> {
    let mut out = Vec::new();
    let Some(first) = path.first() else {
        return out;
    };
    let mut current = &placed[*first];
    out.push(current.widget.name().to_string());
    for index in &path[1..] {
        current = &current.children[*index];
        out.push(current.widget.name().to_string());
    }
    out
}

/// The offscreen run: frames, transitions, winner.
pub fn run_headless(options: Options, audio_report: bool) -> Result<MatchReport, String> {
    let out = options
        .out
        .clone()
        .ok_or_else(|| "--match --out DIR is the offscreen path".to_string())?;
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;

    let renderer = Renderer::new().map_err(|e| e.to_string())?;
    let ui = UiRenderer::new_for(renderer.device(), wgpu::TextureFormat::Rgba8Unorm);
    let target = Target::new(renderer.device(), options.size[0], options.size[1]);
    let size = options.size;
    let out = options.out.as_ref().unwrap();

    let mut session = Session::new(&options)?;
    let cfg = crate::options::read_cfg(&options.game);
    let mut audio = crate::audio::AudioSystem::new_offscreen(&options.game, &cfg);
    // Nothing from the shell is on screen here; the arena's own intro/crowd
    // sounds are the match's music, and they arrive as script requests.
    audio.stop_music("the match is on screen");
    audio.drain_match(session.audio_listener());
    let mut last_state = session.matched.state();
    let mut wrote_countdown = false;
    let mut wrote_first = false;
    let mut wrote_mid = false;
    let mut wrote_last = false;
    let mut quit = false;

    // The countdown's number is 3 at the start of its middle second; one frame
    // there is the demonstration the roadmap asks for.
    while session.matched.state() != MatchState::Results {
        let state = session.matched.state();
        if state != last_state {
            let report = session.report();
            let line = format!(
                "{:?} -> {:?} at {:.2}s, points {:?}, winners {:?}",
                last_state,
                state,
                session.matched.clock(),
                report.robots.iter().map(|r| r.points).collect::<Vec<_>>(),
                report.winners
            );
            println!("  {line}");
            last_state = state;
        }

        if let Some(quit_at) = options.quit_at {
            if session.matched.state() == MatchState::Fight && session.matched.clock() >= quit_at {
                session.paused = true;
                ra2script::set_paused(true);
                write_frame(
                    &mut session,
                    &renderer,
                    &ui,
                    &target,
                    size,
                    out,
                    "pause.png",
                )?;
                let point = {
                    let dialog = session.dialog.clone();
                    let placed = ra2ui::resolve_with(
                        &dialog,
                        session.presentation.layout,
                        Some(session.presentation.assets.faces()),
                    );
                    button_point(&placed, "quit button")
                }
                .ok_or_else(|| "ingame_dialog.gib's quit button is not clickable".to_string())?;
                let hit = session
                    .pause_button_at(point)
                    .unwrap_or_else(|| "nothing".into());
                println!("  pause: click {point:?} hits {hit:?}");
                if hit != "quit button" {
                    return Err(format!("the quit button hit {hit:?}, not itself"));
                }
                write_menu(&renderer, &ui, &options)?;
                quit = true;
                break;
            }
        }

        if state == MatchState::Countdown && !wrote_countdown {
            write_frame(
                &mut session,
                &renderer,
                &ui,
                &target,
                size,
                out,
                "countdown.png",
            )?;
            wrote_countdown = true;
        }

        if state == MatchState::Fight && !wrote_first {
            write_frame(
                &mut session,
                &renderer,
                &ui,
                &target,
                size,
                out,
                "fight-first.png",
            )?;
            wrote_first = true;
        }
        if state == MatchState::Fight
            && !wrote_mid
            && session.matched.clock() >= options.seconds / 2.0
        {
            write_frame(
                &mut session,
                &renderer,
                &ui,
                &target,
                size,
                out,
                "fight-mid.png",
            )?;
            wrote_mid = true;
        }

        let before = session.matched.state();
        session.step();
        audio.drain_match(session.audio_listener());
        let cues = session.take_cues();
        audio.play_cues(cues, session.audio_listener());
        if before == MatchState::Fight
            && session.matched.state() != MatchState::Fight
            && !wrote_last
        {
            if !wrote_mid {
                // A natural finish before the scheduled midpoint still leaves
                // the five named frames; this one is the final pose, said so.
                println!(
                    "  fight ended before the midpoint; fight-mid.png is the final fight frame"
                );
                write_frame(
                    &mut session,
                    &renderer,
                    &ui,
                    &target,
                    size,
                    out,
                    "fight-mid.png",
                )?;
                wrote_mid = true;
            }
            // The duel does not advance in `Over`, so this is the final fight
            // pose with the HUD still up.
            write_frame(
                &mut session,
                &renderer,
                &ui,
                &target,
                size,
                out,
                "fight-last.png",
            )?;
            wrote_last = true;
        }
    }

    if !quit {
        let final_state = session.matched.state();
        if final_state != last_state {
            let report = session.report();
            let line = format!(
                "{:?} -> {:?} at {:.2}s, points {:?}, winners {:?}",
                last_state,
                final_state,
                session.matched.clock(),
                report.robots.iter().map(|r| r.points).collect::<Vec<_>>(),
                report.winners
            );
            println!("  {line}");
        }
        if session.matched.state() == MatchState::Results {
            let report = session.report();
            write_frame(
                &mut session,
                &renderer,
                &ui,
                &target,
                size,
                out,
                "results.png",
            )?;
            println!(
                "  Results: reason {:?}, winners {:?}, points {:?}",
                report.reason,
                report.winners,
                report
                    .robots
                    .iter()
                    .map(|r| (r.bot_name.as_str(), r.points))
                    .collect::<Vec<_>>()
            );
        }
        let report = session.report();
        println!(
            "winner: {}",
            if report.winners.is_empty() {
                "none (draw)".to_string()
            } else {
                report
                    .winners
                    .iter()
                    .map(|&i| report.robots[i].bot_name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );
        println!(
            "clock {:.2}s, reason {:?}, points {:?}",
            report.duration,
            report.reason,
            report
                .robots
                .iter()
                .map(|r| (r.bot_name.as_str(), r.points))
                .collect::<Vec<_>>()
        );
    }
    println!(
        "wrote {} frame(s) to {}",
        std::fs::read_dir(out)
            .map(|d| d.filter_map(Result::ok).count())
            .unwrap_or(0),
        out.display()
    );
    let total = audio.audio().clock();
    let seconds = std::env::var("RA2_AUDIO_SECONDS")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(total);
    audio.finish_capture(seconds)?;
    print!("{}", audio.report(audio_report));
    Ok(session.report())
}

/// A moment `--camera-shots` photographs.
#[derive(Clone, Debug, PartialEq)]
enum Shot {
    /// `intro@T`: T seconds into the arena intro.
    Intro(f32),
    /// `skip@T`: click `Skip Intro` T seconds into the intro (no picture).
    Skip(f32),
    /// `countdown@N`: the first frame whose countdown number is N.
    Countdown(u32),
    /// `f2|f3|f3b|f4@S`: press that camera key at S fight seconds, then shoot.
    Key { key: u8, at: f32 },
}

fn parse_shot(spec: &str) -> Result<(String, Shot), String> {
    let (name, at) = match spec.split_once('@') {
        Some((name, at)) => (
            name,
            Some(
                at.parse::<f32>()
                    .map_err(|_| format!("--camera-shots {spec:?}: {at:?} is not a number"))?,
            ),
        ),
        None => (spec, None),
    };
    let shot = match name {
        "intro" => Shot::Intro(at.unwrap_or(2.5)),
        "skip" => Shot::Skip(at.unwrap_or(4.8)),
        "countdown" => Shot::Countdown(at.unwrap_or(2.0) as u32),
        "f2" => Shot::Key {
            key: 2,
            at: at.unwrap_or(13.0),
        },
        "f3" | "f3b" => Shot::Key {
            key: 3,
            at: at.unwrap_or(if name == "f3" { 17.0 } else { 20.0 }),
        },
        "f4" => Shot::Key {
            key: 4,
            at: at.unwrap_or(24.0),
        },
        other => {
            return Err(format!(
                "--camera-shots: {other:?} is not intro/skip/countdown/f2/f3/f3b/f4"
            ))
        }
    };
    Ok((name.to_string(), shot))
}

/// `--camera-shots`: run one match (the arena's intro on) and photograph the
/// listed camera moments in time order, each to `cam-<name>.png`. The camera
/// keys are real key presses in the order given: `f3` then `f3b` is F3 twice,
/// which is how the original's second static camera is reached. Fight seconds
/// must not exceed `--seconds`.
pub fn run_camera_shots(options: Options, shots: &[String]) -> Result<(), String> {
    let out = options
        .out
        .clone()
        .ok_or_else(|| "--camera-shots needs --out DIR".to_string())?;
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut pending: Vec<(String, Shot)> = shots
        .iter()
        .map(|s| parse_shot(s))
        .collect::<Result<_, _>>()?;
    let renderer = Renderer::new().map_err(|e| e.to_string())?;
    let ui = UiRenderer::new_for(renderer.device(), wgpu::TextureFormat::Rgba8Unorm);
    let size = options.size;
    let target = Target::new(renderer.device(), size[0], size[1]);
    let mut session = Session::new(&options)?;
    let mut intro_time = 0.0f32;
    let dt = session.matched.config.dt;
    // The shots the match reaches in time order; a key shot waits for its
    // fight second, so keep them sorted by when they can fire.
    let mut guard = 0u32;
    while !pending.is_empty() {
        guard += 1;
        if guard > 60 * 60 * 20 {
            return Err(format!(
                "camera shots never reached: {:?}",
                pending.iter().map(|p| p.0.clone()).collect::<Vec<_>>()
            ));
        }
        let state = session.matched.state();
        let clock = session.matched.clock();
        let countdown = ra2sim::countdown_number(session.matched.countdown_left());
        // Shots are taken in the order given: only the first can fire.
        let fire = pending.first().and_then(|(_, shot)| {
            let ready = match *shot {
                Shot::Intro(t) | Shot::Skip(t) => state == MatchState::Intro && intro_time >= t,
                Shot::Countdown(n) => state == MatchState::Countdown && countdown == Some(n),
                // A match that ends before the shot's second (a robot out of
                // the arena at once) is photographed in its final pose.
                Shot::Key { at, .. } => {
                    (state == MatchState::Fight && clock >= at)
                        || matches!(state, MatchState::Over | MatchState::Results)
                }
            };
            ready.then_some(0usize)
        });
        if let Some(i) = fire {
            let (name, shot) = pending.remove(i);
            if let Shot::Key { key, .. } = shot {
                session.camera_key(key);
            }
            if matches!(shot, Shot::Skip(_)) {
                // `Skip Intro`: the countdown inherits the flight's pose at this
                // moment (the watch camera sets only the eye, docs/30).
                session.skip_intro();
                continue;
            }
            write_frame(
                &mut session,
                &renderer,
                &ui,
                &target,
                size,
                &out,
                &format!("cam-{name}.png"),
            )?;
            continue;
        }
        if state == MatchState::Results {
            return Err(format!(
                "the match ended before the camera shots {:?}",
                pending.iter().map(|p| p.0.clone()).collect::<Vec<_>>()
            ));
        }
        if state == MatchState::Intro {
            intro_time += dt;
        }
        session.step();
    }
    Ok(())
}

pub(crate) fn write_frame(
    session: &mut Session,
    renderer: &Renderer,
    ui: &UiRenderer,
    target: &Target,
    size: [u32; 2],
    out: &Path,
    name: &str,
) -> Result<(), String> {
    let scene = session.scene()?;
    session.render_3d(
        renderer,
        &target.color_view,
        &target.depth_view,
        wgpu::TextureFormat::Rgba8Unorm,
        size[0],
        size[1],
    )?;
    ui.render_over_to(
        renderer.device(),
        renderer.queue(),
        &scene,
        &target.color_view,
        wgpu::TextureFormat::Rgba8Unorm,
        size[0],
        size[1],
    )
    .map_err(|e| e.to_string())?;
    let pixels = renderer
        .read_texture(&target.color, size[0], size[1])
        .map_err(|e| e.to_string())?;
    let path = out.join(name);
    write_png(&path, size, pixels)?;
    println!("  wrote {}", path.display());
    Ok(())
}

fn write_menu(renderer: &Renderer, ui: &UiRenderer, options: &Options) -> Result<(), String> {
    use crate::frame::FrameRenderer;
    let mut shell = ra2_shell::Shell::boot(&options.game, SHELL_SIZE)?;
    let mut frames = FrameRenderer::open(&options.game)?;
    let frame = frames.frame(&mut shell, options.size);
    let pixels = ui
        .render_at(
            renderer.device(),
            renderer.queue(),
            &frame.scene,
            options.size,
        )
        .map_err(|e| e.to_string())?;
    let path = options.out.as_ref().unwrap().join("menu.png");
    write_png(&path, options.size, pixels)?;
    println!(
        "  quit returns to {} and wrote {}",
        shell.screen_id(),
        path.display()
    );
    Ok(())
}

fn write_png(path: &Path, size: [u32; 2], pixels: Vec<u8>) -> Result<(), String> {
    let image = image::RgbaImage::from_raw(size[0], size[1], pixels)
        .ok_or_else(|| "the readback did not fill the image".to_string())?;
    image
        .save(path)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// The offscreen colour target.
pub(crate) struct Target {
    pub(crate) color: wgpu::Texture,
    pub(crate) color_view: wgpu::TextureView,
    _depth: wgpu::Texture,
    pub(crate) depth_view: wgpu::TextureView,
}

impl Target {
    pub(crate) fn new(device: &wgpu::Device, width: u32, height: u32) -> Target {
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("match-color"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("match-depth"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        Target {
            color_view: color.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_view: depth.create_view(&wgpu::TextureViewDescriptor::default()),
            color,
            _depth: depth,
        }
    }
}

/// A point inside `name` that the control hit test reaches, or `None`.
fn button_point(placed: &[ra2ui::Placement<'_>], name: &str) -> Option<[f32; 2]> {
    let widget = placed
        .iter()
        .flat_map(|p| p.walk())
        .find(|p| p.widget.name() == name)?;
    let frame = widget.frame;
    for iy in 1..8 {
        for ix in 1..10 {
            let point = [
                frame.x + frame.w * ix as f32 / 10.0,
                frame.y + frame.h * iy as f32 / 8.0,
            ];
            if let Some(path) = ra2ui::input::hit_control(placed, point[0], point[1]) {
                if names_on_path(placed, &path).iter().any(|n| n == name) {
                    return Some(point);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2sim::r#match::{RobotReport, WinReason};

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

    fn robot(slot: usize, name: &str, team: &str, points: i32) -> RobotReport {
        RobotReport {
            slot,
            team: slot,
            bot_name: name.into(),
            team_name: team.into(),
            team_logo: String::new(),
            bot_preview: None,
            points,
            damage_dealt: 10.0 * (slot + 1) as f32,
            damage_taken: 5.0,
            battery: 1.0,
            air: 1.0,
            armor: 1.0,
            dot: 0,
            defeated: slot == 1,
            eliminated: false,
            winner: slot == 0,
            immobilized: false,
            components: Vec::new(),
        }
    }

    /// A priced contact on a component plays one of that component's own
    /// `damagesounds` at the contact point, at the volume the damage maps to
    /// (`FUN_0045f3a0`), and a hit of 4 or less is silent.
    #[test]
    fn a_contact_plays_the_struck_components_damage_sound() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let options = Options::new(
            &game,
            None,
            Vec::new(),
            Some(3.0),
            Some(std::env::temp_dir().join("ra2-match-impact")),
            None,
            [640, 502],
        )
        .unwrap();
        let mut session = Session::new(&options).expect("the match");
        assert!(session.matched.duel.record_impacts);
        let chassis_sounds = session.matched.duel.robots[1].rig.components[0]
            .sounds
            .damage
            .clone();
        assert!(!chassis_sounds.is_empty(), "the chassis has impact sounds");
        for damage in [3.0_f32, 52.0] {
            session.matched.duel.impacts.push(ra2sim::combat::Impact {
                attacker: 0,
                target: 1,
                component: 0,
                by: 0,
                damage,
                point: glam::Vec3::new(1.0, 2.0, 3.0),
                normal: glam::Vec3::Y,
                speed: 4.0,
            });
        }
        let mut sounds = crate::match_sounds::MatchSounds::new();
        let cues = sounds.observe(&mut session.matched);
        let impacts: Vec<_> = cues
            .iter()
            .filter_map(|c| match c {
                crate::match_sounds::SoundCue::Impact {
                    path,
                    position,
                    hundredths_db,
                } => Some((path.clone(), *position, *hundredths_db)),
                _ => None,
            })
            .collect();
        assert_eq!(impacts.len(), 1, "damage 3 is under the floor: {cues:?}");
        let (path, position, volume) = &impacts[0];
        assert!(
            chassis_sounds.contains(path),
            "{path} not in {chassis_sounds:?}"
        );
        assert_eq!(*position, [1.0, 2.0, 3.0]);
        assert!((-760..=-740).contains(volume), "{volume}");
        assert!(session.matched.duel.impacts.is_empty(), "consumed");
    }

    /// Steps a session by 0.05 s until `until` says stop, collecting every cue
    /// with the elapsed time at the step that made it (the match clock stands
    /// still through the intro).
    fn run_for_cues(
        session: &mut Session,
        max_seconds: f32,
        mut until: impl FnMut(&Session, &[crate::match_sounds::SoundCue]) -> bool,
    ) -> Vec<(f32, crate::match_sounds::SoundCue)> {
        let mut all = Vec::new();
        let mut clock = 0.0f32;
        for _ in 0..(max_seconds / 0.05) as usize {
            session.advance(0.05);
            clock += 0.05;
            let cues = session.take_cues();
            let stop = until(session, &cues);
            all.extend(cues.into_iter().map(|c| (clock, c)));
            if stop {
                break;
            }
        }
        all
    }

    /// The countdown window plays one of six `Begin_*` lines half a second
    /// after the bell (live: bell created at 30.52 s, line played at 31.04 s
    /// and at 30.85 s in the two full-match captures, `docs/35`).
    #[test]
    fn the_bell_is_followed_half_a_second_later_by_one_begin_line() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let options =
            Options::new(&game, None, Vec::new(), Some(12.0), None, None, [640, 502]).unwrap();
        let mut session = Session::new(&options).expect("the match");
        session.skip_intro();
        let mut fight_for = 0.0f32;
        let cues = run_for_cues(&mut session, 12.0, |s, _| {
            if s.matched.state() == MatchState::Fight {
                fight_for += 0.05;
            }
            fight_for > 1.5
        });
        let bell = cues
            .iter()
            .find(|(_, c)| matches!(c, crate::match_sounds::SoundCue::Beep { go: true }))
            .map(|(t, _)| *t)
            .expect("the bell");
        let lines: Vec<_> = cues
            .iter()
            .filter_map(|(t, c)| match c {
                crate::match_sounds::SoundCue::Announce { path } => Some((*t, *path)),
                _ => None,
            })
            .collect();
        assert_eq!(lines.len(), 1, "{lines:?}");
        let (at, path) = lines[0];
        assert!(
            crate::match_sounds::BEGIN_LINES.contains(&path),
            "{path} is not one of the six"
        );
        assert!(
            (0.4..=0.65).contains(&(at - bell)),
            "{} s after the bell",
            at - bell
        );
        // Every one of them is a shipped file.
        for line in crate::match_sounds::BEGIN_LINES {
            assert!(game.join(line).is_file(), "{line}");
        }
    }

    /// The crowd object is built with the match and ticks through the arena
    /// intro: excitement 55 falls a point a second, so the first mood change
    /// (`normal_to_somewhat` at 44) comes about 11 s in, long before the bell
    /// (live: `crowd/*.wav` created at 4.95 s, the transition at 16.07 s).
    #[test]
    fn the_crowd_ticks_through_the_intro() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let options =
            Options::new(&game, None, Vec::new(), Some(40.0), None, None, [640, 502]).unwrap();
        let mut session = Session::new(&options).expect("the match");
        let cues = run_for_cues(&mut session, 13.0, |s, _| {
            s.matched.state() != MatchState::Intro
        });
        let change = cues
            .iter()
            .find(|(_, c)| {
                matches!(c, crate::match_sounds::SoundCue::CrowdOneShot { path }
                    if path.contains("normal_to_somewhat"))
            })
            .map(|(t, _)| *t);
        let change = change.unwrap_or_else(|| panic!("no mood change in the intro: {cues:?}"));
        assert!((10.0..=12.5).contains(&change), "{change}");
    }

    /// Weapon triggers sound once per press, at the class's own level
    /// (`-500` electric burst motor, `0` piston), not once per stroke: the
    /// live Scout v ALARM fights have the piston's `air_trigger` about every
    /// half second and the burst motor's `burst_trigger` twice in 100 s.
    #[test]
    fn weapon_triggers_sound_once_per_press() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let bots = vec![
            game.join("AI/Team0/Bot0.bot"),
            game.join("AI/Team0/Bot1.bot"),
        ];
        let options = Options::new(&game, None, bots, Some(40.0), None, None, [640, 502]).unwrap();
        let mut session = Session::new(&options).expect("the match");
        session.skip_intro();
        let mut fight_for = 0.0f32;
        let cues = run_for_cues(&mut session, 40.0, |s, _| {
            if s.matched.state() == MatchState::Fight {
                fight_for += 0.05;
            }
            fight_for >= 25.0
        });
        let fires: Vec<(f32, i32)> = cues
            .iter()
            .filter_map(|(t, c)| match c {
                crate::match_sounds::SoundCue::Fire { hundredths_db, .. } => {
                    Some((*t, *hundredths_db))
                }
                _ => None,
            })
            .collect();
        assert!(!fires.is_empty(), "the AIs fire in 25 s of fight");
        assert!(
            fires.iter().all(|(_, db)| *db == 0 || *db == -500),
            "{fires:?}"
        );
        // Never faster than the AI's own half-second trigger (3 Hz at most
        // for two robots, with slack for the step grid).
        assert!(
            fires.len() as f32 <= 25.0 * 4.0 + 4.0,
            "{} fires",
            fires.len()
        );
    }

    /// The arena's own `DoIntro` reaches the mixer: the intro music and the
    /// announcers are script requests, applied at the match camera.
    #[test]
    fn the_arena_intro_sounds_reach_the_audio_log() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let options = Options::new(
            &game,
            None,
            Vec::new(),
            Some(3.0),
            Some(std::env::temp_dir().join("ra2-match-audio")),
            None,
            [640, 502],
        )
        .unwrap();
        let mut session = Session::new(&options).expect("the match");
        let cfg = crate::options::read_cfg(&game);
        let mut audio = crate::audio::AudioSystem::new_silent(&game, &cfg);
        audio.drain_match(session.audio_listener());
        for _ in 0..30 {
            session.advance(0.05);
            audio.drain_match(session.audio_listener());
        }
        let log = audio.audio().play_log();
        let sounds: Vec<_> = log
            .entries()
            .iter()
            .filter(|entry| entry.kind == ra2_audio::PlayKind::Sound3d)
            .map(|entry| entry.path.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(
            sounds
                .iter()
                .any(|path| path.contains("Sounds/intro_music/")),
            "the intro music is a real request: {sounds:?}"
        );
        assert!(
            sounds
                .iter()
                .any(|path| path.contains("Sounds/announcers/")),
            "an announcer is a real request: {sounds:?}"
        );
        let entry = log
            .entries()
            .iter()
            .find(|entry| entry.kind == ra2_audio::PlayKind::Sound3d)
            .expect("an arena sound");
        assert_eq!(entry.position, Some([0.0, 0.0, 0.0]));
        assert!((entry.gain - audio.audio().sfx_gain()).abs() < 1e-6);
        let listener = session.audio_listener();
        assert_ne!(listener.position, [0.0, 0.0, 0.0], "not a fixed origin");
    }

    fn report() -> MatchReport {
        MatchReport {
            game_type: "DEATHMATCH",
            arena: "box".into(),
            state: MatchState::Results,
            reason: Some(WinReason::LastRobot),
            winners: vec![0],
            duration: 12.0,
            duration_limit: Some(12.0),
            robots: vec![
                robot(0, "Roly Poly", "Red Zone", 7),
                robot(1, "Little Metal Friend", "Prehistoric", 3),
            ],
            log: Vec::new(),
        }
    }

    fn slot_text(doc: &Gib, slot: &str, child: &str) -> String {
        let placed = ra2ui::resolve_with(doc, SHELL_SIZE, None);
        let slot = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == slot)
            .unwrap_or_else(|| panic!("{slot}"));
        slot.find(child)
            .and_then(|p| {
                p.widget
                    .children
                    .iter()
                    .find(|c| c.kind == ra2fmt::gib::WidgetKind::Text)
                    .and_then(|c| c.field("_Text"))
                    .map(|f| f.text().to_string())
            })
            .unwrap_or_default()
    }

    /// The four result slots are filled from the report's own rows: the points
    /// and bot name land in slots 1 and 2, and slots 3/4 stay empty for a duel.
    /// Esc in the original (`0x475ee0`): pause popup on/off during play, ignored
    /// once the match is over.
    #[test]
    fn escape_toggles_the_pause_dialog_and_is_ignored_in_the_results() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let options =
            Options::new(&game, None, Vec::new(), Some(3.0), None, None, [640, 502]).unwrap();
        let mut session = Session::new(&options).expect("the match");
        session.skip_intro();
        while session.matched.state() != MatchState::Fight {
            session.step();
        }
        assert_eq!(session.escape(), Escape::Paused);
        assert!(session.paused);
        assert_eq!(session.escape(), Escape::Resumed);
        assert!(!session.paused);
        while session.matched.state() != MatchState::Results {
            session.step();
        }
        assert_eq!(session.escape(), Escape::Ignored);
        assert!(!session.paused);
    }

    /// The match windows lay out natively on the target (the original has no
    /// design size): at 1024x768 the countdown group is the 210x96 box the
    /// `arena-*-countdown` captures show at x 407..617, y 336..432, and the
    /// scene is exactly the target size so the renderer's fit is the identity.
    #[test]
    fn the_match_windows_lay_out_natively_on_the_target() {
        let Some(game) = game_dir() else { return };
        let mut presentation = Presentation::open(&game).unwrap();
        presentation.layout = [1024.0, 768.0];
        let bind = ScreenBind {
            camera: "Watch 1".into(),
            show_camera: true,
            chat_open: false,
            chat_entry: String::new(),
        };
        let (_, scene) = presentation
            .hud_scene(&report(), MatchState::Countdown, 0.0, 2.5, &bind)
            .unwrap();
        assert_eq!(scene.size, [1024, 768]);
        let placed = ra2ui::resolve_with(&presentation.hud, presentation.layout, None);
        let group = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "countdown group")
            .expect("the countdown group");
        assert_eq!(
            [group.frame.x, group.frame.y, group.frame.w, group.frame.h],
            [407.0, 336.0, 210.0, 96.0]
        );
    }

    #[test]
    fn the_results_screen_takes_the_reports_slots() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let mut presentation = Presentation::open(&game).unwrap();
        let report = report();
        let scene = presentation
            .results_scene(&report)
            .expect("the results scene builds");
        let doc = presentation.results.as_ref().unwrap();
        assert_eq!(slot_text(doc, "result slot 1", "points"), "7");
        assert_eq!(slot_text(doc, "result slot 1", "bot name"), "Roly Poly");
        assert_eq!(slot_text(doc, "result slot 1", "team name"), "Red Zone");
        assert_eq!(slot_text(doc, "result slot 2", "points"), "3");
        assert_eq!(
            slot_text(doc, "result slot 2", "bot name"),
            "Little Metal Friend"
        );
        assert!(slot_text(doc, "result slot 3", "bot name").is_empty());
        assert!(slot_text(doc, "result slot 4", "bot name").is_empty());
        // The glyphs are really in the draw list, not just in the tree.
        let glyphs = scene
            .quads
            .iter()
            .filter(|q| scene.textures[q.texture].label.starts_with('#'))
            .count();
        println!(
            "results scene: {} quads, {glyphs} glyph quads",
            scene.quads.len()
        );
        assert!(
            glyphs > 60,
            "a result screen reads, not a blank panel: {glyphs}"
        );
    }

    /// The results title is drawn over the panel, and nothing covers it.
    ///
    /// The panel is the Window's `+Background` body (520×426, white 70%) and
    /// the title is the `New [TextBox]` at the file's `Position 0 0` with its
    /// own 520×28 body. The draw order is measured from the scene list: the
    /// panel first, then the title's bar, then the title's glyphs. A quad drawn
    /// after the glyphs that overlaps their ink is the failure this pins.
    #[test]
    fn the_results_title_is_not_covered_by_the_panel() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let mut presentation = Presentation::open(&game).unwrap();
        let report = report();
        let scene = presentation
            .results_scene(&report)
            .expect("the results scene builds");

        // The panel: the one white 70% 520×426 fill.
        let panel = scene
            .quads
            .iter()
            .position(|q| {
                q.texture == 0
                    && q.rect.w == 520.0
                    && q.rect.h == 426.0
                    && q.color == [1.0, 1.0, 1.0, 0.7]
            })
            .expect("the results panel");
        // The title's own 520×28 body and its glyphs.
        let bar = scene
            .quads
            .iter()
            .position(|q| {
                q.texture == 0
                    && q.rect.w == 520.0
                    && q.rect.h == 28.0
                    && q.rect.y == scene.quads[panel].rect.y
            })
            .expect("the title bar");
        let glyphs: Vec<usize> = scene
            .quads
            .iter()
            .enumerate()
            .filter(|(_, q)| {
                q.texture != 0
                    && scene.textures[q.texture].label == "#fg_d24"
                    && q.color == [0.0, 0.0, 0.0, 1.0]
            })
            .map(|(i, _)| i)
            .collect();
        assert!(!glyphs.is_empty(), "the title's glyphs are in the scene");
        let first = *glyphs.first().unwrap();
        let last = *glyphs.last().unwrap();
        assert!(
            panel < bar && bar < first,
            "order is panel ({panel}), title bar ({bar}), glyphs ({first}..={last})"
        );

        // The visible ink (the glyph rectangle clipped to what the TextBox lets
        // through) lies inside the panel...
        let panel_rect = scene.quads[panel].rect;
        let (mut left, mut top, mut right, mut bottom) = (
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        );
        for &i in &glyphs {
            let q = &scene.quads[i];
            let r = match q.clip {
                Some(c) => {
                    let x = q.rect.x.max(c.x);
                    let y = q.rect.y.max(c.y);
                    ra2ui::Frame::new(
                        x,
                        y,
                        (q.rect.right().min(c.right()) - x).max(0.0),
                        (q.rect.bottom().min(c.bottom()) - y).max(0.0),
                    )
                }
                None => q.rect,
            };
            assert!(
                r.x >= panel_rect.x
                    && r.y >= panel_rect.y
                    && r.right() <= panel_rect.right()
                    && r.bottom() <= panel_rect.bottom(),
                "title glyph {i} at {r:?} is outside the panel {panel_rect:?}"
            );
            left = left.min(r.x);
            top = top.min(r.y);
            right = right.max(r.right());
            bottom = bottom.max(r.bottom());
        }
        let ink = ra2ui::Frame::new(left, top, right - left, bottom - top);

        // ...and no later quad draws over it.
        for (i, q) in scene.quads.iter().enumerate().skip(last + 1) {
            if q.color[3] <= 0.0 {
                continue;
            }
            let r = q.rect;
            let overlaps =
                r.x < ink.right() && ink.x < r.right() && r.y < ink.bottom() && ink.y < r.bottom();
            assert!(
                !overlaps,
                "quad {i} ({:?} at {r:?}, color {:?}) is drawn over the title ink {ink:?}",
                scene.textures[q.texture].label, q.color
            );
        }
    }

    /// A full headless match writes the five named moments and leaves a PNG on
    /// disk for each; the results PNG is the screen just checked.
    #[test]
    fn a_headless_match_writes_the_named_frames() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let dir = std::env::temp_dir().join(format!("ra2-match-frames-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let options = Options::new(
            &game,
            None,
            Vec::new(),
            Some(1.5),
            Some(dir.clone()),
            None,
            [640, 502],
        )
        .unwrap();
        run_headless(options, false).expect("the headless match runs");
        for name in [
            "countdown.png",
            "fight-first.png",
            "fight-mid.png",
            "fight-last.png",
            "results.png",
        ] {
            let path = dir.join(name);
            let image = image::open(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
                .into_rgba8();
            assert_eq!(image.dimensions(), (640, 502), "{name}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `--quit-at` opens the shipped pause dialog, clicks its quit button and
    /// returns to the main menu; both frames are on disk.
    ///
    /// The pause is opened at fight time 0: a box-arena fight between two
    /// heavy AI bots can end inside the first tick (the match's own win rule),
    /// and then there is no `Fight` frame left to pause over. Zero is the one
    /// fight instant every match has.
    #[test]
    fn quitting_through_the_pause_dialog_writes_pause_and_menu() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let dir = std::env::temp_dir().join(format!("ra2-match-quit-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let options = Options::new(
            &game,
            None,
            Vec::new(),
            Some(2.0),
            Some(dir.clone()),
            Some(0.0),
            [640, 502],
        )
        .unwrap();
        run_headless(options, false).expect("the quit run completes");
        for name in ["pause.png", "menu.png"] {
            let path = dir.join(name);
            assert!(path.is_file(), "{}", path.display());
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `camera textbox` shows the name the scene camera reports and hides
    /// after 3.0 s.
    ///
    /// The intro flies the static configuration (`Static 1`, as in the
    /// original's `arena-box-intro` capture); the countdown starts on the
    /// config's `watch` camera, which asks the arena for its first watch camera
    /// (`Watch 1`); F3 asks for the next static camera each press (`Static 1`,
    /// `Static 2`) and F2 is the camera class's own `Orbit Cam`.
    /// `FUN_00477180` shows the box for 3.0 s (`0x4771a2`) and `0x4774f1` hides
    /// it at zero.
    #[test]
    fn the_camera_textbox_names_the_camera_and_hides_after_three_seconds() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let options = Options::new(
            &game,
            None,
            Vec::new(),
            Some(2.0),
            Some(std::env::temp_dir().join("ra2-match-camera-label")),
            None,
            [640, 502],
        )
        .unwrap();
        let mut session = Session::new(&options).expect("the match");
        if session.matched.state() == MatchState::Intro {
            assert_eq!(session.camera_label(), "Static 1", "the intro's camera");
            session.skip_intro();
            session.step();
        }
        assert_eq!(session.matched.state(), MatchState::Countdown);
        assert_eq!(session.camera_label(), "Watch 1");
        assert!(session.show_camera_label());
        session.camera_key(3);
        assert_eq!(session.camera_label(), "Static 1");
        assert!(session.show_camera_label(), "a change re-opens the window");
        session.camera_key(3);
        assert_eq!(session.camera_label(), "Static 2", "the next static camera");
        session.camera_key(2);
        assert_eq!(session.camera_label(), "Orbit Cam");
        session.camera_key(4);
        assert_eq!(session.camera_label(), "Watch 1");
        let steps = (crate::match_mode::CAMERA_LABEL_SECONDS / session.matched.config.dt).ceil()
            as usize
            + 2;
        for _ in 0..steps {
            session.step();
        }
        assert!(!session.show_camera_label(), "the 3.0 s window has run out");
    }

    /// A second match in one process starts on its own arena's camera: nothing
    /// of the first arena's intro, camera iterators or queued camera requests
    /// carries over, even when the first was never finished.
    #[test]
    fn a_second_match_does_not_inherit_the_first_arenas_camera_state() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let make = |arena: &str, tag: &str| {
            let mut options = Options::new(
                &game,
                None,
                Vec::new(),
                Some(2.0),
                Some(std::env::temp_dir().join(format!("ra2-match-two-{tag}"))),
                None,
                [640, 502],
            )
            .unwrap();
            options.arena = game.join("Arenas").join(arena);
            Session::new(&options).expect("the match")
        };
        // The first match is left in its intro, with its cameras advanced.
        let mut first = make("box.py", "a");
        assert_eq!(first.camera_label(), "Static 1");
        first.camera_key(3);
        first.camera_key(3);
        assert_eq!(first.camera_label(), "Static 3");
        drop(first);
        let second = make("box.py", "b");
        assert_eq!(second.matched.state(), MatchState::Intro);
        assert_eq!(
            second.camera_label(),
            "Static 1",
            "the same arena starts over"
        );
        drop(second);
        let third = make("octagon.py", "c");
        let label = third.camera_label();
        assert!(!label.is_empty() && label != "Static 3", "{label}");
        assert!(ra2script::camera::intro_running());
    }

    /// The shipped dialog is the in-game pause popup; its two buttons are named
    /// by its own records.
    #[test]
    fn the_pause_dialog_is_the_shipped_ingame_popup() {
        let Some(game) = game_dir() else { return };
        let dialog = ra2ui::workshop::load(&game.join("UI"), "common/ingame_dialog.gib").unwrap();
        let root = dialog
            .blocks
            .iter()
            .find_map(|b| match b {
                Block::Widget(w) => Some(w),
                _ => None,
            })
            .unwrap();
        assert_eq!(root.name(), "in game popup win");
        assert!(ra2ui::workshop::find(root, "resume button").is_some());
        assert!(ra2ui::workshop::find(root, "quit button").is_some());
    }
}
