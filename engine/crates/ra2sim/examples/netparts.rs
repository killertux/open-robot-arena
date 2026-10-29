//! Diagnostic: a robot's assembled bodies with the ids the original's snapshots give them.
//!
//! Usage: netparts <bot>...
use ra2sim::rig;

fn main() {
    for path in std::env::args().skip(1) {
        let r = rig::load(&path).expect("a robot");
        println!(
            "robot {:?}: {} components, {} parts",
            r.name,
            r.components.len(),
            r.parts.len()
        );
        for (i, c) in r.components.iter().enumerate() {
            println!(
                "  comp {i:2} parent {:?} role {:?} class {:?} name {:?} pivots {} hinge {}",
                c.parent,
                c.role,
                c.class,
                c.name,
                c.wheel_pivots.len(),
                c.hinge.is_some()
            );
        }
        for (p, part) in r.parts.iter().enumerate() {
            println!(
                "  part {p:2} root {:2} members {:?} parent {:?} joint {}",
                part.root,
                part.members,
                part.parent,
                part.joint
                    .as_ref()
                    .map(|j| format!("{j:?}"))
                    .unwrap_or_default()
                    .chars()
                    .take(70)
                    .collect::<String>()
            );
        }
    }
}
