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
//! Tiles (a repeated pattern — the scroll trough, `metaldots_pattern`), `Lines`
//! borders, `Glyph` (a face character drawn as a widget of its own), `Proxy` (a
//! file drawn in place of the widget) and `Placeholder` (a slot for a screen).
//! Each is a hole with a name, not a silence.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bytemuck::{Pod, Zeroable};
use ra2fmt::giff::Font;
use ra2ui::{draw_order, Frame, Placement};

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
        let archive = ra2fmt::cfl::Cfl::open_path(&game.join("RA2.cfz")).ok();
        let faces = ra2ui::Fonts::load(&game.join("RA2.cfz"))?;
        let mut assets = UiAssets {
            ui_root,
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
        for p in draw_order(roots) {
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
            if p.widget.kind == ra2fmt::gib::WidgetKind::Rectangle {
                if let Some(fill) = rectangle_quad(p, clip) {
                    quads.push(fill);
                }
            }
            if let Some(quad) = self.art_quad(p, clip) {
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
    fn art_quad(&mut self, p: &Placement<'_>, clip: Option<Frame>) -> Option<UiQuad> {
        let file = p.widget.filename()?;
        if file.is_empty() {
            return None;
        }
        let uv = p.widget.uv().unwrap_or([0.0, 0.0, 1.0, 1.0]);
        // A rect that runs past one page is a *tile*: a pattern to repeat. Not
        // drawn yet, and left out rather than stretched.
        if uv[2] > 1.0001 || uv[3] > 1.0001 || uv[0] < -0.0001 || uv[1] < -0.0001 {
            self.tiles.push(file.to_string());
            return None;
        }
        let texture = self.texture(file)?;
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
        // 16 0 30 0`.
        let draught = p.widget.field("_Size").and_then(ra2fmt::gib::Axes::parse);
        let rect = match draught {
            Some(axes) if !axes.first.fraction && !axes.second.fraction => {
                Frame::new(p.frame.x, p.frame.y, axes.first.amount, axes.second.amount)
            }
            _ => p.frame,
        };
        Some(UiQuad {
            rect,
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
                uv: [
                    glyph.cell[0] as f32 / pw,
                    glyph.cell[1] as f32 / ph,
                    (glyph.cell[0] + glyph.cell[2]) as f32 / pw,
                    (glyph.cell[1] + glyph.cell[3]) as f32 / ph,
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
        uv: [0.0, 0.0, 1.0, 1.0],
        color,
        texture: 0, // white
        alpha: true,
        clip,
    })
}

/// Decode art. A `.bmp` or `.tga` the image crate can read, or nothing.
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
        // and a filtering sampler would blur the picture away from it.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
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
    let (x0, y0) = (quad.rect.x, quad.rect.y);
    let (x1, y1) = (quad.rect.right(), quad.rect.bottom());
    let [u0, v0, u1, v1] = quad.uv;
    let use_alpha = f32::from(u8::from(quad.alpha));
    let vertex = |x: f32, y: f32, u: f32, v: f32| UiVertex {
        pos: to_clip(x, y),
        uv: [u, v],
        color: quad.color,
        use_alpha,
    };
    [
        vertex(x0, y0, u0, v0),
        vertex(x1, y0, u1, v0),
        vertex(x1, y1, u1, v1),
        vertex(x0, y0, u0, v0),
        vertex(x1, y1, u1, v1),
        vertex(x0, y1, u0, v1),
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
    #[test]
    fn a_tile_is_named_and_not_drawn() {
        let Some(game) = game_dir() else { return };
        let scene = scene_of(&game, "teamhq/hq_league.gib");
        assert!(
            scene.tiles.iter().any(|t| t.contains("ScrollBack")),
            "the trough is a tile: {:?}",
            scene.tiles
        );
        // Nothing is drawn from a rect that runs past its page: that is what a
        // tile is, and stretching one would be a lie about what it looks like.
        for quad in &scene.quads {
            let [_, _, u1, v1] = quad.uv;
            assert!(
                u1 <= 1.0001 && v1 <= 1.0001,
                "{:?}",
                scene.textures[quad.texture].label
            );
        }
    }
}
