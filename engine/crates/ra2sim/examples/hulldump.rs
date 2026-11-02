//! Diagnostic: the collision hull of one component of a bot, in the bot's frame (docs/29 12as).
//!
//! Usage: `hulldump <bot> <component index>`: the convex hull's vertices, one per line.
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let rig = ra2sim::rig::load(&a[0]).expect("load");
    let i: usize = a[1].parse().expect("component index");
    let c = &rig.components[i];
    let m = rig.part_local[i];
    println!("{} ({} hull points)", c.name, c.hull_points.len());
    let pts: Vec<glam::Vec3> = c
        .hull_points
        .iter()
        .map(|p| rig.components[i].local.transform_point3(*p))
        .collect();
    let _ = m;
    for v in ra2sim::massprops::hull_vertices(&pts) {
        println!("{:.3} {:.3} {:.3}", v.x, v.y, v.z);
    }
}
