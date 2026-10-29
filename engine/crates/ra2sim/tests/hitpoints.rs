//! Chassis hitpoints are `trunc(surface area x armour hardness)` (`FUN_0044d710`).
//! The original reports 1828 for the stock Scout and 2662 for ALARM live.

use std::path::PathBuf;

use ra2sim::rig;

fn game_dir() -> Option<PathBuf> {
    ra2fmt::saves::isolate_for_tests();
    if let Some(dir) = std::env::var_os("RA2_GAME") {
        return Some(PathBuf::from(dir));
    }
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let p = here.join("../../../local/game");
    p.is_dir().then_some(p)
}

#[test]
fn stock_chassis_hitpoints_match_the_original() {
    let Some(game) = game_dir() else { return };
    for (bot, hp) in [("Bot0.bot", 1828.0), ("Bot1.bot", 2662.0)] {
        let path = game.join("AI/Team0").join(bot);
        let r = rig::load(path.to_str().unwrap()).expect("rig");
        assert_eq!(r.components[0].hitpoints, hp, "{bot}");
    }
}
