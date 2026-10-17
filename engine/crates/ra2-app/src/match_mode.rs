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
use ra2sim::r#match::{Match, MatchConfig, MatchReport, MatchState, RobotEntry};
use ra2sim::{DuelCamera, GameType};

use crate::hud;
use crate::SHELL_SIZE;

/// The default arena, game type and the two shipped AI brains the match tests
/// use.
pub const ARENA: &str = "Arenas/box.py";
pub const BOTS: [&str; 2] = ["AI/Team1/Bot0.bot", "AI/Team6/Bot2.bot"];
/// How long the offscreen evidence fight runs before its scripted decision.
pub const DEFAULT_SECONDS: f32 = 30.0;

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
    let path = game.join("ra2.cfg");
    let cfg = ra2fmt::cfg::Cfg::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let name = cfg.default_camera();
    DuelCamera::from_config(name)
        .ok_or_else(|| format!("{name:?} in {} is not watch/static/orbit", path.display()))
}

/// A live match plus its screens. The window loop and the offscreen runner
/// share this, so the pictures cannot diverge.
pub struct Session {
    pub matched: Match,
    pub camera: DuelCamera,
    pub paused: bool,
    /// Accumulates wall-clock time for [`Session::advance`].
    accumulator: f32,
    /// The pause dialog's widgets, loaded once.
    pub dialog: Gib,
    presentation: Presentation,
}

struct Presentation {
    ui_root: PathBuf,
    assets: UiAssets,
    hud: Gib,
    results: Option<Gib>,
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
        })
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
        camera: &str,
    ) -> Result<(hud::Hud, UiScene), String> {
        let Some(root) = self.root_mut() else {
            return Err("gamewindow.gib has no root widget".into());
        };
        let bound = hud::hud(root, report, state, clock, countdown_left);
        hud::set_camera(root, camera);
        let placed = ra2ui::resolve_with(&self.hud, SHELL_SIZE, Some(self.assets.faces()));
        // The scene's coordinates are the design screen, not the target: the
        // renderer's `fit` maps 1280×1005 into the window or PNG.
        let size = [SHELL_SIZE[0] as u32, SHELL_SIZE[1] as u32];
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
            let filled = ra2ui::results::fill(root, &template, &report.results());
            println!(
                "results screen: {filled} of {} bot(s) placed",
                report.robots.len()
            );
            self.results = Some(screen);
        }
        let screen = self.results.as_ref().unwrap();
        let placed = ra2ui::resolve_with(screen, SHELL_SIZE, Some(self.assets.faces()));
        Ok(self.assets.scene(&placed, scensize()))
    }

    /// The HUD with the pause dialog over it.
    #[allow(clippy::too_many_arguments)]
    fn pause_scene(
        &mut self,
        report: &MatchReport,
        state: MatchState,
        clock: f32,
        countdown_left: f32,
        camera: &str,
        dialog: &Gib,
    ) -> Result<UiScene, String> {
        let Some(root) = self.root_mut() else {
            return Err("gamewindow.gib has no root widget".into());
        };
        hud::hud(root, report, state, clock, countdown_left);
        hud::set_camera(root, camera);
        let mut placed = ra2ui::resolve_with(&self.hud, SHELL_SIZE, Some(self.assets.faces()));
        let dialog_placed = ra2ui::resolve_with(dialog, SHELL_SIZE, Some(self.assets.faces()));
        placed.extend(dialog_placed);
        Ok(self.assets.scene(&placed, scensize()))
    }
}

impl Session {
    pub fn new(options: &Options) -> Result<Session, String> {
        // The shipped hazards move the box fight around; the duel's own tests
        // turn them off for the deterministic presentation run as well. The
        // environment is read while the world is built, so this must come
        // before `Match::new`.
        std::env::set_var("RA2_HAZARDS", "0");
        let matched = Match::new(options.match_config()).map_err(|e| e.to_string())?;
        Session::with_match(&options.game, matched)
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
        let camera = camera_from_cfg(game)?;
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
            accumulator: 0.0,
            dialog,
            presentation,
        })
    }

    pub fn report(&self) -> MatchReport {
        self.matched.report()
    }

    /// One fixed simulation step, with the evidence script's decision at the
    /// configured length. A natural finish (a destroyed chassis) is left alone.
    pub fn step(&mut self) {
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
        self.matched.step();
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

    pub fn toggle_pause(&mut self) {
        if self.matched.state() == MatchState::Results {
            return;
        }
        self.paused = !self.paused;
        ra2script::set_paused(self.paused);
        println!("pause: {}", if self.paused { "on" } else { "off" });
    }

    pub fn cycle_camera(&mut self) {
        self.camera = match self.camera {
            DuelCamera::Watch => DuelCamera::Static,
            DuelCamera::Static => DuelCamera::Orbit { period: 24.0 },
            _ => DuelCamera::Watch,
        };
        println!("camera: {}", self.camera.name());
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
        if self.paused {
            let dialog = self.dialog.clone();
            self.presentation.pause_scene(
                &report,
                state,
                clock,
                countdown,
                self.camera.name(),
                &dialog,
            )
        } else {
            self.presentation
                .hud_scene(&report, state, clock, countdown, self.camera.name())
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
        self.presentation
            .hud_scene(&report, state, clock, countdown, self.camera.name())
    }

    /// Whether a click in the pause dialog hit a named button.
    pub fn pause_button_at(&self, point: [f32; 2]) -> Option<String> {
        if !self.paused {
            return None;
        }
        let placed = ra2ui::resolve_with(
            &self.dialog,
            SHELL_SIZE,
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
        let placed =
            ra2ui::resolve_with(screen, SHELL_SIZE, Some(self.presentation.assets.faces()));
        let path = ra2ui::input::hit_control(&placed, point[0], point[1])?;
        let names = names_on_path(&placed, &path);
        for wanted in ["continue button", "Quit Session button"] {
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
pub fn run_headless(options: Options) -> Result<MatchReport, String> {
    let out = options
        .out
        .clone()
        .ok_or_else(|| "--match --out DIR is the offscreen path".to_string())?;
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;

    let renderer = Renderer::new().map_err(|e| e.to_string())?;
    let ui = UiRenderer::new_for(renderer.device(), wgpu::TextureFormat::Rgba8Unorm);
    let target = Target::new(renderer.device(), options.size[0], options.size[1]);

    let mut session = Session::new(&options)?;
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
                write_frame(&mut session, &renderer, &ui, &target, &options, "pause.png")?;
                let point = {
                    let dialog = session.dialog.clone();
                    let placed = ra2ui::resolve_with(
                        &dialog,
                        SHELL_SIZE,
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
                &options,
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
                &options,
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
                &options,
                "fight-mid.png",
            )?;
            wrote_mid = true;
        }

        let before = session.matched.state();
        session.step();
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
                    &options,
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
                &options,
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
                &options,
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
        std::fs::read_dir(&out)
            .map(|d| d.filter_map(Result::ok).count())
            .unwrap_or(0),
        out.display()
    );
    Ok(session.report())
}

fn write_frame(
    session: &mut Session,
    renderer: &Renderer,
    ui: &UiRenderer,
    target: &Target,
    options: &Options,
    name: &str,
) -> Result<(), String> {
    let scene = session.scene()?;
    session.render_3d(
        renderer,
        &target.color_view,
        &target.depth_view,
        wgpu::TextureFormat::Rgba8Unorm,
        options.size[0],
        options.size[1],
    )?;
    ui.render_over_to(
        renderer.device(),
        renderer.queue(),
        &scene,
        &target.color_view,
        wgpu::TextureFormat::Rgba8Unorm,
        options.size[0],
        options.size[1],
    )
    .map_err(|e| e.to_string())?;
    let pixels = renderer
        .read_texture(&target.color, options.size[0], options.size[1])
        .map_err(|e| e.to_string())?;
    let path = options.out.as_ref().unwrap().join(name);
    write_png(&path, options.size, pixels)?;
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
struct Target {
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

impl Target {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Target {
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

fn scensize() -> [u32; 2] {
    [SHELL_SIZE[0] as u32, SHELL_SIZE[1] as u32]
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
            bot_preview: String::new(),
            points,
            damage_dealt: 10.0 * (slot + 1) as f32,
            damage_taken: 5.0,
            battery: 1.0,
            air: 1.0,
            armor: 1.0,
            defeated: slot == 1,
            eliminated: false,
            winner: slot == 0,
            immobilized: false,
        }
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
            glyphs > 100,
            "a result screen reads, not a blank panel: {glyphs}"
        );
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
        run_headless(options).expect("the headless match runs");
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
            Some(0.5),
            [640, 502],
        )
        .unwrap();
        run_headless(options).expect("the quit run completes");
        for name in ["pause.png", "menu.png"] {
            let path = dir.join(name);
            assert!(path.is_file(), "{}", path.display());
        }
        std::fs::remove_dir_all(&dir).ok();
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
