//! The impact sounds and the crowd against the original's own call log of five Compressor fights
//! (`local/oracle/original/audio-impacts-compressor-scout-alarm__live__00N.impacts.tsv`,
//! `tools/oracle/frida/ra2_impactsound.js` + `tools/oracle/impactsound.py --tsv`): every priced
//! collision event that reached the victim component's hit handler (`FUN_0045f3a0`) next to the
//! sound that was created for it, and the crowd's excitement steps, mood changes and shouts.
//! Every test skips when the captures are absent; none of the data is committed.

use std::path::PathBuf;

use ra2_app::crowd::{Crowd, CrowdEvent};
use ra2_audio::rules::impact_volume_hdb;

#[derive(Clone, Debug)]
struct Row {
    /// Wall seconds from the bell.
    t: f32,
    /// The crowd's own seconds from the bell (its clock runs on the capped frame time).
    g: f32,
    kind: String,
    f: Vec<String>,
}

fn original_dir() -> Option<PathBuf> {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join("local/oracle/original");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn load(n: usize) -> Option<Vec<Row>> {
    let name = format!("audio-impacts-compressor-scout-alarm__live__{n:03}.impacts.tsv");
    let text = std::fs::read_to_string(original_dir()?.join(name)).ok()?;
    Some(
        text.lines()
            .skip(1)
            .map(|line| {
                let f: Vec<&str> = line.split('\t').collect();
                Row {
                    t: f[0].parse().unwrap(),
                    g: f[6].parse().unwrap(),
                    kind: f[1].to_string(),
                    f: f[2..6].iter().map(|s| s.to_string()).collect(),
                }
            })
            .collect(),
    )
}

/// `FUN_0045f3a0` plays the struck component's damage sound for every priced event whose damage is
/// above 4 and for no other: 211 of 211 events above 4 in the five fights made a sound, at the
/// volume of the rule (within a hundredth of a decibel's rounding), and `impact_volume_hdb` is that
/// rule.
#[test]
fn every_priced_event_above_four_makes_one_sound_at_the_rule_volume() {
    let mut above = 0;
    for n in 0..5 {
        let Some(rows) = load(n) else { return };
        for r in rows
            .iter()
            .filter(|r| r.kind == "IMP" && r.t >= 0.0 && r.t <= 60.0)
        {
            let damage: f32 = r.f[0].parse().unwrap();
            match impact_volume_hdb(damage) {
                None => assert!(r.f[1].is_empty(), "{r:?}: {damage} is silent"),
                Some(v) => {
                    above += 1;
                    let live: i32 = r.f[2].parse().unwrap_or_else(|_| panic!("{r:?}: no sound"));
                    assert!((live - v).abs() <= 2, "{r:?}: live {live}, rule {v}");
                }
            }
        }
    }
    assert_eq!(above, 211);
}

/// Feed the crowd the fight's scored damage and its 20-point bursts (the robot-destroyed and
/// component-off calls of `FUN_004d53e0`) at the logged times (events of one frame, 50 ms, are applied together), tick it from its creation (11.0 s
/// before its first mood change), and compare its mood changes and taunts with the logged ones.
#[test]
fn the_crowd_replays_the_logged_mood_changes_and_taunts() {
    let (mut moods, mut shouts) = (0, 0);
    for n in 0..5 {
        let Some(rows) = load(n) else { return };
        let first = rows.iter().find(|r| r.kind == "MOOD").unwrap().g;
        let mut crowd = Crowd::new();
        let dt: f32 = std::env::var("CROWD_DT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.0 / 90.0);
        let mut started = false;
        let mut t = first - 11.0;
        let mut feed: Vec<(f32, &Row)> = rows
            .iter()
            .filter(|r| {
                (r.kind == "DMG" && r.f[0].parse::<f32>().unwrap() > 50.0)
                    || (r.kind == "EX" && r.f[1] == "1")
            })
            .map(|r| (r.g, r))
            .collect();
        feed.reverse();
        let mut ours_mood: Vec<(f32, u8)> = Vec::new();
        let mut ours_shout: Vec<f32> = Vec::new();
        let mut note = |t: f32, events: Vec<CrowdEvent>| {
            for e in events {
                match e {
                    CrowdEvent::Loop { mood, on: true } => ours_mood.push((t, mood)),
                    CrowdEvent::OneShot(p)
                        if p.ends_with("boo.wav")
                            || p.ends_with("comeon.wav")
                            || p.ends_with("youmakemesick.wav") =>
                    {
                        ours_shout.push(t)
                    }
                    _ => {}
                }
            }
        };
        let end = rows.iter().map(|r| r.g).fold(0.0, f32::max) - 1.0;
        while t < end {
            if !started && t >= 0.0 {
                started = true;
                let events = crowd.start();
                note(t, events);
            }
            while feed.last().is_some_and(|(ft, _)| *ft <= t + 0.05) {
                let (_, r) = feed.pop().unwrap();
                let events = if r.kind == "DMG" {
                    crowd.damage(r.f[0].parse().unwrap())
                } else {
                    crowd.excite(r.f[0].parse().unwrap(), true)
                };
                note(t, events);
            }
            let events = crowd.tick(dt);
            note(t, events);
            t += dt;
        }
        let live_mood: Vec<(f32, u8)> = rows
            .iter()
            .filter(|r| r.kind == "MOOD" && r.g < end)
            .map(|r| (r.g, r.f[0].parse().unwrap()))
            .collect();
        // The bell-time re-entry (`FUN_00416440`) is not in our replay; compare from the first change.
        // Each logged change has one of ours to the same mood within a second, and the other way
        // round, but for the knife-edge ones (an excitement of 44 or 45 at a threshold that the
        // replay's frame times, not the original's, cross a tick earlier or later).
        let ours: Vec<(f32, u8)> = ours_mood.iter().copied().filter(|m| m.0 < end).collect();
        let near = |a: &[(f32, u8)], m: &(f32, u8)| {
            a.iter().any(|o| o.1 == m.1 && (o.0 - m.0).abs() < 1.0)
        };
        let missing: Vec<_> = live_mood.iter().filter(|m| !near(&ours, m)).collect();
        let extra: Vec<_> = ours.iter().filter(|m| !near(&live_mood, m)).collect();
        assert!(
            missing.len() <= 1 && extra.len() <= 2,
            "fight {n}: missing {missing:?} extra {extra:?}\nours {ours:?}\nlive {live_mood:?}"
        );
        moods += live_mood.len() - missing.len();
        let live_shouts: Vec<f32> = rows
            .iter()
            .filter(|r| r.kind == "SHOUT" && r.f[1] == "1" && r.f[0] != "5")
            .map(|r| r.g)
            .collect();
        // Every logged taunt has one of ours within 1.5 s (our decay clock overshoots a tick less than a Wine frame does, 1.006 s against 1.027 s per point: a second by the 35th); one more or fewer is the same knife
        // edge as above (the taunt flag falls at an excitement of 15).
        let lost: Vec<_> = live_shouts
            .iter()
            .filter(|l| !ours_shout.iter().any(|o| (o - **l).abs() < 1.5))
            .collect();
        assert!(
            lost.is_empty() && ours_shout.len() <= live_shouts.len() + 1,
            "fight {n}: taunts ours {ours_shout:?} live {live_shouts:?}"
        );
        shouts += live_shouts.len();
    }
    assert!(
        moods >= 35 && shouts >= 16,
        "{moods} moods, {shouts} taunts"
    );
}
