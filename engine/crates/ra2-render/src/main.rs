//! `ra2-render` — offscreen wgpu renderer for Robot Arena 2 `.gmf` meshes.
//!
//! Renders a `.gmf` to a PNG. This is the core of the M1 asset-viewer milestone
//! and doubles as a headless regression harness.
//!
//! Usage: ra2-render <file.gmf> <out.png> [width] [height]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use ra2fmt::gmi::{self, ObjectKind};
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
    let shade = 0.30 + 0.70 * l;
    return vec4<f32>(c.rgb * shade, c.a);
}
"#;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: ra2-render <file.gmf> <out.png> [width] [height]");
        return ExitCode::FAILURE;
    }
    let input = &args[0];
    let output = &args[1];
    let w: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1024);
    let h: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(768);

    match run(input, output, w, h) {
        Ok((nverts, ntris)) => {
            println!("rendered {input} -> {output} ({w}x{h}, {ntris} tris, {nverts} verts)");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(input: &str, output: &str, w: u32, h: u32) -> Result<(usize, usize), Box<dyn std::error::Error>> {
    let data = std::fs::read(input)?;
    let scene = gmi::parse(&data)?;

    let gmf_dir = Path::new(input).parent().unwrap_or(Path::new("."));
    let (vertices, base_rgb) = build_vertices(&scene, gmf_dir);

    if vertices.is_empty() {
        return Err("no renderable geometry found".into());
    }
    let (min, max) = bounds(&vertices);
    let center = (min + max) * 0.5;
    let radius = ((max - min).length() * 0.5).max(0.001);

    let tex_rgba = load_texture(&scene, gmf_dir).unwrap_or_else(|| solid(base_rgb, 4, 4));
    let (tex_w, tex_h) = size_of(&scene, gmf_dir).unwrap_or((4, 4));

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

    let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vertices"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let mvp = mvp_matrix(center, radius, w, h);
    let uniforms = Uniforms {
        mvp: mvp.to_cols_array_2d(),
        light: [0.4, -0.7, 0.6, 0.0],
    };
    let ubuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("uniforms"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let tex = device.create_texture_with_data(
        &queue,
        &wgpu::TextureDescriptor {
            label: Some("albedo"),
            size: wgpu::Extent3d { width: tex_w, height: tex_h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &tex_rgba,
    );
    let tex_view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
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
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&tex_view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
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
            cull_mode: None, // RA2 is left-handed; winding is unreliable
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
        rp.set_bind_group(0, &bind_group, &[]);
        rp.set_vertex_buffer(0, vbuf.slice(..));
        rp.draw(0..vertices.len() as u32, 0..1);
    }

    // read back
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
    Ok((vertices.len(), vertices.len() / 3))
}

fn build_vertices(scene: &gmi::GmiScene, _dir: &Path) -> (Vec<Vertex>, [u8; 4]) {
    let mut verts: Vec<Vertex> = Vec::new();
    let mut base_rgb = [180u8, 180u8, 185u8, 255];

    for o in &scene.objects {
        if o.kind != ObjectKind::Geometric {
            continue;
        }
        let Some(mesh) = &o.mesh else { continue };
        if mesh.is_collision() {
            continue; // don't render Havok collision proxies
        }
        // material diffuse fallback
        if let Some(mat) = scene.materials.iter().find(|_| true) {
            if !scene.materials.is_empty() {
                base_rgb = [
                    (mat.diffuse[0] * 255.0) as u8,
                    (mat.diffuse[1] * 255.0) as u8,
                    (mat.diffuse[2] * 255.0) as u8,
                    255,
                ];
            }
        }

        // transform points into world space
        let pts: Vec<[f32; 3]> = mesh.points.iter().map(|p| apply_tm(&o.transform, *p)).collect();

        for (fi, f) in mesh.faces.iter().enumerate() {
            let ia = f.v[0] as usize;
            let ib = f.v[1] as usize;
            let ic = f.v[2] as usize;
            if ia >= pts.len() || ib >= pts.len() || ic >= pts.len() {
                continue;
            }
            let (pa, pb, pc) = (pts[ia], pts[ib], pts[ic]);
            let n = face_normal(pa, pb, pc);
            let uv = |idx: usize| -> [f32; 2] {
                if !mesh.texfaces.is_empty() && fi < mesh.texfaces.len() && !mesh.texcoords.is_empty() {
                    let ti = mesh.texfaces[fi][idx] as usize;
                    if ti < mesh.texcoords.len() {
                        return [mesh.texcoords[ti][0], mesh.texcoords[ti][1]];
                    }
                }
                [0.0, 0.0]
            };
            verts.push(Vertex { pos: pa, nrm: n, uv: uv(0) });
            verts.push(Vertex { pos: pb, nrm: n, uv: uv(1) });
            verts.push(Vertex { pos: pc, nrm: n, uv: uv(2) });
        }
    }
    (verts, base_rgb)
}

fn apply_tm(tm: &[[f32; 3]; 4], p: [f32; 3]) -> [f32; 3] {
    // row-vector convention: p' = p * M  (rows 0..2 = 3x3, row 3 = translation)
    if tm == &[[0.0; 3]; 4] {
        return p;
    }
    [
        p[0] * tm[0][0] + p[1] * tm[1][0] + p[2] * tm[2][0] + tm[3][0],
        p[0] * tm[0][1] + p[1] * tm[1][1] + p[2] * tm[2][1] + tm[3][1],
        p[0] * tm[0][2] + p[1] * tm[1][2] + p[2] * tm[2][2] + tm[3][2],
    ]
}

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = Vec3::new(b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    let v = Vec3::new(c[0] - a[0], c[1] - a[1], c[2] - a[2]);
    let n = u.cross(v).normalize_or_zero();
    [n.x, n.y, n.z]
}

fn bounds(verts: &[Vertex]) -> (Vec3, Vec3) {
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for v in verts {
        let p = Vec3::from(v.pos);
        min = min.min(p);
        max = max.max(p);
    }
    (min, max)
}

fn mvp_matrix(center: Vec3, radius: f32, w: u32, h: u32) -> Mat4 {
    // Z-up (RA2 convention). Look from front-right-top.
    let eye = center + Vec3::new(0.0, -2.4, 1.6).normalize() * (radius * 3.0);
    let view = look_at_rh(eye, center, Vec3::Z);
    let aspect = w as f32 / h as f32;
    let proj = perspective_rh(45f32.to_radians(), aspect, radius * 0.01, radius * 100.0);
    proj * view
}

/// Right-handed look-at with a wgpu 0..1 depth range.
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

/// Right-handed perspective projection mapping to wgpu's 0..1 depth range.
fn perspective_rh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    Mat4::from_cols(
        glam::Vec4::new(f / aspect, 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, f, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, far / (near - far), -1.0),
        glam::Vec4::new(0.0, 0.0, near * far / (near - far), 0.0),
    )
}

fn size_of(scene: &gmi::GmiScene, dir: &Path) -> Option<(u32, u32)> {
    let p = texture_path(scene, dir)?;
    let img = image::open(&p).ok()?;
    Some((img.width().max(1), img.height().max(1)))
}

fn load_texture(scene: &gmi::GmiScene, dir: &Path) -> Option<Vec<u8>> {
    let p = texture_path(scene, dir)?;
    let img = image::open(&p).ok()?.to_rgba8();
    Some(img.into_raw())
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
