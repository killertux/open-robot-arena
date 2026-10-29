//! The hinge and prismatic controllers against the original's live numbers
//! (docs/29 section 12, `ra2sim::motor`): the load cap a robot puts on its
//! hinges, a burst motor's firing and cocking power, a burst piston's stroke
//! and a spinner's spin-up.

use std::path::{Path, PathBuf};

use glam::Vec3;
use ra2sim::rig::JointUse;
use ra2sim::{BotSetup, DriveInput, Duel};

const DT: f32 = 1.0 / 60.0;

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
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

macro_rules! game {
    () => {
        match game_dir() {
            Some(g) => g,
            None => {
                eprintln!("skipping: no local/game (set RA2_GAME)");
                return;
            }
        }
    };
}

/// One stock bot alone in the box arena, settled for `frames`.
fn duel(game: &Path, bot: &str, frames: usize) -> Duel {
    let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
    let setup = BotSetup {
        path: game.join(bot).to_string_lossy().to_string(),
        yaw_deg: Some(270.0),
        spawn: Some([0.0, 0.0]),
    };
    let mut d = Duel::new(&arena, std::slice::from_ref(&setup)).expect("a duel");
    for _ in 0..frames {
        d.step_direct(&[DriveInput::default()], DT);
    }
    d
}

fn parts_of(d: &Duel, kind: JointUse) -> Vec<usize> {
    let r = &d.robots[0];
    (0..r.rig.parts.len())
        .filter(|&p| r.rig.parts[p].joint.is_some_and(|j| j.use_kind == kind))
        .collect()
}

/// The control wired to the motor of part `p`.
fn control_of(d: &Duel, p: usize) -> String {
    let r = &d.robots[0];
    let motor = r.rig.parts[p].joint.unwrap().motor;
    let w = &r.rig.wiring;
    let wire = w
        .wires
        .iter()
        .find(|x| x.component == motor)
        .expect("wired");
    w.controls[wire.controller].name.clone()
}

/// The child body's angular speed about the hinge, relative to its parent.
fn hinge_speed(d: &Duel, p: usize) -> f32 {
    let r = &d.robots[0];
    let info = r.rig.parts[p].joint.unwrap();
    let parent = r.rig.parts[p].parent.unwrap();
    let a = d.physics.state(r.parts[parent].body).unwrap();
    let b = d.physics.state(r.parts[p].body).unwrap();
    let axis = a.rotation
        * r.rig.components[r.rig.parts[parent].root]
            .local
            .inverse()
            .transform_vector3(info.axis);
    (b.angular_velocity - a.angular_velocity).dot(axis.normalize_or(Vec3::Y))
}

/// The robot caps every hinge by the body it drives (`0x480510`), and the
/// live controllers read: the Scout's and ALARM's drive hinges `675` N·m at
/// speed 18 (27 kg wheels), the Scout's fork `462.4` (ours 451: our arm's
/// centre of mass sits 2 cm off the original's), Tornado's spinner `3123.9`
/// at 27.6, Ripblade's `1324.5` at 32.6.
#[test]
fn every_hinge_is_capped_by_the_load_it_carries() {
    let game = game!();
    let _scripts = ra2script::lock();
    for (bot, kind, torque, speed, tol) in [
        ("AI/Team0/Bot0.bot", JointUse::Wheel, 675.0, 18.0, 0.5),
        ("AI/Team0/Bot1.bot", JointUse::Wheel, 675.0, 18.0, 0.5),
        ("AI/Team0/Bot0.bot", JointUse::Burst, 462.4, 5.0, 15.0),
        ("AI/Team13/Bot1.bot", JointUse::Spun, 3123.9, 27.64, 1.0),
        ("AI/Team14/Bot1.bot", JointUse::Spun, 1324.5, 32.59, 1.0),
    ] {
        let d = duel(&game, bot, 0);
        let parts = parts_of(&d, kind);
        assert!(!parts.is_empty(), "{bot}: no {kind:?} hinge");
        for p in parts {
            let s = d.robots[0].hinge_settings(p).unwrap();
            assert!(
                (s.max_torque - torque).abs() <= tol,
                "{bot} part {p}: {} N·m, live {torque}",
                s.max_torque
            );
            assert!(
                (s.motor_speed - speed).abs() < 0.05,
                "{bot} part {p}: speed {}, live {speed}",
                s.motor_speed
            );
        }
    }
}

/// The Scout's fork fires on its `burst` pair (`50 90`: the motor at 49 rad/s
/// and 3600 N·m), is at its end within a few substeps (live 6, peak 37 rad/s),
/// and 0.6 s after the trigger cocks back on its `power` pair (`5 35`: 4 rad/s,
/// 1400 N·m), home in about 0.4 s (`weapons-scout-alarm__live__000`).
#[test]
fn a_burst_motor_fires_on_its_burst_power_and_cocks_on_its_power() {
    let game = game!();
    let _scripts = ra2script::lock();
    let mut d = duel(&game, "AI/Team0/Bot0.bot", 60);
    let p = parts_of(&d, JointUse::Burst)[0];
    let joint = d.robots[0].parts[p].joint.unwrap();
    let info = d.robots[0].rig.parts[p].joint.unwrap();
    let end = if info.limits[1].abs() >= info.limits[0].abs() {
        info.limits[1]
    } else {
        info.limits[0]
    };
    let name = control_of(&d, p);
    d.robots[0].apply_inputs(&[(name, 0, 1.0)]);
    d.step_direct(&[DriveInput::default()], DT);
    let s = d.robots[0].hinge_settings(p).unwrap();
    assert_eq!(
        (s.speed, s.max_torque, s.velocity(100)),
        (50.0, 3600.0, 49.0)
    );
    let mut peak = 0.0f32;
    for _ in 0..9 {
        d.step_direct(&[DriveInput::default()], DT);
        peak = peak.max(hinge_speed(&d, p).abs());
    }
    let angle = d.physics.hinge_angle(joint);
    assert!(
        (angle - end).abs() < 0.12,
        "the fork is at {angle} of {end} 0.17 s after the trigger"
    );
    assert!(
        (25.0..50.0).contains(&peak),
        "fork peak {peak} rad/s, live 37"
    );
    // Still out at 0.5 s, cocking from 0.6 s on the power pair.
    for _ in 0..20 {
        d.step_direct(&[DriveInput::default()], DT);
    }
    assert!(
        (d.physics.hinge_angle(joint) - end).abs() < 0.15,
        "it came back early"
    );
    for _ in 0..8 {
        d.step_direct(&[DriveInput::default()], DT);
    }
    let s = d.robots[0].hinge_settings(p).unwrap();
    assert_eq!(
        (s.speed, s.max_torque, s.velocity(-100)),
        (5.0, 1400.0, -4.0)
    );
    for _ in 0..40 {
        d.step_direct(&[DriveInput::default()], DT);
    }
    // Locked home (within the lock's give as the chassis rocks).
    let angle = d.physics.hinge_angle(joint);
    assert!(
        angle.abs() < 0.12,
        "the fork is not home 1.3 s after the trigger: {angle}"
    );
}

/// ALARM's burst piston: out to its lower stop (-0.73) in about six substeps,
/// held there until 0.25 s, back to its neutral point in about five and locked
/// there, ready to fire again (`weapons-scout-alarm__live__000`: stops 0 and
/// -0.73, a -1000 push, then -10000 a substep).
#[test]
fn a_burst_piston_strikes_out_and_back_as_the_original() {
    let game = game!();
    let _scripts = ra2script::lock();
    let mut d = duel(&game, "AI/Team0/Bot1.bot", 60);
    let p = parts_of(&d, JointUse::Burst)[0];
    let joint = d.robots[0].parts[p].joint.unwrap();
    assert_eq!(d.robots[0].rig.parts[p].joint.unwrap().limits, [-0.73, 0.0]);
    let name = control_of(&d, p);
    for shot in 0..2 {
        d.robots[0].apply_inputs(&[(name.clone(), 0, 1.0)]);
        let mut out_at = None;
        for k in 1..=60 {
            d.step_direct(&[DriveInput::default()], DT);
            let x = d.physics.slider_offset(joint);
            if out_at.is_none() && x < -0.69 {
                out_at = Some(k);
            }
            if k == 13 {
                assert!(x < -0.6, "shot {shot}: back early at 0.22 s ({x})");
            }
        }
        let out_at = out_at.expect("the rod never reached its stop");
        assert!(out_at <= 8, "shot {shot}: out in {out_at} substeps, live 6");
        let x = d.physics.slider_offset(joint);
        assert!(
            x.abs() < 0.03,
            "shot {shot}: not home 1 s after the trigger ({x})"
        );
    }
}

/// The piston pushes its rod alone (`0x4b51b0`): the chassis does not recoil
/// while the hammer goes out and takes its momentum at the stop. Live
/// (`fightcontacts-scout-alarm-box__live__001/002`, ALARM upright on its
/// wheels), the chassis' speed along the stroke goes `0 → +0.4..1.0 → +1.6..2.2`
/// m/s in two steps, and back by about -2 when the rod returns (docs/29
/// section 12k).
#[test]
fn a_burst_piston_kicks_the_robot_only_at_its_stop() {
    let game = game!();
    let _scripts = ra2script::lock();
    let mut d = duel(&game, "AI/Team0/Bot1.bot", 60);
    let p = parts_of(&d, JointUse::Burst)[0];
    let name = control_of(&d, p);
    let along = |d: &Duel| {
        let r = &d.robots[0];
        let fwd = r
            .transform(&*d.physics)
            .transform_vector3(r.rig.forward())
            .normalize();
        d.physics
            .state(r.parts[0].body)
            .unwrap()
            .linear_velocity
            .dot(fwd)
    };
    d.robots[0].apply_inputs(&[(name, 0, 1.0)]);
    let speeds: Vec<f32> = (0..12)
        .map(|_| {
            d.step_direct(&[DriveInput::default()], DT);
            along(&d)
        })
        .collect();
    let peak = speeds.iter().copied().fold(f32::MIN, f32::max);
    let low = speeds.iter().copied().fold(f32::MAX, f32::min);
    assert!(low > -0.2, "the chassis recoils: {speeds:?}");
    assert!(
        (1.5..2.6).contains(&peak),
        "the kick at the stop: {peak} (live 1.6-2.2), {speeds:?}"
    );
}

/// Tornado's spinner: the motor runs at 27 rad/s but the shell settles near 15
/// within half a second (live 14-16 from 0.4 s on,
/// `weapons-tornado-ripblade__live__000`), and its `Spin` switch toggles on each
/// press.
#[test]
fn a_spinner_spins_up_to_the_live_plateau() {
    let game = game!();
    let _scripts = ra2script::lock();
    let mut d = duel(&game, "AI/Team13/Bot1.bot", 30);
    let p = parts_of(&d, JointUse::Spun)[0];
    let name = control_of(&d, p);
    d.robots[0].apply_inputs(&[(name.clone(), 0, 1.0)]);
    let mut speeds = Vec::new();
    for _ in 0..120 {
        d.step_direct(&[DriveInput::default()], DT);
        speeds.push(hinge_speed(&d, p).abs());
    }
    let late: f32 = speeds[60..].iter().sum::<f32>() / 60.0;
    assert!(
        (12.0..19.0).contains(&late),
        "Tornado's spinner at {late} rad/s, live ~15"
    );
    assert!(speeds[29] > 10.0, "slow spin-up: {} at 0.5 s", speeds[29]);
    // A release leaves the switch on; a second press turns it off.
    d.robots[0].apply_inputs(&[(name.clone(), 0, 0.0)]);
    d.step_direct(&[DriveInput::default()], DT);
    assert_eq!(d.robots[0].hinge_settings(p).unwrap().velocity(100), 27.0);
    let cmd = |d: &Duel| {
        let r = &d.robots[0];
        r.control
            .command(&r.rig.wiring, r.rig.parts[p].joint.unwrap().motor)
            .unwrap_or(0.0)
    };
    assert_eq!(cmd(&d), 100.0);
    d.robots[0].apply_inputs(&[(name, 0, 1.0)]);
    d.step_direct(&[DriveInput::default()], DT);
    assert_eq!(cmd(&d), 0.0);
}

/// ALARM's hammer rod, fired: pushed out, it lands on its stop and stays there
/// (the prismatic atom re-solves its limit in every solver pass, `0x555960` →
/// `0x555cc0`), and it starts back 14 substeps after it started out (the
/// piston's tick runs once a frame, `0x448650`). Live (docs/29 section 12r,
/// `tipab-alarm-box-hammer__live__000`, ALARM on its tail): out at 5.4, 10.2,
/// 15.0, 12.8 m/s and dead at the stop, 0.000-0.001 m from it, until the
/// return; the rod's first step back 14 substeps after its first step out
/// (the return push 15 after the one-shot push).
#[test]
fn alarms_rod_stops_dead_at_its_stop_for_a_frame_counted_stroke() {
    let game = game!();
    let _scripts = ra2script::lock();
    let mut d = duel(&game, "AI/Team0/Bot1.bot", 60);
    let p = parts_of(&d, JointUse::Burst)[0];
    let joint = d.robots[0].parts[p].joint.unwrap();
    let name = control_of(&d, p);
    d.robots[0].apply_inputs(&[(name, 0, 1.0)]);
    let offsets: Vec<f32> = (0..40)
        .map(|_| {
            d.step_direct(&[DriveInput::default()], DT);
            d.physics.slider_offset(joint)
        })
        .collect();
    let out = offsets.iter().position(|o| *o < -0.005).expect("it fires");
    let end = offsets
        .iter()
        .position(|o| *o < -0.725)
        .expect("it reaches its stop");
    let back = (end..offsets.len())
        .find(|&i| offsets[i] > -0.725)
        .expect("it returns");
    for o in &offsets[end..back] {
        assert!((o + 0.73).abs() < 0.002, "off its stop: {offsets:?}");
    }
    assert_eq!(back - out, 14, "the stroke in substeps: {offsets:?}");
}

/// The Scout's fork: from its firing setting to its cocking one, 34 substeps,
/// 0.6 s counted in whole frames by the burst motor's tick (`0x446670`). Live
/// (docs/29 section 12r, `0x4815c0` against the substeps): 34 or 36 (36 after
/// a short frame). Counted every substep it took 36.
#[test]
fn the_scouts_fork_cocks_two_frames_short_of_counting_substeps() {
    let game = game!();
    let _scripts = ra2script::lock();
    let mut d = duel(&game, "AI/Team0/Bot0.bot", 60);
    let p = parts_of(&d, JointUse::Burst)[0];
    let name = control_of(&d, p);
    let speed = |d: &Duel| d.robots[0].hinge_settings(p).unwrap().velocity(100);
    let cocked = speed(&d);
    d.robots[0].apply_inputs(&[(name, 0, 1.0)]);
    let mut fired_at = None;
    let mut cocked_at = None;
    for i in 0..60 {
        d.step_direct(&[DriveInput::default()], DT);
        let s = speed(&d);
        if fired_at.is_none() && s != cocked {
            fired_at = Some(i);
        } else if fired_at.is_some() && cocked_at.is_none() && s == cocked {
            cocked_at = Some(i);
        }
    }
    let (f, c) = (fired_at.expect("it fires"), cocked_at.expect("it cocks"));
    assert_eq!(c - f, 34, "fired at {f}, cocking at {c}");
}
