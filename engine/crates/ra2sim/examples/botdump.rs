//! Diagnostic: dump a `.bot`'s component table and each component's attachment
//! points (`ID`, mount classes, host, frame) — the data a mount placement needs.
//!
//! Usage: cargo run --release -p ra2sim --example botdump -- <bot> [from] [to]

fn main() {
    let path = std::env::args().nth(1).expect("usage: <bot> [from] [to]");
    let from: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let to: usize = std::env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);

    let (bot, _root, loaded) = ra2_render::load_bot_components(&path).expect("load bot");
    println!(
        "{} v{}: {} components",
        bot.name,
        bot.version,
        bot.components.len()
    );

    for (i, c) in bot.components.iter().enumerate() {
        if i < from || i > to {
            continue;
        }
        println!(
            "  {i:2} p={:2} onParent={} onSelf={} t=({:7.4},{:7.4},{:7.4}) q=({:6.3},{:6.3},{:6.3},{:6.3}) angle={:7.4} height={:7.4} {:12} {}",
            c.parent,
            c.parent_mount,
            c.own_mount,
            c.translation[0],
            c.translation[1],
            c.translation[2],
            c.rotation[0],
            c.rotation[1],
            c.rotation[2],
            c.rotation[3],
            c.angle,
            c.height,
            c.class,
            c.file,
        );
        if let Some(lc) = loaded.get(i).and_then(|l| l.as_ref()) {
            let table = lc.scene.attachment_table();
            if table.is_empty() && !lc.scene.attachments().next().is_some() {
                // nothing to show
            }
            for (id, a) in &table {
                let frame = &a.transform;
                println!(
                    "        #{id:<2} {:<9} types=[{}]{} body={} pos=({:7.4},{:7.4},{:7.4})",
                    a.name,
                    a.types.join(", "),
                    match &a.host {
                        Some(h) => format!(" host={h}"),
                        None => String::new(),
                    },
                    a.has_body,
                    frame[3][0],
                    frame[3][1],
                    frame[3][2],
                );
            }
        }
    }

    // Which attachment each component actually names on itself and on its parent.
    println!("\nmounts used (component -> parent):");
    for (i, c) in bot.components.iter().enumerate() {
        if c.parent < 0 {
            continue;
        }
        let p = c.parent as usize;
        println!(
            "  {i:2} self({}) -> parent {p} ({})  [{} -> {}]",
            c.own_mount,
            c.parent_mount,
            bot.components[i].class,
            bot.components
                .get(p)
                .map(|q| q.class.as_str())
                .unwrap_or("?"),
        );
    }

    // F9: the chassis' buckle model — the points the game shifts "down and in".
    // Each one owns several mesh vertices (it is a corner where faces meet), and
    // comparing the stored position with those vertices says whether the file
    // holds an already-crushed chassis.
    let Some(ch) = bot.components.first().and_then(|c| c.chassis.as_ref()) else {
        return;
    };
    let mesh = ch.mesh.as_ref();
    println!(
        "\nchassis: {} bottom + {} top plate points, height {:.3}, {} mesh points / {} indices, {} buckle points",
        ch.bottom.len(),
        ch.top.len(),
        ch.height,
        mesh.map(|m| m.points.len()).unwrap_or(0),
        mesh.map(|m| m.indices.len()).unwrap_or(0),
        ch.buckling.len(),
    );
    let Some(mesh) = mesh else { return };
    // F10: the chassis is dressed from these groups. Each lists the skin polygon
    // of one side of the hull; its coordinates are the skin coordinates of the
    // side's vertices in ascending index order.
    let uv_of = |i: u32| mesh.uvs.get(i as usize).copied().unwrap_or([0.0, 0.0]);
    let mut total_faces = 0usize;
    let mut matched = 0usize;
    let mut group_uvs = 0usize;
    let mut group_total = 0usize;
    let mut in_own_rect = 0usize;
    for (g, fg) in ch.face_groups.iter().enumerate() {
        total_faces += fg.faces.len();
        let mut seen: Vec<u32> = Vec::new();
        for f in &fg.faces {
            for &i in f {
                if !seen.contains(&i) {
                    seen.push(i);
                }
            }
        }
        seen.sort_unstable();
        let mesh_tris: std::collections::HashSet<[u32; 3]> = mesh
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let mut k = [t[0], t[1], t[2]];
                k.sort_unstable();
                k
            })
            .collect();
        matched += fg
            .faces
            .iter()
            .filter(|f| {
                let mut k = **f;
                k.sort_unstable();
                mesh_tris.contains(&k)
            })
            .count();
        let (x0, y0, x1, y1) = (
            fg.bounds[0] as f32,
            fg.bounds[1] as f32,
            fg.bounds[2] as f32,
            fg.bounds[3] as f32,
        );
        if fg.coords.iter().all(|c| {
            c[0] >= x0.min(x1) - 2.0
                && c[0] <= x0.max(x1) + 2.0
                && c[1] >= y0.min(y1) - 2.0
                && c[1] <= y0.max(y1) + 2.0
        }) {
            in_own_rect += 1;
        }
        for (k, &i) in seen.iter().enumerate() {
            let Some(c) = fg.coords.get(k) else { continue };
            let uv = uv_of(i);
            group_total += 1;
            if (c[0] / 256.0 - uv[0]).abs() < 1e-3 && (c[1] / 256.0 - uv[1]).abs() < 1e-3 {
                group_uvs += 1;
            }
        }
        if g < 3 {
            println!(
                "  group {g}: {} faces, {} coords, bounds {:?}",
                fg.faces.len(),
                fg.coords.len(),
                fg.bounds
            );
        }
    }
    println!(
        "  face groups: {} groups, {total_faces} faces ({matched} match a mesh triangle), {in_own_rect} wholly inside their own rectangle; coords/256 == model uv for {group_uvs}/{group_total} vertices",
        ch.face_groups.len()
    );
    println!(
        "  top corners ({}): {:?}",
        ch.top_corners.len(),
        &ch.top_corners[..ch.top_corners.len().min(6)]
    );
    // Do the buckle positions name vertices the file's mesh already has? (Their
    // index lists point into the model the *game* builds for the crush, which is
    // not the one in the file, so the coordinates are what we can trust.)
    let (mut exact, mut near, mut far) = (0usize, 0usize, 0usize);
    let mut worst = 0.0f32;
    for b in &ch.buckling {
        let d = mesh
            .points
            .iter()
            .map(|p| {
                let d = [
                    b.position[0] - p[0],
                    b.position[1] - p[1],
                    b.position[2] - p[2],
                ];
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
            })
            .fold(f32::MAX, f32::min);
        if d < 1e-3 {
            exact += 1;
        } else if d < 0.05 {
            near += 1;
        } else {
            far += 1;
            worst = worst.max(d);
        }
    }
    println!(
        "  buckle positions vs mesh points: {exact} exact, {near} within 5 cm, {far} further (worst {worst:.3})"
    );
    // The plate outlines are the chassis' uncrushed footprint. If a bot was
    // saved after a fight its mesh should sit *inside* them.
    let ext = |pts: &[[i32; 2]]| {
        pts.iter().fold(
            (i32::MAX, i32::MAX, i32::MIN, i32::MIN),
            |(x0, z0, x1, z1), p| (x0.min(p[0]), z0.min(p[1]), x1.max(p[0]), z1.max(p[1])),
        )
    };
    let (bx0, bz0, bx1, bz1) = ext(&ch.bottom);
    let (tx0, tz0, tx1, tz1) = ext(&ch.top);
    let meshx = mesh.points.iter().fold(
        (f32::MAX, f32::MAX, f32::MIN, f32::MIN),
        |(x0, z0, x1, z1), p| (x0.min(p[0]), z0.min(p[2]), x1.max(p[0]), z1.max(p[2])),
    );
    println!(
        "  footprints: bottom [{bx0},{bz0}]..[{bx1},{bz1}], top [{tx0},{tz0}]..[{tx1},{tz1}], mesh x/z {:?}..{:?}",
        (meshx.0, meshx.1),
        (meshx.2, meshx.3)
    );
    let (mut n, mut sum, mut worst) = (0usize, 0.0f32, 0.0f32);
    let mut dir = [0.0f32; 3];
    let mut owned = 0usize;
    for b in &ch.buckling {
        for &i in &b.indices {
            let Some(p) = mesh.points.get(i as usize) else {
                continue;
            };
            let d = [
                b.position[0] - p[0],
                b.position[1] - p[1],
                b.position[2] - p[2],
            ];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            worst = worst.max(len);
            sum += len;
            n += 1;
            for (a, v) in dir.iter_mut().zip(d) {
                *a += v;
            }
        }
        owned += b.indices.len();
    }
    let f = |v: f32| if n > 0 { v / n as f32 } else { 0.0 };
    println!(
        "  buckle points own {owned} mesh vertices ({n} in range); stored vs mesh: mean {:.4}, worst {:.4}, mean offset ({:+.4}, {:+.4}, {:+.4})",
        f(sum),
        worst,
        f(dir[0]),
        f(dir[1]),
        f(dir[2]),
    );
    for (i, b) in ch.buckling.iter().take(6).enumerate() {
        println!(
            "    buckle {i}: at ({:+.3}, {:+.3}, {:+.3}) owns {:?}",
            b.position[0], b.position[1], b.position[2], b.indices
        );
    }
}
