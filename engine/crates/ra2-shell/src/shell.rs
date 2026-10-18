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

use ra2fmt::gib::{Block, Gib};
use ra2ui::workshop::{self, BotLab};
use ra2ui::{Fonts, Placement};

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
}

enum Screen {
    Page(Page),
    Lab(BotLab),
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
    /// The game type the picker last selected, in the original's spelling.
    game_type_choice: Option<String>,
    /// The arena's `hazard checkbox`, when the loaded arena script has one.
    hazard_choice: Option<bool>,
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
            game_type_choice: None,
            hazard_choice: None,
        })
    }

    pub fn game(&self) -> &Path {
        &self.game
    }

    pub fn size(&self) -> [f32; 2] {
        self.size
    }

    /// The screen on display, relative to `UI`.
    pub fn screen_id(&self) -> String {
        match &self.screen {
            Screen::Page(page) => page.key.clone(),
            Screen::Lab(_) => workshop::SCREEN.to_string(),
        }
    }

    /// The fragment in the page's slot, or the workshop's own tab/step screen.
    pub fn content_id(&self) -> Option<String> {
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

    /// Click one row of a `ListBox`/`Grid`, by the list's own geometry.
    ///
    /// The point is the row's centre inside the placed list; a row below the
    /// list's clip is not reachable, exactly as a mouse would find it. The
    /// click then goes through [`Shell::click`], so the row selection is
    /// recorded in the same document the frame draws.
    pub fn click_list_row(&mut self, list_name: &str, row: usize) -> Result<Step, String> {
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
        // The visible list, not the authored `ColWidths` sum: the arena list's
        // 300+17 is clipped to the 159-pixel frame the scroller gives it.
        let x = list.frame.x + list.frame.w / 2.0;
        let y = list.frame.y + grid.row_height * row as f32 + grid.row_height / 2.0;
        if let Some(clip) = list.clip {
            if !clip.contains_point(x, y) {
                return Err(format!(
                    "row {row} of {list_name:?} is clipped out of view ({x:.1},{y:.1})"
                ));
            }
        }
        self.click(x, y)
    }

    /// The placed tree, with the screen's own faces.
    pub fn placements(&self) -> Vec<Placement<'_>> {
        match &self.screen {
            Screen::Page(page) => ra2ui::resolve_with(&page.gib, self.size, Some(&self.fonts)),
            Screen::Lab(lab) => lab.placements(self.size, Some(&self.fonts)),
        }
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
        let replacement = match &mut self.screen {
            Screen::Page(page) => click_page(
                &self.ui,
                &self.game,
                &self.fonts,
                self.size,
                page,
                x,
                y,
                &mut step,
            )?,
            Screen::Lab(lab) => click_lab(&self.fonts, self.size, lab, x, y, &mut step)?,
        };
        if let Some(target) = replacement {
            self.screen = load_screen(&self.ui, &target)?;
        }
        step.screen = self.screen_id();
        step.content = self.content_id();
        step.tab = self.tab().map(str::to_string);
        step.step = self.step().map(str::to_string);
        self.remember_choices();
        self.trace.push(step.clone());
        Ok(step)
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
            let selected = ra2ui::input::selected_row(list).and_then(|row| {
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
            if page {
                ra2ui::input::hit_control(placed, x, y)
            } else {
                ra2ui::input::hit(placed, x, y)
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
        let placed: Vec<Placement<'_>> = match &self.screen {
            Screen::Page(page) => ra2ui::resolve_with(&page.gib, self.size, Some(assets.faces())),
            Screen::Lab(lab) => lab.placements(self.size, Some(assets.faces())),
        };
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
    }))
}

/// A page's planned effect, worked out before the document is changed.
enum Plan {
    Nothing,
    Replace { target: String },
    Load { target: String, slot: String },
}

fn plan_page(page: &Page, fonts: &Fonts, size: [f32; 2], x: f32, y: f32, step: &mut Step) -> Plan {
    let placed = ra2ui::resolve_with(&page.gib, size, Some(fonts));
    let Some(path) = ra2ui::input::hit_control(&placed, x, y) else {
        step.hit = "nothing".into();
        return Plan::Nothing;
    };
    let chain = chain(&placed, &path);
    step.hit = chain.last().map(|p| describe(p)).unwrap_or_default();
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
) -> Result<Option<String>, String> {
    let plan = plan_page(page, fonts, size, x, y, step);
    // What the click changes inside the screen: an option-group member, a list
    // row, or a standalone checkbox. All three are the data's own controls; the
    // row geometry is the list's `RowHeight` and its `Sel Bar`.
    let (option, row, checkbox) = {
        let placed = ra2ui::resolve_with(&page.gib, size, Some(fonts));
        let Some(path) = ra2ui::input::hit_control(&placed, x, y) else {
            return Ok(None);
        };
        let chain = chain(&placed, &path);
        let option = option_in(&chain);
        let row = chain
            .iter()
            .rev()
            .find(|p| p.widget.kind == ra2fmt::gib::WidgetKind::ListBox)
            .and_then(|list| {
                let grid = ra2ui::Grid::from_widget(list.widget)?;
                if grid.row_height <= 0.0 {
                    return None;
                }
                let index = ((y - list.frame.y) / grid.row_height).floor();
                if index < 0.0 {
                    return None;
                }
                let index = index as usize;
                (index < ra2ui::input::row_count(list.widget))
                    .then_some((list.widget.name().to_string(), index))
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
        (option, row, checkbox)
    };
    if let Some((group, child)) = &option {
        for block in page.gib.blocks.iter_mut() {
            if let Block::Widget(root) = block {
                ra2ui::input::select_option(root, group, child);
            }
        }
    }
    if let Some((list, index)) = &row {
        for block in page.gib.blocks.iter_mut() {
            if let Block::Widget(root) = block {
                if let Some(widget) = ra2ui::workshop::find_mut(root, list) {
                    ra2ui::input::select_row(widget, *index);
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
        Plan::Replace { target, .. } => Ok(Some(target)),
        Plan::Load { target, slot, .. } => {
            load_into(page, ui, game, &slot, &target)?;
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
    page.content.push((slot.to_string(), key.clone()));
    fill_lists(&mut page.gib, game)?;
    Ok(())
}

/// Fill the lists whose own `Notes` say what fills them.
///
/// The arena picker is the shipped case: its listbox says *"populate with arena
/// names"*, and the names come from the game's own arena scripts
/// (`ra2ui::arenas`). The source and the destination are both named by the
/// data, so this is not a table of screens.
fn fill_lists(gib: &mut Gib, game: &Path) -> Result<(), String> {
    let wants_arenas = gib.walk().any(|w| {
        w.notes()
            .trim()
            .eq_ignore_ascii_case("populate with arena names")
    });
    if !wants_arenas {
        return Ok(());
    }
    let arenas = ra2ui::arenas::all(game)?;
    for block in gib.blocks.iter_mut() {
        let Block::Widget(root) = block else { continue };
        let Some(list) = workshop::find_mut(root, ra2ui::arenas::LIST) else {
            continue;
        };
        ra2ui::arenas::fill_list(list, &arenas);
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
