//! The Paint Shop's five effects (`Appearance`, `FUN_00423010`): the `Effect` dropdown
//! (`Color Fill`, `Texture Fill`, `Surface Layers`, `Edge Lines`, `Decals`, `FUN_004241f0`), the page
//! each one loads into `effect settings group` (`FUN_00424010`), and what a click on a panel of the
//! 3D view does with the page's choice (`FUN_00424670`).
//!
//! | effect | page | list | a click on a panel (`FUN_00424850` / `FUN_00424a80` / `FUN_00426130`) |
//! |---|---|---|---|
//! | Color Fill | `colorfill.gib` | none | the panel's rectangle in the picker's colour |
//! | Texture Fill | `texturefill.gib` | `Texture Library\*.bmp` | the texture stretched over the rectangle |
//! | Surface Layers | `texturefill.gib` | `Texture Library\*.tga` | the same, blended by the texture's alpha |
//! | Edge Lines | `edgelines.gib` | `Texture Library\edges\*.txt` | the edge strips of the panel's polygon |
//! | Decals | `decals.gib` | `Texture Library\decals\*.tga` + `*.bmp` | the decal at the point, kept inside the rectangle |
//!
//! The lists are the directory listings sorted as plain strings (`FUN_00429800`: `Galvanized_Metal.bmp`
//! before `aluminum_horiz.bmp`). *Paint Entire Chassis* / *Edge Entire Chassis* apply the choice to the
//! whole skin / every polygon. Each effect keeps the one undo copy of the skin (`FUN_00423ea0`).

use super::*;

use ra2_render::Vertex;
use ra2ui::workshop::skinfx::Image;

/// One of the five effects.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum Effect {
    #[default]
    ColorFill,
    TextureFill,
    SurfaceLayers,
    EdgeLines,
    Decals,
}

pub(super) const EFFECTS: [Effect; 5] = [
    Effect::ColorFill,
    Effect::TextureFill,
    Effect::SurfaceLayers,
    Effect::EdgeLines,
    Effect::Decals,
];

impl Effect {
    pub(super) fn label(self) -> &'static str {
        match self {
            Effect::ColorFill => "Color Fill",
            Effect::TextureFill => "Texture Fill",
            Effect::SurfaceLayers => "Surface Layers",
            Effect::EdgeLines => "Edge Lines",
            Effect::Decals => "Decals",
        }
    }

    /// The page loaded into `effect settings group` (Surface Layers reuses the texture page).
    fn page(self) -> &'static str {
        match self {
            Effect::ColorFill => "botlab/colorfill.gib",
            Effect::TextureFill | Effect::SurfaceLayers => "botlab/texturefill.gib",
            Effect::EdgeLines => "botlab/edgelines.gib",
            Effect::Decals => "botlab/decals.gib",
        }
    }

    fn index(self) -> usize {
        EFFECTS.iter().position(|e| *e == self).unwrap_or(0)
    }

    /// The page's list widget.
    fn list(self) -> Option<&'static str> {
        match self {
            Effect::ColorFill => None,
            Effect::TextureFill | Effect::SurfaceLayers => Some("texture list"),
            Effect::EdgeLines => Some("edge list"),
            Effect::Decals => Some("decal list"),
        }
    }

    /// The page's picture of the choice.
    fn picture(self) -> Option<&'static str> {
        match self {
            Effect::ColorFill => None,
            Effect::TextureFill | Effect::SurfaceLayers | Effect::Decals => {
                Some("selected texture img")
            }
            Effect::EdgeLines => Some("edge preview img"),
        }
    }
}

const ROW_PX: f32 = 18.0;

/// What the Paint Shop remembers between its effects.
#[derive(Clone, Debug)]
pub(super) struct EffectState {
    pub current: Effect,
    /// The page that is in `effect settings group` now.
    pub loaded: Option<Effect>,
    /// The selected row of each effect's list (the first on a fresh page).
    pub selected: [usize; 5],
    /// The first row each list shows.
    pub scroll: [usize; 5],
    /// The Decals page's size slider (`0 .. 1`, the thumb at the right on a fresh page).
    pub scale: f32,
    /// The decal's turn in radians (`+0x3ac`).
    pub angle: f32,
    /// The size slider's grab while its thumb is dragged.
    pub scale_drag: Option<f32>,
}

impl Default for EffectState {
    fn default() -> Self {
        EffectState {
            current: Effect::ColorFill,
            loaded: None,
            selected: [0; 5],
            scroll: [0; 5],
            scale: 1.0,
            angle: 0.0,
            scale_drag: None,
        }
    }
}

/// The listing of a directory: files with one of the extensions, sorted as plain strings.
fn listing(dir: &Path, exts: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| {
            Path::new(n)
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| exts.iter().any(|e| x.eq_ignore_ascii_case(e)))
        })
        .collect();
    names.sort();
    names
}

/// A picture file as the UI draws pictures: registered with the assets under a content key.
fn file_preview(path: &Path) -> Option<ra2ui::preview::Preview> {
    let image = Image::load(path)?;
    let mut rgba = Vec::with_capacity(image.px.len() * 4);
    for p in &image.px {
        for c in p {
            rgba.push((c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        }
    }
    Some(ra2ui::preview::Preview {
        width: image.width as u32,
        height: image.height as u32,
        rgba,
    })
}

impl Builder {
    fn library_dir(&self) -> PathBuf {
        self.game.join("Texture Library")
    }

    /// The rows of an effect's list.
    pub(super) fn effect_rows(&self, effect: Effect) -> Vec<String> {
        match effect {
            Effect::ColorFill => Vec::new(),
            Effect::TextureFill => listing(&self.library_dir(), &["bmp"]),
            Effect::SurfaceLayers => listing(&self.library_dir(), &["tga"]),
            Effect::Decals => listing(&self.library_dir().join("decals"), &["tga", "bmp"]),
            Effect::EdgeLines => self
                .edge_styles()
                .into_iter()
                .map(|s| s.name.clone())
                .collect(),
        }
    }

    /// The edge styles in the list's order (the `.txt` files' names as plain strings).
    fn edge_styles(&self) -> Vec<ra2ui::workshop::library::EdgeStyle> {
        let mut styles = self.library.edges().to_vec();
        styles.sort_by(|a, b| a.path.file_name().cmp(&b.path.file_name()));
        styles
    }

    /// The effect dropdown's rows, once for the lab.
    pub(super) fn register_effect_dropdown(&mut self) {
        self.shell.set_dropdown_items(
            "effect dropdown",
            EFFECTS.iter().map(|e| e.label().to_string()).collect(),
        );
    }

    /// A row of the effect dropdown was picked.
    pub(super) fn effect_dropdown_pick(&mut self, text: &str) {
        if let Some(effect) = EFFECTS
            .iter()
            .find(|e| e.label().eq_ignore_ascii_case(text.trim()))
        {
            // every pick refills the page's list (`FUN_00426f80`, `FUN_00427b20`, `FUN_00428250`): the first row
            // is selected again and the list is at the top; the Decals page also puts its size slider back
            // to 1 (`FUN_00428230`).
            self.effect.current = *effect;
            self.effect.selected[effect.index()] = 0;
            self.effect.scroll[effect.index()] = 0;
            if *effect == Effect::Decals {
                self.effect.scale = 1.0;
            }
        }
    }

    /// Put the current effect's page in `effect settings group` and its state on the page.
    pub(super) fn refresh_effect_page(&mut self) {
        if self.shell.tab() != Some("paintshop button") {
            return;
        }
        self.register_effect_dropdown();
        let current = self.effect.current;
        if self.effect.loaded != Some(current) {
            let Ok(fragment) = workshop::load(&self.ui, current.page()) else {
                return;
            };
            if let Some(lab) = self.shell.lab_mut() {
                for block in lab.document.blocks.iter_mut() {
                    let ra2fmt::gib::Block::Widget(root) = block else {
                        continue;
                    };
                    if let Some(slot) = workshop::find_mut(root, "effect settings group") {
                        slot.children.clear();
                        workshop::load_into(root, &fragment, "effect settings group");
                    }
                }
            }
            self.effect.loaded = Some(current);
        }
        let rows = self.effect_rows(current);
        let sel = self.effect.selected[current.index()].min(rows.len().saturating_sub(1));
        self.effect.selected[current.index()] = sel;
        let scroll = self.effect.scroll[current.index()];
        let undo = self.build.paint().can_undo();
        let preview = self.effect_preview_key(current, sel, &rows);
        let thumb = self.effect.scale;
        let slider_frame = self.adjust_slider_frame("scale slider");
        let list_frame = current.list().and_then(|n| self.adjust_slider_frame(n));
        let Some(lab) = self.shell.lab_mut() else {
            return;
        };
        for block in lab.document.blocks.iter_mut() {
            let ra2fmt::gib::Block::Widget(root) = block else {
                continue;
            };
            if let Some(button) = workshop::find_mut(root, "undo button") {
                ra2ui::input::set_button_enabled(button, undo);
            }
            if let Some(dropdown) = workshop::find_mut(root, "effect dropdown") {
                if let Some(text) = dropdown
                    .children
                    .iter_mut()
                    .find(|c| c.kind == WidgetKind::TextBox)
                    .and_then(|b| b.children.iter_mut().find(|c| c.kind == WidgetKind::Text))
                {
                    ra2ui::input::set_field(text, "_Text", current.label());
                }
            }
            if let Some(name) = current.list() {
                if let Some(list) = workshop::find_mut(root, name) {
                    if ra2ui::input::row_count(list) != rows.len() {
                        ra2ui::input::text_rows(list, &rows);
                    }
                    // the list shows as many rows as its height holds (`Rows 6` is the file's own guess)
                    ra2ui::input::set_field(list, "_ViewRows", "15");
                    if !rows.is_empty() {
                        ra2ui::input::select_row(list, sel);
                    }
                    // The list scrolls by pixels (live `61d-scroll-end`: the last row ends at the bottom of the
                    // view and the first is cut): `scroll` is the pixel offset of the first row.
                    let view_h = list_frame.map(|f| f.h).unwrap_or(262.0);
                    ra2ui::input::scroll_rows(list, ((scroll as f32) / ROW_PX).round() as usize);
                    let first = (scroll as f32 / ROW_PX).floor() as i64;
                    for (ordinal, row) in list
                        .children
                        .iter_mut()
                        .filter(|c| c.kind == WidgetKind::Text)
                        .enumerate()
                    {
                        let ordinal = ordinal as i64;
                        let top = ordinal as f32 * ROW_PX - scroll as f32;
                        ra2ui::input::set_field(row, "Position", &format!("1 0 {} 0", top - 5.0));
                        let shown = top + ROW_PX > 0.0 && top < view_h && ordinal >= first - 1;
                        ra2ui::input::set_field(
                            row,
                            "Visible",
                            if shown { "true" } else { "false" },
                        );
                    }
                    if let Some(bar) = list.children.iter_mut().find(|c| c.name() == "Sel Bar") {
                        let top = sel as f32 * ROW_PX - scroll as f32;
                        ra2ui::input::set_field(bar, "Position", &format!("0 0 {top} 0"));
                        let shown = !rows.is_empty() && top + ROW_PX > 0.0 && top < view_h;
                        ra2ui::input::set_field(
                            bar,
                            "Visible",
                            if shown { "true" } else { "false" },
                        );
                    }
                }
            }
            if let (Some(name), Some(key)) = (current.picture(), &preview) {
                ra2ui::cells::bind_art(root, name, key);
            }
            if current == Effect::Decals {
                if let (Some(frame), Some(slider)) =
                    (slider_frame, workshop::find_mut(root, "scale slider"))
                {
                    if let Some(img) = slider
                        .children
                        .iter_mut()
                        .find(|c| c.kind == WidgetKind::Image)
                    {
                        let x = (thumb.clamp(0.0, 1.0) * (frame.w - ADJUST_THUMB).max(0.0)).trunc();
                        ra2ui::input::set_field(img, "Position", &format!("{x} 0 0 0"));
                    }
                }
            }
        }
        if current == Effect::ColorFill {
            self.sync_paint_widgets();
        }
    }

    /// The picture of the selected row: the texture itself, the edge style's preview bitmap, the
    /// decal; registered with the assets, its key returned.
    fn effect_preview_key(
        &mut self,
        effect: Effect,
        sel: usize,
        rows: &[String],
    ) -> Option<String> {
        let path = match effect {
            Effect::ColorFill => return None,
            Effect::TextureFill | Effect::SurfaceLayers => self.library_dir().join(rows.get(sel)?),
            Effect::Decals => self.library_dir().join("decals").join(rows.get(sel)?),
            Effect::EdgeLines => {
                let style = self.edge_styles().into_iter().nth(sel)?;
                self.library_dir()
                    .join("edges")
                    .join(style.preview.as_deref()?)
            }
        };
        let preview = file_preview(&path)?;
        let key = preview.key("paint");
        self.assets.add_preview(&key, &preview);
        Some(key)
    }

    /// A press on the effect page: a list row, its scroll bar, a Paint Entire button or the size
    /// slider. Returns whether the press was the page's.
    pub(super) fn effect_press(&mut self, x: f32, y: f32) -> bool {
        if self.tab() != Some("paintshop button") {
            return false;
        }
        let effect = self.effect.current;
        let placed = self.shell.placements();
        let chain_names: Vec<String> = ra2ui::input::hit(&placed, x, y)
            .map(|path| {
                chain(&placed, &path)
                    .iter()
                    .map(|p| p.widget.name().to_string())
                    .collect()
            })
            .unwrap_or_default();
        let has = |n: &str| chain_names.iter().any(|c| c == n);
        // the list: a row of text, or the bar over it
        if let Some(list_name) = effect.list() {
            if let Some(list) = placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == list_name)
            {
                let rows = ra2ui::input::row_count(list.widget);
                let row_h = ROW_PX;
                let max = ((rows as f32 * row_h) - list.frame.h).max(0.0) as usize;
                let top = self.effect.scroll[effect.index()];
                // the arrows move a row (live `61b`, `61d`, `61e`); a press on the track did nothing
                if let Some(part) = ra2ui::dropdown::scrollbar_part(list, x, y) {
                    use ra2ui::dropdown::BarPart;
                    let moved = match part {
                        BarPart::Up => top.saturating_sub(row_h as usize),
                        BarPart::Down => (top + row_h as usize).min(max),
                        BarPart::Track => top,
                    };
                    drop(placed);
                    self.effect.scroll[effect.index()] = moved;
                    let _ = self.refresh();
                    return true;
                }
                if list.frame.contains_point(x, y)
                    && (has(list_name) || has("Sel Bar") || has("High Bar"))
                {
                    let row = ((y - list.frame.y + top as f32) / row_h).floor().max(0.0) as usize;
                    if row < rows {
                        drop(placed);
                        self.effect.selected[effect.index()] = row;
                        self.status = format!("{} row {row}", effect.label());
                        let _ = self.refresh();
                        return true;
                    }
                }
            }
        }
        if has("paint entire") {
            drop(placed);
            let result = self.effect_entire();
            if let Err(e) = result {
                self.status = e;
            }
            let _ = self.refresh();
            return true;
        }
        if has("texture entire") || has("edge entire") {
            drop(placed);
            let result = self.effect_entire();
            if let Err(e) = result {
                self.status = e;
            }
            let _ = self.refresh();
            return true;
        }
        if effect == Effect::Decals {
            if let Some(frame) = placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == "scale slider")
                .map(|p| p.frame)
            {
                if frame.contains_point(x, y) {
                    drop(placed);
                    let grab = slider_grab(&frame, self.effect.scale, x);
                    self.effect.scale_drag = grab;
                    self.scale_to(x - grab.unwrap_or(0.0));
                    return true;
                }
            }
        }
        false
    }

    /// Move the size slider's thumb so its left edge is at `left` (the adjustment sliders' rule).
    pub(super) fn scale_to(&mut self, left: f32) {
        let Some(frame) = self.adjust_slider_frame("scale slider") else {
            return;
        };
        let travel = (frame.w - ADJUST_THUMB).max(1.0);
        self.effect.scale = (left - frame.x).trunc().clamp(0.0, travel) / travel;
        self.status = format!("decal size {:.3}", self.effect.scale);
        let _ = self.refresh();
    }

    /// The choice's texture file for the current effect.
    fn effect_image(&self) -> Result<Image, String> {
        let effect = self.effect.current;
        let rows = self.effect_rows(effect);
        let sel = self.effect.selected[effect.index()];
        let path = match effect {
            Effect::TextureFill | Effect::SurfaceLayers => {
                self.library_dir().join(rows.get(sel).ok_or("no texture")?)
            }
            Effect::Decals => self
                .library_dir()
                .join("decals")
                .join(rows.get(sel).ok_or("no decal")?),
            Effect::EdgeLines => {
                let style = self
                    .edge_styles()
                    .into_iter()
                    .nth(sel)
                    .ok_or("no edge style")?;
                self.library_dir().join("edges").join(&style.file)
            }
            Effect::ColorFill => return Err("no texture".into()),
        };
        Image::load(&path).ok_or_else(|| format!("cannot read {}", path.display()))
    }

    /// Paint Entire Chassis / Edge Entire Chassis.
    fn effect_entire(&mut self) -> Result<(), String> {
        let effect = self.effect.current;
        match effect {
            Effect::ColorFill => {
                let rgb = self.paint_color.rgb;
                self.build.paint_mut().color_all(rgb);
            }
            Effect::TextureFill | Effect::SurfaceLayers => {
                let image = self.effect_image()?;
                self.build.paint_mut().texture_all(&image);
            }
            Effect::EdgeLines => {
                let image = self.effect_image()?;
                let style = self
                    .edge_styles()
                    .into_iter()
                    .nth(self.effect.selected[effect.index()])
                    .ok_or("no edge style")?;
                self.build
                    .paint_mut()
                    .edge_all(&image, style.width, style.length_ratio);
            }
            Effect::Decals => return Ok(()),
        }
        self.status = format!("{}: the entire chassis", effect.label());
        Ok(())
    }

    /// A click on the 3D view's panel `group` at the skin point `uv` (pixels).
    pub(super) fn effect_apply(&mut self, group: usize, uv: [f32; 2]) -> Result<(), String> {
        let effect = self.effect.current;
        match effect {
            Effect::ColorFill => {
                let rgb = self.paint_color.rgb;
                self.build
                    .paint_mut()
                    .color_group(group, rgb)
                    .map_err(|e| e.to_string())?;
            }
            Effect::TextureFill | Effect::SurfaceLayers => {
                let image = self.effect_image()?;
                self.build
                    .paint_mut()
                    .texture_group(group, &image)
                    .map_err(|e| e.to_string())?;
            }
            Effect::EdgeLines => {
                let image = self.effect_image()?;
                let style = self
                    .edge_styles()
                    .into_iter()
                    .nth(self.effect.selected[effect.index()])
                    .ok_or("no edge style")?;
                // `FUN_00424a80` takes the width as an integer
                self.build
                    .paint_mut()
                    .edge_group(group, &image, style.width.trunc(), style.length_ratio)
                    .map_err(|e| e.to_string())?;
            }
            Effect::Decals => {
                let image = self.effect_image()?;
                self.build
                    .paint_mut()
                    .decal_on(
                        group,
                        &image,
                        uv,
                        80.0 * self.effect.scale,
                        self.effect.angle,
                    )
                    .map_err(|e| e.to_string())?;
            }
        }
        self.status = format!("{} on panel {group}", effect.label());
        Ok(())
    }

    /// The decal page's SHIFT / CTRL movement: SHIFT turns the decal by a tenth of a radian per step of
    /// the mouse (`FUN_00424c50`, wrapped at 36), CTRL grows it by 0.025 per step (`FUN_00424cc0`).
    pub(super) fn decal_move(&mut self, delta: f32) {
        if self.effect.current != Effect::Decals {
            return;
        }
        let steps = delta.trunc();
        if self.shift {
            let mut a = steps * 0.1 + self.effect.angle;
            if a < 0.0 {
                a += 36.0;
            } else if a > 36.0 {
                a -= 36.0;
            }
            self.effect.angle = a;
        } else if self.ctrl {
            self.effect.scale = (steps * 0.025 + self.effect.scale).clamp(0.0, 1.0);
        }
    }
}

/// A point of the chassis under the cursor: the panel, the hit triangle's points and their skin
/// coordinates, and the skin point.
#[derive(Clone, Debug)]
pub(super) struct Hit {
    pub group: usize,
    pub tri: [u32; 3],
    pub uvs: [[f32; 2]; 3],
    pub uv: [f32; 2],
}

impl Builder {
    /// Track the cursor over the chassis for the decal preview.
    pub(super) fn update_decal_hover(&mut self, x: f32, y: f32) {
        let on = self.tab() == Some("paintshop button")
            && self.effect.current == Effect::Decals
            && self.panel.preview.contains_point(x, y);
        let had = self.decal_hover.is_some();
        self.decal_hover = if on {
            self.pick_chassis_hit(x, y)
        } else {
            None
        };
        if on || had {
            // the 3D view is cached by revision
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// The decal's preview in the 3D view (`FUN_00424d80` / `FUN_00425020`): the decal's square, as it
    /// will be drawn on the skin, carried by the hit triangle's own skin-to-model map onto the plane of
    /// that triangle (so it runs on past the panel's edge), a hair above the surface.
    pub(super) fn decal_preview_items(&self) -> Vec<DrawItem> {
        let Some(hit) = &self.decal_hover else {
            return Vec::new();
        };
        if self.tab() != Some("paintshop button") || self.effect.current != Effect::Decals {
            return Vec::new();
        }
        let Some(chassis) = self.build.chassis() else {
            return Vec::new();
        };
        let Some(mesh) = chassis.mesh.as_ref() else {
            return Vec::new();
        };
        let p: Vec<Vec3> = hit
            .tri
            .iter()
            .filter_map(|i| mesh.points.get(*i as usize).map(|v| Vec3::from(*v)))
            .collect();
        if p.len() != 3 {
            return Vec::new();
        }
        let (e1, e2) = (p[1] - p[0], p[2] - p[0]);
        let normal = e1.cross(e2).normalize_or_zero();
        let (d1, d2) = (
            [hit.uvs[1][0] - hit.uvs[0][0], hit.uvs[1][1] - hit.uvs[0][1]],
            [hit.uvs[2][0] - hit.uvs[0][0], hit.uvs[2][1] - hit.uvs[0][1]],
        );
        let det = d1[0] * d2[1] - d1[1] * d2[0];
        if det.abs() < 1e-9 || normal == Vec3::ZERO {
            return Vec::new();
        }
        let to_model = |uv: [f32; 2]| -> Vec3 {
            let q = [uv[0] - hit.uvs[0][0], uv[1] - hit.uvs[0][1]];
            let a = (q[0] * d2[1] - q[1] * d2[0]) / det;
            let b = (d1[0] * q[1] - d1[1] * q[0]) / det;
            p[0] + e1 * a + e2 * b
        };
        // the decal's file
        let rows = self.effect_rows(Effect::Decals);
        let Some(name) = rows.get(self.effect.selected[Effect::Decals.index()]) else {
            return Vec::new();
        };
        let Some(image) = file_preview(&self.library_dir().join("decals").join(name)) else {
            return Vec::new();
        };
        let h = 80.0 * self.effect.scale * 0.5;
        let (s, c) = self.effect.angle.sin_cos();
        let corner = |x: f32, y: f32| [hit.uv[0] + x * c - y * s, hit.uv[1] + x * s + y * c];
        let offsets = [
            (-h, -h, 0.0, 0.0),
            (h, -h, 1.0, 0.0),
            (-h, h, 0.0, 1.0),
            (h, h, 1.0, 1.0),
        ];
        let lift = normal * 0.004;
        let v: Vec<Vertex> = offsets
            .iter()
            .map(|(x, y, u, w)| Vertex::new(to_model(corner(*x, *y)) + lift, normal, [*u, *w]))
            .collect();
        let verts = vec![v[0], v[1], v[2], v[1], v[3], v[2]];
        let mut material = ra2_render::material::Material::textured(
            ra2_render::material::Image::from_rgba(image.rgba, image.width, image.height),
        );
        material.transparent = true;
        material.no_alpha_test = true;
        material.lighting = false;
        material.z_write = false;
        material.z_test = false;
        material.cull = false;
        vec![DrawItem::new(verts, material, "decal preview")]
    }
}
