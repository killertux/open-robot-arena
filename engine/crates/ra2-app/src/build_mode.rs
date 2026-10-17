//! `--build`: the bot lab as a workshop that edits a live `ra2ui::workshop::Build`.
//!
//! The screen is the game's own: [`Shell`] boots the main menu and reaches
//! `botlab/botlabwin.gib` through the shipped route (Team HQ → robots → the
//! workshop button), and every tab and chassis step is the `.gib` fragment
//! `ra2ui::workshop::BotLab` loads. What this module adds is the state behind
//! those screens and the clicks the shipped C++ owns:
//!
//! - a [`Build`] for the current robot, a [`GameLibrary`] and the lists the
//!   screens' own `Notes` say to fill (the component palette, the armour grid,
//!   the wiring control grid);
//! - the palette click attaches the selected component through
//!   [`Build::attach`] (the F1 mount rule), and the model's parts, mounts and
//!   stats are drawn in a panel in the screen's own 3D placeholder;
//! - a live preview built from `Build::transforms()` and `ra2-render`'s item
//!   loader (`chassis_items` + `scene_items`), composited into that placeholder;
//! - paint, save/open, import/export and `Test Robot` (M8's [`Match`]).
//!
//! ## What is ours, not the game's
//!
//! The panel is a synthetic `.gib` in the 3D placeholder: the shipped screens
//! have no slot for a part list, a mount chooser or a face-group list, so one is
//! added there and named in `docs/17-workshop-ui.md`. New parts on the chassis
//! baseplate are placed at a fixed spread because there is no mouse-drag
//! placement tool yet; a component-to-component mount uses the model's F1 rule
//! and needs no placement at all. The camera framing is ours; the geometry and
//! transforms are the model's and the renderer's.
//!
//! ## Interaction that the shipped screen already has
//!
//! Tabs and chassis steps are the `BotLab`'s own clicks. The component palette
//! is the screen's `components grid`; a click on a cell selects the definition
//! and attaches it. The `remove button` is the shipped button. The armour grid
//! is the shipped `armor grid` on `armor.gib`; a click sets the armour row.
//!
//! Wiring is editable: the shipped `new analog`/`new switch`/`new button`
//! buttons start a placement, a click on a free cell of the shipped
//! `control grid` adds it, a component row plus an action row binds it through
//! the model's own matrix, and the shipped `delete button` removes it. This is
//! click-to-place then click-to-bind (the grid's note says drag; the model call
//! is the same `ControlBoard::add`).

use std::path::{Path, PathBuf};

use glam::{Mat4, Vec3};
use ra2_render::ui::{UiAssets, UiQuad, UiRenderer, UiScene, UiTexture};
use ra2_render::{DrawItem, RenderGroup, Renderer};
use ra2_shell::Shell;
use ra2fmt::gib::{Field, Gib, Widget, WidgetKind};
use ra2ui::components::Component as PaletteComponent;
use ra2ui::workshop::build::{Build, Placement as BuildPlacement};
use ra2ui::workshop::library::{GameLibrary, TextureEntry};
use ra2ui::workshop::paint::Texture;
use ra2ui::workshop::wiring::{Control, ControlKind};
use ra2ui::workshop::{self};
use ra2ui::{Frame, Placement};

use crate::SHELL_SIZE;

/// The stock chassis a new build starts from: a shipped AI bot whose chassis
/// has the full mesh, face groups and skin. Only its chassis is kept.
pub const STOCK_BOT: &str = "AI/Team1/Bot0.bot";
/// The shipped AI opponent `Test Robot` fights (a DEATHMATCH, two robots).
pub const TEST_OPPONENT: &str = "AI/Team6/Bot2.bot";
/// The default arena, the same one M8's `--match` uses.
pub const TEST_ARENA: &str = "Arenas/box.py";
/// The game's own robot-design folder.
pub const DESIGNS: &str = "Robot Designs";
/// The game's own custom-texture folder.
pub const CUSTOM_TEXTURES: &str = "Custom Textures";

/// `--build` options that the window and the offscreen runner share.
#[derive(Clone, Debug)]
pub struct Options {
    pub game: PathBuf,
    pub start: Option<PathBuf>,
    pub name: String,
    pub out: Option<PathBuf>,
    pub size: [u32; 2],
    pub seconds: f32,
}

/// The row-height and face of the synthetic panel. `fg_mcn12` is the shell's
/// small face; 12-pixel rows fit the placeholder's lower half.
const ROW_FACE: &str = "fg_mcn12";
const ROW_HEIGHT: f32 = 13.0;

/// The live workshop: the shell, the model, and the panel drawn over the 3D
/// placeholder.
pub struct Builder {
    game: PathBuf,
    ui: PathBuf,
    library: GameLibrary,
    build: Build,
    shell: Shell,
    assets: UiAssets,
    renderer: Renderer,
    palette: Vec<PaletteComponent>,
    shown: Vec<PaletteComponent>,
    selected: Option<PaletteComponent>,
    /// `(parent component, parent mount)` the next palette click attaches to.
    target: (usize, i32),
    /// The component a part row or the remove button acts on.
    selected_part: Option<usize>,
    /// The control kind waiting for a free grid cell (the shipped `new
    /// analog`/`new switch`/`new button` buttons set it), and the control and
    /// binding target the player has picked.
    wire_placing: Option<ControlKind>,
    wire_control: Option<[i32; 2]>,
    wire_component: Option<usize>,
    /// The palette row on top (the grid is a one-row window of a longer list).
    palette_scroll: usize,
    selected_texture: Option<TextureEntry>,
    name_buffer: String,
    name_edited: bool,
    designs: PathBuf,
    custom_textures: PathBuf,
    status: String,
    panel: Panel,
    revision: u64,
    preview: Option<(u64, Vec<u8>, u32, u32)>,
    test_requested: bool,
    test_seconds: f32,
    last_report: Option<ra2sim::r#match::MatchReport>,
}

/// The synthetic panel in the 3D placeholder.
struct Panel {
    gib: Gib,
    preview: Frame,
}

impl Builder {
    /// Boot the shell and reach the bot lab, then open `start` (or build from
    /// the stock chassis).
    pub fn open(game: &Path, start: Option<&Path>) -> Result<Builder, String> {
        let game = game
            .canonicalize()
            .map_err(|e| format!("{}: {e}", game.display()))?;
        let library = GameLibrary::open(&game).map_err(|e| e.to_string())?;
        let mut shell = Shell::boot(&game, SHELL_SIZE)?;
        // The shipped route, exactly as `ra2-shell`'s botlab route walks it.
        shell.click_named("Team HQ button")?;
        shell.click_named("continue button")?;
        shell.click_named("robots button")?;
        shell.click_named("workshop button")?;
        if shell.lab().is_none() {
            return Err(format!(
                "the shipped route did not reach {} (on {})",
                workshop::SCREEN,
                shell.screen_id()
            ));
        }

        let build = match start {
            Some(path) => {
                let candidates = if path.is_absolute() {
                    vec![path.to_path_buf()]
                } else {
                    vec![game.join(path), game.join(DESIGNS).join(path)]
                };
                let path = candidates
                    .into_iter()
                    .find(|p| p.is_file())
                    .or_else(|| {
                        let mut named = game.join(DESIGNS).join(path);
                        if named.extension().is_none() {
                            named.set_extension("bot");
                        }
                        named.is_file().then_some(named)
                    })
                    .ok_or_else(|| format!("{} is not a .bot file", path.display()))?;
                Build::load(&library, &path).map_err(|e| format!("{}: {e}", path.display()))?
            }
            None => {
                let stock =
                    GameLibrary::read_bot(&game.join(STOCK_BOT)).map_err(|e| e.to_string())?;
                let chassis = stock
                    .components
                    .first()
                    .and_then(|c| c.chassis.clone())
                    .ok_or_else(|| format!("{STOCK_BOT} has no chassis"))?;
                Build::new("M9 Bot", chassis).with_default_board()
            }
        };
        let name_buffer = build.name().to_string();
        let palette = ra2ui::components::all(&game)?;
        let assets = UiAssets::open(&game).map_err(|e| e.to_string())?;
        let renderer = Renderer::new().map_err(|e| e.to_string())?;
        let designs = game.join(DESIGNS);
        let custom_textures = game.join(CUSTOM_TEXTURES);
        let mut builder = Builder {
            ui: game.join("UI"),
            game,
            library,
            build,
            shell,
            assets,
            renderer,
            palette,
            shown: Vec::new(),
            selected: None,
            target: (0, 0),
            selected_part: None,
            wire_placing: None,
            wire_control: None,
            wire_component: None,
            palette_scroll: 0,
            selected_texture: None,
            name_buffer,
            name_edited: false,
            designs,
            custom_textures,
            status: String::new(),
            panel: Panel {
                gib: Gib {
                    version: "1.09".into(),
                    blocks: Vec::new(),
                },
                preview: Frame::new(0.0, 0.0, 1.0, 1.0),
            },
            revision: 0,
            preview: None,
            test_requested: false,
            test_seconds: 4.0,
            last_report: None,
        };
        builder.refresh()?;
        Ok(builder)
    }

    pub fn build(&self) -> &Build {
        &self.build
    }

    pub fn shell(&self) -> &Shell {
        &self.shell
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn last_report(&self) -> Option<&ra2sim::r#match::MatchReport> {
        self.last_report.as_ref()
    }

    /// The window stores the report when a test match ends; the headless runner
    /// reads it from [`crate::match_mode::run_headless`].
    pub fn set_report(&mut self, report: ra2sim::r#match::MatchReport) {
        if let Some(player) = report.robots.first() {
            self.status = format!(
                "test: {} damage taken, {} armour left",
                player.damage_taken, player.armor
            );
        }
        self.last_report = Some(report);
        self.refresh_panel();
    }

    pub fn take_test_request(&mut self) -> bool {
        std::mem::take(&mut self.test_requested)
    }

    /// The tab on screen, or `None` when the shell has left the workshop.
    pub fn tab(&self) -> Option<&str> {
        self.shell.tab()
    }

    /// A click at a point in the design screen. The panel and the screens'
    /// model-driving controls are checked first; anything else is the shell's
    /// own navigation.
    pub fn click(&mut self, x: f32, y: f32) -> Result<(), String> {
        if self.shell.lab().is_none() {
            self.shell.click(x, y)?;
            return Ok(());
        }
        if let Some(action) = self.panel_action(x, y) {
            return self.apply(&action);
        }
        if let Some(action) = self.widget_action(x, y) {
            return self.apply(&action);
        }
        let category = self.category_click(x, y);
        self.shell.click(x, y)?;
        if category {
            self.palette_scroll = 0;
        }
        self.refresh()
    }

    /// Whether a point lands on one of the palette's six category buttons.
    fn category_click(&self, x: f32, y: f32) -> bool {
        let placed = self.shell.placements();
        let Some(path) = ra2ui::input::hit(&placed, x, y) else {
            return false;
        };
        chain(&placed, &path).iter().any(|p| {
            ra2ui::components::CATEGORIES
                .iter()
                .any(|(_, button)| p.widget.name() == *button)
        })
    }

    /// A click by widget name — the same path a mouse takes, found by
    /// hit-testing the placed screen.
    pub fn click_named(&mut self, name: &str) -> Result<(), String> {
        let point = self.shell.point_for(name)?;
        self.click(point[0], point[1])
    }

    /// Click the synthetic panel row carrying `action`.
    pub fn click_action(&mut self, action: &str) -> Result<(), String> {
        let placed = ra2ui::resolve_with(&self.panel.gib, SHELL_SIZE, Some(self.assets.faces()));
        let row = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == action)
            .ok_or_else(|| format!("the panel has no row {action:?}"))?;
        let point = [
            row.frame.x + row.frame.w * 0.5,
            row.frame.y + row.frame.h * 0.5,
        ];
        self.click(point[0], point[1])
    }

    /// Click the palette cell that holds `file` (`Components\ztek1.txt`).
    pub fn click_palette_file(&mut self, file: &str) -> Result<(), String> {
        let index = self
            .shown
            .iter()
            .position(|c| c.file.eq_ignore_ascii_case(file))
            .ok_or_else(|| {
                format!(
                    "{file:?} is not in the current palette ({} cells: {})",
                    self.shown.len(),
                    self.shown
                        .iter()
                        .map(|c| c.file.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;
        let columns = self.palette_columns();
        let row = index / columns.max(1);
        if row != self.palette_scroll {
            self.palette_scroll = row;
            self.refresh_palette();
        }
        self.click_palette(index)
    }

    /// The palette grid's column count (`components.gib` says `Columns 4`).
    fn palette_columns(&self) -> usize {
        self.shell
            .lab()
            .and_then(|l| l.content_root())
            .and_then(|root| workshop::find(root, "components grid"))
            .and_then(ra2ui::Grid::from_widget)
            .map(|g| g.columns.max(1) as usize)
            .unwrap_or(4)
    }

    /// Click a cell of the shipped `control grid` (place or select a control).
    pub fn click_control_cell(&mut self, cell: [i32; 2]) -> Result<(), String> {
        let point = self.control_cell_point(cell)?;
        self.click(point[0], point[1])
    }

    /// Click the palette cell at `index` in the current category.
    pub fn click_palette(&mut self, index: usize) -> Result<(), String> {
        let point = self.palette_point(index)?;
        self.click(point[0], point[1])
    }

    /// The centre of a filled palette cell, in screen coordinates.
    pub fn palette_point(&self, index: usize) -> Result<[f32; 2], String> {
        let component = self
            .shown
            .get(index)
            .ok_or_else(|| format!("cell {index} is not in the palette"))?;
        let placed = self.shell.placements();
        let grid = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "components grid")
            .ok_or_else(|| "the components tab is not on screen".to_string())?;
        let cells: Vec<&Placement> = grid.children.iter().collect();
        let cell = cells
            .iter()
            .find(|c| c.widget.name() == format!("m9:cell:{index}"))
            .ok_or_else(|| {
                format!(
                    "the palette has no cell {index} for {} ({})",
                    component.name, component.art
                )
            })?;
        Ok([
            cell.frame.x + cell.frame.w * 0.5,
            // The category bar is drawn over the grid's upper half (the screen
            // says the grid is drawn over the buttons, but the labels and the
            // option group share the same 74 pixels), so the clickable strip is
            // at the cell's top edge, above the bar.
            cell.frame.y + 8.0,
        ])
    }

    /// Type one character into the filename box.
    pub fn type_char(&mut self, c: char) {
        if !(c.is_ascii_graphic() || c == ' ') {
            return;
        }
        if self.name_buffer.chars().count() >= 48 {
            return;
        }
        // The design is a file stem; separators are refused rather than
        // silently changed.
        if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            return;
        }
        if !self.name_edited {
            self.name_buffer.clear();
            self.name_edited = true;
        }
        self.name_buffer.push(c);
        self.refresh_panel();
    }

    /// Backspace in the filename box.
    pub fn backspace(&mut self) {
        self.name_buffer.pop();
        self.name_edited = true;
        self.refresh_panel();
    }

    /// Enter saves under the name in the box (Save As).
    pub fn enter(&mut self) -> Result<(), String> {
        self.save().map(|_| ())
    }

    /// Undo the placement model's last edit, or the paint's when the appearance
    /// tab is showing (the model keeps them separate, as the screens do).
    pub fn undo(&mut self) -> bool {
        let done = if self.tab() == Some("paintshop button") {
            self.build.paint_mut().undo()
        } else {
            self.build.undo()
        };
        if done {
            self.selected_part = None;
            self.refresh().ok();
        }
        done
    }

    /// Redo, the pair of [`Builder::undo`].
    pub fn redo(&mut self) -> bool {
        let done = if self.tab() == Some("paintshop button") {
            self.build.paint_mut().redo()
        } else {
            self.build.redo()
        };
        if done {
            self.selected_part = None;
            self.refresh().ok();
        }
        done
    }

    /// Save As to `Robot Designs/<name>.bot`.
    pub fn save(&mut self) -> Result<PathBuf, String> {
        let name = self.design_name();
        self.build.set_name(&name);
        std::fs::create_dir_all(&self.designs)
            .map_err(|e| format!("{}: {e}", self.designs.display()))?;
        let path = self.designs.join(format!("{name}.bot"));
        self.build
            .save_to(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        self.status = format!("saved {}", path.display());
        self.name_buffer = name;
        self.name_edited = false;
        Ok(path)
    }

    /// Open (import) a design from `Robot Designs/`. `name` is a stem or a
    /// relative path under that folder.
    pub fn open_design(&mut self, name: &str) -> Result<(), String> {
        let mut path = self.designs.join(name);
        if path.extension().is_none() {
            path.set_extension("bot");
        }
        let build =
            Build::load(&self.library, &path).map_err(|e| format!("{}: {e}", path.display()))?;
        self.name_buffer = build.name().to_string();
        self.name_edited = false;
        self.build = build;
        self.selected = None;
        self.selected_part = None;
        self.target = (0, 0);
        self.status = format!("opened {}", path.display());
        self.refresh()
    }

    /// Open the design just saved.
    pub fn reopen(&mut self) -> Result<(), String> {
        let name = self.design_name();
        self.open_design(&name)
    }

    /// The export half of the shipped `exportbot.gib`: the current build under
    /// the box's filename, in `Robot Designs/`.
    pub fn export_bot(&mut self, name: &str) -> Result<PathBuf, String> {
        let saved = self.save()?;
        let mut target = self.designs.join(name);
        if target.extension().is_none() {
            target.set_extension("bot");
        }
        if target != saved {
            std::fs::copy(&saved, &target).map_err(|e| format!("{}: {e}", target.display()))?;
        }
        self.status = format!("exported {}", target.display());
        Ok(target)
    }

    /// The import half of `importbot.gib`: the `.bot` files in `Robot Designs/`.
    pub fn designs(&self) -> Vec<String> {
        list_folder(&self.designs, &["bot"])
    }

    /// `import_texture.gib`'s list: the player's own images in `Custom Textures/`.
    pub fn custom_texture_names(&self) -> Vec<String> {
        list_folder(&self.custom_textures, &["bmp", "tga", "jpg", "jpeg", "png"])
    }

    /// Export the current chassis skin into `Custom Textures/` — the shipped
    /// export screen's "export the current texture" checkbox.
    pub fn export_texture(&mut self, name: &str) -> Result<PathBuf, String> {
        let mut target = self.custom_textures.join(name);
        if target.extension().is_none() {
            target.set_extension("tga");
        }
        std::fs::create_dir_all(&self.custom_textures)
            .map_err(|e| format!("{}: {e}", self.custom_textures.display()))?;
        let skin = self.build.paint().skin().ok_or_else(|| {
            "the chassis has no skin to export (a bot without a texture)".to_string()
        })?;
        std::fs::write(&target, skin.to_tga()).map_err(|e| format!("{}: {e}", target.display()))?;
        self.status = format!("exported texture {}", target.display());
        Ok(target)
    }

    /// `Test Robot`: save the build and hand it to M8's match as the player's
    /// robot. The player's bot is AI-driven, as `--match` drives both robots; the
    /// controls it was wired with are the model's and are not read by the AI.
    pub fn test_options(&mut self, size: [u32; 2]) -> Result<crate::match_mode::Options, String> {
        self.save()?;
        let path = self.designs.join(format!("{}.bot", self.design_name()));
        let opponent = self.game.join(TEST_OPPONENT);
        crate::match_mode::Options::new(
            &self.game,
            Some(self.game.join(TEST_ARENA)),
            vec![path, opponent],
            Some(self.test_seconds),
            None,
            None,
            [size[0].max(1), size[1].max(1)],
        )
    }

    fn design_name(&self) -> String {
        let name = self.name_buffer.trim();
        let name = name
            .strip_suffix(".bot")
            .or_else(|| name.strip_suffix(".BOT"))
            .unwrap_or(name);
        if name.is_empty() {
            "Untitled".to_string()
        } else {
            name.to_string()
        }
    }

    /// Apply an action named by the panel or a shipped control.
    fn apply(&mut self, action: &str) -> Result<(), String> {
        if let Some(rest) = action.strip_prefix("m9:tex:") {
            let index: usize = rest
                .parse()
                .map_err(|_| format!("bad texture row {rest:?}"))?;
            let entry = self
                .library
                .textures()
                .entries
                .get(index)
                .cloned()
                .ok_or_else(|| format!("texture {index} is not listed"))?;
            self.selected_texture = Some(entry.clone());
            self.status = format!("texture {}", entry.name);
            self.refresh_panel();
            return Ok(());
        }
        if let Some(rest) = action.strip_prefix("m9:ctex:") {
            let names = self.custom_texture_names();
            let name = names
                .get(rest.parse::<usize>().unwrap_or(usize::MAX))
                .ok_or_else(|| format!("custom texture {rest:?} is not listed"))?;
            let entry = TextureEntry {
                name: name.clone(),
                path: self.custom_textures.join(name),
            };
            self.selected_texture = Some(entry.clone());
            self.status = format!("custom texture {}", entry.name);
            self.refresh_panel();
            return Ok(());
        }
        if let Some(rest) = action.strip_prefix("m9:face:") {
            let group: usize = rest.parse().map_err(|_| format!("bad face row {rest:?}"))?;
            let Some(entry) = self.selected_texture.clone() else {
                self.status = "pick a texture first".into();
                self.refresh_panel();
                return Ok(());
            };
            let texture = Texture::load(&entry.path).map_err(|e| e.to_string())?;
            self.build
                .paint_mut()
                .fill_group(group, &texture)
                .map_err(|e| e.to_string())?;
            self.status = format!("painted face group {group} with {}", entry.name);
            return self.refresh();
        }
        if let Some(rest) = action.strip_prefix("m9:palette:") {
            let index: usize = rest
                .parse()
                .map_err(|_| format!("bad palette row {rest:?}"))?;
            let component = self
                .shown
                .get(index)
                .cloned()
                .ok_or_else(|| format!("palette cell {index} is not filled"))?;
            self.selected = Some(component.clone());
            self.attach_selected()?;
            return Ok(());
        }
        if action == "m9:remove" {
            let index = self
                .selected_part
                .or_else(|| self.build.components().len().checked_sub(1))
                .unwrap_or(0);
            if index == 0 {
                self.status = "the chassis cannot be removed".into();
                self.refresh_panel();
                return Ok(());
            }
            match self.build.remove(index) {
                Ok(removed) => {
                    self.selected_part = None;
                    self.status = format!("removed {removed} component(s)");
                    return self.refresh();
                }
                Err(e) => {
                    self.status = e.to_string();
                    self.refresh_panel();
                    return Ok(());
                }
            }
        }
        if let Some(rest) = action.strip_prefix("m9:mount:") {
            let (parent, mount) = rest
                .split_once(':')
                .ok_or_else(|| format!("bad mount row {rest:?}"))?;
            let parent: usize = parent
                .parse()
                .map_err(|_| format!("bad parent {parent:?}"))?;
            let mount: i32 = mount.parse().map_err(|_| format!("bad mount {mount:?}"))?;
            self.target = (parent, mount);
            self.status = format!("attach to component {parent} mount {mount}");
            self.refresh_panel();
            return Ok(());
        }
        if let Some(rest) = action.strip_prefix("m9:part:") {
            let index: usize = rest.parse().map_err(|_| format!("bad part row {rest:?}"))?;
            self.selected_part = Some(index);
            self.status = format!("selected component {index}");
            self.refresh_panel();
            return Ok(());
        }
        if let Some(rest) = action.strip_prefix("m9:new:") {
            let kind = match rest {
                "analog" => ControlKind::Analog,
                "switch" => ControlKind::Switch,
                "button" => ControlKind::Button,
                other => return Err(format!("unknown control kind {other:?}")),
            };
            self.wire_placing = Some(kind);
            self.status = format!("place a {}: click a free control grid cell", kind.label());
            self.refresh_panel();
            return Ok(());
        }
        if action == "m9:wirecancel" {
            self.wire_placing = None;
            self.status = "placement cancelled".into();
            self.refresh_panel();
            return Ok(());
        }
        if let Some(rest) = action.strip_prefix("m9:wirecell:") {
            let (x, y) = rest
                .split_once(':')
                .ok_or_else(|| format!("bad grid cell {rest:?}"))?;
            let cell = [
                x.parse().map_err(|_| format!("bad cell column {x:?}"))?,
                y.parse().map_err(|_| format!("bad cell row {y:?}"))?,
            ];
            self.place_control(cell);
            return Ok(());
        }
        if let Some(rest) = action.strip_prefix("m9:wirectl:") {
            let (x, y) = rest
                .split_once(':')
                .ok_or_else(|| format!("bad control cell {rest:?}"))?;
            let cell = [
                x.parse().map_err(|_| format!("bad cell column {x:?}"))?,
                y.parse().map_err(|_| format!("bad cell row {y:?}"))?,
            ];
            self.wire_placing = None;
            self.wire_control = Some(cell);
            self.wire_component = None;
            let name = self
                .build
                .board()
                .control_at(cell)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            self.status = format!("selected control {name}");
            self.refresh_panel();
            return Ok(());
        }
        if let Some(rest) = action.strip_prefix("m9:wirecomp:") {
            let index: usize = rest
                .parse()
                .map_err(|_| format!("bad component row {rest:?}"))?;
            self.wire_component = Some(index);
            let class = self
                .build
                .components()
                .get(index)
                .map(|c| c.class.clone())
                .unwrap_or_default();
            self.status = format!("bind to component {index} ({class}): click an action");
            self.refresh_panel();
            return Ok(());
        }
        if let Some(rest) = action.strip_prefix("m9:wireact:") {
            let (slot, index) = rest
                .split_once(':')
                .ok_or_else(|| format!("bad action row {rest:?}"))?;
            let slot: usize = slot.parse().map_err(|_| format!("bad slot {slot:?}"))?;
            let index: usize = index.parse().map_err(|_| format!("bad action {index:?}"))?;
            self.bind_selected(slot, index);
            return Ok(());
        }
        if action == "m9:wireremove" {
            let Some(cell) = self.wire_control else {
                self.status = "select a control first".into();
                self.refresh_panel();
                return Ok(());
            };
            match self.build.board_mut().remove(cell) {
                Some(control) => {
                    self.wire_control = None;
                    self.wire_component = None;
                    self.status = format!("removed control {}", control.name);
                    return self.refresh();
                }
                None => {
                    self.status = "there is no control there".into();
                    self.refresh_panel();
                    return Ok(());
                }
            }
        }
        if let Some(rest) = action.strip_prefix("m9:armor:") {
            let type_id: i32 = rest
                .parse()
                .map_err(|_| format!("bad armour row {rest:?}"))?;
            let def =
                self.library.armour_type(type_id).cloned().ok_or_else(|| {
                    format!("armour type {type_id} is not in Armor_Definitions.txt")
                })?;
            let changed = self.build.set_armour(def.clone());
            self.status = if changed {
                format!("armour {}", def.name)
            } else {
                format!("armour is already {}", def.name)
            };
            return self.refresh();
        }
        match action {
            "m9:undo" => {
                if !self.undo() {
                    self.status = "nothing to undo".into();
                }
            }
            "m9:redo" => {
                if !self.redo() {
                    self.status = "nothing to redo".into();
                }
            }
            "m9:save" => {
                if let Err(error) = self.save() {
                    self.status = error;
                    self.refresh_panel();
                }
            }
            "m9:open" => {
                let name = self.design_name();
                if let Err(error) = self.open_design(&name) {
                    self.status = error;
                    self.refresh_panel();
                }
            }
            "m9:exporttex" => {
                let name = format!("{}.tga", self.design_name());
                if let Err(error) = self.export_texture(&name) {
                    self.status = error;
                    self.refresh_panel();
                }
            }
            "m9:test" => {
                self.test_requested = true;
                self.status = "test robot requested".into();
            }
            other => return Err(format!("unknown workshop action {other:?}")),
        }
        self.refresh_panel();
        Ok(())
    }

    /// Attach the selected palette component at [`Builder::target`].
    ///
    /// The child's own mount is chosen from its attachment types so the model's
    /// socket rule (`Build::attach`) accepts it: the parent mount's type list
    /// must accept one of the child's. Chassis children get a fixed spread
    /// because there is no placement tool; component-to-component links use the
    /// model's F1 mount rule and `Placement::default()`.
    fn attach_selected(&mut self) -> Result<(), String> {
        let Some(component) = self.selected.clone() else {
            self.status = "pick a component in the palette".into();
            self.refresh_panel();
            return Ok(());
        };
        let info = self.library.component_info_by_file(&component.file);
        if !info.loaded {
            self.status = format!("{} did not load", component.file);
            self.refresh_panel();
            return Ok(());
        }
        let (parent, parent_mount) = self.target;
        let own_mount = choose_own_mount(&info, self.build.component_info(parent), parent_mount);
        let placement = if parent == 0 {
            let index = self
                .build
                .components()
                .iter()
                .filter(|c| c.parent == 0)
                .count() as f32;
            // Sit the part on the chassis' top plate. There is no mouse-drag
            // placement tool in this slice, and the baseplate's own rule is
            // "the stored transform decides", so this fixed spread is the UI's
            // placement, not a game rule.
            let top = self
                .build
                .chassis()
                .and_then(|c| c.mesh.as_ref())
                .map(|m| m.points.iter().map(|p| p[1]).fold(f32::MIN, f32::max))
                .unwrap_or(0.5);
            BuildPlacement::at([-0.4 + 0.4 * index, top + 0.05, 0.0])
        } else {
            BuildPlacement::default()
        };
        let before = self.build.components().len();
        match self.build.attach(
            &self.library,
            &component.file,
            parent,
            parent_mount,
            own_mount,
            placement,
        ) {
            Ok(attached) => {
                self.selected_part = Some(attached);
                self.status = format!(
                    "attached {} to {parent} mount {parent_mount} (component {attached})",
                    component.name
                );
                self.refresh()
            }
            Err(e) => {
                self.status = e.to_string();
                if self.build.components().len() != before {
                    return Err("a refused attach changed the component list".into());
                }
                self.refresh_panel();
                Ok(())
            }
        }
    }

    /// The action a click on the synthetic panel names, if any.
    fn panel_action(&self, x: f32, y: f32) -> Option<String> {
        let placed = ra2ui::resolve_with(&self.panel.gib, SHELL_SIZE, Some(self.assets.faces()));
        let path = ra2ui::input::hit(&placed, x, y)?;
        let chain = chain(&placed, &path);
        chain.iter().rev().find_map(|p| {
            p.widget
                .name()
                .strip_prefix("m9:")
                .map(|_| p.widget.name().to_string())
        })
    }

    /// The action a click on a shipped list cell names, if any: a component
    /// palette cell, the remove button, or an armour cell.
    fn widget_action(&self, x: f32, y: f32) -> Option<String> {
        let placed = self.shell.placements();
        let path = ra2ui::input::hit(&placed, x, y)?;
        let chain = chain(&placed, &path);
        if chain.iter().any(|p| p.widget.name() == "remove button") {
            return Some("m9:remove".into());
        }
        if self.tab() == Some("wiring button") {
            for (button, action) in [
                ("new analog", "m9:new:analog"),
                ("new switch", "m9:new:switch"),
                ("new button", "m9:new:button"),
                ("delete button", "m9:wireremove"),
            ] {
                if chain.iter().any(|p| p.widget.name() == button) {
                    return Some(action.into());
                }
            }
            if let Some(name) = chain
                .iter()
                .find_map(|p| p.widget.name().strip_prefix("m9:ctl:"))
            {
                return Some(format!("m9:wirectl:{name}"));
            }
            // An empty cell has no control clone, so the grid's own geometry
            // names it: this is the click-to-place target.
            if let Some(cell) = self.control_cell_at(x, y) {
                if self.build.board().control_at(cell).is_none() {
                    return Some(format!("m9:wirecell:{}:{}", cell[0], cell[1]));
                }
            }
        }
        if let Some(name) = chain
            .iter()
            .find_map(|p| p.widget.name().strip_prefix("m9:armor:"))
        {
            return Some(format!("m9:armor:{name}"));
        }
        // A palette cell is an `Image` whose `Name` is the rank `refresh_palette`
        // gave it. The placed children come in draw order, so the rank cannot
        // be read from the child index or from the art (a few stock components
        // share one preview bitmap).
        let hit = chain
            .iter()
            .find_map(|p| p.widget.name().strip_prefix("m9:cell:"));
        let rank: usize = match hit {
            Some(hit) => hit.parse().ok()?,
            // The selection bar is the grid's own first cell background.
            None if chain.iter().any(|p| p.widget.name() == "Sel Bar") => 0,
            None => return None,
        };
        Some(format!("m9:palette:{rank}"))
    }

    /// Recompute the panel and every list the current screen asks for.
    pub fn refresh(&mut self) -> Result<(), String> {
        self.revision = self.revision.wrapping_add(1);
        self.refresh_palette();
        self.refresh_armour();
        self.refresh_controls();
        self.normalize_target();
        self.refresh_panel();
        Ok(())
    }

    fn refresh_palette(&mut self) {
        let Some(tab) = self.shell.tab() else { return };
        if tab != "components button" {
            return;
        }
        let mut category = None;
        let scroll = self.palette_scroll;
        if let Some(lab) = self.shell.lab_mut() {
            for block in lab.document.blocks.iter_mut() {
                let ra2fmt::gib::Block::Widget(root) = block else {
                    continue;
                };
                if let Some((shown, _)) =
                    ra2ui::components::fill_selected(root, &self.game, &self.palette)
                {
                    category = Some(shown);
                    // The grid is `Rows 1, Columns 4`: only one row is in the
                    // window, and the shipped screen scrolls it. The cells are
                    // tagged with their rank (the placed children come in draw
                    // order, not archive order), then scrolled by hand so a
                    // component in a later row is clickable.
                    if let Some(grid) = workshop::find_mut(root, "components grid") {
                        if let Some(g) = ra2ui::Grid::from_widget(grid) {
                            let columns = g.columns.max(1) as usize;
                            let mut order: Vec<usize> = (0..grid.children.len())
                                .filter(|&i| {
                                    grid.children[i]
                                        .filename()
                                        .is_some_and(|f| f.starts_with("Components"))
                                })
                                .collect();
                            order.sort_by_key(|&i| {
                                let n = grid.children[i]
                                    .field("Position")
                                    .map(|f| f.numbers())
                                    .unwrap_or_default();
                                (
                                    n.get(2).copied().unwrap_or(0.0) as i64,
                                    n.first().copied().unwrap_or(0.0) as i64,
                                )
                            });
                            for (rank, &ci) in order.iter().enumerate() {
                                let row = rank / columns;
                                let cell = &mut grid.children[ci];
                                ra2ui::input::set_field(cell, "Name", &format!("m9:cell:{rank}"));
                                if row < scroll {
                                    ra2ui::input::set_field(cell, "Visible", "false");
                                } else {
                                    ra2ui::input::set_field(cell, "Visible", "true");
                                    ra2ui::input::set_field(
                                        cell,
                                        "Position",
                                        &format!(
                                            "{} 0 {} 0",
                                            g.column_x((rank % columns) as i32),
                                            g.row_height * (row - scroll) as f32
                                        ),
                                    );
                                }
                            }
                        }
                    }
                    break;
                }
            }
        }
        self.shown = match category {
            Some(category) => ra2ui::components::in_category(&self.palette, category)
                .into_iter()
                .cloned()
                .collect(),
            None => Vec::new(),
        };
    }

    fn refresh_armour(&mut self) {
        let Some(tab) = self.shell.tab() else { return };
        if tab != "chassis button" || self.shell.step() != Some("armor button") {
            return;
        }
        let Ok(template) = ra2ui::cells::template(&self.ui, "botlab/armor_cell.gib") else {
            return;
        };
        let armour = self.library.armour().to_vec();
        if let Some(lab) = self.shell.lab_mut() {
            for block in lab.document.blocks.iter_mut() {
                let ra2fmt::gib::Block::Widget(root) = block else {
                    continue;
                };
                let Some(grid) = workshop::find_mut(root, "armor grid") else {
                    continue;
                };
                if grid
                    .children
                    .iter()
                    .any(|c| c.name().starts_with("m9:armor:"))
                {
                    return;
                }
                let made = ra2ui::cells::fill(grid, &template, armour.len());
                let start = grid.children.len().saturating_sub(made);
                for (cell, def) in grid.children[start..].iter_mut().zip(&armour) {
                    ra2ui::input::set_field(cell, "Name", &format!("m9:armor:{}", def.type_id));
                    ra2ui::cells::bind_art(cell, "armor img", &def.texture);
                    ra2ui::cells::bind_prose(cell, &def.description);
                    ra2ui::cells::bind_value(
                        cell,
                        "weight value",
                        &format!("{}", def.weight as i32),
                    );
                }
                return;
            }
        }
    }

    fn refresh_controls(&mut self) {
        let Some(tab) = self.shell.tab() else { return };
        if tab != "wiring button" {
            return;
        }
        let Ok(template) = ra2ui::cells::template(&self.ui, "botlab/control_cell.gib") else {
            return;
        };
        let board = self.build.board().clone();
        if let Some(lab) = self.shell.lab_mut() {
            for block in lab.document.blocks.iter_mut() {
                let ra2fmt::gib::Block::Widget(root) = block else {
                    continue;
                };
                let Some(grid) = workshop::find_mut(root, "control grid") else {
                    continue;
                };
                grid.children.retain(|c| !c.name().starts_with("m9:ctl:"));
                let Some(g) = ra2ui::Grid::from_widget(grid) else {
                    return;
                };
                for control in &board.controls {
                    let x = g.column_x(control.cell[0]);
                    let y = g.row_height * control.cell[1] as f32;
                    for root in template.widgets() {
                        let mut cell = root.clone();
                        ra2ui::input::set_field(
                            &mut cell,
                            "Name",
                            &format!("m9:ctl:{}:{}", control.cell[0], control.cell[1]),
                        );
                        ra2ui::input::set_field(&mut cell, "Position", &format!("{x} 0 {y} 0"));
                        ra2ui::input::set_field(
                            &mut cell,
                            "Size",
                            &format!(
                                "{} 0 {} 0",
                                g.col_widths.first().copied().unwrap_or(66.0),
                                g.row_height
                            ),
                        );
                        let art = match control.kind {
                            ControlKind::Analog => "botlab\\ctrl_analog.bmp",
                            ControlKind::Switch => "botlab\\ctrl_switch.bmp",
                            ControlKind::Button => "botlab\\ctrl_button.bmp",
                        };
                        ra2ui::cells::bind_art(&mut cell, "control img", art);
                        // `control_cell.gib` was authored at 415×273 and its
                        // art files are 60×40; the renderer honours `_Size`, so
                        // without this the cell draws the top-left corner of a
                        // 415-pixel image and reads as a white square.
                        if let Some(img) = workshop::find_mut(&mut cell, "control img") {
                            ra2ui::input::set_field(img, "Position", "3 0 15 0");
                            ra2ui::input::set_field(img, "Size", "60 0 40 0");
                            ra2ui::input::set_field(img, "_Size", "60 0 40 0");
                        }
                        ra2ui::cells::bind_value(&mut cell, "control name", &control.name);
                        ra2ui::cells::bind_value(&mut cell, "control input", &control.key_label());
                        grid.children.push(cell);
                    }
                }
                return;
            }
        }
    }

    fn normalize_target(&mut self) {
        let n = self.build.components().len();
        let (parent, mount) = self.target;
        if parent >= n {
            self.target = (0, 0);
            return;
        }
        if parent == 0 {
            if mount != 0 {
                self.target = (0, 0);
            }
            return;
        }
        let used = self
            .build
            .components()
            .iter()
            .any(|c| c.parent == parent as i32 && c.parent_mount == mount);
        let exists = self
            .build
            .component_info(parent)
            .is_some_and(|info| info.has_mount(mount));
        if used || !exists {
            self.target = (0, 0);
        }
    }

    /// The shipped `control grid`'s geometry, when the wiring tab is on screen.
    fn control_grid(&self) -> Option<(ra2ui::Frame, ra2ui::Grid)> {
        let placed = self.shell.placements();
        let grid = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "control grid")?;
        let geometry = ra2ui::Grid::from_widget(grid.widget)?;
        Some((grid.frame, geometry))
    }

    /// The centre of a control-grid cell, for a scripted click and for a
    /// player's click-to-place.
    pub fn control_cell_point(&self, cell: [i32; 2]) -> Result<[f32; 2], String> {
        let (frame, grid) = self
            .control_grid()
            .ok_or_else(|| "the wiring tab's control grid is not on screen".to_string())?;
        let rect = grid.cell(frame, cell[1], cell[0]);
        Ok([rect.x + rect.w / 2.0, rect.y + rect.h / 2.0])
    }

    /// The cell a point inside the shipped `control grid` names, or `None` when
    /// it is outside the authors' `Rows`/`Columns` (3×4).
    fn control_cell_at(&self, x: f32, y: f32) -> Option<[i32; 2]> {
        let (frame, grid) = self.control_grid()?;
        for row in 0..grid.rows {
            for column in 0..grid.columns {
                if grid.cell(frame, row, column).contains_point(x, y) {
                    return Some([column, row]);
                }
            }
        }
        None
    }

    /// Add the pending control kind at a free cell. The shipped grid holds one
    /// control per cell (`ControlBoard::add`); the name is one of the corpus'
    /// own (`New Analog`, `Flip`, `New Button`).
    fn place_control(&mut self, cell: [i32; 2]) {
        if let Some(existing) = self.build.board().control_at(cell) {
            self.wire_placing = None;
            self.wire_control = Some(cell);
            self.status = format!("cell {},{} holds {}", cell[0], cell[1], existing.name);
            self.refresh_panel();
            return;
        }
        let Some(kind) = self.wire_placing else {
            self.status = "pick New Analog, New Switch or New Button first".into();
            self.refresh_panel();
            return;
        };
        let name = match kind {
            ControlKind::Analog => "New Analog",
            ControlKind::Switch => "Flip",
            ControlKind::Button => "New Button",
        };
        match self.build.board_mut().add(Control::new(name, cell, kind)) {
            Ok(()) => {
                self.wire_placing = None;
                self.wire_control = Some(cell);
                self.wire_component = None;
                self.status = format!("added {name} at cell {},{}", cell[0], cell[1]);
                if let Err(e) = self.refresh() {
                    self.status = e;
                }
            }
            Err(e) => {
                self.status = e.to_string();
                self.refresh_panel();
            }
        }
    }

    /// Bind the selected control to the selected component and action, through
    /// the model's matrix (`Build::bind_action`). One binding per slot: a new
    /// click replaces the row that was there.
    fn bind_selected(&mut self, slot: usize, action_index: usize) {
        let Some(cell) = self.wire_control else {
            self.status = "select a control cell first".into();
            self.refresh_panel();
            return;
        };
        let Some(component) = self.wire_component else {
            self.status = "pick the component to bind".into();
            self.refresh_panel();
            return;
        };
        let Some(control) = self.build.board().control_at(cell).cloned() else {
            self.status = "that cell holds no control".into();
            self.refresh_panel();
            return;
        };
        let class = self
            .build
            .components()
            .get(component)
            .map(|c| c.class.clone())
            .unwrap_or_default();
        let actions =
            ra2ui::workshop::wiring::ControlBoard::actions_for(control.kind, &class, slot);
        let Some(action) = actions.get(action_index).copied() else {
            self.status =
                format!("component {component} ({class}) has no action {action_index} here");
            self.refresh_panel();
            return;
        };
        self.build.board_mut().unbind(component, cell, slot);
        match self.build.bind_action(component, action, cell, slot) {
            Ok(()) => {
                self.status = format!(
                    "{} <- {} on {} slot {slot}",
                    class,
                    action.name(),
                    control.name
                );
                if let Err(e) = self.refresh() {
                    self.status = e;
                }
            }
            Err(e) => {
                self.status = e.to_string();
                self.refresh_panel();
            }
        }
    }

    fn free_mounts(&self, parent: usize) -> Vec<(i32, String)> {
        let Some(info) = self.build.component_info(parent) else {
            return Vec::new();
        };
        info.mounts
            .iter()
            .filter(|(mount, _)| {
                !self
                    .build
                    .components()
                    .iter()
                    .any(|c| c.parent == parent as i32 && c.parent_mount == **mount)
            })
            .map(|(mount, a)| {
                let types = if a.types.is_empty() {
                    "-".to_string()
                } else {
                    a.types.join(" ")
                };
                (*mount, types)
            })
            .collect()
    }

    /// Rebuild the synthetic panel: a background and one `Text` row per line,
    /// framed from the screen's own 3D placeholder.
    fn refresh_panel(&mut self) {
        let (preview, frame) = self.placeholder_frames();
        let mut rows: Vec<(String, String, [f32; 4])> = Vec::new();
        let stats = self.build.stats();
        let class = stats.computed_class.label();
        let header = format!(
            "{} | {:.1} kg | {:.0} hp | {} parts | {}",
            self.design_name(),
            stats.mass,
            stats.hitpoints,
            self.build.components().len(),
            class
        );
        rows.push((String::new(), header, [1.0, 1.0, 1.0, 1.0]));
        rows.push((
            "m9:save".into(),
            format!("Save as: {}_  [Enter]", self.name_buffer),
            [0.9, 1.0, 0.7, 1.0],
        ));
        rows.push((
            "m9:open".into(),
            format!("Open: Robot Designs/ ({} bots)", self.designs().len()),
            [0.8, 0.9, 0.9, 1.0],
        ));
        if !self.status.is_empty() {
            rows.push((String::new(), self.status.clone(), [0.8, 1.0, 0.8, 1.0]));
        }
        let tab = self.shell.tab().unwrap_or("");
        match tab {
            "components button" => {
                rows.push((
                    String::new(),
                    "Parts (click to select)".into(),
                    [0.6, 0.8, 1.0, 1.0],
                ));
                for (i, c) in self.build.components().iter().enumerate().take(3) {
                    let info = self.build.component_info(i);
                    let name = info
                        .map(|i| i.name.clone())
                        .unwrap_or_else(|| c.class.clone());
                    let mass = info.map(|i| i.mass).unwrap_or(0.0);
                    let label = if i == 0 {
                        format!("{i}: {name} ({mass:.1} kg)")
                    } else {
                        format!("{i}: {name} ({mass:.1} kg) <- {}", c.parent)
                    };
                    rows.push((format!("m9:part:{i}"), label, [0.9, 0.9, 0.9, 1.0]));
                }
                rows.push((String::new(), "Attach to".into(), [0.6, 0.8, 1.0, 1.0]));
                let (parent, mount) = self.target;
                rows.push((
                    "m9:mount:0:0".into(),
                    format!(
                        "{}chassis baseplate 0:0",
                        if (parent, mount) == (0, 0) { "> " } else { "" }
                    ),
                    [0.9, 1.0, 0.7, 1.0],
                ));
                for (i, c) in self.build.components().iter().enumerate().skip(1).take(1) {
                    let info = self.build.component_info(i);
                    let name = info
                        .map(|i| i.name.clone())
                        .unwrap_or_else(|| c.class.clone());
                    for (mount, types) in self.free_mounts(i).into_iter().take(3) {
                        rows.push((
                            format!("m9:mount:{i}:{mount}"),
                            format!(
                                "{}{i}: {name} mount {mount} ({types})",
                                if (parent, mount) == (i, mount) {
                                    "> "
                                } else {
                                    ""
                                }
                            ),
                            [0.9, 1.0, 0.7, 1.0],
                        ));
                    }
                }
                rows.push((
                    "m9:undo".into(),
                    "undo last attach (Ctrl+Z)".into(),
                    [0.9, 0.8, 0.5, 1.0],
                ));
                if let Some(component) = &self.selected {
                    rows.push((
                        String::new(),
                        format!(
                            "next: {} -> click a mount, then a palette cell",
                            component.name
                        ),
                        [0.7, 0.9, 0.7, 1.0],
                    ));
                }
            }
            "wiring button" => {
                rows.push((
                    String::new(),
                    "Control board: New ... then click a free cell (3x4)".into(),
                    [0.6, 0.8, 1.0, 1.0],
                ));
                if let Some(kind) = self.wire_placing {
                    rows.push((
                        "m9:wirecancel".into(),
                        format!(
                            "placing {}: click a free cell (or click to cancel)",
                            kind.label()
                        ),
                        [1.0, 0.9, 0.5, 1.0],
                    ));
                }
                for control in self.build.board().controls.iter().take(5) {
                    let selected = self.wire_control == Some(control.cell);
                    rows.push((
                        format!("m9:wirectl:{}:{}", control.cell[0], control.cell[1]),
                        format!(
                            "{}{} [{}] {} cell {},{}",
                            if selected { "> " } else { "" },
                            control.name,
                            control.kind.label(),
                            control.key_label(),
                            control.cell[0],
                            control.cell[1]
                        ),
                        [0.9, 0.9, 0.9, 1.0],
                    ));
                }
                // The board's saved rows come before the binding controls, so
                // the evidence frame shows the row the click wrote.
                rows.push((String::new(), "Bindings".into(), [0.6, 0.8, 1.0, 1.0]));
                for binding in self.build.board().bindings.iter().take(3) {
                    let target = self
                        .build
                        .components()
                        .get(binding.component.max(0) as usize)
                        .map(|c| c.class.clone())
                        .unwrap_or_else(|| format!("#{}", binding.component));
                    rows.push((
                        String::new(),
                        format!(
                            "{target} <- {} @ {},{} slot {}",
                            binding.action,
                            binding.control[0],
                            binding.control[1],
                            binding.control[2]
                        ),
                        [0.9, 0.9, 0.9, 1.0],
                    ));
                }
                if let Some(cell) = self.wire_control {
                    let control = self.build.board().control_at(cell).cloned();
                    let kind = control.as_ref().map(|c| c.kind);
                    rows.push((
                        String::new(),
                        format!(
                            "bind {} ({}) to:",
                            control.as_ref().map(|c| c.name.as_str()).unwrap_or("?"),
                            kind.map(|k| k.label()).unwrap_or("?")
                        ),
                        [0.6, 0.8, 1.0, 1.0],
                    ));
                    for (i, component) in self.build.components().iter().enumerate() {
                        let Some(kind) = kind else { break };
                        if ra2ui::workshop::wiring::ControlBoard::actions_for(
                            kind,
                            &component.class,
                            0,
                        )
                        .is_empty()
                        {
                            continue;
                        }
                        let selected = self.wire_component == Some(i);
                        rows.push((
                            format!("m9:wirecomp:{i}"),
                            format!(
                                "{}{i}: {} ({})",
                                if selected { "> " } else { "" },
                                component.class,
                                self.build
                                    .component_info(i)
                                    .map(|info| info.name.clone())
                                    .unwrap_or_else(|| component.class.clone())
                            ),
                            [0.9, 1.0, 0.7, 1.0],
                        ));
                    }
                    if let (Some(kind), Some(component)) = (kind, self.wire_component) {
                        let target = self.build.components().get(component);
                        for slot in 0..kind.slots() {
                            let Some(target) = target else { break };
                            let actions = ra2ui::workshop::wiring::ControlBoard::actions_for(
                                kind,
                                &target.class,
                                slot,
                            );
                            for (index, action) in actions.iter().enumerate() {
                                rows.push((
                                    format!("m9:wireact:{slot}:{index}"),
                                    format!("slot {slot}: {}", action.name()),
                                    [0.7, 0.9, 0.7, 1.0],
                                ));
                            }
                        }
                    }
                }
            }
            "paintshop button" => {
                rows.push((
                    String::new(),
                    "Texture Library/ (click, then a face)".into(),
                    [0.6, 0.8, 1.0, 1.0],
                ));
                for (i, entry) in self.library.textures().entries.iter().take(5).enumerate() {
                    let selected = self
                        .selected_texture
                        .as_ref()
                        .is_some_and(|t| t.name.eq_ignore_ascii_case(&entry.name));
                    rows.push((
                        format!("m9:tex:{i}"),
                        format!("{}{}", if selected { "> " } else { "" }, entry.name),
                        [0.9, 1.0, 0.7, 1.0],
                    ));
                }
                let custom = self.custom_texture_names();
                for (i, name) in custom.iter().take(2).enumerate() {
                    rows.push((
                        format!("m9:ctex:{i}"),
                        format!("custom: {name}"),
                        [0.8, 0.9, 0.8, 1.0],
                    ));
                }
                rows.push((String::new(), "Faces".into(), [0.6, 0.8, 1.0, 1.0]));
                for (i, group) in self.build.paint().groups().iter().take(4).enumerate() {
                    rows.push((
                        format!("m9:face:{i}"),
                        format!(
                            "face {i}: [{},{}]..[{},{}] {} tris",
                            group.bounds[0],
                            group.bounds[1],
                            group.bounds[2],
                            group.bounds[3],
                            group.triangles
                        ),
                        [0.9, 1.0, 0.7, 1.0],
                    ));
                }
                rows.push((
                    "m9:exporttex".into(),
                    "export texture -> Custom Textures/".into(),
                    [0.9, 0.8, 0.5, 1.0],
                ));
            }
            "chassis button" => {
                let armour = self
                    .build
                    .chassis()
                    .and_then(|c| c.armour)
                    .map(|a| {
                        self.library
                            .armour_type(a.type_id)
                            .map(|d| d.name.clone())
                            .unwrap_or_else(|| format!("type {}", a.type_id))
                    })
                    .unwrap_or_else(|| "-".into());
                rows.push((
                    String::new(),
                    format!(
                        "armour {armour} | chassis {:.1} kg / {:.0} hp",
                        stats.parts.first().map(|p| p.mass).unwrap_or(0.0),
                        stats.chassis_hitpoints
                    ),
                    [0.9, 0.9, 0.9, 1.0],
                ));
                if self.shell.step() == Some("armor button") {
                    rows.push((
                        String::new(),
                        "Plates (click a row; the shipped grid is shown too)".into(),
                        [0.7, 0.9, 0.7, 1.0],
                    ));
                    for def in self.library.armour() {
                        rows.push((
                            format!("m9:armor:{}", def.type_id),
                            format!("{}  {} kg", def.name, def.weight as i32),
                            [0.9, 1.0, 0.7, 1.0],
                        ));
                    }
                } else {
                    rows.push((
                        String::new(),
                        "structure design / armor are the tab's own buttons".into(),
                        [0.7, 0.9, 0.7, 1.0],
                    ));
                }
            }
            "test robot button" => {
                rows.push((
                    "m9:test".into(),
                    format!(
                        "Test Robot: {} vs {} in {}",
                        self.design_name(),
                        TEST_OPPONENT,
                        TEST_ARENA
                    ),
                    [0.9, 1.0, 0.7, 1.0],
                ));
                rows.push((
                    String::new(),
                    "click the row or the shipped Test Robot tab button".into(),
                    [0.9, 0.9, 0.9, 1.0],
                ));
                if let Some(report) = &self.last_report {
                    rows.push((
                        String::new(),
                        format!(
                            "last test: {} damage taken, {} armour left",
                            report.robots[0].damage_taken, report.robots[0].armor
                        ),
                        [0.9, 0.8, 0.5, 1.0],
                    ));
                }
            }
            _ => {
                rows.push((String::new(), "Parts".into(), [0.6, 0.8, 1.0, 1.0]));
                for (i, c) in self.build.components().iter().enumerate().take(6) {
                    let name = self
                        .build
                        .component_info(i)
                        .map(|i| i.name.clone())
                        .unwrap_or_else(|| c.class.clone());
                    rows.push((String::new(), format!("{i}: {name}"), [0.9, 0.9, 0.9, 1.0]));
                }
                rows.push((
                    String::new(),
                    "Save as: ".to_string() + &self.name_buffer + "_",
                    [0.9, 1.0, 0.7, 1.0],
                ));
            }
        }
        self.panel = make_panel(frame, preview, &rows);
    }
    fn placeholder_frames(&self) -> (Frame, Frame) {
        let placed = self.shell.placements();
        let placeholder = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| {
                p.widget.kind == WidgetKind::Placeholder
                    && p.widget.name().to_ascii_lowercase().starts_with("3d")
            })
            .map(|p| p.frame)
            .or_else(|| {
                placed
                    .iter()
                    .flat_map(|p| p.walk())
                    .find(|p| p.widget.name() == workshop::CONTENT_SLOT)
                    .map(|p| p.frame)
            })
            .unwrap_or(Frame::new(0.0, 0.0, SHELL_SIZE[0], SHELL_SIZE[1]));
        // The shell's own bottom bar is drawn over the content area; a panel
        // that ran under it would be unreadable and unclickable.
        let nav_top = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "common buts")
            .map(|p| p.frame.y)
            .unwrap_or(placeholder.bottom());
        let bottom = placeholder.bottom().min(nav_top - 2.0);
        let height = (bottom - placeholder.y).max(40.0);
        let preview_h = (height * 0.28).max(40.0);
        let preview = Frame::new(
            placeholder.x + 2.0,
            placeholder.y + 2.0,
            (placeholder.w - 4.0).max(10.0),
            preview_h,
        );
        let frame = Frame::new(
            placeholder.x + 2.0,
            preview.y + preview.h + 2.0,
            (placeholder.w - 4.0).max(10.0),
            (height - preview_h - 4.0).max(10.0),
        );
        (preview, frame)
    }

    /// Build the frame: the shell's own draw list, the preview composited into
    /// the 3D placeholder, then the synthetic panel over it.
    pub fn scene(&mut self, _target: [u32; 2]) -> Result<UiScene, String> {
        let mut scene = {
            let placed = self.shell.placements();
            self.assets.scene(&placed, [1280, 1005])
        };
        let preview_rect = self.panel.preview;
        if let Some((pixels, width, height)) = self.preview_pixels(preview_rect)? {
            let texture = scene.textures.len();
            scene.textures.push(UiTexture {
                label: "m9 preview".into(),
                width,
                height,
                rgba: pixels,
            });
            scene.quads.push(UiQuad {
                rect: preview_rect,
                corners: None,
                uv: [0.0, 0.0, 1.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
                texture,
                alpha: false,
                clip: Some(preview_rect),
            });
        }
        let panel_placed =
            ra2ui::resolve_with(&self.panel.gib, SHELL_SIZE, Some(self.assets.faces()));
        let panel_scene = self.assets.scene(&panel_placed, [1280, 1005]);
        let offset = scene.textures.len();
        scene.textures.extend(panel_scene.textures);
        for mut quad in panel_scene.quads {
            quad.texture += offset;
            scene.quads.push(quad);
        }
        scene.missing.extend(panel_scene.missing);
        Ok(scene)
    }

    /// Write the current frame through the same UI pipeline the window uses.
    pub fn shot(&mut self, path: &Path, size: [u32; 2]) -> Result<(), String> {
        let scene = self.scene(size)?;
        let renderer = &self.renderer;
        let ui = UiRenderer::new(renderer.device());
        let pixels = ui
            .render(renderer.device(), renderer.queue(), &scene)
            .map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        image::RgbaImage::from_raw(size[0], size[1], pixels)
            .ok_or_else(|| "the readback did not fill the image".to_string())?
            .save(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        println!("  wrote {}", path.display());
        Ok(())
    }

    /// Render the model's chassis and attached components into `rect`.
    ///
    /// The items come from `ra2_render`'s own loaders (`chassis_items`,
    /// `scene_items`) and the model is `Build::transforms()` — there is no
    /// second transform rule. The camera framing is ours.
    fn preview_pixels(&mut self, rect: Frame) -> Result<Option<(Vec<u8>, u32, u32)>, String> {
        if let Some((revision, pixels, w, h)) = &self.preview {
            if *revision == self.revision {
                return Ok(Some((pixels.clone(), *w, *h)));
            }
        }
        let width = rect.w.round().max(16.0) as u32;
        let height = rect.h.round().max(16.0) as u32;
        let bot = self.build.to_bot();
        let world = self.build.transforms();
        let chassis = ra2_render::chassis_items(&bot, Mat4::IDENTITY);
        let mut owned: Vec<(usize, Vec<DrawItem>)> = Vec::new();
        for i in 1..bot.components.len() {
            let Some(info) = self.build.component_info(i) else {
                continue;
            };
            let Some(def) = info.def.as_ref() else {
                continue;
            };
            let Some(scene) = self.library.load_component_mesh(def) else {
                continue;
            };
            let dir = self
                .library
                .component_mesh_path(def)
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| self.game.join("Components"));
            owned.push((
                i,
                ra2_render::scene_items(&scene, Mat4::IDENTITY, &dir, &info.name),
            ));
        }
        let mut groups: Vec<RenderGroup<'_>> = Vec::new();
        if !chassis.is_empty() && !world.is_empty() {
            groups.push(RenderGroup {
                items: &chassis,
                model: world[0],
            });
        }
        for (i, items) in &owned {
            if let Some(model) = world.get(*i) {
                groups.push(RenderGroup {
                    items,
                    model: *model,
                });
            }
        }
        if groups.is_empty() {
            return Ok(None);
        }
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for group in &groups {
            for item in group.items {
                for v in &item.vertices {
                    let p = group.model.transform_point3(Vec3::from(v.pos));
                    min = min.min(p);
                    max = max.max(p);
                }
            }
        }
        let center = (min + max) * 0.5;
        let radius = ((max - min).length() * 0.5).max(0.6);
        let eye = center + Vec3::new(radius * 1.9, radius * 1.2, radius * 2.1);
        let view = ra2_render::look_at_rh(eye, center, Vec3::Y);
        let proj = ra2_render::perspective_rh(
            45f32.to_radians(),
            width as f32 / height as f32,
            0.05,
            radius * 30.0,
        );
        let pixels = self
            .renderer
            .render_groups(&groups, proj * view, width, height)
            .map_err(|e| e.to_string())?;
        self.preview = Some((self.revision, pixels.clone(), width, height));
        Ok(Some((pixels, width, height)))
    }
}

/// One line of the synthetic panel.
fn make_panel(frame: Frame, preview: Frame, rows: &[(String, String, [f32; 4])]) -> Panel {
    let field = |name: &str, raw: &str| Field {
        name: Some(name.to_string()),
        raw: raw.to_string(),
    };
    let mut children = Vec::new();
    // A dark, semi-transparent backing so the text reads over the 3D view.
    children.push(Widget {
        kind: WidgetKind::Rectangle,
        implicit: false,
        fields: vec![
            field("Filled", "true"),
            field("FillColor", "0.02 0.03 0.05 0.72"),
            field("BorderWidth", "1"),
            field("BorderColor", "0.3 0.3 0.35 1"),
            field("Name", ""),
            field("ID", "-1"),
            field("Position", "0 0 0 0"),
            field("Size", "1 1 1 1"),
            field("Visible", "true"),
            field("ChildOrder", "-1"),
            field("ClipMode", "1"),
            field("NumChildren", "0"),
        ],
        attachments: Vec::new(),
        children: Vec::new(),
    });
    let mut y = 3.0;
    for (action, label, color) in rows {
        if y + ROW_HEIGHT > frame.h {
            break;
        }
        children.push(Widget {
            kind: WidgetKind::Text,
            implicit: false,
            fields: vec![
                field("_Font", ROW_FACE),
                field("_Text", label),
                field(
                    "_ForeColor",
                    &format!("{} {} {} {}", color[0], color[1], color[2], color[3]),
                ),
                field("_Name", ""),
                field("_Position", "0 0 0 0"),
                field("Name", action),
                field("ID", "-1"),
                field("Position", &format!("4 0 {y} 0")),
                field(
                    "Size",
                    &format!("{} 0 {} 0", (frame.w - 8.0).max(10.0), ROW_HEIGHT),
                ),
                field("Visible", "true"),
                field("ChildOrder", "0"),
                field("ClipMode", "1"),
                field("NumChildren", "0"),
            ],
            attachments: Vec::new(),
            children: Vec::new(),
        });
        y += ROW_HEIGHT;
    }
    let root = Widget {
        kind: WidgetKind::Group,
        implicit: false,
        fields: vec![
            field("ReportedPos", "0 0 0 0"),
            field("Alignment", "0"),
            field("AspectSetting", "0"),
            field("AspectRatio", "0"),
            field("PassMouse", "true"),
            field("Notes", ""),
            field("Name", "workshop panel"),
            field("ID", "-1"),
            field("Position", &format!("{} 0 {} 0", frame.x, frame.y)),
            field("Size", &format!("{} 0 {} 0", frame.w, frame.h)),
            field("Visible", "true"),
            field("ChildOrder", "3"),
            field("ClipMode", "1"),
            field("NumChildren", &children.len().to_string()),
        ],
        attachments: Vec::new(),
        children,
    };
    Panel {
        gib: Gib {
            version: "1.09".into(),
            blocks: vec![ra2fmt::gib::Block::Widget(root)],
        },
        preview,
    }
}

/// The child's own mount for a link: one the parent's mount accepts; else the
/// component's `base_f` when it has one; else its first mount. A missing type
/// list means "the data does not say", the same reading the model uses.
fn choose_own_mount(
    child: &ra2ui::workshop::library::ComponentInfo,
    parent: Option<&ra2ui::workshop::library::ComponentInfo>,
    parent_mount: i32,
) -> i32 {
    if let Some(point) = parent.and_then(|p| p.mounts.get(&parent_mount)) {
        if let Some((id, _)) = child
            .mounts
            .iter()
            .find(|(_, a)| a.types.iter().any(|t| point.accepts(t)))
        {
            return *id;
        }
    }
    if let Some((id, _)) = child
        .mounts
        .iter()
        .find(|(_, a)| a.types.iter().any(|t| t.eq_ignore_ascii_case("base_f")))
    {
        return *id;
    }
    child.mounts.keys().next().copied().unwrap_or(0)
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

fn list_folder(dir: &Path, extensions: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if extensions.iter().any(|e| ext.eq_ignore_ascii_case(e)) {
            if let Some(name) = path.file_name() {
                out.push(name.to_string_lossy().into_owned());
            }
        }
    }
    out.sort_by_key(|a| a.to_ascii_lowercase());
    out
}

/// The offscreen `--build` run: the scripted clicks, the frames, the save /
/// reload / rig comparison and the `Test Robot` hand-off.
pub fn run_headless(options: &Options) -> Result<(), String> {
    // The script layer is process-global; the match tests serialize on the same
    // lock. The window is single-threaded and does not need it.
    let _guard = ra2script::lock();
    let out = options
        .out
        .as_ref()
        .ok_or_else(|| "--build --out DIR is the offscreen path".to_string())?;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut builder = Builder::open(&options.game, options.start.as_deref())?;
    script(
        &mut builder,
        out,
        &options.name,
        true,
        options.seconds,
        options.size,
    )?;
    Ok(())
}

/// The demo script, shared by the offscreen run and the test.
///
/// Every step is a click at a point in the shell's own coordinates (or a
/// hit-tested widget name), and the text is typed one character at a time into
/// the filename box, as the window's keyboard path does.
#[allow(clippy::too_many_arguments)]
pub fn script(
    builder: &mut Builder,
    out: &Path,
    name: &str,
    frames: bool,
    seconds: f32,
    size: [u32; 2],
) -> Result<(), String> {
    let target = [size[0], size[1]];
    builder.test_seconds = seconds;
    let before = builder.build.stats();
    println!(
        "start: {} parts, {:.1} kg, {:.0} hp",
        builder.build.components().len(),
        before.mass,
        before.hitpoints
    );

    // 1. The chassis tab, and its own structure step.
    builder.click_named("chassis button")?;
    builder.click_named("structure design button")?;
    println!(
        "  chassis step: {:?} ({})",
        builder.shell.step(),
        builder
            .shell
            .lab()
            .and_then(|lab| lab.content_root())
            .map(|w| w.name())
            .unwrap_or("")
    );
    if frames {
        builder.shot(&out.join("build-chassis.png"), target)?;
    }

    // 2. The components tab: mechanics, then the Z-Tek motor's palette cell.
    builder.click_named("components button")?;
    builder.click_named("mechanics button")?;
    let motor_file = "Components\\ztek1.txt";
    let motor = builder.build.components().len();
    builder.click_palette_file(motor_file)?;
    if builder.build.components().len() != motor + 1 {
        return Err(format!(
            "the palette click attached {} components, not one",
            builder.build.components().len() - motor
        ));
    }
    // The motor's axle mount is the parent the wheel hangs off. Click the panel
    // row, as a player would.
    let axle = builder
        .free_mounts(motor)
        .into_iter()
        .find(|(_, types)| {
            types
                .split_whitespace()
                .any(|t| t.eq_ignore_ascii_case("axle_m"))
        })
        .map(|(mount, _)| mount)
        .ok_or_else(|| "the motor has no free axle_M mount".to_string())?;
    builder.click_action(&format!("m9:mount:{motor}:{axle}"))?;
    builder.click_named("mobility button")?;
    builder.click_palette_file("Components\\miniwheel.txt")?;
    if builder.build.components().len() != motor + 2 {
        return Err(format!(
            "the wheel did not attach to the motor: {} components",
            builder.build.components().len()
        ));
    }
    println!(
        "  attached: {}",
        builder
            .build
            .components()
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{i} {} -> {}@{}", c.class, c.parent, c.parent_mount))
            .collect::<Vec<_>>()
            .join(", ")
    );
    if frames {
        builder.shot(&out.join("build-parts.png"), target)?;
    }

    // 3. The wiring tab: add an analog control at a free cell, then bind it to
    // the motor's spin action. Click-to-place, then click-to-bind.
    builder.click_named("wiring button")?;
    builder.click_named("new analog")?;
    builder.click_control_cell([3, 1])?;
    if builder.build.board().control_at([3, 1]).is_none() {
        return Err("the New Analog button plus a free cell did not add a control".into());
    }
    println!(
        "  wiring: added {} at cell 3,1",
        builder
            .build
            .board()
            .control_at([3, 1])
            .map(|c| c.name.clone())
            .unwrap_or_default()
    );
    if frames {
        builder.shot(&out.join("wiring-add.png"), target)?;
    }
    builder.click_action("m9:wirecomp:1")?;
    builder.click_action("m9:wireact:0:0")?;
    let wired = builder.build().board().bindings.clone();
    let expected = wired.iter().any(|binding| {
        binding.component == 1 && binding.control == [3, 1, 0] && binding.action == "Spin Clockwise"
    });
    if !expected {
        return Err(format!("the bind click did not wire the motor: {wired:?}"));
    }
    println!(
        "  wiring: bound component 1 to {:?} on {:?} slot 0",
        wired.last().map(|b| b.action.as_str()).unwrap_or(""),
        builder
            .build()
            .board()
            .control_at([3, 1])
            .map(|c| c.name.as_str())
    );
    if frames {
        builder.shot(&out.join("wiring-bound.png"), target)?;
        builder.shot(&out.join("build-wiring.png"), target)?;
    }

    // 4. Paint: pick a texture from the shipped library, then a face group.
    builder.click_named("paintshop button")?;
    if !builder.build.paint().groups().is_empty() {
        builder.click_action("m9:tex:0")?;
        builder.click_action("m9:face:0")?;
    }
    println!(
        "  paint: {} group(s), changed {}",
        builder.build.paint().group_count(),
        builder.build.paint().is_changed()
    );
    if frames {
        builder.shot(&out.join("build-paint.png"), target)?;
    }

    // 4. Save As, typed into the filename box one character at a time.
    for c in name.chars() {
        builder.type_char(c);
    }
    builder.enter()?;
    let path = builder
        .designs
        .join(format!("{}.bot", builder.design_name()));
    if !path.is_file() {
        return Err(format!("{} was not written", path.display()));
    }
    println!("  saved {}", path.display());
    let stats = builder.build.stats();
    let live_components = builder.build.to_bot().components;
    let live_board = builder.build.board().clone();
    let live_skin = builder.build.paint().skin().cloned();

    // 5. Reload through the game's reader.
    builder.reopen()?;
    let reloaded = builder.build.stats();
    if builder.build.components() != live_components.as_slice() {
        return Err("the reloaded component tree is not the saved one".into());
    }
    let reloaded_board = builder.build.board();
    if reloaded_board != &live_board {
        return Err(format!(
            "the reloaded wiring is not the saved one: {reloaded_board:?} != {live_board:?}"
        ));
    }
    println!(
        "  wiring reloaded: {} control(s), {} binding(s)",
        reloaded_board.controls.len(),
        reloaded_board.bindings.len()
    );
    if reloaded != stats {
        return Err(format!(
            "the reloaded stats differ: {reloaded:?} != {stats:?}"
        ));
    }
    if let Some(skin) = &live_skin {
        let back = builder
            .build
            .paint()
            .skin()
            .ok_or_else(|| "the reloaded chassis has no skin".to_string())?;
        if back.pixels != skin.pixels || back.width != skin.width || back.height != skin.height {
            return Err("the painted skin did not survive save/reload".into());
        }
    }

    // 6. The rig's own numbers (the simulator's loader, not a second formula).
    let rig = ra2sim::rig::load(&path.to_string_lossy())
        .map_err(|e| format!("rig {}: {e}", path.display()))?;
    let rig_mass: f32 = rig.components.iter().map(|c| c.mass).sum();
    let rig_hp: f32 = rig.components.iter().map(|c| c.hitpoints).sum();
    println!(
        "  reloaded: {} parts, {:.1} kg, {:.0} hp (rig: {} parts, {:.1} kg, {:.0} hp)",
        builder.build.components().len(),
        reloaded.mass,
        reloaded.hitpoints,
        rig.components.len(),
        rig_mass,
        rig_hp
    );
    if (reloaded.mass - rig_mass).abs() > 0.01 || (reloaded.hitpoints - rig_hp).abs() > 0.01 {
        return Err(format!(
            "the model and the rig disagree: {:.1} kg/{:.0} hp vs {:.1} kg/{:.0} hp",
            reloaded.mass, reloaded.hitpoints, rig_mass, rig_hp
        ));
    }

    // 7. Test Robot: M8's match, with the built bot as the player's entry.
    if seconds > 0.0 {
        let options = builder.test_options(size)?;
        let match_out = out.join("test-frames");
        std::fs::create_dir_all(&match_out).map_err(|e| format!("{}: {e}", match_out.display()))?;
        let match_options = crate::match_mode::Options {
            out: Some(match_out.clone()),
            ..options
        };
        let report = crate::match_mode::run_headless(match_options, false)?;
        let first = match_out.join("fight-first.png");
        if frames && first.is_file() {
            std::fs::copy(&first, out.join("build-test.png"))
                .map_err(|e| format!("{}: {e}", first.display()))?;
        }
        if let Some(player) = report.robots.first() {
            println!(
                "  test: {} — {:.0} damage taken, {:.0} armour, {:.0} battery",
                player.bot_name, player.damage_taken, player.armor, player.battery
            );
        }
        builder.last_report = Some(report);
        if frames {
            println!("  wrote {}", out.join("build-test.png").display());
        }
        std::fs::remove_dir_all(&match_out).ok();
    }

    println!(
        "done: {} parts, {:.1} kg, {:.0} hp; {:.0} damage taken in the test",
        builder.build.components().len(),
        reloaded.mass,
        reloaded.hitpoints,
        builder
            .last_report
            .as_ref()
            .map(|r| r.robots.first().map(|r| r.damage_taken).unwrap_or(0.0))
            .unwrap_or(0.0)
    );

    // The player's install is not ours to litter: the evidence run deletes the
    // design it wrote after re-reading it. The test uses a symlinked copy of the
    // game, so its `Robot Designs/` is a temp directory.
    std::fs::remove_file(&path).ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2ui::workshop::wiring::{Action, ControlBoard};
    use std::path::PathBuf;

    fn game_dir() -> Option<PathBuf> {
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

    /// A click on a palette row attaches exactly one component, and the model's
    /// undo reverses it.
    #[test]
    fn a_palette_click_attaches_one_component_and_undo_reverses_it() {
        let Some(game) = game_dir() else { return };
        let mut builder = Builder::open(&game, None).unwrap();
        builder.click_named("components button").unwrap();
        builder.click_named("mechanics button").unwrap();
        let before = builder.build.components().len();
        builder.click_palette_file("Components\\ztek1.txt").unwrap();
        assert_eq!(builder.build.components().len(), before + 1);
        assert!(builder.build.can_undo());
        assert!(builder.undo());
        assert_eq!(builder.build.components().len(), before);
    }

    /// M9.3's flow: the shipped `new analog` button, a free `control grid`
    /// cell, a component row and an action row. The saved board is the board
    /// that is read back.
    #[test]
    fn adding_a_control_binding_it_and_saving_round_trips_the_same_board() {
        let Some(game) = game_dir() else { return };
        let mut builder = Builder::open(&game, None).unwrap();
        builder.click_named("components button").unwrap();
        builder.click_named("mechanics button").unwrap();
        builder.click_palette_file("Components\\ztek1.txt").unwrap();
        assert_eq!(builder.build.components()[1].class, "SpinMotor");
        builder.click_named("wiring button").unwrap();
        let before = builder.build.board().controls.len();
        builder.click_named("new analog").unwrap();
        builder.click_control_cell([3, 1]).unwrap();
        assert_eq!(builder.build.board().controls.len(), before + 1);
        let added = builder.build.board().control_at([3, 1]).unwrap().clone();
        assert_eq!(added.kind, ControlKind::Analog);
        assert_eq!(added.name, "New Analog");

        builder.click_action("m9:wirecomp:1").unwrap();
        builder.click_action("m9:wireact:0:0").unwrap();
        assert_eq!(
            builder.build.board().bindings.last(),
            Some(&ra2fmt::bot::Binding {
                component: 1,
                action: "Spin Clockwise".into(),
                control: [3, 1, 0],
            })
        );
        assert!(builder.build.wiring_errors().is_empty());

        let saved = builder.build.save();
        let bot = ra2fmt::bot::parse(&saved).unwrap();
        let reread = ControlBoard::from_bot(&bot).unwrap();
        assert_eq!(&reread, builder.build.board());

        // An invalid action for this kind is the model's own refusal.
        let err = builder
            .build
            .bind_action(1, Action::OnOffClockwise, [3, 1], 0)
            .unwrap_err();
        assert!(
            matches!(
                err,
                ra2ui::workshop::wiring::WiringError::ActionNotAllowed { .. }
            ),
            "{err:?}"
        );
        assert!(err.to_string().contains("Spin Clockwise"), "{err}");

        // One control per cell: New Analog over Forward's cell adds nothing.
        let count = builder.build.board().controls.len();
        builder.click_named("new analog").unwrap();
        builder.click_control_cell([1, 0]).unwrap();
        assert_eq!(builder.build.board().controls.len(), count);
        assert_eq!(
            builder.build.board().control_at([1, 0]).unwrap().name,
            "Forward"
        );
    }

    /// A click on one of the shipped armour cells sets that plate on the model.
    #[test]
    fn an_armour_cell_sets_the_chassis_plate() {
        let Some(game) = game_dir() else { return };
        let mut builder = Builder::open(&game, None).unwrap();
        builder.click_named("chassis button").unwrap();
        builder.click_named("armor button").unwrap();
        let before = builder.build.chassis().unwrap().armour.unwrap().type_id;
        assert_eq!(before, 0, "the stock chassis is plastic");
        builder.click_action("m9:armor:2").unwrap();
        let armour = builder.build.chassis().unwrap().armour.unwrap();
        assert_eq!(armour.type_id, 2, "steel");
        assert_eq!(armour.weight, 20.0);
        assert_eq!(
            armour.hardness, 350.0,
            "the bot records twice the editor's 175"
        );
        assert!(builder.build.can_undo());
        // The writer stores the plate row; a reread is the same armour.
        let bytes = builder.build.save();
        let back = ra2fmt::bot::parse(&bytes).unwrap();
        let wrote = back.components[0]
            .chassis
            .as_ref()
            .and_then(|c| c.armour)
            .unwrap();
        assert_eq!(wrote, armour);
    }

    /// The full scripted run: save to a temp game's `Robot Designs/`, reload,
    /// compare model == reloaded == rig, and write the four frames.
    #[test]
    #[cfg(unix)]
    fn the_scripted_build_saves_reloads_and_matches_the_rig() {
        let Some(game) = game_dir() else { return };
        let dir = std::env::temp_dir().join(format!("ra2-build-game-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        for entry in std::fs::read_dir(&game).unwrap().flatten() {
            let name = entry.file_name();
            if name == DESIGNS || name == CUSTOM_TEXTURES {
                continue;
            }
            std::os::unix::fs::symlink(entry.path(), dir.join(&name)).unwrap();
        }
        std::fs::create_dir_all(dir.join(DESIGNS)).unwrap();
        std::fs::create_dir_all(dir.join(CUSTOM_TEXTURES)).unwrap();

        let out = std::env::temp_dir().join(format!("ra2-build-frames-{}", std::process::id()));
        std::fs::remove_dir_all(&out).ok();
        std::fs::create_dir_all(&out).unwrap();
        let options = Options {
            game: dir.clone(),
            start: None,
            name: format!("M9 Test {}", std::process::id()),
            out: Some(out.clone()),
            size: [640, 502],
            seconds: 0.6,
        };
        run_headless(&options).expect("the scripted build runs");
        for name in [
            "build-chassis.png",
            "build-parts.png",
            "build-wiring.png",
            "wiring-add.png",
            "wiring-bound.png",
            "build-paint.png",
            "build-test.png",
        ] {
            let path = out.join(name);
            let image = image::open(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
                .into_rgba8();
            assert_eq!(image.dimensions(), (640, 502), "{name}");
        }
        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&out).ok();
    }
}
