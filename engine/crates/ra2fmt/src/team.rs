//! The game's team tables: `Teams/Teams.txt` and `AI/Teams.txt`.
//!
//! A team record is the game's own text form, written by `Team::save`
//! (`FUN_004fb010`) and read back by `Team::load` (`FUN_004fabf0`). The first
//! three lines of a record are the fields the shell's screens show:
//!
//! ```text
//! index 0                      (AI/Teams.txt only; Teams.txt starts with the slot)
//! Red Zone                     name
//! "Do NOT enter the Red Zone!" motto
//! AI\red_zone.bmp              logo
//! Robots: 0 1 2
//! Robot In Event: -1
//! ...                          the rest is the team's state, not identity
//! ```
//!
//! `Teams/Teams.txt` has sixteen slots and only the first is named in the
//! shipped file; `AI/Teams.txt` has the fifteen AI teams. Empty slots are
//! skipped — they have no name and no logo, and the shell has nothing to draw
//! for them.
//!
//! The record is line-oriented, so the three identity lines are exactly the
//! three lines before `Robots:`. That is also how the empty slots are told
//! apart: their name line is blank.

/// A team as the shell shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Team {
    /// The name the screen shows (`Red Zone`, `The Rookies`).
    pub name: String,
    /// The team's motto, exactly as the file writes it (the AI mottos carry
    /// their own quotes).
    pub motto: String,
    /// The logo; a game-relative path with backslashes, as `_Filename` wants.
    pub logo: String,
}

/// Parse a team table.
///
/// The identity lines are the three lines before each `Robots:` line. A record
/// with no name is an empty team slot and is skipped.
pub fn parse(text: &str) -> Vec<Team> {
    let lines: Vec<&str> = text
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .collect();
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !line.starts_with("Robots:") || index < 3 {
            continue;
        }
        let name = lines[index - 3].trim();
        let motto = lines[index - 2].trim();
        let logo = lines[index - 1].trim();
        if name.is_empty() || logo.is_empty() {
            continue;
        }
        out.push(Team {
            name: name.to_string(),
            motto: motto.to_string(),
            logo: logo.to_string(),
        });
    }
    out
}

/// Read a team table from disk.
pub fn read(path: &std::path::Path) -> Result<Vec<Team>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(parse(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The record's first three lines are the identity, and the shipped player
    /// file has fifteen empty slots after the one named team.
    #[test]
    fn the_identity_is_the_three_lines_before_robots() {
        let text = "0\r\nThe Rookies\r\nJust getting started...\r\nTeams\\orange_skull.bmp\r\nRobots: 0 1 2\r\nRobot In Event: -1\r\n\r\n\r\n\r\nRobots:\r\nRobot In Event: -1\r\n";
        let teams = parse(text);
        assert_eq!(
            teams,
            vec![Team {
                name: "The Rookies".into(),
                motto: "Just getting started...".into(),
                logo: "Teams\\orange_skull.bmp".into(),
            }]
        );
    }
}
