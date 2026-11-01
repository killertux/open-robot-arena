//! Diagnostic: each robot's `plus.getWeight` as ours reports it (the collidable bodies), with and
//! without the no-collision phantom bodies (docs/29 section 12aj).
//!
//! Usage: `weightprobe <arena.py> <botA> <botB>`
use ra2sim::{BotSetup, Duel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let setups: Vec<BotSetup> = a[1..]
        .iter()
        .map(|p| BotSetup {
            path: p.clone(),
            yaw_deg: None,
            spawn: None,
        })
        .collect();
    let duel = Duel::new(&a[0], &setups)?;
    for r in &duel.robots {
        let phantoms: f32 = r.rig.components.iter().map(|c| c.havok.phantom_mass).sum();
        println!(
            "{:?} lab-style sum {:.3} collidable (= getWeight) {:.3} phantoms {:.3}",
            r.rig.name,
            r.remaining_mass() + phantoms,
            r.remaining_mass(),
            phantoms
        );
        if std::env::var_os("WEIGHT_PARTS").is_some() {
            for c in &r.rig.components {
                println!(
                    "   {:24} mass {:7.3} phantom {:6.3}",
                    c.name, c.mass, c.havok.phantom_mass
                );
            }
        }
    }
    Ok(())
}
