//! The team rosters the exhibition picker offers.
//!
//! The data is the game's own two tables (`ra2fmt::team`):
//!
//! * `Teams/Teams.txt` — **sixteen** records, the player's own team table. The
//!   shipped file names one team (`The Rookies`, team 0) and leaves the other
//!   fifteen slots empty; the picker still lists all sixteen because that is
//!   the table's size and a career may fill them.
//! * `AI/Teams.txt` — **fifteen** named AI teams.
//!
//! Each record's `Robots:` line is the roster: the `.bot` slot numbers under
//! `Teams/TeamN/BotM.bot` or `AI/TeamN/BotM.bot`. The names, mottos and logos
//! the picker shows are the record's own lines. Nothing here invents a team or
//! a robot: an empty slot is listed as an empty name with no robots.

use std::path::{Path, PathBuf};

use ra2fmt::gib::Widget;
use ra2fmt::team::{self, Team};

/// Which of the game's two tables a picker reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// `Teams/Teams.txt`, sixteen records: the player's team table.
    Player,
    /// `AI/Teams.txt`, fifteen records: the computer's teams.
    Computer,
}

impl Side {
    /// The directory the side's teams live under.
    pub fn root(self) -> &'static str {
        match self {
            Side::Player => "Teams",
            Side::Computer => "AI",
        }
    }

    /// The table the side's teams come from.
    pub fn table(self) -> &'static str {
        match self {
            Side::Player => "Teams/Teams.txt",
            Side::Computer => "AI/Teams.txt",
        }
    }

    /// The word the screen's own notes use (`player 1`, `computer`).
    pub fn name(self) -> &'static str {
        match self {
            Side::Player => "player",
            Side::Computer => "computer",
        }
    }

    /// The shipped picker window the side's teams open in.
    pub fn picker(self) -> &'static str {
        match self {
            Side::Player => "exhibition/selectbot_player.gib",
            Side::Computer => "exhibition/selectbot_computer.gib",
        }
    }
}

/// One team record as the picker needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Roster {
    /// The record's slot in its table (0-based).
    pub slot: usize,
    /// The name the picker's dropdown shows; empty for a player slot the
    /// shipped file leaves blank.
    pub name: String,
    pub motto: String,
    pub logo: String,
    /// The `Robots:` slot numbers, in the record's own order.
    pub robots: Vec<i32>,
    /// The record's identity lines are present (`Team::is_named`).
    pub named: bool,
}

impl Roster {
    fn from_team(slot: usize, team: &Team) -> Roster {
        Roster {
            slot,
            name: team.name.clone(),
            motto: team.motto.clone(),
            logo: team.logo.clone(),
            robots: team.robots.clone(),
            named: team.is_named(),
        }
    }

    /// The dropdown text. An unnamed slot is a blank row, which is what the
    /// table holds — not a made-up "Team 5".
    pub fn row_text(&self) -> &str {
        &self.name
    }
}

/// Every record of one side, empty player slots included.
pub fn rosters(game: &Path, side: Side) -> Result<Vec<Roster>, String> {
    let path = game.join(side.table());
    let table = team::read_table(&path)?;
    Ok(table
        .teams
        .iter()
        .enumerate()
        .map(|(slot, team)| Roster::from_team(slot, team))
        .collect())
}

/// The record an empty player slot holds: blank identity lines, no robots,
/// and the reset counters (`FUN_004fa230` clears the record to these; the
/// shipped table's fifteen empty slots are exactly this record).
const EMPTY_RECORD: &str = "0\r\n\r\n\r\n\r\nRobots:\r\nRobot In Event: -1\r\n0\r\n16\r\n16\r\n0\r\n0\r\n0\r\n0\r\n0\r\n10000\r\ntrue\r\n0\r\nfalse\r\n0\r\n-1\r\n0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 \r\n0\r\n0\r\n0\r\n";

/// A fresh empty player record.
pub fn empty_player_record() -> Team {
    team::parse_table(EMPTY_RECORD)
        .ok()
        .and_then(|table| table.teams.into_iter().next())
        .unwrap_or_default()
}

/// `New Team`'s OK (`FUN_004ebd40`, result 1): the first empty slot of
/// `Teams/Teams.txt` becomes the team (`FUN_00502680(slot, 1)` marks it
/// used), and the table is written at once: the dialog broadcasts message 200
/// (`FUN_005882f0`), whose listener, registered by the player table's
/// constructor at `0x502173`, is `FUN_00502720` -> `FUN_005024e0`
/// (`Teams\Teams.txt`). The team's robot folder `Teams/Team<slot>/` is made
/// for its robots. Returns the slot, or `None` when every slot is taken
/// (the dialog's "There are no empty team slots." case).
pub fn create_team(
    game: &Path,
    name: &str,
    motto: &str,
    logo: &str,
) -> Result<Option<usize>, String> {
    let path = game.join(Side::Player.table());
    let mut table = team::read_table(&path)?;
    let Some(slot) = table.teams.iter().position(|t| !t.is_named()) else {
        return Ok(None);
    };
    let ending = table.teams[slot].line_ending;
    let mut record = empty_player_record();
    record.line_ending = ending;
    record.name = name.to_string();
    record.motto = motto.to_string();
    record.logo = logo.to_string();
    table.teams[slot] = record;
    table
        .write_to(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let folder = game.join(Side::Player.root()).join(format!("Team{slot}"));
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    Ok(Some(slot))
}

/// A robot built in the workshop from an empty inventory selection joins
/// the team: the record's `Robots:` line gains the first free slot number and
/// the robot is `Teams/Team<team>/Bot<n>.bot`. The table is written at once.
/// Returns the new robot's path.
pub fn add_robot(game: &Path, team: usize) -> Result<PathBuf, String> {
    let path = game.join(Side::Player.table());
    let mut table = team::read_table(&path)?;
    let record = table
        .teams
        .get_mut(team)
        .filter(|t| t.is_named())
        .ok_or_else(|| format!("{}: team slot {team} is empty", path.display()))?;
    let folder = game.join(Side::Player.root()).join(format!("Team{team}"));
    let slot = (0..)
        .find(|n: &i32| !record.robots.contains(n) && !folder.join(format!("Bot{n}.bot")).exists())
        .expect("a free robot slot");
    record.robots.push(slot);
    table
        .write_to(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    Ok(folder.join(format!("Bot{slot}.bot")))
}

/// `Delete Team`'s Yes (`FUN_004ec110`, result 4): the record is cleared
/// (`FUN_00502680(slot, 0)`, `FUN_004fa230`) and the table written through the
/// same message-200 listener. The robots' files are left where they are; the
/// slot simply lists none.
pub fn delete_team(game: &Path, slot: usize) -> Result<(), String> {
    let path = game.join(Side::Player.table());
    let mut table = team::read_table(&path)?;
    let Some(existing) = table.teams.get(slot) else {
        return Err(format!("{}: no team slot {slot}", path.display()));
    };
    let ending = existing.line_ending;
    let mut record = empty_player_record();
    record.line_ending = ending;
    table.teams[slot] = record;
    table
        .write_to(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    // The client's own per-team career sidecar (`ra2sim::career::
    // sidecar_path_for`) goes with the record.
    let sidecar = if slot == 0 {
        game.join("Teams").join("career.dat")
    } else {
        game.join("Teams").join(format!("career{slot}.dat"))
    };
    if sidecar.is_file() {
        std::fs::remove_file(&sidecar).map_err(|e| format!("{}: {e}", sidecar.display()))?;
    }
    Ok(())
}

/// The default player team: the career's team, which is the first named record
/// of `Teams/Teams.txt` (the same rule `Career::from_tables` uses).
pub fn career_player_slot(rosters: &[Roster]) -> Option<usize> {
    rosters.iter().find(|roster| roster.named).map(|r| r.slot)
}

/// `<game>/Teams/Team<slot>/Bot<bot>.bot` or `<game>/AI/Team<slot>/Bot<bot>.bot`.
pub fn bot_path(game: &Path, side: Side, team: usize, bot: i32) -> PathBuf {
    game.join(side.root())
        .join(format!("Team{team}"))
        .join(format!("Bot{bot}.bot"))
}

/// A `.bot` file's own name and weight class, for the picker's labels and
/// weight icon. `None` when the file is missing or not a bot this crate reads.
pub fn bot_info(path: &Path) -> Option<(String, i32)> {
    let bytes = std::fs::read(path).ok()?;
    let bot = ra2fmt::bot::parse(&bytes).ok()?;
    Some((bot.name, bot.class))
}

/// Fill the picker's own `team dropdown` list with one row per record.
pub fn fill_team_list(list: &mut Widget, rosters: &[Roster]) -> usize {
    let rows: Vec<String> = rosters.iter().map(|r| r.row_text().to_string()).collect();
    crate::input::text_rows(list, &rows)
}

/// The select-team screen's grid (`select_team.gib`), and the cell the binary
/// loads into it for each team (`team_cell.gib`'s own note).
pub const SELECT_GRID: &str = "select team grid";
pub const SELECT_CELL: &str = "teamhq/team_cell.gib";

/// Fill the select-team grid with a cell per named team (`FUN_004ec2c0` /
/// `FUN_004ec310` fill it from `Teams/Teams.txt`) and select the first row,
/// which is what the screen opens with.
pub fn fill_select_grid(
    ui_root: &Path,
    document: &mut ra2fmt::gib::Gib,
    rosters: &[Roster],
) -> Result<usize, String> {
    use ra2fmt::gib::Block;
    let template = crate::cells::template(ui_root, SELECT_CELL)?;
    let named: Vec<&Roster> = rosters.iter().filter(|r| r.named).collect();
    for block in document.blocks.iter_mut() {
        let Block::Widget(root) = block else { continue };
        let Some(grid) = crate::workshop::find_mut(root, SELECT_GRID) else {
            continue;
        };
        grid.children.retain(|c| c.name() != "team cell");
        let made = crate::cells::fill(grid, &template, named.len());
        let bars = grid.children.len() - made;
        for (cell, roster) in grid.children[bars..].iter_mut().zip(&named) {
            crate::cells::bind_value(cell, "team name textbox", &roster.name);
            crate::cells::bind_value(cell, "team motto textbox", &roster.motto);
            crate::cells::bind_art(cell, "logo img", &roster.logo);
        }
        crate::input::clear_selection(grid);
        if !named.is_empty() {
            crate::input::select_cell(grid, 0, 0);
        }
        // The bars span the grid (the file's 444 px is the editor's canvas).
        for bar in grid.children.iter_mut() {
            if bar.name() == "Sel Bar" || bar.name() == "High Bar" {
                crate::input::set_field(bar, "Size", "1 1 80 0");
            }
        }
        // The scroll bar only shows when there are more teams than rows.
        let rows = crate::Grid::from_widget(grid).map_or(0, |g| g.rows.max(0) as usize);
        crate::input::show_scroller(grid, named.len() > rows);
        return Ok(made);
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game_dir() -> Option<PathBuf> {
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

    /// The two tables are the sizes the game ships: sixteen player records
    /// (one named in the shipped file) and fifteen named AI teams. A team's
    /// robots are its own `Robots:` line, and the paths are the game's.
    #[test]
    fn the_rosters_are_the_two_shipped_tables() {
        let Some(game) = game_dir() else { return };
        let players = rosters(&game, Side::Player).unwrap();
        assert_eq!(players.len(), 16, "Teams/Teams.txt records");
        assert_eq!(career_player_slot(&players), Some(0));
        assert_eq!(players[0].name, "The Rookies");
        assert_eq!(players[0].robots, vec![0, 1, 2, 3, 4, 5]);
        assert!(players[1].robots.is_empty(), "an empty player slot");
        assert!(!players[1].named);

        let computers = rosters(&game, Side::Computer).unwrap();
        assert_eq!(computers.len(), 15, "AI/Teams.txt records");
        assert_eq!(computers[1].name, "PREHISTORIC");
        assert_eq!(computers[1].robots, vec![0, 1, 2]);
        assert_eq!(
            bot_path(&game, Side::Computer, 1, 0),
            game.join("AI/Team1/Bot0.bot")
        );
        assert_eq!(
            bot_path(&game, Side::Player, 0, 5),
            game.join("Teams/Team0/Bot5.bot")
        );
        assert_eq!(
            bot_info(&game.join("AI/Team1/Bot0.bot")).map(|(name, _)| name),
            Some("Roly Poly".to_string())
        );
    }
}
