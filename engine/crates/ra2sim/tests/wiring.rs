//! The control wiring: a stock robot's motors, servos, burst devices and wheels
//! move only when the bot's own control board says so (`ra2sim::controls`).
//!
//! The expectations are the recovered ones: a motor's speed is the clamped sum
//! of its active inputs (`0x4ed370`), a zero command releases it (`0x481450`),
//! a burst device waits cocked until `Fire` (tutorial 3), and `Input(name, slot,
//! value)` reaches the controls of that name (`0x4012f0` -> `0x508ce0`).

use std::path::{Path, PathBuf};

use glam::Vec3;
use ra2sim::controls::{dik, KeySet};
use ra2sim::rig::JointUse;
use ra2sim::{BotSetup, DriveInput, Duel};

const DT: f32 = 1.0 / 60.0;

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
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

/// One stock bot alone in the box arena, on level floor, front toward `+X`.
fn duel(game: &Path, bot: &str) -> Duel {
    duel_settled(game, bot, 90)
}

/// As [`duel`], with `frames` of settling. A robot that is not driven has free
/// wheels (a zero command releases a motor), and a nose-heavy one such as
/// `Team5/Bot1` pitches over on them if left alone, so the driving test drives
/// from the drop instead.
fn duel_settled(game: &Path, bot: &str, frames: usize) -> Duel {
    let arena = game.join("Arenas/box.py").to_string_lossy().to_string();
    let setup = BotSetup {
        path: game.join(bot).to_string_lossy().to_string(),
        yaw_deg: Some(270.0),
        spawn: Some([0.0, 0.0]),
    };
    let mut d = Duel::new(&arena, std::slice::from_ref(&setup)).expect("a duel");
    // Settle on the floor first.
    for _ in 0..frames {
        d.step_direct(&[DriveInput::default()], DT);
    }
    d
}

fn step(d: &mut Duel, input: DriveInput, seconds: f32) {
    for _ in 0..(seconds / DT).round() as usize {
        d.step_direct(&[input], DT);
    }
}

/// How fast part `p` turns relative to the chassis, rad/s.
fn relative_spin(d: &Duel, p: usize) -> f32 {
    let r = &d.robots[0];
    let w = |part: usize| {
        d.physics
            .state(r.parts[part].body)
            .map(|s| s.angular_velocity)
            .unwrap_or(Vec3::ZERO)
    };
    (w(p) - w(0)).length()
}

fn joint_part(d: &Duel, kind: JointUse, motor_class: &str) -> usize {
    let r = &d.robots[0];
    (0..r.rig.parts.len())
        .find(|&p| {
            r.rig.parts[p].joint.is_some_and(|j| {
                j.use_kind == kind && r.rig.components[j.motor].class == motor_class
            })
        })
        .unwrap_or_else(|| panic!("no {motor_class} joint of kind {kind:?}"))
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

/// `Team10/Bot0` ("Berserker") has a `Spin` switch wired to its spinner: with
/// the switch off the spinner is still; the AI's `Input("Spin", 0, 1)` or a tap
/// of its key (Space) spins it up; a second `Input("Spin", 0, 1)` / a second tap
/// turns it off.
#[test]
fn a_spinner_spins_only_while_its_switch_is_on() {
    let game = game!();
    let _scripts = ra2script::lock();
    let mut d = duel(&game, "AI/Team10/Bot0.bot");
    let p = joint_part(&d, JointUse::Spun, "SpinMotor");
    let rate = d.robots[0].rig.parts[p].joint.unwrap().rate;

    step(&mut d, DriveInput::default(), 1.0);
    assert!(
        relative_spin(&d, p) < 0.5,
        "the spinner turns with nothing wired on: {}",
        relative_spin(&d, p)
    );

    // The AI's Input call.
    d.robots[0].apply_inputs(&[("Spin".into(), 0, 1.0)]);
    step(&mut d, DriveInput::default(), 2.0);
    // Clearly spinning (not its top speed: the load cap and the 72 kg bar's
    // inertia, every rigid body's own carried to the centre, docs/29 section
    // 12f, make it a slow starter: 13.5 rad/s after 2 s).
    let on = relative_spin(&d, p);
    assert!(on > rate * 0.25, "the spinner reached {on} of {rate} rad/s");

    // A release leaves the switch on; the next press turns it off (live,
    // docs/29 section 12c).
    d.robots[0].apply_inputs(&[("Spin".into(), 0, 0.0)]);
    assert_eq!(
        d.robots[0].control.command(&d.robots[0].rig.wiring, 1),
        Some(100.0)
    );
    d.robots[0].apply_inputs(&[("Spin".into(), 0, 1.0)]);
    assert_eq!(
        d.robots[0].control.command(&d.robots[0].rig.wiring, 1),
        Some(0.0)
    );

    // The human's key: a tap turns it on, a second tap turns it off again.
    let mut keys = KeySet::default();
    keys.set(dik::SPACE, true);
    let tap = DriveInput {
        keys,
        ..Default::default()
    };
    step(&mut d, tap, 0.1);
    step(&mut d, DriveInput::default(), 0.1);
    assert_eq!(
        d.robots[0].control.command(&d.robots[0].rig.wiring, 1),
        Some(100.0),
        "a tap of Space latches the spin switch on"
    );
    step(&mut d, tap, 0.1);
    step(&mut d, DriveInput::default(), 0.1);
    assert_eq!(
        d.robots[0].control.command(&d.robots[0].rig.wiring, 1),
        Some(0.0),
        "a second tap latches it off"
    );
}

/// `Team6/Bot2` has two servo arms on the `Lefthook`/`Righthook` analog controls
/// (the controls `AI/Other.py` drives): still without input, moving with it,
/// and held where they are when it returns to zero.
#[test]
fn a_servo_arm_moves_with_its_input_and_holds_without_it() {
    let game = game!();
    let _scripts = ra2script::lock();
    let mut d = duel(&game, "AI/Team6/Bot2.bot");
    let p = joint_part(&d, JointUse::Spun, "ServoMotor");
    let joint = d.robots[0].parts[p].joint.unwrap();
    let angle = |d: &Duel| d.physics.hinge_angle(joint);
    let wiring = d.robots[0].rig.wiring.clone();
    let name = wiring
        .wires
        .iter()
        .find(|w| w.component == d.robots[0].rig.parts[p].joint.unwrap().motor)
        .map(|w| wiring.controls[w.controller].name.clone())
        .expect("the servo is wired");

    let start = angle(&d);
    step(&mut d, DriveInput::default(), 1.0);
    assert!(
        (angle(&d) - start).abs() < 0.05,
        "the arm moved with nothing wired on"
    );

    d.robots[0].apply_inputs(&[(name.clone(), 0, -100.0)]);
    step(&mut d, DriveInput::default(), 0.6);
    let moved = angle(&d);
    assert!(
        (moved - start).abs() > 0.2,
        "the arm did not follow its input: {start} -> {moved}"
    );

    d.robots[0].apply_inputs(&[(name, 0, 0.0)]);
    step(&mut d, DriveInput::default(), 0.3);
    let held = angle(&d);
    step(&mut d, DriveInput::default(), 1.0);
    assert!(
        (angle(&d) - held).abs() < 0.05,
        "a servo released at zero drifted: {held} -> {}",
        angle(&d)
    );
}

/// A burst motor (`Team5/Bot0`) waits cocked at its start, swings to its end
/// when `Fire` arrives, and is cocked again afterwards.
#[test]
fn a_burst_motor_fires_on_its_trigger_and_cocks_again() {
    let game = game!();
    let _scripts = ra2script::lock();
    let mut d = duel(&game, "AI/Team5/Bot0.bot");
    let p = joint_part(&d, JointUse::Burst, "BurstMotor");
    let info = d.robots[0].rig.parts[p].joint.unwrap();
    let joint = d.robots[0].parts[p].joint.unwrap();
    let travel = if info.limits[1].abs() >= info.limits[0].abs() {
        info.limits[1]
    } else {
        info.limits[0]
    };
    let name = {
        let w = &d.robots[0].rig.wiring;
        let wire = w
            .wires
            .iter()
            .find(|x| x.component == info.motor)
            .expect("wired");
        w.controls[wire.controller].name.clone()
    };

    step(&mut d, DriveInput::default(), 1.0);
    assert!(
        d.physics.hinge_angle(joint).abs() < 0.1,
        "a burst motor left its start with no trigger"
    );

    d.robots[0].apply_inputs(&[(name, 0, 1.0)]);
    let mut best = 0.0f32;
    for _ in 0..(1.5 / DT) as usize {
        d.step_direct(&[DriveInput::default()], DT);
        best = best.max(d.physics.hinge_angle(joint).abs());
    }
    assert!(
        best > travel.abs() * 0.8,
        "the burst motor reached {best} of {} rad",
        travel.abs()
    );

    // Slowly cocked again.
    step(&mut d, DriveInput::default(), 12.0);
    assert!(
        d.physics.hinge_angle(joint).abs() < 0.2,
        "the burst motor did not cock again: {}",
        d.physics.hinge_angle(joint)
    );
}

/// A burst piston (`Team13/Bot0`) extends on `Fire`, with the space bar as the
/// human's key.
#[test]
fn a_burst_piston_extends_when_its_key_is_pressed() {
    let game = game!();
    let _scripts = ra2script::lock();
    let mut d = duel(&game, "AI/Team13/Bot0.bot");
    let p = joint_part(&d, JointUse::Burst, "BurstPiston");
    let joint = d.robots[0].parts[p].joint.unwrap();
    let limits = d.robots[0].rig.parts[p].joint.unwrap().limits;
    let reach = if limits[1].abs() >= limits[0].abs() {
        limits[1]
    } else {
        limits[0]
    };

    step(&mut d, DriveInput::default(), 1.0);
    assert!(d.physics.slider_offset(joint).abs() < 0.02, "it crept");

    let mut keys = KeySet::default();
    keys.set(dik::SPACE, true);
    let fire = DriveInput {
        keys,
        ..Default::default()
    };
    let mut best = 0.0f32;
    for _ in 0..(1.0 / DT) as usize {
        d.step_direct(&[fire], DT);
        best = best.max(d.physics.slider_offset(joint) * reach.signum());
    }
    assert!(
        best > reach.abs() * 0.8,
        "the piston reached {best} of {reach} m after Space"
    );
}

/// The scan code the bot binds to slot `slot` of its control called `name`.
fn bound_key(d: &Duel, name: &str, slot: usize) -> i32 {
    let w = &d.robots[0].rig.wiring;
    w.controls
        .iter()
        .find(|c| c.name == name)
        .and_then(|c| c.keys.get(slot).copied())
        .filter(|&k| k != 0)
        .unwrap_or_else(|| panic!("no key bound to {name} slot {slot}"))
}

/// Forward and turning go through the bindings: for stock bots of both
/// headings, the key bound to `Forward` moves the robot toward its front and
/// the one bound to the second `LeftRight` slot turns it clockwise seen from
/// above (to its right) — a positive rate about `+Y` in the game's left-handed
/// frame. `Team4/Bot1` binds the numeric keypad, not `WASD`.
#[test]
fn driving_and_turning_go_through_the_bindings() {
    let game = game!();
    let _scripts = ra2script::lock();
    for bot in [
        "AI/Team2/Bot0.bot",
        "AI/Team4/Bot1.bot",
        "AI/Team5/Bot1.bot",
        "AI/Team1/Bot0.bot", // heading PI
        "AI/Team11/Bot2.bot",
    ] {
        let mut d = duel_settled(&game, bot, 0);
        let front = {
            let r = &d.robots[0];
            r.transform(&*d.physics)
                .transform_vector3(r.rig.forward())
                .normalize()
        };
        let start = d.robots[0].position(&*d.physics);
        let mut keys = KeySet::default();
        keys.set(bound_key(&d, "Forward", 0), true);
        step(
            &mut d,
            DriveInput {
                keys,
                ..Default::default()
            },
            1.5,
        );
        let moved = d.robots[0].position(&*d.physics) - start;
        assert!(
            moved.dot(front) > 1.0,
            "{bot}: its forward key moved it {:.2} m along its front ({moved:?})",
            moved.dot(front)
        );

        // Turning in place with D: to the robot's right, which in the game's
        // left-handed frame (+X right, +Y up, +Z forward) is clockwise from
        // above and a *positive* rate about +Y. Live, `SuperAI.Turn(+100)`
        // (the D side of `LeftRight`) spins the Razor at +12.7 rad/s
        // (`magic-razor__live__000`); see docs/33 §5.
        let mut d = duel_settled(&game, bot, 0);
        let mut keys = KeySet::default();
        keys.set(bound_key(&d, "LeftRight", 1), true);
        step(
            &mut d,
            DriveInput {
                keys,
                ..Default::default()
            },
            1.0,
        );
        let yaw = d
            .physics
            .state(d.robots[0].body())
            .map(|s| s.angular_velocity.y)
            .unwrap_or(0.0);
        assert!(
            yaw > 0.2,
            "{bot}: its right key turned it at {yaw} rad/s about +Y"
        );
    }
}

/// A robot nothing is wired to the keys of does not drive.
#[test]
fn an_idle_robot_stays_put() {
    let game = game!();
    let _scripts = ra2script::lock();
    let mut d = duel(&game, "AI/Team4/Bot1.bot");
    let start = d.robots[0].position(&*d.physics);
    step(&mut d, DriveInput::default(), 2.0);
    let moved = (d.robots[0].position(&*d.physics) - start).length();
    assert!(moved < 0.3, "an unpowered robot drifted {moved} m");
}
