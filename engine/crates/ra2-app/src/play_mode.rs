//! The exhibition flow: the shipped setup screen starts a match the player
//! drives.
//!
//! The setup screen is `UI/exhibition/exhib_setup.gib`. Its own `start game
//! button` note is *"start the game"*; this module intercepts that click, reads
//! the arena row the picker selected, the game-type tab's selection and the
//! arena's `hazard checkbox`, and builds a real [`ra2sim::r#match::Match`].
//!
//! The robots are not selectable yet: the player is the career/team table's
//! team 0, robot 0 (`Teams/Team0/Bot0.bot`), and the opponent is the first
//! shipped AI team's first robot (`AI/Team1/Bot0.bot`). The arena picker's rows
//! are real controls; the game-type tab is read when it has been opened.
//!
//! ## Headless evidence
//!
//! `--play --drive "0-2:1,0;2-4:1,1" --seconds N --out DIR` boots the main menu,
//! clicks through the shipped setup screen (Exhibition, Arena, one arena row,
//! hazards off for a deterministic frame, Game Settings, Start Game), then
//! drives robot 0 with [`ra2sim::DriveScript`] and writes `play-setup.png`,
//! `play-drive.png` and `play-results.png`.

use std::path::{Path, PathBuf};

use ra2_render::ui::UiRenderer;
use ra2_render::Renderer;
use ra2_shell::Shell;
use ra2sim::r#match::{MatchState, RobotReport};
use ra2sim::{DriveScript, GameType, MatchReport};

use crate::match_mode::{self, Session};
use crate::SHELL_SIZE;

/// The shipped setup screen's start control and its own note.
pub const START_BUTTON: &str = "start game button";
pub const START_NOTE: &str = "start the game";
/// The default headless play length.
pub const DEFAULT_SECONDS: f32 = 8.0;

/// What the shipped setup screen selected.
#[derive(Clone, Debug)]
pub struct Choice {
    /// The name the picker shows (`Combat Arena`).
    pub arena_name: String,
    /// The script the name comes from (`box.py`).
    pub arena_script: String,
    pub arena_path: PathBuf,
    pub game_type: GameType,
    /// Whether the game-type tab had been loaded and read; when false the
    /// choice is `DEATHMATCH` by this client's default, not the screen's.
    pub game_type_read: bool,
    /// `Some` when the arena fragment's `hazard checkbox` was present.
    pub hazards: Option<bool>,
}

/// Read the setup screen's choices. The defaults are the ones the task names:
/// `Arenas/box.py` when no row is selected, `DEATHMATCH` when the game-type tab
/// has not been opened.
pub fn choice_from_shell(game: &Path, shell: &Shell) -> Result<Choice, String> {
    let script = shell
        .arena_choice()
        .unwrap_or(match_mode::DEFAULT_ARENA_SCRIPT)
        .to_string();
    let arenas = ra2ui::arenas::all(game)?;
    let arena = arenas
        .iter()
        .find(|a| a.script.eq_ignore_ascii_case(&script))
        .or_else(|| arenas.iter().find(|a| a.name.eq_ignore_ascii_case(&script)))
        .ok_or_else(|| format!("the arena picker has no {script:?}"))?;
    let game_type_read = shell.game_type_choice().is_some();
    let game_type = shell
        .game_type_choice()
        .and_then(GameType::parse)
        .unwrap_or(GameType::Deathmatch);
    Ok(Choice {
        arena_name: arena.name.clone(),
        arena_script: arena.script.clone(),
        arena_path: game.join("Arenas").join(&arena.script),
        game_type,
        game_type_read,
        hazards: shell.hazard_choice(),
    })
}

/// Build the player-controlled match the setup screen asks for.
pub fn session_from_shell(
    game: &Path,
    shell: &Shell,
    seconds: Option<f32>,
) -> Result<(Session, Choice), String> {
    let choice = choice_from_shell(game, shell)?;
    let player = game.join(match_mode::PLAYER_BOT);
    let opponent = game.join(match_mode::OPPONENT_BOT);
    let session = Session::exhibition(
        game,
        &choice.arena_path,
        choice.game_type,
        &player,
        &opponent,
        seconds,
        choice.hazards,
    )?;
    println!(
        "exhibition: {} ({}), game type {}{}, hazards {}",
        choice.arena_name,
        choice.arena_script,
        choice.game_type.as_str(),
        if choice.game_type_read {
            ""
        } else {
            " (default: the game-type tab was not opened)"
        },
        match choice.hazards {
            Some(true) => "on (hazard checkbox checked)",
            Some(false) => "off (hazard checkbox unchecked)",
            None => "left at the script's default (no hazard checkbox)",
        }
    );
    let report = session.report();
    println!(
        "player robot: {} (robot 0, {}) vs {} ({})",
        report.robots[0].bot_name,
        match_mode::PLAYER_BOT,
        report.robots[1].bot_name,
        match_mode::OPPONENT_BOT
    );
    println!(
        "robot selection: fixed in this client (the setup screen's robots tab is not selectable yet)"
    );
    Ok((session, choice))
}

/// What `--play` was asked to do.
#[derive(Clone, Debug)]
pub struct Options {
    pub game: PathBuf,
    /// `<start>-<end>:<throttle>,<steer>` segments, the existing
    /// [`DriveScript`] grammar. Time is fight time (`Match::clock`).
    pub drive: String,
    pub seconds: f32,
    pub out: PathBuf,
    pub size: [u32; 2],
    /// The arena script to pick in the list (`box.py` by default).
    pub arena_script: Option<String>,
}

/// The run's printed numbers, returned so a test can assert them.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub choice: Choice,
    pub player_bot: String,
    pub opponent_bot: String,
    pub start: [f32; 3],
    pub end: [f32; 3],
    /// Net displacement of robot 0 between spawn and the result screen.
    pub distance: f32,
    pub state: MatchState,
    /// Whether `MatchState::Over` was observed before `Results`.
    pub over: bool,
    pub report: MatchReport,
    pub frames: Vec<PathBuf>,
}

/// The headless run: click through the shipped setup screen, drive, write three
/// frames and print the numbers.
pub fn run_headless(options: &Options) -> Result<Outcome, String> {
    std::fs::create_dir_all(&options.out).map_err(|e| format!("{}: {e}", options.out.display()))?;
    let script = DriveScript::parse(&options.drive);

    // 1. The shipped route to the setup screen and the arena row.
    let mut shell = Shell::boot(&options.game, SHELL_SIZE)?;
    println!(
        "booted {} from {}",
        shell.screen_id(),
        options.game.display()
    );
    let mut steps = Vec::new();
    steps.push(shell.click_named("exhibition button")?);
    steps.push(shell.click_named("arena button")?);
    let arenas = ra2ui::arenas::all(&options.game)?;
    let wanted = options
        .arena_script
        .as_deref()
        .unwrap_or(match_mode::DEFAULT_ARENA_SCRIPT);
    let row = arenas
        .iter()
        .position(|a| a.script.eq_ignore_ascii_case(wanted) || a.name.eq_ignore_ascii_case(wanted))
        .ok_or_else(|| format!("the arena picker has no {wanted:?}"))?;
    steps.push(shell.click_list_row(ra2ui::arenas::LIST, row)?);
    // The checkbox is the arena fragment's own; off keeps the evidence frame
    // in the middle of the box arena. It is still a real click on the shipped
    // control, and the choice is what the match receives.
    steps.push(shell.click_named("hazard checkbox")?);
    for step in &steps {
        println!("{}", crate::describe(step));
    }

    // 2. The setup frame, before Start Game: the arena picker with the chosen
    // row selected.
    let setup = options.out.join("play-setup.png");
    crate::headless::render_png(&mut shell, &options.game, &setup, options.size)?;
    println!("  wrote {}", setup.display());

    // 3. Read the game-type tab, then the click that starts the match. The tab
    // shares `content group` with the arena picker, so the arena choice is
    // remembered by the shell rather than re-read from the document.
    let gametype = shell.click_named("gametype button")?;
    println!("{}", crate::describe(&gametype));
    let start = shell.click_named(START_BUTTON)?;
    println!("{}", crate::describe(&start));
    if start.hit_widget() != Some(START_BUTTON) || start.note.is_some() {
        return Err(format!(
            "{START_BUTTON:?} did not start the game (hit {:?}, note {:?})",
            start.hit, start.note
        ));
    }
    if let Some(note) = shell
        .document()
        .walk()
        .find(|w| w.name() == START_BUTTON)
        .map(|w| w.notes().trim())
    {
        if note != START_NOTE {
            return Err(format!("{START_BUTTON:?} carries note {note:?}"));
        }
    }

    let (mut session, choice) = session_from_shell(&options.game, &shell, Some(options.seconds))?;
    let start_position = session
        .robot_position(session.player_robot)
        .ok_or_else(|| "the match has no player robot".to_string())?;

    // 4. Drive at the match's fixed step. The script's time is fight time.
    let renderer = Renderer::new().map_err(|e| e.to_string())?;
    let ui = UiRenderer::new_for(renderer.device(), wgpu::TextureFormat::Rgba8Unorm);
    let target = match_mode::Target::new(renderer.device(), options.size[0], options.size[1]);
    let mut audio = crate::audio::AudioSystem::new_silent(
        &options.game,
        &crate::options::read_cfg(&options.game),
    );
    audio.stop_music("the exhibition match is on screen");
    audio.drain_match(session.audio_listener());

    let drive_shot_at = (options.seconds * 0.5).clamp(0.5, 2.0);
    let mut wrote_drive = false;
    let mut over = false;
    let mut last_state = session.matched.state();
    let mut frames = vec![setup];
    while session.matched.state() != MatchState::Results {
        let state = session.matched.state();
        if state != last_state {
            println!(
                "  {:?} -> {:?} at {:.2}s, points {:?}, winners {:?}",
                last_state,
                state,
                session.matched.clock(),
                session
                    .report()
                    .robots
                    .iter()
                    .map(|r| r.points)
                    .collect::<Vec<_>>(),
                session.report().winners
            );
            last_state = state;
        }
        if state == MatchState::Over {
            over = true;
        }
        if state == MatchState::Fight && !wrote_drive && session.matched.clock() >= drive_shot_at {
            let path = options.out.join("play-drive.png");
            session.set_drive(script.at(session.matched.clock()));
            match_mode::write_frame(
                &mut session,
                &renderer,
                &ui,
                &target,
                options.size,
                &options.out,
                "play-drive.png",
            )?;
            frames.push(path);
            wrote_drive = true;
        }
        session.set_drive(script.at(session.matched.clock()));
        session.step();
        audio.drain_match(session.audio_listener());
    }

    // 5. The result screen and its own numbers.
    let end_position = session
        .robot_position(session.player_robot)
        .ok_or_else(|| "the player robot disappeared from the match".to_string())?;
    let path = options.out.join("play-results.png");
    match_mode::write_frame(
        &mut session,
        &renderer,
        &ui,
        &target,
        options.size,
        &options.out,
        "play-results.png",
    )?;
    frames.push(path);

    let report = session.report();
    let player: &RobotReport = &report.robots[0];
    let distance = (end_position - start_position).length();
    let start = [start_position.x, start_position.y, start_position.z];
    let end = [end_position.x, end_position.y, end_position.z];
    println!(
        "player robot: {} (robot 0), start ({:.2}, {:.2}, {:.2}), end ({:.2}, {:.2}, {:.2}), distance {:.2} m",
        player.bot_name, start[0], start[1], start[2], end[0], end[1], end[2], distance
    );
    println!(
        "state {:?}, winner {}, points {:?}",
        report.state,
        if report.winners.is_empty() {
            "none (draw)".to_string()
        } else {
            report
                .winners
                .iter()
                .map(|&i| report.robots[i].bot_name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        },
        report
            .robots
            .iter()
            .map(|r| (r.bot_name.as_str(), r.points))
            .collect::<Vec<_>>()
    );
    println!(
        "wrote {} frame(s) to {}",
        frames.len(),
        options.out.display()
    );

    Ok(Outcome {
        choice,
        player_bot: player.bot_name.clone(),
        opponent_bot: report.robots[1].bot_name.clone(),
        start,
        end,
        distance,
        state: report.state,
        over,
        report,
        frames,
    })
}
