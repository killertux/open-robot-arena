//! `ra2-render` — offscreen wgpu renderer for Robot Arena 2 assets.
//!
//! Renders either a `.gmf` mesh or a fully assembled `.bot` robot to a PNG.
//! Core of the M1 asset-viewer milestone and a headless regression harness.
//!
//! Usage:
//!   ra2-render <file.gmf|file.bot> <out.png> [width] [height]
//!
//! Env: RA2_NO_CHASSIS=1  skip the chassis mesh
//!      RA2_DEBUG=1       print per-part bounds

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3};
use ra2fmt::{bot as botfmt, component as compfmt, gmi};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    nrm: [f32; 3],
    uv: [f32; 2],
    /// Tint + alpha (material transparency); `[1,1,1,1]` for opaque geometry.
    color: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Uniforms {
    mvp: [[f32; 4]; 4],
    light: [f32; 4],
}

struct DrawItem {
    vertices: Vec<Vertex>,
    tex: Vec<u8>,
    tw: u32,
    th: u32,
    label: String,
    /// Draw with alpha blending (transparent material).
    blend: bool,
}

const SHADER: &str = r#"
struct Uniforms { mvp: mat4x4<f32>, light: vec4<f32>, };
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VsIn { @location(0) pos: vec3<f32>, @location(1) nrm: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) color: vec4<f32> };
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) nrm: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32> };

@vertex
fn vs_main(i: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = u.mvp * vec4<f32>(i.pos, 1.0);
    o.nrm = i.nrm;
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

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: ra2-render <file.gmf|file.bot|arena.py> <out.png> [width] [height]");
        eprintln!(
            "  env: RA2_OBSTACLE=<name|all|none>  RA2_HIDE=<name,...>  RA2_CAM=<ex,ey,ez,tx,ty,tz>"
        );
        return ExitCode::FAILURE;
    }
    let input = &args[0];
    let output = &args[1];
    let w: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1024);
    let h: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(768);

    match run(input, output, w, h) {
        Ok((items, tris)) => {
            println!("rendered {input} -> {output} ({w}x{h}, {items} parts, {tris} tris)");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(
    input: &str,
    output: &str,
    w: u32,
    h: u32,
) -> Result<(usize, usize), Box<dyn std::error::Error>> {
    let items = collect_items(input)?;
    if items.is_empty() {
        return Err("no renderable geometry found".into());
    }
    let ntris: usize = items.iter().map(|i| i.vertices.len() / 3).sum();

    if std::env::var("RA2_DEBUG").is_ok() {
        for it in &items {
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

    let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for it in &items {
        for v in &it.vertices {
            let p = Vec3::from(v.pos);
            min = min.min(p);
            max = max.max(p);
        }
    }
    let center = (min + max) * 0.5;
    let radius = ((max - min).length() * 0.5).max(0.001);

    // --- wgpu -------------------------------------------------------------
    let mut idesc = wgpu::InstanceDescriptor::new_without_display_handle();
    idesc.backends = wgpu::Backends::all();
    let instance = wgpu::Instance::new(idesc);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("ra2-render"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))?;

    let color_format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let depth_format = wgpu::TextureFormat::Depth32Float;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("mesh-shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });

    let uniforms = Uniforms {
        mvp: mvp_matrix(center, radius, w, h, camera_override(input)).to_cols_array_2d(),
        light: [0.4, -0.7, 0.6, 0.0],
    };
    let ubuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("uniforms"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
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

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });

    let attrs =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4];
    let cull = match std::env::var("RA2_CULL").as_deref() {
        Ok("back") => Some(wgpu::Face::Back),
        Ok("front") => Some(wgpu::Face::Front),
        _ => None,
    };
    let make_pipeline = |label: &str, blend: bool, depth_write: bool| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attrs,
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: blend.then_some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                // RA2/D3D8 geometry is wound clockwise when viewed from outside.
                front_face: wgpu::FrontFace::Cw,
                cull_mode: cull,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
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
    let pipeline_opaque = make_pipeline("opaque", false, true);
    let pipeline_blend = make_pipeline("blend", true, false);

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    struct Gpu {
        vbuf: wgpu::Buffer,
        bg: wgpu::BindGroup,
        count: u32,
        blend: bool,
    }
    let mut gpu_items = Vec::new();
    for it in &items {
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&it.label),
            contents: bytemuck::cast_slice(&it.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let tex = device.create_texture_with_data(
            &queue,
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
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &it.tex,
        );
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bgl,
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
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        gpu_items.push(Gpu {
            vbuf,
            bg,
            count: it.vertices.len() as u32,
            blend: it.blend,
        });
    }

    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("color"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: color_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: depth_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("main"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.06,
                        g: 0.07,
                        b: 0.09,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
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
        // Opaque geometry first, then blended (transparent) geometry.
        rp.set_pipeline(&pipeline_opaque);
        for g in gpu_items.iter().filter(|g| !g.blend) {
            rp.set_bind_group(0, &g.bg, &[]);
            rp.set_vertex_buffer(0, g.vbuf.slice(..));
            rp.draw(0..g.count, 0..1);
        }
        rp.set_pipeline(&pipeline_blend);
        for g in gpu_items.iter().filter(|g| g.blend) {
            rp.set_bind_group(0, &g.bg, &[]);
            rp.set_vertex_buffer(0, g.vbuf.slice(..));
            rp.draw(0..g.count, 0..1);
        }
    }

    let align = 256u32;
    let unpadded = w * 4;
    let padded = unpadded.div_ceil(align) * align;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (padded * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &color,
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
    queue.submit(Some(encoder.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;

    let mapped = slice.get_mapped_range()?;
    let mut rgba = Vec::with_capacity((unpadded * h) as usize);
    for row in 0..h as usize {
        let start = row * padded as usize;
        rgba.extend_from_slice(&mapped[start..start + unpadded as usize]);
    }
    drop(mapped);
    readback.unmap();

    image::save_buffer(output, &rgba, w, h, image::ColorType::Rgba8)?;
    Ok((items.len(), ntris))
}

// ---------------------------------------------------------------------------
// scene collection
// ---------------------------------------------------------------------------

fn collect_items(input: &str) -> Result<Vec<DrawItem>, Box<dyn std::error::Error>> {
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
fn resolve_ci(root: &Path, rel: &str) -> Option<PathBuf> {
    let rel = rel.replace('\\', "/");
    let mut cur = root.to_path_buf();
    for part in rel.split('/').filter(|p| !p.is_empty() && *p != ".") {
        if part == ".." {
            cur.pop();
            continue;
        }
        let direct = cur.join(part);
        if direct.exists() {
            cur = direct;
            continue;
        }
        let want = part.to_ascii_lowercase();
        let found = std::fs::read_dir(&cur)
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().to_ascii_lowercase() == want)?;
        cur = found.path();
    }
    cur.exists().then_some(cur)
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

/// Per-component loaded data used for assembly.
struct LoadedComp {
    label: String,
    scene: gmi::GmiScene,
    dir: PathBuf,
}

fn collect_bot(input: &str) -> Result<Vec<DrawItem>, Box<dyn std::error::Error>> {
    let data = std::fs::read(input)?;
    let bot = botfmt::parse(&data)?;
    let root =
        find_game_root(Path::new(input)).ok_or("could not locate game root (Components dir)")?;
    println!(
        "bot {} v{}: {} components (root={})",
        bot.name,
        bot.version,
        bot.components.len(),
        root.display()
    );

    // Component definitions live in `Components.cfz`; prefer the archive and
    // fall back to loose extracted `.txt` files when present.
    let cfl = ra2fmt::Cfl::open_path(&root.join("Components.cfz")).ok();

    // Load every attached component's mesh (+ its texture).
    let mut loaded: Vec<Option<LoadedComp>> = Vec::with_capacity(bot.components.len());
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
                loaded.push(Some(LoadedComp {
                    label: format!("{}({})", c.class, def.name),
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

    // Component world transforms, chained through parents. RA2 places a component
    // by its stored translation + quaternion, then a mount yaw (`angle`) and a
    // vertical offset (`height`) about/along the model up axis (Y).
    let mut world: Vec<Mat4> = vec![Mat4::IDENTITY; bot.components.len()];
    for (i, c) in bot.components.iter().enumerate() {
        let local = component_local(c);
        if i > 0 && c.parent >= 0 && (c.parent as usize) < i {
            let parent = c.parent as usize;
            // A BurstMotor poses its children at its start (rest) axle angle.
            let rest = match bot.components[parent].burst_angles {
                Some([start, _]) => Mat4::from_rotation_z(start),
                None => Mat4::IDENTITY,
            };
            world[i] = world[parent] * rest * local;
        } else {
            world[i] = local;
        }
    }

    let mut items = Vec::new();

    // chassis (mesh embedded in the bot file)
    if std::env::var("RA2_NO_CHASSIS").is_err() {
        if let Some(ch) = bot.components.first().and_then(|c| c.chassis.as_ref()) {
            if let Some(mesh) = &ch.mesh {
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
                let verts = bot_mesh_vertices(mesh, world[0]);
                let (tex, tw, th) = match &ch.texture_tga {
                    Some(tga) => decode_image(tga)
                        .unwrap_or_else(|| (solid([120, 120, 125, 255], 4, 4), 4, 4)),
                    None => (solid([120, 120, 125, 255], 4, 4), 4, 4),
                };
                if !verts.is_empty() {
                    items.push(DrawItem {
                        vertices: verts,
                        tex,
                        tw,
                        th,
                        label: "chassis".into(),
                        blend: false,
                    });
                }
            }
        }
    }

    // attached components
    for (i, lc) in loaded.iter().enumerate().skip(1) {
        let Some(lc) = lc else { continue };
        push_scene(&lc.scene, world[i], &lc.dir, &lc.label, &mut items);
    }
    Ok(items)
}

fn component_local(c: &ra2fmt::bot::Component) -> Mat4 {
    // RA2 model space is Y-up: `translation` + `rotation` place the component,
    // then the mount `angle` yaws it about +Y and `height` offsets it along +Y.
    // The stored quaternion is (x, y, z, w) — confirmed against PyRA2's
    // quatToAxisAndAngle (element 3 is the scalar) and a chassis whose rotation
    // is exactly [0, 0, 0, 1].
    let t = Vec3::from(c.translation);
    let q = Quat::from_xyzw(c.rotation[0], c.rotation[1], c.rotation[2], c.rotation[3]).normalize();
    Mat4::from_translation(t)
        * Mat4::from_quat(q)
        * Mat4::from_rotation_y(c.angle)
        * Mat4::from_translation(Vec3::new(0.0, c.height, 0.0))
}

fn bot_mesh_vertices(mesh: &ra2fmt::bot::BotMesh, world: Mat4) -> Vec<Vertex> {
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
        let uv = |idx: usize| mesh.uvs.get(idx).copied().unwrap_or([0.0, 0.0]);
        out.push(Vertex {
            pos: pa.into(),
            nrm: n.into(),
            uv: uv(a),
            color: [1.0, 1.0, 1.0, 1.0],
        });
        out.push(Vertex {
            pos: pb.into(),
            nrm: n.into(),
            uv: uv(b),
            color: [1.0, 1.0, 1.0, 1.0],
        });
        out.push(Vertex {
            pos: pc.into(),
            nrm: n.into(),
            uv: uv(cc),
            color: [1.0, 1.0, 1.0, 1.0],
        });
    }
    out
}

/// Build render vertices for a `.gmf` scene, grouped by resolved material so
/// each group can use the correct texture (needed for `Multi/Sub-Object` arenas
/// where each face selects a sub-material).
fn scene_material_groups(
    scene: &gmi::GmiScene,
    world: Mat4,
    dir: &Path,
) -> Vec<(Option<PathBuf>, [f32; 4], Vec<Vertex>)> {
    let mut index: HashMap<u64, usize> = HashMap::new();
    let mut groups: Vec<(Option<PathBuf>, [f32; 4], Vec<Vertex>)> = Vec::new();
    for o in &scene.objects {
        let Some(mesh) = &o.mesh else { continue };
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
        let obj_world = world * tm_mat4(&o.transform);
        let pts: Vec<Vec3> = mesh
            .points
            .iter()
            .map(|p| obj_world.transform_point3(Vec3::from(*p)))
            .collect();
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
            let key = ((mesh.material_ref as i64 as u64) << 32) | (f.material as u32 as u64);
            let gi = *index.entry(key).or_insert_with(|| {
                let (tex, color) = material_slot(scene, mesh.material_ref, f.material, dir);
                groups.push((tex, color, Vec::new()));
                groups.len() - 1
            });
            let (pa, pb, pc) = (pts[ia], pts[ib], pts[ic]);
            let n = (pb - pa).cross(pc - pa).normalize_or_zero();
            let color = groups[gi].1;
            let uv = |idx: usize| -> [f32; 2] {
                if !mesh.texfaces.is_empty()
                    && fi < mesh.texfaces.len()
                    && !mesh.texcoords.is_empty()
                {
                    let ti = mesh.texfaces[fi][idx] as usize;
                    if ti < mesh.texcoords.len() {
                        return [mesh.texcoords[ti][0], mesh.texcoords[ti][1]];
                    }
                }
                [0.0, 0.0]
            };
            let g = &mut groups[gi].2;
            g.push(Vertex {
                pos: pa.into(),
                nrm: n.into(),
                uv: uv(0),
                color,
            });
            g.push(Vertex {
                pos: pb.into(),
                nrm: n.into(),
                uv: uv(1),
                color,
            });
            g.push(Vertex {
                pos: pc.into(),
                nrm: n.into(),
                uv: uv(2),
                color,
            });
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
    for (tex, color, verts) in scene_material_groups(scene, world, dir) {
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
        });
    }
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

fn tm_mat4(tm: &[[f32; 3]; 4]) -> Mat4 {
    if tm == &[[0.0; 3]; 4] {
        return Mat4::IDENTITY;
    }
    Mat4::from_cols(
        glam::Vec4::new(tm[0][0], tm[0][1], tm[0][2], 0.0),
        glam::Vec4::new(tm[1][0], tm[1][1], tm[1][2], 0.0),
        glam::Vec4::new(tm[2][0], tm[2][1], tm[2][2], 0.0),
        glam::Vec4::new(tm[3][0], tm[3][1], tm[3][2], 1.0),
    )
}

fn find_game_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start.parent()?.to_path_buf();
    for _ in 0..6 {
        if dir.join("Components").is_dir() {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

fn decode_image(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
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
struct CamOverride {
    eye: Vec3,
    target: Option<Vec3>,
}

/// Parse `RA2_CAM`: either `ex,ey,ez,tx,ty,tz`, or `@<n>` / `@<name>` to use a
/// static camera declared in an arena script (aimed at the scene centre).
fn camera_override(input: &str) -> Option<CamOverride> {
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

fn mvp_matrix(center: Vec3, radius: f32, w: u32, h: u32, cam: Option<CamOverride>) -> Mat4 {
    let aspect = w as f32 / h as f32;
    if let Some(cam) = cam {
        let target = cam.target.unwrap_or(center);
        let dist = (cam.eye - target).length().max(0.001);
        let view = look_at_rh(cam.eye, target, up_for(cam.eye - target));
        let proj = perspective_rh(
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
    let view = look_at_rh(eye, center, Vec3::Y);
    let proj = perspective_rh(45f32.to_radians(), aspect, radius * 0.01, radius * 100.0);
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

fn look_at_rh(eye: Vec3, center: Vec3, up: Vec3) -> Mat4 {
    let f = (center - eye).normalize();
    let s = f.cross(up).normalize();
    let u = s.cross(f);
    Mat4::from_cols(
        glam::Vec4::new(s.x, u.x, -f.x, 0.0),
        glam::Vec4::new(s.y, u.y, -f.y, 0.0),
        glam::Vec4::new(s.z, u.z, -f.z, 0.0),
        glam::Vec4::new(-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0),
    )
}

fn perspective_rh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    Mat4::from_cols(
        glam::Vec4::new(f / aspect, 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, f, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, far / (near - far), -1.0),
        glam::Vec4::new(0.0, 0.0, near * far / (near - far), 0.0),
    )
}
