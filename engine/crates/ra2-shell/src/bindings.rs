//! The per-screen handler bindings recovered from `Robot Arena 2.exe`.
//!
//! The `.gib` `Notes` strings are editor comments; they do not occur in the
//! executable. What the executable has is a constructor per window that looks a
//! child widget up *by name* and binds a handler to it. This table is those
//! bindings, keyed by `(screen, widget name)`; [`crate::shell`] consults it
//! before it falls back to a `Notes` string, so a screen whose bindings are
//! recovered no longer depends on the notes at all. `docs/28-shell-screen-graph.md`
//! lists the addresses.

/// What a bound handler does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Switch the active screen (`FUN_005e5ba0`, which fades between windows;
    /// the first visit shows the "Loading..." splash while the window is built).
    Goto(&'static str),
    /// Open a window over the active screen and run it modally (`FUN_005e5620`:
    /// it stores the window as the modal, then pumps messages until it closes).
    Window(&'static str),
    /// Close the modal window (the modal pointer is cleared; the loop ends).
    Close,
    /// Leave the game.
    Quit,
}

/// The screens that open as a modal window over whatever is showing.
pub const WINDOWS: &[&str] = &["common/options.gib", "common/credits.gib"];

/// Whether a screen is one of the modal windows.
pub fn is_window(screen: &str) -> bool {
    WINDOWS.iter().any(|w| w.eq_ignore_ascii_case(screen))
}

/// One binding: `screen`'s widget `name` runs `action`.
pub struct Binding {
    pub screen: &'static str,
    pub name: &'static str,
    pub action: Action,
    /// Where the constructor binds it, and the handler it binds.
    pub at: &'static str,
}

/// Main-menu constructor `FUN_00486960`.
pub const BINDINGS: &[Binding] = &[
    Binding {
        screen: "mainmenu/mainmenu.gib",
        name: "Quit button",
        action: Action::Quit,
        at: "0x486960 -> handler 0x486c30",
    },
    Binding {
        screen: "mainmenu/mainmenu.gib",
        name: "Options button",
        action: Action::Window("common/options.gib"),
        at: "0x486960 -> FUN_00486c50: window `options win` from common\\options.gib, shown with FUN_005e5620 (modal)",
    },
    Binding {
        screen: "mainmenu/mainmenu.gib",
        name: "credits button",
        action: Action::Window("common/credits.gib"),
        at: "0x486960 -> FUN_00487260: window `credits win` from common\\credits.gib, shown with FUN_005e5690 -> FUN_005e5620 (modal)",
    },
    Binding {
        screen: "mainmenu/mainmenu.gib",
        name: "exhibition button",
        action: Action::Goto("exhibition/exhib_setup.gib"),
        at: "0x486960 -> FUN_00486d70: `Loading...`, ExhibitionSetupWindow(exhibition\\exhib_setup.gib), FUN_005e5ba0",
    },
    Binding {
        screen: "mainmenu/mainmenu.gib",
        name: "Team HQ button",
        action: Action::Goto("teamhq/select_team.gib"),
        at: "0x486960 -> FUN_00486f10: `Loading...`, SelectTeamWindow(teamhq\\select_team.gib), FUN_005e5ba0",
    },
    Binding {
        screen: "mainmenu/mainmenu.gib",
        name: "Multiplayer button",
        action: Action::Goto("multiplayer/multiplayer.gib"),
        at: "0x486960 -> FUN_004870b0: multiplayer win from multiplayer\\multiplayer.gib, FUN_005e5810",
    },
    // The modal windows' own buttons close them. The executable binds these in
    // the window classes' constructors; the names are the shipped records'.
    Binding {
        screen: "common/options.gib",
        name: "ok button",
        action: Action::Close,
        at: "OptionsWindow (applies, then closes)",
    },
    Binding {
        screen: "common/options.gib",
        name: "cancel button",
        action: Action::Close,
        at: "OptionsWindow (closes without applying)",
    },
    Binding {
        screen: "common/credits.gib",
        name: "close button",
        action: Action::Close,
        at: "CreditsWindow",
    },
];

/// The binding for a widget on a screen, compared case-insensitively.
pub fn lookup(screen: &str, name: &str) -> Option<&'static Binding> {
    if name.is_empty() {
        return None;
    }
    BINDINGS
        .iter()
        .find(|b| b.screen.eq_ignore_ascii_case(screen) && b.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_window_binding_target_is_a_window() {
        for b in BINDINGS {
            if let Action::Window(w) = b.action {
                assert!(is_window(w), "{w}");
            }
        }
    }
}
