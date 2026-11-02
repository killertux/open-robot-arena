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

use std::f32::consts::FRAC_PI_2;
use std::path::{Path, PathBuf};

use glam::{Mat4, Vec3};
use ra2_render::ui::{UiAssets, UiQuad, UiRenderer, UiScene, UiTexture};
use ra2_render::{DrawItem, RenderGroup, Renderer};
use ra2_shell::Shell;
use ra2fmt::gib::{Field, Gib, Widget, WidgetKind};
use ra2ui::components::Component as PaletteComponent;
use ra2ui::workshop::adjust::{TopMove, BEND_TEXT};
use ra2ui::workshop::build::{Build, BuildError};
use ra2ui::workshop::chassis::{self, Outline};
use ra2ui::workshop::library::{ComponentInfo, GameLibrary, TextureEntry};
use ra2ui::workshop::paint::Texture;
use ra2ui::workshop::placement::{self, Candidate, Drag, Hover, Refusal};
use ra2ui::workshop::wiring::{Control, ControlKind};
use ra2ui::workshop::{self};
use ra2ui::{Frame, Placement};

use crate::lab_dialog::{self, DialogKind, Focus, LabDialog};
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

/// The Paint Shop's Color Fill state (`colorfill.gib`, live captures of the Paint Shop): the picker's
/// target (fractions of the `hue group`), the shade / tint slider (0 dark .. 1 white) and the colour
/// the swatch shows. Live the swatch starts as a colour of its own (`(185, 150, 18)` in the first
/// captures, `(161, 176, 25)` in another run) with the target at 0.18 / 0.26 and the thumb at 0.4;
/// the first click on the picker or the slider makes it the target's hue and saturation at the
/// thumb's lightness (measured: target 0.184 / 0.253, thumb 0.40 gives `(161, 176, 25)`).
#[derive(Clone, Copy, Debug)]
pub struct PaintColor {
    pub marker: [f32; 2],
    pub lum: f32,
    pub rgb: [u8; 3],
    drag: Option<PaintDrag>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaintDrag {
    Picker,
    Slider,
}

impl Default for PaintColor {
    fn default() -> Self {
        PaintColor {
            marker: [0.184, 0.253],
            lum: 0.4,
            rgb: [185, 150, 18],
            drag: None,
        }
    }
}

impl PaintColor {
    /// The colour the picker's target (`x` = hue, `1 - y` = saturation) and the slider choose: HSL.
    pub fn picked(&self) -> [u8; 3] {
        let h = self.marker[0].clamp(0.0, 1.0) * 360.0;
        let s = (1.0 - self.marker[1]).clamp(0.0, 1.0);
        let l = self.lum.clamp(0.0, 1.0);
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = l - c / 2.0;
        let (r, g, b) = match (h / 60.0) as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let byte = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        [byte(r), byte(g), byte(b)]
    }
}

/// The lab's orbit camera (`FUN_005a6260`, mode 2): one per tab, 5 m from the robot's origin to
/// start, pitch and yaw 0, the wheel zooming within the tab's limits and the middle button
/// turning it (0.01 rad per mouse count).
#[derive(Clone, Copy, Debug)]
pub struct LabCam {
    pub pitch: f32,
    pub yaw: f32,
    pub dist: f32,
}

impl LabCam {
    pub const START: LabCam = LabCam {
        pitch: 0.0,
        yaw: 0.0,
        dist: 5.0,
    };

    /// The eye and up vector in the robot's own space, looking at its origin.
    pub fn eye_up(&self) -> (Vec3, Vec3) {
        let (sp, cp) = self.pitch.sin_cos();
        let (sy, cy) = self.yaw.sin_cos();
        let eye = -self.dist * Vec3::new(sy * cp, sp, cy * cp);
        let up = Vec3::new(-sp * sy, cp, -sp * cy);
        (eye, up)
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

/// The widget that carries the lab's `Info Display` (the file's record is a `Glyph` that the
/// `DisplayBox` named `Info Display` dresses; the tip's widgets hang on the Glyph, whose frame is
/// the box's).
fn info_display_mut(widget: &mut Widget) -> Option<&mut Widget> {
    if widget
        .attachments
        .iter()
        .any(|a| a.body.as_ref().is_some_and(|b| b.name() == "Info Display"))
    {
        return Some(widget);
    }
    widget.children.iter_mut().find_map(info_display_mut)
}

/// The tip's two widgets, as GIB records (`{ICONX}` / `{TEXTX}` fill in the centring).
const TIP_GIB: &str = "T 1.09\r
[Image]\r
_Tiling false\r
_Filename botlab\\{ICON}\r
_BufferWidth 0 0 0 0\r
_Size {ICONW} 0 17 0\r
_Alpha {ALPHA}\r
_Filtering 0\r
_ForeColor 1 1 1 1\r
_Name \r
_Position {ICONX} 0 {ICONY} 0\r
0 0 1 1\r
Name tip icon\r
ID -1\r
Position {ICONX} 0 {ICONY} 0\r
Size {ICONW} 0 17 0\r
Visible true\r
ChildOrder 0\r
ClipMode 0\r
NumChildren 0\r
[Text]\r
_Font fg_mcn14\r
_Text {TEXT}\r
_ForeColor {COLOR} 1\r
_Name \r
_Position {TEXTX} 0 {TEXTY} 0\r
Name tip text\r
ID -1\r
Position {TEXTX} 0 {TEXTY} 0\r
Size 700 0 16 0\r
Visible true\r
ChildOrder 1\r
ClipMode 0\r
NumChildren 0\r
";

/// The live workshop: the shell, the model, and the panel drawn over the 3D
/// placeholder.
pub struct Builder {
    /// The size the shell is laid out at (1280x1005 for the window; the
    /// harness lays it out natively at the capture's size).
    size: [f32; 2],
    /// Which of the four random tips the lab shows (`FUN_0043e900` rolls again whenever the screen
    /// changes) and the screen it was rolled for.
    tip_roll: (u32, u32),
    tip_screen: Option<(String, Option<String>)>,
    /// The tip last written into the Info Display (its text and the screen size it was centred for).
    tip_written: Option<String>,
    /// The orange alert over the tip (`FUN_0043e7c0`: `alert.tga` and `1 .8 .3` text, the lab's
    /// `Info Display`) and the seconds it has left (`0x714890`, set to 2.0 on every call, counted
    /// down by the lab's per-frame update at `0x43e683`; an empty alert puts the tip back).
    alert: Option<String>,
    alert_left: f32,
    /// The line the lab's events put in the tip (`FUN_0043e6e0`: the attachment tips, the drag
    /// tip, an empty line after a cancelled drag) until the next screen change recomputes it
    /// (`FUN_0043e900`).
    tip_set: Option<String>,
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
    /// The wiring window in front of the lab, if one is open.
    dialog: Option<LabDialog>,
    wire_control: Option<[i32; 2]>,
    wire_component: Option<usize>,
    /// The palette row on top (the grid is a one-row window of a longer list).
    palette_scroll: usize,
    selected_texture: Option<TextureEntry>,
    paint_mode: PaintMode,
    paint_color: PaintColor,
    selected_decal: Option<TextureEntry>,
    selected_group: Option<usize>,
    decal_at: [f32; 2],
    decal_scale: f32,
    /// The current chassis-drawing state.
    draw_style: DrawStyle,
    draw_centre: Option<[i32; 2]>,
    /// Chassis step 2: the top point with the bright ring (under the cursor, or being dragged).
    draw_point: Option<usize>,
    /// Chassis step 2: the button is down on a top point and the point follows the mouse.
    point_drag: bool,
    /// `DAT_006f52e0`: the editing area shows its three outline alerts only once a move was
    /// accepted since the last point was added to an outline (docs/36 section 9.27e).
    adjust_armed: bool,
    /// Whether the baseplate polygon was closed by a click on its first point.
    outline_closed: bool,
    draw_snap: bool,
    shape_radius: i32,
    shape_length: i32,
    shape_width: i32,
    name_buffer: String,
    /// The Overview's `bot name textbox` has the focus: live, a key goes to the box only after a
    /// click on it, and it is added to the text that is there (`Enter Bot Name` included).
    name_focus: bool,
    name_edited: bool,
    designs: PathBuf,
    custom_textures: PathBuf,
    status: String,
    panel: Panel,
    revision: u64,
    preview: Option<(u64, Vec<u8>, u32, u32)>,
    preview_camera: Option<PreviewCamera>,
    preview_view_key: String,
    /// The control cell and the time of the last click on one, for the double click that opens it.
    last_control_click: Option<(std::time::Instant, [i32; 2])>,
    /// The practice arena of the `Test Robot` tab while that tab is on screen.
    arena: Option<crate::lab_arena::LabArena>,
    /// The `Obstacles` radio the player last picked (`none` after a visit starts over).
    arena_obstacle: &'static str,
    /// The lab camera of each tab.
    lab_cams: std::collections::HashMap<String, LabCam>,
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
    team_new: Option<(usize, Option<usize>)>,
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
        Self::open_with(game, start, size, false)
    }

    /// The lab as the original opens it on an empty inventory cell (`FUN_00439650`): a blank
    /// robot with no chassis shape, Components, Wiring, Paint Shop and Test Robot dim, the name
    /// box reading `Enter Bot Name` (docs/36 section 9.27).
    pub fn open_blank(game: &Path, size: [f32; 2]) -> Result<Builder, String> {
        Self::open_with(game, None, size, true)
    }

    fn open_with(
        game: &Path,
        start: Option<&Path>,
        size: [f32; 2],
        blank: bool,
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
                    vec![
                        ra2fmt::saves::resolve(&game, path),
                        ra2fmt::saves::resolve(&game, Path::new(DESIGNS).join(path)),
                    ]
                };
                let path = candidates
                    .into_iter()
                    .find(|p| p.is_file())
                    .or_else(|| {
                        let mut named = Path::new(DESIGNS).join(path);
                        if named.extension().is_none() {
                            named.set_extension("bot");
                        }
                        let named = ra2fmt::saves::resolve(&game, named);
                        named.is_file().then_some(named)
                    })
                    .ok_or_else(|| format!("{} is not a .bot file", path.display()))?;
                Build::load(&library, &path).map_err(|e| format!("{}: {e}", path.display()))?
            }
            None if blank => Build::blank("Enter Bot Name"),
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
            tip_roll: (0, u32::MAX),
            tip_screen: None,
            tip_written: None,
            alert: None,
            alert_left: 0.0,
            tip_set: None,
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
            dialog: None,
            wire_control: None,
            wire_component: None,
            palette_scroll: 0,
            selected_texture: None,
            paint_mode: PaintMode::Fill,
            paint_color: PaintColor::default(),
            selected_decal: None,
            selected_group: None,
            decal_at: [0.5, 0.5],
            decal_scale: 0.5,
            draw_style: DrawStyle::Freeform,
            draw_centre: None,
            draw_point: None,
            point_drag: false,
            adjust_armed: false,
            outline_closed: false,
            draw_snap: true,
            shape_radius: 300,
            shape_length: 600,
            shape_width: 400,
            name_buffer,
            name_focus: false,
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
            preview_view_key: String::new(),
            last_control_click: None,
            arena: None,
            arena_obstacle: "none",
            lab_cams: std::collections::HashMap::new(),
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

    /// Whether the `Test Robot` tab's practice arena is on screen.
    pub fn arena_active(&self) -> bool {
        self.view_only && self.arena.is_some()
    }

    /// The practice arena exists exactly while the `Test Robot` tab is on screen: entering the tab
    /// builds a new one (live: the clock starts at 0 and the robot on its start point) and leaving
    /// drops it.
    fn sync_arena(&mut self) {
        let wanted = self.view_only && self.tab() == Some("test robot button");
        if !wanted {
            if self.arena.take().is_some() {
                // The temporary copy of the robot the arena loaded.
                let _ = std::fs::remove_file(
                    std::env::temp_dir().join(format!("ora-lab-test-{}.bot", std::process::id())),
                );
            }
            self.arena_obstacle = "none";
            return;
        }
        if self.arena.is_some() {
            return;
        }
        let path = std::env::temp_dir().join(format!("ora-lab-test-{}.bot", std::process::id()));
        let saved = match std::env::var_os("ORA_LAB_BOT") {
            Some(own) => std::fs::copy(own, &path).map(|_| ()),
            None => self.build.save_to(&path),
        };
        match saved {
            Ok(()) => match crate::lab_arena::LabArena::new(&self.game, &path, self.arena_obstacle)
            {
                Ok(arena) => self.arena = Some(arena),
                Err(error) => self.status = format!("test robot: {error}"),
            },
            Err(error) => self.status = format!("test robot: {}: {error}", path.display()),
        }
    }

    /// The `status group` of `testarena.gib` shows the robot's name and meters (the match HUD's own
    /// widgets); the points read `N/A`.
    fn bind_arena_hud(&mut self) {
        let Some(arena) = &self.arena else {
            return;
        };
        let report = arena.matched.report();
        let Some(robot) = report.robots.first() else {
            return;
        };
        let Some(lab) = self.shell.lab_mut() else {
            return;
        };
        for block in lab.document.blocks.iter_mut() {
            let ra2fmt::gib::Block::Widget(root) = block else {
                continue;
            };
            if let Some(group) = workshop::find_mut(root, "status group") {
                crate::hud::bind_status(group, robot, "N/A");
            }
        }
    }

    /// One tick of wall time: the practice arena runs its fixed steps.
    pub fn advance(&mut self, seconds: f32) {
        if self.alert.is_some() {
            self.alert_left -= seconds;
            if self.alert_left <= 0.0 {
                self.alert = None;
            }
        }
        // `FUN_0042c850` runs every frame of a drag: a part still refused over the baseplate keeps
        // the alert at two seconds (live: it stayed up for the 5 s the mouse sat still).
        if let Some(active) = &self.drag {
            if let (Some(r), Some(c)) = (&active.refusal, &active.candidate) {
                if c.parent == 0 {
                    self.alert = Some(r.message().to_string());
                    self.alert_left = 2.0;
                }
            }
        }
        self.sync_arena();
        if let Some(arena) = &mut self.arena {
            arena.advance(seconds);
        }
    }

    /// The practice arena's sounds since the last call, with the ears to hear them from.
    pub fn take_arena_cues(
        &mut self,
    ) -> Option<(Vec<crate::match_sounds::SoundCue>, ra2_audio::Listener)> {
        let arena = self.arena.as_mut()?;
        Some((arena.take_cues(), arena.audio_listener()))
    }

    /// The robot's held keys for the practice arena.
    pub fn set_drive(&mut self, input: ra2sim::DriveInput) {
        if let Some(arena) = &mut self.arena {
            arena.set_drive(input);
        }
    }

    pub fn drive(&self) -> ra2sim::DriveInput {
        self.arena.as_ref().map(|a| a.drive()).unwrap_or_default()
    }

    /// The practice arena's own 3D frame at `size`, for scripted runs.
    pub fn arena_frame(&mut self, size: [u32; 2]) -> Result<Vec<u8>, String> {
        let arena = self
            .arena
            .as_mut()
            .ok_or_else(|| "no practice arena".to_string())?;
        arena.frame(&self.renderer, size)
    }

    /// The practice arena, for scripted runs.
    pub fn arena_mut(&mut self) -> Option<&mut crate::lab_arena::LabArena> {
        self.arena.as_mut()
    }

    fn lab_cam(&self) -> LabCam {
        let key = self.tab().unwrap_or("").to_string();
        self.lab_cams.get(&key).copied().unwrap_or(LabCam::START)
    }

    /// The distance limits of the tab's camera: 2..11 on Overview and Chassis, 4..9 elsewhere.
    fn lab_cam_limits(&self) -> (f32, f32) {
        match self.tab() {
            Some("overview button") | Some("chassis button") | None => (2.0, 11.0),
            _ => (4.0, 9.0),
        }
    }

    /// Where the selected control's icon floats: over the `(x, z)` of the robot's control board
    /// (the first `ControlBoard` part), or the chassis origin without one.
    fn wire_anchor(&self, world: &[Mat4]) -> [f32; 2] {
        let comps = self.build.components();
        for (i, c) in comps.iter().enumerate().skip(1) {
            if c.class.eq_ignore_ascii_case("ControlBoard") {
                if let Some(m) = world.get(i) {
                    let p = crate::lab_view::origin_of(*m);
                    return [p.x, p.z];
                }
            }
        }
        [0.0, 0.0]
    }

    /// The mouse wheel over the 3D view: `notches` of 120 units, 0.01 m each, wheel up closer.
    pub fn lab_wheel(&mut self, notches: f32) {
        let (min, max) = self.lab_cam_limits();
        let mut cam = self.lab_cam();
        cam.dist = (cam.dist - notches * 1.2).clamp(min, max);
        let key = self.tab().unwrap_or("").to_string();
        self.lab_cams.insert(key, cam);
        self.preview = None;
    }

    /// The middle button dragged over the 3D view by `dx`, `dy` mouse counts: 0.01 rad each, the
    /// pitch held to +-pi/2.
    pub fn lab_orbit(&mut self, dx: f32, dy: f32) {
        let mut cam = self.lab_cam();
        cam.yaw += dx * 0.01;
        cam.pitch = (cam.pitch - dy * 0.01).clamp(-FRAC_PI_2, FRAC_PI_2);
        let key = self.tab().unwrap_or("").to_string();
        self.lab_cams.insert(key, cam);
        self.preview = None;
    }

    /// The pointer is over the 3D view's rectangle.
    pub fn over_view(&self, x: f32, y: f32) -> bool {
        self.panel.preview.contains_point(x, y)
    }

    /// Set the lab camera directly (scripted comparisons).
    pub fn set_lab_cam(&mut self, cam: LabCam) {
        let key = self.tab().unwrap_or("").to_string();
        self.lab_cams.insert(key, cam);
        self.preview = None;
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
        if self.dialog.is_some() {
            return self.dialog_click(x, y);
        }
        if self.view_only {
            self.name_focus = self.tab() == Some("overview button")
                && self.hit_names(x, y).iter().any(|n| n == "bot name textbox");
        }
        if self.drag.is_some() && !self.panel.preview.contains_point(x, y) {
            // Any other click ends the drag, as the category and tab buttons
            // do (`FUN_0042c280`).
            self.cancel_drag();
        }
        if self.paint_pick(x, y, true) {
            return Ok(());
        }
        if self.panel.preview.contains_point(x, y) {
            if let Some(result) = self.preview_click(x, y) {
                return result;
            }
        }
        if self.tab() == Some("wiring button") {
            let cell = self.hit_names(x, y).iter().find_map(|n| {
                let rest = n.strip_prefix("m9:ctl:")?;
                let (c, r) = rest.split_once(':')?;
                Some([c.parse().ok()?, r.parse().ok()?])
            });
            match cell {
                Some(cell) => {
                    let now = std::time::Instant::now();
                    let double = self.last_control_click.is_some_and(|(t, c)| {
                        c == cell && now.duration_since(t) < std::time::Duration::from_millis(500)
                    });
                    self.last_control_click = Some((now, cell));
                    if double {
                        self.last_control_click = None;
                        self.open_edit_dialog(cell);
                        return Ok(());
                    }
                }
                None => self.last_control_click = None,
            }
        }
        if let Some(action) = self.panel_action(x, y) {
            return self.apply(&action);
        }
        if self.on_point_screen()
            && self
                .editing_grid()
                .is_some_and(|(frame, _)| frame.contains_point(x, y))
        {
            // Chassis step 2: a press in the editing area takes the top point under the cursor.
            self.adjust_press(x, y);
            return Ok(());
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
        if self.disabled_control_at(x, y) {
            // A dim tab or the dim Armor button takes no click (`FUN_005ec4e0(0)`).
            return Ok(());
        }
        let category = self.category_click(x, y);
        let was_step_one = self.tab() == Some("chassis button") && self.structure_step() == 1;
        // The `Test Robot` tab's own controls: an obstacle radio or RESET.
        let arena_pick: Option<Option<&'static str>> = if self.arena.is_some() {
            let names = self.hit_names(x, y);
            if names.iter().any(|n| n == "reset button") {
                Some(None)
            } else {
                names
                    .iter()
                    .find_map(|n| crate::lab_arena::obstacle_for(n))
                    .map(Some)
            }
        } else {
            None
        };
        self.shell.click(x, y)?;
        match (arena_pick, self.arena.as_mut()) {
            (Some(Some(obstacle)), Some(arena)) => {
                self.arena_obstacle = obstacle;
                if let Err(error) = arena.set_obstacle(obstacle) {
                    self.status = format!("test robot: {error}");
                }
            }
            (Some(None), Some(arena)) => {
                if let Err(error) = arena.reset() {
                    self.status = format!("test robot: {error}");
                }
            }
            _ => {}
        }
        if category {
            self.palette_scroll = 0;
        }
        if was_step_one && self.tab() == Some("chassis button") && self.structure_step() == 2 {
            // Step 2 shows the baseplate extruded (`Adjust Top Outline`): the mesh is made here,
            // afresh every time (live `botlab-adjust-step2-reextruded`), and the height slider
            // starts at the default height.
            if let Err(e) = self.build.begin_top_adjust() {
                self.status = e.to_string();
            }
            self.point_drag = false;
            self.draw_point = None;
            self.sync_height_thumb(ra2ui::workshop::extrude::fraction_for_height(
                ra2ui::workshop::extrude::DEFAULT_HEIGHT,
            ));
        }
        self.take_style_pick();
        self.refresh()
    }

    /// Whether the point is on a control the lab has dimmed: the Components, Wiring, Paint Shop and
    /// Test Robot tabs (`FUN_00439650` and the chassis' finish enable them) and the Armor button.
    fn disabled_control_at(&self, x: f32, y: f32) -> bool {
        let placed = self.shell.placements();
        let Some(path) = ra2ui::input::hit(&placed, x, y) else {
            return false;
        };
        let chain = chain(&placed, &path);
        chain.iter().any(|p| {
            matches!(
                p.widget.name(),
                "components button"
                    | "wiring button"
                    | "paintshop button"
                    | "test robot button"
                    | "armor button"
            ) && (ra2ui::input::checkbox_dimmed(p.widget) || ra2ui::input::button_dimmed(p.widget))
        })
    }

    /// Dim the tabs the original dims: a blank robot has only Overview and Chassis; the Wiring
    /// tab waits for a control board; the Armor button waits for a chassis shape.
    fn refresh_tabs(&mut self) {
        let blank = self.build.is_blank();
        let wiring = !blank && self.build.has_control_board();
        let Some(lab) = self.shell.lab_mut() else {
            return;
        };
        for block in lab.document.blocks.iter_mut() {
            let ra2fmt::gib::Block::Widget(root) = block else {
                continue;
            };
            for (name, on) in [
                ("components button", !blank),
                ("wiring button", wiring),
                ("paintshop button", !blank),
                ("test robot button", !blank),
            ] {
                if let Some(w) = workshop::find_mut(root, name) {
                    ra2ui::input::set_checkbox_enabled(w, on);
                }
            }
            if let Some(w) = workshop::find_mut(root, "armor button") {
                ra2ui::input::set_button_enabled(w, !blank);
            }
        }
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
        let grid = blueprint_grid(area.frame);
        Ok([
            grid.x + point[0] as f32 / 1000.0 * grid.w,
            grid.y + point[1] as f32 / 1000.0 * grid.h,
        ])
    }
    fn preview_click(&mut self, x: f32, y: f32) -> Option<Result<(), String>> {
        let tab = self.shell.tab()?.to_string();
        self.ensure_preview();
        match tab.as_str() {
            "components button" => Some(self.place_at_preview(x, y)),
            "paintshop button" => Some(self.paint_at_preview(x, y)),
            "wiring button" => Some(self.wire_at_preview(x, y)),
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
            self.tip_set = Some(self.status.clone());
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
        self.tip_set = Some(placement::TIP_DRAG.into());
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
            // Live (`botlab` capture after a click on nothing): the tip line is left empty.
            self.tip_set = Some(String::new());
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
        if active.refusal.is_none() {
            self.tip_set = Some(placement::TIP_DRAG.into());
        }
        // Only the baseplate branch of `FUN_0042c850` raises the alert (0x42cf46, 0x42d11a); a
        // part snapping onto another part's smart zone does not.
        if let (Some(r), Some(c)) = (&active.refusal, &active.candidate) {
            if c.parent == 0 {
                self.show_alert(r.message());
            }
        }
        self.drag = Some(active);
        self.preview = None;
        self.refresh_panel();
    }

    /// The mouse moved. With a part on the drag the part follows it; SHIFT turns
    /// it and CTRL raises it by the dominant axis of the movement
    /// (`0x42b6b4`: the larger of |dx| and |dy|, signed).
    pub fn mouse_moved(&mut self, x: f32, y: f32) {
        let last = self.cursor.replace([x, y]);
        if self.dialog.is_none() && self.on_point_screen() {
            self.adjust_move(x, y);
        }
        if self.paint_color.drag.is_some() {
            self.paint_pick(x, y, false);
        }
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
        // Scripted drops aim through the preview: look straight down for the gesture.
        let saved = self.lab_cam();
        self.set_lab_cam(LabCam {
            pitch: -FRAC_PI_2,
            yaw: 0.0,
            dist: saved.dist,
        });
        let result = self.place_component_aimed(file, over);
        self.set_lab_cam(saved);
        result
    }

    fn place_component_aimed(&mut self, file: &str, over: PlaceOn) -> Result<usize, String> {
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
        // The scan needs the baseplate in view: look straight down for its length (the lab camera
        // starts level, and the player turns it with the middle button).
        let saved = self.lab_cam();
        self.set_lab_cam(LabCam {
            pitch: -FRAC_PI_2,
            yaw: 0.0,
            dist: saved.dist,
        });
        let result = self.place_component_scan(file);
        self.set_lab_cam(saved);
        result
    }

    fn place_component_scan(&mut self, file: &str) -> Result<usize, String> {
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
                // The player-facing Paint Shop is Color Fill: the swatch's colour, no texture.
                if self.view_only && self.selected_texture.is_none() {
                    let texture = self.paint_solid();
                    self.build
                        .paint_mut()
                        .fill_group(group, &texture)
                        .map_err(|e| e.to_string())?;
                    self.status = format!("color fill face group {group}");
                    return self.refresh();
                }
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
        let grid = blueprint_grid(area.frame);
        let u = ((x - grid.x) / grid.w * 1000.0).round().clamp(0.0, 1000.0) as i32;
        let v = ((y - grid.y) / grid.h * 1000.0).round().clamp(0.0, 1000.0) as i32;
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
            ((grid.frame.w / stride).floor() as usize).max(1),
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
        if self.dialog.is_some() {
            self.dialog_type(c);
            return;
        }
        if !(c.is_ascii_graphic() || c == ' ') {
            return;
        }
        if self.view_only && (self.tab() != Some("overview button") || !self.name_focus) {
            // The name box lives on the Overview tab and takes keys after a click on it.
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
        if !self.name_edited && !self.view_only {
            self.name_buffer.clear();
        }
        self.name_edited = true;
        self.name_buffer.push(c);
        self.refresh_panel();
        self.refresh_overview();
    }

    /// The text the Overview's name box holds.
    pub fn name_text(&self) -> &str {
        &self.name_buffer
    }

    /// Backspace in the filename box.
    pub fn backspace(&mut self) {
        if self.dialog.is_some() {
            self.dialog_backspace();
            return;
        }
        if self.view_only && (self.tab() != Some("overview button") || !self.name_focus) {
            return;
        }
        self.name_buffer.pop();
        self.name_edited = true;
        self.refresh_panel();
        self.refresh_overview();
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
    pub fn set_team_new(&mut self, team: usize, slot: Option<usize>) {
        self.team_new = Some((team, slot));
        if slot.is_some() {
            // The name box of a new robot reads `Enter Bot Name` (live `workshop-newbot`; `FUN_00439650`).
            self.name_buffer = "Enter Bot Name".to_string();
            self.refresh_panel();
        }
    }

    pub fn save_to_team(&mut self) -> Result<Option<PathBuf>, String> {
        if self.team_target.is_none() {
            if let Some((team, slot)) = self.team_new {
                // A new robot in a selected `New Robot` cell is saved whatever was done (live: Back to HQ
                // with nothing built wrote `Bot<cell>.bot`, 127 bytes, named `Enter Bot Name`, and the
                // cell joined the roster); without a cell it only joins when something was built.
                if slot.is_some() || self.build.can_undo() || self.name_edited {
                    let path = ra2ui::teams::add_robot(&self.game, team, slot)?;
                    self.team_target = Some(path);
                }
            }
        }
        let Some(path) = self.team_target.clone() else {
            return Ok(None);
        };
        let name = if self.name_buffer.trim().is_empty() {
            "Enter Bot Name".to_string()
        } else {
            self.design_name()
        };
        self.build.set_name(&name);
        let path = ra2fmt::saves::prepare_write(&self.game, &path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
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
        let path =
            ra2fmt::saves::prepare_write(&self.game, &self.designs.join(format!("{name}.bot")))
                .map_err(|e| format!("{}: {e}", self.designs.display()))?;
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
        let path = self.read_path(&path);
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

    /// A path under the install, read through the save directory.
    fn read_path(&self, path: &Path) -> PathBuf {
        match path.strip_prefix(&self.game) {
            Ok(rel) => ra2fmt::saves::resolve(&self.game, rel),
            Err(_) => path.to_path_buf(),
        }
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
            ra2fmt::saves::copy(&self.game, &saved, &target)
                .map_err(|e| format!("{}: {e}", target.display()))?;
        }
        self.status = format!("exported {}", target.display());
        Ok(target)
    }

    /// The import half of `importbot.gib`: the `.bot` files in `Robot Designs/`.
    pub fn designs(&self) -> Vec<String> {
        list_folder(&self.game, DESIGNS, &["bot"])
    }

    /// `import_texture.gib`'s list: the player's own images in `Custom Textures/`.
    pub fn custom_texture_names(&self) -> Vec<String> {
        list_folder(
            &self.game,
            CUSTOM_TEXTURES,
            &["bmp", "tga", "jpg", "jpeg", "png"],
        )
    }

    /// Export the current chassis skin into `Custom Textures/` — the shipped
    /// export screen's "export the current texture" checkbox.
    pub fn export_texture(&mut self, name: &str) -> Result<PathBuf, String> {
        let mut target = self.custom_textures.join(name);
        if target.extension().is_none() {
            target.set_extension("tga");
        }
        let skin = self.build.paint().skin().ok_or_else(|| {
            "the chassis has no skin to export (a bot without a texture)".to_string()
        })?;
        let target = ra2fmt::saves::write(&self.game, &target, skin.to_tga())
            .map_err(|e| format!("{}: {e}", target.display()))?;
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
                path: ra2fmt::saves::resolve(&self.game, Path::new(CUSTOM_TEXTURES).join(name)),
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
            self.outline_closed = false;
            self.status = format!("cleared the {} outline", which.label());
            return self.refresh();
        }
        if action == "m9:finished" {
            // `Finished` (live): the structure design is done, the chassis page is shown again, and
            // an outline of three points or more has been extruded; with fewer the robot stays blank.
            let shaped = self.build.extrude_chassis().map_err(|e| e.to_string())?;
            self.status = if shaped {
                "chassis finished".into()
            } else {
                "the baseplate outline needs three points".into()
            };
            self.draw_point = None;
            self.shell.click_named("chassis button")?;
            return self.refresh();
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
            if self.view_only {
                self.open_props_dialog(cell);
            } else {
                self.place_control(cell);
            }
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
            self.preview = None;
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
            if self.build.board().control_at(cell).is_none() {
                self.status = "there is no control there".into();
                self.refresh_panel();
                return Ok(());
            }
            // `Delete this input control?`: a modal `Wiring` box (the body is blank live).
            let Ok(mut gib) = workshop::load(&self.ui, "common/messagebox.gib") else {
                return self.confirm_delete(cell);
            };
            ra2_shell::yes_no_box(&mut gib, "Wiring");
            self.dialog = Some(LabDialog {
                gib,
                kind: DialogKind::ConfirmDelete { cell },
                focus: Focus::None,
            });
            return Ok(());
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
            "m9:paintall" => {
                let texture = self.paint_solid();
                self.build
                    .paint_mut()
                    .fill_all(&texture)
                    .map_err(|e| e.to_string())?;
                self.status = "painted the entire chassis".into();
                return self.refresh();
            }
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

    /// Whether chassis step 2 (`Adjust Top Outline`) is the screen.
    fn on_adjust_screen(&self) -> bool {
        self.tab() == Some("chassis button")
            && self.shell.lab().is_some()
            && self.structure_step() == 2
    }

    /// Whether the editing area takes point drags: chassis step 2's top outline, or step 1's
    /// baseplate once its outline is closed (live `botlab-step1-drag-*`: the same widget, the same
    /// rules and latch, the dragged point is a baseplate point).
    fn on_point_screen(&self) -> bool {
        self.on_adjust_screen()
            || (self.tab() == Some("chassis button")
                && self.shell.lab().is_some()
                && self.structure_step() == 1
                && self.outline_closed
                && self.draw_style == DrawStyle::Freeform
                && self.build.chassis().is_some_and(|c| c.bottom.len() >= 3))
    }

    /// The step's `Editing Area` frame and its blueprint grid.
    fn editing_grid(&self) -> Option<(Frame, Frame)> {
        let placed = self.shell.placements();
        let area = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "Editing Area")?;
        Some((area.frame, blueprint_grid(area.frame)))
    }

    /// The draggable outline: step 2's top points, step 1's baseplate.
    fn editable_points(&self) -> Option<&[[i32; 2]]> {
        let chassis = self.build.chassis()?;
        Some(if self.structure_step() == 2 {
            &chassis.top
        } else {
            &chassis.bottom
        })
    }

    /// The point whose selection image is under a screen point. The image is a 16 px square
    /// (live: a point at 129,238 is taken by a cursor from x 121 to 136 and y 232 to 245).
    fn top_point_at(&self, x: f32, y: f32) -> Option<usize> {
        let (_, grid) = self.editing_grid()?;
        self.editable_points()?.iter().position(|p| {
            let px = grid.x + p[0] as f32 / 1000.0 * grid.w;
            let py = grid.y + p[1] as f32 / 1000.0 * grid.h;
            (px - 8.0..px + 8.0).contains(&x.floor()) && (py - 8.0..py + 8.0).contains(&y.floor())
        })
    }

    /// The outline point a screen point stands for: blueprint units, snapped to the grid when the
    /// checkbox says so (`FUN_00479260`).
    fn outline_point_at(&self, x: f32, y: f32) -> Option<[i32; 2]> {
        let (_, grid) = self.editing_grid()?;
        let u = ((x.floor() - grid.x) / grid.w * 1000.0).round() as i32;
        let v = ((y.floor() - grid.y) / grid.h * 1000.0).round() as i32;
        let point = [u, v];
        Some(if self.draw_snap {
            chassis::snap(point)
        } else {
            point
        })
    }

    /// The mouse goes down in the editing area of step 2: a drag starts on the point under it.
    fn adjust_press(&mut self, x: f32, y: f32) {
        if let Some(index) = self.top_point_at(x, y) {
            self.draw_point = Some(index);
            self.point_drag = true;
        }
    }

    /// The mouse moves on step 2. With a point held the point follows it (the editing area's
    /// validator, then the lab's bend test); without, the point under the cursor is the one with
    /// the bright ring. A move outside the editing area does not reach it (the ring stays).
    fn adjust_move(&mut self, x: f32, y: f32) {
        let inside = self
            .editing_grid()
            .is_some_and(|(frame, _)| frame.contains_point(x, y));
        if !inside {
            return;
        }
        let Some(index) = self.draw_point.filter(|_| self.point_drag) else {
            self.draw_point = self.top_point_at(x, y);
            return;
        };
        let Some(point) = self.outline_point_at(x, y) else {
            return;
        };
        let moved = if self.structure_step() == 2 {
            self.build.adjust_top_point(index, point)
        } else {
            self.build.adjust_bottom_point(index, point)
        };
        match moved {
            Ok(moved) => {
                if moved.validated() {
                    self.adjust_armed = true;
                }
                match moved {
                    TopMove::Moved => {
                        self.status =
                            format!("moved top point {index} to {}, {}", point[0], point[1]);
                        let _ = self.refresh();
                    }
                    TopMove::Refused(rule) => {
                        self.status = format!("top point {index} stays: {}", rule.text());
                        if self.adjust_armed {
                            self.show_alert(rule.text());
                        }
                    }
                    TopMove::Bend => {
                        self.status = format!("top point {index} stays: {BEND_TEXT}");
                        self.show_alert(BEND_TEXT);
                    }
                    TopMove::Unchanged => {}
                }
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// A click on a chassis step's `Editing Area`.
    fn draw_chassis(&mut self, point: [i32; 2]) -> Result<(), String> {
        let step = self.structure_step();
        if step == 2 {
            // Step 2 is a drag of the top points (`adjust_press` / `adjust_move`), not a drawing click.
            return Ok(());
        }
        match self.draw_style {
            DrawStyle::Freeform => {
                // A click on the first point of an outline of three or more closes it (live: the
                // fourth click of a square on its first corner leaves four points); a closed
                // outline takes no more points.
                if self.outline_closed {
                    return Ok(());
                }
                if let Some(first) = self.build.chassis().and_then(|c| c.bottom.first().copied()) {
                    let near = (first[0] - point[0]).pow(2) + (first[1] - point[1]).pow(2);
                    if self.build.chassis().is_some_and(|c| c.bottom.len() >= 3) && near < 60 * 60 {
                        self.outline_closed = true;
                        self.status = "baseplate outline closed".into();
                        self.refresh_panel();
                        return Ok(());
                    }
                }
                // The editing area's rules judge the new point first (`FUN_004a10d0`); a refusal adds
                // nothing and shows its alert only while the latch `DAT_006f52e0` is set. Live: a
                // point on an edge, one that makes the outline cross itself and one that makes a
                // corner too sharp were each refused with no alert and the ring red (`botlab-step1-*`).
                if let Some(chassis) = self.build.chassis() {
                    let snapped = if self.draw_snap {
                        chassis::snap(point)
                    } else {
                        point
                    };
                    if let Err(rule) = workshop::adjust::check_add(&chassis.bottom, snapped) {
                        self.status = rule.text().into();
                        if self.adjust_armed {
                            self.show_alert(rule.text());
                        }
                        self.refresh_panel();
                        return Ok(());
                    }
                }
                match self
                    .build
                    .add_chassis_outline_point(Outline::Bottom, point, self.draw_snap)
                {
                    Ok(index) => {
                        // `FUN_004a0e00`: a point added to the list clears the alert latch.
                        self.adjust_armed = false;
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

    /// Put the height slider's thumb where `fraction` (0 at the bottom of the track) says.
    fn sync_height_thumb(&mut self, fraction: f32) {
        let Some(lab) = self.shell.lab_mut() else {
            return;
        };
        for block in lab.document.blocks.iter_mut() {
            let ra2fmt::gib::Block::Widget(root) = block else {
                continue;
            };
            let Some(slider) = workshop::find_mut(root, "height slider") else {
                continue;
            };
            let track = slider.size().map(|s| s.second.amount).unwrap_or(121.0);
            let Some(thumb) = slider
                .children
                .iter_mut()
                .find(|c| c.kind == WidgetKind::Image)
            else {
                continue;
            };
            let thumb_h = thumb.size().map(|s| s.second.amount).unwrap_or(16.0);
            let y = ((1.0 - fraction.clamp(0.0, 1.0)) * (track - thumb_h).max(0.0)).max(0.0);
            ra2ui::input::set_field(thumb, "Position", &format!("0 0 {y} 0"));
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
                // Live: the thumb at the top gives 1.32 m, at the bottom 0.436364 (docs/36 section 9.27).
                let height = ra2ui::workshop::extrude::height_for_fraction(fraction);
                self.build
                    .set_chassis_height(height)
                    .map_err(|e| e.to_string())?;
                self.status = format!("chassis height {height:.2} m");
                self.sync_height_thumb(fraction);
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

    /// The names of the widgets under a point, outermost first.
    fn hit_names(&self, x: f32, y: f32) -> Vec<String> {
        let placed = self.shell.placements();
        ra2ui::input::hit(&placed, x, y)
            .map(|p| {
                chain(&placed, &p)
                    .iter()
                    .map(|n| n.widget.name().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The widget names under a point, outermost first (`hit` and `hit_control`), for scripted
    /// diagnosis.
    pub fn debug_hit(&self, x: f32, y: f32) -> Vec<String> {
        let placed = self.shell.placements();
        let mut out = Vec::new();
        for control_only in [false, true] {
            let path = if control_only {
                ra2ui::input::hit_control(&placed, x, y)
            } else {
                ra2ui::input::hit(&placed, x, y)
            };
            let names: Vec<String> = path
                .map(|p| {
                    chain(&placed, &p)
                        .iter()
                        .map(|n| n.widget.name().to_string())
                        .collect()
                })
                .unwrap_or_default();
            out.push(format!(
                "{}: {}",
                if control_only { "control" } else { "hit" },
                names.join(" > ")
            ));
        }
        out
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
        if self.tab() == Some("paintshop button") {
            if chain.iter().any(|p| p.widget.name() == "paint entire") {
                return Some("m9:paintall".into());
            }
            if chain.iter().any(|p| p.widget.name() == "undo button") {
                return Some("m9:undo".into());
            }
        }
        if self.tab() == Some("wiring button") {
            // The controller's picture is laid over the strip of icons above it; the control
            // under the point is what is clicked.
            let plain = chain.clone();
            let chain = match ra2ui::input::hit_control(&placed, x, y) {
                Some(path) => self::chain(&placed, &path),
                None => chain.clone(),
            };
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
            // The control's picture is not a "control" widget, so a click on it is found in the
            // plain hit chain (live: the whole cell selects).
            if let Some(name) = chain
                .iter()
                .chain(plain.iter())
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
        self.refresh_tabs();
        self.refresh_effects();
        self.refresh_palette();
        self.refresh_comp_preview();
        self.refresh_armour();
        self.refresh_controls();
        self.refresh_panel();
        Ok(())
    }

    /// Show an orange alert for two seconds (`FUN_0043e7c0`); an empty one puts the tip back.
    pub fn show_alert(&mut self, text: &str) {
        if text.is_empty() {
            self.alert = None;
        } else {
            self.alert = Some(text.to_string());
            self.alert_left = 2.0;
        }
    }

    /// The alert on show, if any.
    pub fn alert(&self) -> Option<&str> {
        self.alert.as_deref()
    }

    /// The tip line's text as the lab shows it now (an alert is not part of it).
    pub fn tip_line(&mut self) -> String {
        self.tip_text()
    }

    /// The tip line at the bottom of the lab (`FUN_0043e900`): the screen's `Info Display` carries
    /// the `tip.bmp` icon and one line of `fg_mcn14` text in `.5 .6 1`, centred as a pair. The text
    /// follows the state: no chassis yet, no parts, no control board, no wiring, else one of four
    /// pointers at the other tabs, chosen at random (and again whenever the screen changes) among
    /// those that do not name the tab the player is on.
    fn tip_text(&mut self) -> String {
        const PAINT: &str = "Click the Paint Shop button to customize your robot's appearance.";
        const COMPONENTS: &str = "Click the Components button to continue working on your robot.";
        const CHASSIS: &str = "Click the Chassis button to add special armor to your robot.";
        const TEST: &str = "Click the Test Robot button to try out your robot.";
        let adjust = self.on_adjust_screen();
        let Some(lab) = self.shell.lab_mut() else {
            return String::new();
        };
        // The original's `+0x1d8`: 1 overview, 2 chassis, 3 / 4 the two structure screens, 5 armour,
        // 6 components, 7 wiring, 8 appearance, 9 test robot.
        let screen = match (lab.tab, lab.step) {
            ("overview button", _) => 1,
            ("chassis button", Some("structure design button")) => {
                if adjust {
                    4
                } else {
                    3
                }
            }
            ("chassis button", Some(_)) => 5,
            ("chassis button", None) => 2,
            ("components button", _) => 6,
            ("wiring button", _) => 7,
            ("paintshop button", _) => 8,
            _ => 9,
        };
        let key = (lab.tab.to_string(), lab.step.map(str::to_string));
        if self.tip_screen.as_ref() != Some(&key) {
            self.tip_screen = Some(key);
            // A new screen: the pick is drawn again (below, when it is needed).
            self.tip_roll.1 = u32::MAX;
            self.tip_set = None;
        }
        if let Some(set) = &self.tip_set {
            return set.clone();
        }
        let blank = self.build.is_blank();
        let parts = self.build.components().len();
        if blank {
            return match screen {
                2 => "Click the Structure Design button to build a chassis",
                3 => "Click on the grid to add points to your chassis",
                _ => "Click the Chassis button to start your robot",
            }
            .to_string();
        }
        if screen == 4 {
            return "Click and drag points to adjust the sides of your chassis".into();
        }
        if parts <= 1 && screen != 6 {
            return "Click the Components button to add parts to your robot".into();
        }
        if !self.build.has_control_board() && screen == 6 {
            return "Click the Control Board (under Power) to add it first".into();
        }
        if self.build.board().controls.is_empty() {
            return "Click the Wiring button to set up keyboard or joystick controls for your robot"
                .into();
        }
        // `0x43eba1`: `FUN_00419190(4)` from the engine's generator, drawn again until the pick is
        // not about the screen shown. The state is the session's clock (`GetTickCount`, 0x4e456a)
        // advanced by whatever drew before, so which of the four shows is not reproducible.
        let about_here = |pick: usize| match pick {
            0 => screen == 8,
            1 => screen == 6,
            2 => screen == 2 || screen == 5,
            _ => screen == 9,
        };
        if self.tip_roll.1 == u32::MAX {
            if self.tip_roll.0 == 0 {
                // `ORA_TIP_SEED` pins the state for the side-by-side harnesses.
                self.tip_roll.0 = std::env::var("ORA_TIP_SEED")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as u32)
                            .unwrap_or(1)
                    });
            }
            let mut lcg = ra2sim::lcg::Lcg(self.tip_roll.0);
            let mut pick = lcg.range(4);
            while about_here(pick) {
                pick = lcg.range(4);
            }
            self.tip_roll = (lcg.0, pick as u32);
        }
        match self.tip_roll.1 {
            0 => PAINT,
            1 => COMPONENTS,
            2 => CHASSIS,
            _ => TEST,
        }
        .to_string()
    }

    /// Put the tip into the lab's `Info Display`: an icon and a text widget, centred as a pair
    /// (the original's markup `<image filename "UI\\botlab\\tip.bmp"><text font fg_mcn14 color .5 .6 1 1
    /// align 2>`, icon 19 x 17 at the line's top, three pixels before the text).
    fn refresh_tip(&mut self) {
        let alert = self.alert.clone();
        let text = match &alert {
            Some(text) => text.clone(),
            None => self.tip_text(),
        };
        let key = format!(
            "{text}|{}|{}x{}",
            alert.is_some(),
            self.size[0],
            self.size[1]
        );
        if self.tip_written.as_deref() == Some(key.as_str()) {
            return;
        }
        let width = self
            .shell
            .text_extent("fg_mcn14", &text)
            .map(|(w, _)| w)
            .unwrap_or(0.0);
        // The widgets hang on the `Glyph` the box dresses, whose frame is the whole bottom pane; the
        // box itself (80 % wide, 18 tall, 2 px down) is where the pair is centred.
        let (row, pane) = {
            let placed = self.shell.placements();
            let info = placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == "Info Display")
                .map(|p| p.frame);
            // The pane the split gives the box's widgets: `lowersplit`'s second slot.
            let pane = placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == "lowersplit")
                .and_then(|split| split.children.get(1))
                .map(|slot| slot.frame);
            (info, pane)
        };
        let (Some(row), Some(pane)) = (row, pane) else {
            return;
        };
        let Some(lab) = self.shell.lab_mut() else {
            return;
        };
        // The alert's icon (`alert.tga`, 17 wide) and the tip's (`tip.bmp`, 19 wide).
        let icon_w = if alert.is_some() { 17.0 } else { 19.0 };
        let left =
            ((row.w.floor() - (icon_w + 3.0 + width)) / 2.0).floor() + row.x.floor() - pane.x;
        let top = row.y - pane.y;
        let doc = TIP_GIB
            .replace("{TEXT}", &text)
            .replace("{ICONX}", &format!("{left}"))
            .replace("{ICONY}", &format!("{top}"))
            .replace(
                "{ICON}",
                if alert.is_some() {
                    "alert.tga"
                } else {
                    "tip.bmp"
                },
            )
            .replace("{ICONW}", &format!("{icon_w}"))
            .replace("{ALPHA}", if alert.is_some() { "true" } else { "false" })
            .replace(
                "{COLOR}",
                if alert.is_some() {
                    "1 0.8 0.3"
                } else {
                    "0.5 0.6 1"
                },
            )
            .replace("{TEXTX}", &format!("{}", left + icon_w + 2.0))
            .replace("{TEXTY}", &format!("{}", top - 8.0));
        let gib = match ra2fmt::gib::parse(&doc) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("tip parse: {e:?}");
                return;
            }
        };
        // An empty tip draws nothing, icon included (live, after a cancelled drag).
        let extra: Vec<Widget> = gib
            .blocks
            .into_iter()
            .filter_map(|b| match b {
                ra2fmt::gib::Block::Widget(w) if !text.is_empty() => Some(w),
                _ => None,
            })
            .collect();
        for block in lab.document.blocks.iter_mut() {
            let ra2fmt::gib::Block::Widget(root) = block else {
                continue;
            };
            if let Some(info) = info_display_mut(root) {
                info.children.clear();
                info.children.extend(extra.iter().cloned());
            }
        }
        self.tip_written = Some(key);
    }

    /// The tabs that show the open design's numbers (overview, components) take
    /// its name, mass and weight class; the overview's snapshot box takes the
    /// snapshot the `.bot` carries.
    fn refresh_overview(&mut self) {
        let stats = self.build.stats();
        let name = if self.name_edited {
            self.name_buffer.clone()
        } else {
            self.build.name().to_string()
        };
        // A blank robot has no weight and the class label has not been computed yet (live: `0.0` /
        // `None`; the label is only written when a part or the chassis changes the mass).
        let blank = self.build.is_blank();
        let mass = if blank {
            "0.0".to_string()
        } else {
            format!("{:.1}", stats.mass)
        };
        let class = if blank {
            "None"
        } else {
            ra2ui::workshop::stats::WeightClass::of_mass(stats.mass).full_label()
        };
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
        self.sync_paint_widgets();
    }

    /// Put the swatch, the picker's target and the slider's thumb where the colour state says,
    /// and let `Selected Color` hang below the swatch (live it is drawn outside the 64 px group).
    fn sync_paint_widgets(&mut self) {
        let color = self.paint_color;
        let placed = self.shell.placements();
        let hue = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "hue group")
            .map(|p| p.frame);
        let slider = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "lum slider")
            .map(|p| p.frame);
        drop(placed);
        let Some(lab) = self.shell.lab_mut() else {
            return;
        };
        for block in lab.document.blocks.iter_mut() {
            let ra2fmt::gib::Block::Widget(root) = block else {
                continue;
            };
            if let Some(group) = workshop::find_mut(root, "selected color group") {
                ra2ui::input::set_field(group, "ClipMode", "0");
                if let Some(label) = workshop::find_mut(group, "selected label") {
                    ra2ui::input::set_field(label, "ClipMode", "0");
                }
            }
            if let Some(image) = workshop::find_mut(root, "selected color img") {
                let [r, g, b] = color.rgb;
                ra2ui::input::set_field(
                    image,
                    "_ForeColor",
                    &format!(
                        "{} {} {} 1",
                        f32::from(r) / 255.0,
                        f32::from(g) / 255.0,
                        f32::from(b) / 255.0
                    ),
                );
            }
            if let (Some(target), Some(frame)) = (workshop::find_mut(root, "picker target"), hue) {
                let x = color.marker[0] * frame.w - 4.0;
                let y = color.marker[1] * frame.h - 4.0;
                ra2ui::input::set_field(target, "Position", &format!("{x} 0 {y} 0"));
            }
            if let (Some(slider_widget), Some(frame)) =
                (workshop::find_mut(root, "lum slider"), slider)
            {
                if let Some(thumb) = slider_widget
                    .children
                    .iter_mut()
                    .find(|c| c.kind == WidgetKind::Image)
                {
                    let thumb_w = thumb.size().map(|s| s.first.amount).unwrap_or(12.0);
                    let x = color.lum * (frame.w - thumb_w).max(0.0);
                    ra2ui::input::set_field(thumb, "Position", &format!("{x} 0 0 0"));
                }
            }
        }
    }

    /// A press or drag on the Paint Shop's picker or slider: the target or the thumb goes to the
    /// pointer and the swatch takes the colour they choose. `Some(start)` begins a drag.
    fn paint_pick(&mut self, x: f32, y: f32, start: bool) -> bool {
        if self.tab() != Some("paintshop button") {
            return false;
        }
        let placed = self.shell.placements();
        let frame_of = |name: &str| {
            placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == name)
                .map(|p| p.frame)
        };
        let hue = frame_of("hue group");
        let slider = frame_of("lum slider");
        drop(placed);
        let which = if start {
            if hue.is_some_and(|f| f.contains_point(x, y)) {
                Some(PaintDrag::Picker)
            } else if slider.is_some_and(|f| f.contains_point(x, y)) {
                Some(PaintDrag::Slider)
            } else {
                None
            }
        } else {
            self.paint_color.drag
        };
        let Some(which) = which else {
            return false;
        };
        match (which, hue, slider) {
            (PaintDrag::Picker, Some(f), _) => {
                self.paint_color.marker = [
                    ((x - f.x) / f.w.max(1.0)).clamp(0.0, 1.0),
                    ((y - f.y) / f.h.max(1.0)).clamp(0.0, 1.0),
                ];
            }
            (PaintDrag::Slider, _, Some(f)) => {
                self.paint_color.lum = ((x - f.x) / f.w.max(1.0)).clamp(0.0, 1.0);
            }
            _ => return false,
        }
        if start {
            self.paint_color.drag = Some(which);
        }
        self.paint_color.rgb = self.paint_color.picked();
        self.sync_paint_widgets();
        self.status = format!("color {:?}", self.paint_color.rgb);
        true
    }

    /// The solid texture the Paint Shop's Color Fill paints with.
    fn paint_solid(&self) -> Texture {
        let [r, g, b] = self.paint_color.rgb;
        Texture::solid(16, 16, [r, g, b, 255])
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
        self.sync_arena();
        self.bind_arena_hud();
        self.refresh_tip();
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
        self.sync_arena();
        self.arena_quad(&mut scene)?;
        // The mouse legend and the SHIFT / CTRL tips sit inside the 3D view's own group: the
        // original draws the view first and these over it.
        {
            let placed = self.shell.placements();
            let overlay: Vec<&Placement> = placed
                .iter()
                .flat_map(|p| p.walk())
                .filter(|n| {
                    let name = n.widget.name();
                    // Live: the SHIFT / CTRL tips and REMOVE belong to the Components tab only.
                    name == "mouse instructions grp"
                        || (self.tab() == Some("components button")
                            && matches!(name, "tips group" | "remove button"))
                })
                .collect();
            for group in overlay {
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
        self.chassis_overlay(&mut scene);
        if let Some(dialog) = &self.dialog {
            let placed = ra2ui::resolve_with(&dialog.gib, self.size, Some(self.assets.faces()));
            let over = self
                .assets
                .scene(&placed, [self.size[0] as u32, self.size[1] as u32]);
            let offset = scene.textures.len();
            scene.textures.extend(over.textures);
            for mut quad in over.quads {
                quad.texture += offset;
                scene.quads.push(quad);
            }
            scene.missing.extend(over.missing);
        }
        if let Some(spec) = std::env::var_os("ORA_DEBUG_RECT") {
            // `x0,y0,x1,y1`: the quads that touch that rectangle, for diagnosing the lab's screens.
            let v: Vec<f32> = spec
                .to_string_lossy()
                .split(',')
                .filter_map(|t| t.trim().parse().ok())
                .collect();
            if let [x0, y0, x1, y1] = v[..] {
                for (i, q) in scene.quads.iter().enumerate() {
                    let r = q.rect;
                    if r.x < x1 && r.x + r.w > x0 && r.y < y1 && r.y + r.h > y0 {
                        eprintln!(
                            "quad {i}: {r:?} tex {} color {:?} clip {:?}",
                            q.texture, q.color, q.clip
                        );
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

    /// The practice arena's frame in `test arena placeholder`, under the widgets that are the
    /// placeholder's children (the `Obstacles` list, RESET and the status box).
    fn arena_quad(&mut self, scene: &mut UiScene) -> Result<(), String> {
        if self.arena.is_none() {
            return Ok(());
        }
        let placed = self.shell.placements();
        let Some(rect) = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "test arena placeholder")
            .map(|p| p.frame)
        else {
            return Ok(());
        };
        let size = [
            rect.w.round().max(16.0) as u32,
            rect.h.round().max(16.0) as u32,
        ];
        let pixels = match self.arena.as_mut() {
            Some(arena) => arena.frame(&self.renderer, size)?,
            None => return Ok(()),
        };
        let texture = scene.textures.len();
        scene.textures.push(UiTexture {
            label: "lab practice arena".into(),
            width: size[0],
            height: size[1],
            rgba: pixels,
        });
        let quad = UiQuad {
            rect,
            corners: None,
            uv: [0.0, 0.0, 1.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
            texture,
            alpha: false,
            clip: Some(rect),
        };
        // Right after the background that covers the placeholder.
        let at = scene
            .quads
            .iter()
            .position(|q| {
                q.color == [0.0, 0.0, 0.0, 1.0]
                    && (q.rect.x - rect.x).abs() < 1.5
                    && (q.rect.y - rect.y).abs() < 1.5
                    && (q.rect.w - rect.w).abs() < 1.5
                    && (q.rect.h - rect.h).abs() < 1.5
            })
            .map_or(0, |i| i + 1);
        if std::env::var_os("ORA_DEBUG_QUADS").is_some() {
            eprintln!("arena rect {rect:?} inserted at {at}");
            for (i, q) in scene
                .quads
                .iter()
                .enumerate()
                .filter(|(_, q)| q.rect.w > 400.0)
            {
                eprintln!(
                    "  quad {i}: {:?} tex {} color {:?}",
                    q.rect, q.texture, q.color
                );
            }
        }
        scene.quads.insert(at, quad);
        Ok(())
    }

    /// The blueprint's grid, outline and points drawn over the chassis steps' `Editing Area`
    /// (live: eleven 1 px lines of (112,112,127) 45 px apart, the outline white, each point a
    /// small ring, the selected one brighter).
    fn chassis_overlay(&self, scene: &mut UiScene) {
        if self.tab() == Some("wiring button") {
            self.controller_lines(scene);
        }
        if self.tab() != Some("chassis button") {
            return;
        }
        let placed = self.shell.placements();
        let Some(area) = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "Editing Area")
        else {
            return;
        };
        let grid = blueprint_grid(area.frame);
        let texture = scene.textures.len();
        scene.textures.push(UiTexture {
            label: "m9 ink".into(),
            width: 1,
            height: 1,
            rgba: vec![255, 255, 255, 255],
        });
        let mut ink = Ink {
            scene,
            texture,
            clip: area.frame,
        };
        let line_color = [112.0 / 255.0, 112.0 / 255.0, 127.0 / 255.0, 1.0];
        for k in 0..=10 {
            let t = k as f32 * grid.w / 10.0;
            ink.line(
                [grid.x + t, grid.y],
                [grid.x + t, grid.y + grid.h],
                1.0,
                line_color,
            );
            ink.line(
                [grid.x, grid.y + t],
                [grid.x + grid.w, grid.y + t],
                1.0,
                line_color,
            );
        }
        let step = self.structure_step();
        let Some(chassis) = self.build.chassis() else {
            return;
        };
        let (points, closed) = if step == 2 {
            (&chassis.top, true)
        } else {
            (&chassis.bottom, self.outline_closed)
        };
        let at = |p: &[i32; 2]| {
            [
                grid.x + p[0] as f32 / 1000.0 * grid.w,
                grid.y + p[1] as f32 / 1000.0 * grid.h,
            ]
        };
        let white = [1.0, 1.0, 1.0, 1.0];
        if step == 2 {
            // Live: the baseplate outline 2 px (153,153,191) under the top outline, 1 px white
            // (centred on the pixel row / column the point's coordinate names), each top point a
            // 16 px selection image: a grey ring, white while it is under the cursor or held.
            let violet = [0.6, 0.6, 0.749, 1.0];
            let bottom = &chassis.bottom;
            for i in 0..bottom.len() {
                if bottom.len() >= 3 {
                    ink.line(
                        at(&bottom[i]),
                        at(&bottom[(i + 1) % bottom.len()]),
                        2.0,
                        violet,
                    );
                }
            }
            let nudge = |p: [f32; 2]| [p[0] + 0.5, p[1] + 0.5];
            for i in 0..points.len() {
                if points.len() >= 3 {
                    let (a, b) = (at(&points[i]), at(&points[(i + 1) % points.len()]));
                    ink.line(nudge(a), nudge(b), 1.0, white);
                }
            }
        } else {
            for i in 0..points.len() {
                if i + 1 < points.len() || (closed && points.len() >= 3) {
                    let (a, b) = (at(&points[i]), at(&points[(i + 1) % points.len()]));
                    ink.line(a, b, 2.0, white);
                }
            }
        }
        for (i, p) in points.iter().enumerate() {
            let lit = self.draw_point == Some(i) && (step == 2 || closed);
            ink.ring(
                at(p),
                6.0,
                if step == 2 || lit { 2.0 } else { 1.0 },
                if lit {
                    white
                } else if step == 2 {
                    [0.749, 0.749, 0.749, 1.0]
                } else {
                    [0.78, 0.78, 0.82, 1.0]
                },
            );
        }
        // Step 1, an open freeform outline: the point a click would add is under the cursor (snapped),
        // a 1 px (204,204,255) line runs to it from the last point and its ring is green when the
        // editing area's rules accept it, red when they refuse it (live `botlab-step1-hover-*`).
        if step == 1 && !closed && self.draw_style == DrawStyle::Freeform {
            let hover = self
                .cursor
                .filter(|c| area.frame.contains_point(c[0], c[1]))
                .and_then(|c| self.outline_point_at(c[0], c[1]));
            if let Some(p) = hover {
                if let Some(last) = points.last() {
                    ink.line(at(last), at(&p), 1.0, [0.8, 0.8, 1.0, 1.0]);
                }
                let ok = workshop::adjust::check_add(points, p).is_ok();
                ink.ring(
                    at(&p),
                    6.0,
                    2.0,
                    if ok {
                        [0.0, 0.8, 0.0, 1.0]
                    } else {
                        [0.8, 0.0, 0.0, 1.0]
                    },
                );
            }
        }
    }

    /// The controller's grid rules (live: 1 px, (102,102,102), between the three columns and rows).
    fn controller_lines(&self, scene: &mut UiScene) {
        let Some((frame, _)) = self.control_grid() else {
            return;
        };
        let texture = scene.textures.len();
        scene.textures.push(UiTexture {
            label: "m9 ink".into(),
            width: 1,
            height: 1,
            rgba: vec![255, 255, 255, 255],
        });
        let mut ink = Ink {
            scene,
            texture,
            clip: frame,
        };
        let (w, h) = control_cell_size(frame);
        let color = [0.4, 0.4, 0.4, 1.0];
        for k in 1..3 {
            let x = frame.x + w * k as f32;
            ink.line([x, frame.y], [x, frame.y + frame.h], 1.0, color);
            let y = frame.y + h * k as f32;
            ink.line([frame.x, y], [frame.x + frame.w, y], 1.0, color);
        }
    }

    /// A harness shortcut: the baseplate drawn as the 600-unit square the live runs used and
    /// extruded (what clicking four corners, the first one again, step 2 and Finished does).
    pub fn draw_square_chassis(&mut self) -> Result<(), String> {
        self.build
            .set_chassis_rectangle(Outline::Bottom, [500, 500], 600, 600)
            .map_err(|e| e.to_string())?;
        self.build.extrude_chassis().map_err(|e| e.to_string())?;
        self.refresh()
    }

    /// The screen point of a widget on the lab's own screen, for a scripted click.
    pub fn point_of(&self, name: &str) -> Result<[f32; 2], String> {
        self.shell.point_for(name)
    }

    /// The screen point of a placed part in the 3D view (its mesh's centre), for a scripted click.
    pub fn part_point(&mut self, index: usize) -> Result<[f32; 2], String> {
        let world = self.build.transforms();
        let info = self
            .build
            .component_info(index)
            .ok_or_else(|| format!("no component {index}"))?;
        let centre = info
            .bounds
            .map(|(lo, hi)| (Vec3::from(lo) + Vec3::from(hi)) * 0.5)
            .unwrap_or(Vec3::ZERO);
        let at = world[index].transform_point3(centre);
        self.preview_point_of(at)
    }

    /// The screen point of row `row` (0 = first item) of the open connection window's dropdown list.
    pub fn dialog_list_point(&self, row: usize) -> Option<[f32; 2]> {
        let dialog = self.dialog.as_ref()?;
        let placed = ra2ui::resolve_with(&dialog.gib, self.size, Some(self.assets.faces()));
        let list = placed.iter().flat_map(|p| p.walk()).find(|p| {
            p.widget.kind == WidgetKind::ListBox
                && ra2ui::input::field_of(p.widget, "Visible") == Some("true")
                && p.frame.w > 0.0
        })?;
        let height = ra2ui::Grid::from_widget(list.widget)
            .map(|g| g.row_height)
            .filter(|h| *h > 0.0)
            .unwrap_or(18.0);
        Some([
            list.frame.x + list.frame.w * 0.5,
            list.frame.y + height * (row as f32 + 0.5),
        ])
    }

    /// The screen point of a widget in the open wiring window, for a scripted click.
    pub fn dialog_point_for(&self, name: &str) -> Option<[f32; 2]> {
        let dialog = self.dialog.as_ref()?;
        let placed = ra2ui::resolve_with(&dialog.gib, self.size, Some(self.assets.faces()));
        let found = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == name)?;
        Some([
            found.frame.x + found.frame.w * 0.5,
            found.frame.y + found.frame.h * 0.5,
        ])
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
        // What else the frame depends on besides the model: the lab camera, the tab and the
        // selected control.
        let view_key = {
            let cam = self.lab_cam();
            format!(
                "{:?}|{:?}|{:?}|{:?}|{:?}|{}",
                self.tab(),
                self.wire_control,
                cam.pitch.to_bits(),
                cam.yaw.to_bits(),
                cam.dist.to_bits(),
                self.on_adjust_screen()
            )
        };
        if let Some((revision, pixels, w, h)) = &self.preview {
            if *revision == self.revision
                && *w == width
                && *h == height
                && self.preview_view_key == view_key
            {
                return Ok(Some((pixels.clone(), *w, *h)));
            }
        }
        self.preview_view_key = view_key;
        let bot = self.build.to_bot();
        let world = self.build.transforms();
        let mut chassis = ra2_render::chassis_items(&bot, Mat4::IDENTITY);
        if matches!(
            self.tab(),
            Some("components button") | Some("wiring button")
        ) {
            // Live, the chassis is a ghost while parts are placed and wired, so the parts under
            // its top panel can be seen from above.
            // (`D3DRS_ALPHABLENDENABLE` with SRCALPHA / ONE, no z write, no culling, no texture,
            // diffuse (0.625, 0.625, 0.625, 0.14): the live state of the chassis' draw.)
            let plate =
                crate::lab_view::baseplate(&chassis, &self.ui.join("botlab/baseplate_grid.bmp"));
            for item in chassis.iter_mut() {
                item.material.stages.clear();
                item.material.transparent = true;
                item.material.additive = true;
                item.material.no_alpha_test = true;
                item.material.z_write = false;
                item.material.early = true;
                item.material.cull = false;
                item.material.diffuse = [0.625, 0.625, 0.625, 0.14];
            }
            if let Some(plate) = plate {
                chassis.insert(0, plate);
            }
        }
        // The lines the original draws over the ghost chassis, and a selected control's graphics.
        let mut overlay: Vec<DrawItem> = Vec::new();
        if self.view_only
            && !world.is_empty()
            && matches!(
                self.tab(),
                Some("components button") | Some("wiring button")
            )
        {
            let (eye, _) = self.lab_cam().eye_up();
            let cam = crate::lab_view::Cam::new(
                eye,
                Vec3::ZERO,
                std::f32::consts::FRAC_PI_4,
                height as f32,
            );
            let plain = ra2_render::chassis_items(&bot, Mat4::IDENTITY);
            let edges: Vec<(Vec3, Vec3)> = crate::lab_view::outline_edges(&plain)
                .into_iter()
                .map(|(a, b)| (world[0].transform_point3(a), world[0].transform_point3(b)))
                .collect();
            let lines = crate::lab_view::ribbon(
                &cam,
                &edges,
                crate::lab_view::EDGE_COLOR,
                crate::lab_view::EDGE_PIXELS,
            );
            if !lines.is_empty() {
                overlay.push(DrawItem::new(
                    lines,
                    ra2_render::Material {
                        lighting: false,
                        has_vertex_colors: true,
                        cull: false,
                        ..ra2_render::Material::default()
                    },
                    "lab chassis edges",
                ));
            }
            if self.tab() == Some("wiring button") {
                if let Some(cell) = self.wire_control {
                    if let Some(control) = self.build.board().control_at(cell) {
                        let mut parts: Vec<[f32; 2]> = Vec::new();
                        for b in &self.build.board().bindings {
                            if [b.control[0], b.control[1]] != cell {
                                continue;
                            }
                            let Some(m) = world.get(b.component as usize) else {
                                continue;
                            };
                            let p = crate::lab_view::origin_of(*m);
                            if !parts.contains(&[p.x, p.z]) {
                                parts.push([p.x, p.z]);
                            }
                        }
                        let anchor = self.wire_anchor(&world);
                        let file = self.ui.join(format!(
                            "botlab/ctrl_icon_{}.bmp",
                            control.kind.label().to_lowercase()
                        ));
                        let scene = crate::lab_view::wire_scene(&cam, anchor, &parts, &file);
                        // The wires are drawn over the icon (live: a wire crosses its label).
                        overlay.push(scene.icon);
                        overlay.push(scene.lines);
                    }
                }
            }
        }
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
        // The lab's own graphics are not part of the framing either.
        if !overlay.is_empty() {
            groups.push(RenderGroup {
                items: &overlay,
                model: Mat4::IDENTITY,
            });
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
        let _ = (center, radius);
        let (eye, up) = if self.view_only {
            // The original's lab camera (`FUN_005a6260` mode 2, one object per tab): a 45 degree
            // perspective at `dist` from the robot's origin, orbiting with the middle button.
            // Step 2's pane is drawn over the gib's own (0.1, 0.1, 0.1) image: 25 of 255 live; the
            // other tabs' views clear to 38 of 255.
            let backdrop = if self.on_adjust_screen() {
                25.0 / 255.0
            } else {
                0.15
            };
            self.renderer.set_background([backdrop; 3]);
            let cam = self.lab_cam();
            let (eye, up) = cam.eye_up();
            // The lab lights the robot with one white point light on the camera (live `D3DLIGHT8`:
            // type 1, diffuse and specular 1, range 1000, attenuation 1 / 0 / 0, at the eye),
            // over the usual 38/255 ambient.
            self.renderer.set_lights(&[ra2_render::material::Light {
                position: eye,
                range: 1000.0,
                diffuse: [1.0; 3],
                specular: [1.0; 3],
                attenuation: [1.0, 0.0, 0.0],
            }]);
            (eye, up)
        } else {
            (
                center + Vec3::new(radius * 1.9, radius * 1.2, radius * 2.1),
                Vec3::Y,
            )
        };
        let view = ra2_render::look_at_lh(eye, Vec3::ZERO, up);
        let proj = ra2_render::perspective_lh(
            45f32.to_radians(),
            width as f32 / height as f32,
            0.05,
            if self.view_only {
                2000.0
            } else {
                radius * 30.0
            },
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

/// Solid-colour strokes drawn into a scene from a one-pixel white texture.
struct Ink<'a> {
    scene: &'a mut UiScene,
    texture: usize,
    clip: Frame,
}

impl Ink<'_> {
    fn quad(&mut self, rect: Frame, corners: Option<[[f32; 2]; 4]>, color: [f32; 4]) {
        self.scene.quads.push(UiQuad {
            rect,
            corners,
            uv: [0.0, 0.0, 1.0, 1.0],
            color,
            texture: self.texture,
            alpha: true,
            clip: Some(self.clip),
        });
    }

    /// A stroke `width` px wide between two points.
    fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, color: [f32; 4]) {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.01 {
            return;
        }
        if dy.abs() < 0.01 || dx.abs() < 0.01 {
            let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
            let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
            let rect = if dy.abs() < 0.01 {
                Frame::new(x0, y0 - width * 0.5 + 0.5, x1 - x0, width)
            } else {
                Frame::new(x0 - width * 0.5 + 0.5, y0, width, y1 - y0)
            };
            self.quad(rect, None, color);
            return;
        }
        let (nx, ny) = (-dy / len * width * 0.5, dx / len * width * 0.5);
        let corners = [
            [a[0] + nx, a[1] + ny],
            [b[0] + nx, b[1] + ny],
            [b[0] - nx, b[1] - ny],
            [a[0] - nx, a[1] - ny],
        ];
        let rect = Frame::new(a[0].min(b[0]), a[1].min(b[1]), dx.abs(), dy.abs());
        self.quad(rect, Some(corners), color);
    }

    /// A ring of radius `r`, drawn as twelve strokes.
    fn ring(&mut self, c: [f32; 2], r: f32, width: f32, color: [f32; 4]) {
        let at = |i: usize| {
            let t = std::f32::consts::TAU * i as f32 / 12.0;
            [c[0] + r * t.cos(), c[1] + r * t.sin()]
        };
        for i in 0..12 {
            self.line(at(i), at(i + 1), width, color);
        }
    }
}

/// The blueprint's drawing grid inside the `Editing Area` (live, 1024x768: the area is 466 px
/// square at (31.8, 141.5) and the grid's eleven lines run 39..489 across and 148..598 down, 45 px
/// apart: the `0..1000` outline range is 450 px).
fn blueprint_grid(area: Frame) -> Frame {
    Frame::new(
        area.x + area.w * 0.015_45,
        // live: the first grid row is 148 with the area at y 140.98 (lab panes are whole pixels
        // since docs/36 9.27c, which moved the area up by half a pixel)
        area.y + area.h * 0.015_06,
        area.w * 0.9657,
        area.h * 0.9657,
    )
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

fn list_folder(game: &Path, rel: &str, extensions: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = ra2fmt::saves::list(game, rel)
        .into_iter()
        .filter(|name| {
            let path = ra2fmt::saves::resolve(game, Path::new(rel).join(name));
            path.is_file()
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|ext| extensions.iter().any(|e| ext.eq_ignore_ascii_case(e)))
        })
        .collect();
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
        builder.mouse_moved(select[0], select[1]);
        builder.click(select[0], select[1])?;
        let target = [
            point[0] + (500 - point[0]) / 16,
            point[1] + (500 - point[1]) / 16,
        ];
        let moved = builder.outline_point_for(target)?;
        builder.mouse_moved(moved[0], moved[1]);
        builder.mouse_released(moved[0], moved[1]);
        let after = builder
            .build
            .chassis()
            .map(|c| c.top.clone())
            .unwrap_or_default();
        if after.first().copied() == Some(point) {
            return Err(format!(
                "the chassis drag did not move top point 0: {:?} ({})",
                after.first(),
                builder.status()
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
    builder.click_named("power button")?;
    builder.place_component_free("Components\\controlboard.txt")?;
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
    let live_components = builder.build.to_bot().through_the_file().components;
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
    // The file keeps six significant digits, so the masses agree to that and not to the last bit.
    let close = |a: f32, b: f32| (a - b).abs() <= 1e-4 * b.abs().max(1.0);
    if !(close(reloaded.mass, stats.mass)
        && close(reloaded.hitpoints, stats.hitpoints)
        && close(reloaded.chassis_hitpoints, stats.chassis_hitpoints)
        && reloaded.declared_class == stats.declared_class
        && reloaded.computed_class == stats.computed_class
        && reloaded.parts.len() == stats.parts.len())
    {
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
    // The lab's sum counts the phantom bodies (a wheel's traction ring) that the rig's `mass` leaves out.
    let rig_mass: f32 = rig
        .components
        .iter()
        .map(|c| c.mass + c.havok.phantom_mass)
        .sum();
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

// -- The wiring windows (docs/36 section 9.27) --------------------------------------------------

impl Builder {
    /// Whether a wiring window is open (it takes every click and key).
    pub fn dialog_open(&self) -> bool {
        self.dialog.is_some()
    }

    /// The control kind whose strip icon the press at `(x, y)` is on, for a drag.
    pub fn control_icon_at(&self, x: f32, y: f32) -> Option<ControlKind> {
        if self.tab() != Some("wiring button") {
            return None;
        }
        let placed = self.shell.placements();
        let path = ra2ui::input::hit(&placed, x, y)?;
        let chain = chain(&placed, &path);
        [
            ("new analog", ControlKind::Analog),
            ("new switch", ControlKind::Switch),
            ("new button", ControlKind::Button),
        ]
        .into_iter()
        .find(|(name, _)| chain.iter().any(|p| p.widget.name() == *name))
        .map(|(_, kind)| kind)
    }

    /// The button is released at a point: a control icon being dragged that lands on a free cell
    /// of the controller opens its properties window.
    pub fn mouse_released(&mut self, x: f32, y: f32) {
        let _ = (x, y);
        self.paint_color.drag = None;
        // `FUN_004791e0`: the drag of a top point ends where it is.
        self.point_drag = false;
        if self.dialog.is_some() || self.tab() != Some("wiring button") {
            return;
        }
        if self.wire_placing.is_some() {
            if let Some(cell) = self.control_cell_at(x, y) {
                if self.build.board().control_at(cell).is_none() {
                    self.open_props_dialog(cell);
                }
            }
        }
    }

    fn open_props_dialog(&mut self, cell: [i32; 2]) {
        let Some(kind) = self.wire_placing.take() else {
            self.status = "pick SWITCH, BUTTON or ANALOG first".into();
            return;
        };
        let Ok(mut gib) = workshop::load(&self.ui, lab_dialog::props_file(kind)) else {
            return;
        };
        let name = lab_dialog::default_name(kind).to_string();
        lab_dialog::set_text(&mut gib, "control name", &name);
        // The input boxes start empty; the file's own `W` / `S` is a design-time sample.
        lab_dialog::set_text(&mut gib, "input1", "");
        lab_dialog::set_text(&mut gib, "input2", "");
        self.dialog = Some(LabDialog {
            gib,
            kind: DialogKind::Props {
                kind,
                cell,
                name,
                keys: [None, None],
                editing: false,
            },
            focus: Focus::None,
        });
    }

    /// A double click on a control opens its own window (live: the same `Analog Control` /
    /// `Switch Control` / `Button Control` window with the control's name and keys in it).
    fn open_edit_dialog(&mut self, cell: [i32; 2]) {
        let Some(control) = self.build.board().control_at(cell).cloned() else {
            return;
        };
        let Ok(mut gib) = workshop::load(&self.ui, lab_dialog::props_file(control.kind)) else {
            return;
        };
        lab_dialog::set_text(&mut gib, "control name", &control.name);
        let mut keys = [None, None];
        for (slot, row) in control.keys.iter().enumerate().take(2) {
            let code = Control::key_code(row);
            let label = if code > 0 {
                keys[slot] = Some(code);
                ra2ui::workshop::wiring::dik_name(code)
            } else {
                String::new()
            };
            lab_dialog::set_text(&mut gib, ["input1", "input2"][slot], &label);
        }
        self.wire_control = Some(cell);
        self.dialog = Some(LabDialog {
            gib,
            kind: DialogKind::Props {
                kind: control.kind,
                cell,
                name: control.name.clone(),
                keys,
                editing: true,
            },
            focus: Focus::None,
        });
    }

    /// A click on a component in the 3D view of the Wiring tab: with a control selected it
    /// opens that control's connection window for the part.
    fn wire_at_preview(&mut self, x: f32, y: f32) -> Result<(), String> {
        let Some(cell) = self.wire_control else {
            return Ok(());
        };
        let Some((origin, direction)) = self.bot_ray(x, y) else {
            return Ok(());
        };
        let Some(component) = self.build.pick_part(origin, direction) else {
            return Ok(());
        };
        self.open_wire_dialog(component, cell);
        Ok(())
    }

    fn open_wire_dialog(&mut self, component: usize, cell: [i32; 2]) {
        let Some(control) = self.build.board().control_at(cell).cloned() else {
            return;
        };
        let class = self
            .build
            .components()
            .get(component)
            .map(|c| c.class.clone())
            .unwrap_or_default();
        let actions: Vec<_> =
            ra2ui::workshop::wiring::ControlBoard::actions_for(control.kind, &class, 0).to_vec();
        if actions.is_empty() {
            // A part this kind of control cannot drive takes no window.
            self.status = format!("a {} control cannot drive a {class}", control.kind.label());
            return;
        }
        let Ok(mut gib) = workshop::load(&self.ui, lab_dialog::wire_file(control.kind)) else {
            return;
        };
        let (name, art) = self
            .build
            .component_info(component)
            .map(|i| (i.name.clone(), i.file.clone()))
            .unwrap_or_default();
        lab_dialog::set_text(&mut gib, "component name", &name);
        if let Some(c) = self
            .palette
            .iter()
            .find(|c| c.file.eq_ignore_ascii_case(&art))
        {
            for block in gib.blocks.iter_mut() {
                if let ra2fmt::gib::Block::Widget(root) = block {
                    ra2ui::cells::bind_art(root, "component image", &c.art);
                }
            }
        }
        let slots = control.kind.slots();
        let items: Vec<String> = std::iter::once("<none>".to_string())
            .chain(actions.iter().map(|a| a.name().to_string()))
            .collect();
        let mut picks = [0usize; 2];
        for slot in 0..slots {
            let bound = self
                .build
                .board()
                .bindings
                .iter()
                .find(|b| {
                    b.component == component as i32 && b.control == [cell[0], cell[1], slot as i32]
                })
                .and_then(|b| actions.iter().position(|a| a.name() == b.action));
            picks[slot] = bound.map(|i| i + 1).unwrap_or(0);
            lab_dialog::set_text(&mut gib, &format!("input{} key", slot + 1), "");
            ra2ui::dropdown::set_text(&mut gib, lab_dialog::DROPDOWNS[slot], &items[picks[slot]]);
            let key = control
                .keys
                .get(slot)
                .map(|row| ra2ui::workshop::wiring::dik_name(Control::key_code(row)))
                .unwrap_or_default();
            lab_dialog::set_text(&mut gib, &format!("input{} key", slot + 1), &key);
        }
        self.dialog = Some(LabDialog {
            gib,
            kind: DialogKind::Wire {
                kind: control.kind,
                cell,
                component,
                actions,
                picks,
            },
            focus: Focus::None,
        });
    }

    /// The rows a connection window's dropdown lists.
    fn dialog_items(&self) -> Vec<String> {
        match self.dialog.as_ref().map(|d| &d.kind) {
            Some(DialogKind::Wire { actions, .. }) => std::iter::once("<none>".to_string())
                .chain(actions.iter().map(|a| a.name().to_string()))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn dialog_click(&mut self, x: f32, y: f32) -> Result<(), String> {
        use ra2ui::dropdown::{self, Part};
        let Some(dialog) = self.dialog.as_ref() else {
            return Ok(());
        };
        let placed = ra2ui::resolve_with(&dialog.gib, self.size, Some(self.assets.faces()));
        let hit = ra2ui::input::hit_control(&placed, x, y);
        // A dropdown first (an open list takes every click).
        if matches!(dialog.kind, DialogKind::Wire { .. }) {
            if let Some((name, part)) = dropdown::click_part(&placed, hit.as_deref(), x, y) {
                let items = self.dialog_items();
                let slot = lab_dialog::DROPDOWNS.iter().position(|d| *d == name);
                let dialog = self.dialog.as_mut().expect("a dialog");
                let gib = &mut dialog.gib;
                let open = dropdown::is_open(gib, &name);
                match part {
                    Part::Field if open => dropdown::close(gib, &name),
                    Part::Field => {
                        let current = dropdown::text(gib, &name).unwrap_or_default();
                        let selected = items
                            .iter()
                            .position(|i| i.eq_ignore_ascii_case(current.trim()))
                            .unwrap_or(0);
                        dropdown::open(gib, &name, &items, selected);
                    }
                    Part::Row(row) => {
                        if dropdown::pick(gib, &name, &items, row).is_some() {
                            if let (Some(slot), DialogKind::Wire { picks, .. }) =
                                (slot, &mut dialog.kind)
                            {
                                picks[slot] = row;
                            }
                        }
                    }
                    Part::Scroll(top) => dropdown::scroll(gib, &name, top),
                    Part::Outside => dropdown::close(gib, &name),
                }
                return Ok(());
            }
        }
        let names: Vec<String> = hit
            .map(|path| {
                chain(&placed, &path)
                    .iter()
                    .map(|p| p.widget.name().to_string())
                    .collect()
            })
            .unwrap_or_default();
        let has = |n: &str| names.iter().any(|c| c == n);
        if let Some(DialogKind::ConfirmDelete { cell }) =
            self.dialog.as_ref().map(|d| d.kind.clone())
        {
            if has("yes button") {
                self.dialog = None;
                return self.confirm_delete(cell);
            }
            if has("no button") {
                self.dialog = None;
                return self.refresh();
            }
            return Ok(());
        }
        if has("cancel button") {
            self.dialog = None;
            return self.refresh();
        }
        if has("ok button") {
            return self.dialog_ok();
        }
        if let Some(dialog) = self.dialog.as_mut() {
            if matches!(dialog.kind, DialogKind::Props { .. }) {
                dialog.focus = if has("control name") {
                    Focus::Name
                } else if has("input1") {
                    Focus::Key(0)
                } else if has("input2") {
                    Focus::Key(1)
                } else {
                    dialog.focus
                };
            }
        }
        Ok(())
    }

    /// Delete the control in `cell` (the Yes of the confirmation).
    fn confirm_delete(&mut self, cell: [i32; 2]) -> Result<(), String> {
        match self.build.board_mut().remove(cell) {
            Some(control) => {
                self.wire_control = None;
                self.wire_component = None;
                self.status = format!("removed control {}", control.name);
                self.refresh()
            }
            None => {
                self.status = "there is no control there".into();
                self.refresh_panel();
                Ok(())
            }
        }
    }

    fn dialog_ok(&mut self) -> Result<(), String> {
        let Some(dialog) = self.dialog.take() else {
            return Ok(());
        };
        match dialog.kind {
            DialogKind::ConfirmDelete { .. } => {}
            DialogKind::Props {
                kind,
                cell,
                name,
                keys,
                editing,
            } => {
                let rows: Vec<[i32; 4]> = keys
                    .iter()
                    .take(kind.slots())
                    .map(|k| [0, 0, k.unwrap_or(0), 0])
                    .collect();
                if editing {
                    // The control keeps its cell and its bindings; the name and keys change.
                    if let Some(existing) = self
                        .build
                        .board_mut()
                        .controls
                        .iter_mut()
                        .find(|c| c.cell == cell)
                    {
                        existing.name = name.trim().to_string();
                        existing.keys = rows;
                        self.status = format!("edited {}", name.trim());
                    }
                    return self.refresh();
                }
                let control = Control::new(name.trim(), cell, kind).with_keys(rows);
                match self.build.board_mut().add(control) {
                    Ok(()) => {
                        self.wire_control = Some(cell);
                        self.wire_component = None;
                        self.status = format!("added {}", name.trim());
                    }
                    Err(e) => self.status = e.to_string(),
                }
            }
            DialogKind::Wire {
                cell,
                component,
                actions,
                picks,
                kind,
            } => {
                for (slot, pick) in picks.iter().enumerate().take(kind.slots()) {
                    self.build.board_mut().unbind(component, cell, slot);
                    if *pick > 0 {
                        if let Some(action) = actions.get(pick - 1).copied() {
                            if let Err(e) = self.build.bind_action(component, action, cell, slot) {
                                self.status = e.to_string();
                            }
                        }
                    }
                }
            }
        }
        self.refresh()
    }

    fn dialog_type(&mut self, c: char) {
        let Some(dialog) = self.dialog.as_mut() else {
            return;
        };
        let focus = dialog.focus;
        let DialogKind::Props { name, keys, .. } = &mut dialog.kind else {
            return;
        };
        match focus {
            Focus::Name => {
                if (c.is_ascii_graphic() || c == ' ') && name.chars().count() < 24 {
                    name.push(c);
                    let shown = name.clone();
                    lab_dialog::set_text(&mut dialog.gib, "control name", &shown);
                }
            }
            Focus::Key(slot) => {
                if let Some(code) = lab_dialog::dik_for_char(c) {
                    keys[slot] = Some(code);
                    let label = ra2ui::workshop::wiring::dik_name(code);
                    lab_dialog::set_text(&mut dialog.gib, &format!("input{}", slot + 1), &label);
                }
            }
            Focus::None => {}
        }
    }

    fn dialog_backspace(&mut self) {
        let Some(dialog) = self.dialog.as_mut() else {
            return;
        };
        let focus = dialog.focus;
        let DialogKind::Props { name, keys, .. } = &mut dialog.kind else {
            return;
        };
        match focus {
            Focus::Name => {
                name.pop();
                let shown = name.clone();
                lab_dialog::set_text(&mut dialog.gib, "control name", &shown);
            }
            Focus::Key(slot) => {
                keys[slot] = None;
                lab_dialog::set_text(&mut dialog.gib, &format!("input{}", slot + 1), "");
            }
            Focus::None => {}
        }
    }

    /// A named key that has no character (the arrows): it fills a focused input box.
    pub fn key_named(&mut self, name: &str) {
        let Some(code) = lab_dialog::dik_for_name(name) else {
            return;
        };
        let Some(dialog) = self.dialog.as_mut() else {
            return;
        };
        let focus = dialog.focus;
        if let (Focus::Key(slot), DialogKind::Props { keys, .. }) = (focus, &mut dialog.kind) {
            keys[slot] = Some(code);
            let label = ra2ui::workshop::wiring::dik_name(code);
            lab_dialog::set_text(&mut dialog.gib, &format!("input{}", slot + 1), &label);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2ui::workshop::wiring::{Action, ControlBoard};
    use std::path::PathBuf;

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

    /// The player-facing lab draws no synthetic text panel (the original's has
    /// none); the harness keeps it unless the player route is applied.
    #[test]
    fn the_player_route_hides_the_synthetic_panel_unless_asked() {
        let Some(game) = game_dir() else { return };
        let mut builder = Builder::open(&game, None).unwrap();
        assert!(!builder.view_only, "the scripted harness keeps its panel");
        set_debug_panel_default(false);
        builder.apply_debug_panel_default().unwrap();
        assert!(builder.view_only);
        // The robot view fills the placeholder: no strip is cut off for rows.
        assert_eq!(builder.panel.preview.w, builder.placeholder_frames().0.w);
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
        assert_eq!(
            back.components,
            builder.build.to_bot().through_the_file().components
        );
        assert_eq!(back.write(), bytes);
        assert!((back.components.last().unwrap().angle - part.angle).abs() < 1e-6);
    }

    /// `Delete Control` asks first (live `botlab-wiring-delete-confirm`: a `Wiring` box with `Yes` and
    /// `No`): No keeps the control, Yes removes it.
    #[test]
    fn delete_control_asks_before_it_deletes() {
        let Some(game) = game_dir() else { return };
        let Some(scratch) = std::env::var_os("ORA_TEST_SCRATCH").map(PathBuf::from) else {
            // The Builder writes nothing, but it reads a design: run on the shipped one.
            let mut builder = Builder::open_sized(
                &game,
                Some(&game.join("Teams/Team0/Bot0.bot")),
                [1024.0, 768.0],
            )
            .unwrap();
            delete_flow(&mut builder, None);
            return;
        };
        let mut builder = Builder::open_sized(
            &game,
            Some(&game.join("Teams/Team0/Bot0.bot")),
            [1024.0, 768.0],
        )
        .unwrap();
        delete_flow(&mut builder, Some(&scratch));
    }

    fn delete_flow(builder: &mut Builder, shot: Option<&Path>) {
        builder.click_named("wiring button").unwrap();
        let before = builder.build.board().controls.len();
        assert!(before > 0, "C.R.U.S.H. has controls");
        let cell = builder.build.board().controls[0].cell;
        builder
            .apply(&format!("m9:wirectl:{}:{}", cell[0], cell[1]))
            .unwrap();
        builder.apply("m9:wireremove").unwrap();
        assert!(builder.dialog_open(), "Delete Control opens the box");
        assert_eq!(builder.build.board().controls.len(), before, "nothing yet");
        if let Some(dir) = shot {
            // The harness' view: no debug panel over the 3D view.
            builder.view_only = true;
            builder
                .shot(&dir.join("delete-confirm.png"), [1024, 768])
                .unwrap();
            builder.view_only = false;
        }
        let no = builder.dialog_point_for("no button").expect("a No button");
        builder.click(no[0], no[1]).unwrap();
        assert!(!builder.dialog_open());
        assert_eq!(builder.build.board().controls.len(), before, "No keeps it");
        builder
            .apply(&format!("m9:wirectl:{}:{}", cell[0], cell[1]))
            .unwrap();
        builder.apply("m9:wireremove").unwrap();
        let yes = builder
            .dialog_point_for("yes button")
            .expect("a Yes button");
        builder.click(yes[0], yes[1]).unwrap();
        assert!(!builder.dialog_open());
        assert_eq!(
            builder.build.board().controls.len(),
            before - 1,
            "Yes removes it"
        );
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
        // The window asks for the tip every frame, which is what notes the screen.
        let _ = builder.tip_text();
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
        // The tip line shows it as the orange alert (`FUN_0043e7c0`, live `botlab-alert-intersect`), and
        // keeps it up while the part stays refused (the exe judges every frame).
        assert_eq!(
            builder.alert(),
            Some("Components should not intersect the chassis or other components.")
        );
        builder.advance(3.0);
        assert!(builder.alert().is_some(), "still refused: the alert stays");
        builder.click(over[0], over[1]).unwrap();
        assert!(builder.dragging(), "a refused part stays in the drag");
        assert_eq!(builder.build.components().len(), count);
        // Off the baseplate sheet altogether: over nothing.
        let nothing = [builder.panel.preview.x + 3.0, builder.panel.preview.y + 3.0];
        builder.mouse_moved(nothing[0], nothing[1]);
        // No longer refused: the alert runs out two seconds later and the drag tip is back.
        builder.advance(1.0);
        assert!(builder.alert().is_some(), "an alert lasts two seconds");
        builder.advance(1.5);
        assert!(builder.alert().is_none(), "and goes");
        assert_eq!(builder.tip_text(), placement::TIP_DRAG);
        builder.click(nothing[0], nothing[1]).unwrap();
        assert!(!builder.dragging(), "a click over nothing cancels");
        // Live (`botlab-tip-cancel-blank`): the tip line is left empty.
        assert_eq!(builder.tip_text(), "");
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
        builder.click_named("power button").unwrap();
        builder
            .place_component_free("Components\\controlboard.txt")
            .unwrap();
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
        ra2fmt::saves::bind(&dir, &dir);

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

    /// The lab camera against the live matrices (`FUN_005a6260` mode 2, dumped through Frida): at
    /// pitch 0.5 / yaw 0.7 the camera's forward axis was `(0.565, 0.479, 0.671)` and its up
    /// `(-0.309, 0.878, -0.367)`; the eye sits `distance` behind the forward axis.
    #[test]
    fn the_lab_camera_matches_the_live_matrices() {
        let cam = LabCam {
            pitch: 0.5,
            yaw: 0.7,
            dist: 5.0,
        };
        let (eye, up) = cam.eye_up();
        let forward = -eye / 5.0;
        assert!(
            (forward - Vec3::new(0.565, 0.479, 0.671)).length() < 0.002,
            "{forward:?}"
        );
        assert!(
            (up - Vec3::new(-0.309, 0.878, -0.367)).length() < 0.002,
            "{up:?}"
        );
        let (eye, up) = LabCam::START.eye_up();
        assert_eq!((eye, up), (Vec3::new(0.0, 0.0, -5.0), Vec3::Y));
    }

    /// The Paint Shop's picker and slider: the live swatch was `(161, 176, 25)` with the target at
    /// 0.184 / 0.253 of the picker and the thumb at 0.4.
    #[test]
    fn the_paint_swatch_is_the_pickers_hue_and_saturation_at_the_sliders_lightness() {
        let color = PaintColor::default();
        let [r, g, b] = color.picked();
        assert!((i32::from(r) - 161).abs() <= 3, "{r}");
        assert!((i32::from(g) - 176).abs() <= 3, "{g}");
        assert!((i32::from(b) - 25).abs() <= 3, "{b}");
    }
}
