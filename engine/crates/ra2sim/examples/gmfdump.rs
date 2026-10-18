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
        for rb in &o.rigid_bodies {
            dump_rb(rb, 1);
        }
    }
}

fn dump_rb(rb: &ra2fmt::gmi::RigidBody, depth: usize) {
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
