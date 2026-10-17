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

    pub fn trace(&self) -> &[Step] {
        &self.trace
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
        self.trace.push(step.clone());
        Ok(step)
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
    let option = {
        let placed = ra2ui::resolve_with(&page.gib, size, Some(fonts));
        ra2ui::input::hit_control(&placed, x, y).and_then(|path| option_in(&chain(&placed, &path)))
    };
    if let Some((group, child)) = &option {
        for block in page.gib.blocks.iter_mut() {
            if let Block::Widget(root) = block {
                ra2ui::input::select_option(root, group, child);
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
