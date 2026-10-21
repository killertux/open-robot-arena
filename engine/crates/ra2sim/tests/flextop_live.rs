//! FlexTop's table against the original (docs/29 section 12j).
//!
//! The table (`floor_collision`, 400 kg) hangs from the fixed `anchor03` by a
//! point-to-point constraint at (0, 0.2025, 0) and two angular dashpots
//! (strength 1000, damping 0.08); the only fixed collidable is the implicit
//! plane `Plane01` 9.8 m down. Live (`flextable-scout-alarm-flextop__live__000`,
//! `frida/ra2_dashpot.js`, the table's `T` rows once the robots had fallen
//! off) the free table swings through these states, one step apart.

use std::path::{Path, PathBuf};

use glam::{Quat, Vec3};
use ra2sim::Duel;

fn game_dir() -> Option<PathBuf> {
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

/// Live, steps 0 / 30 / 60 / 120 of the free swing: the table's rotation
/// vector (rad) and angular velocity (rad/s).
const LIVE: [(usize, [f32; 3], [f32; 3]); 4] = [
    (0, [0.0048, -0.0108, 0.0990], [0.3696, -0.0816, 0.4801]),
    (30, [0.0412, -0.0241, -0.0250], [-0.2906, 0.0749, -0.6713]),
    (60, [-0.0701, 0.0217, -0.0598], [0.1201, 0.0902, 0.5886]),
    (
        120,
        [-0.0445, -0.0281, -0.1325],
        [-0.3059, -0.0152, -0.1475],
    ),
];

#[test]
fn the_flextop_table_swings_on_its_pivot_as_live() {
    let Some(game) = game_dir() else {
        eprintln!("skipped: no game data (set RA2_GAME)");
        return;
    };
    if std::env::var("RA2_PHYSICS").is_ok_and(|b| b != "havok17") {
        eprintln!("skipped: not the havok17 backend");
        return;
    }
    let _scripts = ra2script::lock();
    let arena = game.join("Arenas/flextop.py").to_string_lossy().to_string();
    let mut duel = Duel::new(&arena, &[]).expect("a duel");
    let table = duel
        .props
        .iter()
        .find(|p| p.prop.name == "floor_collision")
        .map(|p| p.body)
        .expect("FlexTop's table is a movable body");

    // Mass properties as measured live (body +0x190, +0x30, +0x19c).
    let (mass, com, inv) = duel.physics.mass_properties(table).expect("mass");
    assert!((mass - 400.0).abs() < 1e-3, "{mass} kg");
    assert!(
        (com - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-3,
        "centre {com}"
    );
    let i = inv.inverse();
    assert!((i.x_axis.x - 15550.73).abs() < 1.0 && (i.y_axis.y - 31099.06).abs() < 1.0);

    // Start from the live state; the pivot holds the table's (0, 0.1196, 0).
    let pivot = Vec3::new(0.0, 0.202_458_58, 0.0);
    let on_table = Vec3::new(0.0, 0.119_571_24, 0.0);
    let rot = Quat::from_scaled_axis(Vec3::from(LIVE[0].1));
    duel.physics
        .set_transform(table, pivot - rot * on_table, rot);
    duel.physics.set_velocity(
        table,
        Vec3::new(-0.4209, -0.2408, 0.3171),
        Vec3::from(LIVE[0].2),
    );
    let mut step = 0;
    for (at, rv, w) in LIVE {
        while step < at {
            duel.step_direct(&[], ra2sim::DEFAULT_STEP);
            step += 1;
        }
        let s = duel.physics.state(table).expect("state");
        let ours = s.rotation.to_scaled_axis();
        assert!(
            (ours - Vec3::from(rv)).length() < 0.012,
            "step {at}: rotation {ours} live {rv:?}"
        );
        assert!(
            (s.angular_velocity - Vec3::from(w)).length() < 0.06,
            "step {at}: angular velocity {} live {w:?}",
            s.angular_velocity
        );
        let held = s.position + s.rotation * on_table;
        assert!((held - pivot).length() < 0.01, "step {at}: pivot at {held}");
    }
}
