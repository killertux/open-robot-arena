//! The exhibition flow: the shipped setup screen starts a match the player
//! drives, with the screen's own arena, game-type and robot choices.
//!
//! The setup screen is `UI/exhibition/exhib_setup.gib`. Its own `start game
//! button` note is *"start the game"*; this module intercepts that click, reads
//! the choices the screen made, and builds a real [`ra2sim::r#match::Match`].
//!
//! ## The robot pickers
//!
//! The Robots tab loads `exhib_robots.gib`, whose four `playerN group` slots
//! carry the note *"load appropriate gib for player N here"*. A click on a slot
//! opens the shipped picker window for that side:
//!
//! * `selectbot_player.gib` (`Select Player Robot`) for slots 1 and 2, the
//!   player's team 1 group;
//! * `selectbot_computer.gib` (`Select Computer Robot`) for slots 3 and 4, the
//!   computer's team 2 group.
//!
//! The picker's `team dropdown` lists the game's own table — `Teams/Teams.txt`
//! (16 records) for the player, `AI/Teams.txt` (15) for the computer — and its
//! `robots grid` gets one `choosebot_cell.gib` per `Robots:` slot of the chosen
//! team. `ok button` accepts; the chosen cell is loaded into the slot the note
//! names. Defaults: the career's player team (the first named `Teams.txt`
//! record) and AI team 1, robot 0.
//!
//! This client uses slot 1 (the player) and slot 3 (the computer's first robot);
//! the two-robot match has no second robot per side, so slots 2 and 4 stay
//! empty. They are the same shipped pickers and can be wired to a four-robot
//! Battle Royal later.
//!
//! ## The arena and game type
//!
//! The arena list is filtered by the game-type option group's `Selection`
//! (`Arenas/__init__.py`'s `enumAvailable`). The list is filled with all twelve
//! arenas until the Game Settings tab has been opened; after that its own
//! `game type optiongroup` decides. If the selected arena does not support the
//! selected type, the first arena that does is used and the run says so.
//!
//! ## Headless evidence
//!
//! `--play --drive "0-2:1,0" --seconds N --out DIR` boots the main menu, clicks
//! through the shipped setup screen (Exhibition, the two robot pickers, Arena
//! with its scroller, hazards, Game Settings, Start Game), then drives robot 0
//! with [`ra2sim::DriveScript`] and writes `play-setup.png`,
//! `play-drive.png` and `play-results.png`.

use std::path::{Path, PathBuf};

use ra2_render::ui::UiRenderer;
use ra2_render::Renderer;
use ra2_shell::{Pick, Shell};
use ra2sim::r#match::{MatchState, RobotReport};
use ra2sim::{DriveScript, GameType, MatchReport};

use crate::match_mode::{self, Session};
use crate::SHELL_SIZE;

/// The shipped setup screen's start control and its own note.
pub const START_BUTTON: &str = "start game button";
pub const START_NOTE: &str = "start the game";
/// The default headless play length.
pub const DEFAULT_SECONDS: f32 = 8.0;
/// The two slots this client uses: the human's (`team 1 group`) and the
/// computer's first robot (`team 2 group`).
pub const PLAYER_PANEL: &str = "player1 group";
pub const OPPONENT_PANEL: &str = "player3 group";

/// The game types the five checkbox records select, in the file's order.
pub const GAME_TYPE_CONTROLS: [(&str, GameType); 5] = [
    ("deathmatch checkbox", GameType::Deathmatch),
    ("battle royal checkbox", GameType::BattleRoyal),
    ("teams checkbox", GameType::TeamMatch),
    ("tabletop checkbox", GameType::Tabletop),
    ("kingofhill checkbox", GameType::KingOfTheHill),
];

/// Parse the command line's spelling: `deathmatch`, `battle-royal`,
/// `team-match`, `tabletop`, `king-of-hill`.
pub fn parse_game_type(name: &str) -> Option<GameType> {
    match name.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "deathmatch" | "death-match" => Some(GameType::Deathmatch),
        "battle-royal" | "battleroyal" => Some(GameType::BattleRoyal),
        "team-match" | "teams" => Some(GameType::TeamMatch),
        "tabletop" | "table-top" => Some(GameType::Tabletop),
        "king-of-hill" | "kingofhill" | "koth" => Some(GameType::KingOfTheHill),
        _ => None,
    }
}

/// The checkbox the game-type tab uses for `game_type`.
pub fn game_type_control(game_type: GameType) -> &'static str {
    GAME_TYPE_CONTROLS
        .iter()
        .find(|(_, declared)| *declared == game_type)
        .map(|(control, _)| *control)
        .unwrap_or("deathmatch checkbox")
}

/// One robot selected from a team's own roster.
#[derive(Clone, Debug)]
pub struct RobotChoice {
    /// `player` or `computer`, the picker window's own word.
    pub side: &'static str,
    /// The record's slot in its table.
    pub team: usize,
    pub team_name: String,
    /// The `.bot` slot number from the team's `Robots:` line.
    pub bot: i32,
    pub bot_name: String,
    pub path: PathBuf,
}

/// What the shipped setup screen selected.
#[derive(Clone, Debug)]
pub struct Choice {
    /// The name the picker shows (`Combat Arena`).
    pub arena_name: String,
    /// The script the name comes from (`box.py`).
    pub arena_script: String,
    pub arena_path: PathBuf,
    /// The row the list selected and how many rows it offered, when it is known.
    pub arena_row: Option<(usize, usize)>,
    /// A note when the selected row had to be replaced or none was selected.
    pub arena_note: Option<String>,
    pub game_type: GameType,
    /// Whether the game-type tab had been loaded and read; when false the
    /// choice is the shipped option group's own `deathmatch` default, not this
    /// client's.
    pub game_type_read: bool,
    /// The selected checkbox's own name (`kingofhill checkbox`).
    pub game_type_control: String,
    /// `Some` when the arena fragment's `hazard checkbox` was present.
    pub hazards: Option<bool>,
    pub player: RobotChoice,
    pub opponent: RobotChoice,
    /// The time dropdown's visible text (`5 minutes`), when read.
    pub time_text: Option<String>,
    /// The seconds the dropdown's value states, when it states one.
    pub time_seconds: Option<f32>,
}

impl Choice {
    /// A one-line description of the two chosen robots for the trace.
    pub fn robots_line(&self) -> String {
        format!(
            "{} team {} ({} slot {}), robot {}: {} ({}) vs {} team {} ({} slot {}), robot {}: {} ({})",
            self.player.side,
            self.player.team_name,
            self.player.side,
            self.player.team,
            self.player.bot,
            self.player.bot_name,
            self.player.path.display(),
            self.opponent.side,
            self.opponent.team_name,
            self.opponent.side,
            self.opponent.team,
            self.opponent.bot,
            self.opponent.bot_name,
            self.opponent.path.display()
        )
    }
}

/// A robot the picker should select: the record and the `.bot` slot number.
fn robot_choice(game: &Path, side: ra2ui::teams::Side, pick: Pick) -> Result<RobotChoice, String> {
    let rosters = ra2ui::teams::rosters(game, side)?;
    let roster = rosters.get(pick.team).ok_or_else(|| {
        format!(
            "{} team {} is not in {}",
            side.name(),
            pick.team,
            side.table()
        )
    })?;
    if !roster.robots.contains(&pick.bot) {
        return Err(format!(
            "{} team {} ({}) has no Bot{}.bot slot; its Robots line is {:?}",
            side.name(),
            pick.team,
            roster.name,
            pick.bot,
            roster.robots
        ));
    }
    let path = ra2ui::teams::bot_path(game, side, pick.team, pick.bot);
    if !path.is_file() {
        return Err(format!(
            "{} ({} team {} robot {}) is not a .bot file",
            path.display(),
            side.name(),
            pick.team,
            pick.bot
        ));
    }
    let (bot_name, _) = ra2ui::teams::bot_info(&path)
        .ok_or_else(|| format!("{} is not a .bot this build reads", path.display()))?;
    Ok(RobotChoice {
        side: side.name(),
        team: pick.team,
        team_name: roster.name.clone(),
        bot: pick.bot,
        bot_name,
        path,
    })
}

/// The default picks: the career's player team and AI team 1, both robot 0.
pub fn default_picks(game: &Path) -> Result<(Pick, Pick), String> {
    let players = ra2ui::teams::rosters(game, ra2ui::teams::Side::Player)?;
    let player_team = ra2ui::teams::career_player_slot(&players)
        .ok_or_else(|| "Teams/Teams.txt has no named team".to_string())?;
    let player_bot = *players[player_team]
        .robots
        .first()
        .ok_or_else(|| format!("player team {player_team} has no robots"))?;
    let computers = ra2ui::teams::rosters(game, ra2ui::teams::Side::Computer)?;
    let opponent_team = if computers.len() > 1 { 1 } else { 0 };
    let opponent_bot = *computers
        .get(opponent_team)
        .and_then(|team| team.robots.first())
        .ok_or_else(|| "AI/Teams.txt has no team 1 robot".to_string())?;
    Ok((
        Pick {
            team: player_team,
            bot: player_bot,
        },
        Pick {
            team: opponent_team,
            bot: opponent_bot,
        },
    ))
}

/// Read the setup screen's choices.
///
/// The arena is filtered by the game type the option group selected. If the
/// selected row does not support that type, the first arena that does is used
/// and [`Choice::arena_note`] says so; with no row selected the default is
/// `Arenas/box.py` when it supports the type, otherwise the first that does.
pub fn choice_from_shell(game: &Path, shell: &Shell) -> Result<Choice, String> {
    let game_type_read = shell.game_type_choice().is_some();
    let game_type = shell
        .game_type_choice()
        .and_then(GameType::parse)
        .unwrap_or(GameType::Deathmatch);
    let available = ra2ui::arenas::available(game, game_type.as_str())?;
    if available.is_empty() {
        return Err(format!("no shipped arena declares {}", game_type.as_str()));
    }
    let all = ra2ui::arenas::all(game)?;
    let requested = shell.arena_choice().map(str::to_string);
    let selected = requested
        .as_deref()
        .and_then(|script| available.iter().find(|arena| arena.script == script))
        .cloned();
    let (arena, note) = if let Some(arena) = selected {
        (arena, None)
    } else if let Some(script) = &requested {
        let old = all
            .iter()
            .find(|arena| arena.script == *script)
            .map(|arena| format!("{} ({})", arena.name, arena.script))
            .unwrap_or_else(|| script.clone());
        (
            available[0].clone(),
            Some(format!(
                "{old} does not support {}; using the first arena that does",
                game_type.as_str()
            )),
        )
    } else {
        let default = available
            .iter()
            .find(|arena| {
                arena
                    .script
                    .eq_ignore_ascii_case(match_mode::DEFAULT_ARENA_SCRIPT)
            })
            .or_else(|| {
                available
                    .iter()
                    .find(|arena| arena.script == match_mode::DEFAULT_ARENA_SCRIPT)
            })
            .unwrap_or(&available[0])
            .clone();
        (
            default,
            Some(format!(
                "no arena row selected; using the default arena that supports {}",
                game_type.as_str()
            )),
        )
    };
    let arena_row = shell
        .arena_row()
        .and_then(|(row, rows)| {
            let list = if rows == available.len() {
                &available
            } else if rows == all.len() {
                &all
            } else {
                return None;
            };
            list.get(row)
                .filter(|candidate| candidate.script == arena.script)
                .map(|_| (row, rows))
        })
        .or_else(|| {
            available
                .iter()
                .position(|candidate| candidate.script == arena.script)
                .map(|row| (row, available.len()))
        });

    let (default_player, default_opponent) = default_picks(game)?;
    let player = robot_choice(
        game,
        ra2ui::teams::Side::Player,
        shell.player_pick().unwrap_or(default_player),
    )?;
    let opponent = robot_choice(
        game,
        ra2ui::teams::Side::Computer,
        shell.opponent_pick().unwrap_or(default_opponent),
    )?;

    let time_choice = shell.time_choice();
    Ok(Choice {
        arena_name: arena.name.clone(),
        arena_script: arena.script.clone(),
        arena_path: game.join("Arenas").join(&arena.script),
        arena_row,
        arena_note: note,
        game_type,
        game_type_read,
        game_type_control: game_type_control(game_type).to_string(),
        hazards: shell.hazard_choice(),
        player,
        opponent,
        time_text: shell.time_text().map(str::to_string),
        time_seconds: time_choice.flatten(),
    })
}

/// Build the player-controlled match the setup screen asks for.
///
/// `seconds` is the headless run's explicit match length and overrides the
/// time dropdown; `time_override` is `--time`, which replaces the value the
/// dropdown shows (its option list is not in the data).
pub fn session_from_shell(
    game: &Path,
    shell: &Shell,
    seconds: Option<f32>,
    time_override: Option<f32>,
) -> Result<(Session, Choice), String> {
    let mut choice = choice_from_shell(game, shell)?;
    if let Some(seconds) = time_override {
        choice.time_seconds = Some(seconds);
    }
    let duration = seconds.or(choice.time_seconds);
    let session = Session::exhibition(
        game,
        &choice.arena_path,
        choice.game_type,
        &choice.player.path,
        &choice.opponent.path,
        duration,
        choice.hazards,
    )?;
    print_choices(&choice, seconds, time_override);
    Ok((session, choice))
}

/// The selections the shipped screen made, in one block.
fn print_choices(choice: &Choice, seconds: Option<f32>, time_override: Option<f32>) {
    if let Some(note) = &choice.arena_note {
        println!("arena: {note}");
    }
    println!(
        "exhibition: {} ({}), game type {}{}, hazards {}",
        choice.arena_name,
        choice.arena_script,
        choice.game_type.as_str(),
        if choice.game_type_read {
            ""
        } else {
            " (default: the Game Settings tab was not opened)"
        },
        match choice.hazards {
            Some(true) => "on (hazard checkbox checked)",
            Some(false) => "off (hazard checkbox unchecked)",
            None => "left at the script's default (no hazard checkbox)",
        }
    );
    println!(
        "player team: {} (Teams/Teams.txt slot {}), robot {}: {} ({})",
        choice.player.team_name,
        choice.player.team,
        choice.player.bot,
        choice.player.bot_name,
        choice.player.path.display()
    );
    println!(
        "opponent team: {} (AI/Teams.txt slot {}), robot {}: {} ({})",
        choice.opponent.team_name,
        choice.opponent.team,
        choice.opponent.bot,
        choice.opponent.bot_name,
        choice.opponent.path.display()
    );
    let arena_row = choice
        .arena_row
        .map(|(row, rows)| format!("arena row {row} of {rows}"))
        .unwrap_or_else(|| "arena row not read (the row was replaced)".to_string());
    println!(
        "selected rows: {arena_row}, game type {}, player team {}, player robot {}, opponent team {}, opponent robot {}",
        choice.game_type_control,
        choice.player.team,
        choice.player.bot,
        choice.opponent.team,
        choice.opponent.bot
    );
    if let Some(seconds) = time_override {
        println!(
            "match time: {seconds:.0} s (--time override; the dropdown shows {})",
            choice.time_text.as_deref().unwrap_or("no value")
        );
    } else {
        match (&choice.time_text, choice.time_seconds) {
            (Some(text), Some(seconds)) => println!(
                "match time: {text} ({seconds:.0} s from the time dropdown's displayed value; its option list is not in the data)"
            ),
            (Some(text), None) => println!(
                "match time: {text} (no limit; the time dropdown's option list is not in the data)"
            ),
            (None, _) => println!(
                "match time: no value read (the Game Settings tab was not loaded); the match has no limit"
            ),
        }
    }
    if let Some(seconds) = seconds {
        println!(
            "match length for this run: {seconds:.0} s (--seconds when given, otherwise --time or the default; it overrides the dropdown's value)"
        );
    }
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
    /// `--arena-index N`: the row to click, after scrolling it into view.
    pub arena_index: Option<usize>,
    /// `--gametype`: open Game Settings first and select the type.
    pub game_type: Option<GameType>,
    /// `--team`/`--bot` and `--opponent-team`/`--opponent-bot`.
    pub team: Option<usize>,
    pub bot: Option<i32>,
    pub opponent_team: Option<usize>,
    pub opponent_bot: Option<i32>,
    /// `--time SECONDS`: the setup screen's match length, replacing the value
    /// the dropdown displays.
    pub time: Option<f32>,
}

impl Options {
    /// The picker target after the flag defaults.
    fn picks(&self) -> Result<(Pick, Pick), String> {
        let (mut player, mut opponent) = default_picks(&self.game)?;
        if let Some(team) = self.team {
            player.team = team;
            player.bot = 0;
        }
        if let Some(bot) = self.bot {
            player.bot = bot;
        }
        if let Some(team) = self.opponent_team {
            opponent.team = team;
            opponent.bot = 0;
        }
        if let Some(bot) = self.opponent_bot {
            opponent.bot = bot;
        }
        Ok((player, opponent))
    }
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
    /// The clicks the run made, for the trace.
    pub steps: usize,
}

/// The shipped click flow up to (not including) `start game button`.
///
/// The robot tabs, the arena list's scroller, the hazards checkbox and (when
/// `--gametype` names one) the game type are all driven through the screen's
/// own controls. The returned shell has every choice recorded and its content
/// slot is the arena tab, so the setup frame is the arena list with the chosen
/// row selected; the caller opens Game Settings for its time value.
pub fn select_setup(options: &Options) -> Result<(Shell, Vec<ra2_shell::Step>), String> {
    let mut shell = Shell::boot(&options.game, SHELL_SIZE)?;
    let mut steps = Vec::new();
    steps.push(shell.click_named("exhibition button")?);

    // Game Settings first when the command line names the type: the arena list
    // then populates with that type's own arenas.
    if let Some(game_type) = options.game_type {
        steps.push(shell.click_named("gametype button")?);
        steps.push(shell.click_named(game_type_control(game_type))?);
    }

    // The two robot pickers, through the Robots tab and its own slots.
    let (player, opponent) = options.picks()?;
    steps.push(shell.click_named("robots button")?);
    for (panel, pick, side) in [
        (PLAYER_PANEL, player, ra2ui::teams::Side::Player),
        (OPPONENT_PANEL, opponent, ra2ui::teams::Side::Computer),
    ] {
        let rosters = ra2ui::teams::rosters(&options.game, side)?;
        let roster = rosters.get(pick.team).ok_or_else(|| {
            format!(
                "{} team {} is not in {}",
                side.name(),
                pick.team,
                side.table()
            )
        })?;
        let column = roster
            .robots
            .iter()
            .position(|bot| *bot == pick.bot)
            .ok_or_else(|| {
                format!(
                    "{} team {} ({}) has no Bot{}.bot; its Robots line is {:?}",
                    side.name(),
                    pick.team,
                    roster.name,
                    pick.bot,
                    roster.robots
                )
            })?;
        steps.push(shell.click_region(panel)?);
        steps.push(shell.click_named("Down Button")?);
        steps.push(shell.click_popup_team(pick.team)?);
        steps.push(shell.click_popup_robot(column)?);
        steps.push(shell.click_named("ok button")?);
    }

    // The arena picker: the list is filtered when Game Settings was opened
    // first, otherwise it offers all twelve. `--arena-index` clicks the row
    // after the list scrolls it into view.
    steps.push(shell.click_named("arena button")?);
    let arena_list = match options.game_type {
        Some(game_type) => ra2ui::arenas::available(&options.game, game_type.as_str())?,
        None => ra2ui::arenas::all(&options.game)?,
    };
    let row = if let Some(index) = options.arena_index {
        if index >= arena_list.len() {
            return Err(format!(
                "--arena-index {index} is past the {} arena rows",
                arena_list.len()
            ));
        }
        Some(index)
    } else if let Some(script) = &options.arena_script {
        arena_list
            .iter()
            .position(|arena| arena.script.eq_ignore_ascii_case(script))
    } else {
        arena_list.iter().position(|arena| {
            arena
                .script
                .eq_ignore_ascii_case(match_mode::DEFAULT_ARENA_SCRIPT)
        })
    };
    match row {
        Some(row) => steps.push(shell.click_list_row(ra2ui::arenas::LIST, row)?),
        None => println!(
            "arena: {} is not offered for {}; no row clicked, the first supporting arena is used",
            options.arena_script.as_deref().unwrap_or("box.py"),
            options
                .game_type
                .map(GameType::as_str)
                .unwrap_or("DEATHMATCH")
        ),
    }
    // The checkbox belongs to the arena fragment, so it is clicked while the
    // arena tab is loaded, before the Game Settings tab replaces it.
    steps.push(shell.click_named("hazard checkbox")?);
    Ok((shell, steps))
}

/// The headless run: click through the shipped setup screen, drive, write three
/// frames and print the numbers.
pub fn run_headless(options: &Options) -> Result<Outcome, String> {
    std::fs::create_dir_all(&options.out).map_err(|e| format!("{}: {e}", options.out.display()))?;
    let script = DriveScript::parse(&options.drive);

    // 1. The shipped route to the setup screen and every choice.
    println!(
        "booted {} from {}",
        ra2_shell::nav::BOOT_SCREEN,
        options.game.display()
    );
    let (mut shell, mut steps) = select_setup(options)?;
    for step in &steps {
        println!("{}", crate::describe(step));
    }

    // 2. The setup frame, before Start Game: the arena list with the chosen
    // row selected. Game Settings is opened after it so the frame shows the
    // arena and its scroller.
    let setup = options.out.join("play-setup.png");
    crate::headless::render_png(&mut shell, &options.game, &setup, options.size)?;
    println!("  wrote {}", setup.display());
    if shell.game_type_choice().is_none() {
        let gametype = shell.click_named("gametype button")?;
        println!("{}", crate::describe(&gametype));
        steps.push(gametype);
    }

    // 3. The click that starts the match. The tab shares `content group` with
    // the arena picker, so the choices live in the shell rather than the
    // document on screen.
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

    let (mut session, choice) =
        session_from_shell(&options.game, &shell, Some(options.seconds), options.time)?;
    // The match really carries the screen's choices.
    let config = &session.matched.config;
    for (index, chosen) in [(0, &choice.player), (1, &choice.opponent)] {
        let relative = chosen
            .path
            .strip_prefix(&options.game)
            .unwrap_or(&chosen.path)
            .to_string_lossy()
            .to_string();
        if !config.robots[index].bot_path.ends_with(&relative) {
            return Err(format!(
                "the match's robot {index} is {:?}, not {}",
                config.robots[index].bot_path,
                chosen.path.display()
            ));
        }
    }
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
        steps: steps.len(),
    })
}
