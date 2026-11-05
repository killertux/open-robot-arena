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
            // `FIDTRACE_COM=1`: the chassis' centre of mass in place of the bot
            // origin, which is what the original's `getLocation` reports
            // (docs/29 section 12g).
            let at = if std::env::var_os("FIDTRACE_COM").is_some() {
                duel.physics
                    .center_of_mass(r.body())
                    .unwrap_or_else(|| tf.w_axis.truncate())
            } else {
                tf.w_axis.truncate()
            };
            (at, heading, speed, r.health[0], frac)
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
            // starts, a perturbation for running a fight as an ensemble. The
            // match holds the robots until the bell (docs/29 section 12v), so
            // they are let go for the warmup and held again where it leaves
            // them; with no warmup they fall at the bell, as live.
            let warmup: usize = std::env::var("FIDTRACE_WARMUP")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            if warmup > 0 {
                m.duel.hold_robots(false);
                for _ in 0..warmup {
                    let n = m.duel.robots.len();
                    m.duel
                        .step_direct(&vec![ra2sim::sim::DriveInput::default(); n], DEFAULT_STEP);
                }
                m.duel.hold_robots(true);
            }
            // `FIDTRACE_JITTER=<n>`: shift every body of each robot by a few
            // tens of micrometres (n times 20 um along a different direction
            // per robot), another perturbation for an ensemble. Unlike the
            // warmup the robots still fall at the bell as live (docs/29
            // section 12ai: a warmup's robots start at rest on the floor, which
            // leaves out the landing and the wide load's rock after it).
            if let Some(n) = std::env::var("FIDTRACE_JITTER")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
            {
                for (i, r) in m.duel.robots.iter().enumerate() {
                    let a = (i as f32 + 1.0) * 2.399_963 * (n + 1.0);
                    let d = Vec3::new(a.cos(), 0.0, a.sin()) * (n * 2.0e-5);
                    for p in r.parts.iter() {
                        if let Some(s) = m.duel.physics.state(p.body) {
                            m.duel
                                .physics
                                .set_transform(p.body, s.position + d, s.rotation);
                        }
                    }
                }
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
            // `FIDTRACE_WHEELS=<path>`: every fight step, each robot's chassis
            // up (`y`) and its wheel records (thrust, touching, normal), as
            // the live `G` rows of `frida/ra2_wheelnormal.js`.
            let mut wheels = match std::env::var("FIDTRACE_WHEELS") {
                Ok(p) => Some(std::fs::File::create(p)?),
                Err(_) => None,
            };
            // `FIDTRACE_BODY=<path>`: every step (the whole match, the clock
            // included) each robot's every part body: `step t robot part com(3)
            // rot(x y z w) v(3) w(3)`, for the step-resolved landing comparison
            // with the live `B` rows of `ra2_fightcontacts.js` (docs/29 12aj).
            let mut bodies = match std::env::var("FIDTRACE_BODY") {
                Ok(p) => Some(std::fs::File::create(p)?),
                Err(_) => None,
            };
            // `FIDTRACE_FIRE=<path>`: every change of a burst device's cycle
            // (cocked / firing / cocking) as `t robot part slider phase`, the
            // match clock first, and the chassis' up.y, to set ALARM's hammer
            // strokes against the live `F` rows (docs/29 section 12ap).
            let mut fires = match std::env::var("FIDTRACE_FIRE") {
                Ok(p) => Some(std::fs::File::create(p)?),
                Err(_) => None,
            };
            let mut last_zone: Vec<usize> = Vec::new();
            let mut last_phase: Vec<Vec<(usize, bool, u8)>> = vec![Vec::new(); m.duel.robots.len()];
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
            // `RA2_TRACE_EVERY=<n>` samples every n steps (default `EVERY`).
            let every = drive_every();
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
                    let pieces = std::mem::take(&mut m.duel.impact_pieces);
                    for i in std::mem::take(&mut m.duel.impacts) {
                        let name = |r: usize, c: usize| match m.duel.robots.get(r) {
                            Some(robot) => robot.rig.components[c].name.replace('"', "'"),
                            None => "arena".to_string(),
                        };
                        let att = if i.attacker == ra2sim::combat::NO_ATTACKER {
                            -1
                        } else {
                            i.attacker as i64
                        };
                        // The arena collidable struck (the contact at this point).
                        let piece = pieces
                            .iter()
                            .find(|(p, r, _)| *p == i.point && *r == i.target)
                            .map(|(_, _, n)| n.clone())
                            .unwrap_or_default();
                        // The point in a robot's chassis frame, about its centre of mass (the
                        // live `S` rows' frame), to set the hits against the original's by place.
                        let local = |r: i64| -> [f32; 3] {
                            let Some(robot) =
                                usize::try_from(r).ok().and_then(|r| m.duel.robots.get(r))
                            else {
                                return [0.0; 3];
                            };
                            let ci = (0..robot.parts.len())
                                .find(|&p| robot.rig.parts[p].parent.is_none())
                                .unwrap_or(0);
                            let body = robot.parts[ci].body;
                            match (
                                m.duel.physics.state(body),
                                m.duel.physics.center_of_mass(body),
                            ) {
                                (Some(st), Some(c)) => {
                                    (st.rotation.inverse() * (i.point - c)).to_array()
                                }
                                _ => [0.0; 3],
                            }
                        };
                        let (la, lv) = (local(att), local(i.target as i64));
                        writeln!(
                            f,
                            "{{\"t\": {t}, \"att\": {}, \"by\": \"{}\", \"vic\": {}, \"comp\": \"{}\", \"piece\": \"{piece}\", \"amount\": {}, \"speed\": {}, \"p\": [{}, {}, {}], \"n\": [{}, {}, {}], \"la\": [{}, {}, {}], \"lv\": [{}, {}, {}]}}",
                            att,
                            name(i.attacker, i.by),
                            i.target,
                            name(i.target, i.component),
                            i.damage,
                            i.speed,
                            i.point.x,
                            i.point.y,
                            i.point.z,
                            i.normal.x,
                            i.normal.y,
                            i.normal.z,
                            la[0],
                            la[1],
                            la[2],
                            lv[0],
                            lv[1],
                            lv[2]
                        )?;
                    }
                }
                if let Some(f) = fires.as_mut() {
                    let t = m.clock();
                    let zc = m.duel.zone_counts();
                    if zc != last_zone {
                        writeln!(
                            f,
                            "{t:.4} Z {} {}",
                            zc.first().copied().unwrap_or(0),
                            zc.get(1).copied().unwrap_or(0)
                        )?;
                        last_zone = zc;
                    }
                    for (ri, r) in m.duel.robots.iter().enumerate() {
                        let now = r.burst_phases();
                        for (k, n) in now.iter().enumerate() {
                            if last_phase[ri].get(k).map(|l| l.2) != Some(n.2) {
                                let up = r
                                    .transform(&*m.duel.physics)
                                    .transform_vector3(glam::Vec3::Y)
                                    .normalize_or_zero()
                                    .y;
                                writeln!(f, "{t:.4} {ri} {} {} {} {up:.3}", n.0, n.1 as u8, n.2)?;
                            }
                        }
                        last_phase[ri] = now;
                    }
                }
                if let Some(f) = bodies.as_mut() {
                    let t = m.clock();
                    for (ri, r) in m.duel.robots.iter().enumerate() {
                        for (pi, part) in r.parts.iter().enumerate() {
                            if let (Some(st), Some(c)) = (
                                m.duel.physics.state(part.body),
                                m.duel.physics.center_of_mass(part.body),
                            ) {
                                writeln!(
                                    f,
                                    "{step} {t:.4} {ri} {pi} {:.5} {:.5} {:.5} {:.5} {:.5} {:.5} {:.5} {:.4} {:.4} {:.4} {:.4} {:.4} {:.4}",
                                    c.x, c.y, c.z, st.rotation.x, st.rotation.y, st.rotation.z,
                                    st.rotation.w, st.linear_velocity.x, st.linear_velocity.y,
                                    st.linear_velocity.z, st.angular_velocity.x,
                                    st.angular_velocity.y, st.angular_velocity.z
                                )?;
                            }
                        }
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
                    if step.is_multiple_of(every) {
                        write!(trace, "{}", row(t, &sample(&m.duel)))?;
                    }
                    if let Some(f) = wheels.as_mut() {
                        for (i, r) in m.duel.robots.iter().enumerate() {
                            let up = r
                                .transform(&*m.duel.physics)
                                .transform_vector3(Vec3::Y)
                                .normalize_or_zero();
                            let ws: Vec<String> = r
                                .magic
                                .iter()
                                .flat_map(|g| g.wheels.iter())
                                .map(|w| {
                                    format!(
                                        "[{}, {}, [{}, {}, {}]]",
                                        w.thrust,
                                        u8::from(w.touching),
                                        w.normal.x,
                                        w.normal.y,
                                        w.normal.z
                                    )
                                })
                                .collect();
                            writeln!(
                                f,
                                "{{\"t\": {t}, \"r\": {i}, \"up\": {}, \"w\": [{}]}}",
                                up.y,
                                ws.join(", ")
                            )?;
                        }
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
        // `fidtrace replay <arena.py> <botA> <botB> <state> <steps> <hits.jsonl>`: both
        // robots set to a state captured live (`tools/oracle/stroke_states.py`
        // turns the `B` rows of `frida/ra2_firststroke.js` into `<state>`: one
        // line per body, `robot part com(3) rot(x y z w) v(3) w(3)`), then
        // `<steps>` physics steps with the drive `FIDTRACE_DRIVE=throttle0,steer0,
        // throttle1,steer1` held and the priced hits written as `FIDTRACE_HITS`
        // does (docs/29 section 12as). Tells the contact stage (shapes, joints,
        // impulses) from the approach: the live fight's own pre-contact state in,
        // the hits out.
        Some("replay") if a.len() == 7 => {
            let cfg = MatchConfig::new(
                a[1].clone(),
                GameType::Deathmatch,
                vec![
                    RobotEntry::ai(0, a[2].clone()),
                    RobotEntry::ai(1, a[3].clone()),
                ],
            )
            .with_duration(Some(30.0));
            let mut m = Match::new(cfg)?;
            // `FIDTRACE_AI=1`: the shipped brains keep driving (and firing) from the
            // state, as the match runs from the bell; the clock restarts at the state.
            let ai = std::env::var_os("FIDTRACE_AI").is_some();
            if ai {
                while m.state() != MatchState::Fight {
                    m.step();
                }
                m.step();
                m.step();
            } else {
                m.duel.hold_robots(false);
            }
            m.duel.record_impacts = true;
            for line in std::fs::read_to_string(&a[4])?.lines() {
                let v: Vec<f32> = line
                    .split_whitespace()
                    .filter_map(|t| t.parse().ok())
                    .collect();
                if v.len() < 15 {
                    continue;
                }
                let (ri, pi) = (v[0] as usize, v[1] as usize);
                let Some(body) = m
                    .duel
                    .robots
                    .get(ri)
                    .and_then(|r| r.parts.get(pi))
                    .map(|p| p.body)
                else {
                    continue;
                };
                let (Some(st), Some(c)) = (
                    m.duel.physics.state(body),
                    m.duel.physics.center_of_mass(body),
                ) else {
                    continue;
                };
                let local = st.rotation.inverse() * (c - st.position);
                let q = glam::Quat::from_xyzw(v[5], v[6], v[7], v[8]).normalize();
                let com = Vec3::new(v[2], v[3], v[4]);
                m.duel.physics.set_transform(body, com - q * local, q);
                m.duel.physics.set_velocity(
                    body,
                    Vec3::new(v[9], v[10], v[11]),
                    Vec3::new(v[12], v[13], v[14]),
                );
            }
            let steps: usize = a[5].parse()?;
            let drive: Vec<f32> = std::env::var("FIDTRACE_DRIVE")
                .unwrap_or_default()
                .split(',')
                .filter_map(|t| t.parse().ok())
                .collect();
            let input = |i: usize| {
                ra2sim::DriveInput::new(
                    drive.get(2 * i).copied().unwrap_or(0.0),
                    drive.get(2 * i + 1).copied().unwrap_or(0.0),
                )
            };
            let inputs = vec![input(0), input(1)];
            let mut bodies = std::env::var("FIDTRACE_BODY")
                .ok()
                .map(std::fs::File::create)
                .transpose()?;
            // `FIDTRACE_PRESS=robot:control:value:seconds;...` presses a control
            // (`weapons`' names, e.g. `Fire`) that long after the state.
            let press: Vec<(usize, String, f32, f32)> = std::env::var("FIDTRACE_PRESS")
                .unwrap_or_default()
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
            let mut f = std::fs::File::create(&a[6])?;
            for step in 0..steps {
                let now = step as f32 * DEFAULT_STEP;
                for (p, name, value, at) in &press {
                    if now >= *at && now < *at + DEFAULT_STEP {
                        if let Some(r) = m.duel.robots.get_mut(*p) {
                            r.control.input(&r.rig.wiring, name, 0, *value);
                        }
                    }
                }
                if ai {
                    m.step();
                } else {
                    m.duel.step_direct(&inputs, DEFAULT_STEP);
                }
                let t = (step + 1) as f32 * DEFAULT_STEP;
                if let Some(bf) = bodies.as_mut() {
                    for (ri, r) in m.duel.robots.iter().enumerate() {
                        for (pi, part) in r.parts.iter().enumerate() {
                            if let (Some(st), Some(c)) = (
                                m.duel.physics.state(part.body),
                                m.duel.physics.center_of_mass(part.body),
                            ) {
                                writeln!(
                                    bf,
                                    "{step} {t:.4} {ri} {pi} {:.5} {:.5} {:.5} {:.5} {:.5} {:.5} {:.5} {:.4} {:.4} {:.4} {:.4} {:.4} {:.4}",
                                    c.x, c.y, c.z, st.rotation.x, st.rotation.y, st.rotation.z,
                                    st.rotation.w, st.linear_velocity.x, st.linear_velocity.y,
                                    st.linear_velocity.z, st.angular_velocity.x,
                                    st.angular_velocity.y, st.angular_velocity.z
                                )?;
                            }
                        }
                    }
                }
                let pieces = std::mem::take(&mut m.duel.impact_pieces);
                for i in std::mem::take(&mut m.duel.impacts) {
                    let name = |r: usize, c: usize| match m.duel.robots.get(r) {
                        Some(robot) => robot.rig.components[c].name.replace('"', "'"),
                        None => "arena".to_string(),
                    };
                    let att = if i.attacker == ra2sim::combat::NO_ATTACKER {
                        -1
                    } else {
                        i.attacker as i64
                    };
                    let piece = pieces
                        .iter()
                        .find(|(p, r, _)| *p == i.point && *r == i.target)
                        .map(|(_, _, n)| n.clone())
                        .unwrap_or_default();
                    writeln!(
                        f,
                        "{{\"t\": {t}, \"att\": {}, \"by\": \"{}\", \"vic\": {}, \"comp\": \"{}\", \"piece\": \"{piece}\", \"amount\": {}, \"speed\": {}, \"p\": [{}, {}, {}], \"n\": [{}, {}, {}]}}",
                        att,
                        name(i.attacker, i.by),
                        i.target,
                        name(i.target, i.component),
                        i.damage,
                        i.speed,
                        i.point.x,
                        i.point.y,
                        i.point.z,
                        i.normal.x,
                        i.normal.y,
                        i.normal.z
                    )?;
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
