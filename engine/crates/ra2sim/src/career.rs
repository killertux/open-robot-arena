//! The career: teams, a nine-event season, points, prizes and repairs.
//!
//! The recon behind this module is in `docs/16-career.md`. The short form:
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

use crate::lcg::Lcg;
use crate::r#match::MatchReport;

/// The sidecar's file name under `<game>/Teams/`.
pub const SIDECAR_NAME: &str = "career.dat";
/// The sidecar format version.
pub const SIDECAR_VERSION: u32 = 1;

/// Every event's bracket is 16 entrants, `FUN_004695a0`'s `16.0` at
/// `0x6a7280` (the constant is compared with `1.0` at `0x6a5600`).
pub const BRACKET_ENTRANTS: usize = 16;
/// The fifteen AI teams an opponent is drawn from (`FUN_00419190(0xf)` at
/// `0x4696d1`; the argument 15 is the exclusive bound).
pub const BRACKET_AI_TEAMS: usize = 15;

/// One participant in an event bracket.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Entrant {
    /// The career's own team.
    Player,
    /// An AI team, by its slot in `AI/Teams.txt` order (0…14).
    Ai(usize),
}

impl Entrant {
    /// `-1` for the player, the AI slot otherwise: the sidecar's spelling.
    pub fn code(&self) -> i32 {
        match self {
            Entrant::Player => -1,
            Entrant::Ai(slot) => *slot as i32,
        }
    }

    /// Read a sidecar code. A negative code is the player; 0…14 is an AI slot.
    pub fn from_code(code: i32) -> Option<Self> {
        if code < 0 {
            Some(Entrant::Player)
        } else if (code as usize) < BRACKET_AI_TEAMS {
            Some(Entrant::Ai(code as usize))
        } else {
            None
        }
    }
}

/// One match of an event round: the bots that fight it. The player is always
/// in `rounds[current].matches[0].entrants[0]` (`FUN_004695a0` skips the random
/// draw for match 0, slot 0; `FUN_0046c830` writes the player back there).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BracketMatch {
    pub entrants: Vec<Entrant>,
}

/// One round's matches. Round 0 has `16 / bots` matches, and each later round
/// has `1 / bots` of the previous count (`FUN_004695a0`'s `local_5c` loop).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BracketRound {
    pub matches: Vec<BracketMatch>,
}

/// One played round, as the bracket screen shows it and as the save keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundResult {
    /// Zero-based round index.
    pub round: usize,
    /// The opponents the player met in this round, in slot order.
    pub opponents: Vec<Entrant>,
    pub won: bool,
    /// The points this round added to the event (`Team+0x54`).
    pub points: i32,
    /// The entered robot's score (`robot_points / 200` is the bonus part).
    pub robot_points: i32,
}

/// Where the player stands in the event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BracketStatus {
    /// There are rounds left to play.
    Playing,
    /// The player lost in this zero-based round; the event is over.
    Eliminated { round: usize },
    /// The player won the final round.
    Champion,
}

impl BracketStatus {
    fn name(&self) -> &'static str {
        match self {
            BracketStatus::Playing => "playing",
            BracketStatus::Eliminated { .. } => "eliminated",
            BracketStatus::Champion => "champion",
        }
    }
}

/// The event bracket: 16 entrants, 2 or 4 bots per match, and the player's
/// path through the rounds.
///
/// The shape is measured from the binary (`docs/16-career.md` §3):
///
/// * `FUN_004695a0` starts at `16.0` (`0x6a7280`) and divides by the bracket
///   config's bots-per-match (`+0x10c`) for each round's match count.
/// * `FUN_00468f50` stores the round's bots (`+4`) and advancing entrants
///   (`+8`); the shipped value is one winner per match.
/// * `FUN_0046c240` calls `"Final Match"` when the round index
///   (`Team+0x110`) equals the bracket's `+0x8` minus one, so `+0x8` is the
///   round count: 4 for a two-bot event, 2 for a four-bot event.
/// * `Events.txt`'s settings line carries that bots-per-match value: every
///   four-tier event has setting 2, every two-tier event has setting 4.
///
/// The field is the original's, drawn from the engine LCG ([`Lcg`]): the bracket
/// is built in one go (`FUN_004695a0`: for each round, every slot but match 0's
/// first is `FUN_00419190(15)`, redrawn while the team is already in that
/// round's list), and when the player wins, the next round's field is the
/// winners of this round's matches (`FUN_0046c830`/`FUN_0046cde0`: slot 0 is the
/// player, slot `k` is entrant `FUN_00419190(bots)` of match `k`). Live (docs/36
/// section 9.23) the first draw of the Blade Tournament bracket from state
/// 4037340146 gave the 16 teams the screen showed, and the three winner draws
/// after round 1 the three opponents of the final. The state a session reaches
/// the bracket with depends on the clock and on every draw the effects made
/// before, so `seed` is explicit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventBracket {
    /// The event's `Events.txt` index.
    pub event: usize,
    /// 16 (`BRACKET_ENTRANTS`).
    pub entrants: usize,
    /// Bots in one match: 2 or 4 (`Events.txt`'s first setting).
    pub bots_per_match: usize,
    /// 2 or 4 (`Events.txt`'s tier count).
    pub rounds: usize,
    /// The zero-based round the player is in.
    pub round: usize,
    /// The engine LCG state the bracket was drawn from ([`Lcg`]).
    pub seed: u64,
    /// The state now: after the creation draws, and after each round's winners.
    lcg: Lcg,
    pub status: BracketStatus,
    /// The current round's field. `opponents` is slot 1 onward of the player's
    /// match (match 0), i.e. the next bots to beat.
    pub current: BracketRound,
    pub opponents: Vec<Entrant>,
    /// The rounds already played, in order.
    pub results: Vec<RoundResult>,
    /// The event's winner once it is over; the player if they took the final.
    pub champion: Option<Entrant>,
}

impl EventBracket {
    /// The bracket an event starts with. The original's `FUN_004695a0` builds
    /// the whole field at once; this builds the player's current round and
    /// leaves later rounds to be drawn on advancement (the original redraws
    /// them in `FUN_0046c830` too).
    pub fn new(event: usize, data: &Event, seed: u64) -> Result<Self, String> {
        let (rounds, bots_per_match) = event_shape(data)?;
        let mut bracket = EventBracket {
            event,
            entrants: BRACKET_ENTRANTS,
            bots_per_match,
            rounds,
            round: 0,
            seed,
            lcg: Lcg(seed as u32),
            status: BracketStatus::Playing,
            current: BracketRound {
                matches: Vec::new(),
            },
            opponents: Vec::new(),
            results: Vec::new(),
            champion: None,
        };
        bracket.draw_field();
        Ok(bracket)
    }

    /// The engine state after the draws made so far.
    pub fn lcg_state(&self) -> u32 {
        self.lcg.0
    }

    /// Replay the advancement to `round` (a reloaded bracket): the winner draws
    /// do not depend on how the player fared.
    pub fn replay_to(&mut self, round: usize) {
        while self.round < round && self.round + 1 < self.rounds {
            self.round += 1;
            self.advance_field(true);
        }
        self.round = round;
    }

    /// The event's round label: `"Final Match"` in the last round, `"Round N"`
    /// otherwise — exactly `FUN_0046c240`'s branch at `0x46c283`/`0x46c295`.
    pub fn round_label(&self) -> String {
        if self.round + 1 >= self.rounds {
            "Final Match".to_string()
        } else {
            format!("Round {}", self.round + 1)
        }
    }

    /// The names of the bots waiting in the current round.
    pub fn opponents(&self) -> &[Entrant] {
        &self.opponents
    }

    /// How many matches the player's current round has.
    pub fn matches_in_round(&self, round: usize) -> usize {
        let divisor = self.bots_per_match.saturating_pow(round as u32);
        (self.entrants / divisor.max(1)).max(1) / self.bots_per_match.max(1)
    }

    /// `FUN_004695a0`: draw the whole bracket from the engine LCG. Every round
    /// has `16 / bots^(round + 1)` matches of `bots` slots; every slot but match
    /// 0's first is `FUN_00419190(15)`, redrawn while the team is in this round's
    /// list (`FUN_005014d0`; the list starts empty each round). Only round 0's
    /// field is kept: the later rounds' slots are overwritten with the winners
    /// ([`EventBracket::advance_field`]), but their draws are made, so the state
    /// ends where the original's does. Live: 46 draws for the Blade Tournament
    /// (15 + 3 accepted, 28 repeats).
    fn draw_field(&mut self) {
        let mut first = None;
        for round in 0..self.rounds {
            let matches = self.matches_in_round(round);
            let mut drawn: Vec<usize> = Vec::new();
            let mut field = BracketRound {
                matches: Vec::with_capacity(matches),
            };
            for m in 0..matches {
                let mut entrants = Vec::with_capacity(self.bots_per_match);
                for slot in 0..self.bots_per_match {
                    if m == 0 && slot == 0 {
                        entrants.push(Entrant::Player);
                        continue;
                    }
                    let mut team = self.lcg.range(BRACKET_AI_TEAMS);
                    // The original loops until the team is new; a field wider
                    // than the AI table (a mod's) would never end, so it stops.
                    while drawn.len() < BRACKET_AI_TEAMS && drawn.contains(&team) {
                        team = self.lcg.range(BRACKET_AI_TEAMS);
                    }
                    drawn.push(team);
                    entrants.push(Entrant::Ai(team));
                }
                field.matches.push(BracketMatch { entrants });
            }
            if first.is_none() {
                first = Some(field);
            }
        }
        self.current = first.unwrap_or(BracketRound {
            matches: Vec::new(),
        });
        self.refresh_opponents();
    }

    /// `FUN_0046c830`: the next round's field when the round is over. The new
    /// round has one slot per match of this one; slot 0 is the player while the
    /// player is alive (no draw), every other slot `k` is entrant
    /// `FUN_00419190(bots)` of this round's match `k` (`FUN_0046cde0`). With the
    /// player out (`player_alive` false), slot 0 is a winner of match 0 other than
    /// the player: redrawn while it is entrant 0 (`FUN_0046cde0`'s second mode;
    /// from the code, not seen live).
    fn advance_field(&mut self, player_alive: bool) {
        let bots = self.bots_per_match;
        let winners: Vec<Entrant> = (0..self.current.matches.len())
            .map(|k| {
                if k == 0 && player_alive {
                    return Entrant::Player;
                }
                let mut pick = self.lcg.range(bots);
                while k == 0 && pick == 0 && bots > 1 {
                    pick = self.lcg.range(bots);
                }
                self.current.matches[k].entrants[pick]
            })
            .collect();
        let matches = winners
            .chunks(bots.max(1))
            .map(|c| BracketMatch {
                entrants: c.to_vec(),
            })
            .collect();
        self.current = BracketRound { matches };
        self.refresh_opponents();
    }

    fn refresh_opponents(&mut self) {
        self.opponents = self
            .current
            .matches
            .first()
            .map(|m| m.entrants.iter().skip(1).copied().collect())
            .unwrap_or_default();
    }

    /// Replace the current round's draw with a saved field. The saved
    /// opponents are what matters for the player's path; the other matches are
    /// regenerated from the seed.
    pub fn restore_opponents(&mut self, opponents: &[Entrant]) {
        if let Some(first) = self.current.matches.first_mut() {
            for (slot, entrant) in opponents.iter().enumerate() {
                if let Some(target) = first.entrants.get_mut(slot + 1) {
                    *target = *entrant;
                }
            }
        }
        self.opponents = opponents.to_vec();
    }

    /// Draw and score the remaining rounds for the AI teams, as
    /// `FUN_0046ca90(0,0)` does after a loss. The player takes no further
    /// points. The final round's winner is drawn from its field.
    fn simulate_remaining(&mut self, participation: i32, league: &mut [i32]) {
        let first = match self.status {
            BracketStatus::Eliminated { round } => round + 1,
            _ => return,
        };
        let mut finalists = Vec::new();
        // A loss in the final round has no rounds left to simulate, but the
        // final's winner still has to be drawn (`FUN_0046ca90` always reaches
        // the final-winner pick).
        if first >= self.rounds {
            finalists = self
                .current
                .matches
                .first()
                .map(|m| {
                    m.entrants
                        .iter()
                        .filter_map(|e| match e {
                            Entrant::Ai(slot) => Some(*slot),
                            Entrant::Player => None,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
        }
        for round in first..self.rounds {
            let saved_round = self.round;
            self.round = round;
            self.advance_field(false);
            for m in &self.current.matches {
                for entrant in &m.entrants {
                    if let Entrant::Ai(slot) = entrant {
                        if *slot < league.len() {
                            league[*slot] += participation;
                        }
                    }
                }
            }
            if round + 1 == self.rounds {
                finalists = self
                    .current
                    .matches
                    .first()
                    .map(|m| {
                        m.entrants
                            .iter()
                            .filter_map(|e| match e {
                                Entrant::Ai(slot) => Some(*slot),
                                Entrant::Player => None,
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
            }
            self.round = saved_round;
        }
        if !finalists.is_empty() {
            let champion = finalists[self.lcg.range(finalists.len())];
            self.champion = Some(Entrant::Ai(champion));
            if champion < league.len() {
                // The final winner's extra share (`FUN_0046ca90`'s last
                // `FUN_0046c5d0`) is the same participation amount.
                league[champion] += participation;
            }
        }
    }
}

/// The event's bracket shape: the tier count is the round count, and the
/// first trailing setting is the bots in a match.
///
/// The shipped data agrees on all 25 events: four tiers ↔ setting 2, two
/// tiers ↔ setting 4, and `bots ^ rounds == 16`. When a file does not, the
/// tier count still decides the rounds and the bots are derived from the
/// 16-entrant constant; that fallback is explicit rather than a guess.
pub fn event_shape(event: &Event) -> Result<(usize, usize), String> {
    let rounds = event.tiers.len();
    if !(2..=4).contains(&rounds) {
        return Err(format!(
            "event {:?} has {rounds} tiers; the shipped brackets are 2 or 4 rounds",
            event.name
        ));
    }
    let from_file = event
        .settings
        .first()
        .copied()
        .filter(|bots| {
            (*bots == 2 || *bots == 4) && (*bots as usize).pow(rounds as u32) == BRACKET_ENTRANTS
        })
        .map(|bots| bots as usize);
    let bots = from_file.unwrap_or(match rounds {
        4 => 2,
        2 => 4,
        // Only 2 and 4 ship; this branch keeps the function total for a mod.
        3 => 2,
        _ => 4,
    });
    Ok((rounds, bots))
}

/// The engine-LCG state (`0x726d58`) whose nine draws give the season-1 schedule the
/// original showed for "The Rookies" in the oracle capture (`teamhq-events`: Blade
/// Tournament, Strategic Engineering, Sawblade Elite, Destructavision, Blackvolt,
/// Robotic Terror Zone, Metal Warriors, Toledo, BBA Midwest = events 2 6 12 9 13 15
/// 5 22 20). Found by exhaustive search over the 2^32 states for consecutive draws
/// that yield exactly that list (one 31-bit solution: 1487546588, plus its top-bit
/// twin). The game's state at that moment is not recoverable, so this is the
/// observed sample, not a rule.
pub const ORACLE_SCHEDULE_STATE: u32 = 1_487_546_588;

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
/// `FUN_004fab50` (the twentieth prize drops the oldest). `repair_seconds` is
/// 30 minutes: the repairs window passes `0x1e` to `FUN_0058b150` with the
/// minutes selector, which splits it into the `giTime` fields as `00:30:00`,
/// and the screen's own instructions say "You have 30 minutes to make repairs"
/// (`docs/22-component-repairs.md`). `repair_cost` is **0**: no cost string or
/// subtraction path was found.
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
            repair_seconds: 1_800.0,
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

/// One pending repair, one row per damaged component.
///
/// `robot` is the robot's display name and `component` is the component's
/// display name (`name`, or its class when the definition has no name);
/// `component_index` is the component's index in the `.bot` design, which is
/// the key the original's `Team+0x12c` list carries. `hp`/`total_hp` are the
/// saved current and maximum hitpoints, and `detached`/`fractured` are the
/// `Team+0x12c`/`Team+0x13c` state.
///
/// `elapsed` is the repair clock's progress. The original repairs window is a
/// progress bar over time (`FUN_004d6e10`, full at `FUN_0058b470`), not a
/// purchase; [`Career::advance_repair`] moves this clock and completes the
/// entry. `cost` is kept explicit and is **0**: no repair cost string or
/// subtraction path was recovered.
#[derive(Clone, Debug, PartialEq)]
pub struct Repair {
    pub robot: String,
    pub component: String,
    /// The component's index in the robot design (`Team+0x12c`'s key).
    pub component_index: usize,
    /// The component's `.bot` class (`SpinMotor`, `Wheel`, ...).
    pub component_class: String,
    /// Remaining hitpoints when the match ended.
    pub hp: f32,
    /// Pristine hitpoints.
    pub total_hp: f32,
    /// The component is no longer on the robot.
    pub detached: bool,
    /// A single blow larger than the component's fracture threshold tore it
    /// off; this is the row that gets the shipped `damage.tga` icon.
    pub fractured: bool,
    pub cost: i64,
    pub seconds: f32,
    /// Seconds of repair time already served.
    pub elapsed: f32,
}

impl Repair {
    /// The row's remaining hitpoint fraction (`hp / total_hp`).
    pub fn hp_percent(&self) -> i32 {
        if self.total_hp > 0.0 {
            ((self.hp / self.total_hp).clamp(0.0, 1.0) * 100.0).round() as i32
        } else {
            0
        }
    }
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

/// What one bracket round did.
#[derive(Clone, Debug, PartialEq)]
pub struct RoundOutcome {
    pub event: usize,
    /// The zero-based round that was played.
    pub round: usize,
    pub opponents: Vec<Entrant>,
    pub won: bool,
    pub points: i32,
    pub participation: i32,
    pub bonus: i32,
    pub robot_points: i32,
    /// The event ended with this round (a final win or any loss).
    pub event_finished: bool,
    /// The event's winner, when this round ended it.
    pub champion: Option<Entrant>,
    /// The next round's opponents; empty when the event is over.
    pub next: Vec<Entrant>,
    /// The event record, when this round ended the event.
    pub record: Option<EventRecord>,
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
    /// The season/draw seed. The original's RNG was not recovered; this is the
    /// explicit seed the schedule and the brackets are drawn from.
    pub seed: u64,
    /// The event in progress, when the career is in one.
    pub bracket: Option<EventBracket>,
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
        Self::from_tables_for(player_table, ai_table, events, policy, None)
    }

    /// [`Career::from_tables`] for the team record at `slot` (the one the
    /// select-team screen continued with, `FUN_004eba60`); `None` is the
    /// first named record.
    pub fn from_tables_for(
        player_table: TeamTable,
        ai_table: TeamTable,
        events: Vec<Event>,
        policy: CareerPolicy,
        slot: Option<usize>,
    ) -> Result<Self, String> {
        let player_slot = match slot {
            Some(slot) => {
                if !player_table.teams.get(slot).is_some_and(Team::is_named) {
                    return Err(format!("team slot {slot} is empty"));
                }
                slot
            }
            None => player_table
                .teams
                .iter()
                .position(Team::is_named)
                .ok_or_else(|| "the player's team table has no named team".to_string())?,
        };
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
            seed: 0,
            bracket: None,
        })
    }

    /// Read a career from a game directory (`Teams/Teams.txt`,
    /// `AI/Teams.txt`, `Events.txt`, and `Teams/career.dat` when it exists).
    pub fn load(game: &Path) -> Result<Self, String> {
        Self::load_with_policy(game, CareerPolicy::default())
    }

    /// [`Career::load`] with an explicit policy.
    pub fn load_with_policy(game: &Path, policy: CareerPolicy) -> Result<Self, String> {
        Self::load_team(game, policy, None)
    }

    /// The career of the team record at `slot` (`None`: the first named one).
    pub fn load_team(
        game: &Path,
        policy: CareerPolicy,
        slot: Option<usize>,
    ) -> Result<Self, String> {
        let player_table = team::read_table(&player_table_path(game))?;
        let ai_table = team::read_table(&ai_table_path(game))?;
        let text = ra2fmt::event::read(&events_path(game))?;
        let events = ra2fmt::event::parse(&text)
            .map_err(|e| format!("{}: {e}", events_path(game).display()))?;
        let mut career = Self::from_tables_for(player_table, ai_table, events, policy, slot)?;
        let sidecar = sidecar_path_for(game, career.player_slot);
        if sidecar.is_file() {
            let text = std::fs::read_to_string(&sidecar)
                .map_err(|e| format!("{}: {e}", sidecar.display()))?;
            career
                .read_sidecar(&text)
                .map_err(|e| format!("{}: {e}", sidecar.display()))?;
        }
        career.sync_history();
        if let SeasonPhase::InEvent { event } = career.phase {
            if career.bracket.is_none() {
                if let Some(data) = career.events.get(event) {
                    career.bracket = Some(EventBracket::new(event, data, career.seed)?);
                }
            }
        }
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
        Self::new_team(game, seed, policy, None)
    }

    /// [`Career::new_with_policy`] for the team record at `slot`.
    pub fn new_team(
        game: &Path,
        seed: u64,
        policy: CareerPolicy,
        slot: Option<usize>,
    ) -> Result<Self, String> {
        let mut career = Self::load_team(game, policy, slot)?;
        career.seed = seed;
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

    /// The Team HQ inventory's `scrap button` (`FUN_004ff0c0`, `0x4ff0c0`): robot slot `slot`
    /// (the grid's cell number, `column * 3 + row`) leaves the team's `Robots:` list
    /// (`FUN_0043b180(cell, 0)` clears its flag); the other robots keep their slots and the
    /// cell shows `New Robot`. The robot entered in the current event cannot be scrapped.
    /// Nothing is deleted from disk, live too: `Teams/Team0/Bot4.bot` stayed after its slot
    /// was scrapped, and no code path of the executable removes a `.bot` (the CRT `remove`
    /// is reached only from the table and `ra2.cfg` writers).
    pub fn scrap_robot(&mut self, slot: usize) -> Result<(), String> {
        let position = self
            .player()
            .robots
            .iter()
            .position(|s| *s == slot as i32)
            .ok_or_else(|| format!("no robot in slot {slot}"))?;
        if self.entered_event().is_some() && slot as i32 == self.player().event_robot {
            return Err(SCRAP_IN_EVENT.to_string());
        }
        self.player_mut().robots.remove(position);
        Ok(())
    }

    /// The Team HQ inventory's `import button` (`FUN_00500860` -> the import window's OK `FUN_00470b40`): robot
    /// slot `slot` (an empty cell) joins the team's `Robots:` list (`FUN_0043b180(cell, 1)` sets its flag). The
    /// list is the flags in cell order, so the slot takes its place in the ascending list (live: after
    /// importing into cell 4 `Robots:` read `0 1 2 3 4 5`).
    pub fn import_robot(&mut self, slot: usize) -> Result<(), String> {
        if self.player().robots.contains(&(slot as i32)) {
            return Err(format!("slot {slot} already holds a robot"));
        }
        let robots = &mut self.player_mut().robots;
        robots.push(slot as i32);
        robots.sort_unstable();
        Ok(())
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

    /// Fill an empty schedule as `FUN_00500120` does: nine draws of
    /// `floor(r * N)` (`FUN_00419190`, `N` = the event count, `r` = the engine LCG
    /// `s = s * 0x41c64e6d + 0x3039`, `(s & 0x7fffffff) / 2^31`, global state at
    /// `0x726d58`), each redrawn while the event is already in the list
    /// (`FUN_005014d0`). The game's LCG state at that moment depends on how many
    /// numbers the engine drew since start (particles etc.), so it is not
    /// recoverable; `seed` is that state. The captured season-1 schedule is
    /// [`ORACLE_SCHEDULE_STATE`].
    pub fn ensure_schedule(&mut self, seed: u64) {
        if !self.schedule.is_empty() || self.events.is_empty() {
            return;
        }
        let ids = draw_schedule(
            self.events.len(),
            self.policy.events_per_season,
            seed as u32,
        );
        self.player_mut()
            .set_schedule(&ids.iter().map(|id| *id as i32).collect::<Vec<_>>());
        self.schedule = ids;
        self.sync_history();
    }

    /// Enter an event from this season's schedule. The bracket's field is
    /// drawn from the event's shape and the career's explicit seed
    /// (`FUN_004695a0`; see [`EventBracket`]).
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
        let data = self
            .events
            .get(event)
            .ok_or_else(|| format!("event {event} is not in Events.txt"))?;
        let bracket = EventBracket::new(event, data, self.seed)?;
        // The session's engine state moves on with the draws, so the next
        // event's bracket is another one.
        self.seed = u64::from(bracket.lcg_state());
        self.bracket = Some(bracket);
        self.phase = SeasonPhase::InEvent { event };
        self.rounds_won = 0;
        self.rounds_lost = 0;
        self.player_mut().set_event_points(0);
        self.player_mut().set_defeated(0);
        self.player_mut().set_event_index(event as i32);
        Ok(())
    }

    /// The bracket in progress, if there is one.
    pub fn bracket(&self) -> Option<&EventBracket> {
        self.bracket.as_ref()
    }

    /// The next bots the player must beat, by AI slot. Empty when no event is
    /// in progress or the event is over.
    pub fn next_opponents(&self) -> &[Entrant] {
        match &self.bracket {
            Some(bracket) if bracket.status == BracketStatus::Playing => bracket.opponents(),
            _ => &[],
        }
    }

    /// The AI team's name for a bracket entrant, from the AI table's named
    /// records in file order.
    pub fn entrant_name(&self, entrant: Entrant) -> String {
        match entrant {
            Entrant::Player => self.team_name().to_string(),
            Entrant::Ai(slot) => self
                .ai_teams()
                .nth(slot)
                .map_or_else(|| format!("AI {slot}"), |team| team.name.clone()),
        }
    }

    /// The participation points one match round is worth: `160 / rounds` — the
    /// divisor is the bracket's `+0x8` (`FUN_0046c400` at `0x46c454` and
    /// `0x46c52d`), which `FUN_0046c240` establishes as the round count. When
    /// there is no bracket (the legacy path and the tests), the report's robot
    /// count is used, which is the previous `160 / robots` reading.
    fn participation_for(&self, robots: usize) -> i32 {
        let divisor = self
            .bracket
            .as_ref()
            .map_or(robots, |bracket| bracket.rounds);
        self.policy.participation_points / divisor.max(1) as i32
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
        let participation = self.participation_for(report.robots.len().max(1));
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
            if !self.repairs.iter().any(|old| {
                old.robot == repair.robot && old.component_index == repair.component_index
            }) {
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

    /// The repair list a report leaves: one entry per damaged, detached or
    /// fractured component on the player's robot(s), grouped by robot and then
    /// in design order. This is the original's `Team+0x12c` list: a component
    /// id, its current/maximum hitpoints, and the `Team+0x13c` fractured flag
    /// (`docs/22-component-repairs.md`). The cost/time still come from
    /// [`CareerPolicy`] because the binary charges nothing and its clock is the
    /// window's 30 minutes.
    pub fn repairs_from_report(&self, report: &MatchReport, entered_robot: usize) -> Vec<Repair> {
        let Some(entered) = report.robots.get(entered_robot) else {
            return Vec::new();
        };
        let mut repairs = Vec::new();
        for robot in report
            .robots
            .iter()
            .filter(|robot| robot.team == entered.team)
        {
            for component in robot.components.iter().filter(|c| c.damaged()) {
                repairs.push(Repair {
                    robot: robot.bot_name.clone(),
                    component: component.name.clone(),
                    component_index: component.index,
                    component_class: component.class.clone(),
                    hp: component.hp,
                    total_hp: component.total_hp,
                    detached: component.detached,
                    fractured: component.fractured,
                    cost: self.policy.repair_cost,
                    seconds: self.policy.repair_seconds,
                    elapsed: 0.0,
                });
            }
        }
        repairs
    }

    /// The repair clock's progress in `0.0..=1.0`, as the window's progress bar
    /// computes it (`(max - current) / max`, `FUN_004d6480`).
    pub fn repair_progress(&self, index: usize) -> Option<f32> {
        let repair = self.repairs.get(index)?;
        if repair.seconds <= 0.0 {
            return Some(1.0);
        }
        Some((repair.elapsed / repair.seconds).clamp(0.0, 1.0))
    }

    /// Advance one repair by `seconds` of the repair clock. When the bar fills,
    /// the entry is completed and removed from the pending list — the original
    /// calls `FUN_004d7010`/`FUN_005ff530` at that moment. Nothing is charged:
    /// `repair_cost` is an explicit zero with no recovered cost path.
    pub fn advance_repair(&mut self, index: usize, seconds: f32) -> Result<Option<Repair>, String> {
        if !(seconds.is_finite() && seconds >= 0.0) {
            return Err(format!("repair time {} is not a duration", seconds));
        }
        let repair = self
            .repairs
            .get_mut(index)
            .ok_or_else(|| format!("there is no repair {index}"))?;
        repair.elapsed += seconds;
        if repair.seconds <= 0.0 || repair.elapsed >= repair.seconds {
            let done = self.repairs.remove(index);
            return Ok(Some(done));
        }
        Ok(None)
    }

    /// Advance every pending repair by `seconds`; the completed ones are
    /// returned in list order. This is the screen's clock over a headless run.
    pub fn advance_repairs(&mut self, seconds: f32) -> Result<Vec<Repair>, String> {
        if !(seconds.is_finite() && seconds >= 0.0) {
            return Err(format!("repair time {} is not a duration", seconds));
        }
        let mut done = Vec::new();
        let mut index = 0;
        while index < self.repairs.len() {
            self.repairs[index].elapsed += seconds;
            if self.repairs[index].seconds <= 0.0
                || self.repairs[index].elapsed >= self.repairs[index].seconds
            {
                done.push(self.repairs.remove(index));
            } else {
                index += 1;
            }
        }
        Ok(done)
    }

    /// Pay for and complete one pending repair. This is kept for callers that
    /// want the entry gone at once; the repairs screen uses
    /// [`Career::advance_repair`] so the bar is the interaction.
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

    /// Play one bracket round. This is the measured event flow
    /// (`FUN_0046ce50` → `FUN_0046c830` → `FUN_0046c400`):
    ///
    /// * the round pays `robot_points / 200 + 160 / rounds` to the player;
    /// * every AI participant of every match in the round gets `160 / rounds`
    ///   (`FUN_0046c400`'s bracket loop at `0x46c4f0`–`0x46c545`);
    /// * a win advances to the next round (`FUN_0046c830`'s
    ///   `FUN_004faaf0`, which increments `Team+0x110`), and the next round's
    ///   field is drawn afresh;
    /// * a loss ends the event (`FUN_0046ce50`'s `FUN_0046ca90(0,0)`), and
    ///   `FUN_0046ca90` draws and scores the remaining rounds for the AI teams
    ///   only;
    /// * the final win stores the trophy and the loss does not.
    pub fn play_round(
        &mut self,
        report: &MatchReport,
        entered_robot: usize,
    ) -> Result<RoundOutcome, String> {
        let (event, round, rounds, opponents) = {
            let bracket = self
                .bracket
                .as_ref()
                .ok_or_else(|| "no event bracket is in progress".to_string())?;
            if bracket.status != BracketStatus::Playing {
                return Err("the event bracket is already over".to_string());
            }
            (
                bracket.event,
                bracket.round,
                bracket.rounds,
                bracket.opponents.clone(),
            )
        };
        let entered = report
            .robots
            .get(entered_robot)
            .ok_or_else(|| format!("the report has no robot {entered_robot}"))?;
        let player_team = entered.team;
        let participation = self.policy.participation_points / rounds.max(1) as i32;
        let bonus = entered.points.max(0) / self.policy.score_divisor;
        let points = participation + bonus;

        // Every AI slot in every match of this round scores, exactly as the
        // binary's nested bracket loop does; repeats in a melee score twice.
        let mut league = self.player().opponent_points();
        {
            let bracket = self.bracket.as_ref().expect("checked above");
            for m in &bracket.current.matches {
                for entrant in &m.entrants {
                    if let Entrant::Ai(slot) = entrant {
                        if *slot < league.len() {
                            league[*slot] += participation;
                        }
                    }
                }
            }
        }

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
        let result = RoundResult {
            round,
            opponents: opponents.clone(),
            won,
            points,
            robot_points: entered.points,
        };
        let (finished, champion, next) = {
            let fastest = won && report.duration < self.player().fastest_win();
            if fastest {
                self.player_mut().set_fastest_win(report.duration);
            }
            let bracket = self.bracket.as_mut().expect("checked above");
            bracket.results.push(result.clone());
            if won {
                self.rounds_won += 1;
                bracket.round += 1;
                if bracket.round >= bracket.rounds {
                    bracket.status = BracketStatus::Champion;
                    bracket.champion = Some(Entrant::Player);
                    (true, bracket.champion, Vec::new())
                } else {
                    bracket.advance_field(true);
                    (false, None, bracket.opponents.clone())
                }
            } else {
                self.rounds_lost += 1;
                bracket.status = BracketStatus::Eliminated { round };
                bracket.simulate_remaining(participation, &mut league);
                (true, bracket.champion, Vec::new())
            }
        };
        self.player_mut().set_opponent_points(league);

        let repairs = self.repairs_from_report(report, entered_robot);
        for repair in repairs {
            if !self.repairs.iter().any(|old| {
                old.robot == repair.robot && old.component_index == repair.component_index
            }) {
                self.repairs.push(repair);
            }
        }

        let record = if finished {
            let trophy = matches!(champion, Some(Entrant::Player));
            Some(self.finish_event(trophy)?)
        } else {
            None
        };
        Ok(RoundOutcome {
            event,
            round,
            opponents,
            won,
            points,
            participation,
            bonus,
            robot_points: entered.points,
            event_finished: finished,
            champion,
            next,
            record,
        })
    }

    /// Complete the current event: fold its points into the season, count the
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
        self.bracket = None;
        Ok(record)
    }

    /// Start the next season: clear the league, the schedule and the history,
    /// bump `Team+0x98`, and draw a new nine-event calendar.
    pub fn next_season(&mut self, seed: u64) -> Result<(), String> {
        if !matches!(self.phase, SeasonPhase::Complete) {
            return Err("the season is not complete".to_string());
        }
        self.seed = seed;
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
        self.bracket = None;
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
            let out = ra2fmt::saves::prepare_write(game, path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            table
                .write_to(&out)
                .map_err(|e| format!("{}: {e}", out.display()))?;
        }

        let sidecar = sidecar_path_for(game, self.player_slot);
        ra2fmt::saves::write(game, &sidecar, self.write_sidecar())
            .map_err(|e| format!("{}: {e}", sidecar.display()))?;
        Ok(())
    }

    /// The sidecar text, in a fixed order for byte-stable saves.
    fn write_sidecar(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("ra2career {SIDECAR_VERSION}\n"));
        out.push_str(&format!("season {}\n", self.season()));
        out.push_str(&format!("seed {}\n", self.seed));
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
        match &self.bracket {
            Some(bracket) => {
                out.push_str("bracket 1\n");
                out.push_str(&format!("bracket-event {}\n", bracket.event));
                out.push_str(&format!(
                    "bracket-shape {} {} {}\n",
                    bracket.entrants, bracket.bots_per_match, bracket.rounds
                ));
                out.push_str(&format!(
                    "bracket-state {} {} {} {}\n",
                    bracket.round,
                    bracket.status.name(),
                    bracket.seed,
                    bracket
                        .champion
                        .map_or_else(|| "none".to_string(), |e| e.code().to_string())
                ));
                out.push_str(&format!("bracket-opponents {}\n", bracket.opponents.len()));
                if !bracket.opponents.is_empty() {
                    out.push_str("bracket-opponent");
                    for entrant in &bracket.opponents {
                        out.push_str(&format!(" {}", entrant.code()));
                    }
                    out.push('\n');
                }
                out.push_str(&format!("bracket-results {}\n", bracket.results.len()));
                for result in &bracket.results {
                    out.push_str(&format!(
                        "bracket-result {} {} {} {} {}\n",
                        result.round,
                        u8::from(result.won),
                        result.points,
                        result.robot_points,
                        result.opponents.len()
                    ));
                    out.push_str("bracket-result-opponents");
                    for entrant in &result.opponents {
                        out.push_str(&format!(" {}", entrant.code()));
                    }
                    out.push('\n');
                }
            }
            None => out.push_str("bracket 0\n"),
        }
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
                "repair {}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                escape(&repair.robot),
                escape(&repair.component),
                repair.cost,
                repair.seconds,
                repair.elapsed,
                repair.component_index,
                escape(&repair.component_class),
                repair.hp,
                repair.total_hp,
                u8::from(repair.detached),
                u8::from(repair.fractured)
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
        let mut bracket_active = false;
        let mut bracket_event = None;
        let mut bracket_shape = None;
        let mut bracket_state = None;
        let mut bracket_opponents = None;
        let mut bracket_results: Option<Vec<RoundResult>> = None;
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
                "seed" => self.seed = rest.trim().parse().map_err(|_| "bad seed")?,
                "bracket" => bracket_active = rest.trim() == "1",
                "bracket-event" => {
                    bracket_event = Some(rest.trim().parse().map_err(|_| "bad bracket event")?)
                }
                "bracket-shape" => {
                    let mut fields = rest.split_whitespace();
                    let entrants = fields
                        .next()
                        .and_then(|v| v.parse().ok())
                        .ok_or("bad bracket entrants")?;
                    let bots = fields
                        .next()
                        .and_then(|v| v.parse().ok())
                        .ok_or("bad bracket bots")?;
                    let rounds = fields
                        .next()
                        .and_then(|v| v.parse().ok())
                        .ok_or("bad bracket rounds")?;
                    bracket_shape = Some((entrants, bots, rounds));
                }
                "bracket-state" => {
                    let mut fields = rest.split_whitespace();
                    let round: usize = fields
                        .next()
                        .and_then(|v| v.parse().ok())
                        .ok_or("bad bracket round")?;
                    let status = fields.next().ok_or("bad bracket status")?.to_string();
                    let seed: u64 = fields
                        .next()
                        .and_then(|v| v.parse().ok())
                        .ok_or("bad bracket seed")?;
                    let champion = match fields.next().ok_or("bad bracket champion")? {
                        "none" => None,
                        code => Some(
                            Entrant::from_code(code.parse().map_err(|_| "bad champion")?)
                                .ok_or("bad champion slot")?,
                        ),
                    };
                    bracket_state = Some((round, status, seed, champion));
                }
                "bracket-opponents" => {
                    let count: usize = rest.parse().map_err(|_| "bad bracket opponent count")?;
                    let line = lines.next().ok_or("the bracket opponents end early")?;
                    bracket_opponents = Some(parse_entrants_line(line, count)?);
                }
                "bracket-results" => {
                    let count: usize = rest.parse().map_err(|_| "bad bracket result count")?;
                    let mut list = Vec::with_capacity(count);
                    for _ in 0..count {
                        let line = lines.next().ok_or("the bracket results end early")?;
                        let body = line
                            .strip_prefix("bracket-result ")
                            .ok_or_else(|| format!("bad bracket result line {line:?}"))?;
                        let mut fields = body.split_whitespace();
                        let round = fields
                            .next()
                            .and_then(|v| v.parse().ok())
                            .ok_or("bad bracket result round")?;
                        let won = fields
                            .next()
                            .and_then(|v| v.parse::<u8>().ok())
                            .ok_or("bad bracket result won")?
                            != 0;
                        let points = fields
                            .next()
                            .and_then(|v| v.parse().ok())
                            .ok_or("bad bracket result points")?;
                        let robot_points = fields
                            .next()
                            .and_then(|v| v.parse().ok())
                            .ok_or("bad bracket result robot points")?;
                        let count: usize = fields
                            .next()
                            .and_then(|v| v.parse().ok())
                            .ok_or("bad bracket result opponents")?;
                        let line = lines
                            .next()
                            .ok_or("the bracket result opponents end early")?;
                        let opponents = parse_entrants_line(line, count)?;
                        list.push(RoundResult {
                            round,
                            opponents,
                            won,
                            points,
                            robot_points,
                        });
                    }
                    bracket_results = Some(list);
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
                        // A sidecar written before the clock existed has no
                        // fifth field; it starts at zero. The component fields
                        // followed in a later version and default to a
                        // robot-named row so old saves still load.
                        let elapsed = fields
                            .next()
                            .and_then(|v| v.parse::<f32>().ok())
                            .unwrap_or(0.0);
                        let component_index = fields
                            .next()
                            .and_then(|v| v.parse::<usize>().ok())
                            .unwrap_or(0);
                        let component_class = fields
                            .next()
                            .map(unescape)
                            .unwrap_or_else(|| "robot".to_string());
                        let hp = fields
                            .next()
                            .and_then(|v| v.parse::<f32>().ok())
                            .unwrap_or(0.0);
                        let total_hp = fields
                            .next()
                            .and_then(|v| v.parse::<f32>().ok())
                            .unwrap_or(0.0);
                        let detached = fields.next().map(|v| v.trim() == "1").unwrap_or(false);
                        let fractured = fields.next().map(|v| v.trim() == "1").unwrap_or(false);
                        list.push(Repair {
                            robot,
                            component,
                            component_index,
                            component_class,
                            hp,
                            total_hp,
                            detached,
                            fractured,
                            cost,
                            seconds,
                            elapsed,
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
        if bracket_active {
            if let (
                Some(event),
                Some((entrants, bots, rounds)),
                Some((round, status, seed, champion)),
            ) = (bracket_event, bracket_shape, bracket_state)
            {
                let data = self
                    .events
                    .get(event)
                    .ok_or_else(|| format!("the bracket's event {event} is not in Events.txt"))?;
                let mut bracket = EventBracket::new(event, data, seed)?;
                bracket.entrants = entrants;
                bracket.bots_per_match = bots;
                bracket.rounds = rounds;
                bracket.replay_to(round);
                bracket.status = match status.as_str() {
                    "playing" => BracketStatus::Playing,
                    "eliminated" => BracketStatus::Eliminated { round },
                    "champion" => BracketStatus::Champion,
                    other => return Err(format!("bad bracket status {other:?}")),
                };
                bracket.champion = champion;
                bracket.results = bracket_results.unwrap_or_default();
                if round >= rounds {
                    bracket.current = BracketRound {
                        matches: Vec::new(),
                    };
                    bracket.opponents.clear();
                }
                if let Some(opponents) = bracket_opponents {
                    bracket.restore_opponents(&opponents);
                }
                self.bracket = Some(bracket);
            }
        } else {
            self.bracket = None;
        }
        Ok(())
    }
}

/// Read a line of the form `<key> <code> <code> …`, with `count` codes.
fn parse_entrants_line(line: &str, count: usize) -> Result<Vec<Entrant>, String> {
    let (_, body) = line
        .split_once(' ')
        .ok_or_else(|| format!("bad entrant line {line:?}"))?;
    let mut entrants = Vec::with_capacity(count);
    for token in body.split_whitespace() {
        if entrants.len() == count {
            break;
        }
        let code: i32 = token.parse().map_err(|_| "bad entrant code")?;
        entrants.push(Entrant::from_code(code).ok_or("bad entrant slot")?);
    }
    if entrants.len() != count {
        return Err(format!(
            "the entrant line has {} codes, expected {count}",
            entrants.len()
        ));
    }
    Ok(entrants)
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
    ra2fmt::saves::resolve(game, "Teams/Teams.txt")
}

/// `<game>/AI/Teams.txt`.
pub fn ai_table_path(game: &Path) -> PathBuf {
    ra2fmt::saves::resolve(game, "AI/Teams.txt")
}

/// `<game>/Events.txt`.
pub fn events_path(game: &Path) -> PathBuf {
    game.join("Events.txt")
}

/// `<game>/Teams/career.dat` — the documented sidecar.
pub fn sidecar_path(game: &Path) -> PathBuf {
    ra2fmt::saves::resolve(game, Path::new("Teams").join(SIDECAR_NAME))
}

/// The sidecar of the team record at `slot`: `Teams/career.dat` for slot 0
/// (the shipped team), `Teams/career<slot>.dat` for the others. Each team
/// keeps its own sidecar-only state, as each keeps its own record.
pub fn sidecar_path_for(game: &Path, slot: usize) -> PathBuf {
    if slot == 0 {
        sidecar_path(game)
    } else {
        ra2fmt::saves::resolve(game, Path::new("Teams").join(format!("career{slot}.dat")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::r#match::{ComponentDamage, MatchReport, MatchState, RobotReport};
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
            ra2fmt::saves::bind(&path, &path);
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
        ra2fmt::saves::isolate_for_tests();
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
            bot_preview: None,
            points,
            damage_dealt: 0.0,
            damage_taken: damage,
            battery: 1.0,
            air: 1.0,
            armor: 1.0,
            dot: 0,
            defeated,
            eliminated: defeated,
            winner: false,
            immobilized: false,
            owner: None,
            // A synthetic report: one component named after the robot, so a
            // `damage` figure still becomes a component-level repair row.
            components: if damage > 0.0 || defeated {
                vec![ComponentDamage {
                    index: 1,
                    name: name.to_string(),
                    class: "TestPart".to_string(),
                    base: String::new(),
                    hp: (100.0 - damage).max(0.0),
                    total_hp: 100.0,
                    detached: defeated || damage >= 100.0,
                    fractured: defeated && damage >= 100.0,
                }]
            } else {
                Vec::new()
            },
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
            duration_limit: None,
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
            .map(|i| {
                let (tiers, bots) = if i % 2 == 0 { (4, 2) } else { (2, 4) };
                Event {
                    id: i as i32,
                    name: format!("Event {i}"),
                    description: String::new(),
                    tiers: (0..tiers)
                        .map(|t| {
                            (
                                format!("Arenas\\event_results\\prizes\\T{t}.gmf"),
                                format!("tier {t}"),
                            )
                        })
                        .collect(),
                    trophy: (
                        format!("Arenas\\event_results\\prizes\\Trophy{i}\\Trophy{i}.gmf"),
                        format!("won event {i}"),
                        "UI\\teamhq\\events\\trophies\\Trophy.tga".to_string(),
                    ),
                    art: String::new(),
                    arena: ("Combat Arena".to_string(), String::new()),
                    settings: vec![bots, 1, 1],
                }
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
        // Each record starts with its id line (`FUN_00469350` reads `+4`
        // first) and ends with three numbers.
        let mut text = String::new();
        for i in 0..25 {
            let (tiers, bots) = if i % 2 == 0 { (4, 2) } else { (2, 4) };
            text.push_str(&format!("{i}\nEvent {i}\ndescription\n{tiers}\n"));
            for t in 0..tiers {
                text.push_str(&format!(
                    "Arenas\\event_results\\prizes\\T{t}.gmf\ntier {t}\n"
                ));
            }
            text.push_str(&format!(
                "Arenas\\event_results\\prizes\\Trophy{i}\\Trophy{i}.gmf\n\
                 won event {i}\nUI\\teamhq\\events\\trophies\\Trophy.tga\n\
                 art.bmp\nCombat Arena\npreview.bmp\n{bots}\n1\n1\n"
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
        // This test is the legacy report/save path, not the bracket: clear the
        // bracket so the divisor is the report's robot count (160/2), as the
        // pre-bracket model measured.
        career.bracket = None;

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
        assert_eq!(career.repairs[0].seconds, 1_800.0);

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

    /// A match that damages a robot leaves one repair entry; the screen's
    /// clock (not a purchase) completes it, and the empty list is what the
    /// career save reads back.
    #[test]
    fn a_damaging_match_leaves_a_repair_the_clock_completes_and_saves() {
        let Some(dir) = copy_game("repairs") else {
            eprintln!("skipping repair test: no local/game (set RA2_GAME to run it)");
            return;
        };
        let mut career = Career::new(dir.path(), 7).unwrap();
        let event = career.schedule[0];
        career.enter_event(event).unwrap();
        let player = robot(0, 0, "Damaged Bot", 100, 42.0, false);
        let opponent = robot(1, 1, "Red Zone", 200, 0.0, true);
        career
            .award_match(&report("DEATHMATCH", vec![player, opponent], vec![0]), 0)
            .unwrap();
        assert_eq!(career.repairs.len(), 1);
        assert_eq!(career.repairs[0].robot, "Damaged Bot");
        assert_eq!(career.repairs[0].component, "Damaged Bot");
        assert_eq!(career.repairs[0].component_index, 1);
        assert_eq!(career.repairs[0].hp, 58.0);
        assert_eq!(career.repairs[0].total_hp, 100.0);
        assert_eq!(career.repairs[0].cost, 0, "the original charges nothing");
        assert_eq!(career.repairs[0].elapsed, 0.0);
        assert_eq!(career.repair_progress(0), Some(0.0));

        // 12 of the repair's 30 minutes served: the bar is 40% full.
        assert_eq!(career.advance_repair(0, 720.0).unwrap(), None);
        let progress = career.repair_progress(0).unwrap();
        assert!((progress - 0.4).abs() < 1e-6, "{progress}");

        career.save(dir.path()).unwrap();
        let mut loaded = Career::load(dir.path()).unwrap();
        assert_eq!(loaded.repairs.len(), 1);
        assert!((loaded.repairs[0].elapsed - 720.0).abs() < 1e-6);
        assert_eq!(loaded.repairs[0].component_index, 1);
        assert_eq!(loaded.repairs[0].component_class, "TestPart");

        // The clock that fills the bar completes the entry; nothing is charged.
        let done = loaded.advance_repair(0, 1_080.0).unwrap().unwrap();
        assert_eq!(done.robot, "Damaged Bot");
        assert_eq!(done.component, "Damaged Bot");
        assert!(loaded.repairs.is_empty(), "the damage is cleared");
        assert_eq!(loaded.money, 0, "a repair does not charge");
        loaded.save(dir.path()).unwrap();
        let reread = Career::load(dir.path()).unwrap();
        assert!(
            reread.repairs.is_empty(),
            "the repaired state is what the save reads back"
        );
    }

    /// Two damaged components on one robot leave two rows; the clock completes
    /// them one at a time, the detached component keeps its name, and a save
    /// in the middle reloads the same component state.
    #[test]
    fn component_damage_leaves_one_row_per_component_and_repairs_in_order() {
        let Some(dir) = copy_game("components") else {
            eprintln!("skipping component repair test: no local/game");
            return;
        };
        let mut career = Career::new(dir.path(), 7).unwrap();
        let event = career.schedule[0];
        career.enter_event(event).unwrap();
        let mut player = robot(0, 0, "Damaged Bot", 100, 42.0, false);
        player.components = vec![
            ComponentDamage {
                index: 0,
                name: "Chassis".to_string(),
                class: "Chassis".to_string(),
                base: "Chassis".to_string(),
                hp: 210.0,
                total_hp: 210.0,
                detached: false,
                fractured: false,
            },
            ComponentDamage {
                index: 2,
                name: "Bent Axle".to_string(),
                class: "Axle".to_string(),
                base: "Axle".to_string(),
                hp: 40.0,
                total_hp: 100.0,
                detached: false,
                fractured: false,
            },
            ComponentDamage {
                index: 5,
                name: "Torn Spinner".to_string(),
                class: "SpinMotor".to_string(),
                base: "SpinMotor".to_string(),
                hp: 0.0,
                total_hp: 80.0,
                detached: true,
                fractured: true,
            },
        ];
        let opponent = robot(1, 1, "Red Zone", 200, 0.0, true);
        career
            .award_match(&report("DEATHMATCH", vec![player, opponent], vec![0]), 0)
            .unwrap();
        assert_eq!(career.repairs.len(), 2, "one row per damaged component");
        assert_eq!(career.repairs[0].robot, "Damaged Bot");
        assert_eq!(career.repairs[0].component, "Bent Axle");
        assert_eq!(career.repairs[0].component_index, 2);
        assert_eq!(career.repairs[0].component_class, "Axle");
        assert_eq!(career.repairs[0].hp, 40.0);
        assert_eq!(career.repairs[0].total_hp, 100.0);
        assert_eq!(career.repairs[0].hp_percent(), 40);
        assert!(!career.repairs[0].detached);
        assert_eq!(career.repairs[1].component, "Torn Spinner");
        assert_eq!(career.repairs[1].component_index, 5);
        assert!(career.repairs[1].detached);
        assert!(career.repairs[1].fractured);

        // Half the clock on the first row leaves the second untouched, and the
        // save round-trips both component states.
        assert_eq!(career.advance_repair(0, 900.0).unwrap(), None);
        career.save(dir.path()).unwrap();
        let mut loaded = Career::load(dir.path()).unwrap();
        assert_eq!(loaded.repairs.len(), 2);
        assert_eq!(loaded.repairs[0].component, "Bent Axle");
        assert_eq!(loaded.repairs[0].component_index, 2);
        assert!((loaded.repairs[0].elapsed - 900.0).abs() < 1e-6);
        assert_eq!(loaded.repairs[1].component, "Torn Spinner");
        assert!(loaded.repairs[1].detached);
        assert!(loaded.repairs[1].fractured);

        // The first row completes alone; the detached row stays pending.
        let done = loaded.advance_repair(0, 900.0).unwrap().unwrap();
        assert_eq!(done.component, "Bent Axle");
        assert_eq!(loaded.repairs.len(), 1);
        assert_eq!(loaded.repairs[0].component, "Torn Spinner");

        let done = loaded.advance_repair(0, 1_800.0).unwrap().unwrap();
        assert_eq!(done.component, "Torn Spinner");
        assert!(done.detached);
        assert!(loaded.repairs.is_empty());

        loaded.save(dir.path()).unwrap();
        let reread = Career::load(dir.path()).unwrap();
        assert!(reread.repairs.is_empty(), "the cleared damage is saved");
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

    /// The bracket the original drew live (docs/36 section 9.23): the engine
    /// state set to 4037340146 before "Enter Event!" on the Blade Tournament gave
    /// 46 draws (15 + 3 accepted, 28 repeats) and the 16 teams the screen showed;
    /// after round 1 the three winner draws (the states 3422776833, 1052791974,
    /// 4027764455 followed) picked the final's opponents High Voltage (8),
    /// Prehistoric (1) and The Scrappers (14).
    #[test]
    fn the_engine_lcg_draws_the_bracket_the_original_drew_live() {
        let Some(game) = game_dir() else { return };
        let text = ra2fmt::event::read(&game.join("Events.txt")).unwrap();
        let events = ra2fmt::event::parse(&text).unwrap();
        let blade = events
            .iter()
            .position(|e| e.name.to_ascii_lowercase().contains("blade"))
            .expect("the Blade Tournament");
        let mut bracket = EventBracket::new(blade, &events[blade], 4_037_340_146).unwrap();
        let field = |bracket: &EventBracket| -> Vec<Vec<Entrant>> {
            bracket
                .current
                .matches
                .iter()
                .map(|m| m.entrants.clone())
                .collect()
        };
        let ai =
            |teams: [usize; 4]| -> Vec<Entrant> { teams.iter().map(|t| Entrant::Ai(*t)).collect() };
        let mut want = vec![
            vec![
                Entrant::Player,
                Entrant::Ai(4),
                Entrant::Ai(13),
                Entrant::Ai(3),
            ],
            ai([9, 2, 8, 10]),
            ai([6, 1, 7, 12]),
            ai([0, 5, 11, 14]),
        ];
        assert_eq!(field(&bracket), want);
        assert_eq!(bracket.lcg_state(), 3_012_094_436, "46 draws, as logged");
        // The winners: the state before the logged draws is the inverse step of
        // the first one.
        bracket.lcg = Lcg(210_106_344);
        bracket.advance_field(true);
        want = vec![vec![
            Entrant::Player,
            Entrant::Ai(8),
            Entrant::Ai(1),
            Entrant::Ai(14),
        ]];
        assert_eq!(field(&bracket), want);
        assert_eq!(bracket.lcg_state(), 4_027_764_455);
    }

    /// The two-bot variant, live: the Sawblade Elite bracket from the same state
    /// took 59 draws (round 1's 15 teams, then 7, 3 and 1 more for the later
    /// rounds, 33 repeats); round 1's opponent was Black Storm (4); after the win
    /// the seven winner draws (the states 3993866673 ... 15792307) gave round 2's
    /// opponent Team Dragon (3), as the results screen showed.
    #[test]
    fn the_two_bot_bracket_and_its_winners_match_the_live_draws() {
        let Some(game) = game_dir() else { return };
        let text = ra2fmt::event::read(&game.join("Events.txt")).unwrap();
        let events = ra2fmt::event::parse(&text).unwrap();
        let sawblade = events
            .iter()
            .position(|e| e.name.to_ascii_lowercase().contains("sawblade"))
            .expect("Sawblade Elite");
        let mut bracket = EventBracket::new(sawblade, &events[sawblade], 4_037_340_146).unwrap();
        assert_eq!((bracket.rounds, bracket.bots_per_match), (4, 2));
        let first: Vec<_> = bracket
            .current
            .matches
            .iter()
            .map(|m| m.entrants.clone())
            .collect();
        assert_eq!(first[0], vec![Entrant::Player, Entrant::Ai(4)]);
        assert_eq!(first[1], vec![Entrant::Ai(13), Entrant::Ai(3)]);
        assert_eq!(first[7], vec![Entrant::Ai(11), Entrant::Ai(14)]);
        assert_eq!(bracket.lcg_state(), 3_572_834_689, "59 draws, as logged");
        bracket.lcg = Lcg(863_542_360);
        bracket.advance_field(true);
        assert_eq!(bracket.opponents(), &[Entrant::Ai(3)]);
        let round2: Vec<_> = bracket
            .current
            .matches
            .iter()
            .map(|m| m.entrants.clone())
            .collect();
        assert_eq!(round2[1], vec![Entrant::Ai(9), Entrant::Ai(10)]);
        assert_eq!(round2[3], vec![Entrant::Ai(0), Entrant::Ai(11)]);
        assert_eq!(bracket.lcg_state(), 15_792_307);
    }

    /// A session's bracket follows from its engine state: the same state gives the
    /// same field, the next event's bracket another one, and the saved state
    /// carries on.
    #[test]
    fn a_session_state_gives_the_same_brackets_again() {
        let draw = |seed: u64| {
            let mut career = synthetic_career();
            career.seed = seed;
            let first = career.schedule[0];
            career.enter_event(first).unwrap();
            let a = career.bracket().unwrap().current.clone();
            let state = career.seed;
            (a, state)
        };
        let (a, state) = draw(1234);
        assert_eq!(draw(1234).0, a, "same state, same bracket");
        assert_ne!(draw(1235).0, a);
        assert_ne!(state, 1234, "the draws moved the session state on");
    }

    /// The season ends after nine events and the next one starts clean.
    #[test]
    fn nine_events_complete_the_season_and_the_next_one_resets_it() {
        let mut career = synthetic_career();
        career.seed = 99;
        let mut expected = 0;
        let mut rounds_total = 0u32;
        for n in 0..9 {
            let event = career.schedule[n];
            career.enter_event(event).unwrap();
            let rounds = career.bracket().unwrap().rounds as u32;
            rounds_total += rounds;
            for round in 0..rounds {
                let outcome = career
                    .play_round(
                        &report(
                            "DEATHMATCH",
                            vec![
                                robot(0, 0, "Test Team", 2000, 0.0, false),
                                robot(1, 1, "AI", 0, 0.0, true),
                            ],
                            vec![0],
                        ),
                        0,
                    )
                    .unwrap();
                assert!(outcome.won);
                assert_eq!(outcome.round, round as usize);
                expected += outcome.points;
                let finished = round + 1 == rounds;
                assert_eq!(outcome.event_finished, finished);
            }
            assert!(!matches!(career.phase, SeasonPhase::InEvent { .. }));
        }
        assert_eq!(career.phase, SeasonPhase::Complete);
        assert_eq!(career.history.len(), 9);
        assert_eq!(career.player().total_events(), 9);
        assert_eq!(career.player().events_won(), 9);
        assert_eq!(career.player().wins(), rounds_total as i32);
        // The event pays `rounds * (160 / rounds + 2000 / 200)`; over the
        // season that is `9 * 160 + 10 * rounds`.
        assert_eq!(expected, 9 * 160 + 10 * rounds_total as i32);
        assert_eq!(career.player().season_points(), expected);
        assert_eq!(career.rank(), 1, "every event was a final win");
        assert_eq!(career.prizes.len(), 10, "nine trophies and the sawblade");
        assert_eq!(career.prizes.last().unwrap().event, usize::MAX);

        career.next_season(11).unwrap();
        assert_eq!(career.phase, SeasonPhase::Schedule);
        assert_eq!(career.season(), 1);
        assert_eq!(career.seed, 11);
        assert_eq!(career.player().season_points(), 0);
        assert_eq!(career.history.len(), 0);
        assert!(career.bracket.is_none());
        assert_eq!(career.schedule.len(), 9);
        assert_eq!(career.player().total_events(), 9, "lifetime count is kept");
    }

    /// The shipped `Events.txt` shapes are the measured bracket shapes: the
    /// tier count is the round count, the first setting is the bots per match,
    /// and the two multiply to the 16 entrants.
    #[test]
    fn every_shipped_event_is_a_sixteen_entrant_bracket() {
        let Some(game) = game_dir() else { return };
        let text = ra2fmt::event::read(&game.join("Events.txt")).unwrap();
        let events = ra2fmt::event::parse(&text).unwrap();
        for event in &events {
            let (rounds, bots) = event_shape(event).unwrap();
            assert_eq!(rounds, event.tiers.len(), "{}", event.name);
            assert_eq!(
                Some(bots),
                event.settings.first().map(|b| *b as usize),
                "{}",
                event.name
            );
            assert_eq!(bots.pow(rounds as u32), BRACKET_ENTRANTS, "{}", event.name);
            assert!(rounds == 2 || rounds == 4);
        }
    }

    /// A loss ends the event but not the season: the history records it, no
    /// trophy is stored, the bracket is gone and the schedule is still open.
    #[test]
    fn a_loss_eliminates_from_the_event_but_not_from_the_season() {
        let mut career = synthetic_career();
        career.seed = 5;
        let event = career.schedule[0];
        career.enter_event(event).unwrap();
        let rounds = career.bracket().unwrap().rounds;
        let outcome = career
            .play_round(
                &report(
                    "DEATHMATCH",
                    vec![
                        robot(0, 0, "Test Team", 400, 40.0, false),
                        robot(1, 1, "AI", 0, 0.0, true),
                    ],
                    vec![1],
                ),
                0,
            )
            .unwrap();
        assert!(!outcome.won);
        assert!(outcome.event_finished);
        assert!(matches!(outcome.champion, Some(Entrant::Ai(_))));
        assert_eq!(outcome.points, 160 / rounds as i32 + 2);
        assert!(career.bracket.is_none());
        assert_eq!(career.phase, SeasonPhase::Schedule);
        assert_eq!(career.history.len(), 1);
        let record = &career.history[0];
        assert_eq!(record.event, event);
        assert!(!record.won);
        assert_eq!(record.rounds_won, 0);
        assert_eq!(record.rounds_lost, 1);
        assert_eq!(record.points, outcome.points);
        assert!(career.prizes.is_empty(), "no trophy for a loss");
        assert_eq!(career.player().season_points(), outcome.points);
        // The AI field still scored its participation, so the league line is
        // not all zero after one round.
        assert!(career.player().opponent_points().iter().any(|p| *p > 0));
    }

    /// The same seed draws the same bracket, round after round.
    #[test]
    fn replaying_the_same_seed_reproduces_the_bracket() {
        let mut first = synthetic_career();
        first.seed = 0x5eed_1234;
        let mut second = synthetic_career();
        second.seed = 0x5eed_1234;
        let event = first.schedule[1];
        first.enter_event(event).unwrap();
        second.enter_event(event).unwrap();
        assert_eq!(first.bracket, second.bracket);
        let rounds = first.bracket().unwrap().rounds;
        for _ in 0..rounds {
            let win = report(
                "DEATHMATCH",
                vec![
                    robot(0, 0, "Test Team", 1000, 0.0, false),
                    robot(1, 1, "AI", 0, 0.0, true),
                ],
                vec![0],
            );
            let a = first.play_round(&win, 0).unwrap();
            let b = second.play_round(&win, 0).unwrap();
            assert_eq!(a, b);
            assert_eq!(first.bracket, second.bracket);
        }
    }

    /// Losing the final still crowns an AI and stores no trophy.
    #[test]
    fn a_final_loss_crowns_an_ai_and_stores_no_trophy() {
        let mut career = synthetic_career();
        career.seed = 21;
        let event = career.schedule[1]; // the synthetic schedule alternates shapes
        assert_eq!(career.events[event].tiers.len(), 2);
        career.enter_event(event).unwrap();
        let win = report(
            "DEATHMATCH",
            vec![
                robot(0, 0, "Test Team", 200, 0.0, false),
                robot(1, 1, "AI", 0, 0.0, true),
            ],
            vec![0],
        );
        let loss = report(
            "DEATHMATCH",
            vec![
                robot(0, 0, "Test Team", 200, 0.0, false),
                robot(1, 1, "AI", 0, 0.0, true),
            ],
            vec![1],
        );
        let first = career.play_round(&win, 0).unwrap();
        assert!(first.won && !first.event_finished);
        let final_loss = career.play_round(&loss, 0).unwrap();
        assert!(final_loss.event_finished);
        assert!(!final_loss.won);
        assert!(matches!(final_loss.champion, Some(Entrant::Ai(_))));
        let record = career.history.last().unwrap();
        assert!(!record.won);
        assert_eq!(record.rounds_won, 1);
        assert_eq!(record.rounds_lost, 1);
        assert!(career.prizes.is_empty());
    }

    /// A save in the middle of a season restores the bracket, the current
    /// round and the next opponents.
    #[test]
    fn save_reload_mid_season_restores_the_bracket_and_opponents() {
        let dir = TestDir::new("bracket-save");
        write_synthetic_events(dir.path());
        let mut career = synthetic_career();
        career.seed = 77;
        career.save(dir.path()).unwrap();
        let event = career.schedule[0];
        career.enter_event(event).unwrap();
        let _ = career
            .play_round(
                &report(
                    "DEATHMATCH",
                    vec![
                        robot(0, 0, "Test Team", 800, 0.0, false),
                        robot(1, 1, "AI", 0, 0.0, true),
                    ],
                    vec![0],
                ),
                0,
            )
            .unwrap();
        let expected_bracket = career.bracket.clone().expect("a bracket");
        let expected_points = career.player().event_points();
        assert!(expected_bracket.round > 0 || expected_bracket.rounds == 1);
        let expected_opponents = career.next_opponents().to_vec();
        career.save(dir.path()).unwrap();

        let loaded = Career::load(dir.path()).unwrap();
        // The session state moved on with the bracket's draws; the bracket kept
        // the state it was drawn from.
        assert_eq!(loaded.seed, career.seed);
        assert_ne!(loaded.seed, 77);
        assert_eq!(loaded.bracket().unwrap().seed, 77);
        assert_eq!(loaded.phase, SeasonPhase::InEvent { event });
        assert_eq!(loaded.bracket.as_ref(), Some(&expected_bracket));
        assert_eq!(loaded.next_opponents(), expected_opponents.as_slice());
        assert_eq!(loaded.player().event_points(), expected_points);
        assert_eq!(loaded.bracket().unwrap().round, expected_bracket.round);
    }

    /// A multi-round event writes the season and event points the career
    /// holds after every round, not only at the end.
    #[test]
    fn a_multi_round_event_writes_the_points_the_career_holds_after_every_round() {
        let dir = TestDir::new("bracket-save-points");
        write_synthetic_events(dir.path());
        let mut career = synthetic_career();
        career.seed = 3;
        career.save(dir.path()).unwrap();
        let event = career.schedule[0];
        career.enter_event(event).unwrap();
        let rounds = career.bracket().unwrap().rounds;
        assert_eq!(rounds, 4, "event 0 is a four-round event");
        let win = report(
            "DEATHMATCH",
            vec![
                robot(0, 0, "Test Team", 2000, 0.0, false),
                robot(1, 1, "AI", 0, 0.0, true),
            ],
            vec![0],
        );
        for round in 0..rounds {
            let outcome = career.play_round(&win, 0).unwrap();
            assert!(outcome.won);
            career.save(dir.path()).unwrap();
            let table = team::read_table(&player_table_path(dir.path())).unwrap();
            let disk = &table.teams[career.player_slot];
            println!(
                "round {}/{}: career season {}, event {}; disk season {}, event {}",
                round + 1,
                rounds,
                career.player().season_points(),
                career.player().event_points(),
                disk.season_points(),
                disk.event_points()
            );
            assert_eq!(
                disk.season_points(),
                career.player().season_points(),
                "season points after round {}",
                round + 1
            );
            assert_eq!(
                disk.event_points(),
                career.player().event_points(),
                "event points after round {}",
                round + 1
            );
        }
        assert!(career.bracket.is_none());
        assert_eq!(career.history.len(), 1);
        // Four rounds at 160/4 = 40 plus 2000/200 = 10 each.
        assert_eq!(career.player().season_points(), 4 * 50);
        let table = team::read_table(&player_table_path(dir.path())).unwrap();
        assert_eq!(
            table.teams[career.player_slot].season_points(),
            career.player().season_points()
        );
    }

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
    /// A round's field has no repeated team (`FUN_004695a0` redraws a team
    /// already in the round's list).
    #[test]
    fn a_rounds_field_has_no_repeated_team() {
        for (event, data) in synthetic_events(4).iter().enumerate() {
            for seed in 0..8u64 {
                let bracket = EventBracket::new(event, data, seed).unwrap();
                let mut teams: Vec<usize> = bracket
                    .current
                    .matches
                    .iter()
                    .flat_map(|m| m.entrants.iter())
                    .filter_map(|e| match e {
                        Entrant::Ai(slot) => Some(*slot),
                        Entrant::Player => None,
                    })
                    .collect();
                assert_eq!(teams.len(), BRACKET_ENTRANTS - 1);
                teams.sort_unstable();
                teams.dedup();
                assert_eq!(
                    teams.len(),
                    BRACKET_ENTRANTS - 1,
                    "event {event} seed {seed}"
                );
            }
        }
    }
}

/// The box text `FUN_004ff0c0` shows when the robot to scrap is the one entered in an event.
pub const SCRAP_IN_EVENT: &str = "This robot is currently entered in an event. In order to scrap it, you must either complete or forfeit the current event.";

/// `FUN_00500120`'s draw: `want` distinct events out of `n`, from LCG state `state`.
pub fn draw_schedule(n: usize, want: usize, mut state: u32) -> Vec<usize> {
    let want = want.min(n);
    let mut ids = Vec::with_capacity(want);
    while ids.len() < want {
        state = state.wrapping_mul(0x41c6_4e6d).wrapping_add(0x3039);
        let r = f64::from(state & 0x7fff_ffff) / 2_147_483_648.0;
        let id = ((r * n as f64) as usize).min(n - 1);
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

#[cfg(test)]
mod schedule_tests {
    use super::*;

    /// The recovered draw reproduces the original's captured season-1 schedule.
    #[test]
    fn the_oracle_state_draws_the_captured_schedule() {
        assert_eq!(
            draw_schedule(25, 9, ORACLE_SCHEDULE_STATE),
            vec![2, 6, 12, 9, 13, 15, 5, 22, 20]
        );
    }
}
