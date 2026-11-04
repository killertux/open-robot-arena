//! The original's own sound-call logs (`local/oracle/original/audio-*.events.tsv`,
//! `tools/oracle/capture_audio.py` + `audio_events.py`) against the numbers this
//! client encodes. Every test skips when the captures are absent, as the other
//! oracle corpus tests do; none of the data is committed.

use std::path::PathBuf;

use ra2_app::match_sounds::{BEGIN_DELAY, BEGIN_LINES, BURST_FIRE_HDB, MOTOR_LOOP_HDB};
use ra2_audio::rules::{impact_volume_hdb, material_volume_hdb};

#[derive(Clone, Debug)]
struct Event {
    t: f32,
    kind: String,
    file: String,
    volume: Option<i32>,
    handle: i64,
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

fn load(name: &str) -> Option<Vec<Event>> {
    let text = std::fs::read_to_string(original_dir()?.join(name)).ok()?;
    Some(
        text.lines()
            .skip(1)
            .filter_map(|line| {
                let f: Vec<&str> = line.split('\t').collect();
                (f.len() >= 5).then(|| Event {
                    t: f[0].parse().unwrap_or(0.0),
                    kind: f[1].to_string(),
                    file: f[2].to_string(),
                    volume: f[3].parse().ok(),
                    handle: f[4].parse().unwrap_or(-1),
                })
            })
            .collect(),
    )
}

const FULL: &str = "audio-match-box-scout-alarm__live__000.events.tsv";
const FIGHTS: [&str; 3] = [
    "audio-fight-scout-alarm__live__000.events.tsv",
    "audio-fight-scout-alarm__live__001.events.tsv",
    "audio-fight-scout-alarm__live__002.events.tsv",
];

fn plays<'a>(
    events: &'a [Event],
    pred: impl Fn(&str) -> bool + 'a,
) -> impl Iterator<Item = &'a Event> {
    events
        .iter()
        .filter(move |e| e.kind == "play" && pred(&e.file))
}

/// The bell, then one of the six `Begin_*` lines half a second later (0.51 s in
/// the two full matches, 0.51-0.68 s in the fights: it waits for the next
/// window update).
#[test]
fn the_begin_line_follows_the_bell_by_the_delay_we_use() {
    for name in std::iter::once(FULL).chain(FIGHTS) {
        let Some(events) = load(name) else { return };
        let bell = plays(&events, |f| f == "match_countdown_high.wav")
            .next()
            .expect("the bell")
            .t;
        let begin: Vec<&Event> = plays(&events, |f| f.starts_with("Begin_")).collect();
        assert_eq!(begin.len(), 1, "{name}: one Begin line");
        assert!(
            BEGIN_LINES
                .iter()
                .any(|l| l.ends_with(&format!("/{}", begin[0].file))),
            "{name}: {}",
            begin[0].file
        );
        let gap = begin[0].t - bell;
        assert!(
            (BEGIN_DELAY - 0.05..BEGIN_DELAY + 0.25).contains(&gap),
            "{name}: Begin line {gap} s after the bell"
        );
    }
}

/// Impact one-shots are created at the volume the damage rule gives and played
/// at that same volume; none is below the floor.
#[test]
fn impact_volumes_stay_inside_the_damage_rule() {
    let Some(events) = load(FULL) else { return };
    let impact =
        |f: &str| f.starts_with("titanium_hit") || f == "hammer_hit.wav" || f == "bracket_hit.wav";
    let mut n = 0;
    for e in events
        .iter()
        .filter(|e| e.kind == "create" && impact(&e.file))
    {
        let v = e.volume.expect("a volume");
        assert!((-1500..=0).contains(&v), "{e:?}");
        // The rule's own outputs are exactly these integers.
        assert!(
            impact_volume_hdb(4.0 + (100.0 - 4.0) * (1.0 + v as f32 / 1500.0)).is_some()
                || v == -1500
                || v == 0,
            "{v}"
        );
        let play = events
            .iter()
            .find(|p| p.kind == "play" && p.handle == e.handle)
            .expect("played");
        assert_eq!(play.volume, Some(v));
        n += 1;
    }
    assert!(n > 50, "{n} impacts");
}

/// Motor loops are created at -700 and the burst motor's firesound at -500
/// (the levels `match_sounds` uses), the piston's at 0.
#[test]
fn motor_and_weapon_levels_are_the_ones_we_use() {
    for name in std::iter::once(FULL).chain(FIGHTS) {
        let Some(events) = load(name) else { return };
        let creates = |file: &str| -> Vec<i32> {
            events
                .iter()
                .filter(|e| e.kind == "create" && e.file == file)
                .filter_map(|e| e.volume)
                .collect()
        };
        let motors = creates("motorloop2.wav");
        assert!(!motors.is_empty(), "{name}");
        assert!(motors.iter().all(|v| *v == MOTOR_LOOP_HDB), "{motors:?}");
        assert_eq!(creates("burst_trigger.wav"), [BURST_FIRE_HDB], "{name}");
        assert_eq!(creates("air_trigger.wav"), [0], "{name}");
    }
}

/// The piston sounds on each trigger: never closer than the AI's own 0.5 s
/// (minus the step grid), a few hundred in a whole fight at most.
#[test]
fn the_piston_trigger_repeats_at_the_ai_rate() {
    for name in FIGHTS {
        let Some(events) = load(name) else { return };
        let times: Vec<f32> = plays(&events, |f| f == "air_trigger.wav")
            .map(|e| e.t)
            .collect();
        assert!(times.len() > 20, "{name}: {}", times.len());
        let close = times.windows(2).filter(|w| w[1] - w[0] < 0.4).count();
        assert!(
            close * 8 < times.len(),
            "{name}: {close} of {}",
            times.len()
        );
    }
}

/// Arena sub-material sounds: created per contact at -300..0 through the
/// second `CRect`, from the arena script's own sound file.
#[test]
fn arena_material_sounds_use_the_second_rectangle() {
    let mut n = 0;
    for name in FIGHTS {
        let Some(events) = load(name) else { return };
        for e in events.iter().filter(|e| {
            e.kind == "create"
                && (e.file == "hzd_spike_hit.wav" || e.file == "sawblade_hits_loop.wav")
        }) {
            let v = e.volume.expect("a volume");
            assert!((-300..=0).contains(&v), "{e:?}");
            // Reachable by the rule: some magnitude at some factor gives v.
            let found = [0.7_f32, 0.8, 1.0]
                .iter()
                .any(|f| (1..4000).any(|m| material_volume_hdb(*f, m as f32 * 0.05) == Some(v)));
            assert!(found, "{v} is not a material_volume_hdb output");
            n += 1;
        }
    }
    if n == 0 {
        eprintln!("no arena material hits in the local captures");
    }
}

/// The intro schedule `Arenas/box.py` makes: music, the arena line a quarter
/// second later, the next three at +3, +8 and +12 s (hazards on), the intro
/// music fading at +20 and the first countdown beep at +22 from the music.
#[test]
fn the_intro_and_countdown_follow_the_box_script() {
    let Some(events) = load(FULL) else { return };
    let music = events
        .iter()
        .find(|e| e.kind == "loop" && e.file.contains("smell_glue"))
        .expect("intro music")
        .t;
    let line = |prefix: &str| -> f32 {
        plays(&events, |f| f.starts_with(prefix))
            .next()
            .unwrap_or_else(|| panic!("{prefix}"))
            .t
            - music
    };
    assert!((line("Arena_") - 0.25).abs() < 0.1);
    assert!((line("Intro_") - 3.25).abs() < 0.1);
    assert!((line("Hazard_") - 8.25).abs() < 0.1);
    assert!((line("Bots_") - 12.25).abs() < 0.1);
    let beep = plays(&events, |f| f == "match_countdown.wav")
        .next()
        .unwrap()
        .t;
    assert!((beep - music - 22.27).abs() < 0.15, "{}", beep - music);
}

/// The crowd is built with the match and ticks through the intro: its first
/// transition comes about 11 s in and it is bored (`somewhat_to_bored`) by the
/// bell, which re-enters with that same one-shot.
#[test]
fn the_crowd_is_bored_at_the_bell() {
    let Some(events) = load(FULL) else { return };
    let built = events
        .iter()
        .find(|e| e.kind == "create" && e.file == "bored_loop.wav")
        .unwrap()
        .t;
    let first = plays(&events, |f| f == "normal_to_somewhat.wav")
        .next()
        .unwrap()
        .t;
    assert!((first - built - 11.1).abs() < 0.6, "{}", first - built);
    let bell = plays(&events, |f| f == "match_countdown_high.wav")
        .next()
        .unwrap()
        .t;
    let at_bell = plays(&events, |f| f == "somewhat_to_bored.wav")
        .filter(|e| (e.t - bell).abs() < 0.1)
        .count();
    assert_eq!(at_bell, 1);
}

const FREQ: &str = "audio-match-freq__live__000.events.tsv";

/// A motor handle `Loop`s and `Stop`s with no fade: no volume call follows its
/// creation at -700, every restart is a fresh `Loop` after a gap of at least one
/// frame-and-a-bit (0.1 s: the AI's think), and every start in the log is at the
/// handle's creation level (docs/35 section 13).
#[test]
fn motor_handles_loop_and_stop_with_no_fade() {
    let Some(events) = load(FREQ) else { return };
    let motors: Vec<i64> = events
        .iter()
        .filter(|e| e.kind == "create" && e.file == "motorloop2.wav")
        .map(|e| e.handle)
        .collect();
    assert!(motors.len() >= 6, "{motors:?}");
    let volume_calls = events
        .iter()
        .filter(|e| e.kind == "setvolume" && motors.contains(&e.handle))
        .count();
    // The one SetVolume each handle makes is the creation's own (the log's V row).
    assert_eq!(volume_calls, motors.len(), "no fade, no level change");
    assert!(events
        .iter()
        .filter(|e| e.kind == "setvolume" && motors.contains(&e.handle))
        .all(|e| e.volume == Some(MOTOR_LOOP_HDB)));
    let mut stopped: std::collections::HashMap<i64, f32> = Default::default();
    let mut gaps = Vec::new();
    for e in events.iter().filter(|e| motors.contains(&e.handle)) {
        match e.kind.as_str() {
            "stop" => {
                stopped.insert(e.handle, e.t);
            }
            "loop" => {
                if let Some(t) = stopped.remove(&e.handle) {
                    gaps.push(e.t - t);
                }
            }
            _ => {}
        }
    }
    assert!(gaps.len() > 100, "{}", gaps.len());
    assert!(gaps.iter().all(|g| *g > 0.09), "{gaps:?}");
}

/// `SetFrequency` of a motor loop, every game frame: the file's 22050 Hz rate times
/// `0.5 + 0.5 r` with `r = |omega| / max rate` (`FUN_004f5b50`): never below an
/// octave down, and past the file's own rate when the motor overspeeds.
#[test]
fn motor_frequency_follows_half_plus_half_the_rate_ratio() {
    let Some(events) = load(FREQ) else { return };
    let hz: Vec<f32> = events
        .iter()
        .filter(|e| e.kind == "freq" && e.file == "motorloop2.wav")
        .filter_map(|e| e.volume.map(|v| v as f32))
        .collect();
    assert!(hz.len() > 1000, "{}", hz.len());
    let low = hz.iter().cloned().fold(f32::MAX, f32::min);
    let high = hz.iter().cloned().fold(0.0, f32::max);
    assert!(low >= 11_025.0 - 1.0, "{low}");
    assert!(high > 22_050.0, "{high}");
    // Ours reproduces the same ratio for the same r (r = (hz / f - 0.5) * 2).
    for h in hz.iter().step_by(37) {
        let r = (h / 22_050.0 - 0.5) * 2.0;
        let ours = ra2_app::match_sounds::motor_pitch(r * 10.0, 10.0);
        assert!((ours - h / 22_050.0).abs() < 1e-4, "{h}: {ours}");
    }
}

/// The wrapper's SetPosition is never called in a fight: every match effect is
/// centred (the recordings are exactly dual-mono; docs/35 section 2).
#[test]
fn no_effect_is_ever_positioned_in_a_match() {
    let Some(events) = load("audio-match-3d__live__000.events.tsv") else {
        return;
    };
    assert!(events.iter().filter(|e| e.kind == "create").count() > 100);
    assert_eq!(events.iter().filter(|e| e.kind == "pos").count(), 0);
}

const OCTAGON: &str = "audio-match-octagon-scout-alarm__live__000.events.tsv";
const COMPRESSOR: &str = "audio-match-compressor-scout-alarm__live__000.events.tsv";
const BOTLAB_REMOVE: &str = "audio-botlab-place__live__000.events.tsv";
const BOTLAB_ATTACH: &str = "audio-botlab-place__live__001.events.tsv";

/// The Octagon's four trap floors loop their slide sound 90 arena ticks (22.5 s) after the bell
/// (`Activate(True)` is called at the bell, docs/35 section 14), and stop it again 8 s later; the
/// hellraisers' trigger sound is a one-shot `Play`, never a `Loop` (`createSound`'s second argument
/// is the 3D flag). The decision starts the outro music and one `End_*` line.
#[test]
fn the_octagon_floors_slide_22_5_s_after_the_bell_and_the_hellraiser_is_a_one_shot() {
    let Some(events) = load(OCTAGON) else { return };
    let bell = plays(&events, |f| f == "match_countdown_high.wav")
        .next()
        .expect("the bell")
        .t;
    let slides: Vec<&Event> = events
        .iter()
        .filter(|e| e.kind == "loop" && e.file == "hzd_trapfloor_loop.wav")
        .collect();
    assert_eq!(slides.len(), 4, "four floors");
    for slide in &slides {
        let after = slide.t - bell;
        assert!(
            (22.0..23.2).contains(&after),
            "slide loop {after} s after the bell"
        );
    }
    assert!(
        events
            .iter()
            .all(|e| !(e.kind == "loop" && e.file == "hellraiser_trigger.wav")),
        "the hellraiser trigger never loops"
    );
    assert!(plays(&events, |f| f == "hellraiser_trigger.wav").count() >= 2);
    let outro = events
        .iter()
        .find(|e| e.kind == "loop" && e.file == "hopp_club.wav")
        .expect("the outro music loops at the decision");
    let line = plays(&events, |f| f.starts_with("End_"))
        .next()
        .expect("one closing line");
    assert!(
        (line.t - outro.t).abs() < 0.1,
        "the music and the line start together"
    );
}

/// The Compressor and the Octagon open with the same intro as the Box (the music, the arena line,
/// the generic line, the hazard line, the bots line) and the crowd's bored transitions.
#[test]
fn the_other_arenas_play_the_same_intro_schedule() {
    for (name, arena) in [
        (OCTAGON, "Arena_Octagon_"),
        (COMPRESSOR, "Arena_Compressor_"),
    ] {
        let Some(events) = load(name) else { return };
        let bell = plays(&events, |f| f == "match_countdown_high.wav")
            .next()
            .expect("the bell")
            .t;
        let at = |pred: &dyn Fn(&str) -> bool| plays(&events, pred).next().map(|e| e.t - bell);
        let welcome = at(&|f| f.starts_with(arena)).expect("arena line");
        let generic = at(&|f| f.starts_with("Intro_")).expect("generic line");
        let hazard = at(&|f| f.starts_with("Hazard_")).expect("hazard line");
        let bots = at(&|f| f.starts_with("Bots_")).expect("bots line");
        // Seconds from the bell: -27.1 / -24.1 / -19.6 / -14.0 (octagon), -27.2 / -24.1 / -19.0 / -13.0
        assert!((-27.6..-26.8).contains(&welcome), "{name}: arena {welcome}");
        assert!(
            (-24.5..-23.8).contains(&generic),
            "{name}: generic {generic}"
        );
        assert!((-20.0..-18.5).contains(&hazard), "{name}: hazard {hazard}");
        assert!((-14.5..-12.5).contains(&bots), "{name}: bots {bots}");
    }
}

/// Bot Lab: attaching a part creates and plays its `attachsound` once at volume 0 (the Mud Tire's
/// definition names none, and `sounds\cmp_generic.wav` plays); removing a part plays
/// `cmp_removal.wav` the same way; the Attach and palette buttons click.
#[test]
fn the_bot_lab_plays_the_attach_and_removal_sounds_once_at_unity() {
    let Some(attach) = load(BOTLAB_ATTACH) else {
        return;
    };
    let generic = plays(&attach, |f| f == "cmp_generic.wav").count();
    assert_eq!(generic, 1, "one attach sound");
    let created = attach
        .iter()
        .find(|e| e.kind == "create" && e.file == "cmp_generic.wav")
        .expect("created");
    assert_eq!(created.volume, Some(0));
    let Some(remove) = load(BOTLAB_REMOVE) else {
        return;
    };
    assert_eq!(plays(&remove, |f| f == "cmp_removal.wav").count(), 1);
    assert!(
        remove.iter().any(|e| e.kind == "ui_click"),
        "the Attach and palette buttons click"
    );
}
