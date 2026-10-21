//! `ra2-app` — the shell as an application.
//!
//! One frame builder serves both ways of presenting the shell: [`frame`] turns
//! the live [`ra2_shell::Shell`] into a [`ra2_render::ui::UiScene`], and the
//! same scene is either drawn into a window's surface or into an offscreen
//! texture that is written to a PNG. The headless path is not a second renderer;
//! it is the same loop with the window replaced by a file.
//!
//! ```text
//! cargo run -q --release -p ra2-app -- --game ../local/game
//! cargo run -q --release -p ra2-app -- --game ../local/game \
//!     --offscreen /tmp/m7.png --clicks "573.5,534.8;787.2,446.0"
//! cargo run -q --release -p ra2-app -- --game ../local/game \
//!     --match --seconds 30 --out /tmp/ra2-match
//! ```

pub mod audio;
pub mod build_mode;
pub mod cli;
pub mod crowd;
pub mod flow;
pub mod frame;
pub mod headless;
pub mod hq_mode;
pub mod hud;
pub mod match_mode;
pub mod match_sounds;
pub mod net_play;
pub mod options;
pub mod play_mode;
pub mod prize_scene;
pub mod reminder;
pub mod viewport;
pub mod window;

use std::path::{Path, PathBuf};

use ra2_shell::Step;

/// The shell's design size, in its own coordinates. The window scales this; the
/// shell itself always lays out at 1280×1005, exactly as the `.gib` screens and
/// the CPU composite assume.
pub const SHELL_SIZE: [f32; 2] = [1280.0, 1005.0];

/// Run the binary's arguments. `--help` is handled here so the library is the
/// only entry point a test needs.
pub fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let args = cli::parse(args)?;
    if args.help {
        println!("{}", cli::usage());
        return Ok(());
    }
    let game = find_game(args.game.as_deref())?;
    // The startup settings. `--size` overrides the file; the offscreen paths
    // keep their own target size and ignore the display keys, as they have no
    // window to size.
    let cfg = options::read_cfg(&game);
    let configured_size = options::display_size(&cfg);
    let window_size = if args.size_set {
        args.size
    } else {
        configured_size
    };
    build_mode::set_debug_panel_default(args.debug_panel);
    if args.build_mode {
        let options = build_mode::Options {
            game: game.clone(),
            start: args.start.clone(),
            name: args.save_as.clone().unwrap_or_else(|| "M9 Bot".to_string()),
            out: args.out.clone(),
            size: if args.out.is_some() {
                args.size
            } else {
                window_size
            },
            seconds: args.seconds.unwrap_or(4.0),
            view: args.build_view.clone(),
        };
        return match options.out {
            Some(_) => build_mode::run_headless(&options),
            None => window::run_build(options, args.audio_report),
        };
    }
    if args.hq {
        let mut options = hq_mode::Options::new(
            &game,
            args.simulate_event,
            args.out.clone(),
            if args.out.is_some() {
                args.size
            } else {
                window_size
            },
            args.seconds,
            args.report_damage,
        )?;
        options.simulate_season = args.simulate_season;
        options.bench_undamaged = args.bench_undamaged;
        return match options.out {
            Some(_) => hq_mode::run_headless(options),
            None => window::run_hq(options, args.audio_report),
        };
    }
    if args.play {
        let out = args
            .out
            .clone()
            .ok_or_else(|| "--play needs --out DIR (the headless exhibition run)".to_string())?;
        let seconds = match args.seconds {
            Some(seconds) => seconds,
            // Without an explicit evidence length, `--time` is both the
            // setup's match length and this run's length.
            None => args.time.unwrap_or(play_mode::DEFAULT_SECONDS),
        };
        if !(seconds.is_finite() && seconds > 0.0) {
            return Err(format!("--seconds must be positive, got {seconds}"));
        }
        if args.arena_index.is_some() && args.arena.is_some() {
            return Err("--play takes --arena or --arena-index, not both".to_string());
        }
        let game_type = match args.gametype.as_deref() {
            Some(name) => Some(play_mode::parse_game_type(name).ok_or_else(|| {
                format!(
                    "--gametype {name:?} is not deathmatch/battle-royal/team-match/tabletop/king-of-hill"
                )
            })?),
            None => None,
        };
        let arena_script = args.arena.as_ref().map(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("box.py")
                .to_string()
        });
        let options = play_mode::Options {
            game,
            drive: args.drive.clone().unwrap_or_default(),
            seconds,
            out,
            size: args.size,
            arena_script,
            arena_index: args.arena_index,
            game_type,
            team: args.team,
            bot: args.bot,
            opponent_team: args.opponent_team,
            opponent_bot: args.opponent_bot,
            time: args.time,
        };
        return play_mode::run_headless(&options).map(|_| ());
    }
    if args.match_mode {
        for earlier in &args.earlier_arenas {
            // Load (and drop) the earlier arenas' models: only the session's
            // texture manager keeps anything from them.
            let path = if earlier.is_absolute() || earlier.exists() {
                earlier.clone()
            } else {
                game.join(earlier)
            };
            ra2_render::collect_items(&path.to_string_lossy())
                .map_err(|e| format!("--earlier-arenas {}: {e}", path.display()))?;
        }
        let options = match_mode::Options::new(
            &game,
            args.arena.clone(),
            args.bots.clone(),
            args.seconds,
            args.out.clone(),
            args.quit_at,
            if args.out.is_some() {
                args.size
            } else {
                window_size
            },
        )?;
        if !args.camera_shots.is_empty() {
            return match_mode::run_camera_shots(options, &args.camera_shots);
        }
        return match options.out {
            Some(_) => match_mode::run_headless(options, args.audio_report).map(|_| ()),
            None => window::run_match(options, args.audio_report),
        };
    }
    match args.offscreen {
        Some(out) => headless::run(
            &game,
            &out,
            args.size,
            &args.clicks,
            &args.click_names,
            args.audio_report,
        ),
        None => window::run(&game, window_size, args.audio_report),
    }
}

/// Whether a click actually activated a control.
///
/// `Shell::click` returns `hit: "nothing"` for a point that reached no control
/// (the bars, empty chrome, a miss). A hit on a control is an activation even
/// when the control's action is only a state change (an option group's radio),
/// which is why this is not "the screen changed".
pub fn activated(step: &Step) -> bool {
    !step.hit.is_empty() && step.hit != "nothing"
}

/// Whether a click plays `Sounds\\button_click.wav`.
///
/// The original plays it from the push-button class's mouse-up handler
/// (`0x5ec1e0`) and from nowhere else: a tab, a checkbox, a radio group member,
/// a slider, a scroll bar and a list row are silent. Live (`ui-click-survey`):
/// the main-menu buttons, Options, OK, Cancel, Main Menu, Continue, Workshop
/// and Back to HQ click; the exhibition tabs, the arena list, the options
/// checkboxes, radios and sliders do not. The shell names the widget class first
/// in `Step::hit` (`Button "start game button"`).
pub fn plays_click(step: &Step) -> bool {
    activated(step) && (step.hit.starts_with("Button") || step.hit == "eject button")
}

/// A click's one-line trace, the same shape `ra2-shell` prints.
pub fn describe(step: &Step) -> String {
    format!(
        "  {} -> {} [{}] at {:.1},{:.1}{}",
        step.command,
        step.screen,
        if step.hit.is_empty() {
            "nothing"
        } else {
            &step.hit
        },
        step.point[0],
        step.point[1],
        match (&step.note, step.content.as_deref()) {
            (_, Some(content)) => format!(", content {content}"),
            (Some(note), None) => format!(", note {note:?}"),
            (None, None) => String::new(),
        }
    )
}

/// The install: `--game`, then `RA2_GAME`/`RA2_GAME_DIR`, then the repository's
/// `local/game` above the working directory or this crate.
pub fn find_game(explicit: Option<&Path>) -> Result<PathBuf, String> {
    fn check(path: &Path) -> Result<PathBuf, String> {
        ra2_shell::game::check(path)?;
        path.canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    if let Some(dir) = explicit {
        return check(dir);
    }
    for key in ["RA2_GAME", "RA2_GAME_DIR"] {
        if let Some(dir) = std::env::var_os(key) {
            return check(Path::new(&dir));
        }
    }
    let mut starts = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    starts.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    for start in starts {
        let mut dir = start.as_path();
        loop {
            let candidate = dir.join("local/game");
            if candidate.is_dir() && ra2_shell::game::check(&candidate).is_ok() {
                return check(&candidate);
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
    }
    ra2_shell::game::find()
}
