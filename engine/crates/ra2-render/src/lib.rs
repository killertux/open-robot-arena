//! `ra2-render` — offscreen wgpu renderer for Robot Arena 2 assets.
//!
//! Renders a `.gmf` mesh, a fully assembled `.bot` robot, or an arena `.py`
//! scene. Core of the M1 asset-viewer milestone, the headless regression
//! harness, and the frame source for the M2 physics sandbox.
//!
//! [`Renderer`] holds the wgpu device so a caller (e.g. the sandbox) can render
//! many frames without re-initialising the GPU.
//!
//! Env: RA2_NO_CHASSIS=1  skip the chassis mesh
//!      RA2_DEBUG=1       print per-part bounds
//!      RA2_CULL          off disables per-material backface culling
//!      RA2_HIDE          comma-separated object-name substrings to skip
//!      RA2_OBSTACLE      arena prop filter (name|all|none)
//!      RA2_CAM           ex,ey,ez,tx,ty,tz  or  @<n|name>

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use ra2fmt::{bot as botfmt, component as compfmt, gmi, mount};
use wgpu::util::DeviceExt;

pub use ra2fmt::mount::{component_local, tm_mat4, MountMode};

pub mod ui;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    pub uv: [f32; 2],
    /// Tint + alpha (material transparency); `[1,1,1,1]` for opaque geometry.
    pub color: [f32; 4],
}

impl Vertex {
    /// A plain lit vertex (position, normal, uv), option-ally tinted.
    pub fn new(pos: Vec3, nrm: Vec3, uv: [f32; 2]) -> Self {
        Vertex {
            pos: pos.into(),
            nrm: nrm.into(),
            uv,
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Uniforms {
    mvp: [[f32; 4]; 4],
    /// The group's model matrix, for rotating normals into world space.
    model: [[f32; 4]; 4],
    light: [f32; 4],
}

/// One drawable batch: triangles sharing a texture, plus how to blend them.
#[derive(Clone)]
pub struct DrawItem {
    pub vertices: Vec<Vertex>,
    pub tex: Vec<u8>,
    pub tw: u32,
    pub th: u32,
    pub label: String,
    /// Draw with alpha blending (transparent material).
    pub blend: bool,
    /// Cull back faces (D3D `CULL_CCW`: the original's per-material state).
    pub cull: bool,
}

/// A set of items sharing one model transform within a frame.
pub struct RenderGroup<'a> {
    pub items: &'a [DrawItem],
    pub model: Mat4,
}

const SHADER: &str = r#"
struct Uniforms { mvp: mat4x4<f32>, model: mat4x4<f32>, light: vec4<f32>, };
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VsIn { @location(0) pos: vec3<f32>, @location(1) nrm: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) color: vec4<f32> };
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) nrm: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32> };

@vertex
fn vs_main(i: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = u.mvp * vec4<f32>(i.pos, 1.0);
    o.nrm = (u.model * vec4<f32>(i.nrm, 0.0)).xyz;
    o.uv = i.uv;
    o.color = i.color;
    return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv);
    let n = normalize(i.nrm);
    let l = max(dot(n, normalize(u.light.xyz)), 0.0);
    let shade = 0.35 + 0.65 * l;
    return vec4<f32>(c.rgb * shade * i.color.rgb, i.color.a);
}
"#;

/// Reusable offscreen wgpu renderer. Holds the GPU device and pipelines so a
/// caller can render many frames (e.g. an animation or a physics sandbox)
/// without re-initialising the adapter each time.
/// One format's pair of mesh pipelines.
struct Pipelines {
    opaque: wgpu::RenderPipeline,
    blend: wgpu::RenderPipeline,
    opaque_cull: wgpu::RenderPipeline,
    blend_cull: wgpu::RenderPipeline,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bgl: wgpu::BindGroupLayout,
    shader: wgpu::ShaderModule,
    sampler: wgpu::Sampler,
    color_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
    /// Pipelines are specific to their colour target's format. The offscreen
    /// readback uses [`Renderer::color_format`]; the app draws the live match
    /// into a window surface whose format the swapchain chose, so the same
    /// renderer has to build a second pair (and it is built once, not per
    /// frame).
    pipelines: RefCell<HashMap<wgpu::TextureFormat, Rc<Pipelines>>>,
    /// Clear colour (the arena's `setBackColor`), RGB 0..1 in the target's own
    /// (non-linear) byte space, like the original's D3D8 clear.
    background: std::cell::Cell<[f32; 3]>,
}

impl Renderer {
    /// Set the clear colour of every following frame (RGB 0..1).
    pub fn set_background(&self, rgb: [f32; 3]) {
        self.background.set(rgb);
    }

    /// The device, so another renderer (the shell's 2D one) can share it rather
    /// than open a second adapter.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Initialise the GPU (first available adapter).
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // --- wgpu ---------------------------------------------------------
        let mut idesc = wgpu::InstanceDescriptor::new_without_display_handle();
        idesc.backends = wgpu::Backends::all();
        let instance = wgpu::Instance::new(idesc);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("ra2-render"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            }))?;
        Renderer::from_device(&device, &queue)
    }

    /// A renderer on a device the caller already owns — the window path has a
    /// surface, so it must share that device rather than open a second adapter.
    ///
    /// The offscreen [`Renderer::new`] path is this function with a fresh
    /// device; both build the same pipelines for [`Renderer::color_format`].
    pub fn from_device(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let device = device.clone();
        let queue = queue.clone();
        let color_format = wgpu::TextureFormat::Rgba8Unorm;
        let depth_format = wgpu::TextureFormat::Depth32Float;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mesh-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let renderer = Renderer {
            device,
            queue,
            bgl,
            shader,
            sampler,
            color_format,
            depth_format,
            pipelines: RefCell::new(HashMap::new()),
            background: std::cell::Cell::new([0.06, 0.07, 0.09]),
        };
        // Build the readback format's pipelines now, so a missing feature is
        // reported by the constructor rather than by the first frame.
        let _ = renderer.pipelines_for(color_format);
        Ok(renderer)
    }

    /// The pipelines that draw into a target of `format`, built once.
    fn pipelines_for(&self, format: wgpu::TextureFormat) -> Rc<Pipelines> {
        if let Some(found) = self.pipelines.borrow().get(&format) {
            return found.clone();
        }
        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&self.bgl)],
                immediate_size: 0,
            });
        let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4];
        // Per-item culling is the original's per-material state; `RA2_CULL=off`
        // disables it for debugging.
        let cull_on = std::env::var("RA2_CULL").as_deref() != Ok("off");
        let make = |label: &str, blend: bool, depth_write: bool, cull: bool| {
            self.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        module: &self.shader,
                        entry_point: Some("vs_main"),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<Vertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &attrs,
                        })],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &self.shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: blend.then_some(wgpu::BlendState::ALPHA_BLENDING),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        // The GMF triangles' right-handed cross product points
                        // into the solid; in the left-handed view that reads
                        // counter-clockwise from outside, so that is the front.
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: (cull && cull_on).then_some(wgpu::Face::Back),
                        ..Default::default()
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: self.depth_format,
                        depth_write_enabled: Some(depth_write),
                        depth_compare: Some(wgpu::CompareFunction::Less),
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState::default(),
                    multiview_mask: None,
                    cache: None,
                })
        };
        let pipelines = Rc::new(Pipelines {
            opaque: make("opaque", false, true, false),
            blend: make("blend", true, false, false),
            opaque_cull: make("opaque-cull", false, true, true),
            blend_cull: make("blend-cull", true, false, true),
        });
        self.pipelines
            .borrow_mut()
            .insert(format, pipelines.clone());
        pipelines
    }

    /// Render a single group of items with an explicit view-projection matrix.
    pub fn render(
        &self,
        items: &[DrawItem],
        mvp: Mat4,
        w: u32,
        h: u32,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        self.render_groups(
            &[RenderGroup {
                items,
                model: Mat4::IDENTITY,
            }],
            mvp,
            w,
            h,
        )
    }

    /// Render several groups of items, each with its own model transform, under
    /// one shared view-projection matrix. Lets the sandbox draw a static arena
    /// and a moving robot without re-uploading the arena's textures per frame.
    pub fn render_groups(
        &self,
        groups: &[RenderGroup<'_>],
        view_proj: Mat4,
        w: u32,
        h: u32,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("color"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.color_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.depth_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

        self.render_groups_to(
            groups,
            view_proj,
            &color_view,
            &depth_view,
            self.color_format,
        )?;
        self.read_texture(&color, w, h)
    }

    /// Draw several groups into a caller-supplied colour target and depth
    /// buffer.
    ///
    /// This is [`Renderer::render_groups`] without the private texture and the
    /// readback: the colour pass clears to the same dark grey, writes depth, and
    /// orders opaque geometry before blended geometry exactly as the readback
    /// path does. `format` must be the colour target's own format; pipelines for
    /// a format are built on first use and cached.
    pub fn render_groups_to(
        &self,
        groups: &[RenderGroup<'_>],
        view_proj: Mat4,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Result<(), Box<dyn std::error::Error>> {
        struct Gpu {
            vbuf: wgpu::Buffer,
            bg: wgpu::BindGroup,
            count: u32,
            blend: bool,
            cull: bool,
        }
        let mut gpu_items: Vec<Gpu> = Vec::new();
        for group in groups {
            if group.items.is_empty() {
                continue;
            }
            let uniforms = Uniforms {
                mvp: (view_proj * group.model).to_cols_array_2d(),
                model: group.model.to_cols_array_2d(),
                // Towards the light, in world space (Y up).
                light: [-0.4, 0.7, -0.6, 0.0],
            };
            let ubuf = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("uniforms"),
                    contents: bytemuck::bytes_of(&uniforms),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            for it in group.items {
                let vbuf = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&it.label),
                        contents: bytemuck::cast_slice(&it.vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                let tex = self.device.create_texture_with_data(
                    &self.queue,
                    &wgpu::TextureDescriptor {
                        label: Some("albedo"),
                        size: wgpu::Extent3d {
                            width: it.tw,
                            height: it.th,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    },
                    wgpu::util::TextureDataOrder::LayerMajor,
                    &it.tex,
                );
                let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
                let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: ubuf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                });
                gpu_items.push(Gpu {
                    vbuf,
                    bg,
                    count: it.vertices.len() as u32,
                    blend: it.blend,
                    cull: it.cull,
                });
            }
        }

        let pipelines = self.pipelines_for(format);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear({
                            let [r, g, b] = self.background.get();
                            wgpu::Color {
                                r: r as f64,
                                g: g as f64,
                                b: b as f64,
                                a: 1.0,
                            }
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // Opaque geometry first, then blended (transparent) geometry; each
            // split by the item's culling state.
            for (blend, cull, pipeline) in [
                (false, false, &pipelines.opaque),
                (false, true, &pipelines.opaque_cull),
                (true, false, &pipelines.blend),
                (true, true, &pipelines.blend_cull),
            ] {
                rp.set_pipeline(pipeline);
                for g in gpu_items
                    .iter()
                    .filter(|g| g.blend == blend && g.cull == cull)
                {
                    rp.set_bind_group(0, &g.bg, &[]);
                    rp.set_vertex_buffer(0, g.vbuf.slice(..));
                    rp.draw(0..g.count, 0..1);
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    /// Copy a colour texture back as RGBA8, top row first.
    ///
    /// The texture must belong to this renderer's device and have a
    /// `COPY_SRC` usage; its bytes are read in the texture's own format (the
    /// match composition uses `Rgba8Unorm` so the HUD's byte pipeline and the
    /// 3D pass share one target).
    pub fn read_texture(
        &self,
        texture: &wgpu::Texture,
        w: u32,
        h: u32,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let align = 256u32;
        let unpadded = w * 4;
        let padded = unpadded.div_ceil(align) * align;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;

        let mapped = slice.get_mapped_range()?;
        let mut rgba = Vec::with_capacity((unpadded * h) as usize);
        for row in 0..h as usize {
            let start = row * padded as usize;
            rgba.extend_from_slice(&mapped[start..start + unpadded as usize]);
        }
        drop(mapped);
        readback.unmap();

        Ok(rgba)
    }
}

// ---------------------------------------------------------------------------
// scene collection
// ---------------------------------------------------------------------------

/// Axis-aligned bounds of every vertex in `items`.
pub fn bounds(items: &[DrawItem]) -> (Vec3, Vec3) {
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for it in items {
        for v in &it.vertices {
            let p = Vec3::from(v.pos);
            min = min.min(p);
            max = max.max(p);
        }
    }
    if min.x > max.x {
        return (Vec3::ZERO, Vec3::ZERO);
    }
    (min, max)
}

/// Print per-part bounds when `RA2_DEBUG` is set.
pub fn debug_print_items(items: &[DrawItem]) {
    if std::env::var("RA2_DEBUG").is_err() {
        return;
    }
    for it in items {
        let (mut a, mut b) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for v in &it.vertices {
            let p = Vec3::from(v.pos);
            a = a.min(p);
            b = b.max(p);
        }
        eprintln!(
            "  part {:26} verts={:5} bbox=({:.3},{:.3},{:.3})..({:.3},{:.3},{:.3}) tex={}x{}",
            it.label,
            it.vertices.len(),
            a.x,
            a.y,
            a.z,
            b.x,
            b.y,
            b.z,
            it.tw,
            it.th
        );
    }
}

/// The scene background colour stored in an arena's `.gmf` (via its `.py`
/// script's model, or the `.gmf` itself); the clear colour when the arena
/// script does not call `setBackColor`.
pub fn scene_background(input: &str) -> Option<[f32; 3]> {
    let lower = input.to_ascii_lowercase();
    let data = if lower.ends_with(".py") {
        let script = ra2fmt::arena::parse_path(Path::new(input)).ok()?;
        let root = find_game_root(Path::new(input))?;
        std::fs::read(resolve_ci(&root, script.model.as_ref()?)?).ok()?
    } else {
        std::fs::read(input).ok()?
    };
    Some(gmi::parse(&data).ok()?.background)
}

/// Physics-proxy triangles of an arena (`.gmf` or `.py`), if it has any.
///
/// RA2 marks its collision proxies with a negative material reference or names
/// like `*Collision*` / `*havok*`; those are the shapes the game actually
/// collides against, and they exclude the decorative geometry a robot would
/// otherwise high-centre on. Arenas that ship none (the practice arena, for
/// instance) return `None` so callers can fall back to the visible geometry.
pub fn collect_collision(
    input: &str,
) -> Result<Option<Vec<[Vec3; 3]>>, Box<dyn std::error::Error>> {
    let lower = input.to_ascii_lowercase();
    let mut out = Vec::new();
    if lower.ends_with(".py") {
        let script = ra2fmt::arena::parse_path(Path::new(input))?;
        if let Some(root) = find_game_root(Path::new(input)) {
            if let Some(m) = &script.model {
                if let Some(p) = resolve_ci(&root, m) {
                    let data = std::fs::read(&p)?;
                    let scene = gmi::parse(&data)?;
                    out.extend(scene_collision_triangles(&scene, Mat4::IDENTITY));
                }
            }
        }
    } else {
        let data = std::fs::read(input)?;
        let scene = gmi::parse(&data)?;
        out.extend(scene_collision_triangles(&scene, Mat4::IDENTITY));
    }
    if out.len() < 4 {
        Ok(None)
    } else {
        Ok(Some(out))
    }
}

/// Triangles of a scene's physics-proxy objects, in `world` space.
pub fn scene_collision_triangles(scene: &gmi::GmiScene, world: Mat4) -> Vec<[Vec3; 3]> {
    let mut out = Vec::new();
    for o in &scene.objects {
        let Some(mesh) = &o.mesh else { continue };
        if mesh.points.is_empty() || !(mesh.is_collision() || is_collision_name(&o.name)) {
            continue;
        }
        let obj = world * tm_mat4(&o.transform);
        let pts: Vec<Vec3> = mesh
            .points
            .iter()
            .map(|p| obj.transform_point3(Vec3::from(*p)))
            .collect();
        for f in &mesh.faces {
            let (a, b, c) = (f.v[0] as usize, f.v[1] as usize, f.v[2] as usize);
            if a >= pts.len() || b >= pts.len() || c >= pts.len() {
                continue;
            }
            out.push([pts[a], pts[b], pts[c]]);
        }
    }
    out
}

/// One rigid body declared by an arena's Havok data.
#[derive(Clone)]
pub struct HavokBody {
    /// Rigid body name (the object the game simulates).
    pub name: String,
    /// kg; 0 means the body is fixed.
    pub mass: f32,
    pub friction: f32,
    pub elasticity: f32,
    pub unyielding: bool,
    pub simulation_geometry: i32,
    /// `USE_DISPLAY_PROXY`: the geometry object named by `display_object` is
    /// what this body simulates (the exporter's display/collision decoupling).
    pub use_display_proxy: bool,
    /// `HAVOK_GEO_TYPE` (0 `Standard`, 1 `Sphere`/`Box` in shipped arenas).
    pub geo_type: i32,
    /// The body is a trigger zone: it has no collision response.
    pub disable_collisions: bool,
    /// Placement of the body (its `NODE_TM`).
    pub transform: Mat4,
    /// Collision geometry in body-local space.
    pub points: Vec<Vec3>,
    /// The geometry object used for simulation.
    pub collision_object: String,
    /// The geometry object to draw.
    pub display_object: String,
    /// Visual batches in body-local space.
    pub items: Vec<DrawItem>,
}

impl HavokBody {
    /// A body with zero mass (or `unyielding`) never moves.
    pub fn is_static(&self) -> bool {
        self.unyielding || self.mass <= 0.0
    }

    /// Trigger zones (`DISABLE_COLLISIONS`) must not become colliders.
    pub fn collides(&self) -> bool {
        !self.disable_collisions
    }

    /// A body whose simulation geometry is an endless plane rather than a mesh
    /// (see [`gmi::RigidBody::is_plane`]).
    pub fn is_plane(&self) -> bool {
        self.geo_type == gmi::GEO_TYPE_PLANE
    }
}

/// Everything an arena's `.gmf` says about its physics.
#[derive(Clone, Default)]
pub struct HavokArena {
    /// World gravity the file asks Havok for (`[0, -9.80998, 0]` stock).
    pub gravity: Option<Vec3>,
    /// Triangles of the fixed collision geometry, in world space.
    pub static_triangles: Vec<[Vec3; 3]>,
    /// Every rigid body in the file (static and movable).
    pub bodies: Vec<HavokBody>,
}

impl HavokArena {
    /// Fixed collision triangles belonging to a single static body.
    pub fn static_triangles_of(&self, name: &str) -> Vec<[Vec3; 3]> {
        let mut out = Vec::new();
        for b in self
            .bodies
            .iter()
            .filter(|b| b.is_static() && b.name == name)
        {
            let w = b.transform;
            for tri in b.points.as_chunks::<3>().0 {
                if tri.len() == 3 {
                    out.push([
                        w.transform_point3(tri[0]),
                        w.transform_point3(tri[1]),
                        w.transform_point3(tri[2]),
                    ]);
                }
            }
        }
        out
    }
}

/// An arena's declared spawn points, in `ID` order: `(position, facing)`.
///
/// Arenas carry these as `type = startpoint` attachment objects in their model
/// (four in every stock arena). The game drops a robot at one, and they are
/// where the fight actually starts — the Octagon's are 8 m out on the *play
/// surface*, not out at the 3.7 m drop around it, which is where an invented
/// spread puts them. The facing is the point's Z axis, which always looks at the
/// arena centre (Point01 at `(-5.94, 1.87, 5.69)` has `(0.707, 0, -0.707)`).
pub fn start_points(input: &str) -> Result<Vec<(Vec3, Vec3)>, Box<dyn std::error::Error>> {
    let Some(path) = arena_model_path(input)? else {
        return Ok(Vec::new());
    };
    let scene = gmi::parse(&std::fs::read(&path)?)?;
    let mut found: Vec<(i32, (Vec3, Vec3))> = Vec::new();
    for o in scene.attachments() {
        if !o.data.to_ascii_lowercase().contains("startpoint") {
            continue;
        }
        let Some(a) = gmi::Attachment::from_object(o) else {
            continue;
        };
        let tm = tm_mat4(&a.transform);
        found.push((
            a.id,
            (
                tm.transform_point3(Vec3::ZERO),
                tm.transform_vector3(Vec3::Z).normalize_or_zero(),
            ),
        ));
    }
    found.sort_by_key(|(id, _)| *id);
    Ok(found.into_iter().map(|(_, v)| v).collect())
}

/// Resolve the mesh an arena path stands for (the `.gmf` for a `.gmf`, the
/// script's main model for a `.py`).
pub fn arena_model_path(input: &str) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    if input.to_ascii_lowercase().ends_with(".py") {
        let script = ra2fmt::arena::parse_path(Path::new(input))?;
        let Some(root) = find_game_root(Path::new(input)) else {
            return Ok(None);
        };
        Ok(script.model.as_deref().and_then(|m| resolve_ci(&root, m)))
    } else {
        Ok(Some(PathBuf::from(input)))
    }
}

/// Does `name` name one of `roots`, or sit underneath one? Group nodes carry
/// no geometry of their own, so their children have to be walked in.
fn under_any(
    ancestry: &std::collections::HashMap<String, String>,
    name: &str,
    roots: &[&str],
) -> bool {
    let mut current = name.to_ascii_lowercase();
    let mut guard = 0;
    loop {
        if roots.iter().any(|r| r.eq_ignore_ascii_case(&current)) {
            return true;
        }
        match ancestry.get(&current) {
            Some(parent) if !parent.is_empty() && guard < 64 => {
                current = parent.clone();
                guard += 1;
            }
            _ => return false,
        }
    }
}

/// The geometry objects a body simulates (see
/// [`gmi::RigidBody::simulation_objects`]), plus the children of any group node
/// among them.
fn simulation_objects<'a>(
    scene: &'a gmi::GmiScene,
    rb: &gmi::RigidBody,
    ancestry: &std::collections::HashMap<String, String>,
) -> Vec<&'a gmi::SceneObject> {
    let roots = rb.simulation_objects();
    scene
        .objects
        .iter()
        .filter(|o| under_any(ancestry, &o.name, &roots))
        .collect()
}

/// Half-width of the quad a plane body becomes. Shipped arenas are ~50 m
/// across, so this is comfortably beyond any edge.
const PLANE_HALF: f32 = 200.0;

/// Two triangles covering an infinite plane placed by `transform`, whose normal
/// is the matrix's Y row.
fn plane_triangles(transform: &Mat4) -> [[Vec3; 3]; 2] {
    let at = transform.transform_point3(Vec3::ZERO);
    let x = transform.transform_vector3(Vec3::X).normalize_or_zero();
    let z = transform.transform_vector3(Vec3::Z).normalize_or_zero();
    let (x, z) = if x == Vec3::ZERO || z == Vec3::ZERO {
        (Vec3::X, Vec3::Z)
    } else {
        (x, z)
    };
    let (a, b) = (x * PLANE_HALF, z * PLANE_HALF);
    [
        [at - a - b, at + a - b, at + a + b],
        [at - a - b, at + a + b, at - a + b],
    ]
}

/// Read an arena's Havok simulation data: gravity, the fixed collision geometry
/// the game actually collides against, and the movable rigid bodies it spawns.
/// Returns `None` when the file has no `RBCollection` (some arenas rely on the
/// visible geometry instead, and the practice arena's props come from its
/// script).
pub fn collect_havok(input: &str) -> Result<Option<HavokArena>, Box<dyn std::error::Error>> {
    let Some(path) = arena_model_path(input)? else {
        return Ok(None);
    };
    let data = std::fs::read(&path)?;
    let scene = gmi::parse(&data)?;
    if scene.objects.iter().all(|o| o.rigid_bodies.is_empty()) {
        return Ok(None);
    }
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();

    let mut arena = HavokArena {
        gravity: scene.simulation.as_ref().map(|s| Vec3::from(s.gravity)),
        ..Default::default()
    };

    let ancestry: std::collections::HashMap<String, String> = scene
        .objects
        .iter()
        .map(|o| (o.name.to_ascii_lowercase(), o.parent.to_ascii_lowercase()))
        .collect();

    for rb in scene.rigid_bodies() {
        let collision = scene.find_object(rb.collision_object());
        let display = scene.find_object(rb.display_object());
        let transform = tm_mat4(&rb.transform);
        let to_body = transform.inverse();
        // Collision geometry is a triangle soup built from the mesh's faces (a
        // mesh's vertex list is not a triangle list). Which objects that comes
        // from is `SIMULATION_GEOMETRY`'s job, and a display-proxy body names a
        // *group* node whose children hold the real geometry — the Octagon's
        // `main_collision` names `main_display`, which is 57 child objects.
        //
        // A *standalone* proxy is a shape the body reuses: one shared object,
        // named by every body that wants it, and the exporters leave it sitting
        // wherever the first user was. The Combat Arena has a single
        // `spike_collision` at spikes1's position, named by *both* spike bodies,
        // so placing it absolutely puts spikes2's collider 8.6 m away at
        // spikes1's. Such a proxy's mesh is already the shape in the body's own
        // frame, so it is used as written. A proxy that names a *group* (the
        // Octagon's `main_collision` names `main_display`) keeps the object
        // placements its children carry.
        let standalone = rb.simulation_objects().into_iter().find(|name| {
            !name.eq_ignore_ascii_case(&rb.name)
                && !name.eq_ignore_ascii_case("(null)")
                && scene
                    .find_object(name)
                    .map(|o| o.mesh.is_some())
                    .unwrap_or(false)
                && !ancestry.values().any(|p| p.eq_ignore_ascii_case(name))
        });
        let mut points = Vec::new();
        for obj in simulation_objects(&scene, rb, &ancestry) {
            let Some(mesh) = &obj.mesh else { continue };
            let place = match &standalone {
                Some(name) if name.eq_ignore_ascii_case(&obj.name) => Mat4::IDENTITY,
                _ => to_body * tm_mat4(&obj.transform),
            };
            for f in &mesh.faces {
                let (a, b, c) = (f.v[0] as usize, f.v[1] as usize, f.v[2] as usize);
                if a < mesh.points.len() && b < mesh.points.len() && c < mesh.points.len() {
                    points.push(place.transform_point3(Vec3::from(mesh.points[a])));
                    points.push(place.transform_point3(Vec3::from(mesh.points[b])));
                    points.push(place.transform_point3(Vec3::from(mesh.points[c])));
                }
            }
        }
        // A plane body has no mesh; the transform places an infinite plane
        // whose normal is the matrix's Y row. Emit it as two large triangles
        // (both windings are added to the collider below).
        if rb.is_plane() {
            for tri in plane_triangles(&transform) {
                for v in tri {
                    points.push(to_body.transform_point3(v));
                }
            }
        }
        // The body is *drawn* as its display proxy, which is normally a group
        // node, so its children are the real batches (`main_display`,
        // `floor_display`, `Arena`).
        let mut items = Vec::new();
        if let Some(disp) = display {
            let roots = [disp.name.as_str()];
            for o in &scene.objects {
                if under_any(&ancestry, &o.name, &roots) && o.mesh.is_some() {
                    items.extend(object_items(&scene, &o.name, &dir));
                }
            }
        }
        let body = HavokBody {
            name: rb.name.clone(),
            mass: rb.mass,
            friction: rb.friction,
            elasticity: rb.elasticity,
            unyielding: rb.unyielding != 0,
            simulation_geometry: rb.simulation_geometry,
            use_display_proxy: rb.use_display_proxy,
            geo_type: rb.geo_type,
            disable_collisions: rb.disable_collisions,
            transform: tm_mat4(&rb.transform),
            points,
            collision_object: collision.map(|o| o.name.clone()).unwrap_or_default(),
            display_object: display.map(|o| o.name.clone()).unwrap_or_default(),
            items,
        };
        if body.is_static() && body.collides() {
            for tri in body.points.as_chunks::<3>().0 {
                if tri.len() == 3 {
                    arena.static_triangles.push([
                        body.transform.transform_point3(tri[0]),
                        body.transform.transform_point3(tri[1]),
                        body.transform.transform_point3(tri[2]),
                    ]);
                }
            }
        }
        arena.bodies.push(body);
    }
    Ok(Some(arena))
}

pub fn collect_items(input: &str) -> Result<Vec<DrawItem>, Box<dyn std::error::Error>> {
    let lower = input.to_ascii_lowercase();
    if lower.ends_with(".bot") {
        collect_bot(input)
    } else if lower.ends_with(".py") {
        collect_arena(input)
    } else {
        let mut items = Vec::new();
        add_gmi(Path::new(input), Mat4::IDENTITY, &mut items)?;
        Ok(items)
    }
}

/// Assemble an arena scene from its Python script: the main `.gmf` model plus
/// the script's extra props. Select props with `RA2_OBSTACLE=<name|all|none>`.
fn collect_arena(input: &str) -> Result<Vec<DrawItem>, Box<dyn std::error::Error>> {
    let script = ra2fmt::arena::parse_path(Path::new(input))?;
    let root = find_game_root(Path::new(input))
        .ok_or("could not locate game root (Arenas/Components dir)")?;
    let obstacle = std::env::var("RA2_OBSTACLE").unwrap_or_else(|_| "none".into());
    println!(
        "arena {:?}: model={:?} xtras={} (obstacle filter: {obstacle:?})",
        script.name,
        script.model.as_deref().unwrap_or("(none)"),
        script.xtras.len()
    );

    let mut items = Vec::new();
    if let Some(m) = &script.model {
        match resolve_ci(&root, m) {
            Some(p) => add_gmi(&p, Mat4::IDENTITY, &mut items)?,
            None => eprintln!("  model not found: {m}"),
        }
    }
    for x in script.xtras_matching(&obstacle) {
        match resolve_ci(&root, &x.gmf) {
            Some(p) => {
                let before = items.len();
                add_gmi(&p, Mat4::IDENTITY, &mut items)?;
                if items.len() > before {
                    println!("  xtra {:?} ({}) -> {}", x.name, x.material, p.display());
                }
            }
            None => eprintln!("  xtra not found: {}", x.gmf),
        }
    }
    Ok(items)
}

/// Resolve a script-relative path under `root`, case-insensitively.
pub fn resolve_ci(root: &Path, rel: &str) -> Option<PathBuf> {
    ra2fmt::find_ci(root, rel)
}

/// Objects whose name contains any `RA2_HIDE` substring are skipped.
fn is_hidden(name: &str) -> bool {
    use std::sync::OnceLock;
    static HIDE: OnceLock<Vec<String>> = OnceLock::new();
    let list = HIDE.get_or_init(|| {
        std::env::var("RA2_HIDE")
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().to_ascii_lowercase())
                    .filter(|x| !x.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    });
    if list.is_empty() {
        return false;
    }
    let n = name.to_ascii_lowercase();
    list.iter().any(|h| n.contains(h))
}

/// RA2 marks physics proxies with `-1` material refs, names like `*Collision*`
/// or `*traction*` (the component def's `contact`/`passthru` objects), or
/// materials named `havok` / `collision` / `collision proxy`. These are not
/// meant to be drawn.
fn is_collision_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("havok") || n.contains("collision") || n.contains("proxy") || n.contains("traction")
}

/// A component's parsed definition plus its loaded `.gmf` mesh.
pub struct BotComponent {
    pub label: String,
    pub def: compfmt::ComponentDef,
    pub scene: gmi::GmiScene,
    /// Directory holding the component's mesh (for resolving its textures).
    pub dir: PathBuf,
}

/// A parsed robot plus its per-component definitions and meshes.
pub type LoadedBot = (botfmt::Bot, PathBuf, Vec<Option<BotComponent>>);

/// Parse a `.bot` and load the definition + mesh of every attached component.
///
/// Index `0` (the chassis) is always `None`: the chassis mesh is embedded in the
/// `.bot` itself, see [`chassis_items`].
pub fn load_bot_components(input: &str) -> Result<LoadedBot, Box<dyn std::error::Error>> {
    let data = std::fs::read(input)?;
    let bot = botfmt::parse(&data)?;
    let root =
        find_game_root(Path::new(input)).ok_or("could not locate game root (Components dir)")?;

    // Component definitions live in `Components.cfz`; prefer the archive and
    // fall back to loose extracted `.txt` files when present.
    let cfl = ra2fmt::Cfl::open_path(&root.join("Components.cfz")).ok();

    let mut loaded: Vec<Option<BotComponent>> = Vec::with_capacity(bot.components.len());
    for (i, c) in bot.components.iter().enumerate() {
        if i == 0 || c.class == "SmartZone" || c.file.is_empty() {
            loaded.push(None);
            continue;
        }
        let text = cfl
            .as_ref()
            .and_then(|cf| cf.get_text(&c.file))
            .or_else(|| std::fs::read_to_string(root.join(c.file.replace('\\', "/"))).ok());
        let Some(text) = text else {
            eprintln!("  skip {:?}: no definition {}", c.class, c.file);
            loaded.push(None);
            continue;
        };
        let def = compfmt::parse(&text);
        let mesh_path = root.join("Components").join(def.mesh_rel_path());
        match std::fs::read(&mesh_path).ok().map(|b| gmi::parse(&b)) {
            Some(Ok(scene)) => {
                let dir = mesh_path.parent().unwrap_or(Path::new(".")).to_path_buf();
                loaded.push(Some(BotComponent {
                    label: format!("{}({})", c.class, def.name),
                    def,
                    scene,
                    dir,
                }));
            }
            other => {
                eprintln!(
                    "  skip {:?}: {} ({:?})",
                    c.class,
                    mesh_path.display(),
                    other.map(|r| r.err())
                );
                loaded.push(None);
            }
        }
    }
    Ok((bot, root, loaded))
}

/// Visual batches for a scene in `world` space (skipping collision proxies).
pub fn scene_items(scene: &gmi::GmiScene, world: Mat4, dir: &Path, label: &str) -> Vec<DrawItem> {
    let mut items = Vec::new();
    push_scene(scene, world, dir, label, &mut items);
    items
}

/// Visual batches for the chassis mesh embedded in a `.bot`, placed by `world`.
pub fn chassis_items(bot: &botfmt::Bot, world: Mat4) -> Vec<DrawItem> {
    let Some(ch) = bot.components.first().and_then(|c| c.chassis.as_ref()) else {
        return Vec::new();
    };
    let Some(mesh) = &ch.mesh else {
        return Vec::new();
    };
    if std::env::var("RA2_DEBUG").is_ok() {
        let ymin = mesh.points.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        let ymax = mesh.points.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        eprintln!(
            "  raw chassis mesh: {} pts, y={:.3}..{:.3}",
            mesh.points.len(),
            ymin,
            ymax
        );
    }
    let verts = chassis_mesh_vertices(ch, world);
    if verts.is_empty() {
        return Vec::new();
    }
    let (tex, tw, th) = match &ch.texture_tga {
        Some(tga) => decode_image(tga).unwrap_or_else(|| (solid([120, 120, 125, 255], 4, 4), 4, 4)),
        None => (solid([120, 120, 125, 255], 4, 4), 4, 4),
    };
    vec![DrawItem {
        vertices: verts,
        tex,
        tw,
        th,
        label: "chassis".into(),
        blend: false,
        cull: true,
    }]
}

pub fn collect_bot(input: &str) -> Result<Vec<DrawItem>, Box<dyn std::error::Error>> {
    let (bot, root, loaded) = load_bot_components(input)?;
    println!(
        "bot {} v{}: {} components (root={})",
        bot.name,
        bot.version,
        bot.components.len(),
        root.display()
    );

    // Component world transforms, chained through parents. RA2 places a component
    // by its stored translation + quaternion, then a mount yaw (`angle`) and a
    // vertical offset (`height`) about/along the model up axis (Y); at a
    // component-to-component mount it can instead align the attachment frames
    // (see `MountMode` and `docs/06-deferred-fidelity.md` F1).
    let world = component_worlds_mounted(&bot, &loaded, MountMode::from_env());

    let mut items = Vec::new();

    // chassis (mesh embedded in the bot file)
    if std::env::var("RA2_NO_CHASSIS").is_err() {
        items.extend(chassis_items(&bot, world[0]));
    }

    // attached components
    for (i, lc) in loaded.iter().enumerate().skip(1) {
        let Some(lc) = lc else { continue };
        push_scene(&lc.scene, world[i], &lc.dir, &lc.label, &mut items);
    }
    Ok(items)
}

/// Chained bot-local transform for every component using the stored transforms
/// alone (no `.gmf` mount frames). Prefer [`component_worlds_mounted`] with the
/// loaded components, which is what the renderer and the sim both do.
pub fn component_worlds(bot: &botfmt::Bot) -> Vec<Mat4> {
    mount::worlds(
        bot,
        &mount::MountFrames::empty(bot.components.len()),
        mount::MountOptions {
            mode: MountMode::Stored,
            chassis_plane: 0.0,
        },
    )
}

/// Height of the chassis' mounting plane, from `RA2_CHASSIS_PLANE` (default 0).
fn chassis_plane() -> f32 {
    std::env::var("RA2_CHASSIS_PLANE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

/// Chained bot-local transform for every component (parents before children).
///
/// `loaded` supplies the components' `.gmf` scenes, which is where the mount
/// frames live; with it, `mode` can place component-to-component mounts by
/// aligning attachment frames instead of by the stored transform alone. The
/// rule itself lives in [`ra2fmt::mount`] and is shared with the workshop.
pub fn component_worlds_mounted(
    bot: &botfmt::Bot,
    loaded: &[Option<BotComponent>],
    mode: MountMode,
) -> Vec<Mat4> {
    let tables: Vec<Option<std::collections::BTreeMap<i32, gmi::Attachment>>> = loaded
        .iter()
        .map(|lc| lc.as_ref().map(|lc| lc.scene.attachment_table()))
        .collect();
    mount::worlds(
        bot,
        &mount::MountFrames::from_tables(&tables),
        mount::MountOptions {
            mode,
            chassis_plane: chassis_plane(),
        },
    )
}

/// Build the chassis' render vertices from the file's **face groups** (F10).
///
/// The bot file dresses each side of the chassis from its own patch of the
/// 256×256 skin: a group lists the triangles of that side and, for its vertices
/// in ascending index order, the skin coordinate each one uses. That ordering is
/// the rule the stock bots agree on — every group's coordinates land inside the
/// group's own rectangle, and they reproduce the model's own per-point UV table
/// exactly on a chassis whose sides do not share vertices.
///
/// Using the model's per-point table instead is what makes a chassis decal come
/// out rotated and mirrored: a mesh point shared by two sides can only carry one
/// coordinate there, so one of the two sides samples the wrong patch of skin.
fn chassis_mesh_vertices(ch: &ra2fmt::bot::Chassis, world: Mat4) -> Vec<Vertex> {
    let Some(mesh) = ch.mesh.as_ref() else {
        return Vec::new();
    };
    // `(group, vertex) -> skin coordinate`.
    let groups: Vec<std::collections::HashMap<u32, [f32; 2]>> = ch
        .face_groups
        .iter()
        .enumerate()
        .map(|(gi, fg)| {
            let mut seen: Vec<u32> = Vec::new();
            for f in ch.faces.iter().filter(|f| f.group == gi as i32) {
                for &i in &f.points {
                    if !seen.contains(&i) {
                        seen.push(i);
                    }
                }
            }
            seen.sort_unstable();
            seen.iter()
                .zip(fg.coords.iter())
                .map(|(&i, c)| (i, [c[0] / 256.0, c[1] / 256.0]))
                .collect()
        })
        .collect();

    let mut out = Vec::new();
    let pts: Vec<Vec3> = mesh
        .points
        .iter()
        .map(|p| world.transform_point3(Vec3::from(*p)))
        .collect();
    for tri in mesh.indices.as_chunks::<3>().0 {
        let (a, b, cc) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        if a >= pts.len() || b >= pts.len() || cc >= pts.len() {
            continue;
        }
        let (pa, pb, pc) = (pts[a], pts[b], pts[cc]);
        let n = (pb - pa).cross(pc - pa).normalize_or_zero();
        // The first group that owns all three vertices dresses this triangle.
        let group_uv = groups
            .iter()
            .find_map(|g| {
                Some([
                    *g.get(&(a as u32))?,
                    *g.get(&(b as u32))?,
                    *g.get(&(cc as u32))?,
                ])
            })
            .unwrap_or([
                mesh.uvs.get(a).copied().unwrap_or([0.0, 0.0]),
                mesh.uvs.get(b).copied().unwrap_or([0.0, 0.0]),
                mesh.uvs.get(cc).copied().unwrap_or([0.0, 0.0]),
            ]);
        // The chassis builder's triangles are the opposite way round to the
        // arenas' data; swap two corners so the one culling rule holds for
        // robots too.
        for (pos, uv) in [(pa, group_uv[0]), (pc, group_uv[2]), (pb, group_uv[1])] {
            out.push(Vertex {
                pos: pos.into(),
                nrm: n.into(),
                uv,
                color: [1.0, 1.0, 1.0, 1.0],
            });
        }
    }
    out
}

/// `(texture, tint+alpha, backface cull, vertices)` of one material batch.
type SceneGroup = (Option<PathBuf>, [f32; 4], bool, Vec<Vertex>);

/// Whether the original culls back faces for a face of `mesh` using
/// `material`. Its material apply (`FUN_005b24c0`) sets `D3DRS_CULLMODE` to CCW
/// or NONE by one flag; the GMF supplies the mesh's `backface_cull` and the
/// material's `two_sided`, so a face is culled when the mesh asks for it and the
/// material is one-sided.
fn face_culls(mesh: &gmi::Mesh, material: Option<&gmi::Material>) -> bool {
    mesh.backface_cull != 0 && material.is_none_or(|m| m.two_sided == 0)
}

/// Build render vertices for a `.gmf` scene, grouped by resolved material so
/// each group can use the correct texture (needed for `Multi/Sub-Object` arenas
/// where each face selects a sub-material).
fn scene_material_groups(scene: &gmi::GmiScene, world: Mat4, dir: &Path) -> Vec<SceneGroup> {
    scene_material_groups_opts(scene, world, dir, None, false)
}

/// Like [`scene_material_groups`] but optionally limited to one object and/or
/// emitted in object-local space (for props that move independently).
fn scene_material_groups_opts(
    scene: &gmi::GmiScene,
    world: Mat4,
    dir: &Path,
    only_object: Option<&str>,
    object_local: bool,
) -> Vec<SceneGroup> {
    let mut index: HashMap<u64, usize> = HashMap::new();
    let mut groups: Vec<SceneGroup> = Vec::new();
    for o in &scene.objects {
        let Some(mesh) = &o.mesh else { continue };
        if let Some(name) = only_object {
            if !o.name.eq_ignore_ascii_case(name) {
                continue;
            }
        }
        if mesh.is_collision() || is_hidden(&o.name) || is_collision_name(&o.name) {
            continue;
        }
        // Skip collision-proxy meshes whose material is tagged "havok"/"collision".
        if scene
            .materials
            .get(mesh.material_ref.max(0) as usize)
            .map(|m| is_collision_name(&m.name))
            .unwrap_or(false)
        {
            continue;
        }
        let obj_world = if object_local {
            world
        } else {
            world * tm_mat4(&o.transform)
        };
        let pts: Vec<Vec3> = mesh
            .points
            .iter()
            .map(|p| obj_world.transform_point3(Vec3::from(*p)))
            .collect();
        let linear = glam::Mat3::from_mat4(obj_world);
        let mirrored = linear.determinant() < 0.0;
        let normal_mat = if linear.determinant().abs() > 1e-12 {
            linear.inverse().transpose()
        } else {
            glam::Mat3::IDENTITY
        };
        for (fi, f) in mesh.faces.iter().enumerate() {
            let (ia, ib, ic) = (f.v[0] as usize, f.v[1] as usize, f.v[2] as usize);
            if ia >= pts.len() || ib >= pts.len() || ic >= pts.len() {
                continue;
            }
            // A face may select a collision sub-material.
            if scene
                .materials
                .get(mesh.material_ref.max(0) as usize)
                .map(|m| is_collision_name(&m.resolve_face(f.material).name))
                .unwrap_or(false)
            {
                continue;
            }
            // Strongly-transparent materials are invisible volumes / hazard zones.
            if scene
                .materials
                .get(mesh.material_ref.max(0) as usize)
                .map(|m| m.resolve_face(f.material).transparency >= 0.5)
                .unwrap_or(false)
            {
                continue;
            }
            let cull = face_culls(
                mesh,
                scene
                    .materials
                    .get(mesh.material_ref.max(0) as usize)
                    .map(|m| m.resolve_face(f.material)),
            );
            let key = ((mesh.material_ref as i64 as u64) << 32)
                | ((cull as u64) << 31)
                | (f.material as u32 as u64 & 0x7fff_ffff);
            let gi = *index.entry(key).or_insert_with(|| {
                let (tex, color) = material_slot(scene, mesh.material_ref, f.material, dir);
                groups.push((tex, color, cull, Vec::new()));
                groups.len() - 1
            });
            let (pa, pb, pc) = (pts[ia], pts[ib], pts[ic]);
            // The stored per-corner normals (`normals[4*face + corner]`, the
            // fourth being the face normal) point out of the solid; fall back
            // to the geometric normal (the data is clockwise-outward, so the
            // right-handed cross product points inwards).
            let fallback = -(pb - pa).cross(pc - pa).normalize_or_zero();
            let corner_normal = |corner: usize| -> Vec3 {
                mesh.normals
                    .get(fi * 4 + corner)
                    .map(|n| (normal_mat * Vec3::from(*n)).normalize_or_zero())
                    .filter(|n| *n != Vec3::ZERO)
                    .unwrap_or(fallback)
            };
            let normals = [corner_normal(0), corner_normal(1), corner_normal(2)];
            let color = groups[gi].1;
            let uv = |idx: usize| -> [f32; 2] {
                if !mesh.texfaces.is_empty()
                    && fi < mesh.texfaces.len()
                    && !mesh.texcoords.is_empty()
                {
                    let ti = mesh.texfaces[fi][idx] as usize;
                    if ti < mesh.texcoords.len() {
                        // GMF texture coordinates are 3ds Max's (v up); the
                        // bitmaps are stored top row first, so v is inverted.
                        return [mesh.texcoords[ti][0], 1.0 - mesh.texcoords[ti][1]];
                    }
                }
                [0.0, 0.0]
            };
            let g = &mut groups[gi].3;
            // A mirroring transform flips the winding; swap two corners back.
            let order = if mirrored { [0, 2, 1] } else { [0, 1, 2] };
            let ps = [pa, pb, pc];
            for k in order {
                g.push(Vertex {
                    pos: ps[k].into(),
                    nrm: normals[k].into(),
                    uv: uv(k),
                    color,
                });
            }
        }
    }
    groups
}

/// Resolve the texture path + tint/alpha for a (material slot, face) pair.
fn material_slot(
    scene: &gmi::GmiScene,
    material_ref: i32,
    face_material: i32,
    dir: &Path,
) -> (Option<PathBuf>, [f32; 4]) {
    let eff = scene
        .materials
        .get(material_ref.max(0) as usize)
        .map(|m| m.resolve_face(face_material));
    let color = eff
        .map(|m| {
            [
                m.diffuse[0].clamp(0.0, 1.0),
                m.diffuse[1].clamp(0.0, 1.0),
                m.diffuse[2].clamp(0.0, 1.0),
                (1.0 - m.transparency).clamp(0.0, 1.0),
            ]
        })
        .unwrap_or([0.6, 0.6, 0.62, 1.0]);
    let tex = eff
        .and_then(|m| m.first_bitmap())
        .and_then(|b| resolve_bitmap(dir, b));
    (tex, color)
}

/// Locate a bitmap referenced by a material, under `dir` or `dir/maps`.
fn resolve_bitmap(dir: &Path, bitmap: &str) -> Option<PathBuf> {
    [
        dir.join(bitmap),
        dir.join("maps").join(bitmap),
        dir.join("maps").join(bitmap.to_lowercase()),
        dir.join(bitmap.replace('\\', "/")),
    ]
    .into_iter()
    .find(|c| c.exists())
}

/// Push one [`DrawItem`] per material group.
fn push_scene(
    scene: &gmi::GmiScene,
    world: Mat4,
    dir: &Path,
    label: &str,
    items: &mut Vec<DrawItem>,
) {
    for (tex, color, cull, verts) in scene_material_groups(scene, world, dir) {
        if verts.is_empty() {
            continue;
        }
        let rgb = [
            (color[0] * 255.0) as u8,
            (color[1] * 255.0) as u8,
            (color[2] * 255.0) as u8,
            255,
        ];
        let (tex, tw, th) = tex
            .as_deref()
            .and_then(|p| decode_image(&std::fs::read(p).ok()?))
            .unwrap_or_else(|| (solid(rgb, 4, 4), 4, 4));
        items.push(DrawItem {
            vertices: verts,
            tex,
            tw,
            th,
            label: label.to_string(),
            blend: color[3] < 0.999,
            cull,
        });
    }
}

/// Visual batches for one geometry object, in object-local space, so a physics
/// body's transform can place it (used for movable arena props).
pub fn object_items(scene: &gmi::GmiScene, object_name: &str, dir: &Path) -> Vec<DrawItem> {
    object_items_opts(scene, object_name, dir, true)
}

/// Visual batches for one geometry object, in `.gmf`/scene space.
pub fn object_scene_items(scene: &gmi::GmiScene, object_name: &str, dir: &Path) -> Vec<DrawItem> {
    object_items_opts(scene, object_name, dir, false)
}

fn object_items_opts(
    scene: &gmi::GmiScene,
    object_name: &str,
    dir: &Path,
    object_local: bool,
) -> Vec<DrawItem> {
    let mut items = Vec::new();
    for (tex, color, cull, verts) in
        scene_material_groups_opts(scene, Mat4::IDENTITY, dir, Some(object_name), object_local)
    {
        if verts.is_empty() {
            continue;
        }
        let rgb = [
            (color[0] * 255.0) as u8,
            (color[1] * 255.0) as u8,
            (color[2] * 255.0) as u8,
            255,
        ];
        let (tex, tw, th) = tex
            .as_deref()
            .and_then(|p| decode_image(&std::fs::read(p).ok()?))
            .unwrap_or_else(|| (solid(rgb, 4, 4), 4, 4));
        items.push(DrawItem {
            vertices: verts,
            tex,
            tw,
            th,
            label: object_name.to_string(),
            blend: color[3] < 0.999,
            cull,
        });
    }
    items
}

/// Every mesh point of a scene in scene space (object transforms applied).
/// `include_collision` keeps the invisible physics proxies (for colliders).
pub fn scene_points(scene: &gmi::GmiScene, include_collision: bool) -> Vec<Vec3> {
    let mut out = Vec::new();
    for o in &scene.objects {
        let Some(mesh) = &o.mesh else { continue };
        if mesh.points.is_empty() {
            continue;
        }
        if !include_collision
            && (mesh.is_collision() || is_hidden(&o.name) || is_collision_name(&o.name))
        {
            continue;
        }
        let world = tm_mat4(&o.transform);
        out.extend(
            mesh.points
                .iter()
                .map(|p| world.transform_point3(Vec3::from(*p))),
        );
    }
    out
}

/// Object-local `(name, points)` for every geometry object with a mesh.
pub fn object_points(scene: &gmi::GmiScene) -> Vec<(String, Vec<Vec3>)> {
    scene
        .objects
        .iter()
        .filter_map(|o| {
            let m = o.mesh.as_ref()?;
            if m.points.is_empty() {
                return None;
            }
            Some((
                o.name.clone(),
                m.points.iter().map(|p| Vec3::from(*p)).collect(),
            ))
        })
        .collect()
}

/// The transform of a geometry object within its `.gmf`.
pub fn object_transform(object: &gmi::SceneObject) -> Mat4 {
    tm_mat4(&object.transform)
}

fn add_gmi(
    path: &Path,
    world: Mat4,
    items: &mut Vec<DrawItem>,
) -> Result<(), Box<dyn std::error::Error>> {
    let data = std::fs::read(path)?;
    let scene = gmi::parse(&data)?;
    let dir = path.parent().unwrap_or(Path::new("."));
    push_scene(&scene, world, dir, &path.display().to_string(), items);
    Ok(())
}

pub fn find_game_root(start: &Path) -> Option<PathBuf> {
    // The same override the tests and the workshop library use: point the
    // engine at a game directory from anywhere, including a saved bot in a
    // temporary folder.
    if let Some(root) = std::env::var_os("RA2_GAME") {
        let root = PathBuf::from(root);
        if root.join("Components").is_dir() {
            return Some(root);
        }
    }
    let mut dir = start.parent()?.to_path_buf();
    for _ in 0..6 {
        if dir.join("Components").is_dir() {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

pub(crate) fn decode_image(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    // TGA has no magic bytes, so the image crate cannot guess it; fall back to
    // an explicit TGA decode (the chassis skin and many textures are TGA).
    let img = image::load_from_memory(bytes)
        .or_else(|_| image::load_from_memory_with_format(bytes, image::ImageFormat::Tga))
        .ok()?
        .to_rgba8();
    let (w, h) = (img.width(), img.height());
    Some((img.into_raw(), w, h))
}

fn solid(rgb: [u8; 4], w: u32, h: u32) -> Vec<u8> {
    rgb.iter()
        .cycle()
        .take((w * h * 4) as usize)
        .copied()
        .collect()
}

// ---------------------------------------------------------------------------
// camera
// ---------------------------------------------------------------------------

/// A camera override resolved from `RA2_CAM`.
pub struct CamOverride {
    pub eye: Vec3,
    pub target: Option<Vec3>,
}

/// Parse `RA2_CAM`: either `ex,ey,ez,tx,ty,tz`, or `@<n>` / `@<name>` to use a
/// static camera declared in an arena script (aimed at the scene centre).
pub fn camera_override(input: &str) -> Option<CamOverride> {
    let spec = std::env::var("RA2_CAM").ok()?;
    let spec = spec.trim();
    if let Some(rest) = spec.strip_prefix('@') {
        let script = ra2fmt::arena::parse_path(Path::new(input)).ok()?;
        let cam = rest
            .parse::<usize>()
            .ok()
            .and_then(|i| script.static_cameras.get(i))
            .or_else(|| {
                script
                    .static_cameras
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(rest))
            })?;
        return Some(CamOverride {
            eye: Vec3::from(cam.position),
            target: None,
        });
    }
    let f: Vec<f32> = spec
        .split(',')
        .filter_map(|x| x.trim().parse().ok())
        .collect();
    if f.len() >= 6 {
        Some(CamOverride {
            eye: Vec3::new(f[0], f[1], f[2]),
            target: Some(Vec3::new(f[3], f[4], f[5])),
        })
    } else {
        None
    }
}

pub fn mvp_matrix(center: Vec3, radius: f32, w: u32, h: u32, cam: Option<CamOverride>) -> Mat4 {
    let aspect = w as f32 / h as f32;
    if let Some(cam) = cam {
        let target = cam.target.unwrap_or(center);
        let dist = (cam.eye - target).length().max(0.001);
        let view = look_at_lh(cam.eye, target, up_for(cam.eye - target));
        let proj = perspective_lh(
            50f32.to_radians(),
            aspect,
            (dist * 0.01).max(0.01),
            dist * 100.0,
        );
        return proj * view;
    }
    // RA2 assets are Y-up. Frame a three-quarter view from above/front/right.
    let dir = Vec3::new(0.75, 0.95, -1.0).normalize();
    let eye = center + dir * (radius * 3.0);
    let view = look_at_lh(eye, center, Vec3::Y);
    let proj = perspective_lh(45f32.to_radians(), aspect, radius * 0.01, radius * 100.0);
    proj * view
}

/// Choose an up vector that is not parallel to the view direction.
fn up_for(dir: Vec3) -> Vec3 {
    if dir.normalize_or_zero().dot(Vec3::Y).abs() > 0.999 {
        Vec3::Z
    } else {
        Vec3::Y
    }
}

/// Left-handed look-at (`D3DXMatrixLookAtLH`). RA2's data is Direct3D
/// left-handed (+X right, +Y up, +Z forward), so the view is built left-handed
/// too; a right-handed look-at mirror-images the world. This and
/// [`perspective_lh`] are the single conversion point for every 3D view.
pub fn look_at_lh(eye: Vec3, center: Vec3, up: Vec3) -> Mat4 {
    let f = (center - eye).normalize();
    let s = up.cross(f).normalize();
    let u = f.cross(s);
    Mat4::from_cols(
        glam::Vec4::new(s.x, u.x, f.x, 0.0),
        glam::Vec4::new(s.y, u.y, f.y, 0.0),
        glam::Vec4::new(s.z, u.z, f.z, 0.0),
        glam::Vec4::new(-s.dot(eye), -u.dot(eye), -f.dot(eye), 1.0),
    )
}

/// Left-handed perspective (`D3DXMatrixPerspectiveFovLH`) with 0..1 depth.
pub fn perspective_lh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    Mat4::from_cols(
        glam::Vec4::new(f / aspect, 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, f, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, far / (far - near), 1.0),
        glam::Vec4::new(0.0, 0.0, -near * far / (far - near), 0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every stock `.bot`, when the (uncommitted) game data is present.
    fn stock_bots() -> Vec<std::path::PathBuf> {
        if let Some(root) = std::env::var_os("RA2_GAME") {
            let root = std::path::PathBuf::from(root);
            if root.join("Components").is_dir() {
                return bots_under(&root);
            }
        }
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let game = loop {
            let candidate = dir.join("local/game");
            if candidate.is_dir() {
                break candidate;
            }
            if !dir.pop() {
                return Vec::new();
            }
        };
        bots_under(&game)
    }

    fn bots_under(game: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![game.to_path_buf()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "bot") {
                    out.push(p);
                }
            }
        }
        out.sort();
        out
    }

    /// The (uncommitted) game directory, when present.
    fn game_root() -> Option<std::path::PathBuf> {
        stock_bots()
            .first()
            .and_then(|p| p.ancestors().find(|a| a.ends_with("game")).map(Into::into))
    }

    /// F5: an arena's main collider simulates its *display proxy*.
    ///
    /// The Octagon's `main_collision` has a 65-vertex mesh of its own and
    /// declares `SIMULATION_GEOMETRY 5` with display proxy `main_display` — a
    /// group of 57 child objects. Taking the body's own mesh instead throws most
    /// of the arena away.
    #[test]
    fn arena_collider_follows_the_display_proxy() {
        let Some(game) = game_root() else { return };
        let path = game.join("Arenas/octagon.py");
        let Some(havok) = collect_havok(&path.to_string_lossy()).unwrap() else {
            return;
        };
        let main = havok
            .bodies
            .iter()
            .find(|b| b.name == "main_collision")
            .expect("the Octagon declares main_collision");
        assert_eq!(main.simulation_geometry, 5);
        assert!(main.use_display_proxy);
        assert_eq!(main.display_object, "main_display");
        // Its own mesh is 108 triangles; the group it names is far bigger.
        assert!(
            main.points.len() / 3 > 350,
            "only {} triangles from the display proxy",
            main.points.len() / 3
        );

        // `topplane_collision` is a plane primitive: no mesh in the file, and
        // its transform puts a downward-facing plane 15.6 m up.
        let plane = havok
            .bodies
            .iter()
            .find(|b| b.name == "topplane_collision")
            .expect("the Octagon declares topplane_collision");
        assert!(plane.is_plane());
        assert_eq!(plane.points.len() / 3, 2);
        assert!(plane.transform.transform_point3(Vec3::ZERO).y > 15.0);
        assert!(plane.transform.transform_vector3(Vec3::Y).y < -0.99);
    }

    /// F11: arenas declare their own spawn points, and they look at the centre.
    ///
    /// The duel used to invent a spread at `(max-min)*0.12`, which for the
    /// Octagon lands on the rim of the 3.7 m drop around its play platform.
    #[test]
    fn start_points_sit_on_the_play_surface() {
        let Some(game) = game_root() else { return };
        let path = game.join("Arenas/octagon.py");
        let starts = start_points(&path.to_string_lossy()).unwrap();
        assert_eq!(starts.len(), 4);
        for (pos, facing) in &starts {
            let radius = pos.x.hypot(pos.z);
            assert!((radius - 8.0).abs() < 0.6, "at {pos:?}, r = {radius}");
            let to_centre = Vec3::new(-pos.x, 0.0, -pos.z).normalize_or_zero();
            assert!(
                facing.dot(to_centre) > 0.99,
                "{facing:?} does not face {to_centre:?}"
            );
        }
    }

    /// F1: the shipped rule must actually *snap* each chain link.
    ///
    /// A `.bot` names a point on the child and the point it mates with on the
    /// parent; the placement has to bring those two points together, which also
    /// constrains how `angle` and a burst motor's start angle may be applied:
    /// both must turn about the parent point's Z, through its origin. The test
    /// also proves it has teeth — under `stored` the same links miss badly.
    #[test]
    fn chain_links_snap_onto_the_parent_point() {
        let bots = stock_bots();
        if bots.is_empty() {
            return; // game data not available
        }
        let mut checked = 0usize;
        let mut stored_misses = 0usize;
        for path in &bots {
            let (bot, _root, loaded) = load_bot_components(path.to_str().unwrap()).unwrap();
            let shipped = component_worlds_mounted(&bot, &loaded, MountMode::FramesFlipped);
            let stored = component_worlds_mounted(&bot, &loaded, MountMode::Stored);
            for (i, lc) in loaded.iter().enumerate() {
                let Some(lc) = lc else { continue };
                let c = &bot.components[i];
                let parent = c.parent.max(0) as usize;
                if parent == 0 || parent >= i {
                    continue; // the chassis has no `.gmf`: nothing to snap to
                }
                let Some(parent_scene) = loaded.get(parent).and_then(|p| p.as_ref()) else {
                    continue;
                };
                let own = lc.scene.attachment_table();
                let on_parent = parent_scene.scene.attachment_table();
                let (Some(own), Some(on_parent)) =
                    (own.get(&c.own_mount), on_parent.get(&c.parent_mount))
                else {
                    continue;
                };
                let gap = |world: &[Mat4]| {
                    let child = (world[i] * tm_mat4(&own.transform)).transform_point3(Vec3::ZERO);
                    let par = (world[parent] * tm_mat4(&on_parent.transform))
                        .transform_point3(Vec3::ZERO);
                    (child - par).length()
                };
                let g = gap(&shipped);
                assert!(
                    g < 1e-3,
                    "{}: component {i} ({}) misses its parent point by {g}",
                    path.display(),
                    lc.label
                );
                if gap(&stored) > 0.05 {
                    stored_misses += 1;
                }
                checked += 1;
            }
        }
        assert!(checked > 100, "only checked {checked} links");
        assert!(
            stored_misses > 100,
            "the test has no teeth: only {stored_misses} links miss under `stored`"
        );
    }

    /// F1: `angle` must turn each chain link about the **mount axis**.
    ///
    /// `AI/Team14/Bot2` is the clean case: four identical anglers hang on the
    /// disc's four rim points, which are 90° apart but share their axes, and
    /// their `angle` values are exactly the rim azimuths (0, pi/2, pi, 3pi/2).
    /// Undoing the spin by that angle — one way round or the other — must make
    /// all four arms coincide. That is what proves the rotation is about the
    /// mount normal rather than the point's Y or a world axis; a wrong axis
    /// leaves the four arms scattered either way.
    ///
    /// Exactly one winding may work. Which one is a convention, settled by the
    /// bots' own previews (see F1), so it is deliberately *not* asserted here.
    #[test]
    fn the_mount_spin_turns_about_the_mount_axis() {
        let path = {
            let bots = stock_bots();
            match bots
                .iter()
                .find(|p| p.ends_with("Team14/Bot2.bot") || p.ends_with("Team14\\Bot2.bot"))
            {
                Some(p) => p.clone(),
                None => return, // game data not available
            }
        };
        let (bot, _root, loaded) = load_bot_components(path.to_str().unwrap()).unwrap();
        let world = component_worlds_mounted(&bot, &loaded, MountMode::FramesFlipped);
        let label = |i: usize| {
            loaded
                .get(i)
                .and_then(|l| l.as_ref())
                .map(|l| l.label.clone())
                .unwrap_or_default()
        };
        let disc = world
            .iter()
            .enumerate()
            .find(|(i, _)| label(*i).contains("Disc"))
            .map(|(i, m)| (i, m.transform_point3(Vec3::ZERO)))
            .expect("no disc in Bot2");
        let m = world[disc.0];
        let axes = [
            m.transform_vector3(Vec3::X).normalize(),
            m.transform_vector3(Vec3::Y).normalize(),
            m.transform_vector3(Vec3::Z).normalize(),
        ];
        let to_disc = |v: Vec3| Vec3::new(v.dot(axes[0]), v.dot(axes[1]), v.dot(axes[2]));
        let spin_z = |v: Vec3, t: f32| {
            Vec3::new(
                v.x * t.cos() - v.y * t.sin(),
                v.x * t.sin() + v.y * t.cos(),
                v.z,
            )
        };

        // (angle, arm in the disc's frame) for each angler.
        let mut links: Vec<(f32, Vec3)> = Vec::new();
        for (i, m) in world.iter().enumerate() {
            if !label(i).contains("Angle Connector") {
                continue;
            }
            let parent = bot.components[i].parent.max(0) as usize;
            if parent != disc.0 {
                continue;
            }
            let angler = m.transform_point3(Vec3::ZERO);
            let Some((_, claw_world)) = world
                .iter()
                .enumerate()
                .find(|(j, _)| bot.components[*j].parent == i as i32)
            else {
                continue;
            };
            let claw = claw_world.transform_point3(Vec3::ZERO);
            links.push((bot.components[i].angle, to_disc(claw - angler).normalize()));
        }
        assert_eq!(links.len(), 4, "expected four anglers, saw {}", links.len());

        let spread = |winding: f32| {
            let v: Vec<Vec3> = links
                .iter()
                .map(|(a, arm)| spin_z(*arm, winding * a))
                .collect();
            let mut worst = 0.0f32;
            for i in 0..v.len() {
                for j in (i + 1)..v.len() {
                    worst = worst.max((v[i] - v[j]).length());
                }
            }
            worst
        };
        let (plus, minus) = (spread(1.0), spread(-1.0));
        assert!(
            (plus < 1e-2) ^ (minus < 1e-2),
            "expected exactly one winding to line the arms up, got +1:{plus} -1:{minus}"
        );
    }
}
