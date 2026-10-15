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
        ["gib", file] => match gib(file) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        ["font", cfz, name, rest @ ..] => match font(cfz, name, rest) {
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
            eprintln!("  ra2 gib  <layout.gib>");
            eprintln!("  ra2 font <RA2.cfz> <fg_name> [--page out.pgm]");
            ExitCode::FAILURE
        }
    }
}

/// Print a font's metrics, one glyph a line, and optionally its page as a PGM.
fn font(cfz: &str, name: &str, rest: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    use std::fmt::Write;
    let cfl = ra2fmt::cfl::Cfl::open_path(std::path::Path::new(cfz))?;
    let entry = format!("{name}.giff");
    let data = cfl
        .get(&entry)
        .ok_or_else(|| format!("{cfz} has no {entry}"))?;
    let font = ra2fmt::giff::parse(&data)?;

    let mut out = String::new();
    let _ = writeln!(out, "font {name}");
    let _ = writeln!(out, "ascent {}", font.ascent);
    let _ = writeln!(out, "descent {}", font.descent);
    let _ = writeln!(out, "leading {}", font.leading);
    let _ = writeln!(out, "line {}", font.line_height());
    let _ = writeln!(out, "page {} {}", font.page_width, font.page_height);
    for (i, g) in font.glyphs.iter().enumerate() {
        let c = char::from_u32(32 + i as u32).unwrap_or('?');
        let _ = writeln!(
            out,
            "glyph {} {} {} {} {} {} {} {}",
            c as u32, g.advance, g.bearing, g.third, g.u0, g.v0, g.u1, g.v1
        );
    }
    print!("{out}");

    if let Some(at) = rest.iter().position(|a| *a == "--page") {
        let path = rest.get(at + 1).ok_or("--page wants a filename")?;
        let (w, h) = (font.page_width as usize, font.page_height as usize);
        let mut pgm = format!("P5\n{w} {h}\n255\n").into_bytes();
        pgm.extend_from_slice(&font.page);
        std::fs::write(path, &pgm)?;
        eprintln!("wrote {path} ({w}x{h} coverage)");
    }
    Ok(())
}

/// Print a `.gib` screen's widget tree, one widget a line, indented.
fn gib(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let gib = ra2fmt::gib::parse(&text)?;
    println!(
        "T {} — {} top-level block(s)",
        gib.version,
        gib.blocks.len()
    );
    fn show(w: &ra2fmt::gib::Widget, depth: usize) {
        use std::fmt::Write;
        let mut line = String::new();
        for _ in 0..depth {
            line.push_str("  ");
        }
        let _ = write!(line, "{}", w.kind.name());
        if !w.name().is_empty() {
            let _ = write!(line, " {:?}", w.name());
        }
        if let Some(image) = w.filename() {
            let _ = write!(line, " [{image}]");
        }
        if let Some(text) = w.text() {
            if !text.is_empty() {
                let _ = write!(line, " \"{text}\"");
            }
        }
        if let Some(uv) = w.uv() {
            let _ = write!(line, " uv={uv:?}");
        }
        if let Some(rect) = w.size() {
            let _ = write!(
                line,
                " size=[{} {}]",
                show_len(rect.first),
                show_len(rect.second)
            );
        }
        if !w.visible() {
            line.push_str(" hidden");
        }
        if w.id() >= 0 {
            let _ = write!(line, " id={}", w.id());
        }
        for a in &w.attachments {
            let _ = write!(line, " +{}", a.kind.name());
        }
        if !w.notes().is_empty() {
            let _ = write!(line, "  // {}", w.notes());
        }
        println!("{line}");
        for c in &w.children {
            show(c, depth + 1);
        }
    }
    fn show_len(l: ra2fmt::gib::Length) -> String {
        if l.fraction {
            format!("{:.3}·", l.amount)
        } else {
            format!("{}px", l.amount)
        }
    }
    for block in &gib.blocks {
        match block {
            ra2fmt::gib::Block::Widget(w) => show(w, 0),
            ra2fmt::gib::Block::Attachment(a) => {
                println!("+{} (fragment)", a.kind.name());
            }
        }
    }
    Ok(())
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
