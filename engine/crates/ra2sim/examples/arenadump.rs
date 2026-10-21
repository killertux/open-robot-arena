//! Find holes in an arena's collider.
//!
//! Prints the fixed bodies an arena declares (with their triangle counts and
//! bounds) and an ASCII height map of the highest downward hit on a grid, once
//! for the set the sandbox actually uses and once for the visible geometry.
//!
//!     arenadump <arena.py|arena.gmf> [grid]
//!
//! A column where the collider map is empty but the visible map is not is a
//! place a robot can fall through the world.

use glam::Vec3;
use ra2sim::{load_arena, ArenaScene};

fn ray_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let t = origin - a;
    let u = t.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let dist = e2.dot(q) * inv;
    (dist > 1e-4).then_some(dist)
}

/// Every hit below `(x, z)`, as `(count, lowest, highest)`.
fn hits_under(triangles: &[[Vec3; 3]], x: f32, z: f32, top: f32) -> (usize, f32, f32) {
    let origin = Vec3::new(x, top + 5.0, z);
    let mut n = 0;
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    for t in triangles {
        if let Some(d) = ray_triangle(origin, Vec3::NEG_Y, t[0], t[1], t[2]) {
            let y = origin.y - d;
            n += 1;
            lo = lo.min(y);
            hi = hi.max(y);
        }
    }
    (n, lo, hi)
}

/// Height of the highest triangle below `(x, z)`, or `None`.
fn floor_at(triangles: &[[Vec3; 3]], x: f32, z: f32, top: f32) -> Option<f32> {
    let (n, _, hi) = hits_under(triangles, x, z, top);
    (n > 0).then_some(hi)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| {
        eprintln!("usage: arenadump <arena.py|arena.gmf> [grid]");
        std::process::exit(2);
    });
    let grid: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(48);

    let arena: ArenaScene = load_arena(&path)?;
    let visible: Vec<[Vec3; 3]> = {
        let items = ra2_render::collect_items(&path)?;
        let mut t = Vec::new();
        for it in &items {
            for tri in it.vertices.as_chunks::<3>().0 {
                t.push([
                    Vec3::from(tri[0].pos),
                    Vec3::from(tri[1].pos),
                    Vec3::from(tri[2].pos),
                ]);
            }
        }
        t
    };

    // Same geometry, each triangle tagged with the object it came from, so a
    // probe can say *which* object provides a surface.
    let named: Vec<(String, [Vec3; 3])> = {
        let mut t = Vec::new();
        if let Some(model) = ra2_render::arena_model_path(&path)? {
            let scene = ra2fmt::gmi::parse(&std::fs::read(&model)?)?;
            for o in &scene.objects {
                let Some(mesh) = &o.mesh else { continue };
                let place = ra2_render::object_transform(o);
                for f in &mesh.faces {
                    let (a, b, c) = (f.v[0] as usize, f.v[1] as usize, f.v[2] as usize);
                    if a < mesh.points.len() && b < mesh.points.len() && c < mesh.points.len() {
                        t.push((
                            o.name.clone(),
                            [
                                place.transform_point3(Vec3::from(mesh.points[a])),
                                place.transform_point3(Vec3::from(mesh.points[b])),
                                place.transform_point3(Vec3::from(mesh.points[c])),
                            ],
                        ));
                    }
                }
            }
        }
        t
    };

    if let Some(h) = ra2_render::collect_havok(&path)? {
        println!("fixed bodies ({}):", h.bodies.len());
        for b in &h.bodies {
            let w = b.transform;
            let mut lo = Vec3::splat(f32::MAX);
            let mut hi = Vec3::splat(f32::MIN);
            for p in &b.points {
                let p = w.transform_point3(*p);
                lo = lo.min(p);
                hi = hi.max(p);
            }
            println!(
                "  {:<22} mass={:7.1} geom={} collides={} static={} tris={:4} \
                 x=[{:7.3},{:7.3}] y=[{:7.3},{:7.3}] z=[{:7.3},{:7.3}] \
                 proxy={:?} display={:?} use_display={} geo_type={}",
                b.name,
                b.mass,
                b.simulation_geometry,
                b.collides(),
                b.is_static(),
                b.points.len() / 3,
                lo.x,
                hi.x,
                lo.y,
                hi.y,
                lo.z,
                hi.z,
                if b.collision_object.is_empty() {
                    "-"
                } else {
                    &b.collision_object
                },
                if b.display_object.is_empty() {
                    "-"
                } else {
                    &b.display_object
                },
                b.use_display_proxy,
                b.geo_type,
            );
        }
    }

    let (min, max) = ra2_render::bounds(&ra2_render::collect_items(&path)?);
    let span = max - min;
    let cell = (span.x.max(span.z) / grid as f32).max(0.01);
    let col = ((span.x / cell).ceil() as usize).max(1);
    let row = ((span.z / cell).ceil() as usize).max(1);

    let map = |tris: &[[Vec3; 3]]| -> Vec<Vec<Option<f32>>> {
        (0..row)
            .map(|r| {
                (0..col)
                    .map(|c| {
                        let x = min.x + (c as f32 + 0.5) * cell;
                        let z = min.z + (r as f32 + 0.5) * cell;
                        floor_at(tris, x, z, min.y)
                    })
                    .collect()
            })
            .collect()
    };

    let used = map(&arena.triangles);
    let vis = map(&visible);

    // A radial profile settles whether a missing column is a real gap or just a
    // grid cell whose centre fell between triangles.
    if std::env::var("RA2_PROFILE").is_ok() {
        let d = std::f32::consts::FRAC_1_SQRT_2;
        let starts: [[f32; 2]; 4] = [[1.0, 0.0], [0.0, 1.0], [d, d], [-d, d]];
        for s in starts {
            println!("\nprofile ({:.2},{:.2}):", s[0], s[1]);
            let mut r = 0.0f32;
            while r < 18.0 {
                let x = arena.center.x + s[0] * r;
                let z = arena.center.z + s[1] * r;
                let (an, alo, ahi) = hits_under(&arena.triangles, x, z, min.y);
                let (bn, blo, bhi) = hits_under(&visible, x, z, min.y);
                let f = |n: usize, lo: f32, hi: f32| {
                    if n == 0 {
                        "        --".to_string()
                    } else {
                        format!("{n:2}:{lo:6.2}/{hi:6.2}")
                    }
                };
                println!(
                    "  r={r:5.2}  collider={}  visible={}",
                    f(an, alo, ahi),
                    f(bn, blo, bhi)
                );
                if std::env::var("RA2_PROFILE_NAMES").is_ok() {
                    let origin = Vec3::new(x, min.y + 5.0, z);
                    let mut seen: Vec<(String, f32)> = Vec::new();
                    for (label, t) in &named {
                        if let Some(d) = ray_triangle(origin, Vec3::NEG_Y, t[0], t[1], t[2]) {
                            seen.push((label.clone(), origin.y - d));
                        }
                    }
                    seen.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    seen.dedup_by(|a, b| a.0 == b.0);
                    let list: Vec<String> = seen
                        .iter()
                        .take(3)
                        .map(|(n, y)| format!("{n}@{y:.2}"))
                        .collect();
                    println!("          from: {}", list.join(", "));
                }
                r += 0.5;
            }
        }
    }

    let floor_min = used
        .iter()
        .chain(vis.iter())
        .flatten()
        .filter_map(|v| *v)
        .fold(f32::MAX, f32::min);
    for (label, m) in [("collider (used)", &used), ("visible geometry", &vis)] {
        println!("\n{label}: {col}x{row} cells of {cell:.2} m, floor y relative to {floor_min:.2}");
        for r in m.iter().rev() {
            let mut line = String::new();
            for v in r {
                match v {
                    None => line.push_str("  --"),
                    Some(y) => line.push_str(&format!("{:4.0}", y - floor_min)),
                }
            }
            println!("  {line}");
        }
    }

    let holes: Vec<(usize, usize)> = (0..row)
        .flat_map(|r| (0..col).map(move |c| (r, c)))
        .filter(|&(r, c)| used[r][c].is_none() && vis[r][c].is_some())
        .collect();
    println!(
        "\ncolumns where the visible geometry has a floor but the collider does not: {}",
        holes.len()
    );
    for &(r, c) in holes.iter().take(12) {
        let x = min.x + (c as f32 + 0.5) * cell;
        let z = min.z + (r as f32 + 0.5) * cell;
        println!("    ({x:7.2}, {z:7.2})  visible y={:?}", vis[r][c]);
    }
    Ok(())
}
