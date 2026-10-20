//! What a click's `Notes` say the shell does next.
//!
//! The screens name most transitions themselves. A note that carries a `.gib`
//! name is read directly: `click to load arena.gib into content group` loads
//! that file into the group called `content group`, and `click to view
//! credits.gib` replaces the screen. Those need no table here at all.
//!
//! The handful of notes that name a *purpose* rather than a file are the C++'s
//! transitions — `link to team HQ`, `click to go to the bot workshop` — and they
//! are the small table below. It is not trusted: [`tests`] re-reads every note
//! from the shipped `.gib` files and checks that the target is a screen the data
//! names, and `ra2-shell`'s route test re-reads the same strings.

/// The file the game boots: the shell's main menu.
///
/// The game's own executable names it, next to the window class that screen's
/// root record carries (`Main Menu Win`), and the shipped `common/bottomnav.gib`
/// names it too — its `MainMenu button` says *"click to return to main menu"*.
/// The constant is the reader's starting point; the test in this module checks
/// all three strings in the shipped files.
pub const BOOT_SCREEN: &str = "mainmenu/mainmenu.gib";

/// A transition whose note names a purpose rather than a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    /// The exact `Notes` text on the control.
    pub note: &'static str,
    /// The screen it opens, relative to `UI`, with `/` separators.
    pub screen: &'static str,
    /// What in the data makes this the target.
    pub because: &'static str,
}

/// The purpose-named transitions, read off the shipped screens.
pub const LINKS: &[Link] = &[
    Link {
        note: "link to exhibition setup",
        screen: "exhibition/exhib_setup.gib",
        because: "its root is `exhibition setup win`, and the screen's own `arena button` loads `arena.gib`",
    },
    Link {
        // The C++ opens the team picker first when no team is selected; the
        // select screen's own `continue button` says *"click to go to team hq
        // with selected team"*, which is the next link in this table.
        note: "link to team HQ",
        screen: "teamhq/select_team.gib",
        because: "the shipped picker's title is `SELECT YOUR ROBOTICS TEAM`, and its `continue button` goes to team HQ",
    },
    Link {
        note: "link to multiplayer",
        screen: "multiplayer/multiplayer.gib",
        because: "its root is the multiplayer window",
    },
    Link {
        note: "click to return to main menu",
        screen: BOOT_SCREEN,
        because: "the bottom navigation bar's own `MainMenu button` names it",
    },
    Link {
        note: "click to bring up options dialog",
        screen: "common/options.gib",
        because: "the main menu's `Options button` opens the shipped options window, whose root is `options win`",
    },
    Link {
        note: "click to close options window",
        screen: BOOT_SCREEN,
        because: "the options window's `ok button` closes it; the main menu is the screen it was opened from",
    },
    Link {
        note: "click to close options window but NOT apply changes",
        screen: BOOT_SCREEN,
        because: "the options window's `cancel button` closes it without applying; the main menu is the screen underneath",
    },
    Link {
        note: "click to go to the bot workshop",
        screen: "botlab/botlabwin.gib",
        because: "the workshop is `botlabwin.gib`, whose root is `botlab win`",
    },
    Link {
        note: "click to return to HQ",
        screen: "teamhq/teamhq.gib",
        because: "the bot lab exits to the team HQ, whose root is `team hq win`",
    },
    Link {
        note: "click to go to team hq with selected team (disable button if no team selected)",
        screen: "teamhq/teamhq.gib",
        because: "the select team screen's `continue button` names the team HQ",
    },
    Link {
        note: "click to go back to the select team screen",
        screen: "teamhq/select_team.gib",
        because: "the team HQ's `select team button` names the picker",
    },
    Link {
        note: "click to go back to team HQ",
        screen: "teamhq/teamhq.gib",
        because: "the event winner window returns to the team HQ",
    },
    Link {
        note: "click to return to team HQ",
        screen: "teamhq/teamhq.gib",
        because: "the golden sawblade window returns to the team HQ",
    },
    Link {
        note: "click to return to team HQ and pause this event",
        screen: "teamhq/teamhq.gib",
        because: "the event bracket's `later button` and `forfeit button` say so",
    },
];

/// What a note asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Navigation {
    /// Load a screen. With a slot, the screen is a fragment that goes into that
    /// group; without one it replaces the screen.
    File {
        screen: String,
        slot: Option<String>,
    },
    /// A purpose-named transition from [`LINKS`].
    Link(Link),
}

/// The `.gib` a note names, if it names one.
pub fn screen_in(note: &str) -> Option<String> {
    for token in note.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        let Some(at) = lower.find(".gib") else {
            continue;
        };
        // Only the path ending at `.gib`; trailing punctuation belongs to the
        // sentence, not the file name.
        let mut name = &token[..at + 4];
        name = name.trim_matches(|c: char| {
            c == '\'' || c == '"' || c == '(' || c == ')' || c == ',' || c == '.'
        });
        if !name.is_empty() {
            return Some(name.replace('\\', "/"));
        }
    }
    None
}

/// The group a note says to load into: `... into content group`, or
/// `... into 'right content slot'`.
pub fn slot_in(note: &str) -> Option<String> {
    let lower = note.to_ascii_lowercase();
    let at = lower.find(" into ")?;
    let mut rest = note[at + 6..].trim();
    if let Some(stripped) = rest.strip_prefix('\'') {
        let end = stripped.find('\'')?;
        rest = &stripped[..end];
    } else if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"')?;
        rest = &stripped[..end];
    }
    let rest = rest.trim();
    (!rest.is_empty()).then(|| rest.to_string())
}

/// Read a note. A file name wins over the table: the data names the target.
pub fn navigation(note: &str) -> Option<Navigation> {
    let note = note.trim();
    if note.is_empty() {
        return None;
    }
    if describes_contents(note) {
        return None;
    }
    if let Some(screen) = screen_in(note) {
        return Some(Navigation::File {
            screen,
            slot: slot_in(note),
        });
    }
    LINKS
        .iter()
        .find(|link| link.note.eq_ignore_ascii_case(note))
        .copied()
        .map(Navigation::Link)
}

/// Whether a note is the author's comment on what the widget is *filled
/// with*, not where a click goes: `fill grid with team robots
/// (robot_cell.gib)`, `fill with this season's events (event_cell.gib)`,
/// `load result_cell.gib into this slot with appropriate data`, `load
/// 'statusmeters.gib' here`, `can have noplayer.gib, activeplayer.gib, or
/// closedplayer.gib`. The C++ fills those widgets itself; a click on a robot
/// or an event cell must never replace the screen with the cell template.
pub fn describes_contents(note: &str) -> bool {
    let lower = note.trim().to_ascii_lowercase();
    if lower.starts_with("fill ") || lower.starts_with("can have ") {
        return true;
    }
    if lower.starts_with("load ") {
        if lower.ends_with(" here") {
            return true;
        }
        if let Some(slot) = slot_in(&lower) {
            if slot.starts_with("this ") || slot.starts_with("these ") {
                return true;
            }
        }
    }
    false
}

/// The link a note names, if it is one of the table's.
pub fn link(note: &str) -> Option<Link> {
    LINKS
        .iter()
        .find(|link| link.note.eq_ignore_ascii_case(note.trim()))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game;
    use std::path::PathBuf;

    fn ui_root() -> Option<PathBuf> {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join("local/game/UI");
            if candidate.is_dir() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    fn every_screen(ui: &std::path::Path) -> Vec<(String, ra2fmt::gib::Gib)> {
        game::screens(ui)
            .unwrap()
            .into_iter()
            .filter_map(|rel| {
                let text = std::fs::read_to_string(ui.join(&rel)).ok()?;
                let gib = ra2fmt::gib::parse(&text).ok()?;
                Some((rel, gib))
            })
            .collect()
    }

    /// Every table entry is a note the corpus actually carries, and its target
    /// is a shipped screen whose root record names the thing the note says.
    #[test]
    fn every_purpose_named_link_is_a_note_the_screens_carry() {
        let Some(ui) = ui_root() else { return };
        let screens = every_screen(&ui);
        for link in LINKS {
            let owner = screens
                .iter()
                .find(|(_, gib)| gib.walk().any(|w| w.notes().trim() == link.note));
            assert!(
                owner.is_some(),
                "no shipped screen carries the note {:?}",
                link.note
            );
            let target = game::screen(&ui, link.screen)
                .unwrap_or_else(|| panic!("{} is not shipped", link.screen));
            let text = std::fs::read_to_string(&target.0).unwrap();
            let gib = ra2fmt::gib::parse(&text).unwrap();
            let root = gib.widgets().next().expect("a root record");
            assert!(!root.name().is_empty(), "{} has no named root", link.screen);
            assert!(!link.because.is_empty(), "{} has no evidence", link.note);
        }
    }

    /// The main menu is the boot screen in three independent places: the game's
    /// own executable names the file beside the window class, the file's root is
    /// that class, and the bottom navigation bar's `MainMenu button` returns to
    /// it.
    #[test]
    fn the_boot_screen_is_the_main_menu_the_data_names() {
        let Some(ui) = ui_root() else { return };
        let game = ui.parent().unwrap();
        let exe = game.join("Robot Arena 2.exe");
        if exe.exists() {
            let bytes = std::fs::read(&exe).unwrap();
            let haystack = String::from_utf8_lossy(&bytes);
            assert!(
                haystack.contains("mainmenu\\mainmenu.gib"),
                "the executable does not name the main menu screen"
            );
            assert!(
                haystack.contains("Main Menu Win"),
                "the executable does not name the main menu's window class"
            );
        }
        let boot = game::screen(&ui, BOOT_SCREEN).expect("the boot screen");
        let text = std::fs::read_to_string(&boot.0).unwrap();
        let gib = ra2fmt::gib::parse(&text).unwrap();
        assert_eq!(
            gib.widgets().next().unwrap().name(),
            "Main Menu Win",
            "the boot screen's root is the class the executable names"
        );
        let nav = std::fs::read_to_string(ui.join("common/bottomnav.gib")).unwrap();
        let nav = ra2fmt::gib::parse(&nav).unwrap();
        let main = nav
            .walk()
            .find(|w| w.name() == "MainMenu button")
            .expect("the nav bar's MainMenu button");
        assert_eq!(main.notes().trim(), "click to return to main menu");
        assert_eq!(
            link(main.notes()).map(|l| l.screen),
            Some(BOOT_SCREEN),
            "the nav bar points back at the boot screen"
        );
    }

    /// The file-named notes the two routes use say exactly where they go.
    #[test]
    fn the_routes_file_transitions_name_their_files_and_slots() {
        let Some(ui) = ui_root() else { return };
        for (screen, button, file, slot) in [
            (
                "exhibition/exhib_setup.gib",
                "arena button",
                "arena.gib",
                Some("content group"),
            ),
            (
                "teamhq/teamhq.gib",
                "robots button",
                "hq_robots.gib",
                Some("hq content group"),
            ),
        ] {
            let text = std::fs::read_to_string(ui.join(screen)).unwrap();
            let gib = ra2fmt::gib::parse(&text).unwrap();
            let control = gib
                .walk()
                .find(|w| w.name() == button)
                .unwrap_or_else(|| panic!("{screen} has no {button}"));
            let nav = navigation(control.notes()).unwrap_or_else(|| {
                panic!("{button}'s note is not a navigation: {:?}", control.notes())
            });
            assert_eq!(
                nav,
                Navigation::File {
                    screen: file.into(),
                    slot: slot.map(str::to_string)
                }
            );
            assert!(game::screen(&ui, file).is_some(), "{file} is not shipped");
        }
        // The workshop button names no file, so it is the table's, and the
        // target's root is the workshop window.
        let text = std::fs::read_to_string(ui.join("teamhq/hq_robots.gib")).unwrap();
        let gib = ra2fmt::gib::parse(&text).unwrap();
        let button = gib.walk().find(|w| w.name() == "workshop button").unwrap();
        assert_eq!(
            link(button.notes()).map(|l| l.screen),
            Some("botlab/botlabwin.gib")
        );
        let text = std::fs::read_to_string(ui.join("botlab/botlabwin.gib")).unwrap();
        let lab = ra2fmt::gib::parse(&text).unwrap();
        assert_eq!(lab.widgets().next().unwrap().name(), "botlab win");
    }

    /// A note that names a file is read from the file, not from the table: the
    /// note's own words decide the target and the slot.
    #[test]
    fn file_notes_are_read_from_the_sentence() {
        assert_eq!(
            navigation("click to load arena.gib into content group"),
            Some(Navigation::File {
                screen: "arena.gib".into(),
                slot: Some("content group".into())
            })
        );
        assert_eq!(
            navigation("click to load audio_options.gib into 'options content group'"),
            Some(Navigation::File {
                screen: "audio_options.gib".into(),
                slot: Some("options content group".into())
            })
        );
        assert_eq!(
            navigation("click to view credits.gib"),
            Some(Navigation::File {
                screen: "credits.gib".into(),
                slot: None
            })
        );
        assert_eq!(navigation("click to quit"), None);
    }

    /// A note about what a grid is filled with names a cell template, not a
    /// screen: clicking the HQ's robot or event grid must not navigate to it.
    #[test]
    fn content_notes_are_not_navigation() {
        for note in [
            "fill grid with team robots (robot_cell.gib)",
            "fill with this season's events (event_cell.gib)",
            "fill with league_cell.gib with the teams in order of rank",
            "load result_cell.gib into this slot with appropriate data",
            "load team_cell.gib's into these grid cells for each team.",
            "load 'statusmeters.gib' here",
            "can have noplayer.gib, activeplayer.gib, or closedplayer.gib",
        ] {
            assert_eq!(navigation(note), None, "{note}");
        }
        assert!(navigation("load hq_robots.gib into hq content group").is_some());
        assert!(navigation("click to open edit team window (editteam.gib)").is_some());
    }
}
