//! The game's team tables: `Teams/Teams.txt` and `AI/Teams.txt`.
//!
//! A team record is the game's own text form, written by `Team::save`
//! (`FUN_004fb010`) and read back by `Team::load` (`FUN_004fabf0`). The
//! player's file opens with a single `0` head line and then sixteen 23-line
//! records; the AI file has fifteen 24-line records, each prefixed with
//! `index N`. The rest of a record is the same in both:
//!
//! ```text
//! index 0                      (AI/Teams.txt only; Teams.txt has one file head)
//! Red Zone                     name
//! "Do NOT enter the Red Zone!" motto
//! AI\red_zone.bmp              logo
//! Robots: 0 1 2
//! Robot In Event: -1
//! ...                          the rest is the team's state, not identity
//! ```
//!
//! After `Robot In Event:` the record has fourteen state lines, the
//! fifteen-int league line (`Team+0x5c`..`+0x94`), and three closing lines.
//!
//! The typed accessors below name the fields the career recon recovered
//! (`docs/16-career.md`): the league line is the **points of the fifteen AI
//! teams**, the first closing line is the points earned in the current event,
//! the second is the player's season points, the third accumulates defeated
//! robots, and the state lines carry the season index, ranks and history
//! counters. The `+0xb8` float is **not** money: the team-HQ screen shows it
//! as the fastest win with `10000` as its `N/A` sentinel (`0x6f0cec`).
//!
//! The prize line (`+0x100`'s vector, written as a count and paths on one
//! line) and the event-schedule line (`+0xe4`'s vector, written as a count and
//! event ids on one line) are kept verbatim as [`TeamValue::Text`], so a career
//! file written by this crate round-trips even though the shipped files have
//! both empty.
//!
//! Empty player slots are real records: their name and logo lines are blank and
//! [`parse`] skips them, while [`parse_table`] keeps them so the file can be
//! written back byte-for-byte. The player's single `0` head line is kept on
//! [`TeamTable::prefix`].

use crate::bot::LineEnding;

/// A team as the shell shows it, plus the record's state.
#[derive(Clone, Debug, PartialEq)]
pub struct Team {
    /// The AI file's `index N` line; the player's records have none.
    pub head: Option<RecordHead>,
    /// The name the screen shows (`Red Zone`, `The Rookies`).
    pub name: String,
    /// The team's motto, exactly as the file writes it (the AI mottos carry
    /// their own quotes).
    pub motto: String,
    /// The logo; a game-relative path with backslashes, as `_Filename` wants.
    pub logo: String,
    /// The roster's robot slots (`Robots: 0 1 2`).
    pub robots: Vec<i32>,
    /// The robot entered in the current event (`Robot In Event: -1` = none).
    pub event_robot: i32,
    /// The fourteen state lines between `Robot In Event:` and the league line.
    /// The career accessors below name the recovered ones; the list lines are
    /// [`TeamValue::Text`].
    pub state: Vec<TeamValue>,
    /// The league line: the fifteen AI teams' points, `Team+0x5c`..`+0x94`.
    pub season: [i32; 15],
    /// The per-season history ints, written as nine lines after `+0xf4` when
    /// that flag is true (`Team::save`'s `FUN_004fb460` branch). `Team::load`
    /// reads exactly nine. The shipped records are fresh and carry none.
    pub history: Vec<i32>,
    /// The closing lines (+0x54, +0x58, +0x50).
    pub tail: Vec<i32>,
    /// The file's own line ending.
    pub line_ending: LineEnding,
}

impl Default for Team {
    fn default() -> Self {
        Team {
            head: None,
            name: String::new(),
            motto: String::new(),
            logo: String::new(),
            robots: Vec::new(),
            event_robot: -1,
            state: Vec::new(),
            season: [0; 15],
            history: Vec::new(),
            tail: vec![0, 0, 0],
            line_ending: LineEnding::Crlf,
        }
    }
}

impl Team {
    /// The state line at `Team+0xb8`. The team-HQ screen reads it as the
    /// fastest win in seconds and shows `N/A` when it is [`FASTEST_WIN_NONE`].
    /// The field was called "money" in earlier docs; the recon did not find a
    /// cash field or a cash deduction anywhere (see `docs/16-career.md`).
    pub const FASTEST_WIN_INDEX: usize = 8;

    /// The `N/A` sentinel the shipped records carry at `+0xb8`.
    pub const FASTEST_WIN_NONE: f32 = 10_000.0;

    /// The shipped +0xb8 float (the team-HQ screen's fastest win).
    pub fn fastest_win(&self) -> f32 {
        self.state_float(Self::FASTEST_WIN_INDEX)
    }

    /// Set the +0xb8 float.
    pub fn set_fastest_win(&mut self, seconds: f32) {
        self.set_state_float(Self::FASTEST_WIN_INDEX, seconds);
    }

    /// Compatibility alias for [`Team::fastest_win`]. The field was previously
    /// documented as money; that is not established.
    pub fn money(&self) -> f32 {
        self.fastest_win()
    }

    /// Compatibility alias for [`Team::set_fastest_win`].
    pub fn set_money(&mut self, money: f32) {
        self.set_fastest_win(money);
    }

    // The fourteen state lines, in the order `Team::save` writes them.
    pub const SEASON_INDEX: usize = 0;
    pub const CURRENT_RANK: usize = 1;
    pub const BEST_RANK: usize = 2;
    pub const TOTAL_EVENTS: usize = 3;
    pub const WINS: usize = 4;
    pub const LOSSES: usize = 5;
    pub const EVENTS_WON: usize = 6;
    pub const HIGHEST_SCORE: usize = 7;
    pub const PLAYER_FLAG: usize = 9;
    pub const PRIZES: usize = 10;
    pub const HAS_HISTORY: usize = 11;
    pub const SCHEDULE: usize = 12;
    pub const EVENT_INDEX: usize = 13;

    // The three closing lines, in the order `Team::save` writes them.
    /// Points earned in the current event (`+0x54`).
    pub const EVENT_POINTS: usize = 0;
    /// The player's league/season points (`+0x58`).
    pub const SEASON_POINTS: usize = 1;
    /// Defeated robots accumulated by the current event (`+0x50`).
    pub const DEFEATED: usize = 2;

    fn state_ref(&self, index: usize) -> Option<&TeamValue> {
        self.state.get(index)
    }

    fn state_int_or(&self, index: usize, default: i32) -> i32 {
        match self.state_ref(index) {
            Some(TeamValue::Int(v)) => *v,
            Some(TeamValue::Bool(v)) => i32::from(*v),
            Some(TeamValue::Float(v)) => *v as i32,
            Some(TeamValue::Text(line)) => line
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(default),
            None => default,
        }
    }

    fn state_float(&self, index: usize) -> f32 {
        match self.state_ref(index) {
            Some(TeamValue::Int(v)) => *v as f32,
            Some(TeamValue::Bool(v)) => f32::from(u8::from(*v)),
            Some(TeamValue::Float(v)) => *v,
            Some(TeamValue::Text(line)) => line
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0),
            None => 0.0,
        }
    }

    fn set_state_int(&mut self, index: usize, value: i32) {
        self.grow_state(index);
        self.state[index] = TeamValue::Int(value);
    }

    fn set_state_float(&mut self, index: usize, value: f32) {
        self.grow_state(index);
        self.state[index] = if value.fract() == 0.0 && value.abs() < i32::MAX as f32 {
            // The shipped record spells `10000`, not `10000.0`.
            TeamValue::Int(value as i32)
        } else {
            TeamValue::Float(value)
        };
    }

    fn grow_state(&mut self, index: usize) {
        while self.state.len() <= index {
            self.state.push(TeamValue::Int(0));
        }
    }

    /// The season number (`+0x98`, zero-based; the HQ title shows it plus one).
    pub fn season_index(&self) -> i32 {
        self.state_int_or(Self::SEASON_INDEX, 0)
    }

    pub fn set_season_index(&mut self, season: i32) {
        self.set_state_int(Self::SEASON_INDEX, season);
    }

    /// The team's live league rank (`+0x9c`; 16 is the bottom of the shipped
    /// table and the value a fresh team starts with).
    pub fn current_rank(&self) -> i32 {
        self.state_int_or(Self::CURRENT_RANK, 16)
    }

    pub fn set_current_rank(&mut self, rank: i32) {
        self.set_state_int(Self::CURRENT_RANK, rank);
    }

    /// The best rank the team has held (`+0xa0`).
    pub fn best_rank(&self) -> i32 {
        self.state_int_or(Self::BEST_RANK, 16)
    }

    pub fn set_best_rank(&mut self, rank: i32) {
        self.set_state_int(Self::BEST_RANK, rank);
    }

    /// Events played (`+0xa4`).
    pub fn total_events(&self) -> i32 {
        self.state_int_or(Self::TOTAL_EVENTS, 0)
    }

    pub fn set_total_events(&mut self, n: i32) {
        self.set_state_int(Self::TOTAL_EVENTS, n);
    }

    /// Matches won (`+0xa8`; the HQ history labels it `wins`).
    pub fn wins(&self) -> i32 {
        self.state_int_or(Self::WINS, 0)
    }

    pub fn set_wins(&mut self, n: i32) {
        self.set_state_int(Self::WINS, n);
    }

    /// Matches lost (`+0xac`; the HQ history labels it `losses`).
    pub fn losses(&self) -> i32 {
        self.state_int_or(Self::LOSSES, 0)
    }

    pub fn set_losses(&mut self, n: i32) {
        self.set_state_int(Self::LOSSES, n);
    }

    /// Events won (`+0xb0`).
    pub fn events_won(&self) -> i32 {
        self.state_int_or(Self::EVENTS_WON, 0)
    }

    pub fn set_events_won(&mut self, n: i32) {
        self.set_state_int(Self::EVENTS_WON, n);
    }

    /// The highest robot score the team has posted (`+0xb4`).
    pub fn highest_score(&self) -> i32 {
        self.state_int_or(Self::HIGHEST_SCORE, 0)
    }

    pub fn set_highest_score(&mut self, score: i32) {
        self.set_state_int(Self::HIGHEST_SCORE, score);
    }

    /// Whether the team is the local player's (the shipped `+0xf6` is true in
    /// every record; its exact meaning is not established).
    pub fn player_flag(&self) -> bool {
        self.state_int_or(Self::PLAYER_FLAG, 1) != 0
    }

    /// Whether per-event history follows the record (`+0xf4`).
    pub fn has_history(&self) -> bool {
        !self.history.is_empty() || self.state_int_or(Self::HAS_HISTORY, 0) != 0
    }

    /// The nine history ints (`+0xf4`). Their exact meaning is not established;
    /// the career writer stores the points of the nine scheduled events.
    pub fn history(&self) -> &[i32] {
        &self.history
    }

    /// Set the history ints and the `+0xf4` flag. The record then carries nine
    /// extra lines, which is the shape `Team::load` accepts.
    pub fn set_history(&mut self, values: &[i32]) {
        self.grow_state(Self::EVENT_INDEX);
        self.state[Self::HAS_HISTORY] = TeamValue::Bool(!values.is_empty());
        self.history = values.to_vec();
    }

    /// The current event id (`+0xbc`; `-1` is none).
    pub fn event_index(&self) -> i32 {
        self.state_int_or(Self::EVENT_INDEX, -1)
    }

    pub fn set_event_index(&mut self, event: i32) {
        self.set_state_int(Self::EVENT_INDEX, event);
    }

    /// The points earned in the current event (`+0x54`).
    pub fn event_points(&self) -> i32 {
        self.tail.get(Self::EVENT_POINTS).copied().unwrap_or(0)
    }

    pub fn set_event_points(&mut self, points: i32) {
        self.set_tail(Self::EVENT_POINTS, points);
    }

    /// The player's league/season points (`+0x58`).
    pub fn season_points(&self) -> i32 {
        self.tail.get(Self::SEASON_POINTS).copied().unwrap_or(0)
    }

    pub fn set_season_points(&mut self, points: i32) {
        self.set_tail(Self::SEASON_POINTS, points);
    }

    /// Defeated robots accumulated by the current event (`+0x50`). The label
    /// is ours; the writer is `FUN_0046c5b0` (`Team+0x50 += survivors`).
    pub fn defeated(&self) -> i32 {
        self.tail.get(Self::DEFEATED).copied().unwrap_or(0)
    }

    pub fn set_defeated(&mut self, n: i32) {
        self.set_tail(Self::DEFEATED, n);
    }

    fn set_tail(&mut self, index: usize, value: i32) {
        while self.tail.len() <= index {
            self.tail.push(0);
        }
        self.tail[index] = value;
    }

    /// The fifteen AI teams' league points (`+0x5c`..`+0x94`). The player's
    /// own points are [`Team::season_points`].
    pub fn opponent_points(&self) -> [i32; 15] {
        self.season
    }

    pub fn set_opponent_points(&mut self, points: [i32; 15]) {
        self.season = points;
    }

    /// One state line's whitespace-separated tokens. Used for the prize and
    /// event-schedule lines, which are a count followed by paths/ids.
    pub fn state_tokens(&self, index: usize) -> Vec<String> {
        match self.state_ref(index) {
            Some(TeamValue::Text(line)) => line.split_whitespace().map(str::to_string).collect(),
            Some(value) => vec![value.to_string()],
            None => Vec::new(),
        }
    }

    /// Replace a state line with `count` followed by the tokens. A one-token
    /// line is written as an integer so the shipped records stay identical.
    pub fn set_state_tokens(&mut self, index: usize, tokens: &[String]) {
        self.grow_state(index);
        if tokens.len() == 1 {
            self.state[index] = tokens[0]
                .parse::<i32>()
                .map(TeamValue::Int)
                .unwrap_or_else(|_| TeamValue::Text(tokens[0].clone()));
        } else {
            self.state[index] = TeamValue::Text(tokens.join(" "));
        }
    }

    /// The prize paths from the `+0x100` vector's line (the count is the first
    /// token; the rest are the model paths).
    pub fn prizes(&self) -> Vec<String> {
        let tokens = self.state_tokens(Self::PRIZES);
        let count = tokens
            .first()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        tokens.into_iter().skip(1).take(count).collect()
    }

    pub fn set_prizes(&mut self, prizes: &[String]) {
        let mut tokens = Vec::with_capacity(prizes.len() + 1);
        tokens.push(prizes.len().to_string());
        tokens.extend(prizes.iter().cloned());
        self.set_state_tokens(Self::PRIZES, &tokens);
    }

    /// The event ids from the `+0xe4` schedule vector (count, then ids).
    pub fn schedule(&self) -> Vec<i32> {
        let tokens = self.state_tokens(Self::SCHEDULE);
        let count = tokens
            .first()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        tokens
            .into_iter()
            .skip(1)
            .take(count)
            .filter_map(|s| s.parse().ok())
            .collect()
    }

    pub fn set_schedule(&mut self, ids: &[i32]) {
        let mut tokens = Vec::with_capacity(ids.len() + 1);
        tokens.push(ids.len().to_string());
        tokens.extend(ids.iter().map(i32::to_string));
        self.set_state_tokens(Self::SCHEDULE, &tokens);
    }

    /// The line length the writer will emit, as a diagnostic.
    pub fn state_len(&self) -> usize {
        self.state.len()
    }

    /// The record's identity is present (an empty player slot has none).
    pub fn is_named(&self) -> bool {
        !self.name.is_empty() && !self.logo.is_empty()
    }

    /// Write one record, including its trailing line ending.
    pub fn write(&self) -> String {
        let mut out = String::new();
        let e = self.line_ending.s();
        let mut line = |s: &str| {
            out.push_str(s);
            out.push_str(e);
        };
        if let Some(head) = &self.head {
            line(&head.to_string());
        }
        line(&self.name);
        line(&self.motto);
        line(&self.logo);
        let robots = self
            .robots
            .iter()
            .map(|r| r.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        if robots.is_empty() {
            line("Robots:");
        } else {
            line(&format!("Robots: {robots}"));
        }
        line(&format!("Robot In Event: {}", self.event_robot));
        let mut wrote_history = false;
        for (index, v) in self.state.iter().enumerate() {
            line(&v.to_string());
            if index == Self::HAS_HISTORY {
                for value in &self.history {
                    line(&value.to_string());
                }
                wrote_history = !self.history.is_empty();
            }
        }
        // A caller that built a record by hand may hold a history without a
        // full state vector; keep the lines rather than dropping them.
        if !wrote_history {
            for value in &self.history {
                line(&value.to_string());
            }
        }
        let season = self
            .season
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        // The player's file leaves a space after the last season int; the AI
        // file does not (both are the game's own writer, different builds).
        if self.head.is_none() {
            line(&format!("{season} "));
        } else {
            line(&season);
        }
        for v in &self.tail {
            line(&v.to_string());
        }
        out
    }
}

/// The AI file's per-record head line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordHead {
    Index(i32),
}

impl std::fmt::Display for RecordHead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordHead::Index(n) => write!(f, "index {n}"),
        }
    }
}

/// One of the record's state lines: the file mixes ints, booleans, floats and
/// the two list lines (prizes and the event schedule), which are a count
/// followed by tokens on one line.
#[derive(Clone, Debug, PartialEq)]
pub enum TeamValue {
    Int(i32),
    Bool(bool),
    Float(f32),
    /// A list line kept verbatim (`5 path path …`), so the record round-trips
    /// byte-for-byte and the career writer can replace it.
    Text(String),
}

impl std::fmt::Display for TeamValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeamValue::Int(v) => write!(f, "{v}"),
            TeamValue::Bool(v) => write!(f, "{v}"),
            TeamValue::Float(v) => write!(f, "{v}"),
            TeamValue::Text(v) => write!(f, "{v}"),
        }
    }
}

/// A whole team table: its records and the file-level formatting.
#[derive(Clone, Debug, PartialEq)]
pub struct TeamTable {
    /// The player's file head line (`0`); the AI file has none.
    pub prefix: Option<i32>,
    /// Every record, including the player's empty slots.
    pub teams: Vec<Team>,
    /// The table's line ending.
    pub line_ending: LineEnding,
    /// Whether the file ends with a newline (`AI/Teams.txt` does not).
    pub final_newline: bool,
}

impl TeamTable {
    /// Write the table, byte-for-byte when it came from [`parse_table`].
    pub fn write(&self) -> String {
        let mut out = String::new();
        if let Some(prefix) = self.prefix {
            out.push_str(&prefix.to_string());
            out.push_str(self.line_ending.s());
        }
        for team in &self.teams {
            out.push_str(&team.write());
        }
        if !self.final_newline {
            let e = self.line_ending.s();
            if out.ends_with(e) {
                out.truncate(out.len() - e.len());
            }
        }
        out
    }

    /// Write the table to `path`.
    pub fn write_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        std::fs::write(path, self.write())
    }
}

fn parse_state_value(s: &str, line: usize) -> Result<TeamValue, String> {
    Ok(match s {
        "true" => TeamValue::Bool(true),
        "false" => TeamValue::Bool(false),
        s if s.split_whitespace().count() > 1 => TeamValue::Text(s.to_string()),
        s => match s.parse::<i32>() {
            Ok(v) => TeamValue::Int(v),
            Err(_) => TeamValue::Float(
                s.parse()
                    .map_err(|_| format!("line {line}: bad state value {s:?}"))?,
            ),
        },
    })
}

/// Parse a team table into all of its records, empty slots included.
pub fn parse_table(text: &str) -> Result<TeamTable, String> {
    let line_ending = if text.contains("\r\n") {
        LineEnding::Crlf
    } else {
        LineEnding::Lf
    };
    let final_newline = text.ends_with('\n');
    let lines: Vec<&str> = text.lines().collect();
    let mut teams = Vec::new();
    let mut i = 0;
    // The player's file opens with a bare number; the AI file does not.
    let prefix = match lines.first().map(|l| l.trim()) {
        Some(line) if !line.is_empty() && !line.starts_with("index ") => {
            i = 1;
            Some(
                line.parse()
                    .map_err(|_| format!("line 1: bad table head {line:?}"))?,
            )
        }
        _ => None,
    };
    while i < lines.len() {
        // A record's own name line may be blank (an empty player slot); only
        // trailing blank lines are not records.
        if lines[i].trim().is_empty() && lines[i..].iter().all(|l| l.trim().is_empty()) {
            break;
        }
        let line = lines[i].trim();
        let head = if let Some(n) = line.strip_prefix("index ") {
            Some(RecordHead::Index(
                n.trim()
                    .parse()
                    .map_err(|_| format!("line {}: bad index {n:?}", i + 1))?,
            ))
        } else {
            None
        };
        let base = if head.is_some() { 1 } else { 0 };
        if i + base + 17 > lines.len() {
            return Err(format!("line {}: truncated team record", i + 1));
        }
        let start = i + base;
        let at = |k: usize| lines[start + k];
        let has_history = match at(16) {
            "true" => true,
            "false" => false,
            other => {
                return Err(format!(
                    "line {}: expected the +0xf4 flag, found {other:?}",
                    start + 17
                ))
            }
        };
        let extra = if has_history { 9 } else { 0 };
        let need = base + 23 + extra;
        if i + need > lines.len() {
            return Err(format!("line {}: truncated team record", i + 1));
        }
        let robots = at(3)
            .strip_prefix("Robots:")
            .ok_or_else(|| format!("line {}: expected Robots:", start + 4))?;
        let event = at(4)
            .strip_prefix("Robot In Event:")
            .ok_or_else(|| format!("line {}: expected Robot In Event:", start + 5))?;
        let mut state = Vec::with_capacity(14);
        for k in 5..16 {
            state.push(parse_state_value(at(k), start + k + 1)?);
        }
        state.push(parse_state_value(at(16), start + 17)?);
        let mut history = Vec::new();
        if has_history {
            for k in 0..9 {
                history.push(
                    at(17 + k)
                        .trim()
                        .parse::<i32>()
                        .map_err(|_| format!("line {}: bad history value", start + 18 + k))?,
                );
            }
        }
        state.push(parse_state_value(at(17 + extra), start + 18 + extra)?);
        state.push(parse_state_value(at(18 + extra), start + 19 + extra)?);
        let season_line = 19 + extra;
        let mut season = [0i32; 15];
        for (k, slot) in season.iter_mut().enumerate() {
            *slot = at(season_line)
                .split_whitespace()
                .nth(k)
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| format!("line {}: bad season line", start + season_line + 1))?;
        }
        let tail = (0..3)
            .map(|k| at(season_line + 1 + k).trim().parse::<i32>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| format!("line {}: bad closing line", start + season_line + 2))?;
        teams.push(Team {
            head,
            name: at(0).to_string(),
            motto: at(1).to_string(),
            logo: at(2).to_string(),
            robots: robots
                .split_whitespace()
                .map(|s| s.parse().unwrap_or(0))
                .collect(),
            event_robot: event.trim().parse().unwrap_or(-1),
            state,
            season,
            history,
            tail,
            line_ending,
        });
        i += need;
    }
    Ok(TeamTable {
        prefix,
        teams,
        line_ending,
        final_newline,
    })
}

/// Parse a team table, returning only the named teams.
///
/// The identity lines are the three lines before each `Robots:` line. A record
/// with no name is an empty team slot and is skipped.
pub fn parse(text: &str) -> Vec<Team> {
    parse_table(text)
        .map(|t| t.teams.into_iter().filter(Team::is_named).collect())
        .unwrap_or_default()
}

/// Read a team table from disk, returning only the named teams.
pub fn read(path: &std::path::Path) -> Result<Vec<Team>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(parse(&text))
}

/// Read every record of a team table from disk, empty slots included.
pub fn read_table(path: &std::path::Path) -> Result<TeamTable, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_table(&text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game_dir() -> Option<std::path::PathBuf> {
        if let Some(dir) = std::env::var_os("RA2_GAME") {
            let dir = std::path::PathBuf::from(dir);
            return dir.is_dir().then_some(dir);
        }
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        while dir.pop() {
            let candidate = dir.join("local/game");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
        None
    }

    /// The record's first three lines are the identity, and a whole record
    /// parses (the shipped player file has fifteen empty slots after the one
    /// named team).
    #[test]
    fn the_identity_is_the_three_lines_before_robots() {
        let text = "0\r\nThe Rookies\r\nJust getting started...\r\nTeams\\orange_skull.bmp\r\nRobots: 0 1 2\r\nRobot In Event: -1\r\n0\r\n16\r\n16\r\n0\r\n0\r\n0\r\n0\r\n0\r\n10000\r\ntrue\r\n0\r\nfalse\r\n0\r\n-1\r\n0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 \r\n0\r\n0\r\n0\r\n";
        let teams = parse(text);
        assert_eq!(teams.len(), 1);
        assert_eq!(teams[0].name, "The Rookies");
        assert_eq!(teams[0].motto, "Just getting started...");
        assert_eq!(teams[0].logo, "Teams\\orange_skull.bmp");
        assert_eq!(teams[0].robots, vec![0, 1, 2]);
        assert_eq!(teams[0].money(), 10_000.0);
        assert_eq!(teams[0].season, [0; 15]);
    }

    /// The career fields are the record's own lines: the league line, the
    /// closing lines and the prize/schedule list lines. A record with both
    /// lists populated still parses and writes byte-for-byte.
    #[test]
    fn career_fields_are_typed_and_list_lines_round_trip() {
        let text = "0\r\n\
            Career Team\r\n\
            motto\r\n\
            Teams\\career.bmp\r\n\
            Robots: 0 1 2\r\n\
            Robot In Event: -1\r\n\
            0\r\n16\r\n4\r\n3\r\n2\r\n1\r\n5\r\n700\r\n1234.5\r\ntrue\r\n\
            2 Arenas\\prizes\\A.gmf Arenas\\prizes\\B.gmf\r\n\
            false\r\n\
            9 0 3 7 11 2 5 8 14 20\r\n\
            -1\r\n\
            0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 \r\n\
            40\r\n\
            120\r\n\
            6\r\n";
        let table = parse_table(text).unwrap();
        let team = &table.teams[0];
        assert_eq!(team.season_index(), 0);
        assert_eq!(team.current_rank(), 16);
        assert_eq!(team.best_rank(), 4);
        assert_eq!(team.total_events(), 3);
        assert_eq!(team.wins(), 2);
        assert_eq!(team.losses(), 1);
        assert_eq!(team.events_won(), 5);
        assert_eq!(team.highest_score(), 700);
        assert_eq!(team.fastest_win(), 1234.5);
        assert_eq!(
            team.prizes(),
            vec![
                "Arenas\\prizes\\A.gmf".to_string(),
                "Arenas\\prizes\\B.gmf".to_string()
            ]
        );
        assert_eq!(team.schedule(), vec![0, 3, 7, 11, 2, 5, 8, 14, 20]);
        assert_eq!(team.event_index(), -1);
        assert_eq!(team.opponent_points(), [0; 15]);
        assert_eq!(team.event_points(), 40);
        assert_eq!(team.season_points(), 120);
        assert_eq!(team.defeated(), 6);
        assert!(!team.has_history());
        assert_eq!(table.write(), text);

        let mut team = team.clone();
        team.set_season_points(999);
        team.set_opponent_points([7; 15]);
        team.set_prizes(&["Arenas\\prizes\\C.gmf".to_string()]);
        team.set_schedule(&[1, 2, 3, 4, 5, 6, 7, 8, 9]);
        let text = format!("0\r\n{}", team.write());
        let back = parse_table(&text).unwrap().teams[0].clone();
        assert_eq!(back.season_points(), 999);
        assert_eq!(back.opponent_points(), [7; 15]);
        assert_eq!(back.prizes(), vec!["Arenas\\prizes\\C.gmf".to_string()]);
        assert_eq!(back.schedule(), vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);

        // A played season carries the nine history lines; the record grows by
        // nine lines and still parses back.
        let mut team = back;
        let history = [10, 20, 30, 40, 50, 60, 70, 80, 90];
        team.set_history(&history);
        assert!(team.has_history());
        let text = format!("0\r\n{}", team.write());
        let back = parse_table(&text).unwrap().teams[0].clone();
        assert_eq!(back.history(), history);
        assert!(back.has_history());
        assert_eq!(back.schedule(), vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(back.season_points(), 999);
        let again = format!("0\r\n{}", back.write());
        assert_eq!(again, text);
    }

    /// The shipped files round-trip byte-for-byte, and the 15 ints and money
    /// are readable and writable.
    #[test]
    fn shipped_tables_round_trip_and_season_values_survive() {
        let Some(game) = game_dir() else {
            eprintln!("skipping team corpus test: no local/game (set RA2_GAME to run it)");
            return;
        };
        for rel in ["Teams/Teams.txt", "AI/Teams.txt"] {
            let path = game.join(rel);
            let original = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let table = parse_table(&original).unwrap();
            assert_eq!(table.write(), original, "{rel} is not byte-identical");
            assert_eq!(
                table.teams.len(),
                if rel.starts_with("Teams") { 16 } else { 15 }
            );
            assert_eq!(
                table.teams[0].money(),
                if rel.starts_with("Teams") {
                    10_000.0
                } else {
                    100_000.0
                }
            );
            assert_eq!(table.teams[0].season, [0; 15]);
        }

        let path = game.join("Teams/Teams.txt");
        let mut table = read_table(&path).unwrap();
        table.teams[0].season[0] = 7;
        table.teams[0].season[14] = 42;
        table.teams[0].set_money(25_000.0);
        table.teams[1].season[3] = -1;
        table.teams[1].set_money(1234.5);

        let text = table.write();
        let back = parse_table(&text).unwrap();
        assert_eq!(back.teams[0].season[0], 7);
        assert_eq!(back.teams[0].season[14], 42);
        assert_eq!(back.teams[0].money(), 25_000.0);
        assert_eq!(back.teams[1].season[3], -1);
        assert_eq!(back.teams[1].money(), 1234.5);
        // Everything else is unchanged.
        assert_eq!(back.teams[0].name, "The Rookies");
        assert_eq!(back.teams[0].robots, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(back.teams.len(), 16);
    }
}
