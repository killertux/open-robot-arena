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
//! - the palette click selects a component and the shipped `comp_preview`
//!   group shows it; the Attach button starts a drag that ends in
//!   [`Build::drop_part`] (the F1 mount rule, `ra2ui::workshop::placement`,
//!   docs/36 §9.18), and the model's parts and stats are drawn in a panel in
//!   the screen's own 3D placeholder;
//! - a live preview built from `Build::transforms()` and `ra2-render`'s item
//!   loader (`chassis_items` + `scene_items`), composited into that placeholder;
//! - paint, save/open, import/export and `Test Robot` (M8's [`Match`]).
//!
//! ## What is ours, not the game's
//!
//! The panel is a synthetic `.gib` in the 3D placeholder: the shipped screens
//! have no slot for a part list, a mount chooser or a face-group list, so one is
//! added there and named in `docs/17-workshop-ui.md`. New parts on the chassis
//! baseplate and on other parts are placed by the original's drag (select,
//! Attach, drag, click; SHIFT turns, CTRL raises). The camera framing, the pick
//! radius and the overlap test are ours; the geometry and transforms are the
//! model's and the renderer's.
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
use ra2ui::workshop::build::{Build, BuildError};
use ra2ui::workshop::chassis::{self, Outline};
use ra2ui::workshop::library::{ComponentInfo, GameLibrary, TextureEntry};
use ra2ui::workshop::paint::Texture;
use ra2ui::workshop::placement::{self, Candidate, Drag, Hover, Refusal};
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

/// `--debug-panel`: draw the synthetic edit panel (a text list of parts, mass
/// and the save name) under the robot view. The original's lab has no such
/// list, so the player-facing screens leave it off; the scripted harness runs
/// keep it because they drive the model through its rows.
static DEBUG_PANEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Turn the synthetic panel on for the player-facing screens (`--debug-panel`).
pub fn set_debug_panel_default(on: bool) {
    DEBUG_PANEL.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// `--build` options that the window and the offscreen runner share.
#[derive(Clone, Debug)]
pub struct Options {
    pub game: PathBuf,
    pub start: Option<PathBuf>,
    pub name: String,
    pub out: Option<PathBuf>,
    pub size: [u32; 2],
    pub seconds: f32,
    /// Show one tab on the `--start` design, write its frame and stop.
    pub view: Option<String>,
}

/// The row-height and face of the synthetic panel. `fg_mcn12` is the shell's
/// small face; 12-pixel rows fit the placeholder's lower half.
const ROW_FACE: &str = "fg_mcn12";
const ROW_HEIGHT: f32 = 13.0;
/// A `Text` in a group is placed by its baseline (`ra2ui::place`), so a
/// row's text is written one ascent of `fg_mcn12` below the row's top; at
/// the top it was drawn above its own clip and showed only its lower half.
const ROW_BASELINE: f32 = 10.0;

/// Which paint operation the Paint Shop is in. The three shipped screens are
/// `texturefill.gib`, `surfacelayer.gib` and `decals.gib`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaintMode {
    Fill,
    Layer,
    Decal,
}

impl PaintMode {
    fn label(self) -> &'static str {
        match self {
            PaintMode::Fill => "texture fill",
            PaintMode::Layer => "surface layer",
            PaintMode::Decal => "decal",
        }
    }
}

/// Which automatic outline the chassis step-1 drawing tool makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrawStyle {
    Freeform,
    Circle,
    Rectangle,
}

impl DrawStyle {
    fn label(self) -> &'static str {
        match self {
            DrawStyle::Freeform => "freeform",
            DrawStyle::Circle => "circle",
            DrawStyle::Rectangle => "rectangle",
        }
    }
}

/// The camera the last preview frame used, so a click can be unprojected back
/// into the bot's own space. The framing is ours; the geometry is the model's.
#[derive(Clone, Copy, Debug)]
struct PreviewCamera {
    view_proj: Mat4,
    inv_view_proj: Mat4,
    width: u32,
    height: u32,
}

impl PreviewCamera {
    /// The ray through a point of the preview's rectangle.
    fn ray(&self, rect: Frame, x: f32, y: f32) -> Option<(Vec3, Vec3)> {
        if self.width == 0 || self.height == 0 || rect.w <= 0.0 || rect.h <= 0.0 {
            return None;
        }
        let nx = ((x - rect.x) / rect.w) * 2.0 - 1.0;
        let ny = 1.0 - ((y - rect.y) / rect.h) * 2.0;
        let near = self.inv_view_proj.project_point3(Vec3::new(nx, ny, 0.0));
        let far = self.inv_view_proj.project_point3(Vec3::new(nx, ny, 1.0));
        Some((near, far - near))
    }
}

/// The live workshop: the shell, the model, and the panel drawn over the 3D
/// placeholder.
pub struct Builder {
    /// The size the shell is laid out at (1280x1005 for the window; the
    /// harness lays it out natively at the capture's size).
    size: [f32; 2],
    /// The fidelity harness's view: the 3D view fills its placeholder and the
    /// synthetic edit panel is not drawn.
    view_only: bool,
    /// Workshop sounds waiting for the audio drain (attach, removal).
    sounds: Vec<crate::audio::UiSound>,
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
    /// The green box chosen in the component preview: the selected part's own
    /// attachment point (`FUN_0042e650` marks the first one).
    own_mount: i32,
    /// The style (`Components\Styles\...`) picked in the `Component option`
    /// dropdown; the palette's own definition when `None`.
    style_file: Option<String>,
    /// The part being dragged after the Attach button (`FUN_0042c3a0`).
    drag: Option<ActiveDrag>,
    /// The last mouse position, and the SHIFT/CTRL state the window reports.
    cursor: Option<[f32; 2]>,
    shift: bool,
    ctrl: bool,
    /// The `comp_preview` panel's 3D view: the part and its green boxes.
    comp_preview: Option<CompPreview>,
    /// The definition the loaded `comp_preview` group was filled for: the
    /// group is rebuilt only when it changes, so an open style list survives
    /// the refresh after its own click.
    comp_key: Option<String>,
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
    paint_mode: PaintMode,
    selected_decal: Option<TextureEntry>,
    selected_group: Option<usize>,
    decal_at: [f32; 2],
    decal_scale: f32,
    /// The current chassis-drawing state.
    draw_style: DrawStyle,
    draw_centre: Option<[i32; 2]>,
    draw_point: Option<usize>,
    draw_snap: bool,
    shape_radius: i32,
    shape_length: i32,
    shape_width: i32,
    name_buffer: String,
    name_edited: bool,
    designs: PathBuf,
    custom_textures: PathBuf,
    status: String,
    panel: Panel,
    revision: u64,
    preview: Option<(u64, Vec<u8>, u32, u32)>,
    preview_camera: Option<PreviewCamera>,
    test_requested: bool,
    test_seconds: f32,
    last_report: Option<ra2sim::r#match::MatchReport>,
    /// The team robot this workshop session edits (`Teams/Team<N>/Bot<M>.bot`)
    /// when it was opened from the Team HQ's inventory: leaving the lab writes
    /// the design back there, so the HQ, the event join window and the
    /// exhibition pickers all see the edited robot.
    team_target: Option<PathBuf>,
    /// The team a robot built from an empty inventory selection joins when
    /// the lab is left (`FUN_004fee90` opens the lab for the HQ's team).
    team_new: Option<usize>,
}

/// Where a scripted drag drops its part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlaceOn {
    /// A bot-local `(x, z)` on the baseplate, in metres.
    Baseplate([f32; 2]),
    /// A free attachment point of a placed part.
    Mount { parent: usize, mount: i32 },
}

/// Write a paragraph into a widget that carries it twice: its `Glyph`'s `_Text`
/// and the markup of the `DisplayBox` that dresses it (and that box's body).
fn set_prose(widget: &mut Widget, text: &str) {
    if widget.field("_Text").is_some() {
        ra2ui::input::set_field(widget, "_Text", text);
    }
    for attachment in widget.attachments.iter_mut() {
        if attachment.kind == ra2fmt::gib::AttachmentKind::DisplayBox {
            if let Some(field) = attachment.field_mut("Display") {
                field.raw = ra2ui::cells::markup_with_text(&field.raw, text);
            }
            if let Some(body) = attachment.body.as_mut() {
                set_prose(body, text);
            }
        }
    }
    for child in widget.children.iter_mut() {
        set_prose(child, text);
    }
}

/// Write a paragraph into the `Glyph` whose `DisplayBox` body is called `name`
/// (the box is the named record: its frame is the paragraph's rectangle).
fn set_prose_named(widget: &mut Widget, name: &str, text: &str) -> bool {
    if widget
        .attachments
        .iter()
        .any(|a| a.body.as_ref().is_some_and(|b| b.name() == name))
    {
        set_prose(widget, text);
        return true;
    }
    for child in widget.children.iter_mut() {
        if set_prose_named(child, name, text) {
            return true;
        }
    }
    false
}

/// The part's meshes with every vertex tinted red (`FUN_0042c780`'s `1 0 0
/// 0.3` overlay while the part cannot be dropped).
fn tinted(items: &[DrawItem]) -> Vec<DrawItem> {
    items
        .iter()
        .map(|item| {
            let vertices = item
                .vertices
                .iter()
                .map(|v| ra2_render::Vertex {
                    color: [1.0, 0.25, 0.25, 1.0],
                    ..*v
                })
                .collect();
            let mut out = DrawItem::new(vertices, item.material.clone(), item.label.clone());
            out.material.has_vertex_colors = true;
            out.scene_lights = item.scene_lights.clone();
            out
        })
        .collect()
}

/// A part being dragged (`BotLabComponents` `+0xd4`, with `+0xd8`/`+0xd9`:
/// whether it may be dropped and whether it is over anything).
struct ActiveDrag {
    drag: Drag,
    /// The part's meshes, and the same tinted red for a refused spot
    /// (`FUN_0042c780` paints the part red while it cannot be dropped).
    items: Vec<DrawItem>,
    red: Vec<DrawItem>,
    /// Where the mouse last was in the preview, so SHIFT/CTRL re-judge the
    /// part in place.
    at: [f32; 2],
    hover: Option<Hover>,
    candidate: Option<Candidate>,
    refusal: Option<Refusal>,
    /// Where the part floats when nothing is under the mouse: 3 m along the
    /// ray (`FUN_0042c850`'s `0x40400000`).
    floating: Mat4,
}

/// The component preview's last frame and its camera, to unproject a click
/// onto a green box.
struct CompPreview {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    rect: Frame,
    camera: PreviewCamera,
    key: String,
}

/// The synthetic panel in the 3D placeholder.
struct Panel {
    gib: Gib,
    preview: Frame,
}

impl Builder {
    /// The workshop sounds the clicks since the last call made.
    pub fn take_sounds(&mut self) -> Vec<crate::audio::UiSound> {
        std::mem::take(&mut self.sounds)
    }

    /// Boot the shell and reach the bot lab, then open `start` (or build from
    /// the stock chassis).
    pub fn open(game: &Path, start: Option<&Path>) -> Result<Builder, String> {
        Self::open_sized(game, start, SHELL_SIZE)
    }

    /// Lay the lab out natively at `size` (the window's), as the shell's
    /// other screens are (docs/28 §1). The panel and the 3D view follow the
    /// new layout.
    pub fn set_size(&mut self, size: [f32; 2]) -> Result<(), String> {
        if size == self.size || size[0] < 1.0 || size[1] < 1.0 {
            return Ok(());
        }
        self.size = size;
        self.shell.set_size(size);
        self.refresh()
    }

    /// The screen the lab is laid out on; clicks are in this space.
    pub fn size(&self) -> [f32; 2] {
        self.size
    }

    /// [`Builder::open`] with the shell laid out natively at `size`.
    pub fn open_sized(
        game: &Path,
        start: Option<&Path>,
        size: [f32; 2],
    ) -> Result<Builder, String> {
        let game = game
            .canonicalize()
            .map_err(|e| format!("{}: {e}", game.display()))?;
        let library = GameLibrary::open(&game).map_err(|e| e.to_string())?;
        let mut shell = Shell::boot(&game, size)?;
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
            size,
            view_only: false,
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
            own_mount: 0,
            style_file: None,
            drag: None,
            cursor: None,
            shift: false,
            ctrl: false,
            comp_preview: None,
            comp_key: None,
            selected_part: None,
            wire_placing: None,
            wire_control: None,
            wire_component: None,
            palette_scroll: 0,
            selected_texture: None,
            paint_mode: PaintMode::Fill,
            selected_decal: None,
            selected_group: None,
            decal_at: [0.5, 0.5],
            decal_scale: 0.5,
            draw_style: DrawStyle::Freeform,
            draw_centre: None,
            draw_point: None,
            draw_snap: true,
            shape_radius: 300,
            shape_length: 600,
            shape_width: 400,
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
            preview_camera: None,
            test_requested: false,
            sounds: Vec::new(),
            test_seconds: 4.0,
            last_report: None,
            team_target: None,
            team_new: None,
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
        self.cursor = Some([x, y]);
        if self.drag.is_some() && !self.panel.preview.contains_point(x, y) {
            // Any other click ends the drag, as the category and tab buttons
            // do (`FUN_0042c280`).
            self.cancel_drag();
        }
        if self.panel.preview.contains_point(x, y) {
            if let Some(result) = self.preview_click(x, y) {
                return result;
            }
        }
        if let Some(action) = self.panel_action(x, y) {
            return self.apply(&action);
        }
        if let Some(action) = self.drawing_action(x, y) {
            return self.apply(&action);
        }
        if let Some(action) = self.widget_action(x, y) {
            return self.apply(&action);
        }
        if self.tab() == Some("components button") && self.comp_click(x, y) {
            return Ok(());
        }
        let category = self.category_click(x, y);
        self.shell.click(x, y)?;
        if category {
            self.palette_scroll = 0;
        }
        self.take_style_pick();
        self.refresh()
    }

    /// The screen point for an outline coordinate on the shipped `Editing Area`
    /// (chassis steps 1 and 2), for a scripted click.
    pub fn outline_point_for(&self, point: [i32; 2]) -> Result<[f32; 2], String> {
        let placed = self.shell.placements();
        let area = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "Editing Area")
            .ok_or_else(|| "the current screen has no Editing Area".to_string())?;
        Ok([
            area.frame.x + point[0] as f32 / 1000.0 * area.frame.w,
            area.frame.y + point[1] as f32 / 1000.0 * area.frame.h,
        ])
    }
    fn preview_click(&mut self, x: f32, y: f32) -> Option<Result<(), String>> {
        let tab = self.shell.tab()?.to_string();
        self.ensure_preview();
        match tab.as_str() {
            "components button" => Some(self.place_at_preview(x, y)),
            "paintshop button" => Some(self.paint_at_preview(x, y)),
            _ => None,
        }
    }

    fn ensure_preview(&mut self) {
        let rect = self.panel.preview;
        if rect.w > 0.0 && rect.h > 0.0 {
            let _ = self.preview_pixels(rect);
        }
    }

    /// The screen point that shows a baseplate point, for a scripted or
    /// scripted-equivalent click. This is the inverse of [`Builder::baseplate_at`]
    /// and uses the camera the last frame was drawn with.
    pub fn preview_point_at(&mut self, point: [f32; 2]) -> Result<[f32; 2], String> {
        self.ensure_preview();
        let camera = self
            .preview_camera
            .ok_or_else(|| "the preview has not been rendered".to_string())?;
        let rect = self.panel.preview;
        if rect.w <= 0.0 || rect.h <= 0.0 {
            return Err("the current screen has no 3D preview".into());
        }
        let world = self
            .build
            .transforms()
            .first()
            .copied()
            .unwrap_or(Mat4::IDENTITY);
        let p = world.transform_point3(Vec3::new(point[0], 0.0, point[1]));
        let clip = camera.view_proj * p.extend(1.0);
        if clip.w.abs() < 1e-6 {
            return Err("the point is behind the preview camera".into());
        }
        let ndc = clip.truncate() / clip.w;
        Ok([
            rect.x + (ndc.x * 0.5 + 0.5) * rect.w,
            rect.y + (0.5 - ndc.y * 0.5) * rect.h,
        ])
    }

    /// A click on the preview. While a part is being dragged it is the drop
    /// (`FUN_0042d4b0`); otherwise it selects the placed part under the mouse
    /// (`FUN_0042d280`), or nothing.
    fn place_at_preview(&mut self, x: f32, y: f32) -> Result<(), String> {
        if self.drag.is_some() {
            return self.drop_click();
        }
        let Some((origin, direction)) = self.bot_ray(x, y) else {
            return Ok(());
        };
        self.selected_part = self.build.pick_part(origin, direction);
        self.status = match self.selected_part {
            Some(i) => format!(
                "selected component {i}: {}",
                self.build
                    .component_info(i)
                    .map(|info| info.name.clone())
                    .unwrap_or_default()
            ),
            None => String::new(),
        };
        self.refresh_panel();
        Ok(())
    }

    // -- The placement drag (docs/36 §9.18) ----------------------------------

    /// The selected part and the definition the Attach button would place: the
    /// style picked in the `Component option` list, else the palette's own.
    fn selected_info(&self) -> Option<(ComponentInfo, String)> {
        let component = self.selected.as_ref()?;
        let file = self
            .style_file
            .clone()
            .unwrap_or_else(|| component.file.clone());
        let info = self.library.component_info_by_file(&file);
        info.loaded.then(|| {
            let name = if info.name.is_empty() {
                component.name.clone()
            } else {
                info.name.clone()
            };
            (info, name)
        })
    }

    /// The part's styles as the `Component option` list shows them: the
    /// definition's `styles = Label, file.txt, Label, file.txt, ...` line (the
    /// first label is the palette's own definition, then a file and its label
    /// per style; a lone `Default` is no choice). `FUN_0042e650` reads them
    /// through `FUN_0042f050`/`FUN_00462ec0` and loads `Components\Styles\<file>`.
    fn style_list(&self) -> Vec<(String, Option<String>)> {
        let Some(component) = &self.selected else {
            return Vec::new();
        };
        let base = self.library.component_info_by_file(&component.file);
        let Some(styles) = base.def.as_ref().and_then(|d| d.get("styles")) else {
            return Vec::new();
        };
        let tokens: Vec<&str> = styles.split(',').map(str::trim).collect();
        if tokens.len() < 3 {
            return Vec::new();
        }
        let mut out = vec![(tokens[0].to_string(), None)];
        for pair in tokens[1..].chunks(2) {
            if let [file, label] = pair {
                out.push((
                    label.to_string(),
                    Some(format!("Components\\Styles\\{file}")),
                ));
            }
        }
        out
    }

    /// A palette click: it only selects the part and shows its preview
    /// (`FUN_0042e650`); nothing is attached until the Attach button.
    fn select_component(&mut self, component: PaletteComponent) {
        self.cancel_drag();
        self.style_file = None;
        self.selected = Some(component);
        self.comp_preview = None;
        if let Some((info, _)) = self.selected_info() {
            self.own_mount = placement::default_own_mount(&info);
            self.status = placement::preview_tip(&info).into();
        }
    }

    /// The part's meshes, placed at its own origin.
    fn part_items(&self, info: &ComponentInfo) -> Vec<DrawItem> {
        let Some(def) = info.def.as_ref() else {
            return Vec::new();
        };
        let Some(scene) = self.library.load_component_mesh(def) else {
            return Vec::new();
        };
        let dir = self
            .library
            .component_mesh_path(def)
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| self.game.join("Components"));
        ra2_render::scene_items(&scene, Mat4::IDENTITY, &dir, &info.name)
    }

    /// The Attach button (`FUN_0042c3a0`): the mass check first, then the drag.
    fn attach_pressed(&mut self) -> Result<(), String> {
        self.cancel_drag();
        let Some((info, name)) = self.selected_info() else {
            self.status = "Pick a component in the palette first.".into();
            self.refresh_panel();
            return Ok(());
        };
        if let Err(e) = self.build.check_attach_mass(&info) {
            self.status = match e {
                BuildError::OverWeight { limit, .. } => placement::mass_message(limit),
                other => other.to_string(),
            };
            self.refresh_panel();
            return Ok(());
        }
        let items = self.part_items(&info);
        let red = tinted(&items);
        self.drag = Some(ActiveDrag {
            drag: Drag::new(info, name, self.own_mount),
            items,
            red,
            at: self.cursor.unwrap_or([0.0, 0.0]),
            hover: None,
            candidate: None,
            refusal: None,
            floating: Mat4::IDENTITY,
        });
        self.status = placement::TIP_DRAG.into();
        // `FUN_0042c6b0`: the part is placed where the mouse already is.
        if let Some([x, y]) = self.cursor {
            self.drag_to(x, y);
        }
        self.preview = None;
        self.refresh_panel();
        Ok(())
    }

    /// End the drag without adding anything (`FUN_0042c280`).
    fn cancel_drag(&mut self) {
        if self.drag.take().is_some() {
            self.preview = None;
            if self.status == placement::TIP_DRAG
                || self.status == Refusal::Intersects.message()
                || self.status == Refusal::TooLarge.message()
                || self.status == Refusal::OutsideChassis.message()
            {
                self.status.clear();
            }
        }
    }

    /// The mouse ray in the robot's own space, when the preview has been drawn.
    fn bot_ray(&mut self, x: f32, y: f32) -> Option<(Vec3, Vec3)> {
        self.ensure_preview();
        let camera = self.preview_camera?;
        let (origin, direction) = camera.ray(self.panel.preview, x, y)?;
        let world = self
            .build
            .transforms()
            .first()
            .copied()
            .unwrap_or(Mat4::IDENTITY);
        let inverse = world.inverse();
        Some((
            inverse.transform_point3(origin),
            inverse.transform_vector3(direction),
        ))
    }

    /// Re-judge the dragged part at the mouse (`FUN_0042c850`).
    fn drag_to(&mut self, x: f32, y: f32) {
        let Some(mut active) = self.drag.take() else {
            return;
        };
        active.at = [x, y];
        let ray = if self.panel.preview.contains_point(x, y) {
            self.bot_ray(x, y)
        } else {
            None
        };
        active.hover = ray.and_then(|(o, d)| self.build.pick(&active.drag, o, d));
        active.candidate = active
            .hover
            .as_ref()
            .and_then(|h| self.build.plan(&active.drag, h));
        active.refusal = active
            .candidate
            .as_ref()
            .and_then(|c| self.build.judge(&active.drag, c));
        if let Some((o, d)) = ray {
            active.floating = Mat4::from_translation(o + d.normalize_or_zero() * 3.0)
                * Mat4::from_rotation_y(active.drag.angle());
        }
        let hovering = active.candidate.is_some();
        self.status = match (&active.refusal, hovering) {
            (Some(r), _) => r.message().to_string(),
            _ => placement::TIP_DRAG.to_string(),
        };
        self.drag = Some(active);
        self.preview = None;
        self.refresh_panel();
    }

    /// The mouse moved. With a part on the drag the part follows it; SHIFT turns
    /// it and CTRL raises it by the dominant axis of the movement
    /// (`0x42b6b4`: the larger of |dx| and |dy|, signed).
    pub fn mouse_moved(&mut self, x: f32, y: f32) {
        let last = self.cursor.replace([x, y]);
        let Some(active) = self.drag.as_mut() else {
            return;
        };
        let (dx, dy) = last.map(|l| (x - l[0], y - l[1])).unwrap_or((0.0, 0.0));
        let delta = if dx.abs() > dy.abs() { dx } else { dy };
        if self.shift {
            active.drag.rotate(delta);
            let [ax, ay] = active.at;
            self.drag_to(ax, ay);
        } else if self.ctrl {
            let ceiling = self.build.chassis_height();
            active.drag.raise(delta, ceiling);
            let [ax, ay] = active.at;
            self.drag_to(ax, ay);
        } else {
            self.drag_to(x, y);
        }
    }

    /// SHIFT and CTRL as the window reports them.
    pub fn set_modifiers(&mut self, shift: bool, ctrl: bool) {
        self.shift = shift;
        self.ctrl = ctrl;
    }

    /// Whether a part is on the drag.
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The click while dragging (`FUN_0042d4b0`): drop the part if it is over
    /// something and not refused; a refused part stays in the drag; over
    /// nothing, the drag ends.
    fn drop_click(&mut self) -> Result<(), String> {
        let Some(active) = self.drag.as_ref() else {
            return Ok(());
        };
        let Some(hover) = active.hover else {
            self.cancel_drag();
            self.refresh_panel();
            return Ok(());
        };
        if active.candidate.is_none() || active.refusal.is_some() {
            return Ok(());
        }
        let drag = active.drag.clone();
        match self.build.drop_part(&drag, &hover) {
            Ok(index) => {
                let attach = drag
                    .info
                    .def
                    .as_ref()
                    .and_then(|d| d.get("attachsound"))
                    .map(|p| p.trim().to_string());
                self.sounds
                    .push(crate::audio::UiSound::ComponentAttached(attach));
                self.drag = None;
                self.selected_part = None;
                self.status = format!("attached {} (component {index})", drag.name);
                self.refresh()
            }
            Err(e) => {
                self.status = e.to_string();
                self.refresh_panel();
                Ok(())
            }
        }
    }

    /// A click in the component preview picks the green box nearest to it.
    fn comp_click(&mut self, x: f32, y: f32) -> bool {
        self.ensure_comp_preview();
        let Some(cp) = &self.comp_preview else {
            return false;
        };
        if !cp.rect.contains_point(x, y) {
            return false;
        }
        let Some((info, _)) = self.selected_info() else {
            return false;
        };
        let mut best: Option<(f32, i32)> = None;
        for id in placement::selectable_mounts(&info) {
            let Some(point) = info.mounts.get(&id) else {
                continue;
            };
            let at = ra2_render::tm_mat4(&point.transform).transform_point3(Vec3::ZERO);
            let clip = cp.camera.view_proj * at.extend(1.0);
            if clip.w <= 0.0 {
                continue;
            }
            let ndc = clip.truncate() / clip.w;
            let sx = cp.rect.x + (ndc.x * 0.5 + 0.5) * cp.rect.w;
            let sy = cp.rect.y + (0.5 - ndc.y * 0.5) * cp.rect.h;
            let d = ((sx - x).powi(2) + (sy - y).powi(2)).sqrt();
            if d < 30.0 && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, id));
            }
        }
        if let Some((_, id)) = best {
            self.own_mount = id;
            self.comp_preview = None;
            self.refresh_panel();
        }
        true
    }

    /// The screen point of a green box in the component preview, for a scripted
    /// click.
    pub fn box_point(&mut self, mount: i32) -> Result<[f32; 2], String> {
        self.ensure_comp_preview();
        let cp = self
            .comp_preview
            .as_ref()
            .ok_or_else(|| "the component preview is not on screen".to_string())?;
        let (info, _) = self
            .selected_info()
            .ok_or_else(|| "no component is selected".to_string())?;
        let point = info
            .mounts
            .get(&mount)
            .ok_or_else(|| format!("the part has no point {mount}"))?;
        let at = ra2_render::tm_mat4(&point.transform).transform_point3(Vec3::ZERO);
        let clip = cp.camera.view_proj * at.extend(1.0);
        let ndc = clip.truncate() / clip.w;
        Ok([
            cp.rect.x + (ndc.x * 0.5 + 0.5) * cp.rect.w,
            cp.rect.y + (0.5 - ndc.y * 0.5) * cp.rect.h,
        ])
    }

    /// The component preview's rectangle: the shipped `component 3d preview`
    /// placeholder of the `comp_preview` group, when it is loaded.
    fn comp_rect(&self) -> Option<Frame> {
        self.shell
            .placements()
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "component 3d preview")
            .map(|p| p.frame)
    }

    fn ensure_comp_preview(&mut self) {
        if let Some(rect) = self.comp_rect() {
            if rect.w > 0.0 && rect.h > 0.0 {
                let _ = self.comp_preview_pixels(rect);
            }
        }
    }

    /// Load the `comp_preview` group into the screen's `component content
    /// group` for the selected part (`FUN_0042f440` mode 1 hands it the group)
    /// and fill its name, weight and description boxes and the style list
    /// (`FUN_0042e650`).
    fn refresh_comp_preview(&mut self) {
        if self.shell.tab() != Some("components button") {
            return;
        }
        let selected = self.selected_info();
        let styles = self.style_list();
        let fragment = workshop::load(&self.ui, "botlab/comp_preview.gib").ok();
        let labels: Vec<String> = styles.iter().map(|(label, _)| label.clone()).collect();
        let current = self
            .style_file
            .as_ref()
            .and_then(|f| styles.iter().find(|(_, file)| file.as_ref() == Some(f)))
            .or_else(|| styles.first())
            .map(|(label, _)| label.clone())
            .unwrap_or_default();
        if let Some(lab) = self.shell.lab_mut() {
            for block in lab.document.blocks.iter_mut() {
                let ra2fmt::gib::Block::Widget(root) = block else {
                    continue;
                };
                let Some(slot) = workshop::find_mut(root, "component content group") else {
                    continue;
                };
                let key = selected.as_ref().map(|(info, _)| info.file.clone());
                if !slot.children.is_empty() && self.comp_key == key {
                    continue;
                }
                slot.children.clear();
                self.comp_key = key;
                let (Some((info, name)), Some(fragment)) = (&selected, &fragment) else {
                    continue;
                };
                workshop::load_into(root, fragment, "component content group");
                let description = info
                    .def
                    .as_ref()
                    .and_then(|d| d.get("description"))
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let weight = format!("{:.1}", info.mass);
                for (widget, text) in [
                    ("component name textbox", name.as_str()),
                    ("component weight textbox", weight.as_str()),
                ] {
                    if let Some(w) = workshop::find_mut(root, widget) {
                        ra2ui::input::set_field(w, "_Text", text);
                        for child in w.children.iter_mut() {
                            if child.kind == WidgetKind::Text {
                                ra2ui::input::set_field(child, "_Text", text);
                            }
                        }
                    }
                }
                set_prose_named(root, "component description", &description);
                // `FUN_0042e650` hides the style group for a part with no
                // alternatives, and titles it `Component options (N)`.
                if let Some(group) = workshop::find_mut(root, "style dropdown group") {
                    ra2ui::input::set_field(
                        group,
                        "Visible",
                        if labels.is_empty() { "false" } else { "true" },
                    );
                    if let Some(label) = workshop::find_mut(group, "model label") {
                        let text = format!("Component options ({})", labels.len());
                        ra2ui::input::set_field(label, "_Text", &text);
                        for child in label.children.iter_mut() {
                            ra2ui::input::set_field(child, "_Text", &text);
                        }
                    }
                    if let Some(drop) = workshop::find_mut(group, "component option") {
                        ra2ui::input::set_field(drop, "_Text", &current);
                        for child in drop.children.iter_mut() {
                            if child.kind == WidgetKind::TextBox {
                                for text in child.children.iter_mut() {
                                    ra2ui::input::set_field(text, "_Text", &current);
                                }
                            }
                        }
                    }
                }
            }
        }
        self.shell.set_dropdown_items("component option", labels);
    }

    /// A style row was picked in the `Component option` list: the preview and
    /// the part Attach places are that style's definition.
    fn take_style_pick(&mut self) {
        let picks = self.shell.take_dropdown_picks();
        for (name, text) in picks {
            if name != "component option" {
                continue;
            }
            let styles = self.style_list();
            if let Some((_, file)) = styles
                .iter()
                .find(|(label, _)| label.eq_ignore_ascii_case(text.trim()))
            {
                self.cancel_drag();
                self.style_file = file.clone();
                self.comp_preview = None;
                if let Some((info, _)) = self.selected_info() {
                    self.own_mount = placement::default_own_mount(&info);
                }
            }
        }
    }

    /// The part and its green boxes, rendered into `rect`: `AP_box_selected`
    /// on the chosen point, `AP_box_unselected` on the others
    /// (`UI\BotLab\AP_box_*.gmf`, placed by the attachment's own frame).
    fn comp_preview_pixels(&mut self, rect: Frame) -> Option<()> {
        let (info, _) = self.selected_info()?;
        let width = rect.w.round().max(16.0) as u32;
        let height = rect.h.round().max(16.0) as u32;
        let key = format!("{}|{}|{width}x{height}", info.file, self.own_mount);
        if self.comp_preview.as_ref().is_some_and(|c| c.key == key) {
            return Some(());
        }
        let part = self.part_items(&info);
        let boxes = placement::selectable_mounts(&info);
        let ap_dir = ra2fmt::find_ci(&self.ui, "botlab")?;
        let load = |file: &str| -> Vec<DrawItem> {
            let Some(path) = ra2fmt::find_ci(&ap_dir, file) else {
                return Vec::new();
            };
            let Ok(bytes) = std::fs::read(&path) else {
                return Vec::new();
            };
            let Ok(scene) = ra2fmt::gmi::parse(&bytes) else {
                return Vec::new();
            };
            ra2_render::scene_items(&scene, Mat4::IDENTITY, &ap_dir, file)
        };
        let selected_box = load("AP_box_selected.gmf");
        let unselected_box = load("AP_box_unselected.gmf");
        let mut groups: Vec<RenderGroup<'_>> = vec![RenderGroup {
            items: &part,
            model: Mat4::IDENTITY,
        }];
        for id in &boxes {
            let Some(point) = info.mounts.get(id) else {
                continue;
            };
            groups.push(RenderGroup {
                items: if *id == self.own_mount {
                    &selected_box
                } else {
                    &unselected_box
                },
                model: ra2_render::tm_mat4(&point.transform),
            });
        }
        let (mn, mx) = info.bounds.unwrap_or(([-0.2; 3], [0.2; 3]));
        let (mn, mx) = (Vec3::from(mn), Vec3::from(mx));
        let center = (mn + mx) * 0.5;
        let radius = ((mx - mn).length() * 0.5).max(0.15);
        let eye = center + Vec3::new(radius * 1.9, radius * 1.4, radius * 2.2);
        let view = ra2_render::look_at_lh(eye, center, Vec3::Y);
        let proj = ra2_render::perspective_lh(
            45f32.to_radians(),
            width as f32 / height as f32,
            0.02,
            radius * 30.0,
        );
        let view_proj = proj * view;
        let pixels = self
            .renderer
            .render_groups(&groups, view_proj, width, height)
            .ok()?;
        self.comp_preview = Some(CompPreview {
            pixels,
            width,
            height,
            rect,
            camera: PreviewCamera {
                view_proj,
                inv_view_proj: view_proj.inverse(),
                width,
                height,
            },
            key,
        });
        Some(())
    }

    /// The screen point of a world point in the main preview, for a scripted
    /// drag.
    pub fn preview_point_of(&mut self, world: Vec3) -> Result<[f32; 2], String> {
        self.ensure_preview();
        let camera = self
            .preview_camera
            .ok_or_else(|| "the preview has not been rendered".to_string())?;
        let rect = self.panel.preview;
        let clip = camera.view_proj * world.extend(1.0);
        if clip.w.abs() < 1e-6 {
            return Err("the point is behind the preview camera".into());
        }
        let ndc = clip.truncate() / clip.w;
        Ok([
            rect.x + (ndc.x * 0.5 + 0.5) * rect.w,
            rect.y + (0.5 - ndc.y * 0.5) * rect.h,
        ])
    }

    /// The screen point that drops a part on `over`, for a scripted drag.
    pub fn drag_point(&mut self, over: PlaceOn) -> Result<[f32; 2], String> {
        let world = match over {
            PlaceOn::Baseplate(point) => Vec3::new(point[0], 0.0, point[1]),
            PlaceOn::Mount { parent, mount } => {
                let worlds = self.build.transforms();
                let info = self
                    .build
                    .component_info(parent)
                    .ok_or_else(|| format!("component {parent} does not exist"))?;
                let point = info
                    .mounts
                    .get(&mount)
                    .ok_or_else(|| format!("component {parent} has no point {mount}"))?;
                (worlds[parent] * ra2_render::tm_mat4(&point.transform))
                    .transform_point3(Vec3::ZERO)
            }
        };
        self.preview_point_of(world)
    }

    /// The harness's whole gesture: select `file` in the open category, press
    /// Attach, move the mouse to `over` and click. The mouse moves go through
    /// [`Builder::mouse_moved`] and the clicks through [`Builder::click`], as
    /// the window's events do. Returns the new component's index.
    pub fn place_component(&mut self, file: &str, over: PlaceOn) -> Result<usize, String> {
        let before = self.build.components().len();
        self.click_palette_file(file)?;
        let attach = self.attach_point()?;
        self.mouse_moved(attach[0], attach[1]);
        self.click(attach[0], attach[1])?;
        if self.drag.is_none() {
            return Err(format!("the drag did not start ({})", self.status));
        }
        let at = self.drag_point(over)?;
        self.mouse_moved(at[0], at[1]);
        self.click(at[0], at[1])?;
        if self.build.components().len() != before + 1 {
            return Err(format!("the drop did not attach ({})", self.status));
        }
        Ok(before)
    }

    /// [`Builder::place_component`] at the first baseplate spot (a 10 cm grid)
    /// the drag accepts, for a robot whose free room is not known.
    pub fn place_component_free(&mut self, file: &str) -> Result<usize, String> {
        let before = self.build.components().len();
        self.click_palette_file(file)?;
        let attach = self.attach_point()?;
        self.mouse_moved(attach[0], attach[1]);
        self.click(attach[0], attach[1])?;
        for gx in -9..=9 {
            for gz in -9..=9 {
                let at = self.drag_point(PlaceOn::Baseplate([gx as f32 * 0.1, gz as f32 * 0.1]))?;
                self.mouse_moved(at[0], at[1]);
                let free = self
                    .drag
                    .as_ref()
                    .is_some_and(|a| a.candidate.is_some() && a.refusal.is_none());
                if free {
                    self.click(at[0], at[1])?;
                    return if self.build.components().len() == before + 1 {
                        Ok(before)
                    } else {
                        Err(format!("the drop did not attach ({})", self.status))
                    };
                }
            }
        }
        self.cancel_drag();
        Err("no free spot on the baseplate".into())
    }

    /// The screen point of the Attach button: the shipped `attach button` of
    /// the `comp_preview` group when it is loaded, else the panel's row.
    pub fn attach_point(&self) -> Result<[f32; 2], String> {
        let placed = self.shell.placements();
        if let Some(button) = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "attach button")
        {
            return Ok([
                button.frame.x + button.frame.w * 0.5,
                button.frame.y + button.frame.h * 0.5,
            ]);
        }
        Err("the component preview (and its Attach button) is not on screen".into())
    }

    /// Paint the face group under a preview click. Fill and layer use the
    /// selected texture; decal uses the selected decal and the clicked point
    /// inside the group (the screen's *"position the decal with your mouse and
    /// click to apply it"*).
    fn paint_at_preview(&mut self, x: f32, y: f32) -> Result<(), String> {
        let Some((group, at)) = self.pick_chassis(x, y) else {
            self.status = "the click did not land on the chassis".into();
            self.refresh_panel();
            return Ok(());
        };
        self.selected_group = Some(group);
        self.decal_at = at;
        match self.paint_mode {
            PaintMode::Fill | PaintMode::Layer => {
                let Some(entry) = self.selected_texture.clone() else {
                    self.status = "pick a texture first".into();
                    self.refresh_panel();
                    return Ok(());
                };
                let texture = Texture::load(&entry.path).map_err(|e| e.to_string())?;
                let mode = self.paint_mode;
                let paint = self.build.paint_mut();
                let result = if mode == PaintMode::Fill {
                    paint.fill_group(group, &texture)
                } else {
                    paint.layer_group(group, &texture)
                };
                result.map_err(|e| e.to_string())?;
                self.status = format!("{} face group {group} with {}", mode.label(), entry.name);
            }
            PaintMode::Decal => {
                let Some(entry) = self.selected_decal.clone() else {
                    self.status = "pick a decal first".into();
                    self.refresh_panel();
                    return Ok(());
                };
                let texture = Texture::load(&entry.path).map_err(|e| e.to_string())?;
                self.build
                    .paint_mut()
                    .decal_group(group, &texture, at, self.decal_scale)
                    .map_err(|e| e.to_string())?;
                self.status = format!(
                    "decal {} on face group {group} at {:.2}, {:.2} scale {:.2}",
                    entry.name, at[0], at[1], self.decal_scale
                );
            }
        }
        self.refresh()
    }

    /// Ray-cast the preview against the chassis mesh and return the face group
    /// and the skin position inside the group's rectangle.
    fn pick_chassis(&mut self, x: f32, y: f32) -> Option<(usize, [f32; 2])> {
        self.ensure_preview();
        let camera = self.preview_camera?;
        let (origin, direction) = camera.ray(self.panel.preview, x, y)?;
        let bot = self.build.to_bot();
        let chassis = bot.components.first()?.chassis.as_ref()?;
        let mesh = chassis.mesh.as_ref()?;
        let world = self
            .build
            .transforms()
            .first()
            .copied()
            .unwrap_or(Mat4::IDENTITY);
        let points: Vec<Vec3> = mesh
            .points
            .iter()
            .map(|p| world.transform_point3(Vec3::from(*p)))
            .collect();
        let mut best: Option<(f32, [u32; 3], f32, f32)> = None;
        for tri in mesh.indices.as_chunks::<3>().0 {
            let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            if a >= points.len() || b >= points.len() || c >= points.len() {
                continue;
            }
            if let Some((t, u, v)) =
                ray_triangle(origin, direction, points[a], points[b], points[c])
            {
                if best.is_none_or(|(best_t, _, _, _)| t < best_t) {
                    best = Some((t, *tri, u, v));
                }
            }
        }
        let (_, tri, u, v) = best?;
        for group in 0..chassis.face_groups.len() {
            let Some(uvs) = chassis::group_uvs(chassis, group, tri) else {
                continue;
            };
            let uv = [
                uvs[0][0] * (1.0 - u - v) + uvs[1][0] * u + uvs[2][0] * v,
                uvs[0][1] * (1.0 - u - v) + uvs[1][1] * u + uvs[2][1] * v,
            ];
            let rect = self.build.paint().group_rect(group)?;
            let (x1, y1, x2, y2) = rect;
            if x2 <= x1 || y2 <= y1 {
                return Some((group, [0.5, 0.5]));
            }
            let at = [
                ((uv[0] - x1 as f32) / (x2 - x1) as f32).clamp(0.0, 1.0),
                ((uv[1] - y1 as f32) / (y2 - y1) as f32).clamp(0.0, 1.0),
            ];
            return Some((group, at));
        }
        None
    }

    /// A click on a chassis step's shipped `Editing Area`, mapped into the
    /// outline's `0..1000` blueprint. This is the screen's own drawing surface.
    fn drawing_action(&self, x: f32, y: f32) -> Option<String> {
        if self.shell.tab() != Some("chassis button") {
            return None;
        }
        let placed = self.shell.placements();
        let area = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "Editing Area")?;
        if !area.frame.contains_point(x, y) {
            return None;
        }
        let u = ((x - area.frame.x) / area.frame.w * 1000.0).clamp(0.0, 1000.0) as i32;
        let v = ((y - area.frame.y) / area.frame.h * 1000.0).clamp(0.0, 1000.0) as i32;
        Some(format!("m9:draw:{u}:{v}"))
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
        let placed = ra2ui::resolve_with(&self.panel.gib, self.size, Some(self.assets.faces()));
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
        let (horizontal, visible, _) = self.palette_window();
        let scroll = if horizontal {
            // One long row, scrolled by cell: bring the cell into the window.
            if index < self.palette_scroll {
                index
            } else if index >= self.palette_scroll + visible {
                index + 1 - visible
            } else {
                self.palette_scroll
            }
        } else {
            index / self.palette_columns().max(1)
        };
        if scroll != self.palette_scroll {
            self.palette_scroll = scroll;
            self.refresh_palette();
        }
        self.click_palette(index)
    }

    /// Whether the palette is one horizontally scrolled row (`components.gib`'s
    /// grid is `Rows 1` under a horizontal `Scroller`, `PermScrolls true`), and
    /// how many whole cells the grid's frame shows.
    fn palette_window(&self) -> (bool, usize, f32) {
        let placed = self.shell.placements();
        let Some(grid) = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "components grid")
        else {
            return (false, 4, 78.0);
        };
        let Some(g) = ra2ui::Grid::from_widget(grid.widget) else {
            return (false, 4, 78.0);
        };
        let horizontal = g.rows == 1
            && grid
                .widget
                .attachments
                .iter()
                .any(|a| a.kind == ra2fmt::gib::AttachmentKind::Scroller);
        // The cells are the art's own width apart.
        let stride = grid
            .widget
            .children
            .iter()
            .find(|c| c.filename().is_some_and(|f| f.starts_with("Components")))
            .and_then(|c| c.size())
            .map(|s| s.first.amount)
            .filter(|w| *w > 0.0)
            .unwrap_or_else(|| g.column_x(1).max(1.0));
        (
            horizontal,
            ((grid.frame.w / stride).ceil() as usize).max(1),
            stride,
        )
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

    /// Edit a team robot in place: [`Builder::save_to_team`] writes the design
    /// to `path` (the inventory slot the workshop was opened on).
    pub fn set_team_target(&mut self, path: PathBuf) {
        self.team_target = Some(path);
    }

    /// The team robot file this session writes back to, if any.
    pub fn team_target(&self) -> Option<&Path> {
        self.team_target.as_deref()
    }

    /// Write the design back to its team slot (`Back to HQ` from a robot the
    /// inventory opened). `None` when the session edits no team robot.
    /// A new robot (no inventory robot was selected) joins team `team`'s
    /// roster when the lab is left, if anything was built or named.
    pub fn set_team_new(&mut self, team: usize) {
        self.team_new = Some(team);
    }

    pub fn save_to_team(&mut self) -> Result<Option<PathBuf>, String> {
        if self.team_target.is_none() {
            if let Some(team) = self.team_new {
                if self.build.can_undo() || self.name_edited {
                    let path = ra2ui::teams::add_robot(&self.game, team)?;
                    self.team_target = Some(path);
                }
            }
        }
        let Some(path) = self.team_target.clone() else {
            return Ok(None);
        };
        let name = self.design_name();
        self.build.set_name(&name);
        self.build
            .save_to(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        self.status = format!("saved {}", path.display());
        Ok(Some(path))
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
        self.cancel_drag();
        self.style_file = None;
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
            self.apply_paint_group(group)?;
            return Ok(());
        }
        if let Some(rest) = action.strip_prefix("m9:pmode:") {
            self.paint_mode = match rest {
                "fill" => PaintMode::Fill,
                "layer" => PaintMode::Layer,
                "decal" => PaintMode::Decal,
                other => return Err(format!("unknown paint mode {other:?}")),
            };
            self.status = format!("paint mode: {}", self.paint_mode.label());
            self.refresh_panel();
            return Ok(());
        }
        if let Some(rest) = action.strip_prefix("m9:dtex:") {
            let index: usize = rest
                .parse()
                .map_err(|_| format!("bad decal row {rest:?}"))?;
            let entry = self
                .library
                .decals()
                .entries
                .get(index)
                .cloned()
                .ok_or_else(|| format!("decal {index} is not listed"))?;
            self.selected_decal = Some(entry.clone());
            self.status = format!("decal {}", entry.name);
            self.refresh_panel();
            return Ok(());
        }
        if action == "m9:dscale" {
            const SCALES: [f32; 6] = [0.25, 0.5, 0.75, 1.0, 1.5, 2.0];
            let at = SCALES
                .iter()
                .position(|s| (*s - self.decal_scale).abs() < 1e-4)
                .map(|i| (i + 1) % SCALES.len())
                .unwrap_or(0);
            self.decal_scale = SCALES[at];
            self.status = format!("decal scale {:.2}", self.decal_scale);
            self.refresh_panel();
            return Ok(());
        }
        if let Some(rest) = action.strip_prefix("m9:draw:") {
            let (u, v) = rest
                .split_once(':')
                .ok_or_else(|| format!("bad drawing point {rest:?}"))?;
            let point = [
                u.parse().map_err(|_| format!("bad outline x {u:?}"))?,
                v.parse().map_err(|_| format!("bad outline y {v:?}"))?,
            ];
            self.draw_chassis(point)?;
            return Ok(());
        }
        if let Some(rest) = action.strip_prefix("m9:style:") {
            self.draw_style = match rest {
                "freeform" => DrawStyle::Freeform,
                "circle" => DrawStyle::Circle,
                "rectangle" => DrawStyle::Rectangle,
                other => return Err(format!("unknown drawing style {other:?}")),
            };
            self.status = format!("drawing: {}", self.draw_style.label());
            self.refresh_panel();
            return Ok(());
        }
        if let Some(rest) = action.strip_prefix("m9:slider:") {
            let (which, fraction) = rest
                .split_once(':')
                .ok_or_else(|| format!("bad slider row {rest:?}"))?;
            let fraction: f32 = fraction
                .parse()
                .map_err(|_| format!("bad slider fraction {fraction:?}"))?;
            return self.set_slider(which, fraction);
        }
        if action == "m9:outline:clear" {
            let step = self.structure_step();
            let which = if step == 2 {
                Outline::Top
            } else {
                Outline::Bottom
            };
            self.build.clear_chassis_outline(which);
            self.status = format!("cleared the {} outline", which.label());
            return self.refresh();
        }
        if action == "m9:finished" {
            let chassis = self.build.chassis().cloned().unwrap_or_default();
            let step = self.structure_step();
            let (points, outline) = if step == 2 {
                (chassis.top.clone(), Outline::Top)
            } else {
                (chassis.bottom.clone(), Outline::Bottom)
            };
            self.status = match chassis::validate_outline(&points) {
                Ok(()) => format!(
                    "{} outline finished: {} point(s), height {:.2} m (the mesh is not rebuilt)",
                    outline.label(),
                    points.len(),
                    chassis.height
                ),
                Err(e) => format!("{} outline: {e}", outline.label()),
            };
            self.refresh_panel();
            return Ok(());
        }
        if let Some(rest) = action.strip_prefix("m9:snap:") {
            self.draw_snap = rest == "on";
            self.status = format!("snap to grid {}", if self.draw_snap { "on" } else { "off" });
            self.refresh_panel();
            return Ok(());
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
            self.select_component(component);
            return self.refresh();
        }
        if action == "m9:attach" {
            return self.attach_pressed();
        }
        if let Some(rest) = action.strip_prefix("m9:box:") {
            let id: i32 = rest.parse().map_err(|_| format!("bad box {rest:?}"))?;
            self.own_mount = id;
            self.refresh_panel();
            return Ok(());
        }
        if action == "m9:remove" {
            // `FUN_0042f270`: only the selected placed part, and only a leaf.
            let Some(index) = self.selected_part else {
                self.status = "Select a placed component to remove.".into();
                self.refresh_panel();
                return Ok(());
            };
            match self.build.remove_leaf(index) {
                Ok(()) => {
                    self.sounds.push(crate::audio::UiSound::ComponentRemoved);
                    self.selected_part = None;
                    self.status = format!("removed component {index}");
                    return self.refresh();
                }
                Err(e) => {
                    self.status = e.to_string();
                    self.refresh_panel();
                    return Ok(());
                }
            }
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

    /// Apply the current paint mode to a face group (a panel row or a preview
    /// click).
    fn apply_paint_group(&mut self, group: usize) -> Result<(), String> {
        self.selected_group = Some(group);
        match self.paint_mode {
            PaintMode::Fill | PaintMode::Layer => {
                let Some(entry) = self.selected_texture.clone() else {
                    self.status = "pick a texture first".into();
                    self.refresh_panel();
                    return Ok(());
                };
                let texture = Texture::load(&entry.path).map_err(|e| e.to_string())?;
                let mode = self.paint_mode;
                let paint = self.build.paint_mut();
                let result = if mode == PaintMode::Fill {
                    paint.fill_group(group, &texture)
                } else {
                    paint.layer_group(group, &texture)
                };
                result.map_err(|e| e.to_string())?;
                self.status = format!("{} face group {group} with {}", mode.label(), entry.name);
            }
            PaintMode::Decal => {
                let Some(entry) = self.selected_decal.clone() else {
                    self.status = "pick a decal first".into();
                    self.refresh_panel();
                    return Ok(());
                };
                let texture = Texture::load(&entry.path).map_err(|e| e.to_string())?;
                self.build
                    .paint_mut()
                    .decal_group(group, &texture, self.decal_at, self.decal_scale)
                    .map_err(|e| e.to_string())?;
                self.status = format!(
                    "decal {} on face group {group} scale {:.2}",
                    entry.name, self.decal_scale
                );
            }
        }
        self.refresh()
    }

    /// The chassis structure step on screen: 1 for the baseplate, 2 for the
    /// top panel. The screens are told apart by their own widgets.
    fn structure_step(&self) -> u8 {
        let Some(lab) = self.shell.lab() else {
            return 1;
        };
        let Some(root) = lab.content_root() else {
            return 1;
        };
        if workshop::find(root, "height slider").is_some() {
            2
        } else {
            1
        }
    }

    /// A click on a chassis step's `Editing Area`.
    fn draw_chassis(&mut self, point: [i32; 2]) -> Result<(), String> {
        let step = self.structure_step();
        if step == 2 {
            let Some(chassis) = self.build.chassis() else {
                return Ok(());
            };
            if let Some(index) = self.draw_point {
                match self.build.move_chassis_outline_point(
                    Outline::Top,
                    index,
                    point,
                    self.draw_snap,
                ) {
                    Ok(()) => {
                        self.status =
                            format!("moved top point {index} to {}, {}", point[0], point[1]);
                        return self.refresh();
                    }
                    Err(e) => {
                        self.status = e.to_string();
                        self.refresh_panel();
                        return Ok(());
                    }
                }
            }
            let nearest = chassis
                .top
                .iter()
                .enumerate()
                .min_by_key(|(_, p)| (p[0] - point[0]).pow(2) + (p[1] - point[1]).pow(2))
                .map(|(i, _)| i);
            self.draw_point = nearest;
            self.status = match nearest {
                Some(index) => format!("selected top point {index}: click again to move it"),
                None => "the top outline has no points".into(),
            };
            self.refresh_panel();
            return Ok(());
        }
        match self.draw_style {
            DrawStyle::Freeform => {
                match self
                    .build
                    .add_chassis_outline_point(Outline::Bottom, point, self.draw_snap)
                {
                    Ok(index) => {
                        self.status =
                            format!("baseplate point {index} at {}, {}", point[0], point[1]);
                        self.refresh()
                    }
                    Err(e) => {
                        self.status = e.to_string();
                        self.refresh_panel();
                        Ok(())
                    }
                }
            }
            DrawStyle::Circle => {
                self.draw_centre = Some(point);
                self.build
                    .set_chassis_circle(Outline::Bottom, point, self.shape_radius)
                    .map_err(|e| e.to_string())?;
                self.status = format!(
                    "circle at {}, {} radius {}",
                    point[0], point[1], self.shape_radius
                );
                self.refresh()
            }
            DrawStyle::Rectangle => {
                self.draw_centre = Some(point);
                self.build
                    .set_chassis_rectangle(
                        Outline::Bottom,
                        point,
                        self.shape_length,
                        self.shape_width,
                    )
                    .map_err(|e| e.to_string())?;
                self.status = format!(
                    "rectangle at {}, {} {}x{}",
                    point[0], point[1], self.shape_length, self.shape_width
                );
                self.refresh()
            }
        }
    }

    /// The sliders' ranges are ours (the `.gib` has no min/max fields); the
    /// blueprint's unit scale and the corpus' chassis heights bound them.
    fn set_slider(&mut self, which: &str, fraction: f32) -> Result<(), String> {
        let fraction = fraction.clamp(0.0, 1.0);
        match which {
            "radius" => {
                self.shape_radius = 100 + (fraction * 400.0).round() as i32;
                self.status = format!("circle radius {}", self.shape_radius);
                if let (Some(centre), true) =
                    (self.draw_centre, self.draw_style == DrawStyle::Circle)
                {
                    self.build
                        .set_chassis_circle(Outline::Bottom, centre, self.shape_radius)
                        .map_err(|e| e.to_string())?;
                    return self.refresh();
                }
            }
            "length" => {
                self.shape_length = 100 + (fraction * 900.0).round() as i32;
                self.status = format!("rectangle length {}", self.shape_length);
                if let (Some(centre), true) =
                    (self.draw_centre, self.draw_style == DrawStyle::Rectangle)
                {
                    self.build
                        .set_chassis_rectangle(
                            Outline::Bottom,
                            centre,
                            self.shape_length,
                            self.shape_width,
                        )
                        .map_err(|e| e.to_string())?;
                    return self.refresh();
                }
            }
            "width" => {
                self.shape_width = 100 + (fraction * 900.0).round() as i32;
                self.status = format!("rectangle width {}", self.shape_width);
                if let (Some(centre), true) =
                    (self.draw_centre, self.draw_style == DrawStyle::Rectangle)
                {
                    self.build
                        .set_chassis_rectangle(
                            Outline::Bottom,
                            centre,
                            self.shape_length,
                            self.shape_width,
                        )
                        .map_err(|e| e.to_string())?;
                    return self.refresh();
                }
            }
            "height" => {
                let height = 0.1 + (1.0 - fraction) * 1.1;
                self.build
                    .set_chassis_height(height)
                    .map_err(|e| e.to_string())?;
                self.status = format!("chassis height {height:.2} m");
                return self.refresh();
            }
            other => return Err(format!("unknown slider {other:?}")),
        }
        self.refresh_panel();
        Ok(())
    }

    /// The action a click on the synthetic panel names, if any.
    fn panel_action(&self, x: f32, y: f32) -> Option<String> {
        let placed = ra2ui::resolve_with(&self.panel.gib, self.size, Some(self.assets.faces()));
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
        if chain.iter().any(|p| p.widget.name() == "attach button") {
            return Some("m9:attach".into());
        }
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
        if self.tab() == Some("chassis button") {
            // The step layer's group (`steps and snap group`, 46% x 70 px in
            // the content corner) lies over the tools pane's lower-left
            // buttons by design; the control under the point is what the
            // player clicks, not the container drawn over it.
            let chain = match ra2ui::input::hit_control(&placed, x, y) {
                Some(path) => self::chain(&placed, &path),
                None => chain.clone(),
            };
            if chain
                .iter()
                .any(|p| p.widget.name() == "clear outline button")
            {
                return Some("m9:outline:clear".into());
            }
            if chain.iter().any(|p| p.widget.name() == "finished button") {
                return Some("m9:finished".into());
            }
            if chain.iter().any(|p| p.widget.name() == "snap checkbox") {
                return Some(if self.draw_snap {
                    "m9:snap:off".into()
                } else {
                    "m9:snap:on".into()
                });
            }
            for (button, style) in [
                ("freeform checkbox", "freeform"),
                ("circle checkbox", "circle"),
                ("rectangle checkbox", "rectangle"),
            ] {
                if chain.iter().any(|p| p.widget.name() == button) {
                    return Some(format!("m9:style:{style}"));
                }
            }
            for (widget, which) in [
                ("radius slider", "radius"),
                ("length slider", "length"),
                ("width slider", "width"),
            ] {
                if let Some(slider) = chain.iter().find(|p| p.widget.name() == widget) {
                    let fraction = ((x - slider.frame.x) / slider.frame.w.max(1.0)).clamp(0.0, 1.0);
                    return Some(format!("m9:slider:{which}:{fraction:.3}"));
                }
            }
            if let Some(slider) = chain.iter().find(|p| p.widget.name() == "height slider") {
                let fraction =
                    (1.0 - (y - slider.frame.y) / slider.frame.h.max(1.0)).clamp(0.0, 1.0);
                return Some(format!("m9:slider:height:{fraction:.3}"));
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
            None if chain.iter().any(|p| p.widget.name() == "Sel Bar")
                && chain.iter().any(|p| p.widget.name() == "components grid") =>
            {
                0
            }
            None => return None,
        };
        Some(format!("m9:palette:{rank}"))
    }

    /// Recompute the panel and every list the current screen asks for.
    /// The player-facing lab: the robot view fills the screen's own 3D
    /// placeholder and no synthetic panel is drawn, unless `--debug-panel`.
    pub fn apply_debug_panel_default(&mut self) -> Result<(), String> {
        self.view_only = !DEBUG_PANEL.load(std::sync::atomic::Ordering::Relaxed);
        self.preview = None;
        self.refresh()
    }

    pub fn refresh(&mut self) -> Result<(), String> {
        self.revision = self.revision.wrapping_add(1);
        self.refresh_overview();
        self.refresh_effects();
        self.refresh_palette();
        self.refresh_comp_preview();
        self.refresh_armour();
        self.refresh_controls();
        self.refresh_panel();
        Ok(())
    }

    /// The tabs that show the open design's numbers (overview, components) take
    /// its name, mass and weight class; the overview's snapshot box takes the
    /// snapshot the `.bot` carries.
    fn refresh_overview(&mut self) {
        let stats = self.build.stats();
        let name = self.build.name().to_string();
        let mass = format!("{:.1}", stats.mass);
        let class = ra2ui::workshop::stats::WeightClass::of_mass(stats.mass).full_label();
        let snapshot = self
            .build
            .to_bot()
            .snapshot
            .as_deref()
            .and_then(ra2ui::preview::Preview::decode_tga);
        let key = snapshot.as_ref().map(|p| p.key("lab-snapshot"));
        if let (Some(key), Some(snapshot)) = (&key, &snapshot) {
            self.assets.add_preview(key, snapshot);
        }
        let Some(lab) = self.shell.lab_mut() else {
            return;
        };
        for block in lab.document.blocks.iter_mut() {
            let ra2fmt::gib::Block::Widget(root) = block else {
                continue;
            };
            for (widget, text) in [
                ("bot name textbox", name.as_str()),
                ("weight", mass.as_str()),
                ("weightclass", class),
            ] {
                if let Some(w) = workshop::find_mut(root, widget) {
                    ra2ui::input::set_field(w, "_Text", text);
                    for child in w.children.iter_mut() {
                        if child.kind == WidgetKind::Text {
                            ra2ui::input::set_field(child, "_Text", text);
                        }
                    }
                }
            }
            if let Some(key) = &key {
                ra2ui::cells::bind_art(root, "snapshot img", key);
            }
        }
    }

    /// The Paint Shop opens on its first effect, Color Fill: the dropdown names
    /// it and `colorfill.gib` is loaded into `effect settings group` (the
    /// group's own note: "load individual effects gibs here").
    fn refresh_effects(&mut self) {
        if self.shell.tab() != Some("paintshop button") {
            return;
        }
        let Ok(fragment) = workshop::load(&self.ui, "botlab/colorfill.gib") else {
            return;
        };
        let Some(lab) = self.shell.lab_mut() else {
            return;
        };
        for block in lab.document.blocks.iter_mut() {
            let ra2fmt::gib::Block::Widget(root) = block else {
                continue;
            };
            if let Some(dropdown) = workshop::find_mut(root, "effect dropdown") {
                if let Some(text) = dropdown
                    .children
                    .iter_mut()
                    .find(|c| c.kind == WidgetKind::TextBox)
                    .and_then(|b| b.children.iter_mut().find(|c| c.kind == WidgetKind::Text))
                {
                    ra2ui::input::set_field(text, "_Text", "Color Fill");
                }
            }
            if let Some(slot) = workshop::find_mut(root, "effect settings group") {
                if slot.children.is_empty() {
                    workshop::load_into(root, &fragment, "effect settings group");
                }
            }
        }
    }

    fn refresh_palette(&mut self) {
        let Some(tab) = self.shell.tab() else { return };
        if tab != "components button" {
            return;
        }
        let mut category = None;
        let scroll = self.palette_scroll;
        let (horizontal, visible, stride) = self.palette_window();
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
                                if horizontal {
                                    let cell = &mut grid.children[ci];
                                    ra2ui::input::set_field(
                                        cell,
                                        "Name",
                                        &format!("m9:cell:{rank}"),
                                    );
                                    let shown = rank >= scroll && rank < scroll + visible;
                                    ra2ui::input::set_field(
                                        cell,
                                        "Visible",
                                        if shown { "true" } else { "false" },
                                    );
                                    if shown {
                                        // `fill_grid` put the cell at its own column;
                                        // the scroll moves the row left by cells.
                                        let at = cell
                                            .field("Position")
                                            .map(|f| f.numbers())
                                            .unwrap_or_default();
                                        let x = at.first().copied().unwrap_or(0.0)
                                            - stride * scroll as f32;
                                        let y = at.get(2).copied().unwrap_or(0.0);
                                        ra2ui::input::set_field(
                                            cell,
                                            "Position",
                                            &format!("{x} 0 {y} 0"),
                                        );
                                    }
                                    continue;
                                }
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
        let Some((frame, _)) = self.control_grid() else {
            return;
        };
        let (cell_w, cell_h) = control_cell_size(frame);
        let selected = self.wire_control;
        if let Some(lab) = self.shell.lab_mut() {
            for block in lab.document.blocks.iter_mut() {
                let ra2fmt::gib::Block::Widget(root) = block else {
                    continue;
                };
                let Some(grid) = workshop::find_mut(root, "control grid") else {
                    continue;
                };
                grid.children.retain(|c| !c.name().starts_with("m9:ctl:"));
                // The grid's selection bar shows only on a selected control.
                for bar in grid.children.iter_mut().filter(|c| c.name() == "Sel Bar") {
                    match selected {
                        Some(cell) => {
                            ra2ui::input::set_field(
                                bar,
                                "Position",
                                &format!(
                                    "{} 0 {} 0",
                                    cell_w * cell[0] as f32,
                                    cell_h * cell[1] as f32
                                ),
                            );
                            ra2ui::input::set_field(bar, "Size", &format!("{cell_w} 0 {cell_h} 0"));
                            ra2ui::input::set_field(bar, "Visible", "true");
                        }
                        None => ra2ui::input::set_field(bar, "Visible", "false"),
                    }
                }
                for control in &board.controls {
                    let x = cell_w * control.cell[0] as f32;
                    let y = cell_h * control.cell[1] as f32;
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
                            &format!("{cell_w} 0 {cell_h} 0"),
                        );
                        let art = match control.kind {
                            ControlKind::Analog => "botlab\\ctrl_analog.bmp",
                            ControlKind::Switch => "botlab\\ctrl_switch.bmp",
                            ControlKind::Button => "botlab\\ctrl_button.bmp",
                        };
                        ra2ui::cells::bind_art(&mut cell, "control img", art);
                        ra2ui::cells::bind_value(&mut cell, "control name", &control.name);
                        ra2ui::cells::bind_value(&mut cell, "control input", &control.key_label());
                        grid.children.push(cell);
                    }
                }
                return;
            }
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
        let _ = grid;
        let (w, h) = control_cell_size(frame);
        Ok([
            frame.x + w * (cell[0] as f32 + 0.5),
            frame.y + h * (cell[1] as f32 + 0.5),
        ])
    }

    /// The cell a point inside the shipped `control grid` names, or `None` when
    /// it is outside the board's 3x3 cells ([`control_cell_size`]).
    fn control_cell_at(&self, x: f32, y: f32) -> Option<[i32; 2]> {
        let (frame, _) = self.control_grid()?;
        if !frame.contains_point(x, y) {
            return None;
        }
        let (w, h) = control_cell_size(frame);
        let (column, row) = (((x - frame.x) / w) as i32, ((y - frame.y) / h) as i32);
        (column < 3 && row < 3).then_some([column, row])
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
        if self.shell.tab() == Some("chassis button") {
            rows.push((
                if self.draw_snap {
                    "m9:snap:off".into()
                } else {
                    "m9:snap:on".into()
                },
                format!(
                    "snap to grid: {} (the shipped checkbox's toggle)",
                    if self.draw_snap { "on" } else { "off" }
                ),
                [0.9, 0.8, 0.5, 1.0],
            ));
        } else {
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
        }
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
                for (i, c) in self.build.components().iter().enumerate().take(4) {
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
                rows.push((
                    "m9:undo".into(),
                    "undo last attach (Ctrl+Z)".into(),
                    [0.9, 0.8, 0.5, 1.0],
                ));
                rows.push((
                    "m9:remove".into(),
                    match self.selected_part {
                        Some(i) => format!("remove component {i} (the shipped remove button)"),
                        None => "remove: click a placed part first".into(),
                    },
                    [0.9, 0.8, 0.5, 1.0],
                ));
                if let Some((info, name)) = self.selected_info() {
                    // The shipped `comp_preview` group is the real panel; these
                    // rows are the same controls for the harness frame.
                    rows.push((
                        "m9:attach".into(),
                        format!("[ Attach ] {name} ({:.1} kg)", info.mass),
                        [0.7, 1.0, 0.7, 1.0],
                    ));
                    for id in placement::selectable_mounts(&info) {
                        let types = info
                            .mounts
                            .get(&id)
                            .map(|a| a.types.join(" "))
                            .unwrap_or_default();
                        rows.push((
                            format!("m9:box:{id}"),
                            format!(
                                "{}green box {id} ({types})",
                                if id == self.own_mount { "> " } else { "  " }
                            ),
                            [0.9, 1.0, 0.7, 1.0],
                        ));
                    }
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
                for (mode, name) in [
                    (PaintMode::Fill, "fill"),
                    (PaintMode::Layer, "layer"),
                    (PaintMode::Decal, "decal"),
                ] {
                    rows.push((
                        format!("m9:pmode:{name}"),
                        format!(
                            "{}mode: {} (click the chassis)",
                            if self.paint_mode == mode { "> " } else { "" },
                            mode.label()
                        ),
                        [0.6, 0.8, 1.0, 1.0],
                    ));
                }
                if self.paint_mode == PaintMode::Decal {
                    for (i, entry) in self.library.decals().entries.iter().take(3).enumerate() {
                        let selected = self
                            .selected_decal
                            .as_ref()
                            .is_some_and(|t| t.name.eq_ignore_ascii_case(&entry.name));
                        rows.push((
                            format!("m9:dtex:{i}"),
                            format!("{}decal: {}", if selected { "> " } else { "" }, entry.name),
                            [0.9, 1.0, 0.7, 1.0],
                        ));
                    }
                } else {
                    for (i, entry) in self.library.textures().entries.iter().take(2).enumerate() {
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
                    if let Some(name) = custom.first() {
                        rows.push((
                            "m9:ctex:0".into(),
                            format!("custom: {name}"),
                            [0.8, 0.9, 0.8, 1.0],
                        ));
                    }
                }
                for (i, group) in self.build.paint().groups().iter().take(2).enumerate() {
                    rows.push((
                        format!("m9:face:{i}"),
                        format!(
                            "{}face {i}: [{},{}]..[{},{}] {} tris",
                            if self.selected_group == Some(i) {
                                "> "
                            } else {
                                ""
                            },
                            group.bounds[0],
                            group.bounds[1],
                            group.bounds[2],
                            group.bounds[3],
                            group.triangles
                        ),
                        [0.9, 1.0, 0.7, 1.0],
                    ));
                }
                if self.paint_mode == PaintMode::Decal {
                    rows.push((
                        "m9:dscale".into(),
                        format!(
                            "scale {:.2} (click to step; at {:.2}, {:.2})",
                            self.decal_scale, self.decal_at[0], self.decal_at[1]
                        ),
                        [0.9, 0.8, 0.5, 1.0],
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
        let has_3d = placed.iter().flat_map(|p| p.walk()).any(|p| {
            p.widget.kind == WidgetKind::Placeholder
                && p.widget.name().to_ascii_lowercase().starts_with("3d")
        });
        let nav_top = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "common buts")
            .map(|p| p.frame.y)
            .unwrap_or(self.size[1]);
        if !has_3d {
            // Chassis step 1 has no 3D view: its shipped `Editing Area` is the
            // drawing surface. The panel becomes a three-row strip along the
            // bottom of the blueprint, clear of the drawing area and of the
            // right-hand tools.
            let content = placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| {
                    p.widget.name() == "blueprint" || p.widget.name() == workshop::CONTENT_SLOT
                })
                .map(|p| p.frame)
                .unwrap_or(Frame::new(0.0, 0.0, self.size[0], self.size[1]));
            let height = 44.0;
            let y = (content.bottom() - height - 2.0).max(content.y);
            let frame = Frame::new(content.x + 2.0, y, (content.w - 4.0).max(10.0), height);
            let preview = Frame::new(frame.x, frame.y, 0.0, 0.0);
            return (preview, frame);
        }
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
            .unwrap_or(Frame::new(0.0, 0.0, self.size[0], self.size[1]));
        // The shell's own bottom bar is drawn over the content area; a panel
        // that ran under it would be unreadable and unclickable.
        if self.view_only {
            return (
                placeholder,
                Frame::new(placeholder.x, placeholder.y, 0.0, 0.0),
            );
        }
        let bottom = placeholder.bottom().min(nav_top - 2.0);
        let height = (bottom - placeholder.y).max(40.0);
        // The Components tab needs room to drag a part about; its rows are few.
        let share = if self.shell.tab() == Some("components button") {
            0.6
        } else {
            0.28
        };
        let preview_h = (height * share).max(40.0);
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
            self.assets
                .scene(&placed, [self.size[0] as u32, self.size[1] as u32])
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
        if self.tab() == Some("components button") {
            if let Some(rect) = self.comp_rect() {
                if self.comp_preview_pixels(rect).is_some() {
                    if let Some(cp) = &self.comp_preview {
                        let texture = scene.textures.len();
                        scene.textures.push(UiTexture {
                            label: "m9 component preview".into(),
                            width: cp.width,
                            height: cp.height,
                            rgba: cp.pixels.clone(),
                        });
                        scene.quads.push(UiQuad {
                            rect,
                            corners: None,
                            uv: [0.0, 0.0, 1.0, 1.0],
                            color: [1.0, 1.0, 1.0, 1.0],
                            texture,
                            alpha: false,
                            clip: Some(rect),
                        });
                    }
                    // The Attach button is a child of the preview's placeholder,
                    // so the 3D view is drawn under it.
                    let placed = self.shell.placements();
                    if let Some(group) = placed.iter().flat_map(|p| p.walk()).find(|n| {
                        n.children
                            .iter()
                            .any(|c| c.widget.name() == "attach button")
                    }) {
                        let over = self.assets.scene(
                            std::slice::from_ref(group),
                            [self.size[0] as u32, self.size[1] as u32],
                        );
                        let offset = scene.textures.len();
                        scene.textures.extend(over.textures);
                        for mut quad in over.quads {
                            quad.texture += offset;
                            scene.quads.push(quad);
                        }
                    }
                }
            }
        }
        if self.view_only {
            return Ok(scene);
        }
        let panel_placed =
            ra2ui::resolve_with(&self.panel.gib, self.size, Some(self.assets.faces()));
        let panel_scene = self
            .assets
            .scene(&panel_placed, [self.size[0] as u32, self.size[1] as u32]);
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
            .render_at(renderer.device(), renderer.queue(), &scene, size)
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
        let width = rect.w.round().max(16.0) as u32;
        let height = rect.h.round().max(16.0) as u32;
        if let Some((revision, pixels, w, h)) = &self.preview {
            if *revision == self.revision && *w == width && *h == height {
                return Ok(Some((pixels.clone(), *w, *h)));
            }
        }
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
        // The dragged part rides on top of the robot; it is not part of the
        // framing, or the camera would chase the mouse.
        if let Some(active) = self.drag.as_ref() {
            let (items, model) = match &active.candidate {
                Some(c) if active.refusal.is_none() => (&active.items, c.world),
                Some(c) => (&active.red, c.world),
                None => (&active.red, active.floating),
            };
            if !items.is_empty() {
                groups.push(RenderGroup {
                    items: items.as_slice(),
                    model,
                });
            }
        }
        let center = (min + max) * 0.5;
        let radius = ((max - min).length() * 0.5).max(0.6);
        let (eye, up) = if self.view_only {
            // The original's lab looks straight down at the robot, nose up the
            // screen, over the 3D view's 0.15 grey.
            self.renderer.set_background([0.15; 3]);
            (center + Vec3::new(0.0, radius * 3.0, 0.0), Vec3::Z)
        } else {
            (
                center + Vec3::new(radius * 1.9, radius * 1.2, radius * 2.1),
                Vec3::Y,
            )
        };
        let view = ra2_render::look_at_lh(eye, center, up);
        let proj = ra2_render::perspective_lh(
            45f32.to_radians(),
            width as f32 / height as f32,
            0.05,
            radius * 30.0,
        );
        let view_proj = proj * view;
        self.preview_camera = Some(PreviewCamera {
            view_proj,
            inv_view_proj: view_proj.inverse(),
            width,
            height,
        });
        let pixels = self
            .renderer
            .render_groups(&groups, view_proj, width, height)
            .map_err(|e| e.to_string())?;
        self.preview = Some((self.revision, pixels.clone(), width, height));
        Ok(Some((pixels, width, height)))
    }
}

/// A robot seen straight down, nose up the screen, over `background`: the
/// original's "3d view group" on the repair bench (`repairs.gib`, `3d view
/// group` holding `repairs 3d placeholder`) and the lab's own view. Pixels are
/// RGBA, `width x height`; `None` when the design has nothing to draw.
pub fn robot_top_view(
    renderer: &Renderer,
    game: &Path,
    build: &Build,
    background: [f32; 3],
    width: u32,
    height: u32,
) -> Result<Option<Vec<u8>>, String> {
    let library = GameLibrary::open(game).map_err(|e| e.to_string())?;
    let bot = build.to_bot();
    let world = build.transforms();
    let chassis = ra2_render::chassis_items(&bot, Mat4::IDENTITY);
    let mut owned: Vec<(usize, Vec<DrawItem>)> = Vec::new();
    for i in 1..bot.components.len() {
        let Some(info) = build.component_info(i) else {
            continue;
        };
        let Some(def) = info.def.as_ref() else {
            continue;
        };
        let Some(scene) = library.load_component_mesh(def) else {
            continue;
        };
        let dir = library
            .component_mesh_path(def)
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| game.join("Components"));
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
    let eye = center + Vec3::new(0.0, radius * 3.0, 0.0);
    let view = ra2_render::look_at_lh(eye, center, Vec3::Z);
    let proj = ra2_render::perspective_lh(
        45f32.to_radians(),
        width as f32 / height as f32,
        0.05,
        radius * 30.0,
    );
    renderer.set_background(background);
    renderer
        .render_groups(&groups, proj * view, width, height)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// The control grid's cell size: the board is three cells across and three
/// down, each a third of the grid's frame (capture: rules at x 556, 698, 840,
/// 982 and y 386, 499, 612 on a 426 x 339 px grid). The file's own
/// `Columns 4`/`ColWidths 66`/`RowHeight 70` are the editor's canvas.
fn control_cell_size(frame: Frame) -> (f32, f32) {
    (frame.w / 3.0, frame.h / 3.0)
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
                field("Position", &format!("4 0 {} 0", y + ROW_BASELINE)),
                field(
                    "Size",
                    &format!("{} 0 {} 0", (frame.w - 8.0).max(10.0), ROW_HEIGHT),
                ),
                field("Visible", "true"),
                field("ChildOrder", "0"),
                // The row's box is its hit area; the text is not clipped to
                // it (its descenders and baseline fall below the ascent).
                field("ClipMode", "0"),
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

/// Möller–Trumbore: the ray's distance and barycentric `u, v` of a hit.
fn ray_triangle(
    origin: Vec3,
    direction: Vec3,
    a: Vec3,
    b: Vec3,
    c: Vec3,
) -> Option<(f32, f32, f32)> {
    let edge1 = b - a;
    let edge2 = c - a;
    let h = direction.cross(edge2);
    let det = edge1.dot(h);
    if det.abs() < 1e-8 {
        return None;
    }
    let inv = 1.0 / det;
    let s = origin - a;
    let u = inv * s.dot(h);
    if !(-1e-6..=1.0 + 1e-6).contains(&u) {
        return None;
    }
    let q = s.cross(edge1);
    let v = inv * direction.dot(q);
    if v < -1e-6 || u + v > 1.0 + 1e-6 {
        return None;
    }
    let t = inv * edge2.dot(q);
    (t > 1e-6).then_some((t, u, v))
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
    if let Some(view) = &options.view {
        let mut builder = Builder::open_sized(
            &options.game,
            options.start.as_deref(),
            [options.size[0] as f32, options.size[1] as f32],
        )?;
        builder.view_only = true;
        builder.refresh()?;
        // The harness path: the bot lab as the player sees it after opening a
        // design from the inventory and clicking one tab, no editing.
        let button = match view.as_str() {
            "overview" => "overview button",
            "chassis" => "chassis button",
            "components" => "components button",
            "wiring" => "wiring button",
            "paint" => "paintshop button",
            other => return Err(format!("unknown --build-view {other:?}")),
        };
        if view != "overview" {
            builder.click_named(button)?;
        }
        return builder.shot(&out.join(format!("build-view-{view}.png")), options.size);
    }
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

    // 1b. Chassis step 2: move a top-panel point (the shipped "select points
    // and move them"), with the shipped snap checkbox turned off first.
    builder.click_named("step 2 button")?;
    // The shipped `snap checkbox` is drawn over by its own two state images,
    // so click its frame rather than looking for a reachable named widget.
    let snap_frame = builder
        .shell()
        .placements()
        .iter()
        .flat_map(|p| p.walk())
        .find(|p| p.widget.name() == "snap checkbox")
        .map(|p| p.frame);
    if let Some(frame) = snap_frame {
        builder.click(frame.x + frame.w * 0.5, frame.y + 2.0)?;
    }
    if builder.draw_snap {
        // The shipped checkbox is drawn over by its own state images; the
        // panel's row drives the same model flag.
        builder.click_action("m9:snap:off")?;
    }
    println!(
        "  chassis: snap {}",
        if builder.draw_snap { "on" } else { "off" }
    );
    let top = builder
        .build
        .chassis()
        .map(|c| c.top.clone())
        .unwrap_or_default();
    if let Some(point) = top.first().copied() {
        let select = builder.outline_point_for(point)?;
        builder.click(select[0], select[1])?;
        let target = [
            point[0] + (500 - point[0]) / 4,
            point[1] + (500 - point[1]) / 4,
        ];
        let moved = builder.outline_point_for(target)?;
        builder.click(moved[0], moved[1])?;
        let after = builder
            .build
            .chassis()
            .map(|c| c.top.clone())
            .unwrap_or_default();
        if after.first().copied() == Some(point) {
            return Err(format!(
                "the chassis drawing click did not move top point 0: {:?}",
                after.first()
            ));
        }
        println!(
            "  chassis: moved top point 0 from {:?} to {:?} (height {:.2} m)",
            point,
            after.first(),
            builder.build.chassis().map(|c| c.height).unwrap_or(0.0)
        );
    }
    if frames {
        builder.shot(&out.join("chassis-step.png"), target)?;
    }

    // 2. The components tab: the preview now shows the moved top point (the
    // drawing tool's result), then the palette work.
    builder.click_named("components button")?;
    if frames {
        builder.shot(&out.join("chassis-draw.png"), target)?;
    }
    builder.click_named("mechanics button")?;
    let motor_file = "Components\\ztek1.txt";
    let motor = builder.build.components().len();
    // Pick the part: it is only selected, and the comp_preview group shows it.
    builder.click_palette_file(motor_file)?;
    if builder.build.components().len() != motor {
        return Err("a palette click attached a part; only Attach may".into());
    }
    if frames {
        builder.shot(&out.join("component-selected.png"), target)?;
    }
    // Attach, then drag the part over the baseplate and click.
    let attach = builder.attach_point()?;
    builder.mouse_moved(attach[0], attach[1]);
    builder.click(attach[0], attach[1])?;
    if !builder.dragging() {
        return Err(format!(
            "Attach did not start a drag ({})",
            builder.status()
        ));
    }
    let place = builder.drag_point(PlaceOn::Baseplate([0.37, -0.22]))?;
    builder.mouse_moved(place[0], place[1]);
    if frames {
        builder.shot(&out.join("component-drag.png"), target)?;
    }
    builder.click(place[0], place[1])?;
    if builder.build.components().len() != motor + 1 {
        return Err(format!(
            "the drop did not attach the motor ({})",
            builder.status()
        ));
    }
    let placed = builder.build.components()[motor].clone();
    if (placed.translation[0] - 0.37).abs() > 0.03
        || (placed.translation[2] + 0.22).abs() > 0.03
        || placed.translation[1] != 0.0
    {
        return Err(format!(
            "the drop did not place the motor on the baseplate: {:?}",
            placed.translation
        ));
    }
    println!(
        "  placement: component {motor} at {:.2}, {:.2} m, angle {:.2}, height {:.2}",
        placed.translation[0], placed.translation[2], placed.angle, placed.height
    );
    if frames {
        builder.shot(&out.join("placement.png"), target)?;
    }
    if frames {
        // A refused spot: the part is drawn red and the tip says why. Clicking
        // over nothing ends the drag.
        builder.click_palette_file(motor_file)?;
        let attach = builder.attach_point()?;
        builder.click(attach[0], attach[1])?;
        let over = builder.drag_point(PlaceOn::Baseplate([0.40, -0.22]))?;
        builder.mouse_moved(over[0], over[1]);
        builder.shot(&out.join("component-refused.png"), target)?;
        println!("  refused drop: {}", builder.status());
        let corner = [builder.panel.preview.x + 3.0, builder.panel.preview.y + 3.0];
        builder.mouse_moved(corner[0], corner[1]);
        builder.click(corner[0], corner[1])?;
        if builder.dragging() {
            return Err("a click over nothing did not end the drag".into());
        }
    }
    // The motor's axle point is the socket the wheel's drag snaps to.
    let axle = builder
        .build
        .component_info(motor)
        .and_then(|info| {
            info.mounts
                .iter()
                .find(|(_, a)| a.types.iter().any(|t| t == "axle_m"))
                .map(|(id, _)| *id)
        })
        .ok_or_else(|| "the motor has no free axle_M mount".to_string())?;
    builder.click_named("mobility button")?;
    builder.place_component(
        "Components\\miniwheel.txt",
        PlaceOn::Mount {
            parent: motor,
            mount: axle,
        },
    )?;
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
    builder.click_control_cell([2, 1])?;
    if builder.build.board().control_at([2, 1]).is_none() {
        return Err("the New Analog button plus a free cell did not add a control".into());
    }
    println!(
        "  wiring: added {} at cell 2,1",
        builder
            .build
            .board()
            .control_at([2, 1])
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
        binding.component == 1 && binding.control == [2, 1, 0] && binding.action == "Spin Clockwise"
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
            .control_at([2, 1])
            .map(|c| c.name.as_str())
    );
    if frames {
        builder.shot(&out.join("wiring-bound.png"), target)?;
        builder.shot(&out.join("build-wiring.png"), target)?;
    }

    // 4. Paint: fill, then a surface layer, then a decal placed with the mouse.
    builder.click_named("paintshop button")?;
    if !builder.build.paint().groups().is_empty() {
        builder.click_action("m9:pmode:fill")?;
        builder.click_action("m9:tex:0")?;
        builder.click_action("m9:face:0")?;
        let after_fill = builder.build.paint().skin().unwrap().clone();
        builder.click_action("m9:pmode:layer")?;
        builder.click_action("m9:tex:1")?;
        builder.click_action("m9:face:0")?;
        let after_layer = builder.build.paint().skin().unwrap().clone();
        if after_layer == after_fill {
            return Err("the surface layer did not change the skin".into());
        }
        if frames {
            builder.shot(&out.join("layer.png"), target)?;
        }
        builder.click_action("m9:pmode:decal")?;
        builder.click_action("m9:dtex:0")?;
        builder.click_action("m9:dscale")?;
        let decal_point = builder.preview_point_at([0.0, 0.0])?;
        builder.click(decal_point[0], decal_point[1])?;
        let after_decal = builder.build.paint().skin().unwrap().clone();
        if after_decal == after_layer {
            return Err("the decal click did not change the skin".into());
        }
        if frames {
            builder.shot(&out.join("decal.png"), target)?;
        }
        // Ctrl+Z on the Paint Shop is the paint undo, not the placement one.
        if !builder.undo() {
            return Err("the paint undo found nothing to undo".into());
        }
        if builder.build.paint().skin().unwrap() != &after_layer {
            return Err("paint undo did not restore the previous skin".into());
        }
        if !builder.redo() {
            return Err("the paint redo found nothing to redo".into());
        }
        if builder.build.paint().skin().unwrap() != &after_decal {
            return Err("paint redo did not restore the decal".into());
        }
    }
    println!(
        "  paint: {} group(s), changed {}, modes fill/layer/decal",
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

    /// A palette click only selects the part; Attach plus a drag and a click
    /// adds exactly one component, and the model's undo reverses it.
    #[test]
    fn a_palette_click_selects_and_only_a_drag_attaches_one_component() {
        let Some(game) = game_dir() else { return };
        let mut builder = Builder::open(&game, None).unwrap();
        builder.click_named("components button").unwrap();
        builder.click_named("mechanics button").unwrap();
        let before = builder.build.components().len();
        builder.click_palette_file("Components\\ztek1.txt").unwrap();
        assert_eq!(
            builder.build.components().len(),
            before,
            "a palette click attaches nothing"
        );
        assert_eq!(builder.status(), ra2ui::workshop::placement::TIP_BOXES);
        let index = builder
            .place_component("Components\\ztek1.txt", PlaceOn::Baseplate([0.2, 0.1]))
            .unwrap();
        assert_eq!(index, before);
        assert_eq!(builder.build.components().len(), before + 1);
        assert!(builder.build.can_undo());
        assert!(builder.undo());
        assert_eq!(builder.build.components().len(), before);
    }

    /// Open the lab on the Mechanics palette with the Z-Tek selected and the
    /// drag started, the mouse on the Attach button.
    fn dragging_ztek(game: &Path) -> Builder {
        let mut builder = Builder::open(game, None).unwrap();
        builder.click_named("components button").unwrap();
        builder.click_named("mechanics button").unwrap();
        builder.click_palette_file("Components\\ztek1.txt").unwrap();
        let attach = builder.attach_point().unwrap();
        builder.mouse_moved(attach[0], attach[1]);
        builder.click(attach[0], attach[1]).unwrap();
        assert!(builder.dragging(), "Attach starts the drag");
        assert_eq!(builder.status(), placement::TIP_DRAG);
        builder
    }

    /// SHIFT turns the part by the dominant axis of the mouse movement in ten
    /// degree steps and CTRL raises it in five centimetre steps, and the
    /// drop stores both in the file's own `angle` and `height`.
    #[test]
    fn shift_and_control_turn_and_raise_the_dragged_part() {
        let Some(game) = game_dir() else { return };
        let mut builder = dragging_ztek(&game);
        let at = builder.drag_point(PlaceOn::Baseplate([0.1, 0.0])).unwrap();
        builder.mouse_moved(at[0], at[1]);
        // Hold SHIFT and move 12 px right and 3 down: dominant axis +12, so
        // 12 * 0.2 = 2.4 steps, which truncates to 2 = 20 degrees.
        builder.set_modifiers(true, false);
        builder.mouse_moved(at[0] + 12.0, at[1] + 3.0);
        // Hold CTRL and move 10 px: 2 steps = 0.10 m.
        builder.set_modifiers(false, true);
        builder.mouse_moved(at[0] + 12.0, at[1] + 13.0);
        builder.set_modifiers(false, false);
        // The part stayed where it was while the modifiers were held.
        builder.mouse_moved(at[0], at[1]);
        builder.click(at[0], at[1]).unwrap();
        let part = builder.build.components().last().unwrap().clone();
        assert!(
            (part.angle - 20f32.to_radians()).abs() < 1e-3,
            "angle {}",
            part.angle
        );
        assert!((part.height - 0.10).abs() < 1e-4, "height {}", part.height);
        assert!((part.translation[0] - 0.1).abs() < 0.03);
        // The dropped robot is the game's own file: the writer's bytes read
        // back to the same components and write back to the same bytes.
        let bytes = builder.build.save();
        let back = ra2fmt::bot::parse(&bytes).unwrap();
        assert_eq!(back.components, builder.build.to_bot().components);
        assert_eq!(back.write(), bytes);
        assert_eq!(back.components.last().unwrap().angle, part.angle);
    }

    /// A refused spot keeps the part in the drag with the exe's text; a click
    /// over nothing ends it; and nothing was attached either way.
    #[test]
    fn a_refused_spot_keeps_the_drag_and_a_click_over_nothing_ends_it() {
        let Some(game) = game_dir() else { return };
        let mut builder = Builder::open(&game, None).unwrap();
        builder.click_named("components button").unwrap();
        builder.click_named("mechanics button").unwrap();
        builder
            .place_component("Components\\ztek1.txt", PlaceOn::Baseplate([0.0, 0.0]))
            .unwrap();
        let count = builder.build.components().len();
        builder.click_palette_file("Components\\ztek1.txt").unwrap();
        let attach = builder.attach_point().unwrap();
        builder.click(attach[0], attach[1]).unwrap();
        // Onto the first motor.
        let over = builder.drag_point(PlaceOn::Baseplate([0.03, 0.0])).unwrap();
        builder.mouse_moved(over[0], over[1]);
        assert_eq!(
            builder.status(),
            "Components should not intersect the chassis or other components."
        );
        builder.click(over[0], over[1]).unwrap();
        assert!(builder.dragging(), "a refused part stays in the drag");
        assert_eq!(builder.build.components().len(), count);
        // Off the baseplate sheet altogether: over nothing.
        let nothing = [builder.panel.preview.x + 3.0, builder.panel.preview.y + 3.0];
        builder.mouse_moved(nothing[0], nothing[1]);
        builder.click(nothing[0], nothing[1]).unwrap();
        assert!(!builder.dragging(), "a click over nothing cancels");
        assert_eq!(builder.build.components().len(), count);
    }

    /// Mass check on the Attach button: a robot at the limit refuses the part
    /// with the exe's sentence and no drag starts.
    #[test]
    fn attach_refuses_a_part_that_would_pass_the_mass_limit() {
        let Some(game) = game_dir() else { return };
        let mut builder = Builder::open(&game, None).unwrap();
        builder
            .build
            .set_limits(ra2ui::workshop::build::BuildLimits { max_mass: 60.0 });
        builder.click_named("components button").unwrap();
        builder.click_named("mechanics button").unwrap();
        builder.click_palette_file("Components\\ztek1.txt").unwrap();
        let attach = builder.attach_point().unwrap();
        builder.click(attach[0], attach[1]).unwrap();
        assert!(!builder.dragging());
        assert_eq!(
            builder.status(),
            "Total mass of robot must not exceed 60 kg."
        );
    }

    /// A placed part is selected by clicking it, there is no move, and the
    /// remove button refuses a part that still has children.
    #[test]
    fn a_click_selects_a_placed_part_and_remove_refuses_one_with_children() {
        let Some(game) = game_dir() else { return };
        let mut builder = Builder::open(&game, None).unwrap();
        builder.click_named("components button").unwrap();
        builder.click_named("mechanics button").unwrap();
        let motor = builder
            .place_component("Components\\ztek1.txt", PlaceOn::Baseplate([0.0, 0.0]))
            .unwrap();
        let axle = builder
            .build
            .component_info(motor)
            .and_then(|i| {
                i.mounts
                    .iter()
                    .find(|(_, a)| a.types.iter().any(|t| t == "axle_m"))
                    .map(|(id, _)| *id)
            })
            .unwrap();
        builder.click_named("mobility button").unwrap();
        builder
            .place_component(
                "Components\\miniwheel.txt",
                PlaceOn::Mount {
                    parent: motor,
                    mount: axle,
                },
            )
            .unwrap();
        // Click the motor in the preview: it is selected.
        let at = builder.drag_point(PlaceOn::Baseplate([0.0, 0.0])).unwrap();
        builder.click(at[0], at[1]).unwrap();
        assert_eq!(builder.selected_part, Some(motor));
        builder.click_action("m9:remove").unwrap();
        assert_eq!(
            builder.status(),
            "You must delete the components attached to this first."
        );
        assert_eq!(builder.build.components().len(), 3);
        // The wheel is a leaf: select it by row and remove it, then the motor.
        builder.click_action("m9:part:2").unwrap();
        builder.click_action("m9:remove").unwrap();
        assert_eq!(builder.build.components().len(), 2);
        builder.click_action("m9:part:1").unwrap();
        builder.click_action("m9:remove").unwrap();
        assert_eq!(builder.build.components().len(), 1);
    }

    /// The green boxes: a click on a box in the component preview chooses the
    /// part's own mount, and the drag uses it.
    #[test]
    fn a_click_on_a_green_box_chooses_the_parts_own_mount() {
        let Some(game) = game_dir() else { return };
        let mut builder = Builder::open(&game, None).unwrap();
        builder.click_named("components button").unwrap();
        builder.click_named("mechanics button").unwrap();
        builder.click_palette_file("Components\\ztek1.txt").unwrap();
        let (info, _) = builder.selected_info().unwrap();
        let boxes = placement::selectable_mounts(&info);
        assert!(boxes.len() >= 2, "the Z-Tek has two base faces: {boxes:?}");
        let first = builder.own_mount;
        let other = *boxes.iter().find(|b| **b != first).unwrap();
        let at = builder.box_point(other).unwrap();
        builder.click(at[0], at[1]).unwrap();
        assert_eq!(builder.own_mount, other);
        let attach = builder.attach_point().unwrap();
        builder.click(attach[0], attach[1]).unwrap();
        let place = builder.drag_point(PlaceOn::Baseplate([0.2, 0.2])).unwrap();
        builder.mouse_moved(place[0], place[1]);
        builder.click(place[0], place[1]).unwrap();
        assert_eq!(builder.build.components().last().unwrap().own_mount, other);
    }

    /// `styles = Standard, ztek2.txt, High Power`: the list has both labels and
    /// picking the second places `Components\Styles\ztek2.txt`.
    #[test]
    fn the_style_list_swaps_the_definition_the_part_is_placed_with() {
        let Some(game) = game_dir() else { return };
        let mut builder = Builder::open(&game, None).unwrap();
        builder.click_named("components button").unwrap();
        builder.click_named("mechanics button").unwrap();
        builder.click_palette_file("Components\\ztek1.txt").unwrap();
        let styles = builder.style_list();
        assert_eq!(
            styles
                .iter()
                .map(|(label, _)| label.as_str())
                .collect::<Vec<_>>(),
            ["Standard", "High Power"]
        );
        // Open the dropdown and click its second row.
        let field = builder
            .shell()
            .placements()
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "component option")
            .map(|p| p.frame)
            .unwrap();
        builder
            .click(field.x + 10.0, field.y + field.h * 0.5)
            .unwrap();
        let list = builder
            .shell()
            .placements()
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| {
                p.widget.kind == WidgetKind::ListBox && p.frame.h > 0.0 && p.frame.y >= field.y
            })
            .map(|p| p.frame)
            .unwrap();
        // Rows are 18 px: the second one.
        builder.click(list.x + 10.0, list.y + 27.0).unwrap();
        assert_eq!(
            builder.style_file.as_deref(),
            Some("Components\\Styles\\ztek2.txt")
        );
        let attach = builder.attach_point().unwrap();
        builder.click(attach[0], attach[1]).unwrap();
        let place = builder.drag_point(PlaceOn::Baseplate([0.2, 0.2])).unwrap();
        builder.mouse_moved(place[0], place[1]);
        builder.click(place[0], place[1]).unwrap();
        assert_eq!(
            builder.build.components().last().unwrap().file,
            "Components\\Styles\\ztek2.txt"
        );
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
        builder
            .place_component("Components\\ztek1.txt", PlaceOn::Baseplate([0.2, 0.2]))
            .unwrap();
        assert_eq!(builder.build.components()[1].class, "SpinMotor");
        builder.click_named("wiring button").unwrap();
        let before = builder.build.board().controls.len();
        builder.click_named("new analog").unwrap();
        builder.click_control_cell([2, 1]).unwrap();
        assert_eq!(builder.build.board().controls.len(), before + 1);
        let added = builder.build.board().control_at([2, 1]).unwrap().clone();
        assert_eq!(added.kind, ControlKind::Analog);
        assert_eq!(added.name, "New Analog");

        builder.click_action("m9:wirecomp:1").unwrap();
        builder.click_action("m9:wireact:0:0").unwrap();
        assert_eq!(
            builder.build.board().bindings.last(),
            Some(&ra2fmt::bot::Binding {
                component: 1,
                action: "Spin Clockwise".into(),
                control: [2, 1, 0],
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
            .bind_action(1, Action::OnOffClockwise, [2, 1], 0)
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

    /// The shipped chassis step's slider and `clear outline button` drive the
    /// model, and a click in the `Editing Area` adds a baseplate point.
    #[test]
    fn the_shipped_chassis_slider_and_clear_button_act_on_the_outline() {
        let Some(game) = game_dir() else { return };
        let mut builder = Builder::open(&game, None).unwrap();
        builder.click_named("chassis button").unwrap();
        builder.click_named("structure design button").unwrap();

        let slider = builder
            .shell()
            .placements()
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "radius slider")
            .map(|p| p.frame)
            .expect("the shipped radius slider");
        builder
            .click(slider.x + slider.w * 0.75, slider.y + slider.h * 0.5)
            .unwrap();
        assert_eq!(builder.shape_radius, 400);

        let area = builder.outline_point_for([500, 500]).unwrap();
        builder.click(area[0], area[1]).unwrap();
        let points = builder.build.chassis().unwrap().bottom.len();
        assert!(points > 4, "the freeform click added a point: {points}");

        // The button's centre can sit under the steps group's row at some
        // sizes; `click_named` finds a point that actually reaches it.
        builder.click_named("clear outline button").unwrap();
        assert!(builder.build.chassis().unwrap().bottom.is_empty());
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
            view: None,
        };
        run_headless(&options).expect("the scripted build runs");
        for name in [
            "build-chassis.png",
            "chassis-step.png",
            "chassis-draw.png",
            "build-parts.png",
            "placement.png",
            "build-wiring.png",
            "wiring-add.png",
            "wiring-bound.png",
            "build-paint.png",
            "layer.png",
            "decal.png",
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
