//! The wgpu renderer: a software model of Direct3D 8's fixed-function pipeline
//! as the original drives it (see [`crate::material`]).
//!
//! Per draw it reproduces, in WGSL: per-vertex Gouraud lighting (emissive 0,
//! `ambient x global ambient`, plus point lights with D3D8's range and
//! attenuation, and the local-viewer Blinn specular term added after the
//! texture stages when the material's power is positive), up to three texture stages with their own UV set
//! and point/bilinear filter, lightmap stages, the sphere environment map, the
//! `GREATEREQUAL 0x10` alpha test and `SRCALPHA` blending (additive or not).
//! Frame order is the original's: opaque batches, then alpha-tested batches
//! sorted back to front, then every blended triangle sorted back to front.
//!
//! GPU resources are cached across frames: textures by the identity of their
//! shared [`Image`], vertex buffers by a content hash, bind groups by their
//! texture set, and the per-draw uniforms live in one persistent buffer.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;

use crate::material::{Image, Light, Material, StageKind, GLOBAL_AMBIENT};

/// One render vertex: position, normal, two UV sets and the vertex colour
/// (`D3DFVF_DIFFUSE`, only read when the material says the mesh carries colours).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    pub uv: [f32; 2],
    pub uv2: [f32; 2],
    pub color: [f32; 4],
}

impl Vertex {
    /// A plain vertex (position, normal, uv); the second UV set mirrors the
    /// first and the vertex colour is white.
    pub fn new(pos: Vec3, nrm: Vec3, uv: [f32; 2]) -> Self {
        Vertex {
            pos: pos.into(),
            nrm: nrm.into(),
            uv,
            uv2: uv,
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

/// One drawable batch: triangles sharing a material.
#[derive(Clone)]
pub struct DrawItem {
    pub vertices: Vec<Vertex>,
    pub label: String,
    pub material: Material,
    /// The lights of the scene this batch was loaded from. The renderer lights
    /// the whole frame with the first set it finds (or [`Renderer::set_lights`]).
    pub scene_lights: Option<Arc<Vec<Light>>>,
}

impl DrawItem {
    pub fn new(vertices: Vec<Vertex>, material: Material, label: impl Into<String>) -> Self {
        DrawItem {
            vertices,
            label: label.into(),
            material,
            scene_lights: None,
        }
    }

    /// Texture size of the first stage, for debug output.
    pub fn tex_size(&self) -> (u32, u32) {
        self.material
            .stages
            .first()
            .map(|s| (s.image.w, s.image.h))
            .unwrap_or((0, 0))
    }
}

/// A set of items sharing one model transform within a frame.
pub struct RenderGroup<'a> {
    pub items: &'a [DrawItem],
    pub model: Mat4,
}

const MAX_LIGHTS: usize = 8;
/// Bytes per draw in the dynamic uniform buffer (the 256-byte offset alignment
/// rounds the 272-byte struct up).
const DRAW_STRIDE: u64 = 512;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct DrawUniform {
    mvp: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    cam_right: [f32; 4],
    cam_up: [f32; 4],
    cam_fwd: [f32; 4],
    diffuse: [f32; 4],
    ambient: [f32; 4],
    /// x: bit0 lighting, bit1 vertex colours present, bit2 vertex-colour
    /// ambient, bit3 alpha test, bit4 transparent.
    flags: [u32; 4],
    /// Per stage: bits 0-1 kind (0 off, 1 modulate, 2 lightmap, 3 env),
    /// bit 2 UV set, bit 3 bilinear.
    stages: [u32; 4],
    /// `D3DMATERIAL8.Specular` rgb, w = power (0: `SPECULARENABLE` off).
    specular: [f32; 4],
    /// The camera's world position (`D3DRS_LOCALVIEWER` is TRUE by default).
    eye: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct LightUniform {
    /// xyz position, w range.
    pos: [f32; 4],
    diffuse: [f32; 4],
    atten: [f32; 4],
    specular: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct FrameUniform {
    ambient: [f32; 4],
    lights: [LightUniform; MAX_LIGHTS],
    count: [u32; 4],
}

const SHADER: &str = r#"
struct Draw {
  mvp: mat4x4<f32>,
  model: mat4x4<f32>,
  cam_right: vec4<f32>,
  cam_up: vec4<f32>,
  cam_fwd: vec4<f32>,
  diffuse: vec4<f32>,
  ambient: vec4<f32>,
  flags: vec4<u32>,
  stages: vec4<u32>,
  specular: vec4<f32>,
  eye: vec4<f32>,
};
struct Light { pos: vec4<f32>, diffuse: vec4<f32>, atten: vec4<f32>, specular: vec4<f32>, };
struct Frame { ambient: vec4<f32>, lights: array<Light, 8>, count: vec4<u32>, };

@group(0) @binding(0) var<uniform> d: Draw;
@group(0) @binding(1) var<uniform> frame: Frame;
@group(0) @binding(2) var sp: sampler;
@group(0) @binding(3) var sl: sampler;
@group(1) @binding(0) var t0: texture_2d<f32>;
@group(1) @binding(1) var t1: texture_2d<f32>;
@group(1) @binding(2) var t2: texture_2d<f32>;

struct VsIn {
  @location(0) pos: vec3<f32>,
  @location(1) nrm: vec3<f32>,
  @location(2) uv: vec2<f32>,
  @location(3) uv2: vec2<f32>,
  @location(4) color: vec4<f32>,
};
struct VsOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv0: vec2<f32>,
  @location(1) uv1: vec2<f32>,
  @location(2) uv_env: vec2<f32>,
  @location(3) diffuse: vec4<f32>,
  @location(4) specular: vec3<f32>,
};

@vertex
fn vs_main(i: VsIn) -> VsOut {
  var o: VsOut;
  o.clip = d.mvp * vec4<f32>(i.pos, 1.0);
  o.uv0 = i.uv;
  o.uv1 = i.uv2;
  let wp = (d.model * vec4<f32>(i.pos, 1.0)).xyz;
  // D3DRS_NORMALIZENORMALS is 0 in every lit draw of the original (live state dump): the normal keeps the
  // length the file gave it (the Buzzard wheel's hub parts and the slipper wheel are 0.586 long, so
  // they light 0.586 as bright; docs/34 section 11.5).
  let n = (d.model * vec4<f32>(i.nrm, 0.0)).xyz;
  let has_vc = (d.flags.x & 2u) != 0u;
  // D3DRS_DIFFUSEMATERIALSOURCE stays COLOR1: the vertex colour when the
  // vertex has one, else the material.
  let md = select(d.diffuse, i.color, has_vc);
  if ((d.flags.x & 1u) != 0u) {
    var ma = d.ambient;
    if (has_vc && (d.flags.x & 4u) != 0u) { ma = i.color; }
    var c = ma.rgb * frame.ambient.rgb;
    var sp = vec3<f32>(0.0);
    // D3DRS_LOCALVIEWER is FALSE in the original (live state dump, every draw): the viewer vector is the
    // camera's -forward for every vertex, not the direction to the eye.
    let to_eye = -normalize(d.cam_fwd.xyz);
    for (var k = 0u; k < frame.count.x; k = k + 1u) {
      let l = frame.lights[k];
      let to = l.pos.xyz - wp;
      let dist = length(to);
      if (dist > l.pos.w || dist <= 0.0) { continue; }
      let att = 1.0 / (l.atten.x + l.atten.y * dist + l.atten.z * dist * dist);
      let ldir = to / dist;
      let ndl = dot(n, ldir);
      if (ndl <= 0.0) { continue; }
      c = c + md.rgb * l.diffuse.rgb * (ndl * att);
      if (d.specular.w > 0.0) {
        let ndh = dot(n, normalize(ldir + to_eye));
        if (ndh > 0.0) {
          sp = sp + d.specular.rgb * l.specular.rgb * (pow(ndh, d.specular.w) * att);
        }
      }
    }
    o.diffuse = vec4<f32>(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), md.a);
    o.specular = clamp(sp, vec3<f32>(0.0), vec3<f32>(1.0));
  } else {
    // Lighting off: the vertex colour as is, or white.
    o.diffuse = select(vec4<f32>(1.0), i.color, has_vc);
    o.specular = vec3<f32>(0.0);
  }
  // Sphere map: camera-space normal through the texture matrix
  // (0.5, 0; 0, -0.5; 0.5, 0.5).
  let nc = vec3<f32>(dot(n, d.cam_right.xyz), dot(n, d.cam_up.xyz), dot(n, d.cam_fwd.xyz));
  o.uv_env = vec2<f32>(0.5 * nc.x + 0.5, -0.5 * nc.y + 0.5);
  return o;
}

fn fetch(stage: u32, uv: vec2<f32>, linear: bool) -> vec4<f32> {
  if (stage == 0u) {
    return select(textureSampleLevel(t0, sp, uv, 0.0), textureSampleLevel(t0, sl, uv, 0.0), linear);
  }
  if (stage == 1u) {
    return select(textureSampleLevel(t1, sp, uv, 0.0), textureSampleLevel(t1, sl, uv, 0.0), linear);
  }
  return select(textureSampleLevel(t2, sp, uv, 0.0), textureSampleLevel(t2, sl, uv, 0.0), linear);
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
  var cur = i.diffuse;
  for (var s = 0u; s < 3u; s = s + 1u) {
    let info = d.stages[s];
    let kind = info & 3u;
    if (kind == 0u) { break; }
    var uv = select(i.uv0, i.uv1, (info & 4u) != 0u);
    if (kind == 3u) { uv = i.uv_env; }
    let t = fetch(s, uv, (info & 8u) != 0u);
    if (kind == 3u) {
      cur = vec4<f32>(t.rgb + t.a * cur.rgb, cur.a);
    } else {
      cur = vec4<f32>(t.rgb * cur.rgb, cur.a);
      if (s == 0u) {
        // ALPHAOP: MODULATE when transparent, else SELECTARG1 (the texture).
        cur.a = select(t.a, t.a * cur.a, (d.flags.x & 16u) != 0u);
      }
    }
  }
  // D3DRS_SPECULARENABLE: the specular colour is added after the stages.
  cur = vec4<f32>(min(cur.rgb + i.specular, vec3<f32>(1.0)), cur.a);
  if ((d.flags.x & 8u) != 0u && round(cur.a * 255.0) < 16.0) { discard; }
  return cur;
}
"#;

/// Pipeline state variants (everything `FUN_005b24c0` switches that wgpu bakes).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PipelineKey {
    /// 0 opaque, 1 alpha blend, 2 additive.
    blend: u8,
    z_write: bool,
    z_test: bool,
    cull: bool,
}

struct Frameful<T> {
    value: T,
    used: u64,
}

struct GpuTexture {
    /// Keeps the allocation alive so its address stays a valid key.
    _image: Arc<Image>,
    view: wgpu::TextureView,
}

struct GpuBindGroup {
    group: wgpu::BindGroup,
    _images: Vec<Arc<Image>>,
}

struct GpuBuffer {
    buffer: wgpu::Buffer,
}

/// Reusable offscreen wgpu renderer. Holds the GPU device, pipelines and
/// resource caches so a caller can render many frames without re-initialising.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bgl_frame: wgpu::BindGroupLayout,
    bgl_tex: wgpu::BindGroupLayout,
    shader: wgpu::ShaderModule,
    sampler_point: wgpu::Sampler,
    sampler_linear: wgpu::Sampler,
    color_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
    /// Pipelines are specific to their colour target's format.
    pipelines: RefCell<HashMap<(wgpu::TextureFormat, PipelineKey), Rc<wgpu::RenderPipeline>>>,
    /// Clear colour (the arena's `setBackColor`), RGB 0..1 in the target's own
    /// (non-linear) byte space, like the original's D3D8 clear.
    background: Cell<[f32; 3]>,
    /// Explicit scene lights; `None` takes them from the frame's items.
    lights: RefCell<Option<Vec<Light>>>,
    frame: Cell<u64>,
    textures: RefCell<HashMap<usize, Frameful<GpuTexture>>>,
    white: GpuTexture,
    bind_groups: RefCell<HashMap<[usize; 3], Frameful<GpuBindGroup>>>,
    buffers: RefCell<HashMap<u64, Frameful<GpuBuffer>>>,
    draw_buf: RefCell<(wgpu::Buffer, u64)>,
    frame_buf: wgpu::Buffer,
}

impl Renderer {
    /// Set the clear colour of every following frame (RGB 0..1).
    pub fn set_background(&self, rgb: [f32; 3]) {
        self.background.set(rgb);
    }

    /// Light the following frames with these lights (at most eight are used),
    /// overriding the ones the draw items carry. An empty slice means no lights.
    pub fn set_lights(&self, lights: &[Light]) {
        *self.lights.borrow_mut() = Some(lights.to_vec());
    }

    /// Go back to taking the lights from the frame's draw items.
    pub fn clear_lights(&self) {
        *self.lights.borrow_mut() = None;
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

        let uniform = |binding, dynamic| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: None,
            },
            count: None,
        };
        let sampler = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let bgl_frame = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[uniform(0, true), uniform(1, false), sampler(2), sampler(3)],
        });
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let bgl_tex = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("textures"),
            entries: &[tex_entry(0), tex_entry(1), tex_entry(2)],
        });

        // Direct3D 8's default texture addressing is WRAP; mip filtering is
        // never enabled by the original, so the samplers have no mip chain.
        let make_sampler = |filter| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            })
        };
        let sampler_point = make_sampler(wgpu::FilterMode::Nearest);
        let sampler_linear = make_sampler(wgpu::FilterMode::Linear);

        let white = upload_texture(&device, &queue, &Image::solid([255; 4]));
        let draw_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("draw-uniforms"),
            size: DRAW_STRIDE * 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame-uniform"),
            size: std::mem::size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let renderer = Renderer {
            device,
            queue,
            bgl_frame,
            bgl_tex,
            shader,
            sampler_point,
            sampler_linear,
            color_format,
            depth_format,
            pipelines: RefCell::new(HashMap::new()),
            background: Cell::new([0.06, 0.07, 0.09]),
            lights: RefCell::new(None),
            frame: Cell::new(0),
            textures: RefCell::new(HashMap::new()),
            white,
            bind_groups: RefCell::new(HashMap::new()),
            buffers: RefCell::new(HashMap::new()),
            draw_buf: RefCell::new((draw_buf, DRAW_STRIDE * 64)),
            frame_buf,
        };
        // Build the common pipelines now, so a missing feature is reported by
        // the constructor rather than by the first frame.
        let _ = renderer.pipeline(
            color_format,
            PipelineKey {
                blend: 0,
                z_write: true,
                z_test: true,
                cull: true,
            },
        );
        Ok(renderer)
    }

    fn pipeline(&self, format: wgpu::TextureFormat, key: PipelineKey) -> Rc<wgpu::RenderPipeline> {
        if let Some(found) = self.pipelines.borrow().get(&(format, key)) {
            return found.clone();
        }
        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&self.bgl_frame), Some(&self.bgl_tex)],
                immediate_size: 0,
            });
        let attrs = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x2, 4 => Float32x4
        ];
        // `RA2_CULL=off` disables the per-material culling for debugging.
        let cull_on = std::env::var("RA2_CULL").as_deref() != Ok("off");
        let blend = match key.blend {
            0 => None,
            1 => Some(wgpu::BlendState::ALPHA_BLENDING),
            _ => Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            }),
        };
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("mesh"),
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
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    // The GMF triangles' right-handed cross product points into
                    // the solid; in the left-handed view that reads
                    // counter-clockwise from outside, so that is the front.
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: (key.cull && cull_on).then_some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: self.depth_format,
                    depth_write_enabled: Some(key.z_write),
                    // D3DRS_ZFUNC is left at its default, LESSEQUAL.
                    depth_compare: Some(if key.z_test {
                        wgpu::CompareFunction::LessEqual
                    } else {
                        wgpu::CompareFunction::Always
                    }),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        let pipeline = Rc::new(pipeline);
        self.pipelines
            .borrow_mut()
            .insert((format, key), pipeline.clone());
        pipeline
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
        let extent = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("color"),
            size: extent,
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
            size: extent,
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

    fn texture_view(&self, image: &Arc<Image>, frame: u64) -> usize {
        let key = Arc::as_ptr(image) as usize;
        let mut cache = self.textures.borrow_mut();
        let entry = cache.entry(key).or_insert_with(|| Frameful {
            value: upload_texture(&self.device, &self.queue, image),
            used: frame,
        });
        entry.used = frame;
        key
    }

    /// The texture bind group for a material's stage images (cached).
    fn bind_group(&self, material: &Material, frame: u64) -> [usize; 3] {
        let mut keys = [0usize; 3];
        for (k, s) in keys.iter_mut().zip(&material.stages) {
            *k = self.texture_view(&s.image, frame);
        }
        let mut groups = self.bind_groups.borrow_mut();
        if let Some(g) = groups.get_mut(&keys) {
            g.used = frame;
            return keys;
        }
        let textures = self.textures.borrow();
        let view = |k: usize| -> &wgpu::TextureView {
            if k == 0 {
                &self.white.view
            } else {
                &textures[&k].value.view
            }
        };
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tex"),
            layout: &self.bgl_tex,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view(keys[0])),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view(keys[1])),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(view(keys[2])),
                },
            ],
        });
        groups.insert(
            keys,
            Frameful {
                value: GpuBindGroup {
                    group,
                    _images: material.stages.iter().map(|s| s.image.clone()).collect(),
                },
                used: frame,
            },
        );
        keys
    }

    /// A vertex buffer for `vertices`, cached by content.
    fn vertex_buffer(&self, vertices: &[Vertex], frame: u64) -> u64 {
        let bytes: &[u8] = bytemuck::cast_slice(vertices);
        let key = hash_bytes(bytes);
        let mut cache = self.buffers.borrow_mut();
        let entry = cache.entry(key).or_insert_with(|| Frameful {
            value: GpuBuffer {
                buffer: self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("vertices"),
                        contents: bytes,
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
            },
            used: frame,
        });
        entry.used = frame;
        key
    }

    /// Draw several groups into a caller-supplied colour target and depth
    /// buffer.
    ///
    /// This is [`Renderer::render_groups`] without the private texture and the
    /// readback. `format` must be the colour target's own format; pipelines for
    /// a format are built on first use and cached.
    pub fn render_groups_to(
        &self,
        groups: &[RenderGroup<'_>],
        view_proj: Mat4,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let frame = self.frame.get() + 1;
        self.frame.set(frame);

        // Camera axes from the view-projection (`w_clip` is the view-space z,
        // clip x/y are scaled right/up).
        let row = |r: usize| {
            Vec4::new(
                view_proj.x_axis[r],
                view_proj.y_axis[r],
                view_proj.z_axis[r],
                view_proj.w_axis[r],
            )
        };
        let fwd = row(3).truncate().normalize_or_zero();
        let right = row(0).truncate().normalize_or_zero();
        let up = row(1).truncate().normalize_or_zero();
        // The eye: the point the projection sends to clip (0, 0, *, 0).
        let eye_h = view_proj.inverse() * Vec4::new(0.0, 0.0, 1.0, 0.0);
        let eye = if eye_h.w.abs() > 1e-12 {
            eye_h.truncate() / eye_h.w
        } else {
            -fwd * 1.0e6
        };

        // Frame uniform: global ambient and the scene lights.
        let lights: Vec<Light> = match &*self.lights.borrow() {
            Some(l) => l.clone(),
            None => groups
                .iter()
                .flat_map(|g| g.items.iter())
                .find_map(|i| i.scene_lights.as_ref())
                .map(|l| l.to_vec())
                // Not from the original: a frame whose items carry no scene
                // lights (a lone robot or component in a viewer) gets one
                // distant key light, so it is not drawn in pure ambient.
                .unwrap_or_else(|| vec![Light::viewer_key()]),
        };
        let mut fu = FrameUniform::zeroed();
        fu.ambient = [GLOBAL_AMBIENT[0], GLOBAL_AMBIENT[1], GLOBAL_AMBIENT[2], 1.0];
        for (slot, l) in fu.lights.iter_mut().zip(lights.iter().take(MAX_LIGHTS)) {
            slot.pos = [l.position.x, l.position.y, l.position.z, l.range];
            slot.diffuse = [l.diffuse[0], l.diffuse[1], l.diffuse[2], 1.0];
            slot.atten = [l.attenuation[0], l.attenuation[1], l.attenuation[2], 0.0];
            slot.specular = [l.specular[0], l.specular[1], l.specular[2], 0.0];
        }
        fu.count[0] = lights.len().min(MAX_LIGHTS) as u32;
        self.queue
            .write_buffer(&self.frame_buf, 0, bytemuck::bytes_of(&fu));

        // One draw per item for the opaque and alpha-tested passes; the blended
        // pass re-batches triangles after a global depth sort.
        struct Call {
            /// Cached vertex buffer, or `None` for the transient blended one.
            vb: Option<u64>,
            first: u32,
            count: u32,
            tex: [usize; 3],
            key: PipelineKey,
            slot: u64,
        }
        let mut draws: Vec<DrawUniform> = Vec::new();
        let mut push_uniform = |model: Mat4, m: &Material| -> u64 {
            let mut stages = [0u32; 4];
            for (s, st) in stages.iter_mut().zip(&m.stages) {
                let kind = match st.kind {
                    StageKind::Modulate => 1,
                    StageKind::LightMap => 2,
                    StageKind::Env => 3,
                };
                *s = kind | (st.uv_set.min(1) << 2) | ((st.linear as u32) << 3);
            }
            let flags = (m.lighting as u32)
                | ((m.has_vertex_colors as u32) << 1)
                | ((m.vertex_color_ambient as u32) << 2)
                | ((m.alpha_tested() as u32) << 3)
                | ((m.transparent as u32) << 4);
            draws.push(DrawUniform {
                mvp: (view_proj * model).to_cols_array_2d(),
                model: model.to_cols_array_2d(),
                cam_right: [right.x, right.y, right.z, 0.0],
                cam_up: [up.x, up.y, up.z, 0.0],
                cam_fwd: [fwd.x, fwd.y, fwd.z, 0.0],
                diffuse: m.diffuse,
                ambient: m.ambient,
                flags: [flags, 0, 0, 0],
                stages,
                specular: [
                    m.specular[0],
                    m.specular[1],
                    m.specular[2],
                    if m.specular_enabled() { m.power } else { 0.0 },
                ],
                eye: [eye.x, eye.y, eye.z, 1.0],
            });
            (draws.len() - 1) as u64
        };
        let key_of = |m: &Material| PipelineKey {
            blend: if !m.transparent {
                0
            } else if m.additive {
                2
            } else {
                1
            },
            z_write: m.z_write,
            z_test: m.z_test,
            cull: m.cull,
        };

        let mut opaque: Vec<Call> = Vec::new();
        let mut early: Vec<Call> = Vec::new();
        // (depth, call) for whole alpha-tested batches.
        let mut tested: Vec<(f32, Call)> = Vec::new();
        // Blended triangles: (depth, item ref, first vertex).
        struct Tri {
            depth: f32,
            group: usize,
            item: usize,
            first: usize,
        }
        let mut tris: Vec<Tri> = Vec::new();
        for (gi, group) in groups.iter().enumerate() {
            for (ii, it) in group.items.iter().enumerate() {
                if it.vertices.len() < 3 {
                    continue;
                }
                let m = &it.material;
                if m.blended() && !m.early {
                    for (t, c) in it.vertices.as_chunks::<3>().0.iter().enumerate() {
                        let p =
                            (Vec3::from(c[0].pos) + Vec3::from(c[1].pos) + Vec3::from(c[2].pos))
                                / 3.0;
                        let world = group.model.transform_point3(p);
                        tris.push(Tri {
                            depth: world.dot(fwd),
                            group: gi,
                            item: ii,
                            first: t * 3,
                        });
                    }
                    continue;
                }
                let call = Call {
                    vb: Some(self.vertex_buffer(&it.vertices, frame)),
                    first: 0,
                    count: it.vertices.len() as u32 / 3 * 3,
                    tex: self.bind_group(m, frame),
                    key: key_of(m),
                    slot: push_uniform(group.model, m),
                };
                if m.early {
                    early.push(call);
                } else if m.alpha_tested() {
                    // Back to front by the batch's centre.
                    let mut c = Vec3::ZERO;
                    for v in &it.vertices {
                        c += Vec3::from(v.pos);
                    }
                    c /= it.vertices.len() as f32;
                    tested.push((group.model.transform_point3(c).dot(fwd), call));
                } else {
                    opaque.push(call);
                }
            }
        }
        tested.sort_by(|a, b| b.0.total_cmp(&a.0));
        tris.sort_by(|a, b| b.depth.total_cmp(&a.depth));

        // Re-batch the sorted blended triangles into runs of one material state,
        // in world space, in one transient vertex buffer.
        let mut blend_verts: Vec<Vertex> = Vec::new();
        // (item, first vertex, vertex count) of each run.
        let mut runs: Vec<((usize, usize), usize, usize)> = Vec::new();
        for t in &tris {
            let it = &groups[t.group].items[t.item];
            let extend = runs.last().is_some_and(|&((g, i), _, _)| {
                (g == t.group && i == t.item)
                    || same_state(&groups[g].items[i].material, &it.material)
            });
            if !extend {
                runs.push(((t.group, t.item), blend_verts.len(), 0));
            }
            let model = groups[t.group].model;
            let linear = glam::Mat3::from_mat4(model);
            let normal_mat = if linear.determinant().abs() > 1e-12 {
                linear.inverse().transpose()
            } else {
                glam::Mat3::IDENTITY
            };
            for v in &it.vertices[t.first..t.first + 3] {
                let mut w = *v;
                w.pos = model.transform_point3(Vec3::from(v.pos)).into();
                // the direction follows the model, the length stays the file's (NORMALIZENORMALS is off)
                w.nrm = ((normal_mat * Vec3::from(v.nrm)).normalize_or_zero()
                    * Vec3::from(v.nrm).length())
                .into();
                blend_verts.push(w);
            }
            if let Some(last) = runs.last_mut() {
                last.2 += 3;
            }
        }
        let mut blended: Vec<Call> = Vec::new();
        for &((g, i), first, count) in &runs {
            let m = &groups[g].items[i].material;
            blended.push(Call {
                vb: None,
                first: first as u32,
                count: count as u32,
                tex: self.bind_group(m, frame),
                key: key_of(m),
                slot: push_uniform(Mat4::IDENTITY, m),
            });
        }

        // Upload all draw uniforms.
        let needed = DRAW_STRIDE * draws.len().max(1) as u64;
        {
            let mut db = self.draw_buf.borrow_mut();
            if db.1 < needed {
                let size = needed.next_power_of_two();
                db.0 = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("draw-uniforms"),
                    size,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                db.1 = size;
            }
            let mut bytes = vec![0u8; needed as usize];
            for (i, d) in draws.iter().enumerate() {
                let at = i * DRAW_STRIDE as usize;
                bytes[at..at + std::mem::size_of::<DrawUniform>()]
                    .copy_from_slice(bytemuck::bytes_of(d));
            }
            self.queue.write_buffer(&db.0, 0, &bytes);
        }
        let db = self.draw_buf.borrow();
        let bg_frame = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame"),
            layout: &self.bgl_frame,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &db.0,
                        offset: 0,
                        size: wgpu::BufferSize::new(std::mem::size_of::<DrawUniform>() as u64),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.frame_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_point),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear),
                },
            ],
        });
        let blend_buf = (!blend_verts.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("blended"),
                    contents: bytemuck::cast_slice(&blend_verts),
                    usage: wgpu::BufferUsages::VERTEX,
                })
        });

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
            let buffers = self.buffers.borrow();
            let bind_groups = self.bind_groups.borrow();
            let mut last_key: Option<PipelineKey> = None;
            let mut run = |rp: &mut wgpu::RenderPass<'_>, c: &Call| {
                if last_key != Some(c.key) {
                    let p = self.pipeline(format, c.key);
                    rp.set_pipeline(&p);
                    last_key = Some(c.key);
                }
                rp.set_bind_group(0, &bg_frame, &[(c.slot * DRAW_STRIDE) as u32]);
                rp.set_bind_group(1, &bind_groups[&c.tex].value.group, &[]);
                match c.vb {
                    Some(k) => rp.set_vertex_buffer(0, buffers[&k].value.buffer.slice(..)),
                    None => {
                        let Some(buf) = &blend_buf else { return };
                        rp.set_vertex_buffer(0, buf.slice(..));
                    }
                }
                rp.draw(c.first..c.first + c.count, 0..1);
            };
            for c in early.iter().chain(opaque.iter()) {
                run(&mut rp, c);
            }
            for (_, c) in &tested {
                run(&mut rp, c);
            }
            for c in &blended {
                run(&mut rp, c);
            }
        }
        self.queue.submit(Some(encoder.finish()));
        drop(db);
        self.evict(frame);
        Ok(())
    }

    /// Drop cache entries unused for a while.
    fn evict(&self, frame: u64) {
        self.buffers
            .borrow_mut()
            .retain(|_, e| frame.saturating_sub(e.used) < 30);
        self.bind_groups
            .borrow_mut()
            .retain(|_, e| frame.saturating_sub(e.used) < 120);
        self.textures
            .borrow_mut()
            .retain(|_, e| frame.saturating_sub(e.used) < 240);
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

/// Whether two materials need identical device state (so consecutive blended
/// triangles can share a draw).
fn same_state(a: &Material, b: &Material) -> bool {
    a.additive == b.additive
        && a.cull == b.cull
        && a.lighting == b.lighting
        && a.has_vertex_colors == b.has_vertex_colors
        && a.vertex_color_ambient == b.vertex_color_ambient
        && a.diffuse == b.diffuse
        && a.ambient == b.ambient
        && a.stages.len() == b.stages.len()
        && a.stages.iter().zip(&b.stages).all(|(x, y)| {
            Arc::ptr_eq(&x.image, &y.image)
                && x.kind == y.kind
                && x.linear == y.linear
                && x.uv_set == y.uv_set
        })
}

fn upload_texture(device: &wgpu::Device, queue: &wgpu::Queue, image: &Arc<Image>) -> GpuTexture {
    let tex = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("albedo"),
            size: wgpu::Extent3d {
                width: image.w,
                height: image.h,
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
        &image.rgba,
    );
    GpuTexture {
        _image: image.clone(),
        view: tex.create_view(&wgpu::TextureViewDescriptor::default()),
    }
}

/// A quick 64-bit content hash (FxHash over 32-bit words).
fn hash_bytes(bytes: &[u8]) -> u64 {
    const K: u64 = 0x517c_c1b7_2722_0a95;
    let mut h: u64 = bytes.len() as u64;
    let (words, rest) = bytes.as_chunks::<8>();
    for c in words {
        h = (h.rotate_left(5) ^ u64::from_le_bytes(*c)).wrapping_mul(K);
    }
    for &b in rest {
        h = (h.rotate_left(5) ^ b as u64).wrapping_mul(K);
    }
    h
}
