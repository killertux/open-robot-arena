//! `plus.getWeight` in a match against the original (docs/29 section 12aj).
//!
//! Live (`fightcontacts-sentinel-wideload-compressor__live__004..007`'s json
//! `getWeight`, the Scout from the same session's C.R.U.S.H. attempt, ALARM
//! likewise) the scripts see the sum of the **collidable** bodies' mass: the
//! no-collision phantoms (a wheel's traction ring, a SmartZone) are in the
//! Havok simulation but not in the weight, unlike the Bot Lab's number
//! (`0x45a4f0` there sums every body: C.R.U.S.H. 395.2).

use std::path::{Path, PathBuf};

use ra2sim::{BotSetup, Duel};

fn game_dir() -> Option<PathBuf> {
    ra2fmt::saves::isolate_for_tests();
    if let Some(dir) = std::env::var_os("RA2_GAME") {
        return Some(PathBuf::from(dir));
    }
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    for up in [3usize, 4] {
        let mut p = here.to_path_buf();
        for _ in 0..up {
            p.pop();
        }
        let p = p.join("local/game");
        if p.join("Components").is_dir() {
            return Some(p);
        }
    }
    None
}

#[test]
fn the_weight_the_scripts_see_leaves_the_phantom_bodies_out_as_live() {
    let Some(game) = game_dir() else {
        eprintln!("skipped: no game data (set RA2_GAME)");
        return;
    };
    let _scripts = ra2script::lock();
    let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
    // (bot, live getWeight, phantom mass the lab sum adds)
    let live = [
        ("AI/Team0/Bot2.bot", 416.394_1, 10.0),
        ("AI/Team8/Bot2.bot", 383.667_85, 22.0),
        ("AI/Team0/Bot0.bot", 225.494_66, 18.0),
        ("AI/Team0/Bot1.bot", 339.116_02, 34.0),
    ];
    for (bot, weight, phantoms) in live {
        let setup = BotSetup {
            path: game.join(bot).to_string_lossy().to_string(),
            yaw_deg: None,
            spawn: None,
        };
        let d = Duel::new(&arena, std::slice::from_ref(&setup)).expect("a duel");
        let r = &d.robots[0];
        assert!(
            (r.remaining_mass() - weight).abs() < 0.01,
            "{bot}: getWeight {} live {weight}",
            r.remaining_mass()
        );
        let ghosts: f32 = r.rig.components.iter().map(|c| c.havok.phantom_mass).sum();
        assert!((ghosts - phantoms).abs() < 0.01, "{bot}: phantoms {ghosts}");
    }
}
