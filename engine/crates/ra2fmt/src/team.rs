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
//! After `Robot In Event:` the record has fourteen state lines (the money is
//! line 9, `Team+0xb8`), the fifteen-int season line (`Team+0x5c`..`+0x94`,
//! all zero in the shipped copy), and three closing zero lines.
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
    /// The fourteen state lines between `Robot In Event:` and the season line.
    /// Their meanings are only partly recovered; [`Team::money`] reads the one
    /// the binary exposes (`Team+0xb8`).
    pub state: Vec<TeamValue>,
    /// The season line: fifteen ints at `Team+0x5c`..`+0x94`.
    pub season: [i32; 15],
    /// The closing zero lines (three in the shipped files).
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
            tail: vec![0, 0, 0],
            line_ending: LineEnding::Crlf,
        }
    }
}

impl Team {
    /// The state line the binary exposes as the team's money (`Team+0xb8`).
    pub const MONEY_INDEX: usize = 8;

    /// The team's money, as the game's `Team+0xb8` float.
    pub fn money(&self) -> f32 {
        match self.state.get(Self::MONEY_INDEX) {
            Some(TeamValue::Int(v)) => *v as f32,
            Some(TeamValue::Float(v)) => *v,
            _ => 0.0,
        }
    }

    /// Set the money. The game writes it as an integer when it has no
    /// fractional part, which is what the shipped files carry.
    pub fn set_money(&mut self, money: f32) {
        if self.state.len() > Self::MONEY_INDEX {
            self.state[Self::MONEY_INDEX] = if money.fract() == 0.0 {
                TeamValue::Int(money as i32)
            } else {
                TeamValue::Float(money)
            };
        }
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
        for v in &self.state {
            line(&v.to_string());
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

/// One of the record's state lines: the file mixes ints and booleans.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TeamValue {
    Int(i32),
    Bool(bool),
    Float(f32),
}

impl std::fmt::Display for TeamValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeamValue::Int(v) => write!(f, "{v}"),
            TeamValue::Bool(v) => write!(f, "{v}"),
            TeamValue::Float(v) => write!(f, "{v}"),
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
        let need = base + 23;
        if i + need > lines.len() {
            return Err(format!("line {}: truncated team record", i + 1));
        }
        let start = i + base;
        let at = |k: usize| lines[start + k];
        let robots = at(3)
            .strip_prefix("Robots:")
            .ok_or_else(|| format!("line {}: expected Robots:", start + 4))?;
        let event = at(4)
            .strip_prefix("Robot In Event:")
            .ok_or_else(|| format!("line {}: expected Robot In Event:", start + 5))?;
        let mut state = Vec::new();
        for k in 5..19 {
            state.push(match at(k) {
                "true" => TeamValue::Bool(true),
                "false" => TeamValue::Bool(false),
                s => {
                    match s.parse::<i32>() {
                        Ok(v) => TeamValue::Int(v),
                        Err(_) => TeamValue::Float(s.parse().map_err(|_| {
                            format!("line {}: bad state value {s:?}", start + k + 1)
                        })?),
                    }
                }
            });
        }
        let mut season = [0i32; 15];
        for (k, slot) in season.iter_mut().enumerate() {
            *slot = at(19)
                .split_whitespace()
                .nth(k)
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| format!("line {}: bad season line", start + 20))?;
        }
        let tail = (0..3)
            .map(|k| at(20 + k).trim().parse::<i32>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| format!("line {}: bad closing line", start + 20))?;
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
