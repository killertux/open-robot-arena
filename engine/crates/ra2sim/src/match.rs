//! The headless match: spawn, countdown, fight, win conditions.
//!
//! The match shell in the original is C++ (`docs/11-match-rules.md` §3). This
//! module is the same shell driven by the recon's addresses, on top of the
//! existing [`crate::duel::Duel`] physics loop rather than a second one:
//!
//! * the fixed physics step and the shipped AI's cadence are the duel's;
//! * the arena's own `Tick` runs at [`ra2script::ARENA_TICK_INTERVAL`];
//! * the countdown is the 4.0 s `0x4759c0` writes;
//! * the end conditions are `FUN_00418040` (last robot / last team) and
//!   `FUN_00418770` (points);
//! * the score is the script layer's `+0x20` store, so King of the Hill's
//!   `plus.addPoints` is the same number the results screen reads; in every
//!   other type the hits add their effective damage (the not-KOTH branch of
//!   `FUN_004175b0`, `docs/19-match-fidelity.md` §1).
//!
//! ## The clock and the results delay
//!
//! The match object's constructor defaults to a timed **3 minutes**
//! (`0x41276d`); the setup window's setter (`FUN_00414230`, `0x414230`) stores
//! whole minutes and uses `0` for "No limit". The match clock (`+0xe0`) is
//! that number times 60 and only ticks while `+0x8e` (fighting) is set, so the
//! countdown is not in it. See [`DEFAULT_MATCH_SECONDS`] and
//! `docs/24-match-shell.md` §1.
//!
//! The results delay is the binary's 3.5 s: `FUN_004170e0` writes
//! `+0xec = *0x6a5788` (`0x41721c`) and `0x412fc0` shows the results window
//! when it expires (`0x41309c`).
//!
//! ## What is not a win condition
//!
//! King of the Hill does **not** end on the zone score. Its score is the
//! script's (`king.py:30-53`); the match still ends on the C++ rule — the last
//! robot, or points when an explicit duration runs out.

use std::path::Path;

use glam::Vec3;

use crate::duel::{BotSetup, Duel};
use crate::netevent::{NetEvent, NetRole};
use crate::sim::DriveInput;

/// The pre-fight countdown timer, in seconds: `0x4759c0` writes `+0x1dc` to
/// exactly 4.0 (`0x475a97`).
///
/// **This is not the visible length.** `0x477480` draws
/// `trunc(+0x1dc)` (`0x63aabc`, `fistp` with truncation) in `count number`,
/// and it begins the fight as soon as that truncation is zero
/// (`0x4775bc`–`0x47766b`). The visible sequence is therefore `3`, `2`, `1`,
/// one second each, and the fight starts with 1.0 s still on the timer. The
/// visible countdown is therefore three seconds long, not four; see
/// [`COUNTDOWN_END_SECONDS`].
pub const COUNTDOWN_SECONDS: f32 = 4.0;

/// The countdown value at which `0x477480` hands off to the fight.
///
/// The test is `(int)(+0x1dc) > 0`: while the truncation is 1, `2` or `3` the
/// number is drawn; at 0, `0x416360` starts the fight. So the fight begins
/// when the timer drops below 1.0, not 0.0.
pub const COUNTDOWN_END_SECONDS: f32 = 1.0;

/// The physics step the existing duel tests use.
///
/// The original's *game* update is 50 ms (`ra2.cfg:26 UpdateInterval`); the
/// game's physics step is not established here, and the duel has always been
/// stepped at 60 Hz, so this is the match's default and an explicit field.
pub const DEFAULT_STEP: f32 = 1.0 / 60.0;

/// How long the result screen waits after `Over`, in seconds.
///
/// Measured: `FUN_004170e0` (the results handoff) writes `+0xea = 1` and
/// `+0xec = 3.5f` (`0x41721c`–`0x417229`, constant `0x6a5788` = `0x40600000`);
/// `0x412fc0` decrements `+0xec` and at zero shows `match results window` and
/// `in game popup win` (`0x41309c`–`0x4131fb`). The screen is *filled* at the
/// handoff; this delay is when it is shown.
pub const DEFAULT_RESULTS_DELAY: f32 = 3.5;

/// The match object's constructor default: three minutes, timed.
///
/// `FUN_00412570` writes `+0xd8 = 1` and `+0xdc = 3` (`0x41276d`–`0x41277e`);
/// `0x415500` turns `+0xdc` into seconds as `+0xdc * 60` and stores that in the
/// clock `+0xe0` (`0x415e90`–`0x415ea7`). The setup window's time dropdown
/// replaces it through `FUN_00414230` (`0x414230`): `0` means "No limit"
/// (`+0xd8 = 0`) and any other value is minutes. [`MatchConfig::new`] starts
/// from this default; callers that mean "No limit" pass `None` explicitly.
pub const DEFAULT_MATCH_MINUTES: u32 = 3;

/// [`DEFAULT_MATCH_MINUTES`] in seconds.
pub const DEFAULT_MATCH_SECONDS: f32 = DEFAULT_MATCH_MINUTES as f32 * 60.0;

/// The number `count number` shows for a countdown timer value.
///
/// `0x477480` draws `trunc(+0x1dc)` and only while it is positive
/// (`0x4775b1`–`0x477676`); at zero it starts the fight. The value 4.0 is
/// truncated to 3 on the first update, so the file's sample `3` is the first
/// number. The upper clamp to 3 covers a caller that draws before the match's
/// first `step` (our HUD can): the binary never shows 4 because it subtracts
/// `dt` before drawing. `None` means the fight has begun.
pub fn countdown_number(left: f32) -> Option<u32> {
    if left < COUNTDOWN_END_SECONDS {
        return None;
    }
    Some((left as u32).clamp(1, 3))
}

/// The five game types the original compares against (§1 of the recon).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameType {
    Deathmatch,
    BattleRoyal,
    TeamMatch,
    Tabletop,
    KingOfTheHill,
}

impl GameType {
    /// The exact string the scripts and the C++ compare against.
    pub fn as_str(self) -> &'static str {
        match self {
            GameType::Deathmatch => "DEATHMATCH",
            GameType::BattleRoyal => "BATTLE ROYAL",
            GameType::TeamMatch => "TEAM MATCH",
            GameType::Tabletop => "TABLETOP",
            GameType::KingOfTheHill => "KING OF THE HILL",
        }
    }

    /// Parse a script/config string. The comparison is case-insensitive here
    /// (the file spelling is not always all-caps) but the stored string is the
    /// original's spelling.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "DEATHMATCH" => Some(GameType::Deathmatch),
            "BATTLE ROYAL" => Some(GameType::BattleRoyal),
            "TEAM MATCH" => Some(GameType::TeamMatch),
            "TABLETOP" => Some(GameType::Tabletop),
            "KING OF THE HILL" => Some(GameType::KingOfTheHill),
            _ => None,
        }
    }
}

/// `EventNumBots`: the reader at `0x4e52dc` accepts only 2 or 4 and rewrites
/// anything else to 4 (`0x4e530e`–`0x4e531d`).
pub fn event_num_bots(raw: usize) -> usize {
    if raw == 2 {
        2
    } else {
        4
    }
}

/// Who drives a robot: a brain (`AI/Bindings.py`) or a human player (the local one, or a network
/// player's). A robot with a brain takes its heading from the brain's `nose` (docs/34 section 11.5);
/// a human-driven one has no `AI` object and keeps the `.bot`'s own heading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Controller {
    Ai,
    Human,
}

/// One entry in the event's robot list.
#[derive(Clone, Debug)]
pub struct RobotEntry {
    /// The event's team slot (`Team+0x30` on the robot in the original).
    pub team: usize,
    /// The `.bot` file.
    pub bot_path: String,
    pub controller: Controller,
    /// The team name and logo the results show instead of the file's own team: a
    /// network match's lobby panel names them (docs/38 section 16), the robot
    /// itself having arrived as a `.bot` with no team table behind it.
    pub team_label: Option<(String, String)>,
    /// The player's name the results show above the team (a network match's panel owner).
    pub owner: Option<String>,
}

impl RobotEntry {
    /// An entry a human player drives (no brain is built for it).
    pub fn human(team: usize, bot_path: impl Into<String>) -> Self {
        RobotEntry {
            team,
            bot_path: bot_path.into(),
            controller: Controller::Human,
            team_label: None,
            owner: None,
        }
    }

    /// The results name this team and logo for the robot.
    pub fn labelled(mut self, name: &str, logo: &str) -> Self {
        self.team_label = Some((name.to_string(), logo.to_string()));
        self
    }

    /// The results show this player's name above the team.
    pub fn owned_by(mut self, name: &str) -> Self {
        self.owner = Some(name.to_string());
        self
    }

    pub fn ai(team: usize, bot_path: impl Into<String>) -> Self {
        RobotEntry {
            team,
            bot_path: bot_path.into(),
            controller: Controller::Ai,
            team_label: None,
            owner: None,
        }
    }
}

/// Everything the match needs from the event.
#[derive(Clone, Debug)]
pub struct MatchConfig {
    /// The arena script (`.py`) or model (`.gmf`), as the duel takes it.
    pub arena: String,
    pub game_type: GameType,
    pub robots: Vec<RobotEntry>,
    /// `EventNumBots`, already through [`event_num_bots`]. Battle Royal spawns
    /// this many robots; the other types use the entries as given.
    pub bot_count: usize,
    /// Match length in seconds. `None` is the setup window's "No limit"
    /// (`FUN_00414230(0)` at `0x414238` clears `+0xd8`). The constructor
    /// default is [`DEFAULT_MATCH_SECONDS`] (3 minutes, `0x41276d`); the time
    /// dropdown stores whole minutes in `+0xdc` (`FUN_00414230`, `0x414230`).
    pub duration: Option<f32>,
    /// Seconds of countdown, defaulting to the recovered 4.0.
    pub countdown: f32,
    /// Seconds between `Over` and `Results`.
    pub results_delay: f32,
    /// Physics step.
    pub dt: f32,
    /// The Bot Lab's `Test Robot`: one robot alone in the practice arena. No countdown, no
    /// robot held on its start point, no judges or last-robot rule: the match never ends.
    pub practice: bool,
}

impl MatchConfig {
    pub fn new(arena: impl Into<String>, game_type: GameType, robots: Vec<RobotEntry>) -> Self {
        let bot_count = match_event_num_bots_from_entries(robots.len());
        MatchConfig {
            arena: arena.into(),
            game_type,
            robots,
            bot_count,
            // `FUN_00412570`'s constructor default (`0x41276d`): 3 minutes.
            duration: Some(DEFAULT_MATCH_SECONDS),
            countdown: COUNTDOWN_SECONDS,
            results_delay: DEFAULT_RESULTS_DELAY,
            dt: DEFAULT_STEP,
            practice: false,
        }
    }

    /// One robot alone in the practice arena (the lab's `Test Robot`).
    pub fn with_practice(mut self) -> Self {
        self.practice = true;
        self
    }

    /// Apply the config file's `EventNumBots` with the original's clamp.
    pub fn with_event_num_bots(mut self, raw: usize) -> Self {
        self.bot_count = event_num_bots(raw);
        self
    }

    /// Set the explicit match length in seconds (`None` = no limit).
    pub fn with_duration(mut self, seconds: Option<f32>) -> Self {
        self.duration = seconds;
        self
    }

    pub fn with_results_delay(mut self, seconds: f32) -> Self {
        self.results_delay = seconds;
        self
    }
}

/// The state machine. `Setup` is construction: the world is built and the
/// robots are on their start points before the first `step`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchState {
    Setup,
    /// The arena's own intro (`ArenaIntros=1`): the scripted camera flies while
    /// the robots sit on their start points. It ends when the script calls
    /// `plus.introComplete()` (or the player skips it) and the countdown begins.
    Intro,
    Countdown,
    Fight,
    Over,
    Results,
}

/// Why the match ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WinReason {
    /// `FUN_00418040`: one robot left (King of the Hill ends here too).
    LastRobot,
    /// `FUN_00418040`'s `TEAM MATCH` branch: one team left.
    LastTeam,
    /// The explicit duration ran out; `FUN_00418770` settles it on points.
    Timeout,
}

/// One component's damage in a match report.
///
/// The original keeps the same data per team in `Team+0x12c` (the component id,
/// current and maximum hitpoints) and `Team+0x13c` (the fractured flag); the
/// repairs window reads exactly those two lists (`docs/22-component-repairs.md`).
/// [`hp`](Self::hp) is the component's remaining hitpoints and
/// [`total_hp`](Self::total_hp) is its pristine maximum, so the screen's bar is
/// `(total - current) / total`, the shipped `FUN_004d6480` expression.
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentDamage {
    /// The component's index in the robot design (the `.bot` component order).
    pub index: usize,
    /// The component definition's display name, or its class when it has none.
    pub name: String,
    /// The `.bot` class string (`SpinMotor`, `Wheel`, ...).
    pub class: String,
    /// The component definition's base class (`Weapon`, `SpinMotor`, ...).
    pub base: String,
    /// Remaining hitpoints.
    pub hp: f32,
    /// Pristine hitpoints.
    pub total_hp: f32,
    /// The component is no longer on the robot.
    pub detached: bool,
    /// A single blow larger than the component's `fracture` threshold tore it
    /// off (`FUN_0045f2b0`'s second rule); this is the original's `damage.tga`
    /// row.
    pub fractured: bool,
}

impl ComponentDamage {
    /// Whether the component needs a repair row: it lost hitpoints, came away,
    /// or was fractured.
    pub fn damaged(&self) -> bool {
        self.detached || self.fractured || self.hp < self.total_hp
    }

    /// The remaining fraction the screen's health bar draws, `0.0..=1.0`.
    pub fn fraction(&self) -> f32 {
        if self.total_hp > 0.0 {
            (self.hp / self.total_hp).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// One robot's row in the result screen's data.
#[derive(Clone, Debug)]
pub struct RobotReport {
    /// 0-based robot id, which is also the result slot's index.
    pub slot: usize,
    pub team: usize,
    pub bot_name: String,
    pub team_name: String,
    pub team_logo: String,
    /// The bot's picture, decoded from the `.bot` file's own embedded TGA
    /// snapshot. The original renders a preview from the live model; the file's
    /// snapshot is the picture the game ships, and this port uses it rather
    /// than drawing another.
    pub bot_preview: Option<ra2ui::preview::Preview>,
    pub points: i32,
    pub damage_dealt: f32,
    pub damage_taken: f32,
    /// The HUD's three meters, as fractions 0..1. The widget mapping and fill
    /// geometry are the binary's (`0x476b60` → `0x4b3540`/`0x4b3570`/`0x4b35a0`;
    /// left-anchored, `trunc(fraction * 52)`, armour `trunc(fraction * 26)`).
    /// `battery` and `air` are `Robot+0xa8` / `+0xa4`: the charge left in the
    /// batteries and tanks over their totals ([`crate::energy`], docs/30); a
    /// robot without a battery or tank reports 0 for that bar. `armor` is the
    /// chassis' hitpoints over its maximum.
    pub battery: f32,
    pub air: f32,
    pub armor: f32,
    /// `Robot+0xbc` clamped to 2 (`FUN_00416940`, `FUN_0043e570`): the chassis'
    /// critical hits, which pick the green (0), yellow (1) or red (2+) dot.
    pub dot: u8,
    /// `plus.isDefeated` — state != 0, which includes eliminated.
    pub defeated: bool,
    /// `plus.isEliminated` — state 3 only.
    pub eliminated: bool,
    /// The result screen's `winner text`.
    pub winner: bool,
    /// The result screen's `immobilized text`: the robot's state is 2
    /// (IMMOBILIZED).
    pub immobilized: bool,
    /// The player behind the robot (a network match's lobby panel), for the results' small line.
    pub owner: Option<String>,
    /// Every component, in design order, with its remaining and pristine
    /// hitpoints and its detached/fractured state. `Career::repairs_from_report`
    /// turns the damaged ones into component-level repair rows.
    pub components: Vec<ComponentDamage>,
}

/// The match's result: the shape `ra2ui::results` fills its four slots from.
#[derive(Clone, Debug)]
pub struct MatchReport {
    pub game_type: &'static str,
    pub arena: String,
    pub state: MatchState,
    pub reason: Option<WinReason>,
    /// 0-based ids of the winner(s), as `FUN_004170e0`'s two ids: at most two,
    /// and exactly the rows that get the result screen's winner text
    /// (`0x4884b0`). Empty is a draw: `FUN_00418770` sets `+0x74` on a shared
    /// best score (including all zero), and that byte suppresses the winner
    /// text (`0x4171f4`–`0x4171fc`).
    pub winners: Vec<usize>,
    /// Fight time in seconds, excluding the countdown.
    pub duration: f32,
    /// The explicit match length, when the event set one (`None` = "No limit").
    /// The HUD's `time` box counts down from this; without it the clock counts
    /// up.
    pub duration_limit: Option<f32>,
    pub robots: Vec<RobotReport>,
    /// The duel's human-readable event log (KO, parts, eliminations, a broken
    /// arena `Tick`). Damage is in the per-robot numbers, not repeated here.
    pub log: Vec<String>,
}

impl MatchReport {
    /// The screen's four rows, in robot order. `ra2ui::results::fill` places
    /// the first four and leaves slots with no bot as the file has them.
    pub fn results(&self) -> Vec<ra2ui::results::MatchResult> {
        self.robots
            .iter()
            .map(|r| ra2ui::results::MatchResult {
                team_logo: r.team_logo.clone(),
                bot_preview: r
                    .bot_preview
                    .as_ref()
                    .map(|picture| picture.key(&format!("result-{}", r.slot)))
                    .unwrap_or_else(|| ra2ui::results::PLACEHOLDER.to_string()),
                bot_preview_image: r.bot_preview.clone(),
                points: r.points,
                bot_name: r.bot_name.clone(),
                team_name: r.team_name.clone(),
                immobilized: r.immobilized,
                owner: r.owner.clone(),
                winner: r.winner,
            })
            .collect()
    }
}

/// A live match.
pub struct Match {
    pub config: MatchConfig,
    pub duel: Duel,
    state: MatchState,
    countdown_left: f32,
    /// Fight time only. Measured: `+0xe4` (elapsed) and `+0xe0` (remaining)
    /// are both gated on `+0x8e`, the "fighting" byte (`0x412fe1`–`0x412ff8`),
    /// which `0x416408` sets only when the countdown ends. The clock therefore
    /// excludes the countdown.
    clock: f32,
    over_left: f32,
    reason: Option<WinReason>,
    winners: Vec<usize>,
    teams: Vec<ra2fmt::team::Team>,
    /// `Teams/Teams.txt`'s records by slot (named or not): a career robot under
    /// `Teams/Team<N>/` belongs to record N.
    player_teams: Vec<ra2fmt::team::Team>,
    /// The robots' own `.bot` snapshots, decoded once when the match is built.
    previews: Vec<Option<ra2ui::preview::Preview>>,
    /// The floating damage numbers ([`crate::popup`]).
    popups: crate::popup::DamageDisplay,
    /// `GraphicMessages`: whether a flushed entry becomes a number.
    damage_messages: bool,
}

impl Match {
    /// Build the world and put every robot on an arena start point.
    ///
    /// The robots take the start points in order and face the direction the
    /// model declares (`ra2_render::start_points`, the same call the duel
    /// uses). Battle Royal's four spawns are the model's four start points.
    pub fn new(config: MatchConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let spawn_count = match config.game_type {
            GameType::BattleRoyal => config.bot_count,
            _ => config.robots.len(),
        };
        if config.practice {
            if spawn_count != 1 {
                return Err(format!("a practice run has one robot, got {spawn_count}").into());
            }
        } else if !(2..=4).contains(&spawn_count) {
            return Err(
                format!("a match needs 2 to 4 robots, the event asks for {spawn_count}").into(),
            );
        }
        if config.robots.len() != spawn_count {
            return Err(format!(
                "{} spawns but {} robot entries",
                spawn_count,
                config.robots.len()
            )
            .into());
        }
        if config.dt <= 0.0 {
            return Err("the match step must be positive".into());
        }

        // The script state must be real *before* the arena script is built:
        // `king.py:26` reads the game type in `Activate`, and the intro cameras
        // read the player list.
        ra2script::prepare_match(
            config.game_type.as_str(),
            (0..spawn_count).map(|i| i as i32).collect(),
        );

        let setups: Vec<BotSetup> = config
            .robots
            .iter()
            .map(|r| BotSetup {
                path: r.bot_path.clone(),
                // `None` = the start point's own facing, which is the model's.
                yaw_deg: None,
                spawn: None,
            })
            .collect();
        let humans: Vec<usize> = config
            .robots
            .iter()
            .enumerate()
            .filter(|(_, r)| r.controller == Controller::Human)
            .map(|(i, _)| i)
            .collect();
        // The arena's `Activate(True)` comes at the bell, not at the build (docs/35 section 14).
        ra2script::hold_arena_activation(!config.practice);
        let built = Duel::new_with_humans(&config.arena, &setups, &humans);
        ra2script::hold_arena_activation(false);
        let mut duel = built?;
        for (robot, entry) in duel.robots.iter_mut().zip(&config.robots) {
            robot.team = entry.team;
        }
        // The original holds every robot where it lowered it onto its start
        // point until the bell (docs/29 section 12v).
        if !config.practice {
            duel.hold_robots(true);
        } else {
            // The practice arena plays the robot's impacts against its walls
            // and props (`FUN_0045f3a0` per priced contact, docs/29 section
            // 12ak): the caller consumes `duel.impacts` each step.
            duel.record_impacts = true;
        }

        let teams = team_table(&config.arena);
        let player_teams = player_team_table(&config.arena);
        let previews = config
            .robots
            .iter()
            .map(|entry| bot_preview(&entry.bot_path))
            .collect();
        let state = if config.practice {
            MatchState::Fight
        } else {
            MatchState::Countdown
        };
        Ok(Match {
            countdown_left: if config.practice {
                0.0
            } else {
                config.countdown
            },
            config,
            duel,
            state,
            clock: 0.0,
            over_left: 0.0,
            reason: None,
            winners: Vec::new(),
            teams,
            player_teams,
            previews,
            popups: crate::popup::DamageDisplay::new(),
            damage_messages: true,
        })
    }

    pub fn state(&self) -> MatchState {
        self.state
    }

    /// Whether the game's own AI is driving every robot (false means the duel's
    /// stand-in driver is filling in for a robot `Bindings.py` does not cover).
    pub fn has_shipped_ai(&self) -> bool {
        self.duel.has_shipped_ai()
    }

    /// Fight time so far, in seconds.
    pub fn clock(&self) -> f32 {
        self.clock
    }

    /// Seconds left in the pre-fight countdown (`0.0` once the fight begins).
    pub fn countdown_left(&self) -> f32 {
        self.countdown_left
    }

    /// Start the arena intro: the app has called the arena's `DoIntro(True)`.
    /// Does nothing once the countdown has begun.
    pub fn begin_intro(&mut self) {
        if self.state == MatchState::Countdown && self.countdown_left >= self.config.countdown {
            self.state = MatchState::Intro;
        }
    }

    /// End the intro at once (the player skipped it, or the script finished):
    /// the countdown begins. Does nothing outside the intro.
    pub fn end_intro(&mut self) {
        if self.state == MatchState::Intro {
            self.state = MatchState::Countdown;
        }
    }

    /// Advance one fixed step (`MatchConfig::dt`, the existing duel's step).
    pub fn step(&mut self) {
        self.step_with(&[]);
    }

    /// Advance one fixed step with per-robot player inputs.
    ///
    /// `inputs[i]` is `Some(input)` for a robot the player drives and `None`
    /// for a robot on the shipped AI. The inputs are ignored during the
    /// countdown and after the fight, and are sampled once per fixed step
    /// during `Fight` — the same cadence the physics uses. An empty slice is
    /// [`Match::step`], the AI-versus-AI match.
    pub fn step_with(&mut self, inputs: &[Option<DriveInput>]) {
        let dt = self.config.dt;
        match self.state {
            MatchState::Setup => self.state = MatchState::Countdown,
            MatchState::Intro => {
                // Like the countdown, the physics and the arena tick (which
                // advances the intro's generator) run and no brain drives.
                let idle = vec![DriveInput::default(); self.duel.robots.len()];
                self.duel.step_direct(&idle, dt);
                self.duel.take_score_hits();
                self.duel.popups.clear();
                if !ra2script::camera::intro_running() {
                    self.state = MatchState::Countdown;
                }
            }
            MatchState::Countdown => {
                // The countdown holds the robots still: the physics and the
                // arena still tick, but no brain drives. The original plays the
                // countdown in the match window and begins the fight at zero
                // (`0x477480` → `0x416360`).
                let idle = vec![DriveInput::default(); self.duel.robots.len()];
                self.duel.step_direct(&idle, dt);
                // Nothing scores before the bell; a contact during the countdown
                // is not a hit the match awards.
                self.duel.take_score_hits();
                self.duel.popups.clear();
                self.countdown_left -= dt;
                // `0x4775bc`: the number is drawn while `trunc(+0x1dc) > 0`;
                // at zero `0x416360` begins the fight and `+0x1dc` is set to
                // 0 (`0x47766b`). That happens one second before the 4.0
                // timer would reach 0.
                if countdown_number(self.countdown_left).is_none() {
                    self.countdown_left = 0.0;
                    self.state = MatchState::Fight;
                    // The robots are let go at the bell (docs/29 section 12v).
                    self.duel.hold_robots(false);
                    // The arena's `Activate(True)`: the Octagon's and the Skull's trap
                    // floors count their 90 ticks from here (live: the slide loop starts
                    // 22.6 s after the bell, `audio-match-octagon-scout-alarm`).
                    self.duel.activate_arena();
                    // `FUN_00412fc0` runs its immobility rule only while the
                    // fight is on (`+0x8e`).
                    self.duel.immobility_active = true;
                }
            }
            MatchState::Fight => {
                self.duel.step_with(inputs, dt);
                self.score_hits();
                self.route_popups();
                self.clock += dt;
                if let Some(reason) = self.end_reason() {
                    self.finish(reason);
                }
            }
            MatchState::Over => {
                self.over_left -= dt;
                if self.over_left <= 0.0 {
                    self.state = MatchState::Results;
                }
            }
            MatchState::Results => {}
        }
        self.popups.step(dt, self.damage_messages);
    }

    /// End the match now, through `plus.eliminatePlayer`'s own request queue.
    ///
    /// The tabletop arenas use this for a robot that fell off the table
    /// (`flextop.py:188`); it is also the way a test drives the last-robot rule
    /// without waiting for a chassis to be ground down.
    pub fn eliminate(&mut self, bot: usize) {
        ra2script::state()
            .requests
            .push(ra2script::Request::Eliminate { bot: bot as i32 });
    }

    /// Apply one hit through the match's own damage event.
    ///
    /// It is the path the physics uses (`Duel::apply_hit`): the victim takes the
    /// damage and the match applies its game type's scoring rule. A test drives
    /// a deterministic blow with it; a weapon can use it too.
    pub fn hit(&mut self, attacker: usize, victim: usize, component: usize, damage: f32) {
        if self.state != MatchState::Fight {
            return;
        }
        let point = self
            .duel
            .robots
            .get(victim)
            .map(|r| r.position(&*self.duel.physics))
            .unwrap_or(Vec3::ZERO);
        self.duel
            .apply_hit(Some(attacker), victim, component, damage, point);
        self.score_hits();
        self.route_popups();
    }

    /// Hand the hit requests of this step to the floating-number display, by the rules of
    /// `FUN_004175b0`: nothing in King of the Hill; a hit with an attacker is a green number
    /// for the attacker when the victim was still fighting and, in Team Match, is on another
    /// team (the same test as the score); a hit without one (a hazard, a flame) is a red number
    /// for the robot struck; and only a human player is shown a number
    /// (`player.vtable[0x18]`).
    fn route_popups(&mut self) {
        let requests = std::mem::take(&mut self.duel.popups);
        if self.config.game_type == GameType::KingOfTheHill {
            return;
        }
        let human = self.duel.human_robot;
        for r in requests {
            match r.attacker {
                Some(attacker) => {
                    if !r.victim_alive || human != Some(attacker) {
                        continue;
                    }
                    if self.config.game_type == GameType::TeamMatch {
                        let team = |i: usize| self.duel.robots.get(i).map(|r| r.team);
                        if team(attacker) == team(r.victim) {
                            continue;
                        }
                    }
                    self.popups
                        .request(attacker, true, r.amount, r.point, r.normal);
                }
                None => {
                    if human == Some(r.victim) {
                        self.popups
                            .request(r.victim, false, r.amount, r.point, r.normal);
                    }
                }
            }
        }
    }

    /// The floating damage numbers alive now.
    pub fn damage_numbers(&self) -> &[crate::popup::Number] {
        self.popups.numbers()
    }

    /// `GraphicMessages` (the options window's "damage" switch).
    pub fn set_damage_messages(&mut self, on: bool) {
        self.damage_messages = on;
    }

    /// Turn this step's hits into points, with the rules `FUN_004175b0` applies.
    ///
    /// The award happens for every game type **except** `KING OF THE HILL`
    /// (`0x4176f2` is a *not-equal* test: `FUN_004206a0` returns
    /// `FUN_00402200(...) == 0`, and `FUN_00402200` is string equality). In
    /// `TEAM MATCH` the attacker and victim must be on different teams
    /// (`0x417737`; the team is robot `+0x30`, `FUN_004177c0`); in the other
    /// modes the rule has no team test, so a teammate hit still scores.
    fn score_hits(&mut self) {
        let hits = self.duel.take_score_hits();
        let role = self.duel.net_role;
        if role == NetRole::Host {
            // An arena script's `plus.addPoints` is the host's award to send.
            for (player, amount) in ra2script::take_script_points() {
                if let Ok(player) = usize::try_from(player) {
                    self.duel.net_events.push(NetEvent::Points {
                        player,
                        points: amount as f32,
                        script: true,
                    });
                }
            }
        }
        if role == NetRole::Joiner || self.config.game_type == GameType::KingOfTheHill {
            // A joiner awards nothing (`FUN_004177e0` is the host's); its
            // scores come from the host's `giAddPointsMessage`s.
            return;
        }
        for hit in hits {
            if self.config.game_type == GameType::TeamMatch {
                let Some(victim) = self.duel.robots.get(hit.victim) else {
                    continue;
                };
                let Some(attacker) = self.duel.robots.get(hit.attacker) else {
                    continue;
                };
                if victim.team == attacker.team {
                    continue;
                }
            }
            // `FUN_004178a0`'s `fild score; fadd damage; __ftol; store`
            // (`0x417918`): the effective damage is added and truncated per hit.
            ra2script::add_points_float(hit.attacker as i32, hit.damage);
            if role == NetRole::Host && hit.damage > 0.0 && hit.damage.is_finite() {
                self.duel.net_events.push(NetEvent::Points {
                    player: hit.attacker,
                    points: hit.damage,
                    script: false,
                });
            }
        }
    }

    /// Network play: make this match the host's or a joiner's end
    /// (docs/38 §6.1). Call it right after [`Match::new`].
    pub fn set_net_role(&mut self, role: NetRole) {
        self.duel.net_role = role;
        ra2script::set_net_role(role);
    }

    /// Host: the scoring events decided since the last call, to send.
    pub fn take_net_events(&mut self) -> Vec<NetEvent> {
        // The script awards of a step with no hits still need draining.
        if self.duel.net_role == NetRole::Host {
            for (player, amount) in ra2script::take_script_points() {
                if let Ok(player) = usize::try_from(player) {
                    self.duel.net_events.push(NetEvent::Points {
                        player,
                        points: amount as f32,
                        script: true,
                    });
                }
            }
        }
        std::mem::take(&mut self.duel.net_events)
    }

    /// Joiner: show what the host decided. Hits are applied whole (the joiner's
    /// HUD numbers, damage dealt and component state follow from them), points
    /// are added exactly as the host added them, and a robot the host put out
    /// is out here.
    pub fn apply_net_event(&mut self, event: &NetEvent) {
        match event {
            NetEvent::Hit {
                attacker,
                victim,
                component,
                damage,
                point,
                normal,
            } => {
                self.duel
                    .apply_remote_hit(*attacker, *victim, *component, *damage, *point, *normal);
                self.duel.take_score_hits();
                self.route_popups();
            }
            NetEvent::Points {
                player,
                points,
                script,
            } => {
                if *script {
                    ra2script::add_points_remote(*player as i32, *points as i32);
                } else {
                    ra2script::add_points_float(*player as i32, *points);
                }
            }
            NetEvent::Immobile {
                robot,
                on,
                at,
                seconds,
            } => {
                self.duel
                    .apply_remote_immobile(*robot, *on, glam::Vec3::from_array(*at), *seconds)
            }
            NetEvent::Out { robot, reason } => self.duel.apply_remote_out(*robot, *reason),
        }
    }

    /// A joiner: the host's match time left (`giTimeSyncMessage`, about once a second): the
    /// fight clock follows it.
    pub fn net_time_left(&mut self, left: f32) {
        if let (MatchState::Fight, Some(duration)) = (self.state, self.config.duration) {
            self.clock = (duration - left).max(0.0);
        }
    }

    /// Joiner: the host ended the match (`giMatchWinMessage`, `0x137`): finish
    /// now if the local copy has not, naming the host's winners.
    pub fn net_finish(&mut self, winners: Vec<usize>) {
        if matches!(self.state, MatchState::Over | MatchState::Results) {
            return;
        }
        self.finish(WinReason::LastRobot);
        self.winners = winners;
    }

    /// The match's current result. Valid in every state; the numbers are live.
    pub fn report(&self) -> MatchReport {
        let robots = self
            .duel
            .robots
            .iter()
            .enumerate()
            .map(|(i, robot)| {
                // The result cell shows the bot's own AI team: a bot shipped under
                // `AI/Team<N>/` belongs to `Teams.txt` record N (the folders line up
                // with the table). `robot.team` is only the match grouping, so it
                // is the fallback for bots from elsewhere (a career team's own).
                // A career robot (`Teams/Team<N>/`) is the player table's
                // record N: the exhibition's player slots and the HQ's events
                // show the player's own team, not the AI record of that number.
                let path = self
                    .config
                    .robots
                    .get(i)
                    .map(|entry| entry.bot_path.as_str());
                let team = &match path.and_then(player_team_index) {
                    Some(index) => self.player_teams.get(index).filter(|t| t.is_named()),
                    None => path
                        .and_then(ai_team_index)
                        .or(Some(robot.team))
                        .and_then(|index| self.teams.get(index)),
                };
                let label = self
                    .config
                    .robots
                    .get(i)
                    .and_then(|entry| entry.team_label.as_ref());
                RobotReport {
                    slot: i,
                    team: robot.team,
                    bot_name: crate::scriptai::bot_name(&robot.rig),
                    team_name: label.map(|(name, _)| name.clone()).unwrap_or_else(|| {
                        team.map(|t| t.name.clone())
                            .unwrap_or_else(|| format!("Team {}", robot.team + 1))
                    }),
                    team_logo: label
                        .map(|(_, logo)| logo.clone())
                        .unwrap_or_else(|| team.map(|t| t.logo.clone()).unwrap_or_default()),
                    bot_preview: self.previews.get(i).cloned().flatten(),
                    points: ra2script::points(i as i32),
                    damage_dealt: self.duel.damage_dealt.get(i).copied().unwrap_or(0.0),
                    damage_taken: self.duel.damage_taken.get(i).copied().unwrap_or(0.0),
                    battery: robot
                        .energy
                        .fraction(crate::energy::Channel::Elec, &robot.attached),
                    air: robot
                        .energy
                        .fraction(crate::energy::Channel::Air, &robot.attached),
                    dot: robot.criticals.min(2) as u8,
                    armor: robot
                        .health
                        .first()
                        .zip(robot.rig.components.first())
                        .map(|(left, c)| {
                            if c.hitpoints > 0.0 {
                                (left / c.hitpoints).clamp(0.0, 1.0)
                            } else {
                                0.0
                            }
                        })
                        .unwrap_or(0.0),
                    defeated: !robot.is_alive(),
                    eliminated: robot.eliminated,
                    winner: self.winners.contains(&i),
                    immobilized: robot.immobilized,
                    owner: self
                        .config
                        .robots
                        .get(i)
                        .and_then(|entry| entry.owner.clone()),
                    components: robot
                        .rig
                        .components
                        .iter()
                        .enumerate()
                        .map(|(component, c)| ComponentDamage {
                            index: component,
                            name: if c.name.is_empty() {
                                c.class.clone()
                            } else {
                                c.name.clone()
                            },
                            class: c.class.clone(),
                            base: c.base.clone(),
                            hp: robot.health.get(component).copied().unwrap_or(0.0),
                            total_hp: c.hitpoints,
                            detached: !robot.attached.get(component).copied().unwrap_or(false),
                            fractured: robot.fractured.get(component).copied().unwrap_or(false),
                        })
                        .collect(),
                }
            })
            .collect();
        MatchReport {
            game_type: self.config.game_type.as_str(),
            arena: self.config.arena.clone(),
            state: self.state,
            reason: self.reason,
            winners: self.winners.clone(),
            duration: self.clock,
            duration_limit: self.config.duration,
            robots,
            log: self.duel.log.clone(),
        }
    }

    /// The C++ end conditions, in the order the original checks them.
    ///
    /// `0x412fc0` checks the clock first (`0x41327d`–`0x4133b3`): when `+0xe0`
    /// reaches zero it calls `FUN_00418770(0)` and that handoff sets the
    /// `isMatchOver` byte, so the last-robot/team check later in the same
    /// update is skipped. The time-out therefore wins a tie at the boundary.
    fn end_reason(&self) -> Option<WinReason> {
        if self.config.practice {
            return None;
        }
        let alive: Vec<usize> = (0..self.duel.robots.len())
            .filter(|&i| self.duel.robots[i].is_alive())
            .collect();
        let timed_out = self
            .config
            .duration
            .map(|limit| self.clock >= limit)
            .unwrap_or(false);

        if timed_out {
            return Some(WinReason::Timeout);
        }

        if self.config.game_type == GameType::TeamMatch {
            let mut teams: Vec<usize> = alive.iter().map(|&i| self.duel.robots[i].team).collect();
            teams.sort_unstable();
            teams.dedup();
            if teams.len() <= 1 {
                return Some(WinReason::LastTeam);
            }
            return None;
        }

        if alive.len() <= 1 {
            return Some(WinReason::LastRobot);
        }
        None
    }

    /// The pause dialog's `End Match` in an event (result 4, handler
    /// `0x476830`; the Esc handler `0x475ee0` runs `FUN_00418770(1)` when the
    /// game type, `FUN_004145f0`, is 2): the judges decide on the points now,
    /// exactly as at a time-out (the parameter only asks the robots' `+0x1c`
    /// ids to be set, which every robot's is), and the match goes on to its
    /// results. Only a match in its countdown or fight can be ended.
    pub fn end_by_judges(&mut self) -> bool {
        if !matches!(self.state, MatchState::Countdown | MatchState::Fight) {
            return false;
        }
        self.finish(WinReason::Timeout);
        true
    }

    fn finish(&mut self, reason: WinReason) {
        self.reason = Some(reason);
        self.winners = self.decide_winners(reason);
        self.state = MatchState::Over;
        self.over_left = self.config.results_delay;
        // `0x416450`: the isMatchOver byte. The scripts check it every tick
        // (`Hazards.py:312,325`, `king.py:36`), so it has to be set here.
        ra2script::set_over(true);
        // `FUN_004170e0(p1, p2)` hands the arena the winners: its `Winner` starts the outro music
        // and the closing announcer line (live: `hopp_club` looped and one `End_*` line at the
        // decision, `docs/35` section 14).
        let id = |n: usize| self.winners.get(n).map_or(-1, |&w| w as i32);
        self.duel.arena_winner(id(0), id(1));
    }

    /// `FUN_00418040` gives the last robot / last team; `FUN_00418770` settles a
    /// finish on points.
    ///
    /// The handoff takes at most **two winner ids**: `FUN_004170e0(p1, p2)`
    /// forwards both to `FUN_004172d0` and then to `FUN_004884b0`, which marks
    /// the result row whose id equals `p1` or `p2` (`0x4884b0`), unless the
    /// match's `+0x74` "no winner" byte is set. `FUN_00418770` sets that byte
    /// when the best score is shared (including all-zero:
    /// `0x4189xx`, `+0x74 = 1`), and `FUN_00418040` passes the sole survivor
    /// plus `-1` for a last-robot finish. A non-team finish therefore names
    /// the **unique** highest-scoring alive robot, or nobody on a tie; a team
    /// finish names the first two robots of the winning side in the ordered
    /// player list (`FUN_0047a220(0)` and `FUN_0047a220(1)`, `0x418770`).
    /// The running best starts at 0, so a points winner needs a positive
    /// total; a 0-point best is the equal branch, a draw.
    fn decide_winners(&self, reason: WinReason) -> Vec<usize> {
        if self.config.game_type == GameType::TeamMatch {
            // `FUN_00418040` ends a TEAM MATCH on the last team standing and
            // hands the first survivor (plus a different team's survivor, or
            // -1 when there is none) to the results screen. Points settle only
            // the time-out (`FUN_00418770`).
            if reason == WinReason::LastTeam {
                return (0..self.duel.robots.len())
                    .find(|&i| self.duel.robots[i].is_alive())
                    .into_iter()
                    .collect();
            }
            if reason == WinReason::Timeout {
                // Only alive robots are summed (`0x418770` gates on
                // `FUN_00413810() == 0`), and a tied total is a draw
                // (`local_24 <= local_28` → `+0x74 = 1`).
                let mut totals: std::collections::BTreeMap<usize, i32> =
                    std::collections::BTreeMap::new();
                for (i, robot) in self.duel.robots.iter().enumerate() {
                    if robot.is_alive() {
                        *totals.entry(robot.team).or_insert(0) += ra2script::points(i as i32);
                    }
                }
                let best = totals.values().copied().max().unwrap_or(0);
                let top: Vec<usize> = totals
                    .iter()
                    .filter(|(_, p)| **p == best)
                    .map(|(t, _)| *t)
                    .collect();
                // `local_24`/`local_28` start at 0; a winning side must have
                // a strictly positive total, otherwise the comparison falls
                // into the equal branch and `+0x74` marks a draw.
                if top.len() != 1 || best <= 0 {
                    return Vec::new();
                }
                // `FUN_0047a220(0/1)`: the first two robots of the winning
                // side in the ordered player list.
                return (0..self.duel.robots.len())
                    .filter(|&i| self.duel.robots[i].team == top[0])
                    .take(2)
                    .collect();
            }
            return Vec::new();
        }

        if reason == WinReason::LastRobot {
            return (0..self.duel.robots.len())
                .filter(|&i| self.duel.robots[i].is_alive())
                .collect();
        }

        // `FUN_00418770`'s non-team branch: only alive robots, and the two ids
        // it hands over are `local_2c` (the robot that took the strictly best
        // score) and `-1`. A shared best sets `+0x74` instead and names
        // nobody.
        let mut best: Option<(usize, i32)> = None;
        let mut tied = false;
        for (i, robot) in self.duel.robots.iter().enumerate() {
            if !robot.is_alive() {
                continue;
            }
            let points = ra2script::points(i as i32);
            match best {
                None => best = Some((i, points)),
                Some((_, top)) if points > top => {
                    best = Some((i, points));
                    tied = false;
                }
                Some((_, top)) if points == top => tied = true,
                Some(_) => {}
            }
        }
        match best {
            // The original's running best starts at 0 and only a strictly
            // higher score writes `local_2c`; a 0-point best is the equal
            // branch, i.e. a draw.
            Some((i, points)) if !tied && points > 0 => vec![i],
            _ => Vec::new(),
        }
    }
}

/// `EventNumBots` from a robot list: the original accepts only 2 or 4.
fn match_event_num_bots_from_entries(n: usize) -> usize {
    event_num_bots(n)
}

/// The AI teams' names and logos, for the result cells.
///
/// A `.bot`'s own snapshot, decoded. The file format stores an uncompressed
/// TGA blob; nothing is rendered here.
fn bot_preview(path: &str) -> Option<ra2ui::preview::Preview> {
    let bytes = std::fs::read(path).ok()?;
    let bot = ra2fmt::bot::parse(&bytes).ok()?;
    ra2ui::preview::Preview::decode_tga(bot.snapshot.as_deref()?)
}

/// The `Teams.txt` record a shipped AI bot belongs to: the `N` of `AI/Team<N>/`.
fn ai_team_index(bot_path: &str) -> Option<usize> {
    let parts: Vec<&str> = bot_path.split(['/', '\\']).collect();
    let at = parts.iter().rposition(|p| p.eq_ignore_ascii_case("AI"))?;
    parts
        .get(at + 1)?
        .strip_prefix("Team")
        .or_else(|| parts.get(at + 1)?.strip_prefix("team"))?
        .parse()
        .ok()
}

/// `Teams/Team<N>/...` -> `N`: the player-table record a career robot belongs to.
fn player_team_index(bot_path: &str) -> Option<usize> {
    let parts: Vec<&str> = bot_path.split(['/', '\\']).collect();
    let at = parts
        .iter()
        .rposition(|p| p.eq_ignore_ascii_case("Teams"))?;
    let folder = parts.get(at + 1)?;
    // The folder must be followed by the `.bot` itself (`Teams/Teams.txt` is
    // not a robot).
    parts.get(at + 2)?;
    folder
        .strip_prefix("Team")
        .or_else(|| folder.strip_prefix("team"))?
        .parse()
        .ok()
}

/// `Teams/Teams.txt` in slot order, unnamed records kept so the slot is the
/// index.
fn player_team_table(arena: &str) -> Vec<ra2fmt::team::Team> {
    let Some(root) = ra2_render::find_game_root(Path::new(arena)) else {
        return Vec::new();
    };
    std::fs::read_to_string(ra2fmt::saves::resolve(&root, "Teams/Teams.txt"))
        .ok()
        .and_then(|text| ra2fmt::team::parse_table(&text).ok())
        .map(|table| table.teams)
        .unwrap_or_default()
}

/// The event's team slot is the index into `AI/Teams.txt`; that is the table
/// the shipped AI team folders (`AI/Team1` …) line up with in their own order.
fn team_table(arena: &str) -> Vec<ra2fmt::team::Team> {
    let Some(root) = ra2_render::find_game_root(Path::new(arena)) else {
        return Vec::new();
    };
    std::fs::read_to_string(ra2fmt::saves::resolve(&root, "AI/Teams.txt"))
        .map(|text| ra2fmt::team::parse(&text))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A career robot's team is the player table's record, an AI robot's the
    /// AI table's, and neither is read from the other path.
    #[test]
    fn a_result_rows_team_comes_from_the_table_the_bot_lives_in() {
        assert_eq!(player_team_index("/g/Teams/Team0/Bot1.bot"), Some(0));
        assert_eq!(player_team_index("C:\\g\\Teams\\Team3\\Bot0.bot"), Some(3));
        assert_eq!(player_team_index("/g/AI/Team0/Bot1.bot"), None);
        assert_eq!(player_team_index("/g/Teams/Teams.txt"), None);
        assert_eq!(player_team_index("/g/Robot Designs/M9 Bot.bot"), None);
        assert_eq!(ai_team_index("/g/AI/Team5/Bot1.bot"), Some(5));
        assert_eq!(ai_team_index("/g/Teams/Team0/Bot1.bot"), None);
    }
    use glam::Vec3;
    use pyo3::prelude::*;

    struct Restore(&'static str, Option<String>);

    impl Drop for Restore {
        fn drop(&mut self) {
            match &self.1 {
                Some(v) => std::env::set_var(self.0, v),
                None => std::env::remove_var(self.0),
            }
        }
    }

    fn game_dir() -> Option<std::path::PathBuf> {
        ra2fmt::saves::isolate_for_tests();
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
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

    fn path(game: &Path, p: &str) -> String {
        game.join(p).to_string_lossy().to_string()
    }

    fn countdown_over(m: &mut Match) {
        for _ in 0..10_000 {
            if m.state() != MatchState::Countdown {
                return;
            }
            m.step();
        }
        panic!("the countdown never ended");
    }

    /// The sound requests so far, as `(kind, file name)`, from the script layer's queue.
    fn sound_calls() -> Vec<(&'static str, String)> {
        let st = ra2script::state();
        let name = |id: ra2script::SoundId| {
            st.sounds
                .find(id)
                .map(|s| s.path.rsplit(['/', '\\']).next().unwrap_or("").to_string())
                .unwrap_or_default()
        };
        st.sounds
            .requests()
            .iter()
            .map(|r| match r {
                ra2script::SoundRequest::Play { path, .. } => (
                    "play",
                    path.rsplit(['/', '\\']).next().unwrap_or("").to_string(),
                ),
                ra2script::SoundRequest::Loop { id } => ("loop", name(*id)),
                _ => ("other", String::new()),
            })
            .collect()
    }

    /// The Octagon's trap floors slide 90 arena ticks after the arena's `Activate(True)`, and the
    /// original calls that at the bell, not when the match is built (live,
    /// `audio-match-octagon-scout-alarm`: `hzd_trapfloor_loop` starts 22.6 s after the bell; ours
    /// started it 5 s before the bell, 22.25 s after the build). Also `Winner`: the arena's
    /// outro music (`hopp_club`, looped) and one `End_*` line start when the match is decided.
    #[test]
    fn the_arena_is_activated_at_the_bell_and_told_the_winner() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let was = std::env::var("RA2_HAZARDS").ok();
        std::env::set_var("RA2_HAZARDS", "1");
        let _restore = Restore("RA2_HAZARDS", was);
        let config = MatchConfig::new(
            path(&game, "Arenas/octagon.py"),
            GameType::Deathmatch,
            vec![
                RobotEntry::ai(0, path(&game, "AI/Team0/Bot0.bot")),
                RobotEntry::ai(1, path(&game, "AI/Team0/Bot1.bot")),
            ],
        )
        .with_results_delay(0.0);
        // The script layer's queue is process-wide: start from an empty one.
        let _ = ra2script::take_sound_requests();
        let mut m = Match::new(config).expect("the match builds");
        let slides = |calls: &[(&str, String)]| {
            calls
                .iter()
                .filter(|(k, f)| *k == "loop" && f == "hzd_trapfloor_loop.wav")
                .count()
        };
        // Through the countdown the floors stay shut: 6 s is 24 arena ticks of the old build-time
        // activation, and the slide was due at tick 90.
        countdown_over(&mut m);
        assert_eq!(slides(&sound_calls()), 0, "no floor slides before the bell");
        let dt = m.config.dt;
        for _ in 0..(20.0 / dt) as usize {
            m.step();
        }
        assert_eq!(
            slides(&sound_calls()),
            0,
            "90 arena ticks are 22.5 s after the bell"
        );
        for _ in 0..(4.0 / dt) as usize {
            m.step();
        }
        assert_eq!(
            slides(&sound_calls()),
            4,
            "the four floors slide 22.5 s after the bell"
        );

        assert!(
            !sound_calls().iter().any(|(_, f)| f == "hopp_club.wav"),
            "no outro before the decision"
        );
        m.eliminate(0);
        m.step();
        assert_eq!(m.state(), MatchState::Over, "or Results");
        let calls = sound_calls();
        assert!(
            calls.contains(&("loop", "hopp_club.wav".to_string())),
            "the outro music loops at the decision: {calls:?}"
        );
        assert!(
            calls
                .iter()
                .any(|(k, f)| *k == "play" && f.starts_with("End_")),
            "one End_* line plays at the decision"
        );
    }

    /// M8.2/M8.4/M8.6: a real DEATHMATCH reaches `Over` with a winner.
    ///
    /// Two shipped AI brains fight on the box arena. The match is then ended
    /// through `plus.eliminatePlayer`'s request — the same state-3 path the
    /// tabletop arenas use when a robot falls off (`flextop.py:188`) — so the
    /// last-robot rule is exercised without waiting several minutes for a
    /// chassis to be ground down. Everything printed is the run's own number.
    #[test]
    fn a_deathmatch_reaches_over_with_a_winner() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        // The hazards change where the box fight happens; the shipped AI test
        // turns them off for the same reason.
        let was = std::env::var("RA2_HAZARDS").ok();
        std::env::set_var("RA2_HAZARDS", "0");
        let _restore = Restore("RA2_HAZARDS", was);

        let config = MatchConfig::new(
            path(&game, "Arenas/box.py"),
            GameType::Deathmatch,
            vec![
                RobotEntry::ai(0, path(&game, "AI/Team1/Bot0.bot")),
                RobotEntry::ai(1, path(&game, "AI/Team6/Bot2.bot")),
            ],
        )
        .with_results_delay(0.0);
        let mut m = Match::new(config).expect("the match builds");
        assert_eq!(m.state(), MatchState::Countdown);
        assert!(m.has_shipped_ai(), "the shipped AI did not load");

        // The timer is 4.0 s, but `0x477480` starts the fight when
        // `trunc(timer)` reaches 0: at 2.9 s the number is still 2, and the
        // handoff is one second before the timer's zero.
        let dt = m.config.dt;
        for _ in 0..(2.9 / dt) as usize {
            m.step();
        }
        assert_eq!(
            m.state(),
            MatchState::Countdown,
            "the visible countdown still has a number at 2.9 s"
        );
        countdown_over(&mut m);
        assert_eq!(m.state(), MatchState::Fight);
        // These two shipped AIs mostly idle in the Combat Arena, so whether the
        // 20 s IMMOBILIZED rule (its own test: `duel.rs`) ends the fight before
        // the elimination below depends on how far they happen to roam, which
        // the battery's power budget (a weak battery slows a robot as it
        // drains) moves by a few centimetres. This test is about the
        // last-robot rule, so the immobility rule stays out of it.
        m.duel.immobility_active = false;

        // Let the real fight run long enough to land blows.
        for _ in 0..(30.0 / dt) as usize {
            m.step();
            if m.state() != MatchState::Fight {
                break;
            }
        }
        let before = m.report();
        println!(
            "deathmatch before the decision: t={:.2}s damage dealt={:?} taken={:?} hp={:?}",
            before.duration,
            before
                .robots
                .iter()
                .map(|r| r.damage_dealt)
                .collect::<Vec<_>>(),
            before
                .robots
                .iter()
                .map(|r| r.damage_taken)
                .collect::<Vec<_>>(),
            m.duel
                .robots
                .iter()
                .map(|r| (r.is_alive(), r.health[0]))
                .collect::<Vec<_>>()
        );

        // The tabletop path: eliminate robot 0 and let the next step apply it.
        m.eliminate(0);
        m.step();
        assert_eq!(
            m.state(),
            MatchState::Over,
            "the last-robot rule did not fire"
        );
        let report = m.report();
        println!(
            "deathmatch over: winner={:?} at {:.2}s, points={:?}, damage taken={:?}",
            report.winners,
            report.duration,
            report.robots.iter().map(|r| r.points).collect::<Vec<_>>(),
            report
                .robots
                .iter()
                .map(|r| r.damage_taken)
                .collect::<Vec<_>>()
        );
        assert_eq!(report.winners, vec![1], "the surviving robot wins");
        assert!(report.robots[0].defeated && report.robots[0].eliminated);
        assert!(report.robots[1].winner);
        assert!(report.duration > 25.0, "fight time {}", report.duration);
        // A real blow over the 10-damage gate is points in DEATHMATCH.
        if report
            .robots
            .iter()
            .any(|r| r.damage_dealt >= crate::duel::SCORE_HIT_MIN_DAMAGE)
        {
            assert!(
                report.robots.iter().any(|r| r.points > 0),
                "damage was dealt but the score stayed zero: {:?}",
                report
                    .robots
                    .iter()
                    .map(|r| (r.damage_dealt, r.points))
                    .collect::<Vec<_>>()
            );
        }
        assert!(
            report.log.iter().any(|l| l.contains("eliminated")),
            "no elimination in the log: {:?}",
            report.log
        );
        // The report is the results screen's data, four slots' worth.
        let rows = report.results();
        assert_eq!(rows.len(), 2);
        assert!(rows[1].winner);
        assert!(!rows[0].winner);

        // Over → Results after the configured delay (0 here).
        m.step();
        assert_eq!(m.state(), MatchState::Results);
    }

    /// M11: a landed hit scores its effective damage in the non-KOTH types.
    ///
    /// `FUN_004175b0`'s award is gated on the game type being **not**
    /// `KING OF THE HILL` (`0x4176f2`; `FUN_004206a0` is inequality), and the
    /// score write truncates the sum (`0x417918`). The 10-damage gate is
    /// `FUN_00417330`'s `10.0 <= param_5`. In `TEAM MATCH` the attacker and
    /// victim must differ (`0x417737`); the other modes have no team test.
    #[test]
    fn a_hit_scores_its_effective_damage_except_in_koth_and_against_teammates() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let was = std::env::var("RA2_HAZARDS").ok();
        std::env::set_var("RA2_HAZARDS", "0");
        let _restore = Restore("RA2_HAZARDS", was);

        let build = |game_type: GameType, teams: [usize; 2]| {
            MatchConfig::new(
                path(&game, "Arenas/box.py"),
                game_type,
                vec![
                    RobotEntry::ai(teams[0], path(&game, "AI/Team1/Bot0.bot")),
                    RobotEntry::ai(teams[1], path(&game, "AI/Team6/Bot2.bot")),
                ],
            )
            .with_results_delay(0.0)
        };

        // DEATHMATCH: every hit over the gate adds its effective damage, and the
        // sum is truncated to the integer field per event.
        let mut m = Match::new(build(GameType::Deathmatch, [0, 1])).expect("a deathmatch");
        countdown_over(&mut m);
        m.hit(0, 1, 0, 12.7);
        assert_eq!(ra2script::points(0), 12, "12.7 truncates to 12");
        m.hit(0, 1, 0, 37.9);
        assert_eq!(ra2script::points(0), 49, "37.9 adds 37");
        m.hit(0, 1, 0, 9.9);
        assert_eq!(
            ra2script::points(0),
            49,
            "a hit under the 10.0 gate does not score"
        );
        assert_eq!(ra2script::points(1), 0, "the victim earns nothing");
        println!(
            "deathmatch points: attacker={} victim={}",
            ra2script::points(0),
            ra2script::points(1)
        );

        // TEAM MATCH: the same hit from a teammate is worth nothing; an
        // opponent's is worth its damage.
        let mut team = Match::new(build(GameType::TeamMatch, [0, 0])).expect("a team match");
        countdown_over(&mut team);
        team.hit(0, 1, 0, 40.0);
        assert_eq!(
            ra2script::points(0),
            0,
            "1.20: hitting a teammate adds no points"
        );
        team.duel.robots[1].team = 1;
        team.hit(0, 1, 0, 40.0);
        assert_eq!(ra2script::points(0), 40, "an opponent's hit scores");
        println!("team match points: teammate=0, opponent=40");

        // KING OF THE HILL: damage never scores; its score is the script's.
        let mut koth =
            Match::new(build(GameType::KingOfTheHill, [0, 1])).expect("a king-of-the-hill match");
        countdown_over(&mut koth);
        koth.hit(0, 1, 0, 40.0);
        assert_eq!(
            ra2script::points(0),
            0,
            "King of the Hill's score is the zone script's, not damage"
        );
    }

    /// Network play (docs/38 §6.1): the host prices the hit, scores it and
    /// reports it; a joiner discards its own pricing and shows the host's
    /// hit and points once, so nothing is counted twice.
    #[test]
    fn the_host_prices_and_the_joiner_shows_what_it_is_told() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let was = std::env::var("RA2_HAZARDS").ok();
        std::env::set_var("RA2_HAZARDS", "0");
        let _restore = Restore("RA2_HAZARDS", was);
        let build = || {
            Match::new(
                MatchConfig::new(
                    path(&game, "Arenas/box.py"),
                    GameType::Deathmatch,
                    vec![
                        RobotEntry::ai(0, path(&game, "AI/Team1/Bot0.bot")),
                        RobotEntry::ai(1, path(&game, "AI/Team6/Bot2.bot")),
                    ],
                )
                .with_results_delay(0.0),
            )
            .expect("a deathmatch")
        };

        // Host: the hit lands, scores, and is reported as a hit then points.
        let mut host = build();
        host.set_net_role(NetRole::Host);
        countdown_over(&mut host);
        host.take_net_events();
        host.hit(0, 1, 0, 12.7);
        let events = host.take_net_events();
        assert_eq!(ra2script::points(0), 12);
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(matches!(
            events[0],
            NetEvent::Hit { attacker: Some((0, 0)), victim: 1, component: 0, damage, .. } if damage == 12.7
        ));
        assert_eq!(
            events[1],
            NetEvent::Points {
                player: 0,
                points: 12.7,
                script: false
            }
        );
        let host_taken = host.duel.damage_taken[1];
        // A script's own award is the host's to send too.
        ra2script::add_points(1, 5);
        let events = host.take_net_events();
        assert_eq!(
            events,
            vec![NetEvent::Points {
                player: 1,
                points: 5.0,
                script: true
            }]
        );
        assert_eq!(ra2script::points(1), 5);

        // Joiner: its own copy of the same blow prices nothing and scores
        // nothing; the host's two events do, once.
        let mut joiner = build();
        joiner.set_net_role(NetRole::Joiner);
        countdown_over(&mut joiner);
        joiner.hit(0, 1, 0, 12.7);
        assert_eq!(joiner.duel.damage_taken[1], 0.0, "no local pricing");
        assert_eq!(ra2script::points(0), 0, "no local score");
        assert!(joiner.take_net_events().is_empty());
        ra2script::add_points(0, 9);
        assert_eq!(
            ra2script::points(0),
            0,
            "a joiner ignores its own script award"
        );
        for e in [
            NetEvent::Hit {
                attacker: Some((0, 0)),
                victim: 1,
                component: 0,
                damage: 12.7,
                point: Vec3::ZERO,
                normal: Vec3::ZERO,
            },
            NetEvent::Points {
                player: 0,
                points: 12.7,
                script: false,
            },
            NetEvent::Points {
                player: 1,
                points: 5.0,
                script: true,
            },
        ] {
            joiner.apply_net_event(&e);
        }
        assert_eq!(joiner.duel.damage_taken[1], host_taken, "the host's number");
        assert_eq!(ra2script::points(0), 12);
        assert_eq!(ra2script::points(1), 5);
        assert!(
            joiner.take_net_events().is_empty(),
            "a joiner reports nothing"
        );
        // The host says robot 1 is out: it is, and the match can end.
        joiner.apply_net_event(&NetEvent::Out {
            robot: 1,
            reason: crate::netevent::out::DESTROYED,
        });
        assert!(!joiner.duel.robots[1].is_alive());
        joiner.net_finish(vec![0]);
        assert_eq!(joiner.state(), MatchState::Over);
        assert_eq!(joiner.report().winners, vec![0]);
    }

    /// The floating damage numbers (docs/37 section 7): only a human player is shown one; a hit
    /// with an attacker is a green number for the attacker (the damage that scores), a hit
    /// without one (a hazard) a red number for the robot struck; none in King of the Hill, none
    /// for a team-mate's hit in Team Match, none with `GraphicMessages` off.
    #[test]
    fn only_a_human_player_is_shown_numbers_by_the_scoring_rules() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let was = std::env::var("RA2_HAZARDS").ok();
        std::env::set_var("RA2_HAZARDS", "0");
        let _restore = Restore("RA2_HAZARDS", was);
        let build = |game_type: GameType, teams: [usize; 2]| {
            MatchConfig::new(
                path(&game, "Arenas/box.py"),
                game_type,
                vec![
                    RobotEntry::ai(teams[0], path(&game, "AI/Team1/Bot0.bot")),
                    RobotEntry::ai(teams[1], path(&game, "AI/Team6/Bot2.bot")),
                ],
            )
            .with_results_delay(0.0)
        };
        let texts = |m: &mut Match| {
            // 0.15 s later the entries become numbers
            for _ in 0..(0.25 / m.config.dt) as usize {
                m.step();
            }
            m.damage_numbers()
                .iter()
                .map(|n| (n.player, n.green, n.text.clone()))
                .collect::<Vec<_>>()
        };

        // two AIs: nobody is shown anything
        let mut m = Match::new(build(GameType::Deathmatch, [0, 1])).expect("a deathmatch");
        countdown_over(&mut m);
        m.hit(0, 1, 0, 40.0);
        assert!(texts(&mut m).is_empty());

        // robot 0 is the human: its own hit is a green number, truncated; a hit on it
        // by a hazard (no attacker) is a red one; the AI's hit on it shows nothing
        let mut m = Match::new(build(GameType::Deathmatch, [0, 1])).expect("a deathmatch");
        m.duel.human_robot = Some(0);
        countdown_over(&mut m);
        m.hit(0, 1, 0, 40.9);
        m.duel.apply_hit(None, 0, 0, 25.0, Vec3::ZERO);
        m.hit(1, 0, 0, 33.0);
        m.route_popups();
        assert_eq!(
            texts(&mut m),
            vec![(0, true, "40".to_string()), (0, false, "25".to_string())]
        );

        // `GraphicMessages` off: the entries are made and no number comes of them
        let mut m = Match::new(build(GameType::Deathmatch, [0, 1])).expect("a deathmatch");
        m.duel.human_robot = Some(0);
        m.set_damage_messages(false);
        countdown_over(&mut m);
        m.hit(0, 1, 0, 40.0);
        assert!(texts(&mut m).is_empty());

        // Team Match: a team-mate's hit shows nothing, an opponent's does
        let mut team = Match::new(build(GameType::TeamMatch, [0, 1])).expect("a team match");
        team.duel.human_robot = Some(0);
        countdown_over(&mut team);
        team.duel.robots[1].team = 0;
        team.hit(0, 1, 0, 40.0);
        assert_eq!(team.popups.pending(), 0);
        team.duel.robots[1].team = 1;
        team.hit(0, 1, 0, 40.0);
        assert_eq!(team.popups.pending(), 1);
        assert_eq!(texts(&mut team), vec![(0, true, "40".to_string())]);

        // King of the Hill: no numbers at all, not even a hazard's
        let mut koth = Match::new(build(GameType::KingOfTheHill, [0, 1])).expect("a koth match");
        koth.duel.human_robot = Some(0);
        countdown_over(&mut koth);
        koth.hit(0, 1, 0, 40.0);
        koth.duel.apply_hit(None, 0, 0, 25.0, Vec3::ZERO);
        koth.route_popups();
        assert!(texts(&mut koth).is_empty());
    }

    /// M11: the results rows carry the points a real DEATHMATCH landed.
    ///
    /// The real fight is the M8.2 two-brain one; a deterministic 24-damage blow
    /// is added after it so the row is guaranteed to be non-zero, then the match
    /// is ended and the report's `results()` (the shape the results screen
    /// fills) is read back.
    #[test]
    fn a_real_deathmatch_reports_non_zero_points_to_the_results_rows() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let was = std::env::var("RA2_HAZARDS").ok();
        std::env::set_var("RA2_HAZARDS", "0");
        let _restore = Restore("RA2_HAZARDS", was);
        let config = MatchConfig::new(
            path(&game, "Arenas/box.py"),
            GameType::Deathmatch,
            vec![
                RobotEntry::ai(0, path(&game, "AI/Team1/Bot0.bot")),
                RobotEntry::ai(1, path(&game, "AI/Team6/Bot2.bot")),
            ],
        )
        .with_results_delay(0.0);
        let mut m = Match::new(config).expect("the match builds");
        countdown_over(&mut m);
        m.hit(0, 1, 0, 24.0);
        m.eliminate(1);
        m.step();
        while m.state() != MatchState::Results {
            m.step();
        }
        let report = m.report();
        let rows = report.results();
        println!(
            "results rows: {:?}",
            rows.iter()
                .map(|r| (r.bot_name.as_str(), r.points))
                .collect::<Vec<_>>()
        );
        assert_eq!(rows[0].points, 24);
        assert!(rows.iter().any(|r| r.points > 0));
        assert_eq!(report.robots[0].points, rows[0].points);
    }

    /// A `TEAM MATCH` last-team finish beats the points table.
    ///
    /// `FUN_00418040` hands the surviving side to the results; only a time-out
    /// goes through `FUN_00418770`'s team totals. Give the side that will be
    /// eliminated the larger score, then eliminate it: the survivor wins.
    #[test]
    fn a_team_match_last_team_finish_beats_a_higher_points_total() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let was = std::env::var("RA2_HAZARDS").ok();
        std::env::set_var("RA2_HAZARDS", "0");
        let _restore = Restore("RA2_HAZARDS", was);
        let config = MatchConfig::new(
            path(&game, "Arenas/box.py"),
            GameType::TeamMatch,
            vec![
                RobotEntry::ai(0, path(&game, "AI/Team1/Bot0.bot")),
                RobotEntry::ai(1, path(&game, "AI/Team6/Bot2.bot")),
            ],
        )
        .with_results_delay(0.0);
        let mut m = Match::new(config).expect("a team match");
        countdown_over(&mut m);
        // Team 1 scores 50; its robot is then eliminated, leaving team 0.
        m.hit(1, 0, 0, 50.0);
        assert_eq!(ra2script::points(1), 50);
        m.eliminate(1);
        m.step();
        assert_eq!(m.state(), MatchState::Over);
        let report = m.report();
        println!(
            "team finish: reason={:?} winners={:?} points={:?}",
            report.reason,
            report.winners,
            report.robots.iter().map(|r| r.points).collect::<Vec<_>>()
        );
        assert_eq!(report.reason, Some(WinReason::LastTeam));
        assert_eq!(report.winners, vec![0], "the surviving team wins");
    }

    /// M8.3: King of the Hill's own `Tick` scores 10 points every 0.5 s.
    ///
    /// The constants are `king.py`'s: `initial_delay = 0` (line 32),
    /// `scoring_delay = .5` (line 33), `points = 10` (line 34). The score is
    /// read from the same `+0x20` store `plus.addPoints` writes; nothing else
    /// in this test writes it. `scoringPlayer` on the script is set on the same
    /// two lines as the `addPoints` call, so both are checked.
    #[test]
    fn king_of_the_hill_scores_ten_every_half_second() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let was = std::env::var("RA2_HAZARDS").ok();
        std::env::set_var("RA2_HAZARDS", "0");
        let _restore = Restore("RA2_HAZARDS", was);

        // `Activate` reads the game type, so it must be set before the arena is
        // instantiated (which `Duel::new` does).
        ra2script::prepare_match("KING OF THE HILL", vec![0, 1]);
        let bot = |p: &str, spawn: Option<[f32; 2]>| BotSetup {
            path: path(&game, p),
            yaw_deg: None,
            spawn,
        };
        let mut duel = Duel::new(
            &path(&game, "Arenas/hilltop.py"),
            &[
                // The hill's top / the King_Zone centre is (0, 0).
                bot("AI/Team1/Bot0.bot", Some([0.0, 0.0])),
                // Parked on an outer start point, nowhere near the zone.
                bot("AI/Team6/Bot2.bot", Some([12.0, 0.0])),
            ],
        )
        .expect("the hilltop arena builds a duel");
        assert!(duel.has_shipped_ai(), "the shipped scripts did not load");

        // The hilltop's *visual* hill is not part of the collider this port
        // builds (the static body is the backdrop box), so a robot spawned at
        // the hill's (0, 0) falls to the box floor. For a scoring test the
        // robot is parked in the zone instead: its chassis is placed at the
        // zone's centre every step, which is the same world position the
        // script's phantom query is answered in. The AI is not driving it.
        let chassis = duel.robots[0].body();
        let rotation = duel
            .physics
            .state(chassis)
            .expect("the chassis body")
            .rotation;
        let park = |duel: &mut Duel| {
            duel.physics
                .set_transform(chassis, Vec3::new(0.0, 2.2, 0.0), rotation);
            duel.physics.set_velocity(chassis, Vec3::ZERO, Vec3::ZERO);
        };

        let dt = 1.0 / 60.0;
        let idle = [DriveInput::default(), DriveInput::default()];
        park(&mut duel);
        duel.step_direct(&idle, dt);
        assert_eq!(ra2script::points(0), 0, "the score starts at zero");

        // The phantom is populated from the model and asked by the script's
        // own name.
        let in_zone = ra2script::state()
            .zone_contents
            .get("king_zone")
            .cloned()
            .unwrap_or_default();
        println!("king_zone contents: {in_zone:?}");
        assert_eq!(in_zone.len(), 1, "one robot is on the hill: {in_zone:?}");
        assert_eq!(in_zone[0].0, 1, "robot ids are 1-based");

        // First score: the arena tick is 0.25 s, so the first `> 0.5` arrives
        // on the third tick, about 0.5 s after the bell.
        let mut first = None;
        for _ in 0..(2.0 / dt) as usize {
            park(&mut duel);
            duel.step_direct(&idle, dt);
            if ra2script::points(0) > 0 {
                first = Some(duel.time);
                break;
            }
        }
        let first = first.expect("the hill never scored");
        let first_points = ra2script::points(0);
        println!("first score {first_points} at {first:.3}s");
        assert_eq!(first_points, 10);
        assert!(
            (0.4..0.7).contains(&first),
            "first score at {first}, expected 0.5 s of script time (zero initial delay)"
        );

        // Measure the rate directly, away from boundaries: 4.2 s later the
        // script's `while self.scoring_delay > .5` has fired eight more times.
        let mut delta = 0;
        while duel.time < first + 4.2 {
            park(&mut duel);
            duel.step_direct(&idle, dt);
            delta = ra2script::points(0) - 10;
        }
        println!("score after {:.2}s from the first: +{delta}", 4.2);
        assert_eq!(delta, 80, "10 points per 0.5 s is 8 scores in 4.0 s");

        // Run to 5.0 s of arena time. Twenty 0.25 s ticks have run; nine of
        // them crossed the 0.5 s accumulator and scored (the tenth is exactly
        // 0.5, and `while > 0.5` does not fire).
        while duel.time < 5.0 {
            park(&mut duel);
            duel.step_direct(&idle, dt);
        }
        let score = ra2script::points(0);
        let script: (i32, i32) = pyo3::Python::with_gil(|py| {
            let arena = ra2script::state()
                .arena_object
                .as_ref()
                .expect("the arena is live")
                .clone_ref(py);
            let obj = arena.bind(py);
            (
                obj.getattr("scoringPlayer")
                    .unwrap()
                    .extract()
                    .unwrap_or(-1),
                obj.getattr("scoring_delay")
                    .unwrap()
                    .extract::<f32>()
                    .map(|v| (v * 100.0) as i32)
                    .unwrap_or(-1),
            )
        });
        println!(
            "t={:.2}s score={score} scoringPlayer={} scoring_delay={}",
            duel.time,
            script.0,
            script.1 as f32 / 100.0
        );
        assert_eq!(score, 90, "9 scores of 10 in 5 s");
        assert_eq!(script.0, 0, "the script's own scoringPlayer is robot 0");
        assert_eq!(
            ra2script::points(1),
            0,
            "the robot off the hill never scores"
        );
    }

    /// M8.5: `EventNumBots=4` spawns four robots on four distinct start points.
    #[test]
    fn battle_royal_spawns_four_distinct_start_points() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        assert_eq!(event_num_bots(4), 4);
        assert_eq!(event_num_bots(2), 2);
        assert_eq!(event_num_bots(3), 4, "anything but 2 is rewritten to 4");
        assert_eq!(event_num_bots(1), 4);

        let arena = path(&game, "Arenas/box.py");
        let config = MatchConfig::new(
            arena.clone(),
            GameType::BattleRoyal,
            vec![
                RobotEntry::ai(0, path(&game, "AI/Team1/Bot0.bot")),
                RobotEntry::ai(1, path(&game, "AI/Team3/Bot2.bot")),
                RobotEntry::ai(2, path(&game, "AI/Team6/Bot2.bot")),
                RobotEntry::ai(3, path(&game, "AI/Team9/Bot1.bot")),
            ],
        )
        .with_event_num_bots(4);
        assert_eq!(config.bot_count, 4);
        let m = Match::new(config).expect("a Battle Royal builds");
        assert_eq!(m.duel.robots.len(), 4);
        assert_eq!(ra2script::state().players, vec![0, 1, 2, 3]);

        let starts = ra2_render::start_points(&arena).expect("the box model has start points");
        assert_eq!(starts.len(), 4, "the box arena declares four spawns");
        let mut positions = Vec::new();
        for (i, robot) in m.duel.robots.iter().enumerate() {
            let p = robot.position(&*m.duel.physics);
            println!(
                "battle royal robot {i}: ({:.2}, {:.2}, {:.2})",
                p.x, p.y, p.z
            );
            let nearest = starts
                .iter()
                .map(|(s, _)| (Vec3::new(s.x, p.y, s.z) - p).length())
                .fold(f32::MAX, f32::min);
            assert!(
                nearest < 0.6,
                "robot {i} at {p:?} is {nearest:.2} m from a start point"
            );
            positions.push((p.x, p.z));
        }
        for a in 0..positions.len() {
            for b in a + 1..positions.len() {
                let d = ((positions[a].0 - positions[b].0).powi(2)
                    + (positions[a].1 - positions[b].1).powi(2))
                .sqrt();
                assert!(d > 2.0, "robots {a} and {b} spawned {d:.2} m apart");
            }
        }
        assert!(m.has_shipped_ai());
    }

    /// M8.5: the two-event-bot configuration still spawns two.
    #[test]
    fn a_two_robot_match_uses_two_start_points() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let config = MatchConfig::new(
            path(&game, "Arenas/box.py"),
            GameType::Deathmatch,
            vec![
                RobotEntry::ai(0, path(&game, "AI/Team1/Bot0.bot")),
                RobotEntry::ai(1, path(&game, "AI/Team6/Bot2.bot")),
            ],
        );
        assert_eq!(config.bot_count, 2);
        let m = Match::new(config).expect("a duel match builds");
        assert_eq!(m.duel.robots.len(), 2);
        assert_eq!(ra2script::state().players, vec![0, 1]);
    }

    /// M8.3: a failing arena `Tick` reaches the match log, not a silent `true`.
    ///
    /// The host returns the Python error with its traceback; the duel records
    /// the first one once. This is the end-to-end version of the host test, in
    /// the match's own log (which the report carries).
    #[test]
    fn a_raising_arena_tick_reaches_the_match_log() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let config = MatchConfig::new(
            path(&game, "Arenas/box.py"),
            GameType::Deathmatch,
            vec![
                RobotEntry::ai(0, path(&game, "AI/Team1/Bot0.bot")),
                RobotEntry::ai(1, path(&game, "AI/Team6/Bot2.bot")),
            ],
        );
        let mut m = Match::new(config).expect("the match builds");
        Python::with_gil(|py| {
            let arena = ra2script::state()
                .arena_object
                .as_ref()
                .expect("the arena is live")
                .clone_ref(py);
            let broken = py
                .eval(
                    c"lambda: (_ for _ in ()).throw(RuntimeError('tick exploded'))",
                    None,
                    None,
                )
                .unwrap();
            arena.bind(py).setattr("Tick", broken).unwrap();
        });
        // A countdown step still ticks the arena.
        m.step();
        let report = m.report();
        let line = report
            .log
            .iter()
            .find(|line| line.contains("tick exploded"))
            .unwrap_or_else(|| panic!("the failure was swallowed: {:?}", report.log));
        println!("arena tick failure reported as: {line}");
        assert!(line.contains("arena Tick failed"), "{line}");
        assert!(line.contains("box.py"), "the script is named: {line}");
        assert!(
            line.contains("Traceback") || line.contains("RuntimeError"),
            "the traceback is carried: {line}"
        );
    }

    /// M8.7: the report carries the three HUD bar fractions and the configured
    /// length the clock counts down from.
    #[test]
    fn the_report_carries_the_hud_bars_and_the_time_limit() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let config = MatchConfig::new(
            path(&game, "Arenas/box.py"),
            GameType::Deathmatch,
            vec![
                RobotEntry::ai(0, path(&game, "AI/Team1/Bot0.bot")),
                RobotEntry::ai(1, path(&game, "AI/Team6/Bot2.bot")),
            ],
        )
        .with_duration(Some(12.0));
        let mut m = Match::new(config).expect("the match builds");
        assert_eq!(m.countdown_left(), COUNTDOWN_SECONDS);
        m.step();
        assert!(m.countdown_left() < COUNTDOWN_SECONDS);
        let report = m.report();
        assert_eq!(report.duration_limit, Some(12.0));
        for robot in &report.robots {
            for (name, value) in [
                ("battery", robot.battery),
                ("air", robot.air),
                ("armor", robot.armor),
            ] {
                assert!(
                    (0.0..=1.0).contains(&value),
                    "robot {} {name} = {value}",
                    robot.slot
                );
            }
        }
        println!(
            "bars: {:?}",
            report
                .robots
                .iter()
                .map(|r| (r.bot_name.as_str(), r.battery, r.air, r.armor))
                .collect::<Vec<_>>()
        );
    }

    /// M8.8: the three camera strings are the config's vocabulary, and nothing
    /// else is silently accepted.
    #[test]
    fn the_camera_modes_are_the_configs_three_words() {
        assert!(matches!(
            crate::DuelCamera::from_config("watch"),
            Some(crate::DuelCamera::Watch)
        ));
        assert!(matches!(
            crate::DuelCamera::from_config("static"),
            Some(crate::DuelCamera::Static)
        ));
        assert!(matches!(
            crate::DuelCamera::from_config("orbit"),
            Some(crate::DuelCamera::Orbit { .. })
        ));
        assert!(crate::DuelCamera::from_config("chase").is_none());
        assert_eq!(crate::DuelCamera::Watch.name(), "watch");
        assert_eq!(crate::DuelCamera::Orbit { period: 24.0 }.name(), "orbit");
    }

    /// M8.4: a zero-point time-out is a draw, not an invented tie-break.
    #[test]
    fn a_pointless_timeout_reports_a_draw() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let was = std::env::var("RA2_HAZARDS").ok();
        std::env::set_var("RA2_HAZARDS", "0");
        let _restore = Restore("RA2_HAZARDS", was);
        let config = MatchConfig::new(
            path(&game, "Arenas/box.py"),
            GameType::Deathmatch,
            vec![
                RobotEntry::ai(0, path(&game, "AI/Team1/Bot0.bot")),
                RobotEntry::ai(1, path(&game, "AI/Team6/Bot2.bot")),
            ],
        )
        .with_duration(Some(0.2))
        .with_results_delay(0.0);
        let mut m = Match::new(config).expect("the match builds");
        countdown_over(&mut m);
        while m.state() == MatchState::Fight {
            m.step();
        }
        let report = m.report();
        println!(
            "timeout: state={:?} reason={:?} winners={:?} at {:.2}s",
            report.state, report.reason, report.winners, report.duration
        );
        assert_eq!(report.reason, Some(WinReason::Timeout));
        assert!(report.winners.is_empty(), "0-0 has no winner to name");
        assert!(report.duration >= 0.2 && report.duration < 0.3);
    }

    /// `FUN_00418770` on the time-out: the unique highest alive total wins.
    ///
    /// `FUN_00418770(0)` is the handoff from `0x4133b3`. For a non-team type it
    /// walks the alive robots, keeps the best score, and hands that robot's id
    /// (plus `-1`) to `FUN_004170e0`; the results row with that id gets the
    /// winner text (`0x4884b0`).
    #[test]
    fn a_points_timeout_names_the_unique_higher_total() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let was = std::env::var("RA2_HAZARDS").ok();
        std::env::set_var("RA2_HAZARDS", "0");
        let _restore = Restore("RA2_HAZARDS", was);
        let config = MatchConfig::new(
            path(&game, "Arenas/box.py"),
            GameType::Deathmatch,
            vec![
                RobotEntry::ai(0, path(&game, "AI/Team1/Bot0.bot")),
                RobotEntry::ai(1, path(&game, "AI/Team6/Bot2.bot")),
            ],
        )
        .with_duration(Some(0.2))
        .with_results_delay(0.0);
        let mut m = Match::new(config).expect("the match builds");
        countdown_over(&mut m);
        // The script's own `+0x20` store, the same field `plus.addPoints`
        // writes in King of the Hill.
        ra2script::add_points(0, 7);
        ra2script::add_points(1, 3);
        while m.state() == MatchState::Fight {
            m.step();
        }
        let report = m.report();
        println!(
            "points timeout: reason={:?} winners={:?} points={:?}",
            report.reason,
            report.winners,
            report.robots.iter().map(|r| r.points).collect::<Vec<_>>()
        );
        assert_eq!(report.reason, Some(WinReason::Timeout));
        assert_eq!(report.winners, vec![0], "7 beats 3");
        assert_eq!(report.robots[0].points, 7);
        assert_eq!(report.robots[1].points, 3);
        let rows = report.results();
        assert!(rows[0].winner, "the results row 1 carries the winner text");
        assert!(!rows[1].winner);
    }

    /// A shared best total at the time-out is a draw: `FUN_00418770` sets
    /// `+0x74` and names nobody.
    #[test]
    fn a_tied_points_timeout_is_a_draw() {
        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let was = std::env::var("RA2_HAZARDS").ok();
        std::env::set_var("RA2_HAZARDS", "0");
        let _restore = Restore("RA2_HAZARDS", was);
        let config = MatchConfig::new(
            path(&game, "Arenas/box.py"),
            GameType::Deathmatch,
            vec![
                RobotEntry::ai(0, path(&game, "AI/Team1/Bot0.bot")),
                RobotEntry::ai(1, path(&game, "AI/Team6/Bot2.bot")),
            ],
        )
        .with_duration(Some(0.2))
        .with_results_delay(0.0);
        let mut m = Match::new(config).expect("the match builds");
        countdown_over(&mut m);
        ra2script::add_points(0, 5);
        ra2script::add_points(1, 5);
        while m.state() == MatchState::Fight {
            m.step();
        }
        let report = m.report();
        println!(
            "tied timeout: winners={:?} points={:?}",
            report.winners,
            report.robots.iter().map(|r| r.points).collect::<Vec<_>>()
        );
        assert_eq!(report.reason, Some(WinReason::Timeout));
        assert!(
            report.winners.is_empty(),
            "5-5 shares the best score, so +0x74 suppresses the winner text"
        );
        assert!(report.results().iter().all(|r| !r.winner));
    }

    /// The countdown numbers and the one-second handoff.
    ///
    /// `0x477480` draws `trunc(+0x1dc)` while it is positive and calls
    /// `0x416360` when it reaches zero. With the 4.0 timer that is `3`, `2`,
    /// `1` for one second each, and the fight begins at 1.0 s remaining.
    #[test]
    fn the_countdown_is_three_two_one_and_starts_at_one_second_left() {
        assert_eq!(countdown_number(4.0), Some(3));
        assert_eq!(countdown_number(3.5), Some(3));
        assert_eq!(countdown_number(3.0), Some(3));
        assert_eq!(countdown_number(2.999), Some(2));
        assert_eq!(countdown_number(2.0), Some(2));
        assert_eq!(countdown_number(1.001), Some(1));
        assert_eq!(countdown_number(1.0), Some(1));
        assert_eq!(countdown_number(0.999), None);

        let Some(game) = game_dir() else { return };
        let _guard = ra2script::lock();
        let was = std::env::var("RA2_HAZARDS").ok();
        std::env::set_var("RA2_HAZARDS", "0");
        let _restore = Restore("RA2_HAZARDS", was);
        let config = MatchConfig::new(
            path(&game, "Arenas/box.py"),
            GameType::Deathmatch,
            vec![
                RobotEntry::ai(0, path(&game, "AI/Team1/Bot0.bot")),
                RobotEntry::ai(1, path(&game, "AI/Team6/Bot2.bot")),
            ],
        )
        .with_duration(Some(120.0));
        let mut m = Match::new(config).expect("the match builds");
        let dt = m.config.dt;
        for _ in 0..(2.9 / dt) as usize {
            m.step();
        }
        assert_eq!(m.state(), MatchState::Countdown);
        assert!(
            m.countdown_left() > 1.0,
            "2.9 s in, {:.3} s remain",
            m.countdown_left()
        );
        for _ in 0..(0.2 / dt) as usize {
            m.step();
            if m.state() == MatchState::Fight {
                break;
            }
        }
        assert_eq!(
            m.state(),
            MatchState::Fight,
            "the fight starts once trunc(timer) is zero"
        );
        assert_eq!(m.countdown_left(), 0.0);
    }

    /// The match object's measured constructor default: a timed 3 minutes.
    #[test]
    fn the_match_defaults_to_the_three_minute_clock() {
        let config = MatchConfig::new(
            "arena",
            GameType::Deathmatch,
            vec![RobotEntry::ai(0, "a"), RobotEntry::ai(1, "b")],
        );
        assert_eq!(config.duration, Some(180.0));
        assert_eq!(DEFAULT_MATCH_SECONDS, 180.0);
        assert!(
            config.duration.is_some(),
            "the time group is shown when timed"
        );
    }

    /// The practice arena prices the robot's hits on its walls and records
    /// them for the sounds (`FUN_0045f3a0`; live `audio-practice-test`: the
    /// Test Robot plays `steel_collision` and `hammer_hit` at the far wall,
    /// docs/29 section 12ak). Before, the lab consumed none.
    #[test]
    fn a_practice_robot_driven_into_the_wall_records_priced_hits() {
        let Some(game) = game_dir() else {
            eprintln!("skipping: no local/game");
            return;
        };
        let _guard = ra2script::lock();
        let was = std::env::var("RA2_HAZARDS").ok();
        std::env::set_var("RA2_HAZARDS", "0");
        let _restore = Restore("RA2_HAZARDS", was);
        let config = MatchConfig::new(
            path(&game, "Scripts/Practice.py"),
            GameType::Deathmatch,
            vec![RobotEntry::human(0, path(&game, "Teams/Team0/Bot0.bot"))],
        )
        .with_practice();
        let Ok(mut m) = Match::new(config) else {
            eprintln!("skipping: no practice arena");
            return;
        };
        let mut input = DriveInput::default();
        input.keys.set(17, true);
        let dt = m.config.dt;
        let mut hits = Vec::new();
        for _ in 0..(3.0 / dt) as usize {
            m.step_with(&[Some(input)]);
            hits.extend(std::mem::take(&mut m.duel.impacts));
        }
        let wall = hits
            .iter()
            .filter(|i| i.attacker == crate::combat::NO_ATTACKER)
            .map(|i| i.damage)
            .fold(0.0, f32::max);
        assert!(wall > 20.0, "the wall hit is priced: {hits:?}");
    }
}
