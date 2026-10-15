//! `ra2` — command line probe/converter for Robot Arena 2 data files.

use std::process::ExitCode;

use ra2fmt::gmi::{self, GmiScene, ObjectKind};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["info", files @ ..] if !files.is_empty() => {
            for f in files {
                if let Err(e) = info(f) {
                    eprintln!("{f}: ERROR: {e}");
                }
            }
            ExitCode::SUCCESS
        }
        ["obj", input, output] => match obj(input, output) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        ["cfz", file, "list"] => match cfz_list(file) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        ["cfz", file, "cat", name] => match cfz_cat(file, name) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        ["arena", file] => match arena(file) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage:");
            eprintln!("  ra2 info <file.gmf> [...]");
            eprintln!("  ra2 obj  <file.gmf> <out.obj>");
            eprintln!("  ra2 cfz  <file.cfz> list");
            eprintln!("  ra2 cfz  <file.cfz> cat <entry>");
            eprintln!("  ra2 arena <script.py>");
            ExitCode::FAILURE
        }
    }
}

fn arena(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let a = ra2fmt::arena::parse_path(std::path::Path::new(path))?;
    println!("=== {path} ===");
    println!(
        "  name:  {}",
        if a.name.is_empty() {
            "(unnamed)"
        } else {
            &a.name
        }
    );
    println!("  model: {}", a.model.as_deref().unwrap_or("(none)"));
    match a.extent {
        Some(e) => println!("  extent: ({}, {}, {}, {})", e[0], e[1], e[2], e[3]),
        None => println!("  extent: (default)"),
    }
    match a.shadow_source {
        Some(s) => println!(
            "  shadow source: ({}, {}, {})  triangles: {}",
            s[0],
            s[1],
            s[2],
            a.shadow_triangles.len()
        ),
        None => println!(
            "  shadow source: (none)  triangles: {}",
            a.shadow_triangles.len()
        ),
    }
    println!("  xtras: {}", a.xtras.len());
    for x in &a.xtras {
        println!(
            "    {:?} gmf={:?} material={:?}{}",
            x.name,
            x.gmf,
            x.material,
            x.sound
                .as_deref()
                .map(|s| format!(" sound={s:?}"))
                .unwrap_or_default()
        );
    }
    println!("  static cameras: {}", a.static_cameras.len());
    for c in &a.static_cameras {
        println!(
            "    {:?} at ({}, {}, {})",
            c.name, c.position[0], c.position[1], c.position[2]
        );
    }
    Ok(())
}

fn cfz_list(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let cfl = ra2fmt::Cfl::open_path(std::path::Path::new(path))?;
    println!("{path}: {} entries", cfl.entries().len());
    for e in cfl.entries() {
        println!("  {:>9}  c{:x}  {}", e.unpacked_size, e.compression, e.name);
    }
    Ok(())
}

fn cfz_cat(path: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let cfl = ra2fmt::Cfl::open_path(std::path::Path::new(path))?;
    let data = cfl
        .get(name)
        .ok_or_else(|| format!("entry {name:?} not found"))?;
    use std::io::Write;
    std::io::stdout().write_all(&data)?;
    Ok(())
}

fn info(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let data = std::fs::read(path)?;
    println!("\n=== {path} ({} bytes) ===", data.len());
    let scene = gmi::parse(&data)?;
    println!(
        "  materials: {}  objects: {}  hinges: {}",
        scene.materials.len(),
        scene.objects.len(),
        scene.hinges.len()
    );
    for h in &scene.hinges {
        println!(
            "    hinge {:?} {} -> {} pivot ({:.3},{:.3},{:.3}) axis ({:.2},{:.2},{:.2}) limited {} friction {:.3} limits {:?}",
            h.name, h.body_a, h.body_b, h.pivot[0], h.pivot[1], h.pivot[2], h.axis[0], h.axis[1], h.axis[2],
            h.limited, h.friction, h.limits
        );
    }
    for m in &scene.materials {
        let tex: Vec<&str> = m
            .textures
            .iter()
            .map(|t| t.bitmap.as_str())
            .filter(|s| !s.is_empty())
            .collect();
        println!(
            "    mat {:?} class={:?} trans={:.2} tex=[{}]",
            m.name,
            m.class,
            m.transparency,
            tex.join(", ")
        );
        for sm in &m.sub_materials {
            let stex: Vec<&str> = sm
                .textures
                .iter()
                .map(|t| t.bitmap.as_str())
                .filter(|s| !s.is_empty())
                .collect();
            println!(
                "      sub {:?} trans={:.2} tex=[{}]",
                sm.name,
                sm.transparency,
                stex.join(", ")
            );
        }
    }
    for o in &scene.objects {
        match &o.kind {
            ObjectKind::Geometric => {
                if let Some(mesh) = &o.mesh {
                    println!(
                        "    mesh {:?} verts={} faces={} uv={} mat={}{}",
                        o.name,
                        mesh.points.len(),
                        mesh.faces.len(),
                        mesh.texcoords.len(),
                        mesh.material_ref,
                        if mesh.is_collision() {
                            " [collision]"
                        } else {
                            ""
                        }
                    );
                }
            }
            ObjectKind::Attachment => {
                let d = o.data.replace(['\r', '\n'], " ");
                let t = &o.transform;
                println!(
                    "    attach {:?} pos=({:7.4},{:7.4},{:7.4}) x=({:6.3},{:6.3},{:6.3}) y=({:6.3},{:6.3},{:6.3}) z=({:6.3},{:6.3},{:6.3}) data={:?}",
                    o.name,
                    t[3][0],
                    t[3][1],
                    t[3][2],
                    t[0][0],
                    t[0][1],
                    t[0][2],
                    t[1][0],
                    t[1][1],
                    t[1][2],
                    t[2][0],
                    t[2][1],
                    t[2][2],
                    d.trim()
                );
            }
            ObjectKind::RigidBodies => {
                println!(
                    "    rigidbodies {:?}: {} bodies",
                    o.name,
                    o.rigid_bodies.len()
                );
                for rb in &o.rigid_bodies {
                    println!(
                        "        rb {:?} mass={:.3} geo_type={}",
                        rb.name, rb.mass, rb.geo_type
                    );
                }
            }
            ObjectKind::Constraints => println!("    constraints {:?}", o.name),
            ObjectKind::Simulation => {}
            ObjectKind::Light => println!("    light {:?}", o.name),
            ObjectKind::Camera => println!("    camera {:?}", o.name),
            ObjectKind::Dashpot => println!("    dashpot {:?}", o.name),
            ObjectKind::Unknown(id) => println!("    unknown node id={id}"),
        }
    }
    Ok(())
}

fn obj(input: &str, output: &str) -> Result<(), Box<dyn std::error::Error>> {
    let data = std::fs::read(input)?;
    let scene = gmi::parse(&data)?;
    let mut out = String::new();
    out.push_str("# exported from Robot Arena 2 .gmf\n");
    let mut vbase = 0usize;
    let mut n_meshes = 0;
    for (i, o) in scene.objects.iter().enumerate() {
        let Some(mesh) = &o.mesh else { continue };
        if mesh.is_collision() {
            continue;
        }
        n_meshes += 1;
        let name = if o.name.is_empty() {
            format!("mesh{i}")
        } else {
            sanitize(&o.name)
        };
        out.push_str(&format!("o {name}\n"));
        for p in &mesh.points {
            out.push_str(&format!("v {:.6} {:.6} {:.6}\n", p[0], p[1], p[2]));
        }
        for t in &mesh.texcoords {
            out.push_str(&format!("vt {:.6} {:.6}\n", t[0], 1.0 - t[1]));
        }
        let has_uv = !mesh.texfaces.is_empty() && mesh.texfaces.len() == mesh.faces.len();
        for (idx, f) in mesh.faces.iter().enumerate() {
            let (a, b, c) = (f.v[0] as usize, f.v[1] as usize, f.v[2] as usize);
            if has_uv && !mesh.texcoords.is_empty() {
                let tf = mesh.texfaces[idx];
                out.push_str(&format!(
                    "f {}/{} {}/{} {}/{}\n",
                    vbase + a + 1,
                    tf[0] + 1,
                    vbase + b + 1,
                    tf[1] + 1,
                    vbase + c + 1,
                    tf[2] + 1
                ));
            } else {
                out.push_str(&format!(
                    "f {} {} {}\n",
                    vbase + a + 1,
                    vbase + b + 1,
                    vbase + c + 1
                ));
            }
        }
        vbase += mesh.points.len();
    }
    std::fs::write(output, out)?;
    println!("wrote {output}: {n_meshes} meshes");
    Ok(())
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// Keep the (unused-for-now) type import honest.
#[allow(dead_code)]
fn _assert_scene(_: &GmiScene) {}
