//! The Team HQ on screen and the event flow: `--hq`, `--simulate-event`.
//!
//! The screens are the shipped ones. The HQ is `teamhq/teamhq.gib` with one of
//! its four tabs loaded into `hq content group` through the same notes the shell
//! reads (`load hq_robots.gib into hq content group`, …). The event detail is
//! `teamhq/event_bracket.gib`, the match is M8's presentation
//! (`ra2sim::match::Match` + `ra2ui::results`), and the award is
//! `teamhq/event_winner.gib`.
//!
//! ## Where every number comes from
//!
//! The career model is the only source. This module builds the screens' plain
//! [`ra2ui::hq::View`] from `Career`: the roster is the player's `Robots:` line,
//! the league is `career.season_points()` + `Team+0x5c..+0x94` through
//! [`ra2ui::standings`], the history counters are the team record's own fields,
//! and the prizes are `Career::prizes`. No point is added, divided or ranked
//! here; the award is `Career::award_match` and the event close is
//! `Career::finish_event`.
//!
//! ## The event
//!
//! An event's arena is `Events.txt`'s `arena` line; its game type is the arena
//! script's own `game_types` list (Combat Arena → `DEATHMATCH`, LumaZone →
//! `KING OF THE HILL`, FlexTop → `TABLETOP`). The career model does not expose a
//! bracket, a round count or an opponent list (`docs/16-career.md` §6), so one
//! match is played against the first AI team and the result is the event's
//! result. That is stated in the run's output and in `docs/18-hq-ui.md`.
//!
//! ## Frames
//!
//! `--hq --simulate-event N --out DIR` writes `hq.png` (the roster), `event.png`
//! (the bracket), `match.png` (the HUD), `results.png`
//! (`common/matchresults.gib`), `award.png` (`event_winner.gib`),
//! `repairs-list.png` and `repairs-done.png` (`teamhq/repairs.gib`), and the
//! after-run evidence `hq-after.png` (league) and `hq-history.png` (prizes).

use std::path::{Path, PathBuf};

use ra2_render::ui::{UiAssets, UiRenderer, UiScene};
use ra2_render::Renderer;
use ra2fmt::event::Event;
use ra2fmt::gib::{AttachmentKind, Block, Gib, Widget, WidgetKind};
use ra2sim::career::{
    BracketStatus, Career, CareerPolicy, Entrant, EventRecord, MatchAward, RoundOutcome,
};
use ra2sim::r#match::{Match, MatchConfig, MatchReport, MatchState, RobotEntry};
use ra2sim::GameType;

use ra2ui::repairs::{self, Click as RepairClick, Repairs, Row as RepairRow};

use crate::match_mode;
use crate::SHELL_SIZE;

/// The schedule seed for a fresh career. The original's RNG was not recovered
/// (`docs/16-career.md` §3), so the schedule is explicit; the value is ours.
pub const SEED: u64 = 7;

/// One click of the repairs screen's clock. The original advances a progress
/// bar over time (`FUN_004d6e10`) against the shipped 30-minute clock
/// (`FUN_0058b150(0x1e,1)` → `00:30:00`); this headless/UI tick is six
/// seconds of that clock. The value is ours, the model's `seconds` is the
/// explicit repair time.
pub const REPAIR_TICK_SECONDS: f32 = 6.0;

/// What `--hq` was asked to do.
#[derive(Clone, Debug)]
pub struct Options {
    pub game: PathBuf,
    /// A row of the season schedule (0-based), not an `Events.txt` id.
    pub simulate_event: Option<usize>,
    /// Play the whole nine-event season.
    pub simulate_season: bool,
    pub out: Option<PathBuf>,
    pub size: [u32; 2],
    pub seconds: f32,
    /// Evidence/test knob: damage added to the player's match report before the
    /// career award. Zero by default; see `cli::Args::report_damage`.
    pub report_damage: f32,
}

impl Options {
    pub fn new(
        game: &Path,
        simulate_event: Option<usize>,
        out: Option<PathBuf>,
        size: [u32; 2],
        seconds: Option<f32>,
        report_damage: Option<f32>,
    ) -> Result<Options, String> {
        let seconds = seconds.unwrap_or(match_mode::DEFAULT_SECONDS);
        if !(seconds.is_finite() && seconds > 0.0) {
            return Err(format!("--seconds must be positive, got {seconds}"));
        }
        let report_damage = report_damage.unwrap_or(0.0);
        if !(report_damage.is_finite() && report_damage >= 0.0) {
            return Err(format!(
                "--report-damage must be non-negative, got {report_damage}"
            ));
        }
        Ok(Options {
            game: game.to_path_buf(),
            simulate_event,
            simulate_season: false,
            out,
            size,
            seconds,
            report_damage,
        })
    }
}

/// The frames a headless event run writes. `--simulate-event` keeps the full
/// evidence set; `--simulate-season` writes the season-level four.
#[derive(Clone, Copy, Default)]
struct FramePlan<'a> {
    hq_events: Option<&'a str>,
    bracket: Option<&'a str>,
    round: Option<&'a str>,
    match_frame: Option<&'a str>,
    results: Option<&'a str>,
    award: Option<&'a str>,
    repairs_list: Option<&'a str>,
    repairs_done: Option<&'a str>,
    hq_after: Option<&'a str>,
    hq_history: Option<&'a str>,
    season_hq: bool,
}

impl FramePlan<'static> {
    fn single() -> Self {
        FramePlan {
            hq_events: Some("hq-events.png"),
            bracket: Some("event.png"),
            match_frame: Some("match.png"),
            results: Some("results.png"),
            award: Some("award.png"),
            repairs_list: Some("repairs-components.png"),
            repairs_done: Some("repairs-done.png"),
            hq_after: Some("hq-after.png"),
            hq_history: Some("hq-history.png"),
            ..FramePlan::default()
        }
    }

    fn season() -> Self {
        FramePlan::default()
    }
}

/// The season's last screen: the golden sawblade when the player tops the
/// table, the season-results window otherwise (`FUN_0046e2b0`'s two branches).
fn season_end_screen(ui_root: &Path, view: &ra2ui::hq::View) -> Result<Gib, String> {
    if view.stats.current_rank == 1 {
        let mut gib = ra2ui::hq::load(ui_root, "teamhq/golden_sawblade.gib")?;
        ra2ui::hq::fit_splits_in(&mut gib);
        Ok(gib)
    } else {
        ra2ui::hq::season_results(ui_root, view)
    }
}

/// What one simulated event did, for the output and the tests.
#[derive(Clone, Debug)]
pub struct Outcome {
    /// The row on the season schedule.
    pub row: usize,
    /// The event's index in `Events.txt`.
    pub event: usize,
    pub event_name: String,
    pub arena: String,
    pub game_type: &'static str,
    pub points_before: i32,
    pub points_after: i32,
    pub event_points: i32,
    pub participation: i32,
    pub bonus: i32,
    pub rank_before: i32,
    pub rank_after: i32,
    pub won: bool,
    pub rounds_won: u32,
    pub rounds_lost: u32,
    pub prize_model: Option<String>,
    pub prize_art: Option<String>,
    pub prize_description: Option<String>,
    /// The reloaded career's season points, read back from the saved tables.
    pub reloaded_points: i32,
    pub reloaded_rank: i32,
    pub reloaded_prizes: usize,
}

impl Outcome {
    pub fn summary(&self) -> String {
        let prize = match (&self.prize_model, &self.prize_description) {
            (Some(model), Some(description)) => {
                let art = self.prize_art.as_deref().unwrap_or("-");
                format!("{model} — {} (art {art})", plain_text(description))
            }
            _ => "none (the event was lost; the losing tier is shown on the award screen)"
                .to_string(),
        };
        format!(
            "event: schedule row {} -> Events.txt #{} {:?}\n\
             arena: {} ({})\n\
             points: {} -> {} (event {} = participation {} + bonus {})\n\
             rank: {} -> {} (career.rank())\n\
             result: won {}, rounds {}/{}\n\
             prize: {}\n\
             reloaded: {} points, rank {}, {} prize(s)",
            self.row,
            self.event,
            self.event_name,
            self.arena,
            self.game_type,
            self.points_before,
            self.points_after,
            self.event_points,
            self.participation,
            self.bonus,
            self.rank_before,
            self.rank_after,
            self.won,
            self.rounds_won,
            self.rounds_won + self.rounds_lost,
            prize,
            self.reloaded_points,
            self.reloaded_rank,
            self.reloaded_prizes
        )
    }
}

/// The current screen of the flow.
enum Screen {
    Hq(Box<ra2ui::hq::Hq>),
    Event {
        gib: Gib,
    },
    Match(Box<match_mode::Session>),
    Award {
        gib: Gib,
    },
    /// `teamhq/repairs.gib`: the between-rounds repair bench.
    Repairs(Box<Repairs>),
}

/// The HQ and the event flow as live state.
pub struct Session {
    pub game: PathBuf,
    pub career: Career,
    assets: UiAssets,
    screen: Screen,
    /// The event the flow is in, if any.
    current_event: Option<usize>,
    /// `--seconds`: the match length for the event. The event setup window's
    /// field was not recovered, so this is explicit.
    event_limit: f32,
    last_award: Option<MatchAward>,
    last_record: Option<EventRecord>,
    last_report: Option<MatchReport>,
    last_round: Option<RoundOutcome>,
    /// Evidence/test damage added to the player's report before the award.
    report_damage: f32,
    /// The repair bench was opened between rounds: `finished button` returns to
    /// the bracket, not the HQ.
    repairs_after_round: bool,
}

impl Session {
    /// Load the career and open the HQ on the robots tab.
    pub fn open(game: &Path) -> Result<Session, String> {
        let career = Career::new_with_policy(game, SEED, CareerPolicy::default())?;
        let assets = UiAssets::open(game).map_err(|e| e.to_string())?;
        let view = build_view(&career, game)?;
        let hq = ra2ui::hq::Hq::open(&game.join("UI"), view)?;
        Ok(Session {
            game: game.to_path_buf(),
            career,
            assets,
            screen: Screen::Hq(Box::new(hq)),
            current_event: None,
            event_limit: match_mode::DEFAULT_SECONDS,
            last_award: None,
            last_record: None,
            last_report: None,
            last_round: None,
            report_damage: 0.0,
            repairs_after_round: false,
        })
    }

    /// Add `damage` to the player's match report before the career award. This
    /// is an evidence/test knob (`--report-damage`), not a game rule: the
    /// shipped match in this environment reports zero damage.
    pub fn set_report_damage(&mut self, damage: f32) {
        if damage.is_finite() && damage >= 0.0 {
            self.report_damage = damage;
        }
    }

    /// The match length this session uses for an event.
    pub fn set_event_seconds(&mut self, seconds: f32) {
        if seconds.is_finite() && seconds > 0.0 {
            self.event_limit = seconds;
        }
    }

    /// The plain data the four HQ tabs draw, from the live career.
    pub fn view(&self) -> Result<ra2ui::hq::View, String> {
        build_view(&self.career, &self.game)
    }

    pub fn hq(&self) -> Option<&ra2ui::hq::Hq> {
        match &self.screen {
            Screen::Hq(hq) => Some(hq.as_ref()),
            _ => None,
        }
    }

    fn hq_mut(&mut self) -> Option<&mut ra2ui::hq::Hq> {
        match &mut self.screen {
            Screen::Hq(hq) => Some(hq.as_mut()),
            _ => None,
        }
    }

    /// Select an HQ tab by its button name.
    pub fn select_tab(&mut self, button: &str) -> Result<(), String> {
        let Some(hq) = self.hq_mut() else {
            return Err("the HQ is not on screen".to_string());
        };
        if !hq.select(button)? {
            return Err(format!("no HQ tab called {button:?}"));
        }
        Ok(())
    }

    /// The rows the shipped `repairs list` draws, from the career's pending
    /// component repairs and their clock. Each row is one component with its
    /// robot, class, current/total hitpoints and the `damage.tga` icon when the
    /// original's fractured flag is set. No cost is shown because the model has
    /// none.
    pub fn repair_rows(&self) -> Vec<RepairRow> {
        self.career
            .repairs
            .iter()
            .enumerate()
            .map(|(index, repair)| {
                let progress = self.career.repair_progress(index).unwrap_or(1.0);
                let remaining = (repair.seconds - repair.elapsed).max(0.0);
                RepairRow::component(
                    repair.robot.clone(),
                    repair.component.clone(),
                    repair.component_class.clone(),
                    repairs::format_time(remaining),
                    progress,
                    repair.hp,
                    repair.total_hp,
                    repair.fractured,
                )
            })
            .collect()
    }

    /// The clock the screen shows: the longest repair still running.
    fn repair_clock(&self) -> String {
        let remaining = self
            .career
            .repairs
            .iter()
            .map(|repair| (repair.seconds - repair.elapsed).max(0.0))
            .fold(0.0f32, f32::max);
        repairs::format_time(remaining)
    }

    /// Open `teamhq/repairs.gib` from the career's current pending list. A
    /// healthy roster opens the screen's own empty state (no rows, `0:00`).
    pub fn open_repairs(&mut self) -> Result<(), String> {
        let selected = self
            .repairs()
            .map(|screen| screen.selected())
            .unwrap_or_default();
        let rows = self.repair_rows();
        let clock = self.repair_clock();
        let mut screen = Repairs::open(&self.game.join("UI"), "Round 1 Repairs", rows, &clock)?;
        screen.set_selected(selected);
        self.screen = Screen::Repairs(Box::new(screen));
        Ok(())
    }

    pub fn repairs(&self) -> Option<&Repairs> {
        match &self.screen {
            Screen::Repairs(screen) => Some(screen.as_ref()),
            _ => None,
        }
    }

    /// Rebuild the repairs screen from the model, keeping the selection.
    pub fn refresh_repairs(&mut self) -> Result<(), String> {
        let Some(current) = self.repairs() else {
            return Err("the repairs screen is not on screen".to_string());
        };
        let selected = current.selected();
        let rows = self.repair_rows();
        let clock = self.repair_clock();
        let Some(screen) = self.repairs_mut() else {
            return Err("the repairs screen is not on screen".to_string());
        };
        screen.set_rows(rows);
        screen.set_selected(selected);
        screen.set_time(&clock);
        screen.refresh()
    }

    fn repairs_mut(&mut self) -> Option<&mut Repairs> {
        match &mut self.screen {
            Screen::Repairs(screen) => Some(screen.as_mut()),
            _ => None,
        }
    }

    /// The repair button's action: advance the selected repair's clock by one
    /// UI tick. The model completes the entry when the bar fills; completing
    /// saves the career, which is what makes the cleared damage persist.
    pub fn repair_tick(&mut self) -> Result<Option<String>, String> {
        if self.career.repairs.is_empty() {
            return Ok(None);
        }
        let index = self
            .repairs()
            .map(|screen| screen.selected())
            .unwrap_or_default()
            .min(self.career.repairs.len() - 1);
        let done = self.career.advance_repair(index, REPAIR_TICK_SECONDS)?;
        if done.is_some() {
            self.career.save(&self.game)?;
        }
        self.refresh_repairs()?;
        Ok(done.map(|repair| repair.component))
    }

    /// The point at an event row's centre on the events tab.
    pub fn event_point(&self, row: usize) -> Result<[f32; 2], String> {
        let Some(hq) = self.hq() else {
            return Err("the HQ is not on screen".to_string());
        };
        hq.event_point(row, SHELL_SIZE, Some(self.assets.faces()))
            .ok_or_else(|| format!("the event grid has no row {row}"))
    }

    /// Click in the shell's coordinates. The HQ's tab and event rows are the
    /// data's own hit tests; the event, match and award screens use the same
    /// `hit_control` a mouse does.
    pub fn click(&mut self, point: [f32; 2]) -> Result<bool, String> {
        match &mut self.screen {
            Screen::Hq(_) => {
                let clicked = {
                    let Session { assets, screen, .. } = self;
                    let Screen::Hq(hq) = screen else {
                        unreachable!()
                    };
                    hq.click(point, SHELL_SIZE, Some(assets.faces()))?
                };
                let Some(clicked) = clicked else {
                    return Ok(false);
                };
                match clicked {
                    ra2ui::hq::Click::Tab(_) => Ok(true),
                    ra2ui::hq::Click::Event(id) => {
                        self.enter_event(id)?;
                        Ok(true)
                    }
                }
            }
            Screen::Event { .. } => {
                let names = self.names_at(point);
                if names.iter().any(|n| n == "play button") {
                    self.start_match()?;
                    return Ok(true);
                }
                if names
                    .iter()
                    .any(|n| n == "forfeit button" || n == "later button")
                {
                    self.return_to_hq()?;
                    return Ok(true);
                }
                Ok(false)
            }
            Screen::Match(_) => {
                if self.match_state() == Some(MatchState::Results)
                    && self.match_results_button_at(point).is_some()
                {
                    self.finish_match()?;
                    return Ok(true);
                }
                Ok(false)
            }
            Screen::Award { .. } => {
                let names = self.names_at(point);
                if names.iter().any(|n| n == "continue button") {
                    // The between-rounds bench is the next screen when the
                    // match left something to repair; a healthy roster goes
                    // straight back to the HQ.
                    self.repairs_after_round = false;
                    if self.career.repairs.is_empty() {
                        self.return_to_hq()?;
                    } else {
                        self.open_repairs()?;
                    }
                    return Ok(true);
                }
                Ok(false)
            }
            Screen::Repairs(_) => {
                let clicked = {
                    let Session { assets, screen, .. } = self;
                    let Screen::Repairs(screen) = screen else {
                        unreachable!()
                    };
                    screen.click(point, SHELL_SIZE, Some(assets.faces()))
                };
                match clicked {
                    Some(RepairClick::Row(index)) => {
                        if let Some(screen) = self.repairs_mut() {
                            screen.set_selected(index);
                        }
                        Ok(true)
                    }
                    Some(RepairClick::Repair) => {
                        match self.repair_tick()? {
                            Some(component) => println!("  repair complete: {component}"),
                            None if self.career.repairs.is_empty() => {
                                println!("  repair list is empty")
                            }
                            None => {
                                let row = self
                                    .repairs()
                                    .and_then(|s| s.rows().first())
                                    .map(|r| r.time.clone())
                                    .unwrap_or_default();
                                println!("  repair clock {row}");
                            }
                        }
                        Ok(true)
                    }
                    Some(RepairClick::Finished) => {
                        if self.repairs_after_round {
                            self.repairs_after_round = false;
                            refresh_event_screen(self)?;
                        } else {
                            self.return_to_hq()?;
                        }
                        Ok(true)
                    }
                    None => Ok(false),
                }
            }
        }
    }

    /// Enter the event `Events.txt` id `id`, as the clicked cell names it.
    pub fn enter_event(&mut self, id: usize) -> Result<(), String> {
        self.career.enter_event(id)?;
        // The career has one `Robot In Event`; the first roster slot is the
        // entered robot. The model does not pick one.
        let robot = self
            .career
            .player()
            .robots
            .first()
            .copied()
            .ok_or_else(|| "the team has no robots".to_string())?;
        self.career.player_mut().event_robot = robot;
        let event = self.career.events[id].clone();
        let bracket = bracket_view(&self.career);
        let gib = event_screen(&self.game.join("UI"), &event, bracket.as_ref())?;
        self.current_event = Some(id);
        self.screen = Screen::Event { gib };
        let opponents = self
            .career
            .next_opponents()
            .iter()
            .map(|entrant| self.career.entrant_name(*entrant))
            .collect::<Vec<_>>();
        println!(
            "entered event #{} {:?} — {} ({}), robot slot {}; {} of {} rounds, next: {}",
            id,
            event.name,
            event.arena.0,
            event.arena.1,
            robot,
            self.career.bracket().map_or(0, |b| b.round + 1),
            self.career.bracket().map_or(0, |b| b.rounds),
            if opponents.is_empty() {
                "no opponent".to_string()
            } else {
                opponents.join(", ")
            }
        );
        Ok(())
    }

    /// Start the event's one match.
    pub fn start_match(&mut self) -> Result<(), String> {
        let Some(event_id) = self.current_event else {
            return Err("no event is in progress".to_string());
        };
        let event = self.career.events[event_id].clone();
        let config = match_config(&self.game, &event, &self.career, self.event_limit)?;
        let game_type = config.game_type;
        let arena = config.arena.clone();
        // The presentation's deterministic run turns the shipped hazards off;
        // the environment is read while the world is built.
        std::env::set_var("RA2_HAZARDS", "0");
        let matched = Match::new(config).map_err(|e| e.to_string())?;
        let session = match_mode::Session::with_match(&self.game, matched)?;
        println!(
            "event match: {:?} at {} — {} ({})",
            event.name,
            arena,
            game_type.as_str(),
            self.career
                .bracket()
                .map(|b| b.round_label())
                .unwrap_or_default()
        );
        self.screen = Screen::Match(Box::new(session));
        Ok(())
    }

    /// Award the round through the career bracket, save, and either show the
    /// next round (a win with rounds left) or the award/prize screen (a final
    /// win or a loss).
    pub fn finish_match(&mut self) -> Result<(), String> {
        let (mut report, event_id) = {
            let screen = match &self.screen {
                Screen::Match(session) => session,
                _ => return Err("no match is on screen".to_string()),
            };
            if screen.matched.state() != MatchState::Results {
                return Err("the match is not over".to_string());
            }
            (
                screen.report(),
                self.current_event
                    .ok_or_else(|| "no event is in progress".to_string())?,
            )
        };
        if self.report_damage > 0.0 {
            if let Some(player) = report.robots.first_mut() {
                let before = player.damage_taken;
                damage_report_components(player, self.report_damage);
                player.damage_taken += self.report_damage;
                println!(
                    "report damage: +{:.0} on {:?} (evidence knob; the match dealt {:.0})",
                    self.report_damage, player.bot_name, before
                );
                for component in player.components.iter().filter(|c| c.damaged()) {
                    println!(
                        "  component {} {:?} [{}]: {:.0}/{:.0}{}{}",
                        component.index,
                        component.name,
                        component.class,
                        component.hp,
                        component.total_hp,
                        if component.detached { " detached" } else { "" },
                        if component.fractured {
                            " fractured"
                        } else {
                            ""
                        }
                    );
                }
            }
        }
        let outcome = self.career.play_round(&report, 0)?;
        self.career.save(&self.game)?;
        let event = self.career.events[event_id].clone();
        println!(
            "round {}: vs {} — {}; +{} event points ({} + {}); saved {}",
            outcome.round + 1,
            outcome
                .opponents
                .iter()
                .map(|entrant| self.career.entrant_name(*entrant))
                .collect::<Vec<_>>()
                .join(", "),
            if outcome.won { "won" } else { "lost" },
            outcome.points,
            outcome.participation,
            outcome.bonus,
            self.game.join("Teams").join("Teams.txt").display()
        );
        self.last_report = Some(report);
        self.last_round = Some(outcome.clone());
        if let Some(record) = outcome.record {
            let bot_name = self
                .last_report
                .as_ref()
                .and_then(|report| report.robots.first())
                .map(|r| r.bot_name.clone())
                .unwrap_or_default();
            let description = if record.won {
                event.trophy.1.clone()
            } else {
                self.career
                    .prize_for_rounds(event_id, record.rounds_won as usize)
                    .map(|(_, message)| message.clone())
                    .unwrap_or_default()
            };
            let gib = award_screen(
                &self.game.join("UI"),
                &event,
                self.career.team_name(),
                &bot_name,
                &record,
                &description,
            )?;
            println!(
                "award: {} event points, won {}, rounds {}/{}",
                record.points, record.won, record.rounds_won, record.rounds_lost
            );
            self.last_record = Some(record);
            self.screen = Screen::Award { gib };
        } else {
            self.last_record = None;
            if self.career.repairs.is_empty() {
                refresh_event_screen(self)?;
            } else {
                self.repairs_after_round = true;
                self.open_repairs()?;
            }
            println!(
                "  next: {} — {}",
                self.career
                    .bracket()
                    .map(|b| b.round_label())
                    .unwrap_or_default(),
                self.career
                    .next_opponents()
                    .iter()
                    .map(|entrant| self.career.entrant_name(*entrant))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        self.last_award = None;
        Ok(())
    }

    /// Reload the career from the game's own files and open the HQ again. This
    /// is what proves the save: the next frames are drawn from what was read
    /// back, not from the in-memory career.
    pub fn return_to_hq(&mut self) -> Result<(), String> {
        self.career = Career::load(&self.game)?;
        let view = build_view(&self.career, &self.game)?;
        let hq = ra2ui::hq::Hq::open(&self.game.join("UI"), view)?;
        self.current_event = None;
        self.screen = Screen::Hq(Box::new(hq));
        Ok(())
    }

    /// Draw the match's 3D pass into a caller's target, when a match is on
    /// screen. UI-only screens are a no-op here.
    pub fn render_3d(
        &self,
        renderer: &Renderer,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        match &self.screen {
            Screen::Match(session) => {
                session.render_3d(renderer, color, depth, format, width, height)
            }
            _ => Ok(()),
        }
    }

    /// Advance a live match by wall-clock seconds (the window's fixed tick).
    pub fn advance(&mut self, seconds: f32) {
        if let Screen::Match(session) = &mut self.screen {
            session.advance(seconds);
        }
    }

    fn match_state(&self) -> Option<MatchState> {
        match &self.screen {
            Screen::Match(session) => Some(session.matched.state()),
            _ => None,
        }
    }

    /// The `teamhq/` group's track unless a match is on screen; the match's
    /// sounds are the arena's own script requests.
    pub fn audio_screen(&self) -> Option<&'static str> {
        match &self.screen {
            Screen::Match(_) => None,
            _ => Some("teamhq/teamhq.gib"),
        }
    }

    /// The live match's listener, when a match is on screen.
    pub fn audio_listener(&self) -> Option<ra2_audio::Listener> {
        match &self.screen {
            Screen::Match(session) => Some(session.audio_listener()),
            _ => None,
        }
    }

    /// The sounds the live match's steps made since the last call.
    pub fn take_cues(&mut self) -> Vec<crate::match_sounds::SoundCue> {
        match &mut self.screen {
            Screen::Match(session) => session.take_cues(),
            _ => Vec::new(),
        }
    }

    fn match_clock(&self) -> Option<f32> {
        match &self.screen {
            Screen::Match(session) => Some(session.matched.clock()),
            _ => None,
        }
    }

    fn match_results_button_at(&mut self, point: [f32; 2]) -> Option<String> {
        match &mut self.screen {
            Screen::Match(session) => session.results_button_at(point),
            _ => None,
        }
    }

    /// The scene for the current screen. `three_d` says whether the match's 3D
    /// pass must be drawn under it.
    pub fn scene(&mut self) -> Result<(UiScene, bool), String> {
        let size = [SHELL_SIZE[0] as u32, SHELL_SIZE[1] as u32];
        let Session { assets, screen, .. } = self;
        match screen {
            Screen::Hq(hq) => {
                // The inventory's cells carry the robots' own snapshots under
                // content-addressed keys; register them before the draw list is
                // built. Nothing is written to disk.
                for row in &hq.view().robots {
                    if let Some(preview) = &row.preview_image {
                        assets.add_preview(&row.preview, preview);
                    }
                }
                let placed = hq.placements(SHELL_SIZE, Some(assets.faces()));
                Ok((assets.scene(&placed, size), false))
            }
            Screen::Event { gib, .. } | Screen::Award { gib, .. } => {
                let placed = ra2ui::resolve_with(gib, SHELL_SIZE, Some(assets.faces()));
                Ok((assets.scene(&placed, size), false))
            }
            Screen::Repairs(screen) => {
                let placed = screen.placements(SHELL_SIZE, Some(assets.faces()));
                Ok((assets.scene(&placed, size), false))
            }
            Screen::Match(session) => Ok((session.scene()?, true)),
        }
    }

    /// The names of the widgets a control click passes through on the current
    /// event/award screen.
    fn names_at(&self, point: [f32; 2]) -> Vec<String> {
        let gib = match &self.screen {
            Screen::Event { gib, .. } | Screen::Award { gib, .. } => gib,
            _ => return Vec::new(),
        };
        let placed = ra2ui::resolve_with(gib, SHELL_SIZE, Some(self.assets.faces()));
        let Some(path) = ra2ui::input::hit_control(&placed, point[0], point[1]) else {
            return Vec::new();
        };
        names_on_path(&placed, &path)
    }

    /// Draw the current screen into a caller's target. UI-only screens are a
    /// direct UI pass; a match draws its 3D scene first, exactly as `--match`.
    pub fn write_frame(
        &mut self,
        renderer: &Renderer,
        ui: &UiRenderer,
        target: &Target,
        size: [u32; 2],
        out: &Path,
        name: &str,
    ) -> Result<(), String> {
        let (scene, three_d) = self.scene()?;
        if three_d {
            let Screen::Match(session) = &self.screen else {
                return Err("a 3D frame without a match".to_string());
            };
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
            write_png(&out.join(name), size, pixels)
        } else {
            let pixels = ui
                .render_at(renderer.device(), renderer.queue(), &scene, size)
                .map_err(|e| e.to_string())?;
            write_png(&out.join(name), size, pixels)
        }
    }

    /// Play schedule row `row` through the whole event (every bracket round),
    /// then return to the reloaded HQ. `capture` is called with each named
    /// moment so the headless run writes the same frames the window would show.
    pub fn simulate(
        &mut self,
        row: usize,
        seconds: f32,
        capture: &mut dyn FnMut(&mut Session, &str) -> Result<(), String>,
    ) -> Result<Outcome, String> {
        self.play_event(row, seconds, capture, FramePlan::single())
            .map(|(outcome, _)| outcome)
    }

    /// Play every event on the schedule, in order, with deterministic
    /// opponents, and return the per-event outcomes. The season's champion
    /// screen (`golden_sawblade.gib` when rank 1, otherwise
    /// `season_results.gib`) is written after the last event.
    pub fn simulate_season(
        &mut self,
        seconds: f32,
        capture: &mut dyn FnMut(&mut Session, &str) -> Result<(), String>,
    ) -> Result<Vec<Outcome>, String> {
        self.set_event_seconds(seconds);
        let mut outcomes = Vec::new();
        let mut first_bracket = true;
        let mut round_captured = false;
        for row in 0..self.career.schedule.len() {
            if matches!(self.career.phase, ra2sim::career::SeasonPhase::Complete) {
                break;
            }
            let plan = FramePlan {
                bracket: first_bracket.then_some("bracket.png"),
                round: (!round_captured).then_some("round.png"),
                award: Some("champion.png"),
                season_hq: first_bracket,
                ..FramePlan::season()
            };
            let (outcome, captured_round) = self.play_event(row, seconds, capture, plan)?;
            round_captured |= captured_round;
            println!("{}", outcome.summary());
            first_bracket = false;
            outcomes.push(outcome);
        }
        if matches!(self.career.phase, ra2sim::career::SeasonPhase::Complete) {
            self.select_tab("events button")?;
            capture(self, "season-final.png")?;
            let view = self.view()?;
            let gib = season_end_screen(&self.game.join("UI"), &view)?;
            self.screen = Screen::Award { gib };
            capture(self, "champion.png")?;
        }
        println!(
            "season {} complete: {} points, rank {}, {} prize(s), {} event(s) played",
            self.career.season(),
            self.career.season_points(),
            self.career.rank(),
            self.career.prizes.len(),
            outcomes.len()
        );
        Ok(outcomes)
    }

    fn play_event(
        &mut self,
        row: usize,
        seconds: f32,
        capture: &mut dyn FnMut(&mut Session, &str) -> Result<(), String>,
        plan: FramePlan<'_>,
    ) -> Result<(Outcome, bool), String> {
        let event = *self
            .career
            .schedule
            .get(row)
            .ok_or_else(|| format!("the season has no schedule row {row}"))?;
        let points_before = self.career.season_points();
        let rank_before = self.career.rank();
        self.set_event_seconds(seconds);

        self.select_tab("events button")?;
        if let Some(name) = plan.hq_events {
            capture(self, name)?;
        }
        let point = self.event_point(row)?;
        let index = {
            let Session { assets, screen, .. } = self;
            let Screen::Hq(hq) = screen else {
                return Err("the HQ is not on screen".to_string());
            };
            hq.event_at(point, SHELL_SIZE, Some(assets.faces()))
        };
        let named = self
            .view()?
            .event_id(index.unwrap_or(usize::MAX))
            .ok_or_else(|| format!("the event cell at {point:?} does not name an event"))?;
        if named != event {
            return Err(format!(
                "the cell at {point:?} is event {named}, the schedule row {row} is {event}"
            ));
        }
        if !self.click(point)? {
            return Err("the event cell click was not handled".to_string());
        }
        if self.career.entered_event() != Some(event) {
            return Err("the click did not enter the event".to_string());
        }
        if plan.season_hq {
            let view = self.view()?;
            let hq = ra2ui::hq::Hq::open_on(&self.game.join("UI"), view, "events button")?;
            let event_screen = std::mem::replace(&mut self.screen, Screen::Hq(Box::new(hq)));
            capture(self, "season.png")?;
            self.screen = event_screen;
        }
        if let Some(name) = plan.bracket {
            capture(self, name)?;
        }

        let mut rounds: Vec<RoundOutcome> = Vec::new();
        let mut match_shot = false;
        let mut results_shot = false;
        let mut round_shot = false;
        let mut repairs_list = false;
        let mut repairs_done = false;
        loop {
            let play = self.button_point("play button")?;
            self.click(play)?;
            if !matches!(self.screen, Screen::Match(_)) {
                return Err("the play button did not start the match".to_string());
            }
            let mut shot = false;
            loop {
                let state = match self.match_state() {
                    Some(state) => state,
                    None => return Err("the match left the screen".to_string()),
                };
                if state == MatchState::Results {
                    break;
                }
                if state == MatchState::Fight
                    && !shot
                    && self.match_clock().unwrap_or_default() >= seconds / 2.0
                {
                    if let (Some(name), false) = (plan.match_frame, match_shot) {
                        capture(self, name)?;
                        match_shot = true;
                    }
                    shot = true;
                }
                let before = state;
                if let Screen::Match(session) = &mut self.screen {
                    session.step();
                }
                if before == MatchState::Fight
                    && self.match_state() != Some(MatchState::Fight)
                    && !shot
                {
                    if let (Some(name), false) = (plan.match_frame, match_shot) {
                        println!(
                            "  the match ended before its midpoint; {name} is the last fight frame"
                        );
                        capture(self, name)?;
                        match_shot = true;
                    }
                    shot = true;
                }
            }
            if !shot {
                if let (Some(name), false) = (plan.match_frame, match_shot) {
                    capture(self, name)?;
                    match_shot = true;
                }
            }
            println!(
                "  fight ended at {:.2}s, results screen is next",
                self.match_clock().unwrap_or_default()
            );
            if let (Some(name), false) = (plan.results, results_shot) {
                capture(self, name)?;
                results_shot = true;
            }
            self.finish_match()?;
            if let Some(outcome) = self.last_round.clone() {
                rounds.push(outcome);
            }
            if let Some(name) = plan.round {
                if !round_shot {
                    if let Some(outcome) = self.last_round.as_ref() {
                        if !outcome.event_finished {
                            capture(self, name)?;
                            round_shot = true;
                        }
                    }
                }
            }
            if matches!(self.screen, Screen::Award { .. }) {
                if let Some(name) = plan.award {
                    capture(self, name)?;
                }
                break;
            }
            // The binary's between-rounds bench (`FUN_0046ce50` opens
            // `repairs.gib` after a winning round).
            if matches!(self.screen, Screen::Repairs(_)) {
                if let Some(name) = plan.repairs_list {
                    if !repairs_list {
                        capture(self, name)?;
                        repairs_list = true;
                    }
                }
                self.finish_repairs()?;
                if let Some(name) = plan.repairs_done {
                    if !repairs_done {
                        capture(self, name)?;
                        repairs_done = true;
                    }
                }
                refresh_event_screen(self)?;
            }
        }

        // The award's continue button opens the bench when the match left
        // damage; the headless run writes both states even when it was empty.
        self.open_repairs()?;
        println!(
            "  repairs: {} component row(s), clock {}",
            self.career.repairs.len(),
            self.repair_clock()
        );
        for row in self.repair_rows() {
            println!(
                "    {} / {} [{}] hp {:.0}/{:.0} ({}%) {}, {}{}",
                row.robot,
                row.component,
                row.class,
                row.hp,
                row.total_hp,
                row.hp_percent(),
                row.time,
                if row.fractured { "fractured" } else { "intact" },
                if row.fractured { " (damage.tga)" } else { "" }
            );
        }
        if let Some(name) = plan.repairs_list {
            capture(self, name)?;
        }
        let mut clicks = 0usize;
        while !self.career.repairs.is_empty() {
            let done = self.repair_tick()?;
            if let Some(component) = done {
                println!("  repair complete: {component}");
            }
            clicks += 1;
            if clicks > 10_000 {
                return Err("the repair clock did not fill".into());
            }
        }
        println!("  repairs done after {clicks} clock tick(s)");
        if let Some(name) = plan.repairs_done {
            capture(self, name)?;
        }

        let record = self
            .last_record
            .clone()
            .ok_or_else(|| "the event did not produce a record".to_string())?;
        let config = match_config(
            &self.game,
            &self.career.events[event],
            &self.career,
            seconds,
        )?;
        let prize = if record.won {
            self.career
                .prizes
                .iter()
                .rev()
                .find(|p| p.event == event)
                .cloned()
        } else {
            None
        };
        let participation = rounds.iter().map(|round| round.participation).sum();
        let bonus = rounds.iter().map(|round| round.bonus).sum();

        self.return_to_hq()?;
        println!(
            "  repair bench after reload: {} pending repair(s)",
            self.career.repairs.len()
        );
        if let Some(name) = plan.hq_after {
            self.select_tab("league button")?;
            capture(self, name)?;
        }
        if let Some(name) = plan.hq_history {
            self.select_tab("history button")?;
            capture(self, name)?;
        }

        let reloaded_points = self.career.season_points();
        let reloaded_rank = self.career.rank();
        let expected = points_before + record.points;
        if reloaded_points != expected {
            return Err(format!(
                "the reloaded points {reloaded_points} are not {expected}"
            ));
        }
        Ok((
            Outcome {
                row,
                event,
                event_name: self.career.events[event].name.clone(),
                arena: config.arena.clone(),
                game_type: config.game_type.as_str(),
                points_before,
                points_after: reloaded_points,
                event_points: record.points,
                participation,
                bonus,
                rank_before,
                rank_after: reloaded_rank,
                won: record.won,
                rounds_won: record.rounds_won,
                rounds_lost: record.rounds_lost,
                prize_model: prize.as_ref().map(|p| p.model.clone()),
                prize_art: prize.as_ref().map(|p| p.art.clone()),
                prize_description: prize.as_ref().map(|p| p.description.clone()),
                reloaded_points,
                reloaded_rank,
                reloaded_prizes: self.career.prizes.len(),
            },
            round_shot,
        ))
    }

    /// Tick the repair bench to empty and leave it. The bench is the same
    /// `teamhq/repairs.gib` screen as the award's bench.
    fn finish_repairs(&mut self) -> Result<(), String> {
        while !self.career.repairs.is_empty() {
            if let Some(component) = self.repair_tick()? {
                println!("  repair complete: {component}");
            }
        }
        Ok(())
    }

    /// A point inside a named control on the event or award screen.
    fn button_point(&self, name: &str) -> Result<[f32; 2], String> {
        self.button_point_named(&[name])
            .ok_or_else(|| format!("no clickable {name:?} on the current screen"))
    }

    fn button_point_named(&self, names: &[&str]) -> Option<[f32; 2]> {
        let gib = match &self.screen {
            Screen::Event { gib, .. } | Screen::Award { gib, .. } => gib,
            _ => return None,
        };
        let placed = ra2ui::resolve_with(gib, SHELL_SIZE, Some(self.assets.faces()));
        for name in names {
            let found = placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == *name)?;
            let frame = found.frame;
            for iy in 1..8 {
                for ix in 1..10 {
                    let point = [
                        frame.x + frame.w * ix as f32 / 10.0,
                        frame.y + frame.h * iy as f32 / 8.0,
                    ];
                    if let Some(path) = ra2ui::input::hit_control(&placed, point[0], point[1]) {
                        let path_names = names_on_path(&placed, &path);
                        if path_names.iter().any(|n| n == name) {
                            return Some(point);
                        }
                    }
                }
            }
        }
        None
    }
}

/// The Team HQ's data for the shell: the team record in the game's own
/// `Teams/` (read-only; nothing is written).
pub fn shell_view(game: &Path) -> Option<ra2ui::hq::View> {
    let career = Career::new_with_policy(game, SEED, CareerPolicy::default()).ok()?;
    build_view(&career, game).ok()
}

/// The `ra2ui::hq::View` for a career: the roster, standings, schedule, prizes
/// and the team record's counters.
pub fn build_view(career: &Career, game: &Path) -> Result<ra2ui::hq::View, String> {
    let teams = ra2ui::standings::roster(game)?;
    let mut points = vec![0; teams.len()];
    if !points.is_empty() {
        points[0] = career.season_points();
    }
    for (index, value) in career.player().opponent_points().iter().enumerate() {
        if index + 1 < points.len() {
            points[index + 1] = *value;
        }
    }
    let standings = ra2ui::standings::table(&teams, &points);

    let robots = career
        .player()
        .robots
        .iter()
        .map(|slot| {
            let path = player_robot_path(game, career.player_slot, *slot);
            match read_bot(&path) {
                Some(bot) => {
                    let preview_image = bot
                        .snapshot
                        .as_deref()
                        .and_then(ra2ui::preview::Preview::decode_tga);
                    let preview = preview_image
                        .as_ref()
                        .map(|picture| picture.key(&format!("hq-{slot}")))
                        .unwrap_or_else(|| "teamhq\\nobot.bmp".to_string());
                    ra2ui::hq::RobotRow {
                        name: bot.name.clone(),
                        class: ra2ui::workshop::stats::WeightClass::from_file(bot.class)
                            .full_label()
                            .to_string(),
                        // The `.bot`'s own snapshot is the picture the original
                        // draws; `nobot.bmp` is only the empty-slot silhouette.
                        preview,
                        preview_image,
                    }
                }
                None => ra2ui::hq::RobotRow {
                    name: format!("Bot {slot}"),
                    class: String::new(),
                    preview: "teamhq\\nobot.bmp".to_string(),
                    preview_image: None,
                },
            }
        })
        .collect();

    let entered = career.entered_event();
    let events = career
        .schedule
        .iter()
        .filter_map(|id| {
            career.events.get(*id).map(|event| {
                let state = if career.history.iter().any(|record| record.event == *id) {
                    ra2ui::hq::EventState::Completed
                } else if entered == Some(*id) {
                    ra2ui::hq::EventState::InProgress
                } else if entered.is_some() {
                    ra2ui::hq::EventState::Disabled
                } else {
                    ra2ui::hq::EventState::Available
                };
                ra2ui::hq::EventRow {
                    id: *id,
                    event: event.clone(),
                    state,
                }
            })
        })
        .collect();

    let prizes = career
        .prizes
        .iter()
        .rev()
        .map(|prize| ra2ui::hq::PrizeRow {
            art: prize.art.clone(),
            description: prize.description.clone(),
        })
        .collect();
    let completed = career
        .history
        .iter()
        .map(|record| (record.event, record.won))
        .collect();

    let player = career.player();
    let stats = ra2ui::hq::Stats {
        total_seasons: player.season_index(),
        current_rank: career.rank(),
        best_rank: player.best_rank(),
        total_events: player.total_events(),
        wins: player.wins(),
        losses: player.losses(),
        events_won: player.events_won(),
        highest_score: player.highest_score(),
        fastest_win: player.fastest_win(),
    };

    Ok(ra2ui::hq::View {
        team: player.clone(),
        robots,
        standings,
        events,
        season: career.season(),
        events_remaining: career
            .policy
            .events_per_season
            .saturating_sub(career.history.len()),
        prizes,
        stats,
        bracket: bracket_view(career),
        completed,
    })
}

/// The bracket screen's plain data, from the career's bracket and the names in
/// the AI table. Future rounds are placeholders (`?`), as the binary draws them
/// with `UI\teamhq\questionmark.bmp`.
pub fn bracket_view(career: &Career) -> Option<ra2ui::hq::BracketView> {
    let bracket = career.bracket()?;
    let mut rounds = Vec::with_capacity(bracket.rounds);
    for index in 0..bracket.rounds {
        let label = if index + 1 == bracket.rounds {
            "Final Match".to_string()
        } else {
            format!("Round {}", index + 1)
        };
        let played = bracket.results.iter().find(|r| r.round == index);
        let current = index == bracket.round && bracket.status == BracketStatus::Playing;
        let matches = if current {
            bracket
                .current
                .matches
                .iter()
                .enumerate()
                .map(|(m, matched)| ra2ui::hq::BracketMatchView {
                    slots: matched
                        .entrants
                        .iter()
                        .map(|entrant| ra2ui::hq::BracketSlot {
                            name: career.entrant_name(*entrant),
                            player: *entrant == Entrant::Player,
                        })
                        .collect(),
                    player_match: m == 0,
                    won: None,
                })
                .collect()
        } else if let Some(result) = played {
            let mut slots = vec![ra2ui::hq::BracketSlot {
                name: career.team_name().to_string(),
                player: true,
            }];
            slots.extend(
                result
                    .opponents
                    .iter()
                    .map(|entrant| ra2ui::hq::BracketSlot {
                        name: career.entrant_name(*entrant),
                        player: false,
                    }),
            );
            vec![ra2ui::hq::BracketMatchView {
                slots,
                player_match: true,
                won: Some(result.won),
            }]
        } else {
            // Not reached: the binary puts a question mark in every cell.
            let count = bracket.matches_in_round(index);
            (0..count)
                .map(|m| ra2ui::hq::BracketMatchView {
                    slots: (0..bracket.bots_per_match)
                        .map(|slot| ra2ui::hq::BracketSlot {
                            name: if m == 0 && slot == 0 {
                                career.team_name().to_string()
                            } else {
                                String::new()
                            },
                            player: m == 0 && slot == 0,
                        })
                        .collect(),
                    player_match: m == 0,
                    won: None,
                })
                .collect()
        };
        rounds.push(ra2ui::hq::BracketRoundView {
            label,
            matches,
            current,
            played: played.is_some(),
        });
    }
    let opponents: Vec<String> = bracket
        .opponents()
        .iter()
        .map(|entrant| career.entrant_name(*entrant))
        .collect();
    let status = match &bracket.status {
        BracketStatus::Playing => {
            if opponents.is_empty() {
                format!(
                    "{} of {} — no opponent drawn",
                    bracket.round + 1,
                    bracket.rounds
                )
            } else {
                format!(
                    "{} of {} — next: {}",
                    bracket.round + 1,
                    bracket.rounds,
                    opponents.join(", ")
                )
            }
        }
        BracketStatus::Eliminated { round } => format!(
            "Eliminated in {} — {} round(s) won",
            if round + 1 == bracket.rounds {
                "the final".to_string()
            } else {
                format!("round {}", round + 1)
            },
            bracket.results.iter().filter(|result| result.won).count()
        ),
        BracketStatus::Champion => "Event champion — the trophy is yours".to_string(),
    };
    Some(ra2ui::hq::BracketView {
        event: bracket.event,
        round: bracket.round,
        round_label: bracket.round_label(),
        status,
        next_opponent: opponents.first().cloned(),
        can_play: bracket.status == BracketStatus::Playing,
        rounds,
    })
}

/// The event's match: the arena script and game type from the data, the player's
/// first robot against the first AI team's first robot. One match.
pub fn match_config(
    game: &Path,
    event: &Event,
    career: &Career,
    seconds: f32,
) -> Result<MatchConfig, String> {
    let arenas = ra2ui::arenas::all(game)?;
    let arena = arenas
        .iter()
        .find(|arena| arena.name.eq_ignore_ascii_case(&event.arena.0))
        .ok_or_else(|| {
            format!(
                "event {:?} names arena {:?}, which is not one of the shipped arenas",
                event.name, event.arena.0
            )
        })?;
    let script = game.join("Arenas").join(&arena.script);
    let game_type = arena_game_type(&script)?;
    let slot = career
        .player()
        .robots
        .first()
        .copied()
        .ok_or_else(|| "the team has no robots".to_string())?;
    let player = player_robot_path(game, career.player_slot, slot);
    if !player.is_file() {
        return Err(format!("{} is not a .bot file", player.display()));
    }
    let ai_slot = match career.next_opponents().first() {
        Some(Entrant::Ai(slot)) => *slot,
        _ => 0,
    };
    let opponent = ai_robot_path(game, ai_slot, 0);
    if !opponent.is_file() {
        return Err(format!("{} is not a .bot file", opponent.display()));
    }
    Ok(MatchConfig::new(
        script.to_string_lossy().to_string(),
        game_type,
        vec![
            RobotEntry::ai(0, player.to_string_lossy().to_string()),
            RobotEntry::ai(1, opponent.to_string_lossy().to_string()),
        ],
    )
    .with_duration(Some(seconds)))
}

/// The game type an arena declares in its own `game_types` list. The list is the
/// data's; the choice among several is the order below (Deathmatch first).
pub fn arena_game_type(script: &Path) -> Result<GameType, String> {
    let text = std::fs::read_to_string(script).map_err(|e| format!("{}: {e}", script.display()))?;
    let line = text
        .lines()
        .find(|line| line.contains("game_types"))
        .unwrap_or_default()
        .to_ascii_uppercase();
    for game_type in [
        GameType::Deathmatch,
        GameType::KingOfTheHill,
        GameType::Tabletop,
        GameType::BattleRoyal,
        GameType::TeamMatch,
    ] {
        if line.contains(game_type.as_str()) {
            return Ok(game_type);
        }
    }
    Err(format!(
        "{} names no game type this build knows",
        script.display()
    ))
}

/// `<game>/Teams/Team<slot>/Bot<robot>.bot` — the player's own roster.
pub fn player_robot_path(game: &Path, team: usize, robot: i32) -> PathBuf {
    game.join("Teams")
        .join(format!("Team{team}"))
        .join(format!("Bot{robot}.bot"))
}

/// `<game>/AI/Team<team>/Bot<robot>.bot` — an AI team's roster.
pub fn ai_robot_path(game: &Path, team: usize, robot: i32) -> PathBuf {
    game.join("AI")
        .join(format!("Team{team}"))
        .join(format!("Bot{robot}.bot"))
}

fn read_bot(path: &Path) -> Option<ra2fmt::bot::Bot> {
    let bytes = std::fs::read(path).ok()?;
    ra2fmt::bot::parse(&bytes).ok()
}

/// `teamhq/event_bracket.gib` with the event's own name, description, art and
/// the career's bracket: round heading, the field and the next opponent.
pub fn event_screen(
    ui_root: &Path,
    event: &Event,
    bracket: Option<&ra2ui::hq::BracketView>,
) -> Result<Gib, String> {
    let mut gib = ra2ui::hq::load(ui_root, "teamhq/event_bracket.gib")?;
    ra2ui::hq::fit_splits_in(&mut gib);
    if let Some(image) = find_mut_in(&mut gib, "event graphic") {
        ra2ui::input::set_field(image, "_Filename", &event.art);
    }
    // The preview the bracket's own note names is the arena's, not the shipped
    // `sample_arena.bmp` placeholder. The image's layout `Size` is the full
    // screen, which is what the renderer draws it at.
    if let Some(image) = find_mut_in(&mut gib, "background img") {
        ra2ui::input::set_field(image, "_Filename", &event.arena.1);
    }
    set_value_in(&mut gib, "event title", &event.name);
    set_first_display(&mut gib, &plain_text(&event.description));
    if let Some(bracket) = bracket {
        set_value_in(&mut gib, "round heading textbox", &bracket.round_label);
        ra2ui::hq::fill_bracket(&mut gib, bracket);
        if let Some(button) = find_mut_in(&mut gib, "play button") {
            ra2ui::input::set_field(
                button,
                "Visible",
                if bracket.can_play { "true" } else { "false" },
            );
        }
        set_value_in(&mut gib, "bracket status", &bracket.status);
    } else {
        set_value_in(&mut gib, "round heading textbox", "ROUND 1");
    }
    Ok(gib)
}

/// Rebuild the bracket screen from the live career. A round win returns here
/// with the next opponent drawn.
pub fn refresh_event_screen(session: &mut Session) -> Result<(), String> {
    let Some(event) = session.current_event else {
        return Err("no event is in progress".to_string());
    };
    let data = session
        .career
        .events
        .get(event)
        .ok_or_else(|| format!("event {event} is not in Events.txt"))?
        .clone();
    let bracket = bracket_view(&session.career);
    let gib = event_screen(&session.game.join("UI"), &data, bracket.as_ref())?;
    session.screen = Screen::Event { gib };
    Ok(())
}

/// `teamhq/event_winner.gib` with the event result the career recorded.
pub fn award_screen(
    ui_root: &Path,
    event: &Event,
    team: &str,
    bot: &str,
    record: &EventRecord,
    description: &str,
) -> Result<Gib, String> {
    let mut gib = ra2ui::hq::load(ui_root, "teamhq/event_winner.gib")?;
    ra2ui::hq::fit_splits_in(&mut gib);
    // The award is a modal: the shipped window has no full-screen background,
    // because the original draws it over the event-results 3D scene. This port
    // composes the event's own arena preview behind it instead of leaving the
    // frame black.
    gib.blocks.insert(
        0,
        Block::Widget(full_screen_image("arena preview", &event.arena.1)),
    );
    if let Some(image) = find_mut_in(&mut gib, "event img") {
        ra2ui::input::set_field(image, "_Filename", &event.art);
    }
    set_value_in(&mut gib, "team name", team);
    set_value_in(&mut gib, "bot name", bot);
    set_value_in(
        &mut gib,
        "rounds won",
        &format!(
            "{}/{}",
            record.rounds_won,
            record.rounds_won + record.rounds_lost
        ),
    );
    set_value_in(&mut gib, "team points", &record.points.to_string());
    set_first_display(&mut gib, &plain_text(description));
    Ok(gib)
}

fn find_mut_in<'a>(document: &'a mut Gib, name: &str) -> Option<&'a mut Widget> {
    document.blocks.iter_mut().find_map(|block| match block {
        Block::Widget(widget) => find_mut(widget, name),
        Block::Attachment(_) => None,
    })
}

fn find_mut<'a>(widget: &'a mut Widget, name: &str) -> Option<&'a mut Widget> {
    if widget.name() == name {
        return Some(widget);
    }
    for child in widget.children.iter_mut() {
        if let Some(found) = find_mut(child, name) {
            return Some(found);
        }
    }
    for attachment in widget.attachments.iter_mut() {
        if let Some(body) = &mut attachment.body {
            if let Some(found) = find_mut(body, name) {
                return Some(found);
            }
        }
    }
    None
}

fn set_value_in(document: &mut Gib, name: &str, value: &str) {
    if let Some(widget) = find_mut_in(document, name) {
        set_value(widget, value);
    }
}

/// Set the first `Glyph`'s text in a subtree (a `DisplayBox` body is a
/// headerless `Widget` holding the `Glyph`).
fn set_text_deep(widget: &mut Widget, text: &str) -> bool {
    if widget.kind == WidgetKind::Glyph {
        ra2ui::input::set_field(widget, "_Text", text);
        return true;
    }
    for child in widget.children.iter_mut() {
        if set_text_deep(child, text) {
            return true;
        }
    }
    false
}

fn set_value(widget: &mut Widget, value: &str) {
    if let Some(text) = widget
        .children
        .iter_mut()
        .find(|c| c.kind == WidgetKind::Text)
    {
        ra2ui::input::set_field(text, "_Text", value);
    } else {
        ra2ui::input::set_field(widget, "_Text", value);
    }
}

/// Write the text of a `DisplayBox` group, keeping the markup's own font and
/// colour, and the body `Glyph` that draws it.
/// Update the document's first `DisplayBox`. That is what
/// `event_bracket.gib`'s description and `event_winner.gib`'s prize text are;
/// the block attaches to the glyph that draws it, not to the named group beside
/// it, so the search is document-wide rather than by widget name.
fn set_first_display(gib: &mut Gib, text: &str) -> bool {
    for block in gib.blocks.iter_mut() {
        if let Block::Widget(widget) = block {
            if set_display(widget, text) {
                return true;
            }
        }
    }
    false
}

fn set_display(widget: &mut Widget, text: &str) -> bool {
    // A `DisplayBox` is attached to the record that draws its body. On
    // `event_winner.gib` that is the unnamed child of `prize description`, not
    // the named group, so the search goes through the subtree.
    if let Some(attachment) = widget
        .attachments
        .iter_mut()
        .find(|a| a.kind == AttachmentKind::DisplayBox)
    {
        if let Some(field) = attachment.field_mut("Display") {
            field.raw = ra2ui::cells::markup_with_text(&field.raw, text);
        }
        if let Some(body) = &mut attachment.body {
            set_text_deep(body, text);
        }
        // The shipped DisplayBox usually attaches to the Glyph that draws it.
        set_text_deep(widget, text);
        return true;
    }
    if set_text_deep(widget, text) {
        return true;
    }
    for child in widget.children.iter_mut() {
        if set_display(child, text) {
            return true;
        }
    }
    for attachment in widget.attachments.iter_mut() {
        if let Some(body) = &mut attachment.body {
            if set_display(body, text) {
                return true;
            }
        }
    }
    false
}

/// The prose inside an `Events.txt` message: the text between the opening tag
/// and `</text>`. The markup itself is `DisplayBox` syntax, not the plain text
/// a `TextBox` shows.
pub fn plain_text(markup: &str) -> String {
    let text = match markup.find('>') {
        Some(at) => &markup[at + 1..],
        None => markup,
    };
    let text = text.strip_suffix("</text>").unwrap_or(text);
    text.replace("\\n", " ").trim().to_string()
}

fn names_on_path(placed: &[ra2ui::Placement<'_>], path: &[usize]) -> Vec<String> {
    let mut names = Vec::new();
    let Some(first) = path.first() else {
        return names;
    };
    let mut current = &placed[*first];
    names.push(current.widget.name().to_string());
    for index in &path[1..] {
        current = &current.children[*index];
        names.push(current.widget.name().to_string());
    }
    names
}

/// A full-screen `Image` root, drawn behind the award window.
fn full_screen_image(name: &str, file: &str) -> Widget {
    let field = |name: &str, raw: &str| ra2fmt::gib::Field {
        name: Some(name.to_string()),
        raw: raw.to_string(),
    };
    Widget {
        kind: WidgetKind::Image,
        implicit: false,
        fields: vec![
            field("_Tiling", "false"),
            field("_Filename", file),
            field("_BufferWidth", "0 0 0 0"),
            field("_Size", "1280 0 1005 0"),
            field("_Alpha", "true"),
            field("_Filtering", "1"),
            field("_ForeColor", "1 1 1 1"),
            field("_Name", ""),
            field("_Position", "0 0 0 0"),
            field("Name", name),
            field("ID", "-1"),
            field("Position", "0 0 0 0"),
            field("Size", "1 1 1 1"),
            field("Visible", "true"),
            field("ChildOrder", "-1"),
            field("ClipMode", "0"),
            field("NumChildren", "0"),
        ],
        attachments: Vec::new(),
        children: Vec::new(),
    }
}

fn write_png(path: &Path, size: [u32; 2], pixels: Vec<u8>) -> Result<(), String> {
    let image = image::RgbaImage::from_raw(size[0], size[1], pixels)
        .ok_or_else(|| "the readback did not fill the image".to_string())?;
    image
        .save(path)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// The offscreen colour target for the match's 3D pass.
pub struct Target {
    pub color: wgpu::Texture,
    pub color_view: wgpu::TextureView,
    _depth: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
}

impl Target {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Target {
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hq-color"),
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
            label: Some("hq-depth"),
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

/// The offscreen run: open the HQ, optionally play one event, and write the
/// named frames.
pub fn run_headless(options: Options) -> Result<(), String> {
    // The events it plays drive the process-global script layer; everything that
    // runs a scripted match serializes on this lock.
    let _guard = ra2script::lock();
    let out = options
        .out
        .clone()
        .ok_or_else(|| "--hq --out DIR is the offscreen path".to_string())?;
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;

    let renderer = Renderer::new().map_err(|e| e.to_string())?;
    let ui = UiRenderer::new_for(renderer.device(), wgpu::TextureFormat::Rgba8Unorm);
    let target = Target::new(renderer.device(), options.size[0], options.size[1]);

    let mut session = Session::open(&options.game)?;
    session.set_event_seconds(options.seconds);
    session.set_report_damage(options.report_damage);
    println!(
        "HQ: {:?}, season {} (zero-based), {} points, rank {}, {} prizes",
        session.career.team_name(),
        session.career.season(),
        session.career.season_points(),
        session.career.rank(),
        session.career.prizes.len()
    );
    session.write_frame(&renderer, &ui, &target, options.size, &out, "hq.png")?;

    let outcome = if options.simulate_season {
        let mut capture = |session: &mut Session, name: &str| {
            session.write_frame(&renderer, &ui, &target, options.size, &out, name)
        };
        session.simulate_season(options.seconds, &mut capture)?;
        None
    } else {
        match options.simulate_event {
            Some(row) => {
                let mut capture = |session: &mut Session, name: &str| {
                    session.write_frame(&renderer, &ui, &target, options.size, &out, name)
                };
                let outcome = session.simulate(row, options.seconds, &mut capture)?;
                println!("{}", outcome.summary());
                Some(outcome)
            }
            None => {
                for (button, name) in [
                    ("league button", "hq-league.png"),
                    ("events button", "hq-events.png"),
                    ("history button", "hq-history.png"),
                ] {
                    session.select_tab(button)?;
                    session.write_frame(&renderer, &ui, &target, options.size, &out, name)?;
                }
                None
            }
        }
    };

    println!(
        "wrote {} frame(s) to {}",
        std::fs::read_dir(&out)
            .map(|dir| dir.filter_map(Result::ok).count())
            .unwrap_or(0),
        out.display()
    );
    if let Some(outcome) = outcome {
        println!("{}", outcome.summary());
    }
    Ok(())
}

/// The `--report-damage` evidence knob, at component level.
///
/// A shipped match in this environment reports zero damage, so the knob takes
/// the player's first two damageable non-chassis components and writes the
/// same state a real hit would: the first loses `damage` hitpoints, the second
/// is torn off and marked fractured (the `damage.tga` row). It is an evidence
/// input, not a game rule; `Career::repairs_from_report` sees only the report.
fn damage_report_components(robot: &mut ra2sim::r#match::RobotReport, damage: f32) {
    let candidates: Vec<usize> = robot
        .components
        .iter()
        .enumerate()
        .filter(|(_, c)| c.total_hp > 0.0 && c.index != 0 && c.class != "Chassis")
        .map(|(i, _)| i)
        .collect();
    // Prefer two different components (name/class) so the evidence frame shows
    // two distinct rows; fall back to the first two when the robot is built of
    // duplicates.
    let mut chosen: Vec<usize> = Vec::new();
    for &i in &candidates {
        let c = &robot.components[i];
        if chosen
            .iter()
            .all(|&j| robot.components[j].name != c.name || robot.components[j].class != c.class)
        {
            chosen.push(i);
            if chosen.len() == 2 {
                break;
            }
        }
    }
    for &i in &candidates {
        if chosen.len() == 2 {
            break;
        }
        if !chosen.contains(&i) {
            chosen.push(i);
        }
    }
    let Some(&first) = chosen.first() else {
        return;
    };
    let c = &mut robot.components[first];
    c.hp = (c.hp - damage).max(0.0);
    if c.hp <= 0.0 {
        c.detached = true;
        c.fractured = true;
    }
    if let Some(&second) = chosen.get(1) {
        let c = &mut robot.components[second];
        c.hp = 0.0;
        c.detached = true;
        c.fractured = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn game_dir() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("RA2_GAME") {
            let dir = PathBuf::from(dir);
            return dir.is_dir().then_some(dir);
        }
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

    /// A game directory whose `Teams/Teams.txt`, `AI/Teams.txt` and
    /// `Events.txt` are copies (the career save writes them) and whose other
    /// entries are symlinks to the real install, so the UI, arenas and robots
    /// are the shipped ones without copying the game.
    struct TestGame(PathBuf);

    impl TestGame {
        fn new(tag: &str) -> Option<TestGame> {
            let real = game_dir()?;
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let root =
                std::env::temp_dir().join(format!("ra2-hq-{}-{tag}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            for entry in std::fs::read_dir(&real).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name();
                let from = entry.path();
                let to = root.join(&name);
                match name.to_str() {
                    Some("Teams") => {
                        std::fs::create_dir_all(&to).unwrap();
                        for child in std::fs::read_dir(&from).unwrap() {
                            let child = child.unwrap();
                            let child_to = to.join(child.file_name());
                            if child.file_name() == "Teams.txt" {
                                std::fs::copy(child.path(), child_to).unwrap();
                            } else {
                                symlink(&child.path(), &child_to);
                            }
                        }
                    }
                    Some("AI") => {
                        std::fs::create_dir_all(&to).unwrap();
                        for child in std::fs::read_dir(&from).unwrap() {
                            let child = child.unwrap();
                            let child_to = to.join(child.file_name());
                            if child.file_name() == "Teams.txt" {
                                std::fs::copy(child.path(), child_to).unwrap();
                            } else {
                                symlink(&child.path(), &child_to);
                            }
                        }
                    }
                    Some("Events.txt") => {
                        std::fs::copy(from, to).unwrap();
                    }
                    _ => symlink(&from, &to),
                }
            }
            Some(TestGame(root))
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestGame {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn symlink(from: &Path, to: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(from, to).unwrap();
        #[cfg(not(unix))]
        {
            if from.is_dir() {
                std::os::windows::fs::symlink_dir(from, to).unwrap();
            } else {
                std::os::windows::fs::symlink_file(from, to).unwrap();
            }
        }
    }

    /// The roster, league and prizes on the HQ's screens come from the career.
    #[test]
    fn the_hq_view_is_the_careers_roster_points_and_prizes() {
        let Some(game) = game_dir() else { return };
        let career = Career::new(&game, SEED).unwrap();
        let view = build_view(&career, &game).unwrap();
        assert!(!view.robots.is_empty());
        assert!(!view.robots[0].name.is_empty());
        assert_eq!(view.events.len(), 9, "the season's nine events");
        assert_eq!(view.standings.len(), 16);
        assert_eq!(view.standings[0].points, career.season_points());
        assert_eq!(view.events_remaining, 9);
        assert_eq!(view.stats.current_rank, career.rank());
    }

    /// The event's arena and game type are the data's: `Events.txt` names the
    /// arena, and the arena script declares its own `game_types`.
    #[test]
    fn the_event_arena_and_game_type_come_from_the_data() {
        let Some(game) = game_dir() else { return };
        let career = Career::new(&game, SEED).unwrap();
        let events = ra2ui::events::all(&game).unwrap();
        for (index, expected) in [
            ("F.O.R.C.E. Tournament", GameType::Deathmatch),
            ("Blade Tournament", GameType::Tabletop),
            ("Robo-Depot", GameType::KingOfTheHill),
        ] {
            let event = events
                .iter()
                .find(|event| event.name == index)
                .unwrap_or_else(|| panic!("no event {index:?}"));
            let config = match_config(&game, event, &career, 1.0).unwrap();
            assert_eq!(config.game_type, expected, "{index}");
        }
    }

    /// The season's bracket data reaches the UI: the round count is the
    /// event's tier count, the first round has `16 / bots` matches and the
    /// next opponent is one of the fifteen AI teams.
    #[test]
    fn the_hq_view_carries_the_bracket_shape_and_the_next_opponent() {
        let Some(game) = game_dir() else { return };
        let mut career = Career::new(&game, SEED).unwrap();
        let event = career.schedule[0];
        career.enter_event(event).unwrap();
        let view = build_view(&career, &game).unwrap();
        let bracket = view.bracket.expect("the event's bracket");
        let data = &career.events[event];
        assert_eq!(bracket.rounds.len(), data.tiers.len());
        assert!(bracket.can_play);
        assert_eq!(bracket.round, 0);
        assert_eq!(
            bracket.round_label,
            if data.tiers.len() == 1 {
                "Final Match".to_string()
            } else {
                "Round 1".to_string()
            }
        );
        let bots = data.settings.first().copied().unwrap_or(2) as usize;
        assert_eq!(bracket.rounds[0].matches.len(), 16 / bots);
        assert_eq!(bracket.rounds[0].matches[0].slots.len(), bots);
        assert!(bracket.rounds[0].matches[0].slots[0].player);
        assert!(bracket.next_opponent.is_some());
        assert!(bracket.status.starts_with("1 of "));
    }

    #[test]
    fn a_click_on_the_event_row_enters_that_event() {
        let Some(game) = game_dir() else { return };
        let mut session = Session::open(&game).unwrap();
        session.select_tab("events button").unwrap();
        let expected = session.career.schedule[2];
        let point = session.event_point(2).unwrap();
        assert!(session.click(point).unwrap());
        assert_eq!(session.career.entered_event(), Some(expected));
        assert!(matches!(session.screen, Screen::Event { .. }));
    }

    /// The whole flow through the app's state: simulate an event, save, drop the
    /// session, open it again, and the points, prize and roster are the ones on
    /// disk.
    #[test]
    fn a_simulated_event_saves_and_the_next_app_run_reads_the_same_record() {
        let Some(game) = TestGame::new("reload") else {
            eprintln!("skipping HQ save test: no local/game (set RA2_GAME to run it)");
            return;
        };
        let mut session = Session::open(game.path()).unwrap();
        let before = session.career.season_points();
        let mut capture = |_: &mut Session, _: &str| Ok(());
        // The event's match outcomes are the engine's, not this test's: the
        // evidence decision eliminates whichever robot is behind on armour,
        // battery and air. A multi-round event therefore ends anywhere from
        // the first to the final round. The save/reload assertions must not
        // assume a win.
        let outcome = session
            .simulate(1, 0.3, &mut capture)
            .expect("the event flow runs");
        assert_eq!(outcome.points_before, before);
        assert!(outcome.points_after >= before);
        println!(
            "event: {} round(s) played (won {}, lost {}); record points {}; season {} -> {}",
            outcome.rounds_won + outcome.rounds_lost,
            outcome.rounds_won,
            outcome.rounds_lost,
            outcome.event_points,
            outcome.points_before,
            outcome.points_after
        );
        let expected_prizes = usize::from(outcome.won);
        assert_eq!(outcome.prize_model.is_some(), outcome.won);
        assert_eq!(outcome.reloaded_points, outcome.points_after);
        assert_eq!(session.career.season_points(), outcome.points_after);
        assert_eq!(session.career.prizes.len(), expected_prizes);
        assert_eq!(session.career.history.len(), 1);

        // A second app-level run reads what the first saved.
        drop(session);
        let second = Session::open(game.path()).unwrap();
        assert_eq!(second.career.season_points(), outcome.points_after);
        assert_eq!(second.career.rank(), outcome.reloaded_rank);
        assert_eq!(second.career.prizes.len(), expected_prizes);
        if let Some(model) = &outcome.prize_model {
            assert_eq!(second.career.prizes[0].model, *model);
        }
        assert_eq!(second.career.history.len(), 1);
        let view = second.view().unwrap();
        // The standings are in rank order: row 0 is the leader, which need not
        // be the player when an AI outscored them. Find the player's row.
        let player_row = view
            .standings
            .iter()
            .find(|standing| standing.team.name == second.career.team_name())
            .expect("the player's standing");
        assert_eq!(player_row.points, outcome.points_after);
        assert_eq!(view.prizes.len(), expected_prizes);
        assert_eq!(
            view.robots[0].name,
            session_robot_name(game.path()).unwrap()
        );
        // The game's own table carries the same season points and event points
        // the career holds; this is the save-path invariant, independent of
        // the match's result.
        let table =
            ra2fmt::team::read_table(&ra2sim::career::player_table_path(game.path())).unwrap();
        let slot = second.career.player_slot;
        let disk = &table.teams[slot];
        println!(
            "written player record: season points {}, event points {}, event id {}, total events {}, schedule {}",
            disk.season_points(),
            disk.event_points(),
            disk.event_index(),
            disk.total_events(),
            disk.schedule().len()
        );
        assert_eq!(disk.season_points(), second.career.season_points());
        assert_eq!(disk.season_points(), outcome.points_after);
        assert_eq!(disk.event_points(), second.career.player().event_points());
        assert_eq!(disk.schedule().len(), 9);
        assert!(ra2sim::career::sidecar_path(game.path()).is_file());
    }

    fn session_robot_name(game: &Path) -> Option<String> {
        let career = Career::load(game).ok()?;
        let slot = *career.player().robots.first()?;
        let path = player_robot_path(game, career.player_slot, slot);
        read_bot(&path).map(|bot| bot.name)
    }

    /// The season command plays all nine events (deterministic opponents),
    /// ends in `Complete`, and writes the season-level frames.
    #[test]
    fn a_headless_season_run_completes_and_writes_the_season_frames() {
        let Some(game) = TestGame::new("season") else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("ra2-hq-season-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let mut options = Options::new(
            game.path(),
            None,
            Some(dir.clone()),
            [640, 502],
            Some(0.1),
            None,
        )
        .unwrap();
        options.simulate_season = true;
        run_headless(options).expect("the season run completes");
        // Every run writes these: the HQ boot, the in-progress season screen,
        // the first bracket, the completed season screen and the champion
        // screen. `round.png` needs a won round with another round after it;
        // the match outcomes are the engine's, so it is checked when present
        // rather than demanded from every run.
        for name in [
            "hq.png",
            "season.png",
            "bracket.png",
            "season-final.png",
            "champion.png",
        ] {
            let path = dir.join(name);
            let image = image::open(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
                .into_rgba8();
            assert_eq!(image.dimensions(), (640, 502), "{name}");
        }
        let round = dir.join("round.png");
        if round.is_file() {
            let image = image::open(&round)
                .unwrap_or_else(|e| panic!("{}: {e}", round.display()))
                .into_rgba8();
            assert_eq!(image.dimensions(), (640, 502), "round.png");
        }
        let career = Career::load(game.path()).unwrap();
        assert_eq!(career.history.len(), 9, "nine events were played");
        assert_eq!(career.phase, ra2sim::career::SeasonPhase::Complete);
        assert_eq!(career.schedule.len(), 9);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Two damaged components on one robot become two repairs-screen rows; the
    /// repair clock completes them one at a time, the detached row keeps its
    /// name and fractured icon, the save reads back the cleared damage, and a
    /// fresh career opens the screen's own empty state.
    #[test]
    fn damage_appears_on_the_repairs_screen_and_completing_it_persists() {
        use ra2sim::r#match::{ComponentDamage, RobotReport};
        let Some(game) = TestGame::new("repairs") else {
            return;
        };
        let mut session = Session::open(game.path()).unwrap();

        // Healthy: the screen's own empty list.
        session.open_repairs().unwrap();
        assert!(session.repairs().unwrap().rows().is_empty());
        assert_eq!(session.repair_clock(), "0:00");

        let robot = |team: usize, name: &str, damage: f32| RobotReport {
            slot: team,
            team,
            bot_name: name.to_string(),
            team_name: format!("Team {team}"),
            team_logo: String::new(),
            bot_preview: None,
            points: 0,
            damage_dealt: 0.0,
            damage_taken: damage,
            battery: 1.0,
            air: 1.0,
            armor: 1.0,
            defeated: false,
            eliminated: false,
            winner: false,
            immobilized: false,
            components: Vec::new(),
        };
        let mut player = robot(0, "My Robot", 42.0);
        player.components = vec![
            ComponentDamage {
                index: 1,
                name: "Bent Axle".to_string(),
                class: "Axle".to_string(),
                base: "Axle".to_string(),
                hp: 40.0,
                total_hp: 100.0,
                detached: false,
                fractured: false,
            },
            ComponentDamage {
                index: 3,
                name: "Torn Spinner".to_string(),
                class: "SpinMotor".to_string(),
                base: "SpinMotor".to_string(),
                hp: 0.0,
                total_hp: 80.0,
                detached: true,
                fractured: true,
            },
        ];
        let report = MatchReport {
            game_type: "DEATHMATCH",
            arena: "Arenas/box/box.gmf".to_string(),
            state: MatchState::Results,
            reason: None,
            winners: vec![0],
            duration: 3.0,
            duration_limit: None,
            robots: vec![player, robot(1, "Red Zone", 0.0)],
            log: Vec::new(),
        };
        session.career.award_match(&report, 0).unwrap();
        session.open_repairs().unwrap();
        let rows = session.repairs().unwrap().rows().to_vec();
        assert_eq!(rows.len(), 2, "one row per damaged component: {rows:?}");
        assert_eq!(rows[0].robot, "My Robot");
        assert_eq!(rows[0].component, "Bent Axle");
        assert_eq!(rows[0].class, "Axle");
        assert_eq!(rows[0].hp, 40.0);
        assert_eq!(rows[0].total_hp, 100.0);
        assert_eq!(rows[0].hp_percent(), 40);
        assert!(!rows[0].fractured);
        assert_eq!(rows[1].component, "Torn Spinner");
        assert_eq!(rows[1].class, "SpinMotor");
        assert!(rows[1].fractured);
        assert!(rows[0].time.contains("30:00"), "{}", rows[0].time);
        assert_eq!(session.repair_clock(), "30:00");

        // The 30-minute clock at the six-second tick: 299 ticks leave the row,
        // the 300th completes it, and the other component is still pending.
        for _ in 0..299 {
            assert_eq!(session.repair_tick().unwrap(), None);
        }
        assert_eq!(session.career.repairs.len(), 2);
        assert_eq!(session.repair_tick().unwrap().as_deref(), Some("Bent Axle"));
        assert_eq!(session.career.repairs.len(), 1);
        assert_eq!(session.career.repairs[0].component, "Torn Spinner");

        for _ in 0..300 {
            session.repair_tick().unwrap();
        }
        assert!(session.career.repairs.is_empty());
        assert!(session.repairs().unwrap().rows().is_empty());
        assert_eq!(session.career.money, 0, "no repair cost exists");

        // The cleared damage is what the next app run reads.
        drop(session);
        let second = Session::open(game.path()).unwrap();
        assert!(second.career.repairs.is_empty());
        let mut second = second;
        second.open_repairs().unwrap();
        assert!(second.repairs().unwrap().rows().is_empty());
    }

    /// The headless command writes the five named frames (and the after-run
    /// evidence) at the requested size.
    #[test]
    fn a_headless_hq_run_writes_the_named_frames() {
        let Some(game) = TestGame::new("frames") else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("ra2-hq-frames-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let options = Options::new(
            game.path(),
            Some(0),
            Some(dir.clone()),
            [640, 502],
            Some(0.3),
            Some(40.0),
        )
        .unwrap();
        run_headless(options).expect("the headless HQ run completes");
        for name in [
            "hq.png",
            "event.png",
            "match.png",
            "results.png",
            "award.png",
            "repairs-components.png",
            "repairs-done.png",
            "hq-after.png",
            "hq-history.png",
        ] {
            let path = dir.join(name);
            let image = image::open(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
                .into_rgba8();
            assert_eq!(image.dimensions(), (640, 502), "{name}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The robot inventory's `bot snapshot img` cells draw the `.bot` files'
    /// own TGA snapshots. The scene names in-memory textures with the
    /// content-addressed `#bot-preview:` keys, and the written `hq.png` pixel at
    /// each picture's centre is the source picture's centre — not `nobot.bmp`
    /// and not the white fill.
    #[test]
    fn the_inventory_cells_draw_the_bots_own_snapshots() {
        let Some(game) = TestGame::new("previews") else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("ra2-hq-previews-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let options = Options::new(
            game.path(),
            None,
            Some(dir.clone()),
            [1280, 1005],
            Some(0.3),
            None,
        )
        .unwrap();
        run_headless(options).expect("the headless HQ run completes");
        let image = image::open(dir.join("hq.png"))
            .expect("hq.png")
            .into_rgba8();
        assert_eq!(image.dimensions(), (1280, 1005));

        let mut session = Session::open(game.path()).unwrap();
        assert!(session
            .career
            .player()
            .robots
            .iter()
            .any(|slot| read_bot(&player_robot_path(
                game.path(),
                session.career.player_slot,
                *slot
            ))
            .is_some_and(|bot| bot.snapshot.is_some())));
        let (scene, _) = session.scene().unwrap();
        let mut checked = 0;
        for quad in &scene.quads {
            let texture = &scene.textures[quad.texture];
            if !texture.label.starts_with("#bot-preview:hq-") {
                continue;
            }
            // A tenth into the picture is clear of the cell's name/class text,
            // which the layout hangs over the right part of the strip.
            let (sx, sy) = (texture.width / 10, texture.height / 10);
            let source = ((sy * texture.width + sx) * 4) as usize;
            let expected = [
                texture.rgba[source],
                texture.rgba[source + 1],
                texture.rgba[source + 2],
            ];
            let x = (quad.rect.x + quad.rect.w * 0.1).floor() as u32;
            let y = (quad.rect.y + quad.rect.h * 0.1).floor() as u32;
            let actual = image.get_pixel(x, y);
            for channel in 0..3 {
                assert!(
                    (i32::from(actual[channel]) - i32::from(expected[channel])).abs() <= 2,
                    "inventory preview at ({x},{y}) is {:?}, not {:?} from {}",
                    actual,
                    expected,
                    texture.label
                );
            }
            checked += 1;
        }
        assert!(checked > 0, "the inventory drew a #bot-preview texture");
        std::fs::remove_dir_all(&dir).ok();
    }
}
