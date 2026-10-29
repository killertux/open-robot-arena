//! The team HQ: `teamhq/teamhq.gib`, its four tabs, and what fills them.
//!
//! The screen is the game's own and the tabs are its own `CheckBox`es. Each one
//! carries a note naming the file it loads into the content group:
//! `robots button` says *"load hq_robots.gib into hq content group"*, and the
//! other three say the same for `hq_league.gib`, `hq_events.gib` and
//! `hq_history.gib`. So [`TABS`] is read from the notes, exactly as the bot
//! lab's toolbar is, and the test beside it re-reads the shipped file.
//!
//! ## What fills a screen
//!
//! Each tab's own grid states its content in its `Notes`:
//!
//! | screen | its note | source |
//! |---|---|---|
//! | `hq_robots.gib` | *"fill grid with team robots (robot_cell.gib)"* | the team's roster |
//! | `hq_league.gib` | *"fill with league_cell.gib with the teams in order of rank"* | [`crate::standings`] |
//! | `hq_events.gib` | *"fill with this season's events (event_cell.gib)"* | `Events.txt` through the season |
//! | `hq_history.gib` | *"fill grid with prize bitmaps for all past events … most recently won first"* | the career's prize list |
//!
//! The data's own disagreement is kept rather than smoothed over: `hq_robots`'s
//! note names `robot_cell.gib`, and the game ships `bot_cell.gib` — the cell
//! the binary's own fill uses (`FUN_004ff570`). [`ROBOT_TEMPLATE`] is the file
//! that ships, and the test names the difference.
//!
//! ## What this module does not know
//!
//! It has no career model and no match: [`View`] is the plain data the screens
//! draw, and the application builds it from `ra2sim::career`. That keeps this
//! crate below `ra2sim` (which already depends on it) and keeps every point,
//! rank and prize coming from one place — the career.

use std::path::{Path, PathBuf};

use ra2fmt::event::Event;
use ra2fmt::gib::{Block, Field, Gib, Widget, WidgetKind};
use ra2fmt::team::Team;

use crate::standings::Standing;
use crate::{Fonts, Frame, Grid, Placement};

/// The team HQ window.
pub const SCREEN: &str = "teamhq/teamhq.gib";
/// The group every tab's screen loads into — the name all four notes give.
pub const CONTENT_SLOT: &str = "hq content group";
/// The toolbar's option group.
pub const TAB_GROUP: &str = "hq button optiongroup";

/// One tab: the `CheckBox` that selects it and the screen it loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tab {
    pub button: &'static str,
    pub fragment: &'static str,
}

/// The four tabs, in the order the toolbar lists them.
pub const TABS: [Tab; 4] = [
    Tab {
        button: "robots button",
        fragment: "teamhq/hq_robots.gib",
    },
    Tab {
        button: "league button",
        fragment: "teamhq/hq_league.gib",
    },
    Tab {
        button: "events button",
        fragment: "teamhq/hq_events.gib",
    },
    Tab {
        button: "history button",
        fragment: "teamhq/hq_history.gib",
    },
];

/// The robot inventory's grid and the cell the binary fills it with.
pub const ROBOT_GRID: &str = "robot grid";
pub const ROBOT_TEMPLATE: &str = "teamhq/bot_cell.gib";
/// The cell of a slot with no robot (`FUN_004ff570`: `UI\teamhq\nobot_cell.gib`, `New Robot`).
pub const NO_ROBOT_TEMPLATE: &str = "teamhq/nobot_cell.gib";
/// The inventory shows six cells: three rows of two columns, filled down each column first.
pub const INVENTORY_CELLS: usize = 6;
/// The cell's own binding points.
pub const BOT_NAME: &str = "bot name";
pub const BOT_CLASS: &str = "weight class";
pub const BOT_SNAPSHOT: &str = "bot snapshot img";

/// The league and season-results screens' grids.
pub const LEAGUE_GRID: &str = "league grid";
pub const RESULTS_SCREEN: &str = "teamhq/season_results.gib";
pub const RESULTS_GRID: &str = "results grid";

/// The event schedule's grid.
pub const EVENT_GRID: &str = "event grid";
pub const EVENT_TEMPLATE: &str = "teamhq/event_cell.gib";
/// The three state cells `FUN_004ffcc0` loads into the same grid: completed
/// (`0x6ff21c`), in progress (`0x6ff1fc`) and disabled (`0x6ff1c4`).
pub const EVENT_COMPLETED: &str = "teamhq/eventcell_completed.gib";
pub const EVENT_IN_PROGRESS: &str = "teamhq/eventcell_inprogress.gib";
pub const EVENT_DISABLED: &str = "teamhq/eventcell_disabled.gib";

/// The bracket screen's generated group (`event_bracket.gib`'s note: "build the
/// event bracket inside this group").
pub const BRACKET_GROUP: &str = "bracket group";
pub const BRACKET_HEADING: &str = "round heading textbox";
pub const BRACKET_EVENT_TITLE: &str = "event title";
pub const BRACKET_EVENT_DESCRIPTION: &str = "event description";
pub const BRACKET_PLAY_BUTTON: &str = "play button";
pub const BRACKET_LATER_BUTTON: &str = "later button";
pub const BRACKET_FORFEIT_BUTTON: &str = "forfeit button";

/// The history screen's prize grid.
pub const HISTORY_SCREEN: &str = "teamhq/hq_history.gib";
pub const PRIZE_GRID: &str = "prizes grid";

/// The team header's three fields on `teamhq.gib`.
pub const TEAM_NAME: &str = "hq team name textbox";
pub const TEAM_MOTTO: &str = "hq motto textbox";
pub const TEAM_LOGO: &str = "hq logo img";

/// One robot in the inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RobotRow {
    /// The team folder's robot slot (`Bot<slot>.bot`, an entry of the record's `Robots:` line).
    /// The inventory's cell `i` shows slot `i`; a slot with no robot (never made, or scrapped)
    /// is the `nobot_cell.gib` cell, `New Robot`.
    pub slot: usize,
    pub name: String,
    /// The class the `.bot` records, as the cell's own note asks for it.
    pub class: String,
    /// A file the UI can draw: the shipped `nobot.bmp` silhouette when the
    /// `.bot` has no snapshot, otherwise the synthetic key of
    /// [`RobotRow::preview_image`].
    pub preview: String,
    /// The `.bot`'s own 128×128 snapshot, decoded from the file's embedded TGA.
    /// The caller registers it with the renderer's `UiAssets` under
    /// [`RobotRow::preview`].
    pub preview_image: Option<crate::preview::Preview>,
}

/// One event on the season schedule: its `Events.txt` index, record and the
/// state `FUN_004ffcc0` gives its cell.
#[derive(Clone, Debug, PartialEq)]
pub struct EventRow {
    pub id: usize,
    pub event: Event,
    pub state: EventState,
}

/// Which cell a scheduled event shows. The three special cells are the binary's
/// own (`FUN_004ffcc0` at `0x4ffe…`): a completed event, the event in progress,
/// and every other event while one is in progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventState {
    /// No event is in progress: the normal cell.
    Available,
    /// This event is the one in progress.
    InProgress,
    /// The event is in the career's history.
    Completed,
    /// Another event is in progress; this cell is disabled.
    Disabled,
}

/// One bot in a bracket match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BracketSlot {
    pub name: String,
    pub player: bool,
    /// The team's logo (`Teams\\orange_skull.bmp`), the bracket's picture.
    pub logo: String,
}

/// One match of a round, as `event_bracket.gib`'s generated bracket shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BracketMatchView {
    pub slots: Vec<BracketSlot>,
    /// True for the match the player is in.
    pub player_match: bool,
    /// The result when this match is one the player played.
    pub won: Option<bool>,
}

/// One round column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BracketRoundView {
    /// `"Round 1"` … `"Final Match"`.
    pub label: String,
    pub matches: Vec<BracketMatchView>,
    /// The round the player is in.
    pub current: bool,
    /// The round has been played.
    pub played: bool,
}

/// The bracket state the event screen and the season tab draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BracketView {
    pub event: usize,
    pub rounds: Vec<BracketRoundView>,
    /// The zero-based current round.
    pub round: usize,
    /// `"Round 2"` or `"Final Match"`.
    pub round_label: String,
    /// `"Round 2 of 4 — next: Red Zone"` / `"Eliminated in round 2"` /
    /// `"Event champion"`.
    pub status: String,
    /// The first opponent of the current round, if one is left.
    pub next_opponent: Option<String>,
    /// Whether the `play button` should start the next round.
    pub can_play: bool,
}

/// One prize the career stores, as the history grid draws it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrizeRow {
    /// The trophy art (`UI\teamhq\events\trophies\…` or the golden sawblade).
    pub art: String,
    pub description: String,
}

/// The history screen's counters. Every one is a career field; the formatter
/// (the fastest win's `N/A`) is the screen's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub total_seasons: i32,
    pub current_rank: i32,
    pub best_rank: i32,
    pub total_events: i32,
    pub wins: i32,
    pub losses: i32,
    pub events_won: i32,
    pub highest_score: i32,
    pub fastest_win: f32,
}

/// The data the four tabs draw. Built from the career by the application.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub team: Team,
    pub robots: Vec<RobotRow>,
    pub standings: Vec<Standing>,
    /// The season's nine events, in schedule order. The event grid's cell
    /// `i` is [`View::events`]`[i]`, and so is its row in the click.
    pub events: Vec<EventRow>,
    /// The zero-based season (`Team+0x98`).
    pub season: i32,
    pub events_remaining: usize,
    /// Most recently won first, as the grid's note asks.
    pub prizes: Vec<PrizeRow>,
    pub stats: Stats,
    /// The bracket in progress, when the career is in an event.
    pub bracket: Option<BracketView>,
    /// Finished events on this season's schedule: `(event id, won)`.
    pub completed: Vec<(usize, bool)>,
}

impl View {
    /// The `Events.txt` id of the event the grid cell with this index shows.
    pub fn event_id(&self, index: usize) -> Option<usize> {
        self.events.get(index).map(|row| row.id)
    }

    /// The cell index for an event id on the schedule.
    pub fn event_index(&self, id: usize) -> Option<usize> {
        self.events.iter().position(|row| row.id == id)
    }
}

/// What a click on the HQ did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Click {
    /// A tab button selected its tab.
    Tab(&'static str),
    /// An event-grid cell: the `Events.txt` id it shows.
    Event(usize),
}

/// The composed HQ screen, as live state: the window, its proxy, the selected
/// tab and the tab's fragment. A click changes the group's selection and the
/// content group, and nothing else.
pub struct Hq {
    ui_root: PathBuf,
    pub document: Gib,
    tab: &'static str,
    view: View,
    slot_own: usize,
    slot_ready: bool,
    screen_at: usize,
}

impl Hq {
    /// Compose the HQ on the tab the file itself has selected (`robots button`)
    /// and fill it.
    pub fn open(ui_root: &Path, view: View) -> Result<Hq, String> {
        let mut document = load(ui_root, SCREEN)?;
        // The HQ's split frames and the tab screens were both authored for the
        // editor's canvas: the right pane's `LR slot` is 287 pixels tall and
        // each tab is a full 1280×1005 screen. The game hands the loaded tab the
        // content group's frame; letting the slot fill its split pane and each
        // split's children fill their panes is what makes the league, events and
        // history screens fit the content area instead of being clipped to its
        // first row. The base screen's own left/right split is untouched.
        if let Some(slot) = find_mut_in_gib(&mut document, "LR slot") {
            crate::input::set_field(slot, "Position", "0 0 0 0");
            crate::input::set_field(slot, "Size", "1 1 1 1");
        }
        let mut hq = Hq {
            ui_root: ui_root.to_path_buf(),
            document,
            tab: TABS[0].button,
            view: view.clone(),
            slot_own: 0,
            slot_ready: false,
            screen_at: 0,
        };
        fill_team(&mut hq.document, &view);
        // `open` loaded no fragment: put the first tab in and count what the
        // slot held of its own before it.
        hq.set_screen(TABS[0].fragment)?;
        hq.tab = TABS[0].button;
        hq.fill_tab()?;
        Ok(hq)
    }

    /// Compose the HQ on a named tab (`robots button`, `league button`, …).
    pub fn open_on(ui_root: &Path, view: View, button: &str) -> Result<Hq, String> {
        let mut hq = Hq::open(ui_root, view)?;
        hq.select(button)?;
        Ok(hq)
    }

    pub fn tab(&self) -> &'static str {
        self.tab
    }

    pub fn view(&self) -> &View {
        &self.view
    }

    /// Replace the data every tab draws: the current tab's screen is reloaded
    /// and filled from the new view. This is the refresh after a match.
    pub fn set_view(&mut self, view: View) -> Result<(), String> {
        self.view = view;
        let tab = self.tab;
        self.set_screen(tab_fragment(tab).expect("a known tab"))?;
        fill_team(&mut self.document, &self.view);
        self.fill_tab()
    }

    /// Select a tab by its button name: the toolbar's group moves, the content
    /// group gets the tab's screen, and the new screen is filled.
    pub fn select(&mut self, button: &str) -> Result<bool, String> {
        let Some(tab) = tab(button) else {
            return Ok(false);
        };
        for block in self.document.blocks.iter_mut() {
            if let Block::Widget(root) = block {
                crate::input::select_option(root, TAB_GROUP, button);
            }
        }
        self.tab = tab.button;
        self.set_screen(tab.fragment)?;
        self.fill_tab()?;
        Ok(true)
    }

    /// The tab's screen inside the content group: the first record the fragment
    /// loaded.
    pub fn content_root(&self) -> Option<&Widget> {
        for block in &self.document.blocks {
            if let Block::Widget(w) = block {
                if let Some(slot) = find(w, CONTENT_SLOT) {
                    return slot.children.get(self.screen_at);
                }
            }
        }
        None
    }

    pub fn placements(&self, screen: [f32; 2], fonts: Option<&Fonts>) -> Vec<Placement<'_>> {
        crate::resolve_with(&self.document, screen, fonts)
    }

    /// The point at the centre of an event cell, for a scripted click and for
    /// the test that a row maps to the event the career enters.
    pub fn event_point(
        &self,
        index: usize,
        screen: [f32; 2],
        fonts: Option<&Fonts>,
    ) -> Option<[f32; 2]> {
        let placed = self.placements(screen, fonts);
        let grid = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == EVENT_GRID)?;
        let geometry = Grid::from_widget(grid.widget)?;
        let columns = geometry.columns.max(1) as usize;
        let rows = geometry.rows.max(1) as usize;
        let cell = schedule_cell(grid.frame, rows, columns, index / columns, index % columns);
        Some([cell.x + cell.w / 2.0, cell.y + cell.h / 2.0])
    }

    /// Which event cell a point is in, by the grid's own geometry.
    pub fn event_at(
        &self,
        point: [f32; 2],
        screen: [f32; 2],
        fonts: Option<&Fonts>,
    ) -> Option<usize> {
        let placed = self.placements(screen, fonts);
        let grid = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == EVENT_GRID)?;
        let geometry = Grid::from_widget(grid.widget)?;
        if !grid.frame.contains_point(point[0], point[1]) {
            return None;
        }
        let columns = geometry.columns.max(1) as usize;
        let rows = geometry.rows.max(1) as usize;
        for row in 0..rows {
            for column in 0..columns {
                let cell = schedule_cell(grid.frame, rows, columns, row, column);
                if cell.contains_point(point[0], point[1]) {
                    return Some(row * columns + column);
                }
            }
        }
        None
    }

    /// Click at a point: a tab button selects its tab, an event cell is that
    /// event, anything else is nothing.
    pub fn click(
        &mut self,
        point: [f32; 2],
        screen: [f32; 2],
        fonts: Option<&Fonts>,
    ) -> Result<Option<Click>, String> {
        let (tab_button, on_events, event_index) = {
            let placed = self.placements(screen, fonts);
            let Some(path) = crate::input::hit_control(&placed, point[0], point[1]) else {
                return Ok(None);
            };
            let mut names = Vec::new();
            let mut current = &placed[path[0]];
            names.push(current.widget.name().to_string());
            for index in &path[1..] {
                current = &current.children[*index];
                names.push(current.widget.name().to_string());
            }
            let tab_button = TABS
                .iter()
                .find(|t| names.iter().any(|n| n == t.button))
                .map(|t| t.button);
            let on_events = names.iter().any(|n| n == EVENT_GRID);
            let event_index = if on_events {
                self.event_at(point, screen, fonts)
            } else {
                None
            };
            (tab_button, on_events, event_index)
        };
        if let Some(button) = tab_button {
            self.select(button)?;
            return Ok(Some(Click::Tab(button)));
        }
        if on_events {
            if let Some(index) = event_index {
                if let Some(id) = self.view.event_id(index) {
                    return Ok(Some(Click::Event(id)));
                }
            }
        }
        Ok(None)
    }

    /// Put a screen into the content group, leaving the group's own children.
    fn set_screen(&mut self, relative: &str) -> Result<(), String> {
        let mut fragment = load(&self.ui_root, relative)?;
        fit_splits_in(&mut fragment);
        let own = if self.slot_ready {
            self.slot_own
        } else {
            // First load: the group's children are its own, and the fragment
            // follows them. A content group with none of its own is a real
            // state, so the count is remembered with a flag rather than by
            // treating zero as "not counted yet".
            let mut own = 0;
            for block in &self.document.blocks {
                if let Block::Widget(w) = block {
                    if let Some(slot) = find(w, CONTENT_SLOT) {
                        own = slot.children.len();
                    }
                }
            }
            self.slot_own = own;
            self.slot_ready = true;
            own
        };
        let mut done = false;
        for block in self.document.blocks.iter_mut() {
            let Block::Widget(w) = block else { continue };
            let Some(slot) = find_mut(w, CONTENT_SLOT) else {
                continue;
            };
            slot.children.truncate(own);
            self.screen_at = slot.children.len();
            slot.children.extend(fragment.widgets().cloned());
            done = true;
            break;
        }
        if !done {
            return Err(format!("{SCREEN} has no {CONTENT_SLOT}"));
        }
        Ok(())
    }

    fn fill_tab(&mut self) -> Result<(), String> {
        match self.tab {
            "robots button" => {
                self.fill_robots()?;
                let view = self.view.clone();
                sync_inventory_buttons(&mut self.document, &view);
            }
            "league button" => self.fill_league()?,
            "events button" => self.fill_events()?,
            "history button" => self.fill_history()?,
            other => return Err(format!("unknown HQ tab {other}")),
        }
        Ok(())
    }

    fn fill_robots(&mut self) -> Result<(), String> {
        let template = load(&self.ui_root, ROBOT_TEMPLATE)?;
        let empty = load(&self.ui_root, NO_ROBOT_TEMPLATE)?
            .widgets()
            .next()
            .cloned();
        let rows = &self.view.robots;
        let Some(grid) = find_mut_in(&mut self.document, ROBOT_GRID) else {
            return Ok(());
        };
        // `FUN_004ff570` makes six cells; cell `i` is slot `i` (a robot cell when the team has
        // that slot, else the `New Robot` cell). A refill (after a scrap) starts from the bars.
        grid.children
            .retain(|c| matches!(c.name(), "Sel Bar" | "High Bar"));
        let made = crate::cells::fill(grid, &template, INVENTORY_CELLS);
        let bars = grid.children.len() - made;
        // Nothing is selected until the player clicks a robot (the shipped bar
        // positions are the author's leftovers).
        clear_bars(grid);
        // The inventory grid divides its own rectangle evenly into `Rows` x
        // `Columns` cells (the capture's cells are 341 x 119 px in a 683 x 357
        // grid, not the file's 213 x 70) and fills down each column first; its
        // selection frame is a whole cell (`_EvenCells`, read by `input::select_cell`).
        crate::input::set_field(grid, "_EvenCells", "true");
        if let Some(geometry) = Grid::from_widget(grid) {
            let (cols, rws) = (
                geometry.columns.max(1) as usize,
                geometry.rows.max(1) as usize,
            );
            for (i, cell) in grid.children[bars..].iter_mut().enumerate() {
                let (col, row) = (i / rws, i % rws);
                if col >= cols {
                    break;
                }
                let (w, h) = (1.0 / cols as f32, 1.0 / rws as f32);
                crate::input::set_field(
                    cell,
                    "Position",
                    &format!("{} 1 {} 1", col as f32 * w, row as f32 * h),
                );
                crate::input::set_field(cell, "Size", &format!("{w} 1 {h} 1"));
            }
        }
        for (slot, cell) in grid.children[bars..].iter_mut().enumerate() {
            match rows.iter().find(|r| r.slot == slot) {
                Some(row) => {
                    crate::cells::bind_value(cell, BOT_NAME, &row.name);
                    crate::cells::bind_value(cell, BOT_CLASS, &row.class);
                    if !row.preview.is_empty() {
                        crate::cells::bind_art(cell, BOT_SNAPSHOT, &row.preview);
                    }
                }
                None => {
                    if let Some(root) = &empty {
                        let mut none = root.clone();
                        for field in ["Position", "Size"] {
                            if let Some(v) = cell.field(field).map(|f| f.raw.clone()) {
                                crate::input::set_field(&mut none, field, &v);
                            }
                        }
                        *cell = none;
                    }
                }
            }
        }
        Ok(())
    }

    fn fill_league(&mut self) -> Result<(), String> {
        let template = crate::cells::template(&self.ui_root, crate::standings::LEAGUE_TEMPLATE)?;
        let rows = self.view.standings.clone();
        let Some(grid) = find_mut_in(&mut self.document, LEAGUE_GRID) else {
            return Ok(());
        };
        crate::standings::fill_league(grid, &template, &rows);
        clear_bars(grid);
        bind_value_in(
            &mut self.document,
            "season",
            &format!("Season {}", self.view.season + 1),
        );
        bind_value_in(
            &mut self.document,
            "events remaining",
            &format!("Events remaining: {}", self.view.events_remaining),
        );
        Ok(())
    }

    fn fill_events(&mut self) -> Result<(), String> {
        let rows = self.view.events.clone();
        let ui_root = self.ui_root.clone();
        // The live content group is 420 px tall at 1024x768 where ours is 422.9 (the chain of
        // splits above it is not recovered, docs/36 9.22); the schedule needs the 420 (the
        // calendar 0.95 of it = 399, its split 0.9 = 359, minus the 26 px title = the grid's
        // 333, three rows of 111). A fit of the events pane only, as a fraction so it scales.
        if let Some(group) = find_mut_in(&mut self.document, "events group") {
            crate::input::set_field(group, "Size", "1 1 0.99325 1");
        }
        let Some(grid) = find_mut_in(&mut self.document, EVENT_GRID) else {
            return Ok(());
        };
        let Some(geometry) = Grid::from_widget(grid) else {
            return Ok(());
        };
        let columns = geometry.columns.max(1) as usize;
        let width = geometry.col_widths.first().copied().unwrap_or(0.0);
        grid.children
            .retain(|c| c.name() != "event cell group" && !c.name().starts_with("grid line"));
        clear_bars(grid);
        for (index, row) in rows.iter().enumerate() {
            let relative = match row.state {
                EventState::Completed => EVENT_COMPLETED,
                EventState::InProgress => EVENT_IN_PROGRESS,
                EventState::Disabled => EVENT_DISABLED,
                EventState::Available => EVENT_TEMPLATE,
            };
            let template = load(&ui_root, relative)?;
            let Some(root) = template.widgets().next().cloned() else {
                continue;
            };
            // Like the inventory, the schedule divides its own rectangle evenly
            // into `Rows` x `Columns` cells.
            let (r, c) = (index / columns, index % columns);
            let rows_n = geometry.rows.max(1) as f32;
            let (cw, ch) = (1.0 / columns as f32, 1.0 / rows_n);
            // The art sits inside the slot's left/top line and `BufferWidth 2`: it starts
            // `1 + 2` px in and stops 2 px short of the next slot's line (live, 1024x768: slot
            // 184 x 111, lines at 341/525/709 and 318/429/540/651, art 344..522 x 321..426).
            // Fractions of the 184 x 111 slot, so they hold at the other sizes too.
            let mut cell = root;
            crate::input::set_field(
                &mut cell,
                "Position",
                &format!(
                    "{} 1 {} 1",
                    c as f32 * cw + 3.0 / 184.0 * cw,
                    r as f32 * ch + 3.0 / 111.0 * ch
                ),
            );
            crate::input::set_field(
                &mut cell,
                "Size",
                &format!("{} 1 {} 1", cw * 179.0 / 184.0, ch * 106.0 / 111.0),
            );
            crate::cells::bind_art(&mut cell, "preview img", &row.event.art);
            let (label, color) = match row.state {
                EventState::Completed => {
                    let won = self
                        .view
                        .completed
                        .iter()
                        .find(|(id, _)| *id == row.id)
                        .is_some_and(|(_, won)| *won);
                    if won {
                        ("CHAMPION".to_string(), "0.3 1 0.5 1")
                    } else {
                        ("PLAYED".to_string(), "0.8 0.8 0.8 1")
                    }
                }
                EventState::InProgress => {
                    if let Some(bracket) = &self.view.bracket {
                        (
                            format!("{}/{}", bracket.round + 1, bracket.rounds.len()),
                            "0.4 0.9 1 1",
                        )
                    } else {
                        ("IN EVENT".to_string(), "0.4 0.9 1 1")
                    }
                }
                EventState::Disabled => ("OUT".to_string(), "0.6 0.6 0.6 1"),
                EventState::Available => ("".to_string(), "1 1 1 1"),
            };
            if !label.is_empty() {
                cell.children.push(text_widget(
                    "event state",
                    &label,
                    [3.0, 14.0, width - 6.0, 14.0],
                    "fg_mcn12",
                    color,
                ));
                let children = cell.children.len().to_string();
                crate::input::set_field(&mut cell, "NumChildren", &children);
            }
            grid.children.push(cell);
        }
        // The grid's own lines (`BorderColor 0 0 0 1`, `BorderWidth 1`), one pixel each and shared
        // between neighbours: a vertical line at the left of every column and a horizontal one at
        // the top of every row and under the last (live: no line at the right edge).
        let rows_n = geometry.rows.max(1) as usize;
        let tall = 1.0 + 1.0 / (rows_n as f32 * 111.0);
        for c in 0..columns {
            grid.children.push(grid_line(
                &format!("grid line column {c}"),
                [c as f32 / columns as f32, 1.0, 0.0, 1.0],
                [1.0, 0.0, tall, 1.0],
            ));
        }
        for r in 0..=rows_n {
            grid.children.push(grid_line(
                &format!("grid line row {r}"),
                [0.0, 1.0, r as f32 / rows_n as f32, 1.0],
                [1.0, 1.0, 1.0, 0.0],
            ));
        }
        // The schedule's own title states the season; the bracket state is the
        // short `E<n>/9 R<n>` suffix, which fits the title box's one line.
        if let Some(title) = find_mut_in(&mut self.document, "schedule title") {
            let bracket = match &self.view.bracket {
                Some(bracket) => {
                    let stage = if bracket.round + 1 >= bracket.rounds.len() {
                        "FINAL".to_string()
                    } else {
                        format!("R{}", bracket.round + 1)
                    };
                    format!(" · E{}/9 {stage}", self.view.completed.len() + 1)
                }
                None => String::new(),
            };
            set_value(
                title,
                &format!("Event Schedule - Season {}{bracket}", self.view.season + 1),
            );
            title
                .children
                .retain(|child| child.name() != "bracket status");
        }
        Ok(())
    }

    fn fill_history(&mut self) -> Result<(), String> {
        let stats = self.view.stats.clone();
        for (name, value) in [
            ("total seasons", (stats.total_seasons + 1).to_string()),
            ("current rank", stats.current_rank.to_string()),
            (
                "best rank",
                if stats.total_events == 0 {
                    "n/a".to_string()
                } else {
                    stats.best_rank.to_string()
                },
            ),
            ("total events", stats.total_events.to_string()),
            ("wins", stats.wins.to_string()),
            ("losses", stats.losses.to_string()),
            ("events won", stats.events_won.to_string()),
            ("highest score", stats.highest_score.to_string()),
            ("fastest win", format_fastest(stats.fastest_win)),
        ] {
            bind_value_in(&mut self.document, name, &value);
        }
        let prizes = self.view.prizes.clone();
        let Some(grid) = find_mut_in(&mut self.document, PRIZE_GRID) else {
            return Ok(());
        };
        let Some(geometry) = Grid::from_widget(grid) else {
            return Ok(());
        };
        grid.children.retain(|c| !c.name().starts_with("prize "));
        let columns = geometry.columns.max(1) as usize;
        for (index, prize) in prizes.iter().enumerate() {
            let row = (index / columns) as i32;
            let column = (index % columns) as i32;
            // Cell coordinates are relative to the grid, which is where the
            // cell records' own `Position` is measured.
            let cell = geometry.cell(crate::Frame::new(0.0, 0.0, 0.0, 0.0), row, column);
            let mut image = image_widget(
                &format!("prize {index}"),
                &prize.art,
                cell.x,
                cell.y,
                cell.w,
                cell.h,
            );
            if prize.art.is_empty() {
                image = image_widget(
                    &format!("prize {index}"),
                    "",
                    cell.x,
                    cell.y,
                    cell.w,
                    cell.h,
                );
            }
            grid.children.push(image);
        }
        Ok(())
    }
}

/// Hide the selection and highlight bars of a freshly filled grid.
/// The inventory's four buttons, by name (`FUN_004fd380` keeps them at the window's `+0x170`
/// workshop, `+0x174` scrap, `+0x178` export, `+0x17c` import).
pub const INVENTORY_BUTTONS: [&str; 4] = [
    "workshop button",
    "scrap button",
    "export button",
    "import button",
];

/// What the inventory grid has selected, as the buttons see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryPick {
    /// No cell: all four dim (a fresh window, a refill after a scrap or import; pixel-measured live: the four
    /// labels at 0.5 tint, max 127 of 255, in `teamhq-buttons-none`).
    Nothing,
    /// A robot's cell: Workshop, Scrap and Export, no Import.
    Robot,
    /// A `New Robot` cell: Workshop and Import (`FUN_004ff2e0`: the selected cell's flag `+0x16c`).
    Empty,
}

impl InventoryPick {
    /// Which buttons are enabled: `[workshop, scrap, export, import]`.
    pub fn enabled(self) -> [bool; 4] {
        match self {
            Self::Nothing => [false, false, false, false],
            Self::Robot => [true, true, true, false],
            Self::Empty => [true, false, false, true],
        }
    }
}

/// Dim or light the inventory's buttons for the grid's current selection (`FUN_004ff2e0`, the
/// grid's select handler, and the refill `FUN_004ff570`). The selected cell is slot
/// `column * rows + row`; it is a robot when the view has that slot.
pub fn sync_inventory_buttons(document: &mut Gib, view: &View) {
    let pick = match find_mut_in_gib(document, ROBOT_GRID).and_then(|grid| {
        let rows = Grid::from_widget(grid)?.rows.max(1) as usize;
        crate::input::selected_cell(grid).map(|(row, column)| column * rows + row)
    }) {
        None => InventoryPick::Nothing,
        Some(slot) if view.robots.iter().any(|r| r.slot == slot) => InventoryPick::Robot,
        Some(_) => InventoryPick::Empty,
    };
    for (name, on) in INVENTORY_BUTTONS.iter().zip(pick.enabled()) {
        if let Some(button) = find_mut_in_gib(document, name) {
            crate::input::set_button_enabled(button, on);
        }
    }
}

fn clear_bars(grid: &mut Widget) {
    crate::input::clear_selection(grid);
    if let Some(id) = Grid::from_widget(grid).and_then(|g| g.highlight) {
        for child in grid.children.iter_mut() {
            if crate::input::field_of(child, "ID").and_then(|v| v.trim().parse::<i32>().ok())
                == Some(id)
            {
                crate::input::set_field(child, "Visible", "false");
            }
        }
    }
}

/// Fill an already composed `teamhq.gib` document (the shell's own page, whose
/// content group holds the tab's screen) from a [`View`]: the team header and
/// the tab named by `button`. This is what the shell calls after it loads a tab
/// into `hq content group`.
pub fn fill_document(
    ui_root: &Path,
    document: &mut Gib,
    button: &str,
    view: &View,
) -> Result<(), String> {
    let Some(tab) = tab(button) else {
        return Ok(());
    };
    let taken = std::mem::replace(
        document,
        Gib {
            version: String::new(),
            blocks: Vec::new(),
        },
    );
    let mut hq = Hq {
        ui_root: ui_root.to_path_buf(),
        document: taken,
        tab: tab.button,
        view: view.clone(),
        slot_own: 0,
        slot_ready: false,
        screen_at: 0,
    };
    fill_team(&mut hq.document, view);
    let result = hq.fill_tab();
    *document = hq.document;
    result
}

/// Make every `SplitFrame`'s children fill their pane. See [`Hq::open`] for why
/// the HQ needs this; it is applied only to the screens loaded into the content
/// group, never to the shipped screen's own left/right split.
pub fn fit_splits_in(gib: &mut Gib) {
    for block in gib.blocks.iter_mut() {
        if let Block::Widget(widget) = block {
            fit_splits(widget);
        }
    }
}

fn fit_splits(widget: &mut Widget) {
    if widget.kind == WidgetKind::SplitFrame {
        // The pane fills along the split; across it the child keeps its own authored extent
        // (the calendar's `Slot1` is `Size 0.95 1 198 0` in a top/bottom split: 0.95 of the
        // pane wide, which is the schedule's 552 px at 1024x768, not the pane's 583).
        let horizontal = widget
            .field("Horizontal")
            .and_then(|f| f.flag())
            .unwrap_or(true);
        for child in widget.children.iter_mut() {
            let pair = |name: &str, second: bool, fallback: &str| -> String {
                let n = child
                    .field(name)
                    .map(|f| f.numbers())
                    .filter(|n| n.len() == 4)
                    .unwrap_or_default();
                match (n.is_empty(), second) {
                    (true, _) => fallback.to_string(),
                    (false, false) => format!("{} {}", n[0], n[1]),
                    (false, true) => format!("{} {}", n[2], n[3]),
                }
            };
            let (position, size) = if horizontal {
                (
                    format!("0 0 {}", pair("Position", true, "0 0")),
                    format!("1 1 {}", pair("Size", true, "1 1")),
                )
            } else {
                (
                    format!("{} 0 0", pair("Position", false, "0 0")),
                    format!("{} 1 1", pair("Size", false, "1 1")),
                )
            };
            crate::input::set_field(child, "Position", &position);
            crate::input::set_field(child, "Size", &size);
        }
    }
    for child in widget.children.iter_mut() {
        fit_splits(child);
    }
    for attachment in widget.attachments.iter_mut() {
        if let Some(body) = &mut attachment.body {
            fit_splits(body);
        }
    }
}

fn find_mut_in_gib<'a>(document: &'a mut Gib, name: &str) -> Option<&'a mut Widget> {
    document.blocks.iter_mut().find_map(|block| match block {
        Block::Widget(widget) => find_mut(widget, name),
        Block::Attachment(_) => None,
    })
}

/// A `.gib` under a game's `UI` directory, with its proxies inlined.
pub fn load(ui_root: &Path, relative: &str) -> Result<Gib, String> {
    let path = ra2fmt::find_ci(ui_root, relative)
        .ok_or_else(|| format!("{relative} is not in {}", ui_root.display()))?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut gib = ra2fmt::gib::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    crate::inline_proxies_in(&mut gib, ui_root);
    Ok(gib)
}

/// The tab a button selects.
pub fn tab(button: &str) -> Option<&'static Tab> {
    TABS.iter().find(|t| t.button == button)
}

fn tab_fragment(button: &str) -> Option<&'static str> {
    tab(button).map(|t| t.fragment)
}

/// The first widget called `name`, depth first.
pub fn find<'a>(widget: &'a Widget, name: &str) -> Option<&'a Widget> {
    if widget.name() == name {
        return Some(widget);
    }
    widget.children.iter().find_map(|c| find(c, name))
}

/// The first widget called `name`, to write.
pub fn find_mut<'a>(widget: &'a mut Widget, name: &str) -> Option<&'a mut Widget> {
    if widget.name() == name {
        return Some(widget);
    }
    widget.children.iter_mut().find_map(|c| find_mut(c, name))
}

fn find_mut_in<'a>(document: &'a mut Gib, name: &str) -> Option<&'a mut Widget> {
    document.blocks.iter_mut().find_map(|b| match b {
        Block::Widget(w) => find_mut(w, name),
        Block::Attachment(_) => None,
    })
}

fn bind_value_in(document: &mut Gib, name: &str, value: &str) {
    if let Some(widget) = find_mut_in(document, name) {
        set_value(widget, value);
    }
}

/// Write a value into a label: a `TextBox` keeps it in its `Text` child, a
/// `Text` in its own `_Text`.
fn set_value(widget: &mut Widget, value: &str) {
    if let Some(text) = widget
        .children
        .iter_mut()
        .find(|c| c.kind == WidgetKind::Text)
    {
        crate::input::set_field(text, "_Text", value);
    } else {
        crate::input::set_field(widget, "_Text", value);
    }
}

fn fill_team(document: &mut Gib, view: &View) {
    bind_value_in(document, TEAM_NAME, &view.team.name);
    bind_value_in(document, TEAM_MOTTO, &view.team.motto);
    if let Some(logo) = find_mut_in(document, TEAM_LOGO) {
        crate::input::set_field(logo, "_Filename", &view.team.logo);
    }
}

/// Fill `season_results.gib` from the same standings the league tab draws.
pub fn season_results(ui_root: &Path, view: &View) -> Result<Gib, String> {
    let mut gib = load(ui_root, RESULTS_SCREEN)?;
    let template = crate::cells::template(ui_root, crate::standings::RESULTS_TEMPLATE)?;
    let Some(grid) = find_mut_in(&mut gib, RESULTS_GRID) else {
        return Err(format!("{RESULTS_SCREEN} has no {RESULTS_GRID}"));
    };
    crate::standings::fill_results(grid, &template, &view.standings);
    bind_value_in(&mut gib, "title label", "SEASON RESULTS");
    Ok(gib)
}

/// The fastest win as the history screen formats it (`N/A` at the sentinel).
pub fn format_fastest(seconds: f32) -> String {
    if !seconds.is_finite() || seconds >= 10_000.0 {
        return "N/A".to_string();
    }
    let total = seconds.max(0.0) as u32;
    format!("{}:{:02}", total / 60, total % 60)
}

/// An `Image` record the prize grid can place: the same fields a shipped image
/// carries.
fn image_widget(name: &str, file: &str, x: f32, y: f32, w: f32, h: f32) -> Widget {
    let field = |name: &str, raw: &str| Field {
        name: Some(name.to_string()),
        raw: raw.to_string(),
    };
    Widget {
        kind: WidgetKind::Image,
        implicit: false,
        fields: vec![
            field("_Tiling", "false"),
            field("_Filename", file),
            field("_BufferWidth", "0 0 0 0"),
            field("_Size", &format!("{w} 0 {h} 0")),
            field("_Alpha", "true"),
            field("_Filtering", "0"),
            field("_ForeColor", "1 1 1 1"),
            field("_Name", ""),
            field("_Position", "0 0 0 0"),
            field("Name", name),
            field("ID", "-1"),
            field("Position", &format!("{x} 0 {y} 0")),
            field("Size", &format!("{w} 0 {h} 0")),
            field("Visible", "true"),
            field("ChildOrder", "1"),
            field("ClipMode", "0"),
            field("NumChildren", "0"),
        ],
        attachments: Vec::new(),
        children: Vec::new(),
    }
}

/// A `Text` record: the leaf `event_cell`/bracket labels are built from, with
/// the shipped faces' field names.
fn text_widget(name: &str, text: &str, rect: [f32; 4], font: &str, color: &str) -> Widget {
    text_widget_with(name, text, rect, font, color, false)
}

/// A `Text` record in pixels (`fractional = false`) or in fractions of its
/// parent.
fn text_widget_with(
    name: &str,
    text: &str,
    rect: [f32; 4],
    font: &str,
    color: &str,
    fractional: bool,
) -> Widget {
    let field = |name: &str, raw: &str| Field {
        name: Some(name.to_string()),
        raw: raw.to_string(),
    };
    let flag = if fractional { 1 } else { 0 };
    let [x, y, w, h] = rect;
    Widget {
        kind: WidgetKind::Text,
        implicit: false,
        fields: vec![
            field("_Font", font),
            field("_Text", text),
            field("_ForeColor", color),
            field("_Name", ""),
            field("_Position", "0 0 0 0"),
            field("Name", name),
            field("ID", "-1"),
            field("Position", &format!("{x} {flag} {y} {flag}")),
            field("Size", &format!("{w} {flag} {h} {flag}")),
            field("Visible", "true"),
            field("ChildOrder", "10"),
            field("ClipMode", "1"),
            field("NumChildren", "0"),
        ],
        attachments: Vec::new(),
        children: Vec::new(),
    }
}

/// The entrants of a 16-team bracket (`FUN_0046ad60`'s `16.0`).
pub const BRACKET_ENTRANTS: usize = 16;
/// The art of a bracket cell not drawn yet (`UI\teamhq\questionmark.bmp`).
pub const QUESTION_MARK: &str = "teamhq\\questionmark.bmp";

/// Fill `event_bracket.gib`'s `bracket group` from the career's bracket, laid
/// out for a 1280x1024 screen (the cells are fractions of the group, so the
/// picture scales with the window).
pub fn fill_bracket(gib: &mut Gib, bracket: &BracketView) {
    fill_bracket_at(gib, bracket, [1280.0, 1024.0]);
}

/// The bracket graphic the bracket window builds (`FUN_0046ad60`), for a
/// screen of `screen` pixels:
///
/// * one row per round plus the champion's (`FUN_00406d20() + 1`), 16 team
///   pictures in the first row, each next row `advance / bots` of the one
///   above (`FUN_004698f0`);
/// * the picture size is `min((w - 192) / 16, (h - 15 * rows) / rows)` of
///   the group less 6 px; the 16 first-row pictures spread over the width
///   (`(w - 16 * size) / 15` apart), the rows over the height;
/// * a later row's pictures sit centred under the match they come from;
/// * each match is joined to the next row by the bracket lines of
///   `FUN_0046b4d0` (4 px, black): a 13 px drop from its first and last
///   picture, the bar between them, and a 13 px stem down from its centre;
/// * a picture is the entrant team's logo (`FUN_004699b0` -> the team's
///   `+0x28`) up to the current round, `questionmark.bmp` beyond it;
/// * the player's picture in the current round has a 2 px blue frame 3 px
///   out (`FUN_0046b720`, colour `0 0 1 1`).
pub fn fill_bracket_at(gib: &mut Gib, bracket: &BracketView, screen: [f32; 2]) {
    let frame = crate::resolve(gib, screen)
        .iter()
        .flat_map(|p| p.walk())
        .find(|p| p.widget.name() == BRACKET_GROUP)
        .map(|p| p.frame);
    let Some(group) = find_mut_in(gib, BRACKET_GROUP) else {
        return;
    };
    group.children.clear();
    let Some(frame) = frame.filter(|f| f.w > 10.0 && f.h > 10.0) else {
        return;
    };
    for widget in bracket_widgets(bracket, frame.w, frame.h) {
        group.children.push(widget);
    }
    let children = group.children.len().to_string();
    crate::input::set_field(group, "NumChildren", &children);
}

/// One placed bracket picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BracketCell {
    pub row: usize,
    pub index: usize,
    /// Left, top and edge, in the group's pixels.
    pub x: f32,
    pub y: f32,
    pub size: f32,
}

/// Where `FUN_0046ad60` puts every picture for a bracket of `rounds` rounds
/// with `bots` robots per match and `advance` going on, in a group of
/// `w` x `h` pixels.
pub fn bracket_cells(
    rounds: usize,
    bots: usize,
    advance: usize,
    w: f32,
    h: f32,
) -> Vec<BracketCell> {
    let bots = bots.max(1);
    let advance = advance.clamp(1, bots);
    let (w, h) = (w - 6.0, h - 6.0);
    let rows = rounds + 1;
    let size = ((w - 192.0) / BRACKET_ENTRANTS as f32)
        .min((h - rows as f32 * 15.0) / rows as f32)
        .max(4.0);
    let vgap = if rows > 1 {
        (h - size * rows as f32) / (rows as f32 - 1.0)
    } else {
        0.0
    };
    let hgap = (w - size * BRACKET_ENTRANTS as f32) / (BRACKET_ENTRANTS as f32 - 1.0);
    let mut cells = Vec::new();
    // Row 0: the sixteen, centred.
    let n = BRACKET_ENTRANTS as f32;
    let x0 = w * 0.5 - size * n * 0.5 - (n * 0.5 - 0.5) * hgap;
    let mut previous: Vec<f32> = (0..BRACKET_ENTRANTS)
        .map(|i| x0 + i as f32 * (size + hgap))
        .collect();
    for (index, x) in previous.iter().enumerate() {
        cells.push(BracketCell {
            row: 0,
            index,
            x: 3.0 + x,
            y: 3.0,
            size,
        });
    }
    for row in 1..rows {
        let y = 3.0 + row as f32 * (size + vgap);
        let gap = hgap.min(size * 0.5);
        let mut next = Vec::new();
        for chunk in previous.chunks(bots) {
            let left = chunk[0];
            let right = chunk[chunk.len() - 1] + size;
            let span = size * advance as f32 + gap * (advance as f32 - 1.0);
            let start = (left + right) * 0.5 - span * 0.5;
            for k in 0..advance {
                next.push(start + k as f32 * (size + gap));
            }
        }
        for (index, x) in next.iter().enumerate() {
            cells.push(BracketCell {
                row,
                index,
                x: 3.0 + x,
                y,
                size,
            });
        }
        previous = next;
    }
    cells
}

fn bracket_widgets(bracket: &BracketView, w: f32, h: f32) -> Vec<Widget> {
    let rounds = bracket.rounds.len().max(1);
    let bots = bracket
        .rounds
        .iter()
        .flat_map(|r| r.matches.iter())
        .map(|m| m.slots.len())
        .max()
        .unwrap_or(2)
        .max(2);
    let cells = bracket_cells(rounds, bots, 1, w, h);
    let frac = |x: f32, y: f32, cw: f32, ch: f32| [x / w, y / h, cw / w, ch / h];
    let mut out = Vec::new();
    // Bracket lines first, so the pictures draw over their ends.
    for row in 0..rounds {
        let these: Vec<&BracketCell> = cells.iter().filter(|c| c.row == row).collect();
        let below = cells.iter().find(|c| c.row == row + 1);
        let Some(below) = below else { continue };
        for (m, chunk) in these.chunks(bots).enumerate() {
            let first = chunk[0];
            let last = chunk[chunk.len() - 1];
            let half = first.size * 0.5;
            let mid = (first.y + first.size + below.y) * 0.5;
            let left = first.x;
            let right = last.x + last.size;
            let centre = (left + right) * 0.5;
            let line =
                |name: String, r: [f32; 4]| bracket_line(&name, frac(r[0], r[1], r[2], r[3]));
            out.push(line(
                format!("bracket line {row}:{m}:a"),
                [left + half + 2.0 - 2.0, mid - 13.0, 4.0, 13.0],
            ));
            out.push(line(
                format!("bracket line {row}:{m}:b"),
                [left + half, mid - 2.0, (right - half) - (left + half), 4.0],
            ));
            out.push(line(
                format!("bracket line {row}:{m}:c"),
                [right - half - 2.0 - 2.0, mid - 13.0, 4.0, 13.0],
            ));
            out.push(line(
                format!("bracket line {row}:{m}:d"),
                [centre - 2.0, mid, 4.0, 13.0],
            ));
        }
    }
    let current = bracket.round;
    for cell in &cells {
        let slot = bracket.rounds.get(cell.row).and_then(|round| {
            let m = cell.index / bots;
            let k = cell.index % bots;
            if round.matches.len() == 1 && !round.current {
                // A played round keeps only the player's match (match 0).
                (m == 0).then(|| round.matches[0].slots.get(k)).flatten()
            } else {
                round
                    .matches
                    .get(m)
                    .and_then(|matched| matched.slots.get(k))
            }
        });
        let champion = cell.row == rounds
            && bracket.rounds.last().is_some_and(|r| r.played)
            && bracket
                .rounds
                .last()
                .and_then(|r| r.matches.first())
                .is_some_and(|m| m.won == Some(true));
        let art = if champion {
            bracket
                .rounds
                .first()
                .and_then(|r| r.matches.first())
                .and_then(|m| m.slots.iter().find(|s| s.player))
                .map(|s| s.logo.clone())
                .unwrap_or_else(|| QUESTION_MARK.to_string())
        } else if cell.row <= current {
            slot.filter(|s| !s.logo.is_empty())
                .map(|s| s.logo.clone())
                .unwrap_or_else(|| QUESTION_MARK.to_string())
        } else {
            QUESTION_MARK.to_string()
        };
        let mut image = image_widget(
            &format!("bracket cell {}:{}", cell.row, cell.index),
            &art,
            0.0,
            0.0,
            cell.size,
            cell.size,
        );
        let [x, y, cw, ch] = frac(cell.x, cell.y, cell.size, cell.size);
        crate::input::set_field(&mut image, "Position", &format!("{x} 1 {y} 1"));
        crate::input::set_field(&mut image, "Size", &format!("{cw} 1 {ch} 1"));
        crate::input::set_field(&mut image, "ChildOrder", "5");
        out.push(image);
    }
    // The player's frame on the current round (match 0, slot 0).
    if bracket.can_play {
        if let Some(cell) = cells.iter().find(|c| c.row == current && c.index == 0) {
            let (x0, y0) = (cell.x - 3.0, cell.y - 3.0);
            let e = cell.size + 6.0;
            for (i, r) in [
                [x0, y0, e, 2.0],
                [x0, y0 + e - 2.0, e, 2.0],
                [x0, y0, 2.0, e],
                [x0 + e - 2.0, y0, 2.0, e],
            ]
            .iter()
            .enumerate()
            {
                let mut edge =
                    bracket_line(&format!("player frame {i}"), frac(r[0], r[1], r[2], r[3]));
                crate::input::set_field(&mut edge, "FillColor", "0 0 1 1");
                crate::input::set_field(&mut edge, "ChildOrder", "6");
                out.push(edge);
            }
        }
    }
    out
}

/// A one-pixel black grid line: `pos` and `size` are `[x, mode, y, mode]` (mode 1 is a fraction
/// of the grid, 0 pixels).
fn grid_line(name: &str, pos: [f32; 4], size: [f32; 4]) -> Widget {
    let mut line = bracket_line(name, [0.0; 4]);
    crate::input::set_field(
        &mut line,
        "Position",
        &format!("{} {} {} {}", pos[0], pos[1] as i32, pos[2], pos[3] as i32),
    );
    crate::input::set_field(
        &mut line,
        "Size",
        &format!(
            "{} {} {} {}",
            size[0], size[1] as i32, size[2], size[3] as i32
        ),
    );
    line
}

/// A filled black bar at a fraction of the bracket group.
fn bracket_line(name: &str, rect: [f32; 4]) -> Widget {
    let field = |name: &str, raw: &str| Field {
        name: Some(name.to_string()),
        raw: raw.to_string(),
    };
    let [x, y, w, h] = rect;
    Widget {
        kind: WidgetKind::Rectangle,
        implicit: false,
        fields: vec![
            field("Filled", "true"),
            field("FillColor", "0 0 0 1"),
            field("BorderWidth", "0"),
            field("BorderColor", "0 0 0 0"),
            field("Name", name),
            field("ID", "-1"),
            field("Position", &format!("{x} 1 {y} 1")),
            field("Size", &format!("{w} 1 {h} 1")),
            field("Visible", "true"),
            field("ChildOrder", "4"),
            field("ClipMode", "0"),
            field("NumChildren", "0"),
        ],
        attachments: Vec::new(),
        children: Vec::new(),
    }
}

/// A schedule cell: the grid divides its own rectangle evenly into `Rows` x
/// `Columns` (the cells are laid out that way in [`Hq`]'s fill), so the hit
/// test and the drawing agree at every screen size, not the file's
/// design-time `RowHeight`/`ColWidths`.
fn schedule_cell(frame: Frame, rows: usize, columns: usize, row: usize, column: usize) -> Frame {
    let (w, h) = (frame.w / columns as f32, frame.h / rows as f32);
    Frame::new(frame.x + column as f32 * w, frame.y + row as f32 * h, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game_dir() -> Option<PathBuf> {
        ra2fmt::saves::isolate_for_tests();
        if let Some(dir) = std::env::var_os("RA2_GAME") {
            let dir = PathBuf::from(dir);
            return dir.is_dir().then_some(dir);
        }
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

    fn view(game: &Path) -> View {
        let teams = crate::standings::roster(game).unwrap();
        let mut points = vec![0; teams.len()];
        points[0] = 42;
        View {
            team: teams[0].clone(),
            robots: vec![
                RobotRow {
                    slot: 0,
                    name: "Big Fat Robot".into(),
                    class: "Heavy".into(),
                    preview: String::new(),
                    preview_image: None,
                },
                RobotRow {
                    slot: 1,
                    name: "Tiny".into(),
                    class: "Light".into(),
                    preview: String::new(),
                    preview_image: None,
                },
            ],
            standings: crate::standings::table(&teams, &points),
            events: Vec::new(),
            season: 2,
            events_remaining: 7,
            prizes: vec![
                PrizeRow {
                    art: "UI\\teamhq\\events\\trophies\\Gold Cup Trophy.tga".into(),
                    description: "won".into(),
                },
                PrizeRow {
                    art: "UI\\teamhq\\golden_sawblade.tga".into(),
                    description: "champion".into(),
                },
            ],
            stats: Stats {
                total_seasons: 2,
                current_rank: 1,
                best_rank: 3,
                total_events: 11,
                wins: 9,
                losses: 2,
                events_won: 4,
                highest_score: 248,
                fastest_win: 42.0,
            },
            bracket: None,
            completed: Vec::new(),
        }
    }

    fn text_of(widget: &Widget, name: &str) -> Option<String> {
        let found = find(widget, name)?;
        found
            .children
            .iter()
            .find(|c| c.kind == WidgetKind::Text)
            .and_then(|c| c.field("_Text"))
            .map(|f| f.text().to_string())
            .or_else(|| found.field("_Text").map(|f| f.text().to_string()))
    }

    /// Every tab's note names the file it loads, so the table cannot drift from
    /// the shipped screen.
    #[test]
    fn the_tabs_are_the_screens_their_notes_name() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let screen = load(&ui, SCREEN).unwrap();
        for tab in &TABS {
            let button = screen
                .walk()
                .find(|w| w.name() == tab.button)
                .unwrap_or_else(|| panic!("no {}", tab.button));
            assert!(
                button
                    .notes()
                    .contains(tab.fragment.rsplit('/').next().unwrap()),
                "{}'s note does not name {}: {:?}",
                tab.button,
                tab.fragment,
                button.notes()
            );
            assert!(
                ra2fmt::find_ci(&ui, tab.fragment).is_some(),
                "{}",
                tab.fragment
            );
        }
    }

    /// Opening the HQ puts the robots tab in the content group and fills its
    /// grid from the view: one cell per robot, the name and class the cell's
    /// notes name.
    #[test]
    fn the_robots_tab_is_filled_from_the_roster() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let mut hq = Hq::open(&ui, view(&game)).unwrap();
        assert_eq!(hq.tab(), "robots button");
        assert_eq!(
            hq.content_root().map(|w| w.name()),
            Some("robot inventory group")
        );
        let root = hq
            .document
            .blocks
            .iter_mut()
            .find_map(|b| match b {
                Block::Widget(w) => Some(w),
                _ => None,
            })
            .unwrap();
        let grid = find(root, ROBOT_GRID).unwrap();
        assert_eq!(
            grid.children.len(),
            8,
            "two bars, two robots and four New Robot cells (six cells, cell i is slot i)"
        );
        assert_eq!(
            text_of(&grid.children[4], BOT_NAME).as_deref(),
            Some("New Robot")
        );
        assert_eq!(
            text_of(&grid.children[2], BOT_NAME).as_deref(),
            Some("Big Fat Robot")
        );
        assert_eq!(
            text_of(&grid.children[2], BOT_CLASS).as_deref(),
            Some("Heavy")
        );
        assert_eq!(
            text_of(&grid.children[3], BOT_NAME).as_deref(),
            Some("Tiny")
        );
        // The header is the career's team.
        assert_eq!(text_of(root, TEAM_NAME).as_deref(), Some("The Rookies"));
        hq.select("league button").unwrap();
        assert_eq!(hq.tab(), "league button");
        assert_eq!(
            hq.content_root().map(|w| w.name()),
            Some("league standings split")
        );
    }

    /// The league tab fills `maybehq_league.gib`'s 16-row grid from the
    /// standings the view supplies, in rank order, with the player's points.
    #[test]
    fn the_league_grid_is_the_careers_standings() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let hq = Hq::open_on(&ui, view(&game), "league button").unwrap();
        let placed = hq.placements(crate::DESIGN_SCREEN, None);
        let grid = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == LEAGUE_GRID)
            .unwrap();
        assert_eq!(grid.children.len(), 2 + 16);
        // The content area is large enough for the league grid: the club's
        // first row and at least the next three rows are inside the clip.
        assert!(
            grid.clip.is_some_and(|clip| clip.h >= 280.0),
            "the league grid is clipped to one row: {:?}",
            grid.clip
        );
        for (i, cell) in grid.children.iter().skip(2).take(4).enumerate() {
            assert!(
                cell.clip.is_some_and(|clip| clip.h > 50.0),
                "row {i} is clipped away: {:?}",
                cell.clip
            );
        }
        let top = &grid.children[2];
        assert_eq!(text_of_name(top, "points").as_deref(), Some("42"));
        assert_eq!(
            text_of_name(top, "league team name").as_deref(),
            Some("The Rookies")
        );
        assert_eq!(text_of_name(top, "rank").as_deref(), Some("1"));
        // Season 2 is shown as Season 3; seven of nine remain.
        let root = hq.document.widgets().next().unwrap();
        assert_eq!(text_of(root, "season").as_deref(), Some("Season 3"));
        assert_eq!(
            text_of(root, "events remaining").as_deref(),
            Some("Events remaining: 7")
        );
    }

    fn text_of_name(p: &Placement<'_>, name: &str) -> Option<String> {
        let found = p.walk().find(|q| q.widget.name() == name)?;
        found
            .widget
            .children
            .iter()
            .find(|c| c.kind == WidgetKind::Text)
            .and_then(|c| c.field("_Text"))
            .map(|f| f.text().to_string())
    }

    /// The event grid's cell `i` is the schedule's event `i`, and a click at
    /// the cell's centre returns that event's id.
    #[test]
    fn an_event_cell_click_is_the_event_on_the_schedule() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let events = crate::events::all(&game).unwrap();
        let mut hq_view = view(&game);
        hq_view.events = vec![
            EventRow {
                id: 4,
                event: events[4].clone(),
                state: EventState::Available,
            },
            EventRow {
                id: 9,
                event: events[9].clone(),
                state: EventState::Available,
            },
            EventRow {
                id: 12,
                event: events[12].clone(),
                state: EventState::Available,
            },
        ];
        let mut hq = Hq::open_on(&ui, hq_view, "events button").unwrap();
        let point = hq
            .event_point(1, crate::DESIGN_SCREEN, None)
            .expect("the second cell");
        assert_eq!(hq.event_at(point, crate::DESIGN_SCREEN, None), Some(1));
        assert_eq!(
            hq.click(point, crate::DESIGN_SCREEN, None).unwrap(),
            Some(Click::Event(9))
        );
        assert_eq!(
            hq.click(point, crate::DESIGN_SCREEN, None).unwrap(),
            Some(Click::Event(9)),
            "the grid stays on screen after the click"
        );
    }

    /// The schedule's grid at 1024x768 is the original's: 552 px wide (`Slot1` is 0.95 of the split
    /// pane, whole pixels), one pixel line at the left of each column (x 341, 525, 709 live) and at
    /// the top of each row, and the art 3 px in from the line, 179 px wide (344..522).
    #[test]
    fn the_schedule_grid_has_shared_lines_and_art_inside_them() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let events = crate::events::all(&game).unwrap();
        let mut hq_view = view(&game);
        hq_view.events = events
            .iter()
            .take(9)
            .enumerate()
            .map(|(id, e)| EventRow {
                id,
                event: e.clone(),
                state: EventState::Available,
            })
            .collect();
        let hq = Hq::open_on(&ui, hq_view, "events button").unwrap();
        let placed = hq.placements([1024.0, 768.0], None);
        let all: Vec<_> = placed.iter().flat_map(|p| p.walk()).collect();
        let grid = all.iter().find(|p| p.widget.name() == EVENT_GRID).unwrap();
        assert_eq!(grid.frame.w, 552.0);
        let line = |name: &str| all.iter().find(|p| p.widget.name() == name).unwrap().frame;
        let xs: Vec<f32> = (0..3)
            .map(|c| line(&format!("grid line column {c}")).x.floor())
            .collect();
        assert_eq!(xs, [341.0, 525.0, 709.0]);
        assert_eq!(line("grid line column 1").w, 1.0);
        assert_eq!(line("grid line row 0").y.floor(), 318.0);
        assert_eq!(line("grid line row 0").w.round(), 552.0);
        assert_eq!(grid.frame.h.floor(), 333.0, "three rows of 111 (live)");
        let ys: Vec<f32> = (0..4)
            .map(|r| line(&format!("grid line row {r}")).y.floor())
            .collect();
        assert_eq!(ys, [318.0, 429.0, 540.0, 651.0], "live lines");
        assert_eq!(
            all.iter()
                .filter(|p| p.widget.name().starts_with("grid line row"))
                .count(),
            4,
            "a line under the last row too, none at the right edge"
        );
        let cell = all
            .iter()
            .find(|p| p.widget.name() == "event cell group")
            .unwrap();
        assert_eq!(cell.frame.x.floor(), 344.0, "line + BufferWidth 2");
        assert_eq!(cell.frame.w.round(), 179.0);
    }

    /// The history screen binds the career's counters and draws the prize art
    /// it stores, most recent first.
    #[test]
    fn the_history_tab_shows_the_counters_and_the_prizes() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let hq = Hq::open_on(&ui, view(&game), "history button").unwrap();
        let placed = hq.placements(crate::DESIGN_SCREEN, None);
        let at = |name: &str| {
            placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == name)
                .unwrap_or_else(|| panic!("no {name}"))
        };
        let value = |name: &str| text_of_name(at(name), name);
        assert_eq!(value("current rank").as_deref(), Some("1"));
        assert_eq!(value("wins").as_deref(), Some("9"));
        assert_eq!(value("events won").as_deref(), Some("4"));
        assert_eq!(value("fastest win").as_deref(), Some("0:42"));
        let grid = at(PRIZE_GRID);
        let prizes: Vec<_> = grid
            .children
            .iter()
            .filter(|c| c.widget.name().starts_with("prize "))
            .collect();
        assert_eq!(prizes.len(), 2);
        assert_eq!(
            prizes[0].widget.filename(),
            Some("UI\\teamhq\\events\\trophies\\Gold Cup Trophy.tga"),
            "most recently won first"
        );
        assert_eq!(
            prizes[1].widget.filename(),
            Some("UI\\teamhq\\golden_sawblade.tga")
        );
    }

    /// `season_results.gib` is the same standings with its own cell template.
    #[test]
    fn the_season_results_screen_is_the_same_standings() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let gib = season_results(&ui, &view(&game)).unwrap();
        let placed = crate::resolve(&gib, crate::DESIGN_SCREEN);
        let grid = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == RESULTS_GRID)
            .unwrap();
        assert_eq!(grid.children.len(), 2 + 16);
        assert_eq!(
            text_of_name(&grid.children[2], "points").as_deref(),
            Some("42")
        );
    }

    /// The binary fills the robot grid with `bot_cell.gib`; the screen's note
    /// says `robot_cell.gib`, which does not ship. The data wins and the
    /// difference is named.
    #[test]
    fn the_robot_cell_is_the_file_that_ships() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        assert!(ra2fmt::find_ci(&ui, ROBOT_TEMPLATE).is_some());
        assert!(ra2fmt::find_ci(&ui, "teamhq/robot_cell.gib").is_none());
    }

    /// The bracket group takes the shipped screen's generated `Text`s: the
    /// status line, a column per round and the player's next opponent.
    #[test]
    fn the_bracket_group_is_filled_from_the_view() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let mut gib = load(&ui, "teamhq/event_bracket.gib").unwrap();
        let view = BracketView {
            event: 0,
            round: 1,
            rounds: vec![
                BracketRoundView {
                    label: "Round 1".into(),
                    current: false,
                    played: true,
                    matches: vec![BracketMatchView {
                        slots: vec![
                            BracketSlot {
                                name: "The Rookies".into(),
                                player: true,
                                logo: "Teams\\orange_skull.bmp".into(),
                            },
                            BracketSlot {
                                name: "Red Zone".into(),
                                player: false,
                                logo: "AI\\redX.bmp".into(),
                            },
                        ],
                        player_match: true,
                        won: Some(true),
                    }],
                },
                BracketRoundView {
                    label: "Final Match".into(),
                    current: true,
                    played: false,
                    matches: vec![BracketMatchView {
                        slots: vec![
                            BracketSlot {
                                name: "The Rookies".into(),
                                player: true,
                                logo: "Teams\\orange_skull.bmp".into(),
                            },
                            BracketSlot {
                                name: "Blue Fury".into(),
                                player: false,
                                logo: "AI\\star.bmp".into(),
                            },
                        ],
                        player_match: true,
                        won: None,
                    }],
                },
            ],
            round_label: "Final Match".into(),
            status: "2 of 2 — next: Blue Fury".into(),
            next_opponent: Some("Blue Fury".into()),
            can_play: true,
        };
        fill_bracket(&mut gib, &view);
        let root = gib.widgets().next().unwrap();
        let group = find(root, BRACKET_GROUP).expect("the bracket group");
        let cell = |name: &str| {
            group
                .children
                .iter()
                .find(|c| c.name() == name)
                .and_then(|c| c.field("_Filename"))
                .map(|f| f.text().to_string())
        };
        // Two rounds of 2-bot matches would be 16 + 8 + 4 pictures; the
        // view's widest match decides the bots (2) and its rounds the rows.
        let pictures = group
            .children
            .iter()
            .filter(|c| c.name().starts_with("bracket cell "))
            .count();
        assert_eq!(pictures, 16 + 8 + 4);
        // Round 1 (played): the player's match keeps its two logos; the
        // rest of the row is unknown here.
        assert_eq!(
            cell("bracket cell 0:0").as_deref(),
            Some("Teams\\orange_skull.bmp")
        );
        assert_eq!(cell("bracket cell 0:1").as_deref(), Some("AI\\redX.bmp"));
        assert_eq!(cell("bracket cell 0:2").as_deref(), Some(QUESTION_MARK));
        // The current round shows its field; beyond it, question marks.
        assert_eq!(cell("bracket cell 1:1").as_deref(), Some("AI\\star.bmp"));
        assert_eq!(cell("bracket cell 2:0").as_deref(), Some(QUESTION_MARK));
        assert!(group.children.iter().any(|c| c.name() == "player frame 0"));
        assert!(group
            .children
            .iter()
            .any(|c| c.name() == "bracket line 0:0:b"));
    }

    /// `FUN_0046ad60`'s geometry: sixteen pictures across, each next row
    /// centred under its match, a 4-bot two-round bracket ending in one.
    #[test]
    fn the_bracket_cells_follow_the_recovered_layout() {
        let cells = bracket_cells(2, 4, 1, 1000.0, 400.0);
        let row = |r: usize| cells.iter().filter(|c| c.row == r).collect::<Vec<_>>();
        assert_eq!(row(0).len(), 16);
        assert_eq!(row(1).len(), 4);
        assert_eq!(row(2).len(), 1);
        let size = row(0)[0].size;
        assert!((size - ((994.0 - 192.0) / 16.0f32).min((394.0 - 45.0) / 3.0)).abs() < 1e-3);
        // The first row spans the width less 6 px.
        let last = row(0)[15];
        assert!((last.x + last.size - (3.0 + 994.0)).abs() < 0.5);
        // A next-row picture is centred under its match of four.
        let m0 = (row(0)[0].x + row(0)[3].x + size) / 2.0;
        assert!((row(1)[0].x + size / 2.0 - m0).abs() < 0.5);
        // The final's winner sits in the middle.
        assert!((row(2)[0].x + size / 2.0 - 500.0).abs() < 0.5);
    }

    #[test]
    fn the_fastest_win_formats_like_the_screen() {
        assert_eq!(format_fastest(10_000.0), "N/A");
        assert_eq!(format_fastest(42.0), "0:42");
        assert_eq!(format_fastest(125.0), "2:05");
    }
}
