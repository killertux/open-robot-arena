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
}

const SHADER: &str = r#"
struct Uniforms { mvp: mat4x4<f32>, light: vec4<f32>, };
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VsIn { @location(0) pos: vec3<f32>, @location(1) nrm: vec3<f32>, @location(2) uv: vec2<f32> };
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) nrm: vec3<f32>, @location(1) uv: vec2<f32> };

@vertex
fn vs_main(i: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = u.mvp * vec4<f32>(i.pos, 1.0);
    o.nrm = i.nrm;
    o.uv = i.uv;
    return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv);
    let n = normalize(i.nrm);
    let l = max(dot(n, normalize(u.light.xyz)), 0.0);
    let shade = 0.35 + 0.65 * l;
    return vec4<f32>(c.rgb * shade, c.a);
}
"#;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: ra2-render <file.gmf|file.bot> <out.png> [width] [height]");
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

fn run(input: &str, output: &str, w: u32, h: u32) -> Result<(usize, usize), Box<dyn std::error::Error>> {
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
                it.label, it.vertices.len(), a.x, a.y, a.z, b.x, b.y, b.z, it.tw, it.th
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
        mvp: mvp_matrix(center, radius, w, h).to_cols_array_2d(),
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

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("mesh-pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
            })],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(color_format.into())],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: depth_format,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

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
                size: wgpu::Extent3d { width: it.tw, height: it.th, depth_or_array_layers: 1 },
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
                wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        gpu_items.push(Gpu { vbuf, bg, count: it.vertices.len() as u32 });
    }

    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("color"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
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
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: depth_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("main"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.06, g: 0.07, b: 0.09, a: 1.0 }),
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
        rp.set_pipeline(&pipeline);
        for g in &gpu_items {
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
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
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
    if input.to_ascii_lowercase().ends_with(".bot") {
        collect_bot(input)
    } else {
        let mut items = Vec::new();
        add_gmi(Path::new(input), Mat4::IDENTITY, &mut items)?;
        Ok(items)
    }
}

/// Per-component loaded data used for assembly.
struct LoadedComp {
    label: String,
    scene: gmi::GmiScene,
    dir: PathBuf,
    attachments: HashMap<i32, Mat4>,
}

fn collect_bot(input: &str) -> Result<Vec<DrawItem>, Box<dyn std::error::Error>> {
    let data = std::fs::read(input)?;
    let bot = botfmt::parse(&data)?;
    let root = find_game_root(Path::new(input)).ok_or("could not locate game root (Components dir)")?;
    println!("bot {} v{}: {} components (root={})", bot.name, bot.version, bot.components.len(), root.display());

    // load component scenes + attachment maps
    let mut loaded: Vec<Option<LoadedComp>> = Vec::with_capacity(bot.components.len());
    for (i, c) in bot.components.iter().enumerate() {
        if i == 0 || c.class == "SmartZone" || c.file.is_empty() {
            loaded.push(None);
            continue;
        }
        let txt = root.join(c.file.replace('\\', "/"));
        let Ok(text) = std::fs::read_to_string(&txt) else {
            eprintln!("  skip {:?}: no definition {}", c.class, txt.display());
            loaded.push(None);
            continue;
        };
        let def = compfmt::parse(&text);
        let mesh_path = root.join("Components").join(def.mesh_rel_path());
        match std::fs::read(&mesh_path).ok().map(|b| gmi::parse(&b)) {
            Some(Ok(scene)) => {
                let dir = mesh_path.parent().unwrap_or(Path::new(".")).to_path_buf();
                let attachments = attach_map(&scene);
                loaded.push(Some(LoadedComp {
                    label: format!("{}({})", c.class, def.name),
                    scene,
                    dir,
                    attachments,
                }));
            }
            other => {
                eprintln!("  skip {:?}: {} ({:?})", c.class, mesh_path.display(), other.map(|r| r.err()));
                loaded.push(None);
            }
        }
    }

    // world transform per component:
    //   world[i] = world[parent] * A_parent(mount_on_component) * inv(A_i(mount_on_parent)) * local(i)
    let mut world: Vec<Mat4> = vec![Mat4::IDENTITY; bot.components.len()];
    for (i, c) in bot.components.iter().enumerate() {
        let local = component_local(c);
        let parent = c.parent.max(0) as usize;
        let parent_mount = if i == 0 {
            Mat4::IDENTITY
        } else if parent == 0 {
            Mat4::IDENTITY // chassis mounts are implicit
        } else {
            loaded.get(parent).and_then(|o| o.as_ref())
                .map(|lc| attach_of(&lc.attachments, c.mount_on_component))
                .unwrap_or(Mat4::IDENTITY)
        };
        let child_mount = loaded.get(i).and_then(|o| o.as_ref())
            .map(|lc| attach_of(&lc.attachments, c.mount_on_parent))
            .unwrap_or(Mat4::IDENTITY);
        let base = if i == 0 || c.parent < 0 { Mat4::IDENTITY } else { world[parent] };
        world[i] = base * parent_mount * child_mount.inverse() * local;
    }

    let mut items = Vec::new();

    // chassis (mesh embedded in the bot file)
    if std::env::var("RA2_NO_CHASSIS").is_err() {
        if let Some(ch) = bot.components.first().and_then(|c| c.chassis.as_ref()) {
            if let Some(mesh) = &ch.mesh {
                let xform = world[0] * chassis_transform(ch, mesh);
                let verts = bot_mesh_vertices(mesh, xform);
                let (tex, tw, th) = match &ch.texture_tga {
                    Some(tga) => decode_image(tga).unwrap_or_else(|| (solid([120, 120, 125, 255], 4, 4), 4, 4)),
                    None => (solid([120, 120, 125, 255], 4, 4), 4, 4),
                };
                if !verts.is_empty() {
                    items.push(DrawItem { vertices: verts, tex, tw, th, label: "chassis".into() });
                }
            }
        }
    }

    // attached components
    for (i, lc) in loaded.iter().enumerate().skip(1) {
        let Some(lc) = lc else { continue };
        let (tex, tw, th) = load_texture(&lc.scene, &lc.dir)
            .unwrap_or_else(|| (solid([170, 170, 175, 255], 4, 4), 4, 4));
        let verts = scene_vertices(&lc.scene, world[i]);
        if !verts.is_empty() {
            items.push(DrawItem { vertices: verts, tex, tw, th, label: lc.label.clone() });
        }
    }
    Ok(items)
}

fn attach_map(scene: &gmi::GmiScene) -> HashMap<i32, Mat4> {
    let mut m = HashMap::new();
    for o in &scene.objects {
        if o.kind != gmi::ObjectKind::Attachment {
            continue;
        }
        if let Some(id) = parse_attach_id(&o.data) {
            m.insert(id, tm_mat4(&o.transform));
        }
    }
    m
}

fn attach_of(m: &HashMap<i32, Mat4>, id: i32) -> Mat4 {
    m.get(&id).copied().unwrap_or(Mat4::IDENTITY)
}

fn parse_attach_id(data: &str) -> Option<i32> {
    let lower = data.to_ascii_lowercase();
    let idx = lower.find("id")?;
    let rest = &data[idx + 2..];
    let eq = rest.find('=')?;
    let num: String = rest[eq + 1..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    num.parse().ok()
}

fn component_local(c: &ra2fmt::bot::Component) -> Mat4 {
    let t = Vec3::from(c.translation);
    let q = Quat::from_xyzw(c.rotation[1], c.rotation[2], c.rotation[3], c.rotation[0]).normalize();
    Mat4::from_translation(t)
        * Mat4::from_quat(q)
        * Mat4::from_rotation_z(c.angle)
        * Mat4::from_translation(Vec3::new(0.0, 0.0, c.height))
}

/// Map the chassis mesh (normalized units) onto the chassis footprint given by
/// the outline points (0..1000 = 0..1 world units): centered in X/Y, bottom at Z=0.
fn chassis_transform(ch: &ra2fmt::bot::Chassis, mesh: &ra2fmt::bot::BotMesh) -> Mat4 {
    let mut omin = [f32::MAX; 2];
    let mut omax = [f32::MIN; 2];
    for p in ch.bottom.iter().chain(ch.top.iter()) {
        for k in 0..2 {
            let v = p[k] as f32 / 1000.0;
            omin[k] = omin[k].min(v);
            omax[k] = omax[k].max(v);
        }
    }
    let osx = (omax[0] - omin[0]).max(1e-4);
    let osy = (omax[1] - omin[1]).max(1e-4);

    let mut mmin = [f32::MAX; 3];
    let mut mmax = [f32::MIN; 3];
    for p in &mesh.points {
        for k in 0..3 {
            mmin[k] = mmin[k].min(p[k]);
            mmax[k] = mmax[k].max(p[k]);
        }
    }
    let mcx = (mmin[0] + mmax[0]) * 0.5;
    let mcy = (mmin[1] + mmax[1]) * 0.5;
    let sx = osx / (mmax[0] - mmin[0]).max(1e-4);
    let sy = osy / (mmax[1] - mmin[1]).max(1e-4);
    let sz = ch.height.max(1e-4) / (mmax[2] - mmin[2]).max(1e-4);
    Mat4::from_cols(
        glam::Vec4::new(sx, 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, sy, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, sz, 0.0),
        glam::Vec4::new(-mcx * sx, -mcy * sy, -mmin[2] * sz, 1.0),
    )
}

fn bot_mesh_vertices(mesh: &ra2fmt::bot::BotMesh, world: Mat4) -> Vec<Vertex> {
    let mut out = Vec::new();
    let pts: Vec<Vec3> = mesh.points.iter().map(|p| world.transform_point3(Vec3::from(*p))).collect();
    for tri in mesh.indices.chunks_exact(3) {
        let (a, b, cc) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        if a >= pts.len() || b >= pts.len() || cc >= pts.len() {
            continue;
        }
        let (pa, pb, pc) = (pts[a], pts[b], pts[cc]);
        let n = (pb - pa).cross(pc - pa).normalize_or_zero();
        let uv = |idx: usize| mesh.uvs.get(idx).copied().unwrap_or([0.0, 0.0]);
        out.push(Vertex { pos: pa.into(), nrm: n.into(), uv: uv(a) });
        out.push(Vertex { pos: pb.into(), nrm: n.into(), uv: uv(b) });
        out.push(Vertex { pos: pc.into(), nrm: n.into(), uv: uv(cc) });
    }
    out
}

fn scene_vertices(scene: &gmi::GmiScene, world: Mat4) -> Vec<Vertex> {
    let mut verts = Vec::new();
    for o in &scene.objects {
        let Some(mesh) = &o.mesh else { continue };
        if mesh.is_collision() {
            continue;
        }
        let obj_world = world * tm_mat4(&o.transform);
        let pts: Vec<Vec3> = mesh.points.iter().map(|p| obj_world.transform_point3(Vec3::from(*p))).collect();
        for (fi, f) in mesh.faces.iter().enumerate() {
            let (ia, ib, ic) = (f.v[0] as usize, f.v[1] as usize, f.v[2] as usize);
            if ia >= pts.len() || ib >= pts.len() || ic >= pts.len() {
                continue;
            }
            let (pa, pb, pc) = (pts[ia], pts[ib], pts[ic]);
            let n = (pb - pa).cross(pc - pa).normalize_or_zero();
            let uv = |idx: usize| -> [f32; 2] {
                if !mesh.texfaces.is_empty() && fi < mesh.texfaces.len() && !mesh.texcoords.is_empty() {
                    let ti = mesh.texfaces[fi][idx] as usize;
                    if ti < mesh.texcoords.len() {
                        return [mesh.texcoords[ti][0], mesh.texcoords[ti][1]];
                    }
                }
                [0.0, 0.0]
            };
            verts.push(Vertex { pos: pa.into(), nrm: n.into(), uv: uv(0) });
            verts.push(Vertex { pos: pb.into(), nrm: n.into(), uv: uv(1) });
            verts.push(Vertex { pos: pc.into(), nrm: n.into(), uv: uv(2) });
        }
    }
    verts
}

fn add_gmi(path: &Path, world: Mat4, items: &mut Vec<DrawItem>) -> Result<(), Box<dyn std::error::Error>> {
    let data = std::fs::read(path)?;
    let scene = gmi::parse(&data)?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let (tex, tw, th) = load_texture(&scene, dir).unwrap_or_else(|| (solid([180, 180, 185, 255], 4, 4), 4, 4));
    let verts = scene_vertices(&scene, world);
    if !verts.is_empty() {
        items.push(DrawItem { vertices: verts, tex, tw, th, label: path.display().to_string() });
    }
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
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = (img.width(), img.height());
    Some((img.into_raw(), w, h))
}

fn load_texture(scene: &gmi::GmiScene, dir: &Path) -> Option<(Vec<u8>, u32, u32)> {
    let p = texture_path(scene, dir)?;
    decode_image(&std::fs::read(p).ok()?)
}

fn texture_path(scene: &gmi::GmiScene, dir: &Path) -> Option<PathBuf> {
    for m in &scene.materials {
        for t in &m.textures {
            if t.bitmap.is_empty() {
                continue;
            }
            for cand in [
                dir.join(&t.bitmap),
                dir.join("maps").join(&t.bitmap),
                dir.join("maps").join(t.bitmap.to_lowercase()),
            ] {
                if cand.exists() {
                    return Some(cand);
                }
            }
        }
    }
    None
}

fn solid(rgb: [u8; 4], w: u32, h: u32) -> Vec<u8> {
    rgb.iter().cycle().take((w * h * 4) as usize).copied().collect()
}

// ---------------------------------------------------------------------------
// camera
// ---------------------------------------------------------------------------

fn mvp_matrix(center: Vec3, radius: f32, w: u32, h: u32) -> Mat4 {
    let eye = center + Vec3::new(0.0, -2.4, 1.6).normalize() * (radius * 3.0);
    let view = look_at_rh(eye, center, Vec3::Z);
    let aspect = w as f32 / h as f32;
    let proj = perspective_rh(45f32.to_radians(), aspect, radius * 0.01, radius * 100.0);
    proj * view
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
