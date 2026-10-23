//! Start points: every robot must begin on the surface its start point stands
//! over. The Metal Skull's left start point sits over a plateau its decoded
//! rigid-body geometry does not carry; ALARM used to fall into the pit (y < -2)
//! and the match ended at 0.02 s.

use std::path::{Path, PathBuf};

use ra2sim::{BotSetup, Duel};

fn game_dir() -> Option<PathBuf> {
    ra2fmt::saves::isolate_for_tests();
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

/// The Combat Arena collides as its `arena_collision` body's own box: live
/// the only fixed collidable has that box's bounds (`colliders-box__live__000`),
/// and wheels cross the spike and saw plates at floor height. So no surface
/// stands above the floor inside the arena.
#[test]
fn the_combat_arena_collides_as_its_own_box() {
    let Some(game) = game_dir() else {
        eprintln!("skipped: no game data (set RA2_GAME)");
        return;
    };
    let path = game.join("Arenas/box.py").to_string_lossy().to_string();
    let scene = ra2sim::scene::load_arena(&path).expect("arena");
    for t in &scene.triangles {
        for v in t {
            let inside = v.x.abs() < 12.6 && v.z > -12.3 && v.z < 12.9;
            assert!(
                !inside || v.y < -2.26 || v.y > 13.7,
                "a collider vertex above the floor inside the arena: {v:?}"
            );
        }
    }
}
