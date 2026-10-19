//! Diagnostic: a `.gmf`'s object transforms and attachment frames.
//!
//! Usage: cargo run --release -p ra2sim --example gmfframes -- <file.gmf>

fn row(r: &[f32; 3]) -> String {
    format!("({:7.3} {:7.3} {:7.3})", r[0], r[1], r[2])
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: gmfframes <file.gmf>");
    let scene = ra2fmt::gmi::parse(&std::fs::read(&path).expect("read")).expect("parse");
    for o in &scene.objects {
        let t = &o.transform;
        println!(
            "{:?} {:?} parent={:?} X{} Y{} Z{} O{}",
            o.kind,
            o.name,
            o.parent,
            row(&t[0]),
            row(&t[1]),
            row(&t[2]),
            row(&t[3])
        );
    }
    for h in &scene.hinges {
        println!(
            "hinge {:?} {:?}-{:?} limited={} limits={:?} point{} spin{} X{} Y{} Z{} O{}",
            h.name,
            h.body_a,
            h.body_b,
            h.limited,
            h.limits,
            row(&h.point),
            row(&h.spin_axis),
            row(&h.frame[0]),
            row(&h.frame[1]),
            row(&h.frame[2]),
            row(&h.frame[3])
        );
    }
    for (id, a) in scene.attachment_table() {
        println!(
            "attach {id} {:?} types={:?} host={:?} O{} Z{}",
            a.name,
            a.types,
            a.host,
            row(&a.transform[3]),
            row(&a.transform[2])
        );
    }
}
