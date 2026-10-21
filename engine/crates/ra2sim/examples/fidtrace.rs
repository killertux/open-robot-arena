//! Fidelity traces from our simulation, in the shape the original's live traces
//! have (`tools/oracle/ra2_python.py --sample`), so `tools/oracle/fidelity.py
//! --gameplay` can compare them.
//!
//! ```text
//! fidtrace match <arena.py> <botA> <botB> <seconds> <trace.tsv> <damage.jsonl>
//! fidtrace drive <arena.py> <bot> "0-5:1,0;5-8:0,1" <seconds> <trace.tsv>
//! fidtrace drive-at <arena.py> <bot> <x> <z> <heading> <script> <seconds> <trace.tsv>
//! fidtrace weapons <arena.py> <botA> <botB> <plan> <seconds> <joints.tsv>
//! ```
//!
//! `weapons` spawns two robots with no AI and presses their controls as
//! `tools/oracle/capture_weapons.py` does on the original: the plan is
//! `player:control:value:seconds;...`, each step held for its seconds. Rows,
//! every step: `time player part motor rel_speed position` for every jointed
//! part, `rel_speed` the child body's angular speed relative to its parent's
//! (m/s along the axis for a slider) and `position` the hinge angle or slider
//! offset.
//!
//! `drive-at` spawns the robot at `(x, z)` facing `heading` (the trace's own
//! convention), to repeat a stretch of an original drive trace from where it
//! started.
//!
//! Trace rows: `time<TAB>[(player, (x, y, z), heading, speed, hitpoints, health)]`
//! with the original's heading convention (`atan2(fx, fz)`, radians) and speed
//! (forward component of the chassis velocity), every 6 steps (0.1 s;
//! `RA2_TRACE_EVERY=1` for every step in the drive tests).
//! Damage rows are one JSON object per *step in which a robot's damage_taken
//! grew*: `{"t","victim","attacker"(-1 env),"amount"}`.

use std::io::Write;

use glam::Vec3;
use ra2sim::{
    BotSetup, DriveScript, Duel, GameType, Match, MatchConfig, MatchState, RobotEntry, DEFAULT_STEP,
};

const EVERY: usize = 6;

/// The drive tests' sampling interval in steps: `RA2_TRACE_EVERY` (default
/// [`EVERY`]; 1 samples every step, which the coast measurements need).
fn drive_every() -> usize {
    std::env::var("RA2_TRACE_EVERY")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(EVERY)
}

fn row(t: f32, robots: &[(Vec3, f32, f32, f32, f32)]) -> String {
    let mut s = format!("{t}\t[");
    for (i, (p, h, sp, hp, health)) in robots.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&format!(
            "({i}, ({}, {}, {}), {h}, {sp}, {hp}, {health})",
            p.x, p.y, p.z
        ));
    }
    s.push_str("]\n");
    s
}

fn sample(duel: &Duel) -> Vec<(Vec3, f32, f32, f32, f32)> {
    duel.robots
        .iter()
        .map(|r| {
            let tf = r.transform(&*duel.physics);
            let f = tf.transform_vector3(r.rig.forward());
            let flat = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
            let heading = flat.x.atan2(flat.z);
            let speed = r.velocity(&*duel.physics).dot(flat);
            let hp: f32 = r.health.iter().sum();
            let total: f32 = r.rig.components.iter().map(|c| c.hitpoints).sum();
            let frac = if total > 0.0 { hp / total } else { 0.0 };
            // `FIDTRACE_UP=1`: the chassis' up in place of the health fraction
            // (its `y`: 1 upright, -1 on its back), for diagnosing flips.
            let frac = if std::env::var_os("FIDTRACE_UP").is_some() {
                tf.transform_vector3(glam::Vec3::Y).normalize_or_zero().y
            } else {
                frac
            };
            (tf.w_axis.truncate(), heading, speed, r.health[0], frac)
        })
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(String::as_str) {
        Some("match") if a.len() == 7 => {
            let secs: f32 = a[4].parse()?;
            let cfg = MatchConfig::new(
                a[1].clone(),
                GameType::Deathmatch,
                vec![
                    RobotEntry::ai(0, a[2].clone()),
                    RobotEntry::ai(1, a[3].clone()),
                ],
            )
            .with_duration(Some(secs));
            let mut m = Match::new(cfg)?;
            // `FIDTRACE_WARMUP=<n>`: n idle physics steps before the match
            // starts, a perturbation for running a fight as an ensemble.
            let warmup: usize = std::env::var("FIDTRACE_WARMUP")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            for _ in 0..warmup {
                let n = m.duel.robots.len();
                m.duel
                    .step_direct(&vec![ra2sim::sim::DriveInput::default(); n], DEFAULT_STEP);
            }
            // `FIDTRACE_HITS=<path>`: every priced hit (both directions, before
            // the `>= 10` gate), in the shape `tools/oracle/fight_report.py`
            // reads from the original's captures.
            let mut hits = match std::env::var("FIDTRACE_HITS") {
                Ok(p) => {
                    m.duel.record_impacts = true;
                    Some(std::fs::File::create(p)?)
                }
                Err(_) => None,
            };
            let mut trace = std::fs::File::create(&a[5])?;
            let mut dmg = std::fs::File::create(&a[6])?;
            writeln!(trace, "time_elapsed\t[(player, location xyz, heading, speed, hitpoints(p,0), health fraction)]")?;
            let n = m.duel.robots.len();
            let (mut taken, mut dealt) = (vec![0.0f32; n], vec![0.0f32; n]);
            let mut step = 0usize;
            // `FIDTRACE_LOG=1`: the duel's log lines (zones, hazards, parts
            // lost) on stderr as they happen, with the match clock.
            let show_log = std::env::var_os("FIDTRACE_LOG").is_some();
            let mut logged = 0usize;
            while m.state() != MatchState::Results && step < (secs * 90.0) as usize + 2000 {
                m.step();
                step += 1;
                if show_log {
                    let t = m.clock();
                    for line in m.duel.log.iter().skip(logged) {
                        eprintln!("[{t:7.2}] {line}");
                    }
                    logged = m.duel.log.len();
                }
                if let Some(f) = hits.as_mut() {
                    let t = m.clock();
                    for i in std::mem::take(&mut m.duel.impacts) {
                        let name = |r: usize, c: usize| {
                            m.duel.robots[r].rig.components[c].name.replace('"', "'")
                        };
                        writeln!(
                            f,
                            "{{\"t\": {t}, \"att\": {}, \"by\": \"{}\", \"vic\": {}, \"comp\": \"{}\", \"amount\": {}, \"speed\": {}}}",
                            i.attacker,
                            name(i.attacker, i.by),
                            i.target,
                            name(i.target, i.component),
                            i.damage,
                            i.speed
                        )?;
                    }
                }
                if m.state() == MatchState::Fight {
                    let t = m.clock();
                    #[allow(clippy::needless_range_loop)]
                    for v in 0..n {
                        let d = m.duel.damage_taken[v] - taken[v];
                        if d > 0.0 {
                            let attacker = (0..n)
                                .find(|&k| m.duel.damage_dealt[k] - dealt[k] > 0.0)
                                .map(|k| k as i64)
                                .unwrap_or(-1);
                            writeln!(
                                dmg,
                                "{{\"t\": {t}, \"victim\": {v}, \"attacker\": {attacker}, \"amount\": {d}}}"
                            )?;
                        }
                    }
                    if step.is_multiple_of(EVERY) {
                        write!(trace, "{}", row(t, &sample(&m.duel)))?;
                    }
                }
                taken[..n].copy_from_slice(&m.duel.damage_taken[..n]);
                dealt[..n].copy_from_slice(&m.duel.damage_dealt[..n]);
            }
            let r = m.report();
            eprintln!(
                "fight {:.2}s, reason {:?}, winners {:?}, points {:?}",
                r.duration,
                r.reason,
                r.winners,
                r.robots.iter().map(|x| x.points).collect::<Vec<_>>()
            );
        }
        Some(mode @ ("drive" | "drive-at"))
            if a.len() == 6 || (mode == "drive-at" && a.len() == 9) =>
        {
            let at = mode == "drive-at";
            let (pose, rest) = if at {
                let (x, z, h): (f32, f32, f32) = (a[3].parse()?, a[4].parse()?, a[5].parse()?);
                // The spawn yaw turns the robot's front from `heading + PI`.
                (
                    Some(([x, z], (h - std::f32::consts::PI).to_degrees())),
                    &a[6..],
                )
            } else {
                (None, &a[3..])
            };
            let secs: f32 = rest[1].parse()?;
            let script = DriveScript::parse(&rest[0]);
            let mut duel = Duel::new(
                &a[1],
                &[BotSetup {
                    path: a[2].clone(),
                    yaw_deg: pose.map(|p| p.1),
                    spawn: pose.map(|p| p.0),
                }],
            )?;
            let mut trace = std::fs::File::create(&rest[2])?;
            writeln!(trace, "time_elapsed\t[(player, location xyz, heading, speed, hitpoints(p,0), health fraction)]")?;
            let steps = (secs / DEFAULT_STEP) as usize;
            let every = drive_every();
            for s in 0..steps {
                let t = s as f32 * DEFAULT_STEP;
                let input = script.at(t);
                duel.step_direct(&[input], DEFAULT_STEP);
                if s.is_multiple_of(every) {
                    write!(trace, "{}", row(t, &sample(&duel)))?;
                }
            }
        }
        Some("weapons") if a.len() == 7 => {
            let secs: f32 = a[5].parse()?;
            let plan: Vec<(usize, String, f32, f32)> = a[4]
                .split(';')
                .filter(|s| !s.is_empty())
                .map(|s| {
                    let f: Vec<&str> = s.split(':').collect();
                    (
                        f[0].parse().unwrap_or(0),
                        f[1].to_string(),
                        f[2].parse().unwrap_or(0.0),
                        f[3].parse().unwrap_or(0.0),
                    )
                })
                .collect();
            let mut duel = Duel::new(
                &a[1],
                &[
                    BotSetup {
                        path: a[2].clone(),
                        yaw_deg: None,
                        spawn: None,
                    },
                    BotSetup {
                        path: a[3].clone(),
                        yaw_deg: None,
                        spawn: None,
                    },
                ],
            )?;
            let mut out = std::fs::File::create(&a[6])?;
            writeln!(out, "time\tplayer\tpart\tmotor\trel_speed\tposition")?;
            let steps = (secs / DEFAULT_STEP) as usize;
            let (mut k, mut until) = (0usize, 0.0f32);
            for s in 0..steps {
                let t = s as f32 * DEFAULT_STEP;
                while k < plan.len() && t >= until {
                    let (p, ref name, value, hold) = plan[k];
                    if p < duel.robots.len() {
                        let r = &mut duel.robots[p];
                        r.control.input(&r.rig.wiring, name, 0, value);
                    }
                    until = t + hold;
                    k += 1;
                }
                duel.step_direct(&[ra2sim::sim::DriveInput::default(); 2], DEFAULT_STEP);
                for (p, r) in duel.robots.iter().enumerate() {
                    for (q, part) in r.rig.parts.iter().enumerate() {
                        let (Some(info), Some(parent), Some(joint)) =
                            (part.joint, part.parent, r.parts[q].joint)
                        else {
                            continue;
                        };
                        let (Some(sa), Some(sb)) = (
                            duel.physics.state(r.parts[parent].body),
                            duel.physics.state(r.parts[q].body),
                        ) else {
                            continue;
                        };
                        let axis = (sa.rotation
                            * r.rig.components[r.rig.parts[parent].root]
                                .local
                                .inverse()
                                .transform_vector3(info.axis))
                        .normalize_or_zero();
                        let (rel, pos) = if info.slider {
                            (
                                (sb.linear_velocity - sa.linear_velocity).dot(axis),
                                duel.physics.slider_offset(joint),
                            )
                        } else {
                            (
                                (sb.angular_velocity - sa.angular_velocity).dot(axis),
                                duel.physics.hinge_angle(joint),
                            )
                        };
                        writeln!(
                            out,
                            "{:.4}\t{p}\t{q}\t{}\t{rel:.3}\t{pos:.4}",
                            t + DEFAULT_STEP,
                            r.rig.components[info.motor].name
                        )?;
                    }
                }
            }
        }
        _ => {
            eprintln!("usage: fidtrace match <arena> <botA> <botB> <seconds> <trace.tsv> <damage.jsonl>\n       fidtrace drive <arena> <bot> <script> <seconds> <trace.tsv>\n       fidtrace drive-at <arena> <bot> <x> <z> <heading> <script> <seconds> <trace.tsv>");
            std::process::exit(2);
        }
    }
    Ok(())
}
