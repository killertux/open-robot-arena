//! Diagnostic: where a robot's Flamethrower points in chassis space.
//! Usage: flamedir <bot> [component]
use glam::Vec3;
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: flamedir <bot> [component]");
    let rig = ra2sim::rig::load(&path).expect("load");
    let i: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(rig.components.len() - 1);
    let m = rig.components[i].local;
    let o = m.transform_point3(Vec3::ZERO);
    let d = m.transform_vector3(Vec3::new(0.0, 0.0, -1.0));
    println!(
        "{} origin {:?} dir(0,0,-1) {:?}",
        rig.components[i].class, o, d
    );
}
