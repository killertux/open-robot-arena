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

pub mod cli;
pub mod frame;
pub mod headless;
pub mod hud;
pub mod match_mode;
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
    if args.match_mode {
        let options = match_mode::Options::new(
            &game,
            args.arena.clone(),
            args.bots.clone(),
            args.seconds,
            args.out.clone(),
            args.quit_at,
            args.size,
        )?;
        return match options.out {
            Some(_) => match_mode::run_headless(options),
            None => window::run_match(options),
        };
    }
    match args.offscreen {
        Some(out) => headless::run(&game, &out, args.size, &args.clicks),
        None => window::run(&game, args.size),
    }
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
