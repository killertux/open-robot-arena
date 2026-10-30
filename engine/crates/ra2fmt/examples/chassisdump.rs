//! Diagnostic: the chassis mesh of a `.bot` (points, triangles, buckle model).
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let quiet = args.first().is_some_and(|a| a == "-q");
    for p in args.iter().filter(|a| *a != "-q") {
        let b = ra2fmt::bot::parse(&std::fs::read(p).expect("read")).expect("parse");
        let Some(ch) = b.components.first().and_then(|c| c.chassis.as_ref()) else {
            continue;
        };
        let m = ch.mesh.as_ref().expect("mesh");
        println!(
            "{} v{} points {} tris {} buckle_model {:?} buckling {} corners {}",
            p,
            b.version,
            m.points.len(),
            m.indices.len() / 3,
            b.buckle_model
                .as_ref()
                .map(|x| (x.points.len(), x.indices.len() / 3)),
            ch.buckling.len(),
            ch.top_corners.len()
        );
        if quiet {
            continue;
        }
        for (i, b) in ch.buckling.iter().enumerate() {
            println!(
                "B {i} {:.4} {:.4} {:.4} idx {:?}",
                b.position[0], b.position[1], b.position[2], b.indices
            );
        }
        println!("C {:?}", ch.top_corners);
        for (i, g) in ch.face_groups.iter().enumerate() {
            println!("G {i} {:?}", g);
        }
        for f in &ch.faces {
            println!("F {:?}", f);
        }
        for (i, q) in m.points.iter().enumerate() {
            println!("P {i} {:.4} {:.4} {:.4}", q[0], q[1], q[2]);
        }
        for t in m.indices.chunks(3) {
            println!("T {} {} {}", t[0], t[1], t[2]);
        }
    }
}
