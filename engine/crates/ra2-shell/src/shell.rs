//! The shell as a live state: one screen, its slots, and a hit-tested click.
//!
//! The boot screen is [`nav::BOOT_SCREEN`]. Everything after it is the data's:
//! [`Shell::click`] places the current screen, hit-tests the point against the
//! placed tree, walks up to the first widget whose `Notes` say something, and
//! applies what that note says (a file, a slot, or one of the purpose-named
//! links in [`nav::LINKS`]). Nothing here has a table of screen-to-screen
//! transitions except those purpose-named links, whose test re-reads every note
//! from the shipped files.
//!
//! The workshop is the one screen with a live model of its own
//! ([`ra2ui::workshop::BotLab`]), because its tabs are state: the toolbar's
//! images flip and the content slot swaps without rebuilding the file. Entering
//! `botlab/botlabwin.gib` hands the screen to that model; a purpose-named link
//! out of it (the exit button) hands it back.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ra2fmt::gib::{AttachmentKind, Block, Gib, Widget, WidgetKind};
use ra2ui::teams::{self, Roster, Side};
use ra2ui::workshop::{self, BotLab};
use ra2ui::{Fonts, Placement};

use crate::bindings::{self, Action};
use crate::game;
use crate::nav::{self, Navigation};

/// A page the shell shows: the file, its inlined proxies, and the fragments it
/// has loaded into named slots.
struct Page {
    key: String,
    gib: Gib,
    /// `(slot, screen)`, newest last: what a click loaded into a group. The
    /// current *screen* is still `key`; this is its content.
    content: Vec<(String, String)>,
    /// How many children each slot had before a fragment was loaded into it.
    /// A second load replaces the first rather than stacking on it.
    slot_base: BTreeMap<String, usize>,
    /// The scroll offset of each single-column list, by widget name. It is
    /// view state, not a record the writer owns.
    scroll: BTreeMap<String, usize>,
}

/// One robot the exhibition picker chose: the team's record slot and the
/// `.bot` slot number the team's own `Robots:` line names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pick {
    pub team: usize,
    pub bot: i32,
}

/// The shipped robot picker window, open over the setup screen.
struct Popup {
    side: Side,
    /// The `exhib_robots.gib` slot the choice lands in, 0-based
    /// (`player1 group` … `player4 group`).
    slot: usize,
    /// The slot group's own name (`player1 group`), which `apply_panel` writes
    /// the accepted cell into.
    panel: String,
    gib: Gib,
    rosters: Vec<Roster>,
    /// The record slot selected in the dropdown.
    team: usize,
    /// The ordinal in the selected roster's `Robots:` line, not the `.bot`
    /// slot number.
    robot: Option<usize>,
    /// The dropdown list's scroll offset.
    scroll: usize,
}

enum Screen {
    Page(Page),
    Lab(BotLab),
}

/// The 0-based `playerN group` index, from the slot group's own name.
fn panel_slot(panel: &str) -> Option<usize> {
    match panel {
        "player1 group" => Some(0),
        "player2 group" => Some(1),
        "player3 group" => Some(2),
        "player4 group" => Some(3),
        _ => None,
    }
}

/// What one click did, and what the screen became.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub command: String,
    /// The innermost widget the point landed on.
    pub hit: String,
    /// The note that decided the transition, if one did.
    pub note: Option<String>,
    /// The screen after the click, relative to `UI`.
    pub screen: String,
    /// The fragment in the page's slot, or the workshop's own tab/step screen.
    pub content: Option<String>,
    /// The workshop tab, when the screen is the workshop.
    pub tab: Option<String>,
    /// The workshop step, when one is active.
    pub step: Option<String>,
    /// The point actually clicked (a named click finds its own point).
    pub point: [f32; 2],
}

impl Step {
    /// The `Name` of the widget the click landed on, from the trace string the
    /// shell writes (`Button "start game button"`). A nameless widget is
    /// `None`; the class is still in [`Step::hit`].
    pub fn hit_widget(&self) -> Option<&str> {
        let start = self.hit.find('"')? + 1;
        let rest = &self.hit[start..];
        let end = rest.find('"')?;
        Some(&rest[..end])
    }
}

/// A screen written to a PNG.
#[derive(Debug, Clone, PartialEq)]
pub struct Shot {
    pub path: PathBuf,
    pub size: [u32; 2],
    pub quads: usize,
    pub textures: usize,
    pub missing: usize,
}

/// The shell: one live screen and the game it reads.
pub struct Shell {
    game: PathBuf,
    ui: PathBuf,
    fonts: Fonts,
    size: [f32; 2],
    screen: Screen,
    trace: Vec<Step>,
    /// The arena script the picker last selected (`box.py`). The choice
    /// survives loading another content tab: the arena list and the game-type
    /// tab share `content group`, so the list may not be on screen when Start
    /// is pressed.
    arena_choice: Option<String>,
    /// The row the arena list selected, and how many rows it offered, for the
    /// headless report.
    arena_row: Option<usize>,
    arena_rows: Option<usize>,
    /// The arena list's last scroll offset (0 when nothing scrolled).
    arena_scroll: Option<usize>,
    /// The game type the picker last selected, in the original's spelling.
    game_type_choice: Option<String>,
    /// The arena's `hazard checkbox`, when the loaded arena script has one.
    hazard_choice: Option<bool>,
    /// The match-time dropdown's own visible text (`5 minutes`), and the
    /// seconds it states. `Some(None)` is a "No limit" value; `None` means the
    /// tab was never loaded.
    time_text: Option<String>,
    time_seconds: Option<Option<f32>>,
    /// The robots the player and the computer picked, from the shipped
    /// `selectbot_player.gib`/`selectbot_computer.gib` windows. `None` means
    /// the robots tab was not used; the caller's defaults apply.
    player_pick: Option<Pick>,
    opponent_pick: Option<Pick>,
    /// One pick per `playerN group` slot, 0-based. Slots 1 and 3 are the same
    /// records as `player_pick`/`opponent_pick`; slots 2 and 4 feed a four-robot
    /// Battle Royal or the second robot of a 2v2 Team Match.
    slot_picks: [Option<Pick>; 4],
    /// Which table each slot's robot came from. The slot's own join buttons
    /// choose it (`human player button` / `computer player button`); an unset
    /// slot is the screen's team-1/team-2 default.
    slot_sides: [Option<Side>; 4],
    /// The picked robots' own `.bot` snapshots, by the texture key the slot's
    /// `bot img` carries. The renderer registers them before it draws.
    previews: BTreeMap<String, ra2ui::preview::Preview>,
    /// The picker window open over the setup screen.
    popup: Option<Popup>,
    /// The modal windows open over the screen, oldest first (the options and
    /// credits windows over the main menu). The top one owns the input; the
    /// screen under it keeps its state and keeps drawing.
    windows: Vec<Page>,
    /// The career the Team HQ draws (roster, standings, schedule, history), set
    /// by the application. Without it the HQ shows the shipped records' text.
    hq_view: Option<ra2ui::hq::View>,
    /// The `(content count, fragment)` the HQ was last filled for.
    hq_filled: Option<(usize, String)>,
    /// Whether screen switches animate (the window does; scripts and tests
    /// switch instantly so a frame is always the settled screen).
    animate: bool,
    /// The switch in progress: the screen being left, drawn under the 0.2 s
    /// fade-out, then the new one under the fade-in.
    fade: Option<Fade>,
    /// The screens whose first visit showed the "Loading..." splash.
    visited: std::collections::BTreeSet<String>,
    /// The select-team list's edits. The install's `Teams/` is read-only data,
    /// so a delete or a new team lives here, for the session.
    deleted_teams: std::collections::BTreeSet<usize>,
    new_teams: Vec<teams::Roster>,
    /// The team the open delete box is about.
    pending_delete: Option<usize>,
    /// The schedule row the open join-event window is about (`FUN_00500c40`).
    join_event: Option<usize>,
    /// Enters an event in the application's career and returns the Team HQ's
    /// refreshed data (the bracket exists from then on).
    enter_hook: Option<EnterHook>,
    /// The text box the last click landed on (on the top window, or the
    /// screen when no window is open): typed characters go there.
    text_focus: Option<String>,
    /// The event the join window entered this session (`Events.txt` id) and
    /// the robot column chosen in its grid.
    entered_event: Option<(usize, usize)>,
    /// A message box waiting to open: `(caption, body)` (`FUN_005e5e40`).
    pending_message: Option<(String, String)>,
}

/// The longest text a shell edit field takes.
pub const MAX_TEXT: usize = 32;

/// What the application does when the player enters an event: the event's
/// `Events.txt` index and the join window's chosen robot column in, the Team
/// HQ's data out.
pub type EnterHook = Box<dyn FnMut(usize, Option<usize>) -> Option<ra2ui::hq::View>>;

/// `FUN_005e5ba0`'s screen switch: black overlays of 0.2 s each way. On a
/// screen's first visit `FUN_004e6bd0` first shows the `common/loading.gib`
/// window ("Loading...") over the screen being left while the heavy window is
/// built.
struct Fade {
    from: Screen,
    loading: Option<Gib>,
    overlay: Gib,
    t: f32,
}

/// The fade's duration each way (`0x3e4ccccd` = 0.2).
pub const FADE_SECONDS: f32 = 0.2;
/// How long the splash stays up on a first visit. The original shows it for as
/// long as the window takes to construct, which is not a fixed number; this is
/// a typical load.
pub const LOADING_SECONDS: f32 = 0.6;

fn overlay_gib(alpha: f32) -> Gib {
    let text = format!(
        "T 1.09\n[Window]\nDraggable false\nAlignment 0\nPassMouse false\nNotes fade overlay\nName fade overlay\nID -1\nPosition 0 0 0 0\nSize 1 1 1 1\nVisible true\nChildOrder -1\nClipMode 0\nNumChildren 1\n[Image]\n_Tiling false\n_Filename \n_BufferWidth 0 0 0 0\n_Size 1 0 1 0\n_Alpha true\n_Filtering 0\n_ForeColor 0 0 0 {alpha}\n_Name \n_Position 0 0 0 0\n0 0 1 1\nName \nID -1\nPosition 0 0 0 0\nSize 1 1 1 1\nVisible true\nChildOrder 0\nClipMode 0\nNumChildren 0\nPassMouse false\nNotes \n"
    );
    ra2fmt::gib::parse(&text).expect("the overlay record parses")
}

impl Shell {
    /// Boot the shell on the main menu.
    pub fn boot(game: impl AsRef<Path>, size: [f32; 2]) -> Result<Shell, String> {
        let game = game
            .as_ref()
            .canonicalize()
            .map_err(|e| format!("{}: {e}", game.as_ref().display()))?;
        game::check(&game)?;
        let ui = game.join("UI");
        let fonts = Fonts::load(&game.join("RA2.cfz"))
            .map_err(|e| format!("{}: {e}", game.join("RA2.cfz").display()))?;
        let screen = load_screen(&ui, nav::BOOT_SCREEN)?;
        Ok(Shell {
            game,
            ui,
            fonts,
            size,
            screen,
            trace: Vec::new(),
            arena_choice: None,
            arena_row: None,
            arena_rows: None,
            arena_scroll: None,
            game_type_choice: None,
            hazard_choice: None,
            time_text: None,
            time_seconds: None,
            player_pick: None,
            opponent_pick: None,
            slot_picks: [None; 4],
            slot_sides: [None; 4],
            previews: BTreeMap::new(),
            popup: None,
            windows: Vec::new(),
            hq_view: None,
            hq_filled: None,
            animate: false,
            fade: None,
            visited: Default::default(),
            deleted_teams: Default::default(),
            new_teams: Vec::new(),
            pending_delete: None,
            join_event: None,
            text_focus: None,
            entered_event: None,
            enter_hook: None,
            pending_message: None,
        })
    }

    /// Animate screen switches (the window turns this on).
    pub fn set_animate(&mut self, on: bool) {
        self.animate = on;
        if !on {
            self.fade = None;
        }
    }

    /// Whether a screen switch is still playing.
    pub fn transitioning(&self) -> bool {
        self.fade.is_some()
    }

    /// Whether the "Loading..." splash is up.
    pub fn loading_visible(&self) -> bool {
        self.fade
            .as_ref()
            .is_some_and(|f| f.loading.is_some() && f.t < LOADING_SECONDS)
    }

    /// Advance the switch by `dt` seconds.
    pub fn advance(&mut self, dt: f32) {
        let Some(fade) = &mut self.fade else { return };
        fade.t += dt;
        let load = if fade.loading.is_some() {
            LOADING_SECONDS
        } else {
            0.0
        };
        if fade.t >= load + 2.0 * FADE_SECONDS {
            self.fade = None;
            return;
        }
        let u = (fade.t - load) / FADE_SECONDS;
        let alpha = if u < 0.0 {
            0.0
        } else if u < 1.0 {
            u
        } else {
            2.0 - u
        };
        fade.overlay = overlay_gib(alpha.clamp(0.0, 1.0));
    }

    /// Switch to a freshly loaded screen, with the original's loading splash and
    /// fade when animating.
    fn switch_to(&mut self, screen: Screen, key: &str) {
        let from = std::mem::replace(&mut self.screen, screen);
        if !self.animate {
            return;
        }
        let first = self.visited.insert(key.to_ascii_lowercase());
        let heavy = ["exhibition/exhib_setup.gib", "teamhq/select_team.gib"]
            .iter()
            .any(|k| k.eq_ignore_ascii_case(key));
        let loading = if first && heavy {
            game::screen(&self.ui, "common/loading.gib").and_then(|(path, _)| {
                let text = std::fs::read_to_string(path).ok()?;
                let mut gib = ra2fmt::gib::parse(&text).ok()?;
                ra2ui::inline_proxies_in(&mut gib, &self.ui);
                Some(gib)
            })
        } else {
            None
        };
        self.fade = Some(Fade {
            from,
            loading,
            overlay: overlay_gib(0.0),
            t: 0.0,
        });
    }

    /// Give the Team HQ its career: the tabs are filled from it as they load.
    /// The robots' own snapshots are registered with the shell's previews.
    /// Let the application's career take the entered event (the bracket is
    /// drawn from its answer).
    /// The event entered through the join window this session, with the
    /// robot column chosen in its grid.
    pub fn entered_event(&self) -> Option<(usize, usize)> {
        self.entered_event
    }

    /// The text box typed characters go to, if one has the focus.
    pub fn text_focus(&self) -> Option<&str> {
        self.text_focus.as_deref()
    }

    /// The focused text box's text.
    pub fn focused_text(&self) -> Option<String> {
        let name = self.text_focus.as_deref()?;
        let gib = match self.windows.last() {
            Some(window) => &window.gib,
            None => match &self.screen {
                Screen::Page(page) => &page.gib,
                Screen::Lab(_) => return None,
            },
        };
        gib.blocks.iter().find_map(|b| match b {
            Block::Widget(root) => workshop::find(root, name).map(text_of),
            _ => None,
        })
    }

    /// Whether the focused text box is one the player may type in (its
    /// record's own `Editable true`; labels such as a robot's weight class
    /// are text boxes too).
    fn focused_is_editable(&self) -> bool {
        let Some(name) = self.text_focus.as_deref() else {
            return false;
        };
        let gib = match self.windows.last() {
            Some(window) => &window.gib,
            None => match &self.screen {
                Screen::Page(page) => &page.gib,
                Screen::Lab(_) => return false,
            },
        };
        gib.blocks.iter().any(|b| match b {
            Block::Widget(root) => workshop::find(root, name).is_some_and(|w| {
                ra2ui::input::field_of(w, "Editable").is_some_and(|v| v.trim() == "true")
            }),
            _ => false,
        })
    }

    /// Type into the focused text box (the edit-team window's name and motto,
    /// the multiplayer fields). Printable ASCII only; the field keeps at most
    /// `MAX_TEXT` characters.
    pub fn type_char(&mut self, c: char) -> bool {
        if !(c.is_ascii_graphic() || c == ' ') {
            return false;
        }
        self.edit_focused(|text| {
            if text.chars().count() < MAX_TEXT {
                text.push(c);
            }
        })
    }

    /// Backspace in the focused text box.
    pub fn backspace(&mut self) -> bool {
        self.edit_focused(|text| {
            text.pop();
        })
    }

    fn edit_focused(&mut self, edit: impl FnOnce(&mut String)) -> bool {
        let Some(name) = self.text_focus.clone() else {
            return false;
        };
        let gib = match self.windows.last_mut() {
            Some(window) => &mut window.gib,
            None => match &mut self.screen {
                Screen::Page(page) => &mut page.gib,
                Screen::Lab(_) => return false,
            },
        };
        for block in gib.blocks.iter_mut() {
            if let Block::Widget(root) = block {
                if let Some(widget) = workshop::find_mut(root, &name) {
                    let mut text = text_of(widget);
                    edit(&mut text);
                    set_text(widget, &text);
                    return true;
                }
            }
        }
        false
    }

    pub fn set_enter_hook(&mut self, hook: EnterHook) {
        self.enter_hook = Some(hook);
    }

    pub fn set_hq_view(&mut self, view: ra2ui::hq::View) {
        for row in &view.robots {
            if let Some(preview) = &row.preview_image {
                self.previews.insert(row.preview.clone(), preview.clone());
            }
        }
        self.hq_view = Some(view);
        self.hq_filled = None;
        let _ = self.fill_hq();
    }

    /// Fill the Team HQ page from the career when a tab was just loaded.
    fn fill_hq(&mut self) -> Result<(), String> {
        let Some(view) = &self.hq_view else {
            return Ok(());
        };
        let Screen::Page(page) = &mut self.screen else {
            return Ok(());
        };
        if !page.key.eq_ignore_ascii_case(ra2ui::hq::SCREEN) {
            self.hq_filled = None;
            return Ok(());
        }
        let Some((_, fragment)) = page.content.last() else {
            return Ok(());
        };
        let stamp = (page.content.len(), fragment.clone());
        if self.hq_filled.as_ref() == Some(&stamp) {
            return Ok(());
        }
        let Some(tab) = ra2ui::hq::TABS
            .iter()
            .find(|t| t.fragment.eq_ignore_ascii_case(fragment))
        else {
            return Ok(());
        };
        ra2ui::hq::fill_document(&self.ui, &mut page.gib, tab.button, view)?;
        self.hq_filled = Some(stamp);
        Ok(())
    }

    pub fn game(&self) -> &Path {
        &self.game
    }

    pub fn size(&self) -> [f32; 2] {
        self.size
    }

    /// Re-lay the shell out at a new screen size. The original lays its
    /// screens out natively at the display resolution, so a resize changes the
    /// layout, not a scale.
    pub fn set_size(&mut self, size: [f32; 2]) {
        self.size = size;
        self.sync_list_viewports();
    }

    /// The screen on display, relative to `UI`.
    pub fn screen_id(&self) -> String {
        if let Some(window) = self.windows.last() {
            return window.key.clone();
        }
        self.base_screen_id()
    }

    /// The screen under any modal window: what the music and the flow follow.
    pub fn base_screen_id(&self) -> String {
        match &self.screen {
            Screen::Page(page) => page.key.clone(),
            Screen::Lab(_) => workshop::SCREEN.to_string(),
        }
    }

    /// The fragment in the page's slot, or the workshop's own tab/step screen.
    /// A picker popup in front of the page is the content the trace names.
    pub fn content_id(&self) -> Option<String> {
        if let Some(popup) = &self.popup {
            return Some(popup.side.picker().to_string());
        }
        if let Some(window) = self.windows.last() {
            return window.content.last().map(|(_, screen)| screen.clone());
        }
        match &self.screen {
            Screen::Page(page) => page.content.last().map(|(_, screen)| screen.clone()),
            Screen::Lab(lab) => {
                if let Some(button) = lab.step {
                    workshop::step(button).map(|step| step.fragment.to_string())
                } else {
                    workshop::tab(lab.tab).map(|tab| tab.fragment.to_string())
                }
            }
        }
    }

    /// The workshop tab, when the workshop is on screen.
    pub fn tab(&self) -> Option<&str> {
        match &self.screen {
            Screen::Page(_) => None,
            Screen::Lab(lab) => Some(lab.tab),
        }
    }

    /// The workshop step, when one is active.
    pub fn step(&self) -> Option<&str> {
        match &self.screen {
            Screen::Page(_) => None,
            Screen::Lab(lab) => lab.step,
        }
    }

    /// The composed document, for reading back what a screen now says.
    pub fn document(&self) -> &Gib {
        if let Some(window) = self.windows.last() {
            return &window.gib;
        }
        match &self.screen {
            Screen::Page(page) => &page.gib,
            Screen::Lab(lab) => &lab.document,
        }
    }

    /// The composed document, to write what the screen displays.
    ///
    /// The options screen is the caller: its sliders, checkboxes and dropdown
    /// values are the `ra2.cfg` settings, so the app mirrors the file into the
    /// shipped records before each frame. The workshop's document belongs to
    /// `ra2ui`, so it is not exposed here.
    pub fn document_mut(&mut self) -> Option<&mut Gib> {
        if let Some(window) = self.windows.last_mut() {
            return Some(&mut window.gib);
        }
        match &mut self.screen {
            Screen::Page(page) => Some(&mut page.gib),
            Screen::Lab(_) => None,
        }
    }

    /// The workshop, when it is on screen. The application's build mode owns the
    /// model and fills the lab's lists through this view; the screen itself stays
    /// `ra2ui::workshop`'s.
    pub fn lab(&self) -> Option<&BotLab> {
        match &self.screen {
            Screen::Page(_) => None,
            Screen::Lab(lab) => Some(lab),
        }
    }

    /// The workshop, to write the lists the screens say to fill.
    pub fn lab_mut(&mut self) -> Option<&mut BotLab> {
        match &mut self.screen {
            Screen::Page(_) => None,
            Screen::Lab(lab) => Some(lab),
        }
    }

    /// A point inside `name` that a click reaches it at, found by hit-testing.
    pub fn point_for(&self, name: &str) -> Result<[f32; 2], String> {
        self.reachable(name).map(|(x, y)| [x, y])
    }

    pub fn trace(&self) -> &[Step] {
        &self.trace
    }

    /// The arena script the picker selected, as `Arenas` writes it (`box.py`).
    /// `None` means the screen has not selected a row; the caller's default is
    /// `Arenas/box.py`.
    pub fn arena_choice(&self) -> Option<&str> {
        self.arena_choice.as_deref()
    }

    /// The arena row the list selected (0-based) and how many rows it showed
    /// when it did.
    pub fn arena_row(&self) -> Option<(usize, usize)> {
        Some((self.arena_row?, self.arena_rows?))
    }

    /// The arena list's last scroll offset; `Some(0)` means the list was
    /// loaded but not scrolled.
    pub fn arena_scroll(&self) -> Option<usize> {
        self.arena_scroll
    }

    /// The game type the game-type tab selected, in the original's spelling
    /// (`DEATHMATCH`), or `None` when that tab has never been loaded.
    pub fn game_type_choice(&self) -> Option<&str> {
        self.game_type_choice.as_deref()
    }

    /// The arena's `hazard checkbox`, when its fragment is loaded: `Some(true)`
    /// is checked ("Hazards enabled").
    pub fn hazard_choice(&self) -> Option<bool> {
        self.hazard_choice
    }

    /// The match-time dropdown's own visible text (`5 minutes`), when the
    /// Game Settings tab has been loaded.
    pub fn time_text(&self) -> Option<&str> {
        self.time_text.as_deref()
    }

    /// The match length the dropdown displays: `Some(Some(seconds))` for
    /// "N minutes", `Some(None)` for "No limit", `None` when the tab was not
    /// loaded. The file does not carry the dropdown's option list, only the
    /// value it currently shows.
    pub fn time_choice(&self) -> Option<Option<f32>> {
        self.time_seconds
    }

    /// The player's robot from the shipped picker, or `None` when the robots
    /// tab was never used.
    pub fn player_pick(&self) -> Option<Pick> {
        self.player_pick
    }

    /// The computer's robot from the shipped picker.
    pub fn opponent_pick(&self) -> Option<Pick> {
        self.opponent_pick
    }

    /// The table slot `slot` (0…3) draws its robot from.
    pub fn slot_side(&self, slot: usize) -> Side {
        self.slot_sides
            .get(slot)
            .copied()
            .flatten()
            .unwrap_or(if slot < 2 {
                Side::Player
            } else {
                Side::Computer
            })
    }

    /// The pictures the screen's `bot img` widgets name by synthetic key.
    pub fn previews(&self) -> &BTreeMap<String, ra2ui::preview::Preview> {
        &self.previews
    }

    /// The robot chosen in `playerN group` (`slot` 0…3).
    pub fn slot_pick(&self, slot: usize) -> Option<Pick> {
        self.slot_picks.get(slot).copied().flatten()
    }

    /// The picker window in front of the page, when one is open.
    pub fn popup_id(&self) -> Option<&'static str> {
        self.popup.as_ref().map(|popup| popup.side.picker())
    }

    /// Click one row of a `ListBox`/`Grid`, by the list's own geometry.
    ///
    /// A row below the list's viewport is scrolled into view first — the list's
    /// own `Scroller` state is what `--arena-index 10` exercises — and then the
    /// click goes through [`Shell::click`], so the row selection is recorded in
    /// the same document the frame draws.
    pub fn click_list_row(&mut self, list_name: &str, row: usize) -> Result<Step, String> {
        let (grid, visible) = {
            let placed = self.placements();
            let list = placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == list_name)
                .ok_or_else(|| format!("no widget called {list_name:?} on {}", self.screen_id()))?;
            let grid = ra2ui::Grid::from_widget(list.widget)
                .ok_or_else(|| format!("{list_name:?} is not a list"))?;
            if grid.row_height <= 0.0 {
                return Err(format!("{list_name:?} states no row height"));
            }
            let visible = ra2ui::input::visible_rows(list.widget).max(1);
            (grid, visible)
        };
        let offset = self.list_scroll(list_name);
        if row < offset || row >= offset + visible {
            let target = if row < offset {
                row
            } else {
                (row + 1).saturating_sub(visible)
            };
            self.set_list_scroll(list_name, target)?;
        }
        let placed = self.placements();
        let list = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == list_name)
            .ok_or_else(|| format!("no widget called {list_name:?} on {}", self.screen_id()))?;
        let top = ra2ui::input::field_of(list.widget, "_ScrollTop")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        // The visible list, not the authored `ColWidths` sum: the arena list's
        // 300+17 is clipped to the 159-pixel frame the scroller gives it.
        let x = list.frame.x + list.frame.w / 2.0;
        let y = list.frame.y + grid.row_height * (row - top) as f32 + grid.row_height / 2.0;
        if let Some(clip) = list.clip {
            if !clip.contains_point(x, y) {
                return Err(format!(
                    "row {row} of {list_name:?} is clipped out of view ({x:.1},{y:.1})"
                ));
            }
        }
        self.click(x, y)
    }

    /// Click the picker's own `team dropdown` row.
    pub fn click_popup_team(&mut self, row: usize) -> Result<Step, String> {
        let (grid, visible, top) = {
            let popup = self
                .popup
                .as_ref()
                .ok_or_else(|| "no robot picker is open".to_string())?;
            let placed = ra2ui::resolve_with(&popup.gib, self.size, Some(&self.fonts));
            let list = picker_team_list_placed(&placed)
                .ok_or_else(|| "the picker has no team list".to_string())?;
            let grid = ra2ui::Grid::from_widget(list.widget)
                .ok_or_else(|| "the picker's team list is not a list".to_string())?;
            let visible = ra2ui::input::visible_rows(list.widget).max(1);
            (grid, visible, popup.scroll)
        };
        let offset = top;
        if row < offset || row >= offset + visible {
            let target = if row < offset {
                row
            } else {
                (row + 1).saturating_sub(visible)
            };
            self.set_popup_scroll(target);
        }
        let popup = self
            .popup
            .as_ref()
            .ok_or_else(|| "no robot picker is open".to_string())?;
        let placed = ra2ui::resolve_with(&popup.gib, self.size, Some(&self.fonts));
        let list = picker_team_list_placed(&placed)
            .ok_or_else(|| "the picker has no team list".to_string())?;
        let top = ra2ui::input::field_of(list.widget, "_ScrollTop")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        let x = list.frame.x + list.frame.w / 2.0;
        let y = list.frame.y + grid.row_height * (row - top) as f32 + grid.row_height / 2.0;
        self.click(x, y)
    }

    /// Click column `column` of the picker's `robots grid`.
    pub fn click_popup_robot(&mut self, column: usize) -> Result<Step, String> {
        let popup = self
            .popup
            .as_ref()
            .ok_or_else(|| "no robot picker is open".to_string())?;
        let placed = ra2ui::resolve_with(&popup.gib, self.size, Some(&self.fonts));
        let grid = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "robots grid")
            .ok_or_else(|| "the picker has no robots grid".to_string())?;
        let model = ra2ui::Grid::from_widget(grid.widget)
            .ok_or_else(|| "robots grid is not a grid".to_string())?;
        let x = grid.frame.x + model.column_x(column as i32) + model.row_height / 2.0;
        let y = grid.frame.y + model.row_height / 2.0;
        self.click(x, y)
    }

    /// The arena list's scroll-bar click, as a target offset.
    fn arena_scrollbar_at(&self, x: f32, y: f32) -> Option<usize> {
        let placed = self.placements();
        let list = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == ra2ui::arenas::LIST)?;
        let count = ra2ui::input::content_count(list.widget);
        let visible = ra2ui::input::visible_rows(list.widget).max(1);
        let max = count.saturating_sub(visible);
        scrollbar_target(list, x, y, self.list_scroll(ra2ui::arenas::LIST), max)
    }

    /// Scroll the list under `x, y` by `rows` (positive scrolls down). Returns
    /// whether a scrollable list was under the point.
    ///
    /// The wheel and the scrollbar share this; the popup's team list and the
    /// setup screen's arena list are the two live lists.
    pub fn scroll_at(&mut self, x: f32, y: f32, rows: i32) -> bool {
        if self.popup.is_some() {
            let top = {
                let popup = self.popup.as_ref().unwrap();
                let placed = ra2ui::resolve_with(&popup.gib, self.size, Some(&self.fonts));
                let Some(list) = picker_team_list_placed(&placed) else {
                    return false;
                };
                if !list.widget.visible() || !list.frame.contains_point(x, y) {
                    return false;
                }
                popup.scroll as i32
            };
            let target = (top + rows).max(0) as usize;
            self.set_popup_scroll(target);
            return true;
        }
        let top = {
            let placed = self.placements();
            let Some(list) = placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == ra2ui::arenas::LIST)
            else {
                return false;
            };
            if !list.frame.contains_point(x, y) {
                return false;
            }
            self.list_scroll(ra2ui::arenas::LIST) as i32
        };
        let target = (top + rows).max(0) as usize;
        self.set_list_scroll(ra2ui::arenas::LIST, target).is_ok()
    }

    fn list_scroll(&self, list: &str) -> usize {
        match &self.screen {
            Screen::Page(page) => page.scroll.get(list).copied().unwrap_or(0),
            Screen::Lab(_) => 0,
        }
    }

    fn set_list_scroll(&mut self, list: &str, offset: usize) -> Result<usize, String> {
        let Screen::Page(page) = &mut self.screen else {
            return Err("no page to scroll".to_string());
        };
        let mut applied = offset;
        for block in page.gib.blocks.iter_mut() {
            let Block::Widget(root) = block else { continue };
            let Some(widget) = workshop::find_mut(root, list) else {
                continue;
            };
            applied = ra2ui::input::scroll_rows(widget, offset);
            break;
        }
        page.scroll.insert(list.to_string(), applied);
        if list == ra2ui::arenas::LIST {
            self.arena_scroll = Some(applied);
        }
        Ok(applied)
    }

    fn set_popup_scroll(&mut self, offset: usize) {
        let Some(popup) = &mut self.popup else { return };
        let Some(root) = root_widget_mut(&mut popup.gib) else {
            return;
        };
        let Some(list) = picker_team_list_mut(root) else {
            return;
        };
        popup.scroll = ra2ui::input::scroll_rows(list, offset);
    }

    /// The placed tree, with the screen's own faces. A picker popup is placed
    /// after the page, so it draws and hit-tests in front of it.
    pub fn placements(&self) -> Vec<Placement<'_>> {
        if let Some(fade) = &self.fade {
            let load = if fade.loading.is_some() {
                LOADING_SECONDS
            } else {
                0.0
            };
            let faces = Some(&self.fonts);
            fn screen_placed<'a>(
                screen: &'a Screen,
                size: [f32; 2],
                faces: Option<&Fonts>,
            ) -> Vec<Placement<'a>> {
                match screen {
                    Screen::Page(page) => ra2ui::resolve_with(&page.gib, size, faces),
                    Screen::Lab(lab) => lab.placements(size, faces),
                }
            }
            if fade.t < load {
                let mut placed = screen_placed(&fade.from, self.size, faces);
                if let Some(loading) = &fade.loading {
                    let mut above = ra2ui::resolve_with(loading, self.size, faces);
                    above.append(&mut placed);
                    placed = above;
                }
                return placed;
            }
            let base = if fade.t < load + FADE_SECONDS {
                &fade.from
            } else {
                &self.screen
            };
            let mut placed = screen_placed(base, self.size, faces);
            let mut above = ra2ui::resolve_with(&fade.overlay, self.size, faces);
            above.append(&mut placed);
            return above;
        }
        let mut placed = match &self.screen {
            Screen::Page(page) => ra2ui::resolve_with(&page.gib, self.size, Some(&self.fonts)),
            Screen::Lab(lab) => lab.placements(self.size, Some(&self.fonts)),
        };
        // On a tie of `ChildOrder` the earlier record is in front, so the
        // windows go ahead of the screen, the newest first.
        for window in &self.windows {
            let mut above = ra2ui::resolve_with(&window.gib, self.size, Some(&self.fonts));
            above.append(&mut placed);
            placed = above;
        }
        if let Some(popup) = &self.popup {
            placed.extend(ra2ui::resolve_with(
                &popup.gib,
                self.size,
                Some(&self.fonts),
            ));
        }
        placed
    }

    /// Click at a point, hit-tested against the placed tree.
    pub fn click(&mut self, x: f32, y: f32) -> Result<Step, String> {
        let mut step = Step {
            command: format!("click {x} {y}"),
            hit: String::new(),
            note: None,
            screen: String::new(),
            content: None,
            tab: None,
            step: None,
            point: [x, y],
        };
        if self.popup.is_some() {
            return self.click_popup(x, y);
        }
        // The arena list's own scroll bar: clicking its arrows moves one row,
        // and clicking the track jumps to that fraction. The bar is the
        // document's `+Scroller` attachment, so it is not a `hit_control`.
        if let Some(target) = self.arena_scrollbar_at(x, y) {
            self.set_list_scroll(ra2ui::arenas::LIST, target)?;
            step.hit = "ScrollBar".to_string();
            return Ok(self.finish(step));
        }
        // `Eject` ("click to remove this player from the match") empties the slot.
        if let Some(slot) = self.eject_slot_at(x, y) {
            self.slot_picks[slot] = None;
            self.slot_sides[slot] = None;
            match slot {
                0 => self.player_pick = None,
                2 => self.opponent_pick = None,
                _ => {}
            }
            step.hit = "eject button".to_string();
            self.refresh_robot_slots()?;
            return Ok(self.finish(step));
        }
        // A click on a `load appropriate gib for player N here` region opens the
        // shipped picker window. The region is the screen's own group; no
        // control is under the point, so this is the picker's entry point.
        if let Some((side, panel, hit, note)) = self.picker_region(x, y)? {
            let step = Step {
                command: format!("click {x} {y}"),
                hit,
                note: Some(note),
                screen: String::new(),
                content: None,
                tab: None,
                step: None,
                point: [x, y],
            };
            self.open_picker(side, panel, x, y)?;
            return Ok(self.finish(step));
        }
        if self.windows.is_empty() {
            if let Some(index) = self.event_cell_at(x, y) {
                step.hit = "event grid".to_string();
                self.click_event_cell(index)?;
                return Ok(self.finish(step));
            }
        }
        if !self.windows.is_empty() {
            // A modal window owns the input: a click outside its controls is
            // swallowed, not passed to the screen under it.
            let game_type = self.game_type_choice.clone();
            let window = self.windows.last_mut().expect("a window is open");
            let transition = click_page(
                &self.ui,
                &self.game,
                &self.fonts,
                self.size,
                window,
                x,
                y,
                &mut step,
                game_type.as_deref(),
            )?;
            let confirmed = step
                .note
                .as_deref()
                .is_some_and(|n| n.starts_with("confirm "));
            match transition {
                Some(Transition::Open(target)) => self.open_window(&target)?,
                // Any screen change a window asks for closes it: the screen
                // under it is the one it was opened from.
                Some(Transition::Close | Transition::Replace(_)) => {
                    if let Some(window) = self.windows.pop() {
                        if confirmed {
                            self.confirm_window(&window)?;
                        }
                    }
                }
                None => {}
            }
            self.refresh_join_window();
            return Ok(self.finish(step));
        }
        let transition = match &mut self.screen {
            Screen::Page(page) => click_page(
                &self.ui,
                &self.game,
                &self.fonts,
                self.size,
                page,
                x,
                y,
                &mut step,
                self.game_type_choice.as_deref(),
            )?,
            Screen::Lab(lab) => {
                click_lab(&self.fonts, self.size, lab, x, y, &mut step)?.map(Transition::Replace)
            }
        };
        match transition {
            Some(Transition::Replace(target)) if bindings::is_window(&target) => {
                self.open_window(&target)?;
            }
            Some(Transition::Replace(target)) => {
                self.fade = None;
                let screen = load_screen(&self.ui, &target)?;
                self.switch_to(screen, &target);
                self.load_default_tab()?;
            }
            Some(Transition::Open(target)) => self.open_window(&target)?,
            Some(Transition::Close) | None => {}
        }
        self.refresh_robot_slots()?;
        self.refresh_arena_detail();
        self.fill_hq()?;
        Ok(self.finish(step))
    }

    /// The exhibition setup opens on its `Game Settings` tab: the button is
    /// drawn selected, so the screen's own `gametype.gib` is already in the
    /// content group (the original's constructor does the same load).
    fn load_default_tab(&mut self) -> Result<(), String> {
        let game_type = self.game_type_choice.clone();
        if let Screen::Page(page) = &mut self.screen {
            if page.key.eq_ignore_ascii_case("exhibition/exhib_setup.gib") {
                load_into(
                    page,
                    &self.ui,
                    &self.game,
                    "content group",
                    "gametype.gib",
                    game_type.as_deref(),
                )?;
            }
            // The select-team screen fills its grid with the player's teams.
            if page.key.eq_ignore_ascii_case("teamhq/select_team.gib") {
                let rosters = team_rosters(&self.game, &self.deleted_teams, &self.new_teams);
                let _ = teams::fill_select_grid(&self.ui, &mut page.gib, &rosters);
            }
            // The Team HQ's constructor loads the Robot Inventory tab (its
            // button is the one drawn selected).
            if page.key.eq_ignore_ascii_case(ra2ui::hq::SCREEN) {
                load_into(
                    page,
                    &self.ui,
                    &self.game,
                    ra2ui::hq::CONTENT_SLOT,
                    ra2ui::hq::TABS[0].fragment,
                    None,
                )?;
            }
        }
        self.fill_hq()
    }

    /// What a confirmed window does: the delete box removes the team it asked
    /// about; the edit-team window adds the team it collected.
    fn confirm_window(&mut self, window: &Page) -> Result<(), String> {
        if window.key.eq_ignore_ascii_case("common/messagebox.gib") {
            if let Some(slot) = self.pending_delete.take() {
                self.deleted_teams.insert(slot);
            }
        } else if window.key.eq_ignore_ascii_case("teamhq/joinevent.gib") {
            return self.enter_event(window);
        } else if window.key.eq_ignore_ascii_case("teamhq/editteam.gib") {
            let read = |name: &str| {
                window
                    .gib
                    .blocks
                    .iter()
                    .find_map(|b| match b {
                        Block::Widget(root) => workshop::find(root, name).map(text_of),
                        _ => None,
                    })
                    .unwrap_or_default()
            };
            let name = read("name input textbox");
            let slots: Vec<usize> = teams::rosters(&self.game, Side::Player)
                .unwrap_or_default()
                .into_iter()
                .filter(|r| !r.named)
                .map(|r| r.slot)
                .filter(|slot| !self.new_teams.iter().any(|t| t.slot == *slot))
                .collect();
            if let (false, Some(slot)) = (name.trim().is_empty(), slots.first()) {
                self.new_teams.push(teams::Roster {
                    slot: *slot,
                    name,
                    motto: read("team motto input textbox"),
                    logo: "Teams\\orange_skull.bmp".to_string(),
                    robots: Vec::new(),
                    named: true,
                });
            }
        } else {
            return Ok(());
        }
        let rosters = team_rosters(&self.game, &self.deleted_teams, &self.new_teams);
        if let Screen::Page(page) = &mut self.screen {
            if page.key.eq_ignore_ascii_case("teamhq/select_team.gib") {
                let _ = teams::fill_select_grid(&self.ui, &mut page.gib, &rosters);
            }
        }
        Ok(())
    }

    /// Open a modal window over the screen (`FUN_005e5620`). A window that is
    /// already the top one is not opened twice.
    fn open_window(&mut self, target: &str) -> Result<(), String> {
        // `window#content`: the help frame carries the gib its id names.
        let (target, content) = match target.split_once('#') {
            Some((window, content)) => (window, Some(content)),
            None => (target, None),
        };
        if let Some(top) = self.windows.last() {
            if top.key.eq_ignore_ascii_case(target) {
                return Ok(());
            }
        }
        if let Screen::Page(mut page) = load_screen(&self.ui, target)? {
            // The options window opens on its Video tab: its constructor
            // fills the content group, so the window is never shown empty.
            if target.eq_ignore_ascii_case("common/options.gib") {
                load_into(
                    &mut page,
                    &self.ui,
                    &self.game,
                    "options content group",
                    "common/video_options.gib",
                    None,
                )?;
            }
            if let (true, Some(content)) =
                (target.eq_ignore_ascii_case("help/helpframe.gib"), content)
            {
                load_into(
                    &mut page,
                    &self.ui,
                    &self.game,
                    "help content group",
                    content,
                    None,
                )?;
            }
            if target.eq_ignore_ascii_case("teamhq/editteam.gib") {
                // The constructor sets the caption for a new team and starts the
                // fields empty (FUN_004ebd40: `New Robotics Team`).
                for block in page.gib.blocks.iter_mut() {
                    let Block::Widget(root) = block else { continue };
                    for (name, text) in [
                        ("window caption", "New Robotics Team"),
                        ("name input textbox", ""),
                        ("team motto input textbox", ""),
                    ] {
                        if let Some(w) = workshop::find_mut(root, name) {
                            set_text(w, text);
                        }
                    }
                }
            }
            if let (true, Some((caption, body))) = (
                target.eq_ignore_ascii_case("common/messagebox.gib"),
                self.pending_message.take(),
            ) {
                // `FUN_005e5e40(caption, text, 1)`: an OK-only box.
                for block in page.gib.blocks.iter_mut() {
                    let Block::Widget(root) = block else { continue };
                    for (name, text) in [("caption bar", &caption), ("text box", &body)] {
                        if let Some(w) = workshop::find_mut(root, name) {
                            set_text(w, text);
                        }
                    }
                    for name in ["yes button", "no button", "cancel button"] {
                        if let Some(w) = workshop::find_mut(root, name) {
                            ra2ui::input::set_field(w, "Visible", "false");
                        }
                    }
                }
                self.windows.push(page);
                return Ok(());
            }
            if target.eq_ignore_ascii_case("teamhq/joinevent.gib") {
                if let (Some(view), Some(index)) = (&self.hq_view, self.join_event) {
                    if let Some(row) = view.events.get(index) {
                        let selected = (!view.robots.is_empty()).then_some(0);
                        ra2ui::event_screens::fill_join_window(
                            &self.ui,
                            &mut page.gib,
                            &row.event,
                            &view.robots,
                            selected,
                        )?;
                    }
                }
            }
            if target.eq_ignore_ascii_case("common/messagebox.gib") {
                // The delete box (FUN_004ec110): `Delete Team`, `Are you sure
                // you want to delete <team>?`, Yes/No only.
                let selected = match &self.screen {
                    Screen::Page(p) => p.gib.blocks.iter().find_map(|b| match b {
                        Block::Widget(root) => workshop::find(root, teams::SELECT_GRID)
                            .and_then(ra2ui::input::selected_row),
                        _ => None,
                    }),
                    Screen::Lab(_) => None,
                };
                let rosters = team_rosters(&self.game, &self.deleted_teams, &self.new_teams);
                let team = selected.and_then(|row| rosters.get(row));
                self.pending_delete = team.map(|t| t.slot);
                let body = format!(
                    "Are you sure you want to delete {}?",
                    team.map(|t| t.name.as_str()).unwrap_or("this team")
                );
                for block in page.gib.blocks.iter_mut() {
                    let Block::Widget(root) = block else { continue };
                    for (name, text) in
                        [("caption bar", "Delete Team"), ("text box", body.as_str())]
                    {
                        if let Some(w) = workshop::find_mut(root, name) {
                            set_text(w, text);
                        }
                    }
                    for name in ["ok button", "cancel button"] {
                        if let Some(w) = workshop::find_mut(root, name) {
                            ra2ui::input::set_field(w, "Visible", "false");
                        }
                    }
                }
            }
            self.windows.push(page);
            Ok(())
        } else {
            Err(format!("{target} is not a window"))
        }
    }

    /// The schedule cell a point is in, while the Team HQ shows the Events tab.
    fn event_cell_at(&self, x: f32, y: f32) -> Option<usize> {
        let Screen::Page(page) = &self.screen else {
            return None;
        };
        if self.hq_view.is_none() || !page.key.eq_ignore_ascii_case(ra2ui::hq::SCREEN) {
            return None;
        }
        let placed = ra2ui::resolve_with(&page.gib, self.size, Some(&self.fonts));
        let grid = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == ra2ui::hq::EVENT_GRID)?;
        let geometry = ra2ui::Grid::from_widget(grid.widget)?;
        if !grid.frame.contains_point(x, y) {
            return None;
        }
        // The schedule divides its own rectangle evenly into `Rows` x
        // `Columns` cells (`ra2ui::hq` lays the cells out that way), not by the
        // file's design-time `RowHeight`/`ColWidths`, so the hit test does too.
        // A point on the grid is always the grid's: a miss between cells or
        // past the last event is swallowed, never the grid's `Notes` link.
        let columns = geometry.columns.max(1) as usize;
        let rows = geometry.rows.max(1) as usize;
        let column = (((x - grid.frame.x) / grid.frame.w) * columns as f32).floor() as usize;
        let row = (((y - grid.frame.y) / grid.frame.h) * rows as f32).floor() as usize;
        Some(row.min(rows - 1) * columns + column.min(columns - 1))
    }

    /// The schedule's click handler `FUN_00500c40` (bound at `0x4fdc4e`): no
    /// event in progress opens the join window, the event in progress opens its
    /// bracket, any other cell says one event at a time.
    fn click_event_cell(&mut self, index: usize) -> Result<(), String> {
        use ra2ui::hq::EventState;
        let Some(view) = &self.hq_view else {
            return Ok(());
        };
        if index >= view.events.len() {
            return Ok(());
        }
        let current = view
            .events
            .iter()
            .position(|r| r.state == EventState::InProgress);
        match current {
            None => {
                self.join_event = Some(index);
                self.open_window("teamhq/joinevent.gib")
            }
            Some(current) if current == index => {
                let row = view.events[index].clone();
                let gib = ra2ui::event_screens::bracket_screen(
                    &self.ui,
                    &row.event,
                    view.bracket.as_ref(),
                )?;
                self.show_bracket(gib);
                Ok(())
            }
            Some(_) => {
                self.pending_message = Some((
                    ra2ui::event_screens::CAPTION.to_string(),
                    ra2ui::event_screens::ONE_EVENT_AT_A_TIME.to_string(),
                ));
                self.open_window("common/messagebox.gib")
            }
        }
    }

    fn show_bracket(&mut self, gib: Gib) {
        let key = "teamhq/event_bracket.gib";
        let page = Page {
            key: key.to_string(),
            gib,
            content: Vec::new(),
            slot_base: BTreeMap::new(),
            scroll: BTreeMap::new(),
        };
        self.fade = None;
        self.switch_to(Screen::Page(page), key);
    }

    /// `FUN_00482590` after a click in the join window: the chosen robot's name.
    fn refresh_join_window(&mut self) {
        let Some(window) = self.windows.last_mut() else {
            return;
        };
        if !window.key.eq_ignore_ascii_case("teamhq/joinevent.gib") {
            return;
        }
        let Some(view) = &self.hq_view else {
            return;
        };
        let selected = window.gib.blocks.iter().find_map(|b| match b {
            Block::Widget(root) => workshop::find(root, "robot grid").and_then(|g| {
                ra2ui::input::field_of(g, "_SelectedColumn").and_then(|v| v.parse::<usize>().ok())
            }),
            _ => None,
        });
        let name = selected
            .and_then(|i| view.robots.get(i))
            .map(|r| r.name.clone())
            .unwrap_or_default();
        for block in window.gib.blocks.iter_mut() {
            if let Block::Widget(root) = block {
                if let Some(w) = workshop::find_mut(root, "robot name") {
                    set_text(w, &name);
                }
            }
        }
    }

    /// `FUN_00481f20`: with a robot chosen the join window closes and the
    /// bracket opens for the event; the event is now the one in progress.
    fn enter_event(&mut self, window: &Page) -> Result<(), String> {
        use ra2ui::hq::EventState;
        let chosen = window.gib.blocks.iter().find_map(|b| match b {
            Block::Widget(root) => workshop::find(root, "robot grid")
                .and_then(|g| ra2ui::input::field_of(g, "_SelectedColumn"))
                .and_then(|v| v.parse::<usize>().ok()),
            _ => None,
        });
        let (Some(index), Some(robot)) = (self.join_event.take(), chosen) else {
            return Ok(());
        };
        let Some(view) = &mut self.hq_view else {
            return Ok(());
        };
        for (i, row) in view.events.iter_mut().enumerate() {
            if row.state != EventState::Completed {
                row.state = if i == index {
                    EventState::InProgress
                } else {
                    EventState::Disabled
                };
            }
        }
        let id = view.events[index].id;
        self.entered_event = Some((id, robot));
        if let Some(hook) = &mut self.enter_hook {
            if let Some(fresh) = hook(id, Some(robot)) {
                self.hq_view = Some(fresh);
                self.hq_filled = None;
            }
        }
        let Some(view) = &self.hq_view else {
            return Ok(());
        };
        let row = view.events[view.event_index(id).unwrap_or(index)].clone();
        let gib =
            ra2ui::event_screens::bracket_screen(&self.ui, &row.event, view.bracket.as_ref())?;
        self.show_bracket(gib);
        Ok(())
    }

    /// How many modal windows are open over the screen.
    pub fn window_depth(&self) -> usize {
        self.windows.len()
    }

    /// Stamp the screen/content on a step, remember the choices and append it
    /// to the trace.
    /// The arena list shows as many rows as its scroller's rectangle holds (the
    /// capture: 15 rows in a 282 px box, the scroll bar hidden because the eight
    /// arenas fit), not the file's design-time `Rows 6`. The viewport is only
    /// known once the page is laid out, so it is measured here and written to the
    /// list as `_ViewRows`; the selection and highlight bars span the list.
    fn sync_list_viewports(&mut self) {
        let rows = {
            let placed = self.placements();
            placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == ra2ui::arenas::LIST)
                .and_then(|list| {
                    let grid = ra2ui::Grid::from_widget(list.widget)?;
                    (grid.row_height > 0.0)
                        .then(|| ((list.frame.h / grid.row_height).floor() as usize).max(1))
                })
        };
        let Some(rows) = rows else { return };
        let Screen::Page(page) = &mut self.screen else {
            return;
        };
        let mut rescroll = None;
        for block in page.gib.blocks.iter_mut() {
            let Block::Widget(root) = block else { continue };
            let Some(list) = workshop::find_mut(root, ra2ui::arenas::LIST) else {
                continue;
            };
            if ra2ui::input::field_of(list, "_ViewRows") != Some(&rows.to_string()) {
                ra2ui::input::set_field(list, "_ViewRows", &rows.to_string());
                for bar in list.children.iter_mut() {
                    if bar.name() == "Sel Bar" || bar.name() == "High Bar" {
                        ra2ui::input::set_field(bar, "Size", "1 1 18 0");
                    }
                }
                rescroll = Some(page.scroll.get(ra2ui::arenas::LIST).copied().unwrap_or(0));
            }
            break;
        }
        if let Some(offset) = rescroll {
            let _ = self.set_list_scroll(ra2ui::arenas::LIST, offset);
        }
    }

    fn finish(&mut self, mut step: Step) -> Step {
        // A click on a text box focuses it; any other click takes the focus
        // away, as a click elsewhere does in the original's edit fields.
        self.text_focus = step
            .hit
            .strip_prefix("TextBox ")
            .map(|name| name.trim_matches('"').to_string())
            .filter(|name| !name.is_empty());
        if !self.focused_is_editable() {
            self.text_focus = None;
        }
        self.sync_list_viewports();
        step.screen = self.screen_id();
        step.content = self.content_id();
        step.tab = self.tab().map(str::to_string);
        step.step = self.step().map(str::to_string);
        self.remember_choices();
        self.trace.push(step.clone());
        step
    }

    /// The `load appropriate gib for player N here` region a click landed on.
    ///
    /// The region is the shipped `exhib_robots.gib` group itself; its own note
    /// says where the robot's cell goes. The picker window is the shipped
    /// `selectbot_player.gib`/`selectbot_computer.gib`: slots 1 and 2 are the
    /// player's side (`team 1 group`), slots 3 and 4 the computer's (`team 2
    /// group`), which is what the two window titles say.
    fn picker_region(
        &self,
        x: f32,
        y: f32,
    ) -> Result<Option<(Side, String, String, String)>, String> {
        let placed = self.placements();
        let control = ra2ui::input::hit_control(&placed, x, y);
        // The slot's own join/change buttons open the picker like its region.
        let slot_button = control.as_ref().is_some_and(|path| {
            chain(&placed, path).iter().any(|p| {
                matches!(
                    p.widget.name(),
                    "human player button" | "computer player button" | "change robot button"
                )
            })
        });
        if control.is_some() && !slot_button {
            return Ok(None);
        }
        let Some(path) = ra2ui::input::hit(&placed, x, y) else {
            return Ok(None);
        };
        let hit_chain = chain(&placed, &path);
        let Some(group) = hit_chain.iter().rev().find(|p| {
            let note = p.widget.notes().trim_start().to_ascii_lowercase();
            // `exhib_robots.gib` carries two spellings: "load appropriate gib
            // for player 1 here" on slots 1 and 3, and "load appropriate
            // player 2 gib" on slots 2 and 4.
            note.contains("load appropriate") && note.contains("player")
        }) else {
            return Ok(None);
        };
        let note = group.widget.notes().trim().to_string();
        let panel = group.widget.name().to_string();
        let slot = note
            .split_whitespace()
            .filter_map(|token| token.parse::<usize>().ok())
            .next()
            .unwrap_or(1);
        let mut side = if slot <= 2 {
            Side::Player
        } else {
            Side::Computer
        };
        if slot_button {
            let button = control.as_ref().and_then(|path| {
                chain(&placed, path)
                    .iter()
                    .find_map(|p| match p.widget.name() {
                        "human player button" => Some(Side::Player),
                        "computer player button" => Some(Side::Computer),
                        _ => None,
                    })
            });
            if let Some(button) = button {
                side = button;
            }
        }
        Ok(Some((
            side,
            panel.clone(),
            format!("Group {panel:?}"),
            note,
        )))
    }

    /// The 0-based slot whose `eject button` is under the point.
    fn eject_slot_at(&self, x: f32, y: f32) -> Option<usize> {
        let placed = self.placements();
        let path = ra2ui::input::hit_control(&placed, x, y)?;
        let chain = chain(&placed, &path);
        if !chain.iter().any(|p| p.widget.name() == "eject button") {
            return None;
        }
        chain.iter().find_map(|p| panel_slot(p.widget.name()))
    }

    fn open_picker(&mut self, side: Side, panel: String, _x: f32, _y: f32) -> Result<(), String> {
        let (path, _) = game::screen(&self.ui, side.picker())
            .ok_or_else(|| format!("{} is not shipped", side.picker()))?;
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut gib = ra2fmt::gib::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        ra2ui::inline_proxies_in(&mut gib, &self.ui);
        let rosters = teams::rosters(&self.game, side)?;
        if rosters.is_empty() {
            return Err(format!("{} has no teams", side.table()));
        }
        let team = match side {
            Side::Player => teams::career_player_slot(&rosters).unwrap_or(0),
            // The original's Select Computer Robot opens on its first team.
            Side::Computer => 0,
        };
        // The dropdown's list is opened by its own `Down Button`; the file's
        // `Visible true` is the authored open state, and a drop-down starts
        // closed.
        if let Some(root) = root_widget_mut(&mut gib) {
            set_team_list_open(root, false);
            // The picker is modal: it draws in front of the page whatever the
            // file's own `ChildOrder` (3) says against the page's root.
            ra2ui::input::set_field(root, "ChildOrder", "1000");
        }
        let mut popup = Popup {
            side,
            slot: panel_slot(&panel)
                .ok_or_else(|| format!("{panel:?} is not one of the four robot slots"))?,
            panel,
            gib,
            rosters,
            team,
            robot: None,
            scroll: 0,
        };
        refresh_picker(&mut popup, &self.ui, &self.game, &mut self.previews)?;
        self.popup = Some(popup);
        Ok(())
    }

    /// A click inside the shipped picker window.
    fn click_popup(&mut self, x: f32, y: f32) -> Result<Step, String> {
        let mut popup = self.popup.take().expect("a popup");
        let mut step = Step {
            command: format!("click {x} {y}"),
            hit: String::new(),
            note: None,
            screen: String::new(),
            content: None,
            tab: None,
            step: None,
            point: [x, y],
        };
        let placed = ra2ui::resolve_with(&popup.gib, self.size, Some(&self.fonts));
        // The team list's own scroll bar (it has a `+Scroller` too).
        if let Some(list) = picker_team_list_placed(&placed) {
            let count = ra2ui::input::content_count(list.widget);
            let visible = ra2ui::input::visible_rows(list.widget).max(1);
            let max = count.saturating_sub(visible);
            if let Some(target) = scrollbar_target(list, x, y, popup.scroll, max) {
                if let Some(root) = root_widget_mut(&mut popup.gib) {
                    if let Some(list) = picker_team_list_mut(root) {
                        popup.scroll = ra2ui::input::scroll_rows(list, target);
                    }
                }
                self.popup = Some(popup);
                step.hit = "ScrollBar".to_string();
                return Ok(self.finish(step));
            }
        }
        let Some(path) = ra2ui::input::hit_control(&placed, x, y) else {
            self.popup = Some(popup);
            return Ok(self.finish(step));
        };
        let chain = chain(&placed, &path);
        step.hit = chain.last().map(|p| describe(p)).unwrap_or_default();
        let named = |wanted: &str| chain.iter().any(|p| p.widget.name() == wanted);

        if named("cancel button") {
            self.popup = None;
            return Ok(self.finish(step));
        }
        if named("ok button") {
            let pick = popup.robot.and_then(|ordinal| {
                popup
                    .rosters
                    .get(popup.team)
                    .and_then(|roster| roster.robots.get(ordinal).copied())
                    .map(|bot| Pick {
                        team: popup.team,
                        bot,
                    })
            });
            if let Some(pick) = pick {
                if let Some(slot) = self.slot_picks.get_mut(popup.slot) {
                    *slot = Some(pick);
                }
                self.slot_sides[popup.slot] = Some(popup.side);
                match popup.slot {
                    0 => self.player_pick = Some(pick),
                    2 => self.opponent_pick = Some(pick),
                    _ => {}
                }
                let panel = popup.panel.clone();
                let side = popup.side;
                let ui = self.ui.clone();
                self.apply_panel(&panel, pick, side, &ui)?;
                self.popup = None;
            } else {
                println!(
                    "robots: {} team {} has no robot to accept",
                    popup.side.name(),
                    popup.team
                );
                self.popup = Some(popup);
            }
            return Ok(self.finish(step));
        }
        // The drop-down: its arrow toggles the list; the text box does the same.
        if named("Down Button") || (named("team dropdown") && !named_list(&chain)) {
            if let Some(root) = root_widget_mut(&mut popup.gib) {
                let open = picker_team_list_mut(root)
                    .map(|list| ra2ui::input::field_of(list, "Visible") == Some("true"))
                    .unwrap_or(false);
                set_team_list_open(root, !open);
            }
            self.popup = Some(popup);
            return Ok(self.finish(step));
        }
        // A row of the team list.
        if let Some(list) = chain
            .iter()
            .find(|p| p.widget.kind == ra2fmt::gib::WidgetKind::ListBox)
        {
            if let Some(grid) = ra2ui::Grid::from_widget(list.widget) {
                let row = (((y - list.frame.y) / grid.row_height).floor().max(0.0) as usize)
                    + popup.scroll;
                if row < popup.rosters.len() {
                    popup.team = row;
                    popup.robot = None;
                    if let Some(root) = root_widget_mut(&mut popup.gib) {
                        set_team_list_open(root, false);
                    }
                    refresh_picker(&mut popup, &self.ui, &self.game, &mut self.previews)?;
                }
            }
            self.popup = Some(popup);
            return Ok(self.finish(step));
        }
        // A cell of the robot grid.
        if let Some(grid) = chain.iter().find(|p| {
            p.widget.kind == ra2fmt::gib::WidgetKind::Grid && p.widget.name() == "robots grid"
        }) {
            if let Some(model) = ra2ui::Grid::from_widget(grid.widget) {
                let column = (((x - grid.frame.x) / model.row_height).floor().max(0.0) as usize)
                    .min(model.columns.max(1) as usize - 1);
                let robots = popup
                    .rosters
                    .get(popup.team)
                    .map(|roster| roster.robots.len())
                    .unwrap_or(0);
                if column < robots {
                    popup.robot = Some(column);
                    refresh_picker(&mut popup, &self.ui, &self.game, &mut self.previews)?;
                }
            }
            self.popup = Some(popup);
            return Ok(self.finish(step));
        }
        self.popup = Some(popup);
        Ok(self.finish(step))
    }

    /// Put the accepted robot into the setup screen's `playerN group`.
    ///
    /// The slot's note says to load the appropriate gib here; the exhibition
    /// screen reuses the multiplayer lobby's cells: `activeplayer.gib` for a
    /// slot that holds a robot (its picture, team name, bot name, the
    /// player/computer icon, `Eject` and the change-robot button) and
    /// `noplayer.gib` for an open one.
    fn apply_panel(
        &mut self,
        panel: &str,
        pick: Pick,
        side: Side,
        ui: &Path,
    ) -> Result<(), String> {
        let cell = workshop::load(ui, "multiplayer/activeplayer.gib")?;
        let Some(mut cell) = cell.widgets().next().cloned() else {
            return Err("activeplayer.gib has no root".to_string());
        };
        let game = self.game().to_path_buf();
        let path = teams::bot_path(&game, side, pick.team, pick.bot);
        let bot_name = teams::bot_info(&path)
            .map(|(name, _)| name)
            .unwrap_or_default();
        let team_name = teams::rosters(&game, side)
            .ok()
            .and_then(|rosters| rosters.get(pick.team).map(|r| r.name.clone()))
            .unwrap_or_default();
        let preview = std::fs::read(&path)
            .ok()
            .and_then(|bytes| ra2fmt::bot::parse(&bytes).ok())
            .and_then(|bot| bot.snapshot)
            .and_then(|tga| ra2ui::preview::Preview::decode_tga(&tga));
        let image = match &preview {
            Some(preview) => {
                let key = preview.key(&format!("slot-{panel}"));
                self.previews.insert(key.clone(), preview.clone());
                key
            }
            None => "exhibition\\sample_bot.bmp".to_string(),
        };
        ra2ui::cells::bind_art(&mut cell, "bot img", &image);
        ra2ui::cells::bind_value(&mut cell, "team name", &team_name);
        ra2ui::cells::bind_value(&mut cell, "bot name", &bot_name);
        ra2ui::cells::bind_art(
            &mut cell,
            "player icon img",
            match side {
                Side::Player => "multiplayer\\local_player_icon.bmp",
                Side::Computer => "multiplayer\\computer_player_icon.bmp",
            },
        );
        // The lobby's ready check and spectator tag belong to network play.
        for name in ["ready label", "ready checkbox", "spectator [TextBox]"] {
            if let Some(widget) = workshop::find_mut(&mut cell, name) {
                ra2ui::input::set_field(widget, "Visible", "false");
            }
        }
        self.put_in_panel(panel, cell);
        Ok(())
    }

    /// Show `noplayer.gib` (OPEN, with its two join buttons) in an empty slot.
    fn open_panel(&mut self, panel: &str, ui: &Path) -> Result<(), String> {
        let cell = workshop::load(ui, "multiplayer/noplayer.gib")?;
        let Some(cell) = cell.widgets().next().cloned() else {
            return Err("noplayer.gib has no root".to_string());
        };
        self.put_in_panel(panel, cell);
        Ok(())
    }

    /// Replace what a slot group holds after its own background image.
    fn put_in_panel(&mut self, panel: &str, cell: Widget) {
        let Screen::Page(page) = &mut self.screen else {
            return;
        };
        for block in page.gib.blocks.iter_mut() {
            let Block::Widget(root) = block else { continue };
            let Some(group) = workshop::find_mut(root, panel) else {
                continue;
            };
            group.children.truncate(1);
            group.children.push(cell);
            return;
        }
    }

    /// Whether the robots tab is the content on screen.
    fn robots_tab_loaded(&self) -> bool {
        match &self.screen {
            Screen::Page(page) => page
                .content
                .last()
                .is_some_and(|(_, key)| key.eq_ignore_ascii_case("exhibition/exhib_robots.gib")),
            Screen::Lab(_) => false,
        }
    }

    /// Fill every slot of the robots tab from the picks made so far: the
    /// accepted robot's cell, or the open slot. The tab is rebuilt from its
    /// file whenever it is loaded, so this runs after each click.
    fn refresh_robot_slots(&mut self) -> Result<(), String> {
        if !self.robots_tab_loaded() {
            return Ok(());
        }
        let ui = self.ui.clone();
        for (slot, panel) in [
            "player1 group",
            "player2 group",
            "player3 group",
            "player4 group",
        ]
        .into_iter()
        .enumerate()
        {
            let side = self.slot_side(slot);
            match self.slot_picks[slot] {
                Some(pick) => self.apply_panel(panel, pick, side, &ui)?,
                None => self.open_panel(panel, &ui)?,
            }
        }
        Ok(())
    }

    /// The arena tab's picture and description follow the selected row: the
    /// script's `preview` bitmap into `arena preview img`, and its class
    /// docstring into `description`.
    fn refresh_arena_detail(&mut self) {
        let game = self.game.clone();
        let game_type = self
            .game_type_choice
            .clone()
            .unwrap_or_else(|| "DEATHMATCH".to_string());
        let Screen::Page(page) = &mut self.screen else {
            return;
        };
        let Some(root) = page.gib.widgets().next() else {
            return;
        };
        let Some(list) = workshop::find(root, ra2ui::arenas::LIST) else {
            return;
        };
        let Some(row) = ra2ui::input::selected_row(list) else {
            return;
        };
        let Ok(arenas) = ra2ui::arenas::available(&game, &game_type) else {
            return;
        };
        let Some(arena) = arenas.get(row) else {
            return;
        };
        let picture = image::open(game.join("Arenas").join(arena.preview.replace('\\', "/")))
            .ok()
            .map(|image| {
                let image = image.into_rgba8();
                let (width, height) = image.dimensions();
                ra2ui::preview::Preview {
                    width,
                    height,
                    rgba: image.into_raw(),
                }
            });
        let key = picture.as_ref().map(|p| p.key("arena"));
        for block in page.gib.blocks.iter_mut() {
            let Block::Widget(root) = block else { continue };
            if let Some(key) = &key {
                ra2ui::cells::bind_art(root, "arena preview img", key);
            }
            // The text is the `DisplayBox` attachment of `description grp` (its
            // body is the `description` Glyph), not a child of the group.
            if let Some(group) = workshop::find_mut(root, "description grp") {
                bind_description(group, &arena.description);
            }
        }
        if let (Some(key), Some(picture)) = (key, picture) {
            self.previews.insert(key, picture);
        }
    }

    /// Keep the exhibition picker's choices across content-tab loads.
    ///
    /// The arena list and the game-type tab both live in `content group`, so
    /// opening one replaces the other in the document. The screen's own
    /// `_SelectedRow`, option group's `Selection` and checkbox's `Checked` are
    /// still the record while each is loaded; this copies them when they are.
    fn remember_choices(&mut self) {
        let Screen::Page(page) = &self.screen else {
            return;
        };
        let root = page.gib.widgets().next();
        let Some(root) = root else {
            return;
        };
        if let Some(list) = ra2ui::workshop::find(root, ra2ui::arenas::LIST) {
            self.arena_rows = Some(ra2ui::input::content_count(list));
            self.arena_scroll = ra2ui::input::field_of(list, "_ScrollTop")
                .and_then(|value| value.parse::<usize>().ok());
            let selected_row = ra2ui::input::selected_row(list);
            if let Some(row) = selected_row {
                self.arena_row = Some(row);
            }
            let selected = selected_row.and_then(|row| {
                list.children
                    .iter()
                    .filter(|c| c.kind == ra2fmt::gib::WidgetKind::Text)
                    .filter(|c| c.field("_Font").is_some())
                    .nth(row)
                    .and_then(|c| c.field("_Text"))
                    .map(|f| f.text().to_string())
            });
            if let Some(name) = selected {
                if let Ok(arenas) = ra2ui::arenas::all(&self.game) {
                    if let Some(arena) = arenas.iter().find(|a| a.name == name) {
                        self.arena_choice = Some(arena.script.clone());
                    }
                }
            }
        }
        if let Some(group) = ra2ui::workshop::find(root, "game type optiongroup")
            .filter(|g| g.kind == ra2fmt::gib::WidgetKind::OptionGroup)
        {
            if let Some(selection) = ra2ui::input::field_of(group, "Selection") {
                let game_type = match selection {
                    "deathmatch checkbox" => Some("DEATHMATCH"),
                    "battle royal checkbox" => Some("BATTLE ROYAL"),
                    "teams checkbox" => Some("TEAM MATCH"),
                    "tabletop checkbox" => Some("TABLETOP"),
                    "kingofhill checkbox" => Some("KING OF THE HILL"),
                    _ => None,
                };
                if let Some(game_type) = game_type {
                    self.game_type_choice = Some(game_type.to_string());
                }
            }
        }
        if let Some(hazard) = ra2ui::workshop::find(root, "hazard checkbox") {
            if let Some(checked) = ra2ui::input::field_of(hazard, "Checked") {
                self.hazard_choice = Some(checked == "true");
            }
        }
        if let Some(dropdown) = ra2ui::workshop::find(root, "time dropdown") {
            if let Some(text) = dropdown
                .children
                .iter()
                .find(|c| c.kind == ra2fmt::gib::WidgetKind::TextBox)
                .and_then(|box_| {
                    box_.children
                        .iter()
                        .find(|c| c.kind == ra2fmt::gib::WidgetKind::Text)
                })
                .and_then(|text| text.field("_Text"))
                .map(|field| field.text().trim().to_string())
                .filter(|text| !text.is_empty())
            {
                self.time_text = Some(text.clone());
                self.time_seconds = parse_match_time(&text);
            }
        }
    }

    /// Click a named widget that is a *region* rather than a control (the
    /// setup screen's `playerN group` slots). The point is the region's centre;
    /// the click still goes through [`Shell::click`], so the picker opens the
    /// same way a mouse would open it.
    pub fn click_region(&mut self, name: &str) -> Result<Step, String> {
        let (x, y) = {
            let placed = self.placements();
            let region = placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == name)
                .ok_or_else(|| format!("no region called {name:?} on {}", self.screen_id()))?;
            let frame = region.frame;
            let mut found = None;
            for iy in 1..8 {
                for ix in 1..10 {
                    let x = frame.x + frame.w * ix as f32 / 10.0;
                    let y = frame.y + frame.h * iy as f32 / 8.0;
                    if let Some(path) = ra2ui::input::hit(&placed, x, y) {
                        if chain(&placed, &path)
                            .iter()
                            .any(|p| p.widget.name() == name)
                        {
                            found = Some((x, y));
                            break;
                        }
                    }
                }
                if found.is_some() {
                    break;
                }
            }
            found.ok_or_else(|| format!("no visible part of {name:?} is clickable"))?
        };
        self.click(x, y)
    }

    /// Click the named widget, at a point found by hit-testing where the layout
    /// put it. The point is searched inside the widget's frame because a label
    /// may be drawn over a button — the chassis screen does exactly that.
    pub fn click_named(&mut self, name: &str) -> Result<Step, String> {
        let (x, y) = self.reachable(name)?;
        let mut step = self.click(x, y)?;
        step.command = format!("click {name:?}");
        if let Some(last) = self.trace.last_mut() {
            last.command = step.command.clone();
        }
        Ok(step)
    }

    fn reachable(&self, name: &str) -> Result<(f32, f32), String> {
        let page = matches!(self.screen, Screen::Page(_));
        let hit = |placed: &[Placement<'_>], x: f32, y: f32| {
            // The control under a point first: a container drawn over a
            // control (the workshop's step layer over the tools pane) is not
            // what a player clicks. The workshop falls back to what is under
            // the point for names that are not controls.
            if page {
                ra2ui::input::hit_control(placed, x, y)
            } else {
                ra2ui::input::hit_control(placed, x, y).or_else(|| ra2ui::input::hit(placed, x, y))
            }
        };
        let placed = self.placements();
        let found = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == name);
        let Some(found) = found else {
            let mut names: Vec<&str> = placed
                .iter()
                .flat_map(|p| p.walk())
                .map(|p| p.widget.name())
                .filter(|n| !n.is_empty())
                .collect();
            names.sort();
            names.dedup();
            return Err(format!(
                "no widget called {name:?} on {} (named: {})",
                self.screen_id(),
                names.join(", ")
            ));
        };
        let frame = found.frame;
        for iy in 1..8 {
            for ix in 1..10 {
                let x = frame.x + frame.w * ix as f32 / 10.0;
                let y = frame.y + frame.h * iy as f32 / 8.0;
                if let Some(path) = hit(&placed, x, y) {
                    if chain(&placed, &path)
                        .iter()
                        .any(|p| p.widget.name() == name)
                    {
                        return Ok((x, y));
                    }
                }
            }
        }
        let center = hit(&placed, frame.x + frame.w / 2.0, frame.y + frame.h / 2.0)
            .map(|path| {
                chain(&placed, &path)
                    .last()
                    .map(|p| describe(p))
                    .unwrap_or_default()
            })
            .unwrap_or_else(|| "nothing".into());
        Err(format!(
            "no click reaches {name:?} on {} ({frame:?}; the centre hits {center})",
            self.screen_id()
        ))
    }

    /// Draw the current screen to `path` through `ra2-render`'s UI path.
    pub fn shot(&self, path: impl AsRef<Path>) -> Result<Shot, String> {
        use ra2_render::ui::{UiAssets, UiRenderer};
        use ra2_render::Renderer;

        let path = path.as_ref();
        let [width, height] = self.size;
        let (width, height) = (width as u32, height as u32);
        let mut assets = UiAssets::open(&self.game).map_err(|e| e.to_string())?;
        for (key, preview) in &self.previews {
            assets.add_preview(key, preview);
        }
        let placed = self.placements();
        let scene = assets.scene(&placed, [width, height]);
        let renderer = Renderer::new().map_err(|e| e.to_string())?;
        let ui = UiRenderer::new(renderer.device());
        let pixels = ui
            .render(renderer.device(), renderer.queue(), &scene)
            .map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        image::RgbaImage::from_raw(width, height, pixels)
            .ok_or_else(|| "the readback did not fill the image".to_string())?
            .save(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Shot {
            path: path.to_path_buf(),
            size: [width, height],
            quads: scene.quads.len(),
            textures: scene.textures.len(),
            missing: scene.missing.len(),
        })
    }
}

/// The named player teams the select screen lists: the install's records, less
/// the session's deletions, plus the teams made this session.
fn team_rosters(
    game: &Path,
    deleted: &std::collections::BTreeSet<usize>,
    created: &[teams::Roster],
) -> Vec<teams::Roster> {
    let mut rosters: Vec<teams::Roster> = teams::rosters(game, Side::Player)
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.named && !deleted.contains(&r.slot))
        .collect();
    rosters.extend(
        created
            .iter()
            .filter(|r| !deleted.contains(&r.slot))
            .cloned(),
    );
    rosters
}

/// A text box's displayed text: its own `_Text`, else its `Text` child's.
fn text_of(widget: &Widget) -> String {
    for child in &widget.children {
        if child.kind == WidgetKind::Text {
            if let Some(t) = ra2ui::input::field_of(child, "_Text") {
                return t.to_string();
            }
        }
    }
    ra2ui::input::field_of(widget, "_Text")
        .unwrap_or("")
        .to_string()
}

fn load_screen(ui: &Path, key: &str) -> Result<Screen, String> {
    let (path, key) =
        game::screen(ui, key).ok_or_else(|| format!("{key} is not in {}", ui.display()))?;
    if key.eq_ignore_ascii_case(workshop::SCREEN) {
        return BotLab::open(ui).map(Screen::Lab);
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut gib = ra2fmt::gib::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    ra2ui::inline_proxies_in(&mut gib, ui);
    Ok(Screen::Page(Page {
        key,
        gib,
        content: Vec::new(),
        slot_base: BTreeMap::new(),
        scroll: BTreeMap::new(),
    }))
}

/// A page's planned effect, worked out before the document is changed.
enum Plan {
    Nothing,
    Replace { target: String },
    Open { target: String },
    Close,
    Load { target: String, slot: String },
}

/// What a click asks the shell to do with the screens.
enum Transition {
    Replace(String),
    Open(String),
    Close,
}

fn plan_page(page: &Page, fonts: &Fonts, size: [f32; 2], x: f32, y: f32, step: &mut Step) -> Plan {
    let placed = ra2ui::resolve_with(&page.gib, size, Some(fonts));
    let Some(path) = ra2ui::input::hit_control(&placed, x, y) else {
        step.hit = "nothing".into();
        return Plan::Nothing;
    };
    let chain = chain(&placed, &path);
    step.hit = chain.last().map(|p| describe(p)).unwrap_or_default();
    // The executable's own handler bindings, by widget name, come first; the
    // `Notes` strings only answer for widgets no recovered binding covers.
    let bound = chain
        .iter()
        .rev()
        .find_map(|p| bindings::lookup(&page.key, p.widget.name()).map(|binding| (*p, binding)));
    if let Some((owner, binding)) = bound {
        // The app keys its options staging on the control's note text, so the
        // note is still reported when the record has one.
        let note = owner.widget.notes().trim();
        step.note = Some(if note.is_empty() {
            format!("{} [{}]", owner.widget.name(), binding.at)
        } else {
            note.to_string()
        });
        return match binding.action {
            Action::Goto(screen) => Plan::Replace {
                target: screen.to_string(),
            },
            Action::Window(screen) => Plan::Open {
                target: screen.to_string(),
            },
            Action::Help(gib) => Plan::Open {
                target: format!("help/helpframe.gib#{gib}"),
            },
            Action::Close => Plan::Close,
            Action::Confirm => {
                step.note = Some(format!("confirm {}", binding.at));
                Plan::Close
            }
            Action::Quit => Plan::Nothing,
        };
    }
    let found = chain
        .iter()
        .rev()
        .find_map(|p| nav::navigation(p.widget.notes()).map(|nav| (*p, nav)));
    let Some((owner, nav)) = found else {
        return Plan::Nothing;
    };
    step.note = Some(owner.widget.notes().trim().to_string());
    match nav {
        Navigation::File { screen, slot } => match slot {
            Some(slot) => Plan::Load {
                target: screen,
                slot,
            },
            None => Plan::Replace { target: screen },
        },
        Navigation::Link(link) => Plan::Replace {
            target: link.screen.to_string(),
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn click_page(
    ui: &Path,
    game: &Path,
    fonts: &Fonts,
    size: [f32; 2],
    page: &mut Page,
    x: f32,
    y: f32,
    step: &mut Step,
    game_type: Option<&str>,
) -> Result<Option<Transition>, String> {
    let plan = plan_page(page, fonts, size, x, y, step);
    // What the click changes inside the screen: an option-group member, a list
    // row, or a standalone checkbox. All three are the data's own controls; the
    // row geometry is the list's `RowHeight` and its `Sel Bar`.
    let (option, cell, checkbox) = {
        let placed = ra2ui::resolve_with(&page.gib, size, Some(fonts));
        let Some(path) = ra2ui::input::hit_control(&placed, x, y) else {
            return Ok(None);
        };
        let chain = chain(&placed, &path);
        let option = option_in(&chain);
        let cell = chain
            .iter()
            .rev()
            .find(|p| {
                p.widget.kind == ra2fmt::gib::WidgetKind::ListBox
                    || p.widget.kind == ra2fmt::gib::WidgetKind::Grid
            })
            .and_then(|list| {
                let grid = ra2ui::Grid::from_widget(list.widget)?;
                if grid.row_height <= 0.0 {
                    return None;
                }
                let top = ra2ui::input::field_of(list.widget, "_ScrollTop")
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(0);
                let row = (((y - list.frame.y) / grid.row_height).floor().max(0.0) as usize)
                    .saturating_add(top);
                let columns = grid.columns.max(1) as usize;
                let column = if grid.row_select || columns == 1 {
                    0
                } else {
                    (((x - list.frame.x) / grid.row_height).floor().max(0.0) as usize)
                        .min(columns - 1)
                };
                let count = ra2ui::input::content_count(list.widget);
                (row.saturating_mul(columns).saturating_add(column) < count).then_some((
                    list.widget.name().to_string(),
                    row,
                    column,
                ))
            });
        let checkbox = if option.is_none() {
            chain
                .iter()
                .rev()
                .find(|p| p.widget.kind == ra2fmt::gib::WidgetKind::CheckBox)
                .map(|p| p.widget.name().to_string())
        } else {
            None
        };
        (option, cell, checkbox)
    };
    if let Some((group, child)) = &option {
        for block in page.gib.blocks.iter_mut() {
            if let Block::Widget(root) = block {
                ra2ui::input::select_option(root, group, child);
            }
        }
    }
    if let Some((list, row, column)) = &cell {
        if !list.is_empty() {
            for block in page.gib.blocks.iter_mut() {
                if let Block::Widget(root) = block {
                    if let Some(widget) = ra2ui::workshop::find_mut(root, list) {
                        ra2ui::input::select_cell(widget, *row, *column);
                        let top = ra2ui::input::field_of(widget, "_ScrollTop")
                            .and_then(|value| value.parse::<usize>().ok())
                            .unwrap_or(0);
                        ra2ui::input::scroll_rows(widget, top);
                    }
                }
            }
        }
    }
    if let Some(name) = &checkbox {
        for block in page.gib.blocks.iter_mut() {
            if let Block::Widget(root) = block {
                if let Some(widget) = ra2ui::workshop::find_mut(root, name) {
                    let checked = ra2ui::input::field_of(widget, "Checked") == Some("true");
                    ra2ui::input::set_state(widget, !checked);
                }
            }
        }
    }
    match plan {
        Plan::Nothing => Ok(None),
        Plan::Replace { target, .. } => Ok(Some(Transition::Replace(target))),
        Plan::Open { target } => Ok(Some(Transition::Open(target))),
        Plan::Close => Ok(Some(Transition::Close)),
        Plan::Load { target, slot, .. } => {
            load_into(page, ui, game, &slot, &target, game_type)?;
            Ok(None)
        }
    }
}

/// Load a fragment into the group a note names, then let the data fill any list
/// the loaded screen says to fill.
fn load_into(
    page: &mut Page,
    ui: &Path,
    game: &Path,
    slot: &str,
    target: &str,
    game_type: Option<&str>,
) -> Result<(), String> {
    let (path, key) =
        game::screen(ui, target).ok_or_else(|| format!("{target} is not in {}", ui.display()))?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut fragment = ra2fmt::gib::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    ra2ui::inline_proxies_in(&mut fragment, ui);
    let mut loaded = false;
    let Page { gib, slot_base, .. } = page;
    for block in gib.blocks.iter_mut() {
        let Block::Widget(root) = block else { continue };
        let Some(group) = workshop::find_mut(root, slot) else {
            continue;
        };
        let base = *slot_base
            .entry(slot.to_string())
            .or_insert(group.children.len());
        group.children.truncate(base);
        group.children.extend(fragment.widgets().cloned());
        loaded = true;
        break;
    }
    if !loaded {
        return Err(format!("{} has no group called {slot:?}", page.key));
    }
    if key.eq_ignore_ascii_case("exhibition/gametype.gib") {
        default_game_settings(&mut page.gib);
    }
    if key.eq_ignore_ascii_case("exhibition/exhib_robots.gib") {
        show_slots_for(&mut page.gib, game_type);
    }
    page.content.push((slot.to_string(), key.clone()));
    fill_lists(&mut page.gib, game, game_type)?;
    page.scroll.insert(ra2ui::arenas::LIST.to_string(), 0);
    // The arena tab opens with its first row selected (the original's list).
    for block in page.gib.blocks.iter_mut() {
        let Block::Widget(root) = block else { continue };
        if let Some(list) = workshop::find_mut(root, ra2ui::arenas::LIST) {
            if ra2ui::input::selected_row(list).is_none() {
                ra2ui::input::select_row(list, 0);
            }
        }
    }
    Ok(())
}

/// The Game Settings tab's constructor defaults (`FUN_004733f0`, `0x4733f0`):
/// DEATHMATCH, ALLWEIGHT (the file's own checked boxes) and a match time of 3
/// minutes. The constructor fills the time dropdown with `No limit`, then
/// `1 2 3 4 5 10 15 20`, and selects `3`; the file's `5 minutes` is an editor
/// placeholder. The dropdown shows the bare number.
fn default_game_settings(gib: &mut Gib) {
    for block in gib.blocks.iter_mut() {
        let Block::Widget(root) = block else { continue };
        let Some(dropdown) = workshop::find_mut(root, "time dropdown") else {
            continue;
        };
        for child in dropdown.children.iter_mut() {
            if child.kind == WidgetKind::TextBox {
                set_text(child, "3");
            }
        }
    }
}

/// Only BATTLE ROYAL and TEAM MATCH use all four robot slots. A duel shows
/// two panels side by side in the top row (the original's DEATHMATCH setup):
/// slot 1 and slot 3, the opponent, the pair the match is built from, each
/// centred in half of the area. Slots 2 and 4 are not shown.
fn show_slots_for(gib: &mut Gib, game_type: Option<&str>) {
    let four = matches!(game_type, Some("BATTLE ROYAL") | Some("TEAM MATCH"));
    if four {
        return;
    }
    for block in gib.blocks.iter_mut() {
        let Block::Widget(root) = block else { continue };
        // Hit-testing does not skip an invisible group, so the two unused slots
        // leave the tree (slot 3 now sits where slot 2 was).
        for name in ["team 1 group", "team 2 group"] {
            if let Some(team) = workshop::find_mut(root, name) {
                team.children
                    .retain(|c| c.name() != "player2 group" && c.name() != "player4 group");
            }
        }
        // Two half-width columns, so slot 1 and slot 3 are each centred in
        // their own half and neither group covers the other's hit area.
        for (name, x) in [("team 1 group", "0.25"), ("team 2 group", "0.75")] {
            if let Some(team) = workshop::find_mut(root, name) {
                ra2ui::input::set_field(team, "Position", &format!("{x} 1 0.25 1"));
                ra2ui::input::set_field(team, "Size", "0.5 1 0.5 1");
            }
        }
        for name in ["player1 group", "player3 group"] {
            if let Some(slot) = workshop::find_mut(root, name) {
                ra2ui::input::set_field(slot, "Position", "0.5 1 0.5 1");
            }
        }
    }
}

/// Fill the lists whose own `Notes` say what fills them.
///
/// The arena picker is the shipped case: its listbox says *"populate with arena
/// names"*, and the names come from the game's own arena scripts
/// (`ra2ui::arenas`). When the Game Settings tab has selected a type, the list
/// is that type's `enumAvailable` set; before the tab is opened the full twelve
/// are offered and the caller filters the choice. The source and the
/// destination are both named by the data, so this is not a table of screens.
fn fill_lists(gib: &mut Gib, game: &Path, game_type: Option<&str>) -> Result<(), String> {
    let wants_arenas = gib.walk().any(|w| {
        w.notes()
            .trim()
            .eq_ignore_ascii_case("populate with arena names")
    });
    if !wants_arenas {
        return Ok(());
    }
    let arenas = match game_type {
        Some(game_type) => ra2ui::arenas::available(game, game_type)?,
        None => ra2ui::arenas::all(game)?,
    };
    for block in gib.blocks.iter_mut() {
        let Block::Widget(root) = block else { continue };
        let Some(list) = workshop::find_mut(root, ra2ui::arenas::LIST) else {
            continue;
        };
        ra2ui::arenas::fill_list(list, &arenas);
        ra2ui::input::clear_selection(list);
        ra2ui::input::scroll_rows(list, 0);
        break;
    }
    Ok(())
}

fn click_lab(
    fonts: &Fonts,
    size: [f32; 2],
    lab: &mut BotLab,
    x: f32,
    y: f32,
    step: &mut Step,
) -> Result<Option<String>, String> {
    let placed = lab.placements(size, Some(fonts));
    let Some(path) = ra2ui::input::hit(&placed, x, y) else {
        step.hit = "nothing".into();
        return Ok(None);
    };
    let chain = chain(&placed, &path);
    step.hit = chain.last().map(|p| describe(p)).unwrap_or_default();

    // The bottom bar's Help button opens the help for the tab on show.
    if chain.iter().any(|p| p.widget.name() == "Help button") {
        if let Some(id) = bindings::lab_help_id(lab.tab, lab.step) {
            step.note = Some(format!("FUN_004e4ab0({id}): help window"));
            return Ok(Some(bindings::help_target(id)));
        }
    }
    // A purpose-named transition out of the workshop (the exit button) is the
    // shell's. It is checked first: the exit button is a plain button, so the
    // workshop would ignore it.
    if let Some((owner, link)) = chain
        .iter()
        .rev()
        .find_map(|p| nav::link(p.widget.notes()).map(|link| (*p, link)))
    {
        step.note = Some(owner.widget.notes().trim().to_string());
        return Ok(Some(link.screen.to_string()));
    }
    // Everything else is the workshop's: a tab, a step, the layer's structure
    // buttons, an option group's member, or a click on empty chrome.
    let tab = lab.click(x, y, size, Some(fonts))?;
    if let Some(tab) = tab {
        step.note = Some(format!("the workshop's {} tab", tab.button));
    } else if let Some(step_button) = lab.step {
        step.note = Some(format!("the workshop's {step_button} step"));
    }
    Ok(None)
}

/// Parse the time dropdown's own value (`5 minutes`, `No limit`).
///
/// The listbox under it is empty in the shipped file — the option list is not
/// in the data — so the current value is the only one that can be read. This is
/// exposed as [`Shell::time_choice`].
fn parse_match_time(text: &str) -> Option<Option<f32>> {
    let lower = text.trim().to_ascii_lowercase();
    if lower.contains("no limit") || lower.contains("unlimited") {
        return Some(None);
    }
    let number: f32 = lower.split_whitespace().next()?.parse().ok()?;
    // The constructor's option strings are bare numbers of minutes.
    let seconds = if lower.contains("minute") || lower.split_whitespace().count() == 1 {
        number * 60.0
    } else if lower.contains("second") {
        number
    } else {
        return None;
    };
    (seconds.is_finite() && seconds > 0.0).then_some(Some(seconds))
}

fn root_widget_mut(gib: &mut Gib) -> Option<&mut Widget> {
    gib.blocks.iter_mut().find_map(|block| match block {
        Block::Widget(widget) => Some(widget),
        Block::Attachment(_) => None,
    })
}

/// The picker's `team dropdown` list, from the document.
fn picker_team_list_mut(widget: &mut Widget) -> Option<&mut Widget> {
    let dropdown = workshop::find_mut(widget, "team dropdown")?;
    dropdown
        .children
        .iter_mut()
        .find(|child| child.kind == WidgetKind::ListBox)
}

/// Open or close the picker's team list.
///
/// The authored list sits on the dropdown's text box at `0 0`; a drop-down's
/// open list hangs below the field, so opening moves it to `0 0 20 0` (the
/// field is 20 pixels high) and puts it in front of the text. That is view
/// state, not a record the file owns.
fn set_team_list_open(widget: &mut Widget, open: bool) {
    let Some(list) = picker_team_list_mut(widget) else {
        return;
    };
    if open {
        ra2ui::input::set_field(list, "Position", "0 0 20 0");
        ra2ui::input::set_field(list, "ChildOrder", "100");
    }
    ra2ui::input::set_field(list, "Visible", if open { "true" } else { "false" });
    // The drop-down clips to its collapsed 20-pixel field and its `select team
    // group` is 110 pixels high; the open list hangs below the field and may
    // run past the group, so those two clips are lifted while the list is open.
    for name in ["team dropdown", "select team group", "bg group"] {
        if let Some(widget) = workshop::find_mut(widget, name) {
            ra2ui::input::set_field(widget, "ClipMode", if open { "0" } else { "1" });
        }
    }
    // The motto and the robot-name boxes are later siblings of the drop-down,
    // so they are hit-tested before it; an open list covers them. Hide them
    // while it is open (the drop-down is modal) and show them again on close.
    for name in ["team motto", "robot name"] {
        if let Some(box_) = workshop::find_mut(widget, name) {
            ra2ui::input::set_field(box_, "Visible", if open { "false" } else { "true" });
        }
    }
}

/// The same list after placement.
fn picker_team_list_placed<'a>(placed: &'a [Placement<'a>]) -> Option<&'a Placement<'a>> {
    let dropdown = placed
        .iter()
        .flat_map(|p| p.walk())
        .find(|p| p.widget.name() == "team dropdown")?;
    dropdown
        .children
        .iter()
        .find(|child| child.widget.kind == WidgetKind::ListBox)
}

fn named_list(chain: &[&Placement<'_>]) -> bool {
    chain.iter().any(|p| p.widget.kind == WidgetKind::ListBox)
}

/// Set a `TextBox`'s visible string, both on the box and on its `Text` child.
fn set_text(widget: &mut Widget, text: &str) {
    ra2ui::input::set_field(widget, "_Text", text);
    for child in widget.children.iter_mut() {
        if child.kind == WidgetKind::Text {
            ra2ui::input::set_field(child, "_Text", text);
        }
    }
}

/// Rebuild the open picker from its selected team and robot: the dropdown's
/// rows, the dropdown text, the robot cells (with their weight icons), the
/// selection and the labels the screen's own notes name.
fn refresh_picker(
    popup: &mut Popup,
    ui: &Path,
    game: &Path,
    previews: &mut BTreeMap<String, ra2ui::preview::Preview>,
) -> Result<(), String> {
    let template = workshop::load(ui, "exhibition/choosebot_cell.gib")?
        .widgets()
        .next()
        .cloned()
        .ok_or_else(|| "choosebot_cell.gib has no root".to_string())?;
    let team = popup.team.min(popup.rosters.len().saturating_sub(1));
    popup.team = team;
    let roster = popup.rosters[team].clone();
    if roster.robots.is_empty() {
        popup.robot = None;
    } else if popup.robot.is_none_or(|robot| robot >= roster.robots.len()) {
        popup.robot = Some(0);
    }
    let robot_name = popup
        .robot
        .and_then(|ordinal| roster.robots.get(ordinal).copied())
        .map(|bot| {
            let path = teams::bot_path(game, popup.side, team, bot);
            match teams::bot_info(&path) {
                Some((name, _)) => name,
                None => format!("Bot{bot} (no .bot file)"),
            }
        })
        .unwrap_or_else(|| "no robots in this team".to_string());
    let Some(root) = root_widget_mut(&mut popup.gib) else {
        return Err("the picker has no root widget".to_string());
    };
    if let Some(list) = picker_team_list_mut(root) {
        teams::fill_team_list(list, &popup.rosters);
        popup.scroll = ra2ui::input::scroll_rows(list, popup.scroll);
        ra2ui::input::select_cell(list, team, 0);
        popup.scroll = ra2ui::input::scroll_rows(list, popup.scroll);
    }
    if let Some(dropdown) = workshop::find_mut(root, "team dropdown") {
        if let Some(textbox) = dropdown
            .children
            .iter_mut()
            .find(|child| child.kind == WidgetKind::TextBox)
        {
            set_text(textbox, &roster.name);
        }
    }
    if let Some(grid) = workshop::find_mut(root, "robots grid") {
        grid.children
            .retain(|child| child.name() == "Sel Bar" || child.name() == "High Bar");
        for (ordinal, bot) in roster.robots.iter().enumerate() {
            let mut cell = template.clone();
            ra2ui::input::set_field(&mut cell, "Position", &format!("{} 0 0 0", 90 * ordinal));
            let path = teams::bot_path(game, popup.side, team, *bot);
            // The cell's `bot preview` is the snapshot the `.bot` file carries
            // (a 128x128 TGA the game rendered when the design was saved);
            // `sample_bot.bmp` ("No Robot Preview") stays for a bot without one.
            let snapshot = std::fs::read(&path)
                .ok()
                .and_then(|bytes| ra2fmt::bot::parse(&bytes).ok())
                .and_then(|bot| bot.snapshot)
                .and_then(|tga| ra2ui::preview::Preview::decode_tga(&tga));
            if let Some(snapshot) = snapshot {
                let key = snapshot.key("picker");
                ra2ui::cells::bind_art(&mut cell, "bot preview", &key);
                previews.insert(key, snapshot);
            }
            if let Some((_, class)) = teams::bot_info(&path) {
                let icon = match class {
                    0 => "exhibition/light_icon.bmp",
                    1 => "exhibition/medium_icon.bmp",
                    _ => "exhibition/heavy_icon.bmp",
                };
                if let Some(icon_widget) = workshop::find_mut(&mut cell, "weight icon") {
                    ra2ui::input::set_field(icon_widget, "_Filename", icon);
                }
            }
            grid.children.push(cell);
        }
        if let Some(robot) = popup.robot {
            ra2ui::input::select_cell(grid, 0, robot);
        }
    }
    if let Some(name) = workshop::find_mut(root, "robot name") {
        set_text(name, &robot_name);
    }
    if let Some(motto) = workshop::find_mut(root, "team motto") {
        set_text(motto, &roster.motto);
    }
    if !roster.logo.is_empty() {
        if let Some(logo) = workshop::find_mut(root, "team logo img") {
            ra2ui::input::set_field(logo, "_Filename", &roster.logo);
        }
    }
    Ok(())
}

/// The offset a click inside a list's `+Scroller` bar asks for.
///
/// The bar is the attachment's `ScrollBar`: its own `Less`/`More` buttons are
/// the first and last parts, and the track between them is the third. A click
/// on an arrow moves one row; a click on the track jumps to that fraction of
/// the content. `None` when the point is not on this list's bar.
fn scrollbar_target(list: &Placement<'_>, x: f32, y: f32, top: usize, max: usize) -> Option<usize> {
    let bar = list
        .attachments
        .iter()
        .find(|a| a.attachment.kind == AttachmentKind::Scroller)?
        .body
        .walk()
        .find(|p| p.widget.kind == WidgetKind::ScrollBar)?;
    if !bar.frame.contains_point(x, y) || !bar.widget.visible() {
        return None;
    }
    let model = ra2ui::ScrollBar::from_widget(bar.widget);
    let less = bar.children.get(model.less)?.frame;
    let more = bar.children.get(model.more)?.frame;
    let parts = model.parts(bar.frame, less, more);
    let track = parts[2];
    if y < track.y {
        return Some(top.saturating_sub(1));
    }
    if y >= track.bottom() {
        return Some((top + 1).min(max));
    }
    if track.h <= 0.0 {
        return Some(top);
    }
    let fraction = ((y - track.y) / track.h).clamp(0.0, 1.0);
    Some((fraction * max as f32).round() as usize)
}

fn chain<'a>(roots: &'a [Placement<'a>], path: &[usize]) -> Vec<&'a Placement<'a>> {
    let mut out = Vec::new();
    let Some(first) = path.first() else {
        return out;
    };
    out.push(&roots[*first]);
    for index in &path[1..] {
        let next = &out.last().expect("a root").children[*index];
        out.push(next);
    }
    out
}

/// Put `text` into every `DisplayBox` under `widget` (the group's own and its
/// children's: in `arena.gib` the box hangs on the group's unnamed child).
fn bind_description(widget: &mut ra2fmt::gib::Widget, text: &str) {
    for attachment in widget
        .attachments
        .iter_mut()
        .filter(|a| a.kind == ra2fmt::gib::AttachmentKind::DisplayBox)
    {
        if let Some(field) = attachment.field_mut("Display") {
            field.raw = ra2ui::cells::markup_with_text(&field.raw, text);
        }
        if let Some(body) = attachment.body.as_deref_mut() {
            ra2ui::cells::bind_prose(body, text);
        }
    }
    if widget.kind == ra2fmt::gib::WidgetKind::Glyph {
        ra2ui::input::set_field(widget, "_Text", text);
    }
    for child in widget.children.iter_mut() {
        bind_description(child, text);
    }
}

/// A widget's description for the trace: its class and its data name.
fn describe(p: &Placement<'_>) -> String {
    let kind = p.widget.kind.name();
    let name = p.widget.name();
    if name.is_empty() {
        format!("{kind} (unnamed)")
    } else {
        format!("{kind} {name:?}")
    }
}

/// The option group and member a hit path passes through, if any.
fn option_in(chain: &[&Placement<'_>]) -> Option<(String, String)> {
    let mut group = None;
    let mut member = None;
    for p in chain {
        if p.widget.kind == ra2fmt::gib::WidgetKind::OptionGroup {
            group = Some(p.widget.name().to_string());
        }
        if p.widget.kind == ra2fmt::gib::WidgetKind::CheckBox {
            member = Some(p.widget.name().to_string());
        }
    }
    Some((group?, member?))
}
