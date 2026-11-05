//! The Scout stood on its tail, against the original's controlled tips
//! (`docs/29-havok17-backend.md` section 12bd).
//!
//! `frida/ra2_tipab.js` (`tools/oracle/capture_tipab.py`) turned the Scout of a box match
//! rigidly about its chassis' centre of mass (pitch about the chassis' side axis, the centre
//! 0.05 m above where its lowest point rests) and let it go. 162 such trials, free floor and
//! against the wall (`tipab-scout-*__live__*`), come out the same as ours in kind (on the tail,
//! on the wheels, on the back) in all but 15 at the edge of a basin: the tail stand is a
//! real pose of the original, 0.320 m above the centre height of the rest, vertical to 0.0024
//! in `up.y` (live 0.00237, ours 0.00247 over the stands), and it holds for 60 s.
//! The cases below are the ones both engines agree on well inside their basins.

use std::path::{Path, PathBuf};

use glam::{Quat, Vec3};
use ra2sim::{BotSetup, DriveInput, Duel, DEFAULT_STEP};

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

/// The chassis' side axis (the wheels' axle) in the chassis frame, the live trials' pitch axis.
const SIDE: Vec3 = Vec3::X;
/// The live trials released the robot with its centre this far above the rest centre (the lowest
/// point 0.05 m above where it rests: the `dcentre` of the grids), by pitch in degrees.
fn lift_for(deg: f32) -> f32 {
    match deg as i32 {
        -80 => 0.3794,
        -85 => 0.372,
        -95 => 0.359,
        -100 => 0.3815,
        _ => 0.3598,
    }
}

/// Settle the Scout alone on the box floor, turn every body rigidly about the chassis' centre
/// of mass by `deg` about its side axis (spinning at `w` rad/s about it), release it 0.05 m up
/// and run `secs`. Returns the chassis' rest centre height, then its centre height and axes
/// (side, up, forward) in the world at the end.
fn tip(game: &Path, deg: f32, w: f32, secs: f32) -> (f32, f32, [Vec3; 3]) {
    let _scripts = ra2script::lock();
    let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
    let setup = BotSetup {
        path: game.join("AI/Team0/Bot0.bot").to_string_lossy().to_string(),
        yaw_deg: None,
        spawn: None,
    };
    let mut duel = Duel::new(&arena, std::slice::from_ref(&setup)).expect("a duel");
    let idle = [DriveInput::default()];
    for _ in 0..60 {
        duel.step_direct(&idle, DEFAULT_STEP);
    }
    let bodies: Vec<_> = duel.robots[0].parts.iter().map(|p| p.body).collect();
    let snap: Vec<(u32, Vec3, Quat)> = bodies
        .iter()
        .map(|&b| {
            let s = duel.physics.state(b).unwrap();
            (b, duel.physics.center_of_mass(b).unwrap(), s.rotation)
        })
        .collect();
    let c0 = snap[0].1;
    let r0 = snap[0].2;
    let place = |duel: &mut Duel, q: Quat, at: Vec3, w: Vec3| {
        for &(b, c, rot) in &snap {
            let s = duel.physics.state(b).unwrap();
            let now = duel.physics.center_of_mass(b).unwrap();
            let off = s.rotation.inverse() * (s.position - now);
            let com = at + q * (c - c0);
            let r = q * rot;
            duel.physics.set_transform(b, com + r * off, r);
            duel.physics.set_velocity(b, w.cross(com - at), w);
        }
    };
    // Hold it 1 m up and still for a while, as the live agent does, then turn and release it.
    for _ in 0..30 {
        place(&mut duel, Quat::IDENTITY, c0 + Vec3::Y, Vec3::ZERO);
        duel.step_direct(&idle, DEFAULT_STEP);
    }
    let axis = r0 * SIDE;
    let q = Quat::from_axis_angle(axis, deg.to_radians());
    place(&mut duel, q, c0 + Vec3::Y * lift_for(deg), axis * w);
    for _ in 0..(secs / DEFAULT_STEP).round() as usize {
        duel.step_direct(&idle, DEFAULT_STEP);
    }
    let chassis = bodies[0];
    let s = duel.physics.state(chassis).unwrap();
    let c = duel.physics.center_of_mass(chassis).unwrap();
    (
        c0.y,
        c.y,
        [
            s.rotation * Vec3::X,
            s.rotation * Vec3::Y,
            s.rotation * Vec3::Z,
        ],
    )
}

fn available() -> Option<PathBuf> {
    let Some(game) = game_dir() else {
        eprintln!("skipped: no game data (set RA2_GAME)");
        return None;
    };
    if std::env::var("RA2_PHYSICS").is_ok_and(|b| b != "havok17") {
        eprintln!("skipped: not the havok17 backend");
        return None;
    }
    Some(game)
}

/// Pitched -90, -85 or -95 degrees and let go, the Scout ends standing on its tail: its forward
/// axis vertical, its up axis level (live: `up.y` 0.0024, the centre 0.320 m above the rest
/// centre, held for 60 s).
#[test]
fn the_scout_stands_on_its_tail_as_the_original_does() {
    let Some(game) = available() else { return };
    for deg in [-85.0, -90.0, -95.0] {
        let (rest, y, [_, up, fwd]) = tip(&game, deg, 0.0, 5.0);
        assert!(fwd.y > 0.99, "pitch {deg}: forward axis y {}", fwd.y);
        assert!(up.y.abs() < 0.03, "pitch {deg}: up axis y {}", up.y);
        assert!(
            (y - rest - 0.320).abs() < 0.01,
            "pitch {deg}: centre {} above rest",
            y - rest
        );
    }
}

/// Spun about its side axis as it is let go, the Scout ends where the live trials put it
/// (`tipab-scout-tail-wscan-box__live__000`): standing for a gentle spin, back on its wheels
/// or on its back for a harder one, the same sides of the basins. Only the cases well inside
/// a basin; the edges between them are chaotic in both engines.
#[test]
fn the_tail_stand_basins_are_the_originals() {
    let Some(game) = available() else { return };
    // (pitch, spin, up.y of the chassis: 1 on its wheels, -1 on its back, ~0 on its tail)
    let cases = [
        (-90.0, -3.0, 0.0),
        (-90.0, -1.0, 0.0),
        (-90.0, 1.25, 0.0),
        (-90.0, 2.25, 1.0),
        (-90.0, -6.5, -1.0),
        (-80.0, -3.0, 0.0),
        (-80.0, 1.0, 1.0),
        (-100.0, -2.5, 1.0),
        (-100.0, -4.5, -1.0),
        (-100.0, 1.5, 0.0),
    ];
    for (deg, w, want) in cases {
        let (_, _, [_, up, _]) = tip(&game, deg, w, 5.0);
        assert!(
            (up.y - want).abs() < 0.1,
            "pitch {deg} spin {w}: up.y {} (live {want})",
            up.y
        );
    }
}
