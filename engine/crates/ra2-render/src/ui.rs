//! The shell's screens, drawn.
//!
//! `ra2ui` turns a `.gib` file into rectangles; this turns those rectangles into
//! quads and draws them with the GPU. The scene is built on the CPU and kept
//! away from the GPU so it can be checked on its own, and so the same draw list
//! could feed any backend.
//!
//! ## What a quad is
//!
//! One textured rectangle: where it goes, which part of which texture it takes,
//! what colour multiplies it, and what it is clipped to. The rules are the ones
//! `tools/ui_compose.py` established against the corpus, restated here:
//!
//! * **A screen is drawn in `ChildOrder` ascending**, ties by file order
//!   ([`ra2ui::draw_order`]), because a background is 1 and the logo group 3.
//! * **An image's file is already cropped to its cell**, so the file is sampled
//!   0..1 and the four numbers in the layout only say whether the thing is a
//!   cell, a tile or a whole-page image. A tile is a repeat and is not drawn
//!   here yet.
//! * **`_Alpha false`** means the art's own alpha is ignored — the trough and
//!   the arrows are drawn that way.
//! * **Text is a tinted glyph page**: the face's coverage becomes the quad's
//!   alpha and the label's `_ForeColor` its colour, which is what the composite
//!   does by blending towards the tint by the coverage.
//! * **`Placement::clip`** is the scissor rectangle.
//!
//! ## What is not drawn yet
//!
//! `Lines` borders, `Glyph` (a face character drawn as a widget of its own),
//! `Proxy` (a file drawn in place of the widget) and `Placeholder` (a slot for a
//! screen). The shell's own chrome — every `Button`'s art — is not in the data
//! at all: its four state images carry a blank `_Filename` and only
//! `Selection.tga` and the `UI_Scroll*` family are in `RA2.cfz` to go on, so a
//! button draws its caption and nothing else. Each is a hole with a name.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bytemuck::{Pod, Zeroable};
use ra2fmt::giff::Font;
use ra2ui::{Frame, Placement};

use crate::resolve_ci;

/// A texture the draw list can name: the game's own art, a face's glyph page, or
/// the single white pixel a fill uses.
pub struct UiTexture {
    pub label: String,
    pub width: u32,
    pub height: u32,
    /// RGBA8, top row first.
    pub rgba: Vec<u8>,
}

/// One rectangle to draw, in draw order.
#[derive(Debug, Clone, PartialEq)]
pub struct UiQuad {
    pub rect: Frame,
    /// The four corners — top-left, top-right, bottom-right, bottom-left — when
    /// the quad is not axis-aligned. A `Lines` segment is the one thing that
    /// needs them; everything else leaves this `None` and draws `rect`.
    pub corners: Option<[[f32; 2]; 4]>,
    /// The source rect in its texture, normalised.
    pub uv: [f32; 4],
    pub color: [f32; 4],
    /// Index into [`UiScene::textures`].
    pub texture: usize,
    /// `_Alpha`: when false the art's own alpha is ignored.
    pub alpha: bool,
    /// What this quad is clipped to, already intersected with everything above
    /// it. `None` when nothing clips.
    pub clip: Option<Frame>,
}

/// A screen's draw list: its size, the textures it needs, and its quads in order.
pub struct UiScene {
    pub size: [u32; 2],
    pub textures: Vec<UiTexture>,
    pub quads: Vec<UiQuad>,
    /// The images the layout names that are not in the game at all.
    pub missing: Vec<String>,
    /// The quads a repeated pattern would have drawn, and are left out.
    pub tiles: Vec<String>,
}

/// A screen's art and faces, and the textures they have been loaded into.
pub struct UiAssets {
    /// The game's own root, where a component's preview art lives — the
    /// definitions name it `Components\ztek\ztek_preview.bmp`, not under `UI`.
    game_root: PathBuf,
    ui_root: PathBuf,
    archive: Option<ra2fmt::cfl::Cfl>,
    faces: ra2ui::Fonts,
    textures: Vec<UiTexture>,
    /// Name to texture, `None` once a name is known not to resolve.
    index: HashMap<String, Option<usize>>,
    missing: Vec<String>,
    tiles: Vec<String>,
}

impl UiAssets {
    /// Load a game's faces and prepare to resolve its art.
    pub fn open(game: &Path) -> Result<UiAssets, Box<dyn std::error::Error>> {
        let ui_root = game.join("UI");
        let game_root = game.to_path_buf();
        let archive = ra2fmt::cfl::Cfl::open_path(&game.join("RA2.cfz")).ok();
        let faces = ra2ui::Fonts::load(&game.join("RA2.cfz"))?;
        let mut assets = UiAssets {
            ui_root,
            game_root,
            archive,
            faces,
            textures: vec![UiTexture {
                label: "white".into(),
                width: 1,
                height: 1,
                rgba: vec![255, 255, 255, 255],
            }],
            index: HashMap::new(),
            missing: Vec::new(),
            tiles: Vec::new(),
        };
        assets.index.insert("white".into(), Some(0));
        Ok(assets)
    }

    pub fn faces(&self) -> &ra2ui::Fonts {
        &self.faces
    }

    /// The white pixel every fill is drawn with.
    pub fn white(&self) -> usize {
        0
    }

    /// Build the draw list for a placed screen.
    pub fn scene(&mut self, roots: &[Placement<'_>], screen: [u32; 2]) -> UiScene {
        let mut quads = Vec::new();
        for (p, parent) in ra2ui::draw_order_parented(roots) {
            if !p.widget.visible() {
                continue;
            }
            let clip = p.clip;
            if clip.is_some_and(|c| c.w <= 0.0 || c.h <= 0.0) {
                continue;
            }
            if p.widget.kind == ra2fmt::gib::WidgetKind::Text {
                self.text_quads(p, clip, &mut quads);
                continue;
            }
            if p.widget.kind == ra2fmt::gib::WidgetKind::Glyph {
                self.glyph_quads(p, clip, &mut quads);
                continue;
            }
            if p.widget.kind == ra2fmt::gib::WidgetKind::Lines {
                lines_quads(p, parent, clip, &mut quads);
                continue;
            }
            if p.widget.kind == ra2fmt::gib::WidgetKind::Rectangle {
                if let Some(fill) = rectangle_quad(p, clip) {
                    quads.push(fill);
                }
            }
            if p.widget.kind == ra2fmt::gib::WidgetKind::Image {
                // An `Image` with a blank `_Filename` is a **colour fill**: the
                // game draws its `_ForeColor` over its frame, and the shell uses
                // that for nearly every panel — the match-results title bar, a
                // button's state backgrounds, and 223 of the 295 `Background`
                // images (the rest name a texture). It is not art the loader
                // failed to find, and skipping it is what left black text on the
                // clear colour.
                if let Some(fill) = image_fill_quad(p, parent, clip) {
                    quads.push(fill);
                    continue;
                }
            }
            if let Some(quad) = self.art_quad(p, parent, clip) {
                quads.push(quad);
            }
        }
        UiScene {
            size: screen,
            textures: std::mem::take(&mut self.textures),
            quads,
            missing: std::mem::take(&mut self.missing),
            tiles: std::mem::take(&mut self.tiles),
        }
    }

    /// The quad a widget's `_Filename` draws, if it names art this build can
    /// load and is not a repeated pattern.
    fn art_quad(
        &mut self,
        p: &Placement<'_>,
        parent: Option<&Placement<'_>>,
        clip: Option<Frame>,
    ) -> Option<UiQuad> {
        let file = ra2ui::art_for(p.widget, parent.map(|q| q.widget))?;
        let uv = p.widget.uv().unwrap_or([0.0, 0.0, 1.0, 1.0]);
        // A rect that runs past one page is a *tile*: a pattern to repeat. The
        // rect is the authoring size over the texture — `metaldots_pattern`'s
        // 341-pixel box is 5.32813 x 64 — so the pattern's period is the
        // texture's own size and the frame says how much of it to draw. (Where
        // the runtime has resized the widget since — a scroll trough whose track
        // is 973 pixels and whose rect says 4.25 pages of a 16-pixel pattern —
        // the natural size is the reading that looks like a trough.)
        let tiled = uv[2] > 1.0001 || uv[3] > 1.0001 || uv[0] < -0.0001 || uv[1] < -0.0001;
        let texture = self.texture(file)?;
        if tiled {
            self.tiles.push(file.to_string());
            let (tw, th) = (
                self.textures[texture].width.max(1) as f32,
                self.textures[texture].height.max(1) as f32,
            );
            return Some(UiQuad {
                rect: p.frame,
                corners: None,
                uv: [0.0, 0.0, p.frame.w / tw, p.frame.h / th],
                color: p.widget.fore_color().unwrap_or([1.0, 1.0, 1.0, 1.0]),
                texture,
                alpha: p
                    .widget
                    .field("_Alpha")
                    .and_then(|f| f.flag())
                    .unwrap_or(true),
                clip,
            });
        }
        let alpha = p
            .widget
            .field("_Alpha")
            .and_then(|f| f.flag())
            .unwrap_or(true);
        let color = p.widget.fore_color().unwrap_or([1.0, 1.0, 1.0, 1.0]);
        // The file is already cropped to its cell, so it is sampled 0..1 and
        // the rect only decided whether this *is* a cell. `mainmenu/ra2_logo.tga`
        // is 512x76 and its rect starts at u0 = 0.15625: sampling at the rect
        // draws the middle of the logo.
        //
        // An `Image` also states its own `_Size`, which is the size the art is
        // *drawn* at when it differs from the box the layout gave the widget:
        // a scroll bar's thumb is `Size 1 1 1 1` inside its track and `_Size
        // 16 0 30 0`. (A tile ignores it: a pattern fills its frame.)
        let rect = match stated_size(p.widget.field("_Size")) {
            Some((w, h)) => Frame::new(p.frame.x, p.frame.y, w, h),
            None => p.frame,
        };
        Some(UiQuad {
            rect,
            corners: None,
            uv: [0.0, 0.0, 1.0, 1.0],
            color,
            texture,
            alpha,
            clip,
        })
    }

    /// The quads a label draws: one per glyph's cell, tinted by `_ForeColor`.
    fn text_quads(&mut self, p: &Placement<'_>, clip: Option<Frame>, out: &mut Vec<UiQuad>) {
        let (Some(text), Some(face)) = (p.widget.text(), p.widget.font()) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let Some(font) = self.faces.get(face).cloned() else {
            return;
        };
        let Some(texture) = self.glyph_page(face, &font) else {
            return;
        };
        let color = p.widget.fore_color().unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let (pw, ph) = (font.page_width as f32, font.page_height as f32);
        for glyph in ra2ui::text_run(&font, p.frame, text) {
            if glyph.cell[2] <= 0 || glyph.cell[3] <= 0 {
                continue;
            }
            out.push(UiQuad {
                rect: glyph.rect,
                corners: None,
                // The face's page is stored **bottom-up only**. `v` is swapped
                // (the page's first row is the glyph's last); `u` is not: flipped
                // vertically, the page's row reads `E F G H I J K L M N O P Q R
                // S T U` upright and in order, so swapping `u` would mirror every
                // glyph. It did, for a while — see the module note in
                // `ra2fmt::giff`.
                uv: [
                    glyph.cell[0] as f32 / pw,
                    (glyph.cell[1] + glyph.cell[3]) as f32 / ph,
                    (glyph.cell[0] + glyph.cell[2]) as f32 / pw,
                    glyph.cell[1] as f32 / ph,
                ],
                color,
                texture,
                alpha: true,
                clip,
            });
        }
    }

    /// The quads a `Glyph` draws: a paragraph in the box its `_Margin` states.
    ///
    /// A `Glyph` is the shell's paragraph widget — the bot lab's help text, the
    /// credits, an arena's description — and the binary's own writer is what
    /// says so: its `Glyph` serialiser emits `Compositor`, the box's own
    /// `_Size 1` and then one `_Margin x w h` per entry, which the shipped files
    /// write on one line as `Margins _Size 1`. The widget's layout box is
    /// `Position 0 0 0 0` `Size 0 0 0 0` in all forty — the paragraph is placed
    /// by its margin against whatever holds it — so the box is `_Margin` at the
    /// widget's own frame origin.
    fn glyph_quads(&mut self, p: &Placement<'_>, clip: Option<Frame>, out: &mut Vec<UiQuad>) {
        let (Some(text), Some(face)) = (p.widget.text(), p.widget.font()) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let Some(box_) = glyph_box(p) else {
            return;
        };
        let Some(font) = self.faces.get(face).cloned() else {
            return;
        };
        let Some(texture) = self.glyph_page(face, &font) else {
            return;
        };
        let color = p.widget.fore_color().unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let (pw, ph) = (font.page_width as f32, font.page_height as f32);
        for glyph in ra2ui::paragraph_run(&font, box_, text, box_.w) {
            if glyph.cell[2] <= 0 || glyph.cell[3] <= 0 {
                continue;
            }
            out.push(UiQuad {
                rect: glyph.rect,
                corners: None,
                // The page is stored bottom-up only: swap `v`, not `u`. See
                // `text_quads`.
                uv: [
                    glyph.cell[0] as f32 / pw,
                    (glyph.cell[1] + glyph.cell[3]) as f32 / ph,
                    (glyph.cell[0] + glyph.cell[2]) as f32 / pw,
                    glyph.cell[1] as f32 / ph,
                ],
                color,
                texture,
                alpha: true,
                clip,
            });
        }
    }

    /// A face's page as a texture: white, with the coverage in the alpha, which
    /// is what makes a glyph quad a tinted coverage.
    fn glyph_page(&mut self, name: &str, font: &Font) -> Option<usize> {
        let key = format!("#{name}");
        if let Some(found) = self.index.get(&key) {
            return *found;
        }
        // `Font::page` and `Glyph::rect` already agree: the ink for a glyph
        // sits at the rows the rect names, first row first. (Measured: drawn
        // the other way up, an `M` comes out an `M` mirrored, which is the `V`
        // that `MULTIPLAYER` used to read as.)
        let mut rgba = Vec::with_capacity(font.page.len() * 4);
        for i in 0..font.page.len() {
            rgba.extend_from_slice(&[255, 255, 255, font.page[i]]);
        }
        let index = self.textures.len();
        self.textures.push(UiTexture {
            label: key.clone(),
            width: font.page_width,
            height: font.page_height,
            rgba,
        });
        self.index.insert(key, Some(index));
        Some(index)
    }

    /// Resolve an art name to a texture: the loose tree first, then the archive.
    fn texture(&mut self, relative: &str) -> Option<usize> {
        let key = relative
            .replace('\\', "/")
            .trim_start_matches("./")
            .to_string();
        if let Some(found) = self.index.get(&key) {
            return *found;
        }
        let mut data = resolve_ci(&self.ui_root, &key).and_then(|p| std::fs::read(p).ok());
        if data.is_none() {
            // Not under `UI`: a component's preview is named from the game's own
            // root (`Components\ztek\ztek_preview.bmp`) and lives there.
            data = resolve_ci(&self.game_root, &key).and_then(|p| std::fs::read(p).ok());
        }
        if data.is_none() {
            let stem = key.rsplit('/').next().unwrap_or(&key);
            data = self.archive.as_ref().and_then(|c| c.get(stem));
        }
        let decoded = data.as_deref().and_then(decode_art);
        let index = decoded.as_ref().map(|(rgba, w, h)| {
            let index = self.textures.len();
            self.textures.push(UiTexture {
                label: key.clone(),
                width: *w,
                height: *h,
                rgba: rgba.clone(),
            });
            index
        });
        if index.is_none() {
            self.missing.push(key.clone());
        }
        self.index.insert(key, index);
        index
    }
}

/// A `Glyph`'s paragraph box: the `_Margin` triple `x w h`, hung from the
/// widget's own frame origin.
fn glyph_box(p: &Placement<'_>) -> Option<Frame> {
    let m = p.widget.field("_Margin")?.numbers();
    if m.len() < 3 {
        return None;
    }
    Some(Frame::new(p.frame.x + m[0], p.frame.y, m[1], m[2]))
}

/// The quads a `Lines` widget draws.
///
/// `Points n` followed by `Point x xf y yf` twice per segment — the pairs are
/// the segments, which is what the corpus settles: `controls_reminder.gib`
/// repeats `(62,0)` and `(62,70)` so that the four pairs are the four sides of
/// a rectangle, and `selectbot_computer.gib`'s visible one insets the box by
/// two pixels at a `Width` of four, which only reads as a rectangle when the
/// list is cut into pairs. A `xf`/`yf` of 1 makes the amount a fraction of the
/// frame the polyline sits in — the widget's own `Size` is `0 0 0 0` on all 77
/// of them, and the four that state a fraction (`wiring.gib`'s `1 1 0 1`) are
/// the borders of the area around them.
fn lines_quads(
    p: &Placement<'_>,
    parent: Option<&Placement<'_>>,
    clip: Option<Frame>,
    out: &mut Vec<UiQuad>,
) {
    let frame = p.frame;
    let outer = parent.map(|q| q.frame).unwrap_or(frame);
    let points: Vec<[f32; 2]> = p
        .widget
        .fields_named("Point")
        .filter_map(|f| {
            let n = f.numbers();
            if n.len() < 4 {
                return None;
            }
            let x = if n[1] != 0.0 {
                frame.x + n[0] * outer.w
            } else {
                frame.x + n[0]
            };
            let y = if n[3] != 0.0 {
                frame.y + n[2] * outer.h
            } else {
                frame.y + n[2]
            };
            Some([x, y])
        })
        .collect();
    if points.len() < 2 {
        return;
    }
    // `Width 0` is the thinnest line the widget has, which is one pixel.
    let width = p
        .widget
        .field("Width")
        .and_then(|f| f.number())
        .unwrap_or(0.0)
        .max(1.0);
    let color = p
        .widget
        .field("Color")
        .and_then(|f| f.four())
        .unwrap_or([1.0, 1.0, 1.0, 1.0]);
    if color[3] <= 0.0 {
        return;
    }
    for [a, b] in points.as_chunks::<2>().0 {
        let (a, b) = (*a, *b);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if len <= 0.0 {
            continue;
        }
        let (nx, ny) = (-dy / len * width / 2.0, dx / len * width / 2.0);
        let corners = [
            [a[0] + nx, a[1] + ny],
            [b[0] + nx, b[1] + ny],
            [b[0] - nx, b[1] - ny],
            [a[0] - nx, a[1] - ny],
        ];
        let rect = Frame::new(
            corners.iter().map(|c| c[0]).fold(f32::INFINITY, f32::min),
            corners.iter().map(|c| c[1]).fold(f32::INFINITY, f32::min),
            corners
                .iter()
                .map(|c| c[0])
                .fold(f32::NEG_INFINITY, f32::max)
                - corners.iter().map(|c| c[0]).fold(f32::INFINITY, f32::min),
            corners
                .iter()
                .map(|c| c[1])
                .fold(f32::NEG_INFINITY, f32::max)
                - corners.iter().map(|c| c[1]).fold(f32::INFINITY, f32::min),
        );
        out.push(UiQuad {
            rect,
            corners: Some(corners),
            uv: [0.0, 0.0, 1.0, 1.0],
            color,
            texture: 0, // white
            alpha: true,
            clip,
        });
    }
}

/// A `Rectangle`'s fill, when it is filled and not transparent.
fn rectangle_quad(p: &Placement<'_>, clip: Option<Frame>) -> Option<UiQuad> {
    let filled = p
        .widget
        .field("Filled")
        .and_then(|f| f.flag())
        .unwrap_or(false);
    if !filled {
        return None;
    }
    let color = p
        .widget
        .field("FillColor")
        .and_then(|f| f.four())
        .unwrap_or([1.0, 1.0, 1.0, 1.0]);
    if color[3] <= 0.0 {
        return None;
    }
    Some(UiQuad {
        rect: p.frame,
        corners: None,
        uv: [0.0, 0.0, 1.0, 1.0],
        color,
        texture: 0, // white
        alpha: true,
        clip,
    })
}

/// A blank-named `Image`'s fill: the game draws `_ForeColor` over the widget's
/// frame.
///
/// An `Image` that names no file is not missing art — it is a solid colour, and
/// the shipped screens use it for every panel they draw themselves: a button's
/// state background, the semitransparent bar behind the match-results title, and
/// 223 of the corpus's 295 `Background` images. The frame is the widget's own
/// `Size`, not `_Size`: `_Size` is the size the art was authored at, and
/// `hq_league.gib`'s full-screen background is authored at 640×480 in a
/// 1280×1005 frame.
///
/// The one blank-named image that is *art* is a selection bar's, and
/// [`ra2ui::art_for`] names it; that case falls through to the textured path.
fn image_fill_quad(
    p: &Placement<'_>,
    parent: Option<&Placement<'_>>,
    clip: Option<Frame>,
) -> Option<UiQuad> {
    let file = p.widget.filename()?;
    if !file.is_empty() {
        return None;
    }
    if ra2ui::art_for(p.widget, parent.map(|q| q.widget)).is_some() {
        return None;
    }
    let color = p.widget.fore_color().unwrap_or([1.0, 1.0, 1.0, 1.0]);
    if color[3] <= 0.0 {
        return None;
    }
    Some(UiQuad {
        rect: p.frame,
        corners: None,
        uv: [0.0, 0.0, 1.0, 1.0],
        color,
        texture: 0, // white
        alpha: true,
        clip,
    })
}

/// Decode art. A `.bmp` or `.tga` the image crate can read, or nothing.
/// The `_Size` the editor writes when the designer never gave a piece of art a
/// size: the design canvas itself, 1280×1005. **Eighteen fields in fourteen
/// files** carry it — `teamhq/event_cell.gib` and the three `eventcell_*.gib`
/// among them — and a 140×80 event logo drawn at the canvas would be stretched
/// across the screen. The game draws such art in the box the layout gave it, so
/// the canvas value is read as *unset*.
///
/// Anything else is a real size and overrides the box: a scroll thumb's
/// `16 0 30 0` inside a 973-pixel track, an armour icon's `88 0 72 0`.
const UNSET_SIZE: [f32; 2] = [1280.0, 1005.0];

/// The size a widget's `_Size` states, or nothing when it states the editor's
/// unset.
fn stated_size(field: Option<&ra2fmt::gib::Field>) -> Option<(f32, f32)> {
    let axes = field.and_then(ra2fmt::gib::Axes::parse)?;
    if axes.first.fraction || axes.second.fraction {
        return None;
    }
    let size = (axes.first.amount, axes.second.amount);
    (size != (UNSET_SIZE[0], UNSET_SIZE[1])).then_some(size)
}

fn decode_art(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    crate::decode_image(bytes)
}

// ---------------------------------------------------------------------------
// the GPU half
// ---------------------------------------------------------------------------

const UI_SHADER: &str = r#"
struct VsIn {
  @location(0) pos: vec2<f32>,
  @location(1) uv: vec2<f32>,
  @location(2) color: vec4<f32>,
  @location(3) use_alpha: f32,
};
struct VsOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) color: vec4<f32>,
  @location(2) use_alpha: f32,
};

@vertex
fn vs_main(i: VsIn) -> VsOut {
  var o: VsOut;
  o.clip = vec4<f32>(i.pos, 0.0, 1.0);
  o.uv = i.uv;
  o.color = i.color;
  o.use_alpha = i.use_alpha;
  return o;
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
  let c = textureSample(tex, samp, i.uv);
  // `_Alpha false` ignores the art's own alpha, which is how the game draws a
  // scroll trough and its arrows.
  let a = mix(i.color.a, c.a * i.color.a, i.use_alpha);
  return vec4<f32>(c.rgb * i.color.rgb, a);
}
"#;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct UiVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    use_alpha: f32,
}

/// Draws a [`UiScene`] into an offscreen texture.
pub struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    format: wgpu::TextureFormat,
}

impl UiRenderer {
    pub fn new(device: &wgpu::Device) -> UiRenderer {
        // Not sRGB: the composite this is checked against blends bytes, and the
        // art is already 8-bit.
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui-shader"),
            source: wgpu::ShaderSource::Wgsl(UI_SHADER.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<UiVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        // Nearest: the composite picks one source pixel per destination pixel,
        // and a filtering sampler would blur the picture away from it. Repeating,
        // because that is what a tile's uv asks for; every cell and every glyph
        // has a uv inside 0..1, so nothing else notices.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui-sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        UiRenderer {
            pipeline,
            bgl,
            sampler,
            format,
        }
    }

    /// Draw a scene and read it back as RGBA8, top row first.
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &UiScene,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let (width, height) = (scene.size[0].max(1), scene.size[1].max(1));
        let textures: Vec<wgpu::BindGroup> = scene
            .textures
            .iter()
            .map(|t| bind_texture(device, queue, &self.bgl, &self.sampler, t))
            .collect();
        // Group consecutive quads that share a texture and a clip: one draw and
        // one scissor each.
        let mut batches: Vec<(usize, [u32; 4], Vec<UiVertex>)> = Vec::new();
        for quad in &scene.quads {
            let Some(scissor) = scissor_of(quad.clip, width, height) else {
                continue;
            };
            let push = match batches.last_mut() {
                Some((texture, clip, verts)) if *texture == quad.texture && *clip == scissor => {
                    verts
                }
                _ => {
                    batches.push((quad.texture, scissor, Vec::new()));
                    &mut batches.last_mut().unwrap().2
                }
            };
            push.extend_from_slice(&quad_vertices(quad, width, height));
        }
        let gpu: Vec<(usize, [u32; 4], wgpu::Buffer, u32)> = batches
            .iter()
            .map(|(texture, clip, verts)| {
                let buffer = wgpu::util::DeviceExt::create_buffer_init(
                    device,
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("ui-verts"),
                        contents: bytemuck::cast_slice(verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    },
                );
                (*texture, *clip, buffer, verts.len() as u32)
            })
            .collect();

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui-color"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("ui") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Black, opaque: the composite starts the same way.
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            for (texture, clip, buffer, count) in &gpu {
                pass.set_scissor_rect(clip[0], clip[1], clip[2], clip[3]);
                pass.set_bind_group(0, &textures[*texture], &[]);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..*count, 0..1);
            }
        }

        let row = (width * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui-readback"),
            size: (row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));

        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let data = slice.get_mapped_range()?;
        let mut out = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            let start = (y * row) as usize;
            out.extend_from_slice(&data[start..start + (width * 4) as usize]);
        }
        drop(data);
        buffer.unmap();
        Ok(out)
    }
}

fn bind_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    texture: &UiTexture,
) -> wgpu::BindGroup {
    let size = wgpu::Extent3d {
        width: texture.width.max(1),
        height: texture.height.max(1),
        depth_or_array_layers: 1,
    };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(&texture.label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &texture.rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(texture.width.max(1) * 4),
            rows_per_image: Some(texture.height.max(1)),
        },
        size,
    );
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ui-bind"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// The four corners of a quad, in clip space, with its source rect.
fn quad_vertices(quad: &UiQuad, width: u32, height: u32) -> [UiVertex; 6] {
    let to_clip = |x: f32, y: f32| [x / width as f32 * 2.0 - 1.0, 1.0 - y / height as f32 * 2.0];
    let [u0, v0, u1, v1] = quad.uv;
    let use_alpha = f32::from(u8::from(quad.alpha));
    let vertex = |x: f32, y: f32, u: f32, v: f32| UiVertex {
        pos: to_clip(x, y),
        uv: [u, v],
        color: quad.color,
        use_alpha,
    };
    let [tl, tr, br, bl] = quad.corners.unwrap_or([
        [quad.rect.x, quad.rect.y],
        [quad.rect.right(), quad.rect.y],
        [quad.rect.right(), quad.rect.bottom()],
        [quad.rect.x, quad.rect.bottom()],
    ]);
    [
        vertex(tl[0], tl[1], u0, v0),
        vertex(tr[0], tr[1], u1, v0),
        vertex(br[0], br[1], u1, v1),
        vertex(tl[0], tl[1], u0, v0),
        vertex(br[0], br[1], u1, v1),
        vertex(bl[0], bl[1], u0, v1),
    ]
}

/// The scissor rectangle a clip asks for, clamped to the target, or nothing when
/// there is nothing left of it.
fn scissor_of(clip: Option<Frame>, width: u32, height: u32) -> Option<[u32; 4]> {
    let Some(clip) = clip else {
        return Some([0, 0, width, height]);
    };
    let x = clip.x.max(0.0).floor();
    let y = clip.y.max(0.0).floor();
    let right = clip.right().min(width as f32).ceil();
    let bottom = clip.bottom().min(height as f32).ceil();
    if right <= x || bottom <= y {
        return None;
    }
    Some([x as u32, y as u32, (right - x) as u32, (bottom - y) as u32])
}

#[cfg(test)]
mod tests {
    use ra2fmt::gib::Field;

    fn size(raw: &str) -> Option<(f32, f32)> {
        stated_size(Some(&Field {
            name: Some("_Size".into()),
            raw: raw.to_string(),
        }))
    }

    /// A `_Size` is the art's own size, and the canvas value is the editor's
    /// unset rather than a size.
    #[test]
    fn the_canvas_size_is_not_a_size() {
        assert_eq!(size("16 0 30 0"), Some((16.0, 30.0)));
        assert_eq!(size("88 0 72 0"), Some((88.0, 72.0)));
        assert_eq!(size("1280 0 1005 0"), None);
        // A fractional axis is not a pixel size either.
        assert_eq!(size("1 1 1 1"), None);
        assert_eq!(stated_size(None), None);
    }

    use super::*;

    fn game_dir() -> Option<PathBuf> {
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

    fn scene_of(game: &Path, screen: &str) -> UiScene {
        let text = std::fs::read_to_string(game.join("UI").join(screen)).unwrap();
        let gib = ra2fmt::gib::parse(&text).unwrap();
        let mut assets = UiAssets::open(game).unwrap();
        let placed: Vec<_> = ra2ui::resolve_with(&gib, [1280.0, 1005.0], Some(assets.faces()))
            .into_iter()
            .collect();
        assets.scene(&placed, [1280, 1005])
    }

    /// The long-label smear was the shipped cell, not the length. A character's
    /// cell is the same in a two-character label as in the 44-character help
    /// line: the parser now derives the cell from the page's own glyph run, so
    /// the cell is the glyph (its packed width) and not a slice of a different
    /// packing. A long label is not a different font or a different measurement.
    #[test]
    fn a_glyphs_cell_does_not_depend_on_the_label_length() {
        use ra2ui::Frame;
        let Some(game) = game_dir() else { return };
        let assets = UiAssets::open(&game).unwrap();
        let Some(font) = assets.faces().get("fg_mcn14") else {
            return;
        };
        let frame = Frame::new(40.0, 40.0, 0.0, 28.0);
        let short = ra2ui::text_run(font, frame, "At");
        let long = ra2ui::text_run(font, frame, "At the top of your screen there are buttons.");
        assert_eq!(short.len(), 2);
        assert!(long.len() > 30);
        // The same two characters, at the same pen, with the same cells. A long
        // label is not a different font or a different measurement.
        assert_eq!(short[0].cell, long[0].cell, "the 'A' cell");
        assert_eq!(short[1].cell, long[1].cell, "the 't' cell");
        assert_eq!(short[0].rect.x, long[0].rect.x);
        assert_eq!(short[1].rect.x, long[1].rect.x);
        // The cell is the page's glyph run: a real width inside the page, and
        // the same for both labels. It may be wider or narrower than the
        // advance — the page's ink is not the advance table.
        for c in ['A', 't', 'h', 'n'] {
            let glyph = font.glyph(c);
            let (_, _, w, _) = glyph.rect(font.page_width, font.page_height);
            assert!(
                w > 0 && w <= font.page_width as i32,
                "{c:?}: cell width {w}"
            );
        }
    }

    /// **The page is bottom-up only; the glyph quad swaps `v`, not `u`.** The
    /// page's row at y 233 flipped vertically reads `E F G H I J K L M N O P Q R
    /// S T U V` upright and in order, so a quad whose left edge samples the
    /// cell's right edge (`u0 > u1`) mirrors every letter. This pins the
    /// measured orientation: the left vertex's `u` is the cell's left, and only
    /// the vertical pair is swapped (the page's first row is the glyph's last).
    #[test]
    fn a_glyph_quad_swaps_v_and_not_u() {
        let Some(game) = game_dir() else { return };
        let scene = scene_of(&game, "help/help_ws_overview.gib");
        let quad = scene
            .quads
            .iter()
            .find(|q| q.texture != 0 && scene.textures[q.texture].label.starts_with('#'))
            .expect("a label's glyph quad");
        assert!(
            quad.uv[0] < quad.uv[2],
            "u must rise left to right (cell left {}, right {})",
            quad.uv[0],
            quad.uv[2]
        );
        assert!(
            quad.uv[1] > quad.uv[3],
            "v must fall top to bottom (cell bottom {}, top {})",
            quad.uv[1],
            quad.uv[3]
        );
    }

    /// The `+Background` attachment is drawn, and behind the widget it dresses:
    /// `common/matchresults.gib`'s window carries a 520×426 white 70% panel, and
    /// each result slot a translucent black body. The cell's own black text
    /// ("The Wrangler") lands *after* both, which is what makes it readable.
    ///
    /// The frame and colour are the attachment's own data, read off its body:
    /// `Position`/`Size` place it, and its image child's `_ForeColor` is the
    /// fill. No colour is invented.
    #[test]
    fn the_match_results_background_is_drawn_behind_its_cells() {
        use ra2fmt::gib::Block;
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let mut gib = ra2ui::workshop::load(&ui, "common/matchresults.gib").unwrap();
        let template = ra2ui::cells::template(&ui, ra2ui::results::TEMPLATE).unwrap();
        let results = [ra2ui::results::MatchResult {
            team_logo: "Teams\\orange_skull.bmp".into(),
            bot_preview: "exhibition\\sample_bot.bmp".into(),
            points: 248,
            bot_name: "The Wrangler".into(),
            team_name: "TEAM STINKBOMBS".into(),
            immobilized: false,
            winner: false,
        }];
        for block in gib.blocks.iter_mut() {
            if let Block::Widget(root) = block {
                ra2ui::results::fill(root, &template, &results);
                break;
            }
        }
        let mut assets = UiAssets::open(&game).unwrap();
        let placed: Vec<_> = ra2ui::resolve_with(&gib, [1280.0, 1005.0], Some(assets.faces()))
            .into_iter()
            .collect();
        let scene = assets.scene(&placed, [1280, 1005]);

        let panel = scene
            .quads
            .iter()
            .position(|q| {
                q.texture == 0
                    && q.rect.w == 520.0
                    && q.rect.h == 426.0
                    && q.color == [1.0, 1.0, 1.0, 0.7]
            })
            .expect("the window's white 70% background");
        let panel_rect = scene.quads[panel].rect;
        let text = scene
            .quads
            .iter()
            .position(|q| {
                q.texture != 0
                    && scene.textures[q.texture].label.starts_with('#')
                    && q.color == [0.0, 0.0, 0.0, 1.0]
                    && q.rect.y > panel_rect.y + 40.0
                    && q.rect.x > panel_rect.x
            })
            .expect("the cell's black bot name");
        let text_rect = scene.quads[text].rect;
        let cell = scene
            .quads
            .iter()
            .position(|q| {
                q.texture == 0
                    && q.rect.w >= text_rect.w
                    && q.rect.h >= text_rect.h
                    && q.rect.contains(&text_rect)
                    && q.color == [0.0, 0.0, 0.0, 0.15]
            })
            .expect("the result slot's translucent background");
        assert!(panel < cell, "the panel is behind the slot");
        assert!(cell < text, "the slot's background is behind its text");
    }

    /// The draw list a screen makes: an image's file is sampled whole, the
    /// cement is drawn before the logo, the labels are in the list, every quad
    /// is inside its clip, and every image the layout names resolves.
    #[test]
    fn the_main_menu_draws_its_background_before_its_logo() {
        let Some(game) = game_dir() else { return };
        let scene = scene_of(&game, "mainmenu/mainmenu.gib");

        assert!(scene.missing.is_empty(), "{:?}", scene.missing);
        assert!(
            scene.quads.len() > 40,
            "images and glyphs: {}",
            scene.quads.len()
        );
        // The labels are there: a run of glyph quads off a face's page, which
        // this names `#<face>`.
        let glyphs = scene
            .quads
            .iter()
            .filter(|q| scene.textures[q.texture].label.starts_with('#'))
            .count();
        assert!(glyphs > 30, "glyph quads: {glyphs}");
        // The cement is the screen, and the logo is drawn after it.
        let cement = scene
            .quads
            .iter()
            .position(|q| q.texture != 0 && q.rect.w >= 1280.0 && q.rect.h >= 1005.0)
            .expect("the cement background, screen sized");
        let logo = scene
            .quads
            .iter()
            .position(|q| q.texture != 0 && q.rect.w == 512.0 && q.rect.h == 76.0)
            .expect("the logo, 512x76");
        assert!(cement < logo, "the cement is drawn before the logo");
        // Every quad is inside its clip, and none is inside an empty one, and a
        // cell of art is sampled whole (the glyph pages are cells of their own).
        for quad in &scene.quads {
            if let Some(clip) = quad.clip {
                // The clip can be *smaller* than the quad — that is the whole
                // point of `ClipMode`, and a `ClipMode 2` widget is often
                // outside its parent — but never empty.
                assert!(clip.w > 0.0 && clip.h > 0.0);
                assert!(clip.x >= 0.0 && clip.y >= 0.0);
            }
            let [u0, v0, u1, v1] = quad.uv;
            assert!((-0.0001..=1.0001).contains(&u0) && (-0.0001..=1.0001).contains(&u1));
            assert!((-0.0001..=1.0001).contains(&v0) && (-0.0001..=1.0001).contains(&v1));
        }
        assert!(scene.textures.len() >= 10 && scene.textures.len() <= 24);
        assert_eq!(scene.textures[0].label, "white");
    }

    /// A screen whose art lives in the archive still resolves it, and a repeated
    /// pattern is named rather than stretched across the screen.
    /// A `Glyph` draws a wrapped paragraph, and a `Lines` widget draws its
    /// point pairs as segments.
    ///
    /// Neither is visible in the picture on the screens they ship in — the
    /// corpus's own tree leaves their background tiles as later roots, so the
    /// tile paints over them (see `ra2ui`'s note on `NumChildren`) — so the
    /// check is on the draw list: `credits.gib`'s two glyphs put glyph quads of
    /// the `fg_mcn14` page into the scene, and `selectbot_computer.gib`'s one
    /// visible `Lines` puts four segments, each carrying its own corners.
    #[test]
    fn a_glyph_draws_a_paragraph_and_a_lines_draws_segments() {
        let Some(game) = game_dir() else { return };

        let scene = scene_of(&game, "common/credits.gib");
        let face = scene
            .textures
            .iter()
            .position(|t| t.label == "#fg_mcn14")
            .expect("the paragraph face's page");
        let from_paragraph = scene
            .quads
            .iter()
            .filter(|q| q.texture == face && q.rect.h > 0.0)
            .count();
        assert!(
            from_paragraph > 200,
            "two glyphs' worth of lines: {from_paragraph}"
        );

        let scene = scene_of(&game, "exhibition/selectbot_computer.gib");
        let segments: Vec<_> = scene.quads.iter().filter(|q| q.corners.is_some()).collect();
        assert_eq!(
            segments.len(),
            4,
            "one visible Lines, eight points, four segments"
        );
        for s in &segments {
            assert!(s.uv == [0.0, 0.0, 1.0, 1.0], "a segment is a white fill");
            assert_eq!(s.color, [0.4, 0.3, 0.8, 1.0], "the file's own colour");
        }
    }

    #[test]
    fn a_tile_is_drawn_at_its_own_size() {
        let Some(game) = game_dir() else { return };
        let scene = scene_of(&game, "teamhq/hq_league.gib");
        assert!(
            scene.tiles.iter().any(|t| t.contains("ScrollBack")),
            "the trough is a tile: {:?}",
            scene.tiles
        );
        // A pattern repeats at its own size — a 16-pixel trough texture over a
        // 973-pixel track asks for about sixty copies — rather than being
        // stretched over the frame, which is a lie about what it looks like.
        let mut checked = 0;
        for quad in &scene.quads {
            let label = scene.textures[quad.texture].label.clone();
            if !scene.tiles.iter().any(|t| label.contains(t.as_str())) {
                continue;
            }
            checked += 1;
            let (tw, th) = (
                scene.textures[quad.texture].width as f32,
                scene.textures[quad.texture].height as f32,
            );
            assert!((quad.uv[2] - quad.rect.w / tw).abs() < 0.01, "{label}");
            assert!((quad.uv[3] - quad.rect.h / th).abs() < 0.01, "{label}");
            assert!(quad.uv[2] > 1.0 || quad.uv[3] > 1.0, "{label} repeats");
        }
        assert!(checked > 0, "the trough was drawn");
    }
}
