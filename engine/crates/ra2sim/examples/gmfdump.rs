//! Diagnostic: print a .gmf's objects, rigid bodies and collision proxies.
//!
//! Usage: cargo run --release -p ra2sim --example gmfdump -- <file.gmf>
fn main() {
    let path = std::env::args().nth(1).expect("usage: gmfdump <file.gmf>");
    let data = std::fs::read(&path).expect("read");
    let scene = ra2fmt::gmi::parse(&data).expect("parse");
    if let Some(s) = &scene.simulation {
        println!("simulation: {s:?}");
    }
    if std::env::var_os("GMFDUMP_MATERIALS").is_some() {
        for (i, m) in scene.materials.iter().enumerate() {
            println!(
                "material {i} {:?} class {:?} transparency {} two_sided {} xp {} subs {} textures {:?}",
                m.name,
                m.class,
                m.transparency,
                m.two_sided,
                m.xp_type,
                m.sub_materials.len(),
                m.textures.iter().map(|t| format!("{t:?}").chars().take(60).collect::<String>()).collect::<Vec<_>>()
            );
        }
    }
    for o in &scene.objects {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        let mut n = 0;
        let mut coll = false;
        if let Some(m) = &o.mesh {
            n = m.points.len();
            coll = m.is_collision();
            for p in &m.points {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        println!(
            "{:?} {:?} parent={:?} pts={} coll={} bbox={:.2?}..{:.2?}",
            o.kind, o.name, o.parent, n, coll, lo, hi
        );
        if std::env::var_os("GMFDUMP_TM").is_some() {
            println!(
                "    tm {:?} data {:?}",
                o.transform,
                o.data.chars().take(120).collect::<String>()
            );
        }
        if let Some(m) = &o.mesh {
            // The solid the triangles bound, as `massprops` takes it, in the
            // object's placed frame, against the hull of its points.
            let t = ra2_render::object_transform(o);
            let pts: Vec<glam::Vec3> = m
                .points
                .iter()
                .map(|p| t.transform_point3(glam::Vec3::from(*p)))
                .collect();
            let tris: Vec<[u32; 3]> = m.faces.iter().map(|f| f.v).collect();
            if std::env::var_os("GMFDUMP_POINTS").is_some() {
                println!(
                    "    points {:?}",
                    pts.iter().map(|p| p.to_array()).collect::<Vec<_>>()
                );
                println!("    faces {tris:?}");
                let mats: std::collections::BTreeSet<i32> =
                    m.faces.iter().map(|f| f.material).collect();
                println!(
                    "    material_ref {} face materials {mats:?} backface_cull {}",
                    m.material_ref, m.backface_cull
                );
            }
            if pts.len() >= 4 {
                println!(
                    "    placed: faces {} mesh volume/centroid {:?} hull {:?} from first corner {:?}",
                    tris.len(),
                    ra2sim::massprops::mesh_volume_centroid(&pts, &tris),
                    ra2sim::massprops::hull_volume_centroid(&pts),
                    tris.first().and_then(|t| ra2sim::massprops::triangles_mass_from(&pts, &tris, pts[t[0] as usize])).map(|m| (m.0, m.1))
                );
            }
        }
        for rb in &o.rigid_bodies {
            dump_rb(rb, 1);
        }
    }
}

fn dump_rb(rb: &ra2fmt::gmi::RigidBody, depth: usize) {
    if std::env::var_os("GMFDUMP_POINTS").is_some() {
        println!(
            "{}rb {:?} node_tm {:?}",
            "  ".repeat(depth),
            rb.name,
            rb.transform
        );
    }
    println!(
        "{}rb {:?} mass={} elast={} fric={} simgeo={} proxy={:?} dispproxy={:?}/{} geo_type={} unyield={} nocoll={}",
        "  ".repeat(depth),
        rb.name,
        rb.mass,
        rb.elasticity,
        rb.friction,
        rb.simulation_geometry,
        rb.geometry_proxy,
        rb.display_proxy,
        rb.use_display_proxy,
        rb.geo_type,
        rb.unyielding,
        rb.disable_collisions
    );
    for c in &rb.children {
        dump_rb(c, depth + 1);
    }
}
