//! Fidelity traces from our simulation, in the shape the original's live traces
//! have (`tools/oracle/ra2_python.py --sample`), so `tools/oracle/fidelity.py
//! --gameplay` can compare them.
//!
//! ```text
//! fidtrace match <arena.py> <botA> <botB> <seconds> <trace.tsv> <damage.jsonl>
//! fidtrace drive <arena.py> <bot> "0-5:1,0;5-8:0,1" <seconds> <trace.tsv>
//! fidtrace drive-at <arena.py> <bot> <x> <z> <heading> <script> <seconds> <trace.tsv>
//! ```
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
            while m.state() != MatchState::Results && step < (secs * 90.0) as usize + 2000 {
                m.step();
                step += 1;
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
        _ => {
            eprintln!("usage: fidtrace match <arena> <botA> <botB> <seconds> <trace.tsv> <damage.jsonl>\n       fidtrace drive <arena> <bot> <script> <seconds> <trace.tsv>\n       fidtrace drive-at <arena> <bot> <x> <z> <heading> <script> <seconds> <trace.tsv>");
            std::process::exit(2);
        }
    }
    Ok(())
}
