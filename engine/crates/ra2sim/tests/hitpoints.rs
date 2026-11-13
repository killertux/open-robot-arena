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

/// Live (docs/36 9.40): the original's component list and maximum hitpoints at the match start, per robot
/// (`tests/data/partcount_live.tsv`: the K rows of `partsfight-*` and the `event-repairs-triples` capture):
/// same count, same order, same maximum for the chassis (trunc(area x hardness)) and every other component.
/// The Atom Smasher of the career team (`Teams/Team0/Bot4.bot`, 1.11, 17 components) is the one the 9.39 live
/// log saw; `Robot Designs/XTRA_Atom Smasher.bot` is a different, older design (1.07, 18 components, a
/// SmartZone, chassis 1604) and not the robot that log was about.
#[test]
fn components_and_maximum_hitpoints_match_the_original_per_robot() {
    let Some(game) = game_dir() else { return };
    let table = include_str!("data/partcount_live.tsv");
    let mut bots: Vec<(String, Vec<f32>)> = Vec::new();
    for line in table
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<&str> = line.split('\t').collect();
        let (bot, id, max): (&str, usize, f32) =
            (f[0], f[1].parse().unwrap(), f[2].parse().unwrap());
        if bots.last().map(|b| b.0.as_str()) != Some(bot) {
            bots.push((bot.to_string(), Vec::new()));
        }
        let live = &mut bots.last_mut().unwrap().1;
        assert_eq!(live.len(), id, "{bot}: ids run 0..n");
        live.push(max);
    }
    assert!(
        bots.len() >= 7,
        "Scout, ALARM, Razor, FlapJack, Sentinel, Wide Load, Atom Smasher"
    );
    for (bot, live) in bots {
        let path = game.join(&bot);
        if !path.is_file() {
            continue;
        }
        let r = rig::load(path.to_str().unwrap()).expect("rig");
        let ours: Vec<f32> = r.components.iter().map(|c| c.hitpoints).collect();
        assert_eq!(ours, live, "{bot}");
    }
}
