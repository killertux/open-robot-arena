//! Start points: every robot must begin on the surface its start point stands
//! over. The Metal Skull's left start point sits over a plateau its decoded
//! rigid-body geometry does not carry; ALARM used to fall into the pit (y < -2)
//! and the match ended at 0.02 s.

use std::path::{Path, PathBuf};

use ra2sim::{BotSetup, Duel};

fn game_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("RA2_GAME") {
        return Some(PathBuf::from(dir));
    }
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../local/game");
    p.is_dir().then_some(p)
}

#[test]
fn every_stock_arena_starts_both_robots_above_the_pit() {
    let Some(game) = game_dir() else { return };
    let _scripts = ra2script::lock();
    for (arena, floor_min) in [("Skull.py", 0.5f32), ("box.py", -3.0)] {
        let bots: Vec<BotSetup> = ["AI/Team0/Bot0.bot", "AI/Team0/Bot1.bot"]
            .iter()
            .map(|b| BotSetup {
                path: game.join(b).to_string_lossy().to_string(),
                yaw_deg: None,
                spawn: None,
            })
            .collect();
        let path = game
            .join("Arenas")
            .join(arena)
            .to_string_lossy()
            .to_string();
        let mut duel = Duel::new(&path, &bots).expect("a duel");
        for _ in 0..60 {
            duel.step_direct(&[Default::default(), Default::default()], 1.0 / 60.0);
        }
        for r in &duel.robots {
            let y = r.position(&*duel.physics).y;
            assert!(y > floor_min, "{arena}: {} rests at y = {y}", r.rig.name);
        }
    }
}
