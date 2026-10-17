//! M9.7's acceptance: the workshop's stats are the simulator's own numbers.
//!
//! This test does not recompute weight or hitpoints from a second formula. It
//! loads each shipped bot twice — once through the workshop model
//! (`ra2ui::workshop::{build, stats}`) and once through the simulator's own
//! loader (`ra2sim::rig::load`) — and asserts the numbers are equal, part by
//! part and in total.

use std::path::{Path, PathBuf};

use ra2ui::workshop::build::resolve_info;
use ra2ui::workshop::library::GameLibrary;
use ra2ui::workshop::stats::BotStats;

fn game_root() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("RA2_GAME") {
        let dir = PathBuf::from(dir);
        return dir.join("Components").is_dir().then_some(dir);
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while dir.pop() {
        let candidate = dir.join("local/game");
        if candidate.join("Components").is_dir() {
            return Some(candidate);
        }
    }
    None
}

fn bots(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            bots(&path, out);
        } else if path.extension().is_some_and(|e| e == "bot") {
            out.push(path);
        }
    }
}

#[test]
fn every_shipped_bots_stats_equal_the_rigs_own() {
    let Some(root) = game_root() else {
        eprintln!("skipping the stats comparison: no local/game (set RA2_GAME)");
        return;
    };
    let library = GameLibrary::open(&root).unwrap();
    let mut files = Vec::new();
    for sub in ["Robot Designs", "AI", "Teams"] {
        let dir = root.join(sub);
        if dir.is_dir() {
            bots(&dir, &mut files);
        }
    }
    files.sort();
    assert_eq!(files.len(), 57, "the shipped .bot corpus");

    let mut mass_max = 0.0f32;
    let mut hp_max = 0.0f32;
    let mut total_mass_max = 0.0f32;
    let mut total_hp_max = 0.0f32;
    let mut heaviest = (String::new(), 0.0f32);
    let mut lightest = (String::new(), f32::MAX);
    let mut chassis_only = 0usize;
    let mut over_editor_limit = 0usize;
    let mut class_matches = 0usize;

    for path in &files {
        let bot = GameLibrary::read_bot(path).unwrap();
        let info = resolve_info(&library, &bot);
        let stats = BotStats::new(&bot, &info);
        let rig = ra2sim::rig::load(path.to_str().unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        // The workshop's placement is the renderer/rig's own function, not a
        // second derivation: the full transform vector must agree.
        let build =
            ra2ui::workshop::build::Build::from_bot_with_info(bot.clone(), info.clone()).unwrap();
        let world = build.transforms();
        for (i, m) in world.iter().enumerate() {
            let delta = (*m - rig.components[i].local)
                .to_cols_array()
                .iter()
                .fold(0.0f32, |a, v| a.max(v.abs()));
            assert!(
                delta < 1e-5,
                "{}: component {i} transform differs from the rig by {delta}",
                path.display()
            );
        }

        assert_eq!(
            stats.parts.len(),
            rig.components.len(),
            "{} part count",
            path.display()
        );
        for (i, part) in stats.parts.iter().enumerate() {
            let sim = &rig.components[i];
            let dm = (part.mass - sim.mass).abs();
            let dh = (part.hitpoints - sim.hitpoints).abs();
            mass_max = mass_max.max(dm);
            hp_max = hp_max.max(dh);
            assert!(
                dm < 1e-4,
                "{}: part {i} ({}) mass {} != rig {}",
                path.display(),
                part.class,
                part.mass,
                sim.mass
            );
            assert!(
                dh < 1e-4,
                "{}: part {i} ({}) hitpoints {} != rig {}",
                path.display(),
                part.class,
                part.hitpoints,
                sim.hitpoints
            );
        }

        let sim_total_hp: f32 = rig.components.iter().map(|c| c.hitpoints).sum();
        let dtm = (stats.mass - rig.mass).abs();
        let dth = (stats.hitpoints - sim_total_hp).abs();
        total_mass_max = total_mass_max.max(dtm);
        total_hp_max = total_hp_max.max(dth);
        assert!(
            dtm < 1e-3,
            "{}: total mass {} != rig {}",
            path.display(),
            stats.mass,
            rig.mass
        );
        assert!(
            dth < 1e-3,
            "{}: total hitpoints {} != rig {}",
            path.display(),
            stats.hitpoints,
            sim_total_hp
        );

        let name = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();
        if stats.mass > heaviest.1 {
            heaviest = (name.clone(), stats.mass);
        }
        if stats.mass < lightest.1 {
            lightest = (name, stats.mass);
        }
        if stats.parts.len() == 1 {
            chassis_only += 1;
        }
        if stats.mass > 800.0 {
            over_editor_limit += 1;
        }
        if stats.declared_class == stats.computed_class {
            class_matches += 1;
        }
    }

    eprintln!(
        "{}/{} bots: max part |Δmass| {mass_max:.6} kg, max part |Δhp| {hp_max:.6}, \
         max total |Δmass| {total_mass_max:.6} kg, max total |Δhp| {total_hp_max:.6}",
        files.len(),
        files.len()
    );
    eprintln!(
        "heaviest {:.1} kg ({}), lightest {:.1} kg ({}), {chassis_only} chassis-only, \
         {over_editor_limit} over the editor's 800 kg, {class_matches}/{} classes agree",
        heaviest.1,
        heaviest.0,
        lightest.1,
        lightest.0,
        files.len()
    );
}

/// M9.7 for a *built* bot: build from scratch out of real components, save it,
/// and compare the model's stats with the rig's for the saved file.
#[test]
fn a_built_bots_stats_equal_the_rigs_after_a_save_and_reload() {
    let Some(root) = game_root() else { return };
    // The saved bot lives in a temporary folder, so the loader cannot find the
    // game by walking up from it; point it at the same root the model used.
    std::env::set_var("RA2_GAME", &root);
    let library = GameLibrary::open(&root).unwrap();
    let stock = GameLibrary::read_bot(&root.join("AI/Team1/Bot0.bot")).unwrap();
    let mut chassis = stock.components[0].chassis.clone().unwrap();
    chassis.texture_tga = None;
    let mut build = ra2ui::workshop::build::Build::new("Built Stats", chassis);

    let motor_info = library.component_info_by_file("Components\\anglemotor1.txt");
    let base_mount = *motor_info.mounts.keys().next().unwrap();
    let motor = build
        .attach(
            &library,
            "Components\\anglemotor1.txt",
            0,
            0,
            base_mount,
            ra2ui::workshop::build::Placement::default(),
        )
        .unwrap();
    let wheel_info = library.component_info_by_file("Components\\miniwheel.txt");
    let own_mount = *wheel_info
        .mounts
        .iter()
        .find(|(_, a)| a.types.iter().any(|t| t == "axle_f"))
        .map(|(id, _)| id)
        .unwrap();
    let axle = *motor_info
        .mounts
        .iter()
        .find(|(_, a)| a.types.iter().any(|t| t == "axle_m"))
        .map(|(id, _)| id)
        .unwrap();
    build
        .attach(
            &library,
            "Components\\miniwheel.txt",
            motor,
            axle,
            own_mount,
            ra2ui::workshop::build::Placement::default(),
        )
        .unwrap();

    let model = build.stats();
    let dir = std::env::temp_dir().join(format!("ra2-stats-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Built Stats.bot");
    build.save_to(&path).unwrap();
    let rig = ra2sim::rig::load(path.to_str().unwrap()).unwrap();
    assert!(
        (model.mass - rig.mass).abs() < 1e-3,
        "{} vs {}",
        model.mass,
        rig.mass
    );
    let sim_hp: f32 = rig.components.iter().map(|c| c.hitpoints).sum();
    assert!((model.hitpoints - sim_hp).abs() < 1e-3);
    for (i, part) in model.parts.iter().enumerate() {
        assert!((part.mass - rig.components[i].mass).abs() < 1e-4);
        assert!((part.hitpoints - rig.components[i].hitpoints).abs() < 1e-4);
    }
    eprintln!(
        "built bot: {:.1} kg, {:.0} hp, {} parts (rig {:.1} kg, {:.0} hp)",
        model.mass,
        model.hitpoints,
        model.parts.len(),
        rig.mass,
        sim_hp
    );
    std::fs::remove_dir_all(&dir).ok();
}
