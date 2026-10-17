//! The career: teams, a nine-event season, points, prizes and repairs.
//!
//! The recon behind this module is in `docs/14-career.md`. The short form:
//!
//! * **Teams** are the game's own records (`ra2fmt::team`). The player's league
//!   points live at `Team+0x58` (the second closing line); the fifteen AI
//!   teams' points are the `Team+0x5c`..`+0x94` line; the current event's
//!   points are `Team+0x54`; the history counters are the eight ints at
//!   `Team+0x98`..`+0xb4`.
//! * **A season is nine events** (`FUN_00500120` builds nine entries;
//!   `FUN_004ffcc0` fills a 3×3 grid and opens the bracket when more than
//!   eight are complete; `FUN_0046e2b0` compares the count with 9). The nine
//!   are drawn from the 25 in `Events.txt`; the exact RNG was not recovered,
//!   so [`Career::new`] takes an explicit seed and [`Career::set_schedule`]
//!   takes the schedule outright.
//! * A team **enters events individually**, one at a time (`FUN_00500c40`'s
//!   message is "Each team may enter one event at a time. Finish your current
//!   event before beginning a new one.").
//! * **Event points** are the established formula in `FUN_0046c400`:
//!   `robot_points / 200 + 160 / robots` for the player and `160 / robots`
//!   for every other participant. Damage is only worth points in King of the
//!   Hill, where the arena script itself calls `plus.addPoints`; no other
//!   damage-to-points rule was located, so this module does not invent one.
//! * A **trophy** (the event's `trophy` model from `Events.txt`) is stored when
//!   the event is won (`FUN_0046ca90` → `FUN_004fab50`); the losing tiers are
//!   shown on the results screen but are not stored in the team record.
//! * **Repairs** are a time-driven progress bar, not a cash purchase: the
//!   repairs window shows damaged components and `repair time`, and completing
//!   the bar calls `FUN_005ff530` to restore the components. No repair cost or
//!   cash field was recovered, so cost and time are explicit
//!   [`RepairPolicy`] config with zero cost and the window's `0x1e` (30) as
//!   the default seconds.
//!
//! ## Save location
//!
//! The game's own files are authoritative for the classic fields:
//! `Teams/Teams.txt` (the player's record and the league line) and
//! `AI/Teams.txt` (the fifteen AI identities). Both are written with
//! [`TeamTable::write`], so an unchanged career is byte-identical to the
//! shipped files. The fields the team record has no recovered home for — the
//! season phase, the pending repair list, the per-event history and the
//! explicit money counter — live in a documented sidecar,
//! `Teams/career.dat`, next to the game's own table. The sidecar is never a
//! substitute for `Teams.txt`: it is versioned text and is only read by this
//! crate.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ra2fmt::event::Event;
use ra2fmt::team::{self, Team, TeamTable};

use crate::r#match::MatchReport;

/// The sidecar's file name under `<game>/Teams/`.
pub const SIDECAR_NAME: &str = "career.dat";
/// The sidecar format version.
pub const SIDECAR_VERSION: u32 = 1;

/// The season's shape: a new career starts on the schedule, enters one event,
/// and is complete after nine finished events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeasonPhase {
    /// Between events; an event from the schedule may be entered.
    Schedule,
    /// The team is in an event, identified by its index in `Events.txt`.
    InEvent { event: usize },
    /// Nine events are finished; the season results (or the golden sawblade)
    /// follow.
    Complete,
}

impl SeasonPhase {
    fn name(&self) -> &'static str {
        match self {
            SeasonPhase::Schedule => "schedule",
            SeasonPhase::InEvent { .. } => "event",
            SeasonPhase::Complete => "complete",
        }
    }
}

/// Everything that is explicit because the binary did not establish a value.
///
/// `participation_points` (160) and `score_divisor` (200) are the two constants
/// in `FUN_0046c400`. `events_per_season` is the `9` in `FUN_00500120`,
/// `FUN_004ffcc0` and `FUN_0046e2b0`. `max_prizes` is the `0x13` guard in
/// `FUN_004fab50` (the twentieth prize drops the oldest). `repair_seconds` and
/// `repair_cost` are **not** established: the repairs window passes `0x1e` to
/// the progress bar, but its unit is unknown, and no cost path was found.
#[derive(Clone, Debug, PartialEq)]
pub struct CareerPolicy {
    pub participation_points: i32,
    pub score_divisor: i32,
    pub events_per_season: usize,
    pub max_prizes: usize,
    pub repair_seconds: f32,
    pub repair_cost: i64,
}

impl Default for CareerPolicy {
    fn default() -> Self {
        CareerPolicy {
            participation_points: 160,
            score_divisor: 200,
            events_per_season: 9,
            max_prizes: 20,
            repair_seconds: 30.0,
            repair_cost: 0,
        }
    }
}

impl CareerPolicy {
    /// A policy with an explicit repair time and cost, keeping the recovered
    /// scoring constants.
    pub fn with_repairs(seconds: f32, cost: i64) -> Self {
        CareerPolicy {
            repair_seconds: seconds,
            repair_cost: cost,
            ..CareerPolicy::default()
        }
    }
}

/// A prize the team keeps. The model path is the event's trophy model; the
/// tier prizes are descriptions shown on the results screen and are available
/// through [`Career::prize_for_rounds`], not stored as team property.
#[derive(Clone, Debug, PartialEq)]
pub struct Prize {
    /// The event's index in `Events.txt`.
    pub event: usize,
    /// The season it was won in.
    pub season: i32,
    /// The `.gmf` model path (`Arenas\event_results\prizes\…`).
    pub model: String,
    /// The winning message, as `Events.txt` carries it.
    pub description: String,
    /// The trophy art (`UI\teamhq\events\trophies\…`).
    pub art: String,
}

/// One pending repair. `component` is the damaged robot rather than a single
/// component: a [`MatchReport`] carries per-robot damage and defeat only, and
/// the game's component-level damage list (`Team+0x12c`) is not parsed here.
#[derive(Clone, Debug, PartialEq)]
pub struct Repair {
    pub robot: String,
    pub component: String,
    pub cost: i64,
    pub seconds: f32,
}

/// One finished event, as the history screen counts it.
#[derive(Clone, Debug, PartialEq)]
pub struct EventRecord {
    pub event: usize,
    /// Matches won in this event.
    pub rounds_won: u32,
    /// Matches lost in this event.
    pub rounds_lost: u32,
    /// The points the event added to the season (`Team+0x54`).
    pub points: i32,
    /// Whether the team won the event (the trophy).
    pub won: bool,
}

/// What one match report added.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchAward {
    /// The points added to the team's current event (`0x54`).
    pub points: i32,
    /// `participation + robot_points / 200`.
    pub participation: i32,
    pub bonus: i32,
    /// Points added to each AI team's league total, by the report's team slot.
    pub opponents: BTreeMap<usize, i32>,
    /// Whether the entered robot is among the report's winners.
    pub won: bool,
}

/// The career: the game's team tables, the event list, the season and the
/// sidecar-only state.
#[derive(Clone, Debug)]
pub struct Career {
    pub policy: CareerPolicy,
    /// The player's table: every record, including the empty slots.
    pub player_table: TeamTable,
    /// The fifteen AI records.
    pub ai_table: TeamTable,
    /// The named record in `player_table` that is the player's team.
    pub player_slot: usize,
    /// `Events.txt`, in file order.
    pub events: Vec<Event>,
    pub phase: SeasonPhase,
    /// The nine event indices for this season.
    pub schedule: Vec<usize>,
    pub prizes: Vec<Prize>,
    /// Pending repairs from unfinished matches.
    pub repairs: Vec<Repair>,
    pub history: Vec<EventRecord>,
    /// Explicit money. No cash field or deduction was recovered from the
    /// binary; the counter exists so a caller can model an economy and is kept
    /// in the sidecar, not in the game's team file.
    pub money: i64,
    /// Matches won in the current event.
    pub rounds_won: u32,
    /// Matches lost in the current event.
    pub rounds_lost: u32,
}

impl Career {
    /// Build a career from already-parsed tables and events (no I/O). The
    /// schedule and prizes come from the player's team record; the sidecar-only
    /// fields start empty.
    pub fn from_tables(
        player_table: TeamTable,
        ai_table: TeamTable,
        events: Vec<Event>,
        policy: CareerPolicy,
    ) -> Result<Self, String> {
        let player_slot = player_table
            .teams
            .iter()
            .position(Team::is_named)
            .ok_or_else(|| "the player's team table has no named team".to_string())?;
        let player = &player_table.teams[player_slot];
        let schedule = player
            .schedule()
            .into_iter()
            .filter_map(|id| usize::try_from(id).ok())
            .filter(|id| *id < events.len())
            .collect();
        let season = player.season_index();
        let prizes = player
            .prizes()
            .into_iter()
            .map(|model| Prize {
                event: usize::MAX,
                season,
                model,
                description: String::new(),
                art: String::new(),
            })
            .collect();
        let phase = match player.event_index() {
            index if index >= 0 && (index as usize) < events.len() => SeasonPhase::InEvent {
                event: index as usize,
            },
            _ => SeasonPhase::Schedule,
        };
        Ok(Career {
            policy,
            player_table,
            ai_table,
            player_slot,
            events,
            phase,
            schedule,
            prizes,
            repairs: Vec::new(),
            history: Vec::new(),
            money: 0,
            rounds_won: 0,
            rounds_lost: 0,
        })
    }

    /// Read a career from a game directory (`Teams/Teams.txt`,
    /// `AI/Teams.txt`, `Events.txt`, and `Teams/career.dat` when it exists).
    pub fn load(game: &Path) -> Result<Self, String> {
        Self::load_with_policy(game, CareerPolicy::default())
    }

    /// [`Career::load`] with an explicit policy.
    pub fn load_with_policy(game: &Path, policy: CareerPolicy) -> Result<Self, String> {
        let player_table = team::read_table(&player_table_path(game))?;
        let ai_table = team::read_table(&ai_table_path(game))?;
        let text = ra2fmt::event::read(&events_path(game))?;
        let events = ra2fmt::event::parse(&text)
            .map_err(|e| format!("{}: {e}", events_path(game).display()))?;
        let mut career = Self::from_tables(player_table, ai_table, events, policy)?;
        let sidecar = sidecar_path(game);
        if sidecar.is_file() {
            let text = std::fs::read_to_string(&sidecar)
                .map_err(|e| format!("{}: {e}", sidecar.display()))?;
            career
                .read_sidecar(&text)
                .map_err(|e| format!("{}: {e}", sidecar.display()))?;
        }
        career.sync_history();
        Ok(career)
    }

    /// Start a career from the game's tables, creating a nine-event schedule
    /// when the player's record has none. The original picks nine distinct
    /// events with an RNG (`FUN_00500120`); its seed is not recovered, so the
    /// caller states one. Use [`Career::set_schedule`] for an exact calendar.
    pub fn new(game: &Path, seed: u64) -> Result<Self, String> {
        Self::new_with_policy(game, seed, CareerPolicy::default())
    }

    /// [`Career::new`] with an explicit policy.
    pub fn new_with_policy(game: &Path, seed: u64, policy: CareerPolicy) -> Result<Self, String> {
        let mut career = Self::load_with_policy(game, policy)?;
        career.ensure_schedule(seed);
        Ok(career)
    }

    /// The player's team record.
    pub fn player(&self) -> &Team {
        &self.player_table.teams[self.player_slot]
    }

    /// The player's team record, mutably.
    pub fn player_mut(&mut self) -> &mut Team {
        &mut self.player_table.teams[self.player_slot]
    }

    /// The fifteen AI records that carry identity (empty slots are skipped).
    pub fn ai_teams(&self) -> impl Iterator<Item = &Team> {
        self.ai_table.teams.iter().filter(|t| t.is_named())
    }

    /// The name of the player's team.
    pub fn team_name(&self) -> &str {
        &self.player().name
    }

    /// The season number (`Team+0x98`, zero-based).
    pub fn season(&self) -> i32 {
        self.player().season_index()
    }

    /// The player's season points (`Team+0x58`).
    pub fn season_points(&self) -> i32 {
        self.player().season_points()
    }

    /// The league rank: one plus the number of AI teams with more points
    /// (`FUN_00500230`'s comparison against `Team+0x58`).
    pub fn rank(&self) -> i32 {
        let player = self.season_points();
        let ahead = self
            .player()
            .opponent_points()
            .iter()
            .filter(|points| **points > player)
            .count();
        ahead as i32 + 1
    }

    /// Write a nine-event schedule into the team record. Ids are indices into
    /// `Events.txt`; duplicates and out-of-range ids are rejected.
    pub fn set_schedule(&mut self, ids: &[usize]) -> Result<(), String> {
        if ids.len() != self.policy.events_per_season {
            return Err(format!(
                "a season has {} events, {} were given",
                self.policy.events_per_season,
                ids.len()
            ));
        }
        for (i, id) in ids.iter().enumerate() {
            if *id >= self.events.len() {
                return Err(format!("event {id} is not in Events.txt"));
            }
            if ids[..i].contains(id) {
                return Err(format!("event {id} appears twice"));
            }
        }
        self.schedule = ids.to_vec();
        self.player_mut()
            .set_schedule(&ids.iter().map(|id| *id as i32).collect::<Vec<_>>());
        self.sync_history();
        Ok(())
    }

    /// Keep the player's `+0xf4` history lines in step with the career: one
    /// int per scheduled event, holding that event's points. When the career
    /// has no history but the record already carries some, they are kept (a
    /// game-written season is not overwritten with zeros).
    fn sync_history(&mut self) {
        let values = if self.schedule.is_empty() {
            Vec::new()
        } else if !self.history.is_empty() {
            self.schedule
                .iter()
                .map(|id| {
                    self.history
                        .iter()
                        .find(|record| record.event == *id)
                        .map_or(0, |record| record.points)
                })
                .collect::<Vec<_>>()
        } else if !self.player().history().is_empty() {
            self.player().history().to_vec()
        } else {
            vec![0; self.schedule.len()]
        };
        self.player_mut().set_history(&values);
    }

    /// Fill an empty schedule from the event list with a small deterministic
    /// PRNG. Nine distinct events, as `FUN_00500120` draws; the original's RNG
    /// and seed are not recovered.
    pub fn ensure_schedule(&mut self, seed: u64) {
        if !self.schedule.is_empty() || self.events.is_empty() {
            return;
        }
        let want = self.policy.events_per_season.min(self.events.len());
        let mut x = seed | 1;
        let mut ids = Vec::with_capacity(want);
        while ids.len() < want {
            x = x
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let id = (x >> 33) as usize % self.events.len();
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        self.player_mut()
            .set_schedule(&ids.iter().map(|id| *id as i32).collect::<Vec<_>>());
        self.schedule = ids;
        self.sync_history();
    }

    /// Enter an event from this season's schedule.
    pub fn enter_event(&mut self, event: usize) -> Result<(), String> {
        if !matches!(self.phase, SeasonPhase::Schedule) {
            return Err(format!(
                "the career is not on the schedule (it is {})",
                self.phase.name()
            ));
        }
        if !self.schedule.contains(&event) {
            return Err(format!("event {event} is not on this season's schedule"));
        }
        self.phase = SeasonPhase::InEvent { event };
        self.rounds_won = 0;
        self.rounds_lost = 0;
        self.player_mut().set_event_points(0);
        self.player_mut().set_defeated(0);
        self.player_mut().set_event_index(event as i32);
        Ok(())
    }

    /// The prize tier for a number of wins in an event: the tier the results
    /// screen shows. `rounds_won` at or past the tier list is the trophy.
    pub fn prize_for_rounds(&self, event: usize, rounds_won: usize) -> Option<&(String, String)> {
        let event = self.events.get(event)?;
        event.tiers.get(rounds_won)
    }

    /// The event the career is in, if any.
    pub fn entered_event(&self) -> Option<usize> {
        match self.phase {
            SeasonPhase::InEvent { event } => Some(event),
            _ => None,
        }
    }

    /// Award one match report.
    ///
    /// `entered_robot` is the report robot the team entered. Only that robot's
    /// script points count; a teammate's points are ignored, which is the
    /// 1.20 rule ("hitting teammates no longer adds points") applied to the
    /// career total. Every other team gets the established participation
    /// points, `160 / robots`.
    pub fn award_match(
        &mut self,
        report: &MatchReport,
        entered_robot: usize,
    ) -> Result<MatchAward, String> {
        let entered = report
            .robots
            .get(entered_robot)
            .ok_or_else(|| format!("the report has no robot {entered_robot}"))?;
        let player_team = entered.team;
        let robots = report.robots.len().max(1) as i32;
        let participation = self.policy.participation_points / robots;
        let bonus = entered.points.max(0) / self.policy.score_divisor;
        let points = participation + bonus;

        let mut opponents = BTreeMap::new();
        for robot in &report.robots {
            if robot.team == player_team {
                // A teammate does not score for the team.
                continue;
            }
            *opponents.entry(robot.team).or_insert(0) += participation;
        }

        // The player's league line is one entry per AI team, in AI order.
        let mut league = self.player().opponent_points();
        for (team, amount) in &opponents {
            if let Some(slot) = ai_slot(*team, player_team) {
                if slot < league.len() {
                    league[slot] += *amount;
                }
            }
        }
        self.player_mut().set_opponent_points(league);

        {
            let player = self.player_mut();
            player.set_event_points(player.event_points() + points);
            if entered.points > player.highest_score() {
                player.set_highest_score(entered.points);
            }
            let defeated = report
                .robots
                .iter()
                .filter(|robot| robot.team == player_team && robot.defeated)
                .count() as i32;
            player.set_defeated(player.defeated() + defeated);
        }

        let won = report.winners.contains(&entered_robot);
        if won {
            self.rounds_won += 1;
            if report.duration < self.player().fastest_win() {
                self.player_mut().set_fastest_win(report.duration);
            }
        } else {
            self.rounds_lost += 1;
        }

        let repairs = self.repairs_from_report(report, entered_robot);
        for repair in repairs {
            if !self
                .repairs
                .iter()
                .any(|old| old.robot == repair.robot && old.component == repair.component)
            {
                self.repairs.push(repair);
            }
        }

        Ok(MatchAward {
            points,
            participation,
            bonus,
            opponents,
            won,
        })
    }

    /// The repair list a report leaves: one entry per damaged or defeated
    /// robot on the team. Component-level damage is not in [`MatchReport`];
    /// `component` names the robot and the cost/time come from
    /// [`CareerPolicy`] because the binary did not establish them.
    pub fn repairs_from_report(&self, report: &MatchReport, entered_robot: usize) -> Vec<Repair> {
        let Some(entered) = report.robots.get(entered_robot) else {
            return Vec::new();
        };
        report
            .robots
            .iter()
            .filter(|robot| robot.team == entered.team)
            .filter(|robot| robot.defeated || robot.damage_taken > 0.0)
            .map(|robot| Repair {
                robot: robot.bot_name.clone(),
                component: robot.bot_name.clone(),
                cost: self.policy.repair_cost,
                seconds: self.policy.repair_seconds,
            })
            .collect()
    }

    /// Pay for and complete one pending repair.
    pub fn apply_repair(&mut self, index: usize) -> Result<Repair, String> {
        let repair = self
            .repairs
            .get(index)
            .cloned()
            .ok_or_else(|| format!("there is no repair {index}"))?;
        if self.money < repair.cost {
            return Err(format!(
                "a repair costs {} but the team has {}",
                repair.cost, self.money
            ));
        }
        self.money -= repair.cost;
        self.repairs.remove(index);
        Ok(repair)
    }

    /// Complete every pending repair, cheapest first.
    pub fn apply_repairs(&mut self) -> Result<Vec<Repair>, String> {
        let mut done = Vec::new();
        while !self.repairs.is_empty() {
            done.push(self.apply_repair(0)?);
        }
        Ok(done)
    }

    /// Finish the current event: fold its points into the season, count the
    /// event, store the trophy when won, recompute the rank and advance the
    /// phase after nine events.
    pub fn finish_event(&mut self, won: bool) -> Result<EventRecord, String> {
        let event = self
            .entered_event()
            .ok_or_else(|| "no event is in progress".to_string())?;
        let points = self.player().event_points();
        let record = EventRecord {
            event,
            rounds_won: self.rounds_won,
            rounds_lost: self.rounds_lost,
            points,
            won,
        };

        let rounds_won = self.rounds_won;
        let rounds_lost = self.rounds_lost;
        let player = self.player_mut();
        player.set_season_points(player.season_points() + points);
        player.set_event_points(0);
        player.set_defeated(0);
        player.set_total_events(player.total_events() + 1);
        player.set_events_won(player.events_won() + i32::from(won));
        let wins = player.wins() + rounds_won as i32;
        let losses = player.losses() + rounds_lost as i32;
        player.set_wins(wins);
        player.set_losses(losses);
        let rank = {
            let player_points = player.season_points();
            let ahead = player
                .opponent_points()
                .iter()
                .filter(|p| **p > player_points)
                .count();
            ahead as i32 + 1
        };
        player.set_current_rank(rank);
        if rank < player.best_rank() {
            player.set_best_rank(rank);
        }
        player.set_event_index(-1);
        player.event_robot = -1;

        if won {
            let event_data = &self.events[event];
            let prize = Prize {
                event,
                season: self.season(),
                model: event_data.trophy.0.clone(),
                description: event_data.trophy.1.clone(),
                art: event_data.trophy.2.clone(),
            };
            self.prizes.push(prize);
            if self.prizes.len() > self.policy.max_prizes {
                self.prizes.remove(0);
            }
        }

        self.history.push(record.clone());
        self.rounds_won = 0;
        self.rounds_lost = 0;
        self.sync_history();

        if self.history.len() >= self.policy.events_per_season {
            self.phase = SeasonPhase::Complete;
            // The season champion's award (`FUN_004e84b0`): the golden
            // sawblade, stored when the player tops the final table.
            if self.rank() == 1 {
                self.prizes.push(Prize {
                    event: usize::MAX,
                    season: self.season(),
                    model: "UI\\teamhq\\golden_sawblade.tga".to_string(),
                    description: "Season champion".to_string(),
                    art: "UI\\teamhq\\golden_sawblade.tga".to_string(),
                });
                if self.prizes.len() > self.policy.max_prizes {
                    self.prizes.remove(0);
                }
            }
        } else {
            self.phase = SeasonPhase::Schedule;
        }

        let models = self
            .prizes
            .iter()
            .map(|prize| prize.model.clone())
            .collect::<Vec<_>>();
        self.player_mut().set_prizes(&models);
        Ok(record)
    }

    /// Start the next season: clear the league, the schedule and the history,
    /// bump `Team+0x98`, and draw a new nine-event calendar.
    pub fn next_season(&mut self, seed: u64) -> Result<(), String> {
        if !matches!(self.phase, SeasonPhase::Complete) {
            return Err("the season is not complete".to_string());
        }
        let player = self.player_mut();
        player.set_season_index(player.season_index() + 1);
        player.set_season_points(0);
        player.set_event_points(0);
        player.set_opponent_points([0; 15]);
        player.set_current_rank(16);
        player.set_event_index(-1);
        player.event_robot = -1;
        player.set_prizes(&[]);
        player.set_schedule(&[]);
        player.set_history(&[]);
        self.schedule.clear();
        self.prizes.clear();
        self.history.clear();
        self.repairs.clear();
        self.rounds_won = 0;
        self.rounds_lost = 0;
        self.phase = SeasonPhase::Schedule;
        self.ensure_schedule(seed);
        Ok(())
    }

    /// Write the game's two team tables and the sidecar.
    ///
    /// The player's record gets the recovered fields (league line, closing
    /// lines, history ints, schedule and prize lines, event id). The AI table
    /// is written back unchanged: the league reads the AI totals from the
    /// player's record, not from the AI records.
    pub fn save(&self, game: &Path) -> Result<(), String> {
        let mut player_table = self.player_table.clone();
        let player = &mut player_table.teams[self.player_slot];
        let prizes = self
            .prizes
            .iter()
            .map(|prize| prize.model.clone())
            .collect::<Vec<_>>();
        let schedule = self
            .schedule
            .iter()
            .map(|id| *id as i32)
            .collect::<Vec<_>>();
        player.set_prizes(&prizes);
        player.set_schedule(&schedule);
        // `+0xf4` is true once a season schedule exists; the nine ints are the
        // per-slot points. Without a sidecar history, keep the game's own ints.
        let history = if self.schedule.is_empty() {
            Vec::new()
        } else if !self.history.is_empty() {
            self.schedule
                .iter()
                .map(|id| {
                    self.history
                        .iter()
                        .find(|record| record.event == *id)
                        .map_or(0, |record| record.points)
                })
                .collect()
        } else if !self.player().history().is_empty() {
            self.player().history().to_vec()
        } else {
            vec![0; self.schedule.len()]
        };
        player.set_history(&history);
        if !matches!(self.phase, SeasonPhase::InEvent { .. }) {
            player.event_robot = -1;
            player.set_event_index(-1);
        }

        let player_path = player_table_path(game);
        let ai_path = ai_table_path(game);
        for (path, table) in [(&player_path, &player_table), (&ai_path, &self.ai_table)] {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            table
                .write_to(path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }

        let sidecar = sidecar_path(game);
        if let Some(parent) = sidecar.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&sidecar, self.write_sidecar())
            .map_err(|e| format!("{}: {e}", sidecar.display()))?;
        Ok(())
    }

    /// The sidecar text, in a fixed order for byte-stable saves.
    fn write_sidecar(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("ra2career {SIDECAR_VERSION}\n"));
        out.push_str(&format!("season {}\n", self.season()));
        match &self.phase {
            SeasonPhase::Schedule => out.push_str("phase schedule\n"),
            SeasonPhase::InEvent { event } => out.push_str(&format!("phase event {event}\n")),
            SeasonPhase::Complete => out.push_str("phase complete\n"),
        }
        out.push_str(&format!("money {}\n", self.money));
        out.push_str(&format!(
            "rounds {} {}\n",
            self.rounds_won, self.rounds_lost
        ));
        out.push_str(&format!(
            "entered {}\n",
            self.entered_event().map_or(-1, |e| e as i64)
        ));
        out.push_str(&format!("schedule {}\n", self.schedule.len()));
        for id in &self.schedule {
            out.push_str(&format!("event {id}\n"));
        }
        out.push_str(&format!("prizes {}\n", self.prizes.len()));
        for prize in &self.prizes {
            // `usize::MAX` is a prize recovered from the team line without a
            // sidecar, so its event is unknown; `-1` says so.
            let event = if prize.event == usize::MAX {
                "-1".to_string()
            } else {
                prize.event.to_string()
            };
            out.push_str(&format!(
                "prize {}\t{}\t{}\t{}\t{}\n",
                event,
                prize.season,
                escape(&prize.model),
                escape(&prize.description),
                escape(&prize.art)
            ));
        }
        out.push_str(&format!("repairs {}\n", self.repairs.len()));
        for repair in &self.repairs {
            out.push_str(&format!(
                "repair {}\t{}\t{}\t{}\n",
                escape(&repair.robot),
                escape(&repair.component),
                repair.cost,
                repair.seconds
            ));
        }
        out.push_str(&format!("history {}\n", self.history.len()));
        for record in &self.history {
            out.push_str(&format!(
                "result {}\t{}\t{}\t{}\t{}\n",
                record.event,
                record.rounds_won,
                record.rounds_lost,
                record.points,
                u8::from(record.won)
            ));
        }
        out
    }

    /// Read a sidecar over the team-file state. The sidecar is the home of the
    /// fields the team record does not carry; the team record remains the home
    /// of the game's own points, ranks and schedule.
    fn read_sidecar(&mut self, text: &str) -> Result<(), String> {
        let mut lines = text.lines();
        let header = lines.next().ok_or("the sidecar is empty")?;
        let version = header
            .strip_prefix("ra2career ")
            .and_then(|v| v.parse::<u32>().ok())
            .ok_or_else(|| format!("bad sidecar header {header:?}"))?;
        if version != SIDECAR_VERSION {
            return Err(format!(
                "sidecar version {version}, expected {SIDECAR_VERSION}"
            ));
        }
        let mut schedule = None;
        let mut prizes = None;
        let mut repairs = None;
        let mut history = None;
        while let Some(line) = lines.next() {
            if line.trim().is_empty() {
                continue;
            }
            let (key, rest) = line.split_once(' ').unwrap_or((line, ""));
            match key {
                "season" => {
                    let season: i32 = rest.parse().map_err(|_| "bad season")?;
                    self.player_mut().set_season_index(season);
                }
                "phase" => {
                    self.phase = match rest.split_whitespace().collect::<Vec<_>>().as_slice() {
                        ["schedule"] => SeasonPhase::Schedule,
                        ["event", id] => SeasonPhase::InEvent {
                            event: id.parse().map_err(|_| "bad event id")?,
                        },
                        ["complete"] => SeasonPhase::Complete,
                        other => return Err(format!("bad phase {other:?}")),
                    };
                }
                "money" => self.money = rest.parse().map_err(|_| "bad money")?,
                "rounds" => {
                    let mut parts = rest.split_whitespace();
                    self.rounds_won = parts
                        .next()
                        .and_then(|v| v.parse().ok())
                        .ok_or("bad rounds")?;
                    self.rounds_lost = parts
                        .next()
                        .and_then(|v| v.parse().ok())
                        .ok_or("bad rounds")?;
                }
                "entered" => {
                    // The phase carries the event; this line is a cross-check.
                    let id: i64 = rest.parse().map_err(|_| "bad entered")?;
                    if id >= 0 && !matches!(self.phase, SeasonPhase::InEvent { .. }) {
                        self.phase = SeasonPhase::InEvent { event: id as usize };
                    }
                }
                "schedule" => {
                    let count: usize = rest.parse().map_err(|_| "bad schedule count")?;
                    let mut ids = Vec::with_capacity(count);
                    for _ in 0..count {
                        let line = lines.next().ok_or("the schedule ends early")?;
                        let id = line
                            .strip_prefix("event ")
                            .ok_or_else(|| format!("bad schedule line {line:?}"))?;
                        ids.push(id.parse::<usize>().map_err(|_| "bad schedule id")?);
                    }
                    schedule = Some(ids);
                }
                "prizes" => {
                    let count: usize = rest.parse().map_err(|_| "bad prize count")?;
                    let mut list = Vec::with_capacity(count);
                    for _ in 0..count {
                        let line = lines.next().ok_or("the prize list ends early")?;
                        let body = line
                            .strip_prefix("prize ")
                            .ok_or_else(|| format!("bad prize line {line:?}"))?;
                        let mut fields = body.split('\t');
                        let event = fields
                            .next()
                            .and_then(|v| v.parse::<i64>().ok())
                            .map(|v| if v < 0 { usize::MAX } else { v as usize })
                            .ok_or("bad prize event")?;
                        let season = fields
                            .next()
                            .and_then(|v| v.parse::<i32>().ok())
                            .ok_or("bad prize season")?;
                        let model = unescape(fields.next().ok_or("bad prize model")?);
                        let description = unescape(fields.next().ok_or("bad prize text")?);
                        let art = unescape(fields.next().ok_or("bad prize art")?);
                        list.push(Prize {
                            event,
                            season,
                            model,
                            description,
                            art,
                        });
                    }
                    prizes = Some(list);
                }
                "repairs" => {
                    let count: usize = rest.parse().map_err(|_| "bad repair count")?;
                    let mut list = Vec::with_capacity(count);
                    for _ in 0..count {
                        let line = lines.next().ok_or("the repair list ends early")?;
                        let body = line
                            .strip_prefix("repair ")
                            .ok_or_else(|| format!("bad repair line {line:?}"))?;
                        let mut fields = body.split('\t');
                        let robot = unescape(fields.next().ok_or("bad repair robot")?);
                        let component = unescape(fields.next().ok_or("bad repair component")?);
                        let cost = fields
                            .next()
                            .and_then(|v| v.parse::<i64>().ok())
                            .ok_or("bad repair cost")?;
                        let seconds = fields
                            .next()
                            .and_then(|v| v.parse::<f32>().ok())
                            .ok_or("bad repair seconds")?;
                        list.push(Repair {
                            robot,
                            component,
                            cost,
                            seconds,
                        });
                    }
                    repairs = Some(list);
                }
                "history" => {
                    let count: usize = rest.parse().map_err(|_| "bad history count")?;
                    let mut list = Vec::with_capacity(count);
                    for _ in 0..count {
                        let line = lines.next().ok_or("the history ends early")?;
                        let body = line
                            .strip_prefix("result ")
                            .ok_or_else(|| format!("bad history line {line:?}"))?;
                        let mut fields = body.split('\t');
                        let event = fields
                            .next()
                            .and_then(|v| v.parse::<usize>().ok())
                            .ok_or("bad history event")?;
                        let rounds_won = fields
                            .next()
                            .and_then(|v| v.parse::<u32>().ok())
                            .ok_or("bad history wins")?;
                        let rounds_lost = fields
                            .next()
                            .and_then(|v| v.parse::<u32>().ok())
                            .ok_or("bad history losses")?;
                        let points = fields
                            .next()
                            .and_then(|v| v.parse::<i32>().ok())
                            .ok_or("bad history points")?;
                        let won = fields
                            .next()
                            .and_then(|v| v.parse::<u8>().ok())
                            .ok_or("bad history won")?
                            != 0;
                        list.push(EventRecord {
                            event,
                            rounds_won,
                            rounds_lost,
                            points,
                            won,
                        });
                    }
                    history = Some(list);
                }
                other => return Err(format!("unknown sidecar key {other:?}")),
            }
        }
        if let Some(ids) = schedule {
            self.schedule = ids;
        }
        if let Some(list) = prizes {
            self.prizes = list;
        }
        if let Some(list) = repairs {
            self.repairs = list;
        }
        if let Some(list) = history {
            self.history = list;
        }
        Ok(())
    }
}

/// Map a report team slot to the player's fifteen-entry league line.
fn ai_slot(team: usize, player_team: usize) -> Option<usize> {
    if team == player_team {
        return None;
    }
    let slot = if team > player_team { team - 1 } else { team };
    (slot < 15).then_some(slot)
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// `<game>/Teams/Teams.txt`.
pub fn player_table_path(game: &Path) -> PathBuf {
    game.join("Teams").join("Teams.txt")
}

/// `<game>/AI/Teams.txt`.
pub fn ai_table_path(game: &Path) -> PathBuf {
    game.join("AI").join("Teams.txt")
}

/// `<game>/Events.txt`.
pub fn events_path(game: &Path) -> PathBuf {
    game.join("Events.txt")
}

/// `<game>/Teams/career.dat` — the documented sidecar.
pub fn sidecar_path(game: &Path) -> PathBuf {
    game.join("Teams").join(SIDECAR_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::r#match::{MatchReport, MatchState, RobotReport};
    use ra2fmt::bot::LineEnding;
    use ra2fmt::team::TeamValue;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(tag: &str) -> Self {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("ra2career-{}-{}-{n}", std::process::id(), tag));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(path.join("Teams")).unwrap();
            std::fs::create_dir_all(path.join("AI")).unwrap();
            TestDir(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn game_dir() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("RA2_GAME") {
            let dir = PathBuf::from(dir);
            return dir.is_dir().then_some(dir);
        }
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        while dir.pop() {
            let candidate = dir.join("local/game");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
        None
    }

    /// Copy the three files a career needs into a temporary game directory.
    fn copy_game(tag: &str) -> Option<TestDir> {
        let game = game_dir()?;
        let dir = TestDir::new(tag);
        for rel in ["Teams/Teams.txt", "AI/Teams.txt", "Events.txt"] {
            let from = game.join(rel);
            let to = dir.path().join(rel);
            std::fs::copy(&from, &to)
                .unwrap_or_else(|e| panic!("{} -> {}: {e}", from.display(), to.display()));
        }
        Some(dir)
    }

    fn robot(
        slot: usize,
        team: usize,
        name: &str,
        points: i32,
        damage: f32,
        defeated: bool,
    ) -> RobotReport {
        RobotReport {
            slot,
            team,
            bot_name: name.to_string(),
            team_name: format!("Team {team}"),
            team_logo: String::new(),
            bot_preview: String::new(),
            points,
            damage_dealt: 0.0,
            damage_taken: damage,
            defeated,
            eliminated: defeated,
            winner: false,
            immobilized: false,
        }
    }

    fn report(
        game_type: &'static str,
        robots: Vec<RobotReport>,
        winners: Vec<usize>,
    ) -> MatchReport {
        MatchReport {
            game_type,
            arena: "Arenas/box/box.gmf".to_string(),
            state: MatchState::Results,
            reason: None,
            winners,
            duration: 42.0,
            robots,
            log: Vec::new(),
        }
    }

    /// A minimal table with one named player record (and no empty slots) and
    /// the fifteen AI records, for the tests that do not need the shipped data.
    fn synthetic_tables(events: &[Event]) -> (TeamTable, TeamTable) {
        let mut player = Team {
            name: "Test Team".to_string(),
            motto: "Test".to_string(),
            logo: "Teams\\test.bmp".to_string(),
            robots: vec![0, 1, 2],
            state: vec![TeamValue::Int(0); 14],
            ..Team::default()
        };
        player.state[Team::EVENT_INDEX] = TeamValue::Int(-1);
        player.set_schedule(&[]);
        let mut player_table = TeamTable {
            prefix: Some(0),
            teams: vec![player],
            line_ending: LineEnding::Crlf,
            final_newline: true,
        };
        let mut ai_table = TeamTable {
            prefix: None,
            teams: Vec::new(),
            line_ending: LineEnding::Crlf,
            final_newline: false,
        };
        for (i, event) in events.iter().take(15).enumerate() {
            let mut state = vec![TeamValue::Int(0); 14];
            state[Team::PLAYER_FLAG] = TeamValue::Bool(true);
            state[Team::HAS_HISTORY] = TeamValue::Bool(false);
            ai_table.teams.push(Team {
                head: Some(team::RecordHead::Index(i as i32)),
                name: event.name.clone(),
                motto: "AI".to_string(),
                logo: format!("AI\\team{i}.bmp"),
                robots: vec![0, 1, 2],
                state,
                tail: vec![0, 0, 0],
                ..Team::default()
            });
        }
        player_table.teams[0].set_schedule(&[0, 1, 2, 3, 4, 5, 6, 7, 8]);
        (player_table, ai_table)
    }

    fn synthetic_events(count: usize) -> Vec<Event> {
        (0..count)
            .map(|i| Event {
                name: format!("Event {i}"),
                description: String::new(),
                tiers: vec![
                    (
                        "Arenas\\event_results\\prizes\\A.gmf".to_string(),
                        "tier 0".to_string(),
                    ),
                    (
                        "Arenas\\event_results\\prizes\\B.gmf".to_string(),
                        "tier 1".to_string(),
                    ),
                ],
                trophy: (
                    format!("Arenas\\event_results\\prizes\\Trophy{i}\\Trophy{i}.gmf"),
                    format!("won event {i}"),
                    "UI\\teamhq\\events\\trophies\\Trophy.tga".to_string(),
                ),
                art: String::new(),
                arena: ("Combat Arena".to_string(), String::new()),
                settings: vec![2, 1, 1, 1],
            })
            .collect()
    }

    fn synthetic_career() -> Career {
        let events = synthetic_events(25);
        let (player, ai) = synthetic_tables(&events);
        Career::from_tables(player, ai, events, CareerPolicy::default()).unwrap()
    }

    /// Write the 25 synthetic events in `Events.txt`'s own record shape, so a
    /// `Career::load` test has a real event list to read.
    fn write_synthetic_events(dir: &Path) {
        let mut text = String::from("0\n");
        for i in 0..25 {
            text.push_str(&format!(
                "Event {i}\ndescription\n2\n\
                 Arenas\\event_results\\prizes\\A.gmf\ntier 0\n\
                 Arenas\\event_results\\prizes\\B.gmf\ntier 1\n\
                 Arenas\\event_results\\prizes\\Trophy{i}\\Trophy{i}.gmf\n\
                 won event {i}\nUI\\teamhq\\events\\trophies\\Trophy.tga\n\
                 art.bmp\nCombat Arena\npreview.bmp\n2\n1\n1\n1\n"
            ));
        }
        std::fs::write(events_path(dir), text).unwrap();
    }

    /// The established scoring formula, the teammate rule, and a full
    /// enter → award → finish → repair → save → reload run with every field
    /// compared.
    #[test]
    fn a_career_run_scores_repairs_saves_and_reloads_identically() {
        let Some(dir) = copy_game("run") else {
            eprintln!("skipping career run: no local/game (set RA2_GAME to run it)");
            return;
        };
        let mut career = Career::new(dir.path(), 7).unwrap();
        assert_eq!(career.schedule.len(), 9, "a season is nine events");
        assert!(
            career.schedule.iter().all(|id| *id < career.events.len()),
            "every scheduled event exists"
        );
        let event = career.schedule[0];
        career.enter_event(event).unwrap();

        // The player's robot scores 500; the AI robot is defeated. Two robots
        // means 160/2 = 80 participation each, and 500/200 = 2 for the player.
        let mut first = robot(0, 0, "Test Team", 500, 20.0, false);
        first.winner = true;
        let mut second = robot(1, 1, "Red Zone", 0, 80.0, true);
        second.winner = false;
        let award = career
            .award_match(
                &report("DEATHMATCH", vec![first.clone(), second], vec![0]),
                0,
            )
            .unwrap();
        assert_eq!(award.participation, 80);
        assert_eq!(award.bonus, 2);
        assert_eq!(award.points, 82);
        assert_eq!(career.player().event_points(), 82);
        assert_eq!(career.player().season_points(), 0);
        assert_eq!(career.player().opponent_points()[0], 80);
        assert_eq!(career.rounds_won, 1);

        // A teammate's 900 points are not the team's: the awarded points are
        // still the entered robot's 500.
        let teammate = robot(1, 0, "Teammate", 900, 0.0, false);
        let award = career
            .award_match(
                &report("DEATHMATCH", vec![first.clone(), teammate], vec![0]),
                0,
            )
            .unwrap();
        assert_eq!(award.bonus, 2, "the teammate's points do not count");
        assert_eq!(award.opponents.len(), 0, "a teammate is not an opponent");
        assert_eq!(career.player().event_points(), 164);

        // The report left one repair per damaged/defeated team robot.
        assert_eq!(career.repairs.len(), 1);
        assert_eq!(career.repairs[0].robot, "Test Team");
        assert_eq!(career.repairs[0].cost, 0);
        assert_eq!(career.repairs[0].seconds, 30.0);

        let record = career.finish_event(true).unwrap();
        assert_eq!(record.event, event);
        assert_eq!(record.points, 164);
        assert_eq!(career.player().season_points(), 164);
        assert_eq!(career.player().event_points(), 0);
        assert_eq!(career.player().total_events(), 1);
        assert_eq!(career.player().events_won(), 1);
        assert_eq!(career.prizes.len(), 1);
        assert_eq!(career.prizes[0].event, event);
        assert!(career.prizes[0].model.ends_with(".gmf"));
        assert_eq!(career.rank(), 1, "164 beats the AI's 80");
        assert_eq!(career.player().current_rank(), 1);
        assert_eq!(career.player().best_rank(), 1);

        // Money has no game field; it is explicit sidecar state and must still
        // round-trip.
        career.money = 1234;
        let before = snapshot(&career);
        career.save(dir.path()).unwrap();
        let first_team = std::fs::read(dir.path().join("Teams/Teams.txt")).unwrap();
        let first_ai = std::fs::read(dir.path().join("AI/Teams.txt")).unwrap();
        let first_side = std::fs::read(sidecar_path(dir.path())).unwrap();

        // The game's own file carries the schedule and the nine history lines:
        // the state we recover is not sidecar-only.
        let written = team::read_table(&player_table_path(dir.path())).unwrap();
        assert_eq!(
            written.teams[career.player_slot].schedule(),
            career
                .schedule
                .iter()
                .map(|id| *id as i32)
                .collect::<Vec<_>>()
        );
        assert_eq!(written.teams[career.player_slot].history().len(), 9);
        assert!(written.teams[career.player_slot].has_history());
        assert_eq!(written.teams[career.player_slot].prizes().len(), 1);

        let loaded = Career::load(dir.path()).unwrap();
        assert_eq!(snapshot(&loaded), before, "every career field survives");

        // Writing the loaded career again is byte-stable.
        loaded.save(dir.path()).unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("Teams/Teams.txt")).unwrap(),
            first_team
        );
        assert_eq!(
            std::fs::read(dir.path().join("AI/Teams.txt")).unwrap(),
            first_ai
        );
        assert_eq!(std::fs::read(sidecar_path(dir.path())).unwrap(), first_side);

        // Repair a damaged robot: the list empties and the money (explicit,
        // zero-cost here) is unchanged.
        let mut loaded = loaded;
        let repair = loaded.apply_repair(0).unwrap();
        assert_eq!(repair.robot, "Test Team");
        assert!(loaded.repairs.is_empty());
        assert_eq!(loaded.money, 1234, "a zero-cost repair does not charge");
        loaded.save(dir.path()).unwrap();
        let back = Career::load(dir.path()).unwrap();
        assert!(back.repairs.is_empty(), "the repaired list round-trips");
        assert_eq!(back.player().season_points(), 164);
    }

    /// The event list is the shipped file: 25 events, tiers two or four.
    #[test]
    fn the_event_list_is_the_shipped_twenty_five() {
        let Some(game) = game_dir() else { return };
        let text = ra2fmt::event::read(&game.join("Events.txt")).unwrap();
        let events = ra2fmt::event::parse(&text).unwrap();
        assert_eq!(events.len(), 25);
        assert_eq!(
            events.iter().filter(|e| e.tiers.len() == 4).count(),
            14,
            "the shipped tier split"
        );
        assert_eq!(events.iter().filter(|e| e.tiers.len() == 2).count(), 11);
    }

    /// The season ends after nine events and the next one starts clean.
    #[test]
    fn nine_events_complete_the_season_and_the_next_one_resets_it() {
        let mut career = synthetic_career();
        for n in 0..9 {
            let event = career.schedule[n];
            career.enter_event(event).unwrap();
            career
                .award_match(
                    &report(
                        "DEATHMATCH",
                        vec![
                            robot(0, 0, "Test Team", 200, 0.0, false),
                            robot(1, 1, "AI", 0, 0.0, true),
                        ],
                        vec![0],
                    ),
                    0,
                )
                .unwrap();
            career.finish_event(true).unwrap();
        }
        assert_eq!(career.phase, SeasonPhase::Complete);
        assert_eq!(career.history.len(), 9);
        assert_eq!(career.player().total_events(), 9);
        assert_eq!(career.player().events_won(), 9);
        assert_eq!(career.player().wins(), 9);
        assert!(career.prizes.len() >= 9, "each won event stores a trophy");
        // 200/200 + 160/2 = 81 an event.
        assert_eq!(career.player().season_points(), 9 * 81);
        assert_eq!(career.rank(), 1);

        career.next_season(11).unwrap();
        assert_eq!(career.phase, SeasonPhase::Schedule);
        assert_eq!(career.season(), 1);
        assert_eq!(career.player().season_points(), 0);
        assert_eq!(career.history.len(), 0);
        assert_eq!(career.schedule.len(), 9);
        assert_eq!(career.player().total_events(), 9, "lifetime count is kept");
    }

    /// A no-op career load/save leaves the shipped team tables byte-identical.
    #[test]
    fn the_shipped_team_tables_round_trip_after_a_no_op_career() {
        let Some(dir) = copy_game("noop") else {
            eprintln!("skipping career corpus test: no local/game (set RA2_GAME to run it)");
            return;
        };
        let original_player = std::fs::read(dir.path().join("Teams/Teams.txt")).unwrap();
        let original_ai = std::fs::read(dir.path().join("AI/Teams.txt")).unwrap();
        let career = Career::load(dir.path()).unwrap();
        career.save(dir.path()).unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("Teams/Teams.txt")).unwrap(),
            original_player,
            "Teams/Teams.txt is byte-identical"
        );
        assert_eq!(
            std::fs::read(dir.path().join("AI/Teams.txt")).unwrap(),
            original_ai,
            "AI/Teams.txt is byte-identical"
        );
    }

    /// A no-op save/reload of a synthetic career is byte-stable too.
    #[test]
    fn the_career_sidecar_is_byte_stable() {
        let dir = TestDir::new("stable");
        write_synthetic_events(dir.path());
        let career = synthetic_career();
        career.save(dir.path()).unwrap();
        let first = std::fs::read(sidecar_path(dir.path())).unwrap();
        let loaded = Career::load(dir.path()).unwrap();
        loaded.save(dir.path()).unwrap();
        let second = std::fs::read(sidecar_path(dir.path())).unwrap();
        assert_eq!(first, second);
        assert_eq!(loaded.schedule, career.schedule);
        assert_eq!(loaded.player().season, career.player().season);
    }

    #[test]
    fn the_sidecar_round_trips_every_text_field() {
        let dir = TestDir::new("text");
        write_synthetic_events(dir.path());
        let mut career = synthetic_career();
        career.enter_event(career.schedule[0]).unwrap();
        career
            .award_match(
                &report(
                    "DEATHMATCH",
                    vec![
                        robot(0, 0, "Name\twith\\tabs", 0, 1.0, false),
                        robot(1, 1, "AI", 0, 0.0, true),
                    ],
                    vec![0],
                ),
                0,
            )
            .unwrap();
        career.finish_event(true).unwrap();
        career.save(dir.path()).unwrap();
        let loaded = Career::load(dir.path()).unwrap();
        assert_eq!(loaded.prizes, career.prizes);
        assert_eq!(loaded.history, career.history);
    }

    #[allow(clippy::type_complexity)]
    fn snapshot(
        career: &Career,
    ) -> (
        Vec<i32>,
        Vec<i32>,
        Vec<String>,
        Vec<i32>,
        i32,
        i32,
        Vec<usize>,
        Vec<Prize>,
        Vec<Repair>,
        Vec<EventRecord>,
        i64,
    ) {
        (
            career.player().season.to_vec(),
            career.player().tail.clone(),
            career
                .player()
                .state
                .iter()
                .map(ToString::to_string)
                .collect(),
            career.player().history().to_vec(),
            career.player().season_index(),
            career.player().season_points(),
            career.schedule.clone(),
            career.prizes.clone(),
            career.repairs.clone(),
            career.history.clone(),
            career.money,
        )
    }
}
