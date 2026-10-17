//! Which of the three shipped tracks belongs to a screen.
//!
//! The game ships exactly three music files, one per screen group:
//!
//! ```text
//! Music/Main UI/Enter the Battlefield.mp3
//! Music/Team HQ/Robot Tribal Dance.mp3
//! Music/Workshop/Deep inside the Bot Lab.mp3
//! ```
//!
//! The executable names the three directories literally (`Music\Main UI`,
//! `Music\Team HQ`, `Music\Workshop`), which is the evidence that the grouping
//! is by the shell's three main areas rather than by individual screen. The
//! screen paths in the shell (`ra2-shell::nav`) have the matching roots:
//!
//! | Screen root | Track | Evidence |
//! |---|---|---|
//! | `mainmenu/` | `Main UI` | the boot screen is `mainmenu/mainmenu.gib`; the executable names `Music\Main UI` |
//! | `teamhq/` | `Team HQ` | the team HQ is `teamhq/teamhq.gib`; the executable names `Music\Team HQ` |
//! | `botlab/` | `Workshop` | the bot lab is `botlab/botlabwin.gib`; the executable names `Music\Workshop` |
//!
//! The other shipped screen roots — `exhibition/`, `multiplayer/`, `help/`,
//! `armor/` — are **not mapped**. The executable names three music directories
//! and nothing in the shipped route notes says which of them, if any, a setup
//! or help screen inherits. Guessing "all non-HQ screens are Main UI" would
//! make the mapping untrustworthy, so [`music_for_screen`] returns `None` for
//! them and the caller keeps the current track (or plays silence) until the
//! mapping is established.

use std::path::{Path, PathBuf};

/// One of the shipped music tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Track {
    /// The directory under `Music/`, exactly as shipped (`Main UI`).
    pub group: &'static str,
    /// The file inside it, exactly as shipped (`Enter the Battlefield.mp3`).
    pub file: &'static str,
}

impl Track {
    /// The path relative to the game directory, with `/` separators.
    pub fn relative_path(&self) -> String {
        format!("Music/{}/{}", self.group, self.file)
    }

    /// The track inside a game installation directory.
    pub fn in_game_dir(&self, game: &Path) -> PathBuf {
        game.join("Music").join(self.group).join(self.file)
    }
}

/// The main-menu group's track.
pub const MAIN_UI: Track = Track {
    group: "Main UI",
    file: "Enter the Battlefield.mp3",
};

/// The team-HQ group's track.
pub const TEAM_HQ: Track = Track {
    group: "Team HQ",
    file: "Robot Tribal Dance.mp3",
};

/// The bot-workshop group's track.
pub const WORKSHOP: Track = Track {
    group: "Workshop",
    file: "Deep inside the Bot Lab.mp3",
};

/// Every shipped track, in the order above.
pub const TRACKS: [Track; 3] = [MAIN_UI, TEAM_HQ, WORKSHOP];

/// The track for a shell screen path, or `None` when the screen's group is not
/// established.
///
/// `screen` is a path relative to `UI` as `ra2-shell` stores it
/// (`mainmenu/mainmenu.gib`, `teamhq\teamhq.gib`, `botlab/botlabwin.gib`).
/// Backslashes are accepted, a leading `UI/` is ignored, and the first path
/// component is matched case-insensitively.
pub fn music_for_screen(screen: &str) -> Option<Track> {
    let screen = screen.replace('\\', "/");
    let screen = screen.strip_prefix("UI/").unwrap_or(&screen);
    let root = screen.split('/').next()?.trim();
    if root.eq_ignore_ascii_case("mainmenu") {
        Some(MAIN_UI)
    } else if root.eq_ignore_ascii_case("teamhq") {
        Some(TEAM_HQ)
    } else if root.eq_ignore_ascii_case("botlab") {
        Some(WORKSHOP)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_groups_map_to_their_tracks() {
        assert_eq!(music_for_screen("mainmenu/mainmenu.gib"), Some(MAIN_UI));
        assert_eq!(music_for_screen("UI\\teamhq\\teamhq.gib"), Some(TEAM_HQ));
        assert_eq!(music_for_screen("BOTLAB/botlabwin.gib"), Some(WORKSHOP));
    }

    #[test]
    fn unestablished_groups_are_not_guessed() {
        for screen in [
            "exhibition/arena.gib",
            "multiplayer/multiplayer.gib",
            "help/help.gib",
            "armor/armor.gib",
            "",
        ] {
            assert_eq!(music_for_screen(screen), None, "{screen}");
        }
    }

    #[test]
    fn the_paths_are_the_shipped_ones() {
        assert_eq!(
            MAIN_UI.relative_path(),
            "Music/Main UI/Enter the Battlefield.mp3"
        );
        assert_eq!(
            TEAM_HQ.relative_path(),
            "Music/Team HQ/Robot Tribal Dance.mp3"
        );
        assert_eq!(
            WORKSHOP.relative_path(),
            "Music/Workshop/Deep inside the Bot Lab.mp3"
        );
        assert_eq!(TRACKS.len(), 3);
    }
}
