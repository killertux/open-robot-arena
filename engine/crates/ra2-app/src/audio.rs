//! The client's audio: shell music, the UI click, and the script's match sounds.
//!
//! [`ra2_audio`] and [`ra2script::SoundQueue`] are the mechanics; this module is
//! the one place the running client drives them, so the shell, the match, the
//! HQ and the workshop share a policy instead of each opening its own mixer.
//!
//! - **Music per screen.** [`AudioSystem::on_screen`] maps a `Shell` screen id
//!   through [`ra2_audio::music_for_screen`] and only starts a track when the
//!   resolved file changes. A screen group that is not mapped stops the music;
//!   re-rendering the same screen does nothing.
//! - **UI click.** [`AudioSystem::click`] plays `Sounds/button_click.wav` at
//!   unity through the SFX category, so `SoundVolume` and `SoundMuted` apply.
//!   The shipped executable contains the path `Sounds\button_click.wav`; no
//!   call site was traced, so *which* controls use it is this client's mapping.
//! - **Match sounds.** [`AudioSystem::drain_match`] takes the requests
//!   `ra2script::take_sound_requests` collected and applies them with the
//!   queue's own handle table, at the camera the match is rendering.
//!
//! `Audio::new` falls back to a silent instance when there is no output device;
//! every call still lands in the [`ra2_audio::PlayLog`], which is what
//! `--audio-report` prints. A missing device is never an error.

use std::path::{Path, PathBuf};

use ra2_audio::{music_for_screen, Audio, Listener, PlayKind, Track};
use ra2fmt::cfg::Cfg;

/// The shipped UI click. The executable names the file; the controls that use
/// it are this client's mapping (see the module docs).
pub const CLICK_SOUND: &str = "Sounds/button_click.wav";

/// A UI or match cue with a fixed file, named by the exe (`Sounds\\...wav`).
///
/// Each name is a string in `Robot Arena 2.exe`; where the call site was traced
/// the address is given, and where it was not the mapping is this client's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiSound {
    /// `Sounds\button_click.wav` (`0x5ec275`, the button class).
    Click,
    /// `Sounds\cmp_removal.wav` (`0x42f2be`): a component taken off a bot.
    ComponentRemoved,
    /// A component's own `attachsound` (`cmp_generic.wav`, `cmp_wheels.wav`, ...)
    /// when it is attached in the workshop.
    ComponentAttached(Option<String>),
    /// `Sounds\snapshot.wav` (`0x43b8f2`, `0x5d7a67`): the workshop snapshot.
    Snapshot,
    /// `Sounds\point_plotted.wav` (`0x478fdf`, `0x47905d`): a graph point.
    PointPlotted,
    /// `Sounds\eject.wav` (`0x47d3f2`, `0x4d0791`).
    Eject,
    /// `Sounds\chat.wav` (`0x415849`): a chat line in a match.
    Chat,
    /// `Sounds\match_countdown.wav` (`0x4158bc`): a countdown number.
    CountdownBeep,
    /// `Sounds\match_countdown_high.wav` (`0x6f36fc`): the bell.
    CountdownGo,
    /// `Sounds\critical_hit.wav` (`0x4157d6`): a chassis critical hit.
    CriticalHit,
    /// `Sounds\immobile_countdown.wav` (`0x415763`): the immobility warning.
    ImmobileWarning,
}

impl UiSound {
    /// The file, or `None` for an attach with no sound named.
    pub fn path(&self) -> Option<String> {
        let name = match self {
            UiSound::Click => "Sounds/button_click.wav",
            UiSound::ComponentRemoved => "Sounds/cmp_removal.wav",
            UiSound::ComponentAttached(Some(path)) => return Some(path.clone()),
            UiSound::ComponentAttached(None) => return None,
            UiSound::Snapshot => "Sounds/snapshot.wav",
            UiSound::PointPlotted => "Sounds/point_plotted.wav",
            UiSound::Eject => "Sounds/eject.wav",
            UiSound::Chat => "Sounds/chat.wav",
            UiSound::CountdownBeep => "Sounds/match_countdown.wav",
            UiSound::CountdownGo => "Sounds/match_countdown_high.wav",
            UiSound::CriticalHit => "Sounds/critical_hit.wav",
            UiSound::ImmobileWarning => "Sounds/immobile_countdown.wav",
        };
        Some(name.to_string())
    }
}

/// The client's audio system and its report.
pub struct AudioSystem {
    game: PathBuf,
    audio: Audio,
    /// The file currently playing, so a re-render does not restart it.
    current_music: Option<PathBuf>,
    /// The screen whose track is current, for the report.
    current_screen: Option<String>,
    /// The last listener a match used, for the report.
    listener: Option<Listener>,
    /// Paths that could not be resolved or played.
    problems: Vec<String>,
    /// Music start/stop lines, which the `PlayLog` does not record.
    events: Vec<String>,
    /// The script host's match time at the last drain, for the audio clock.
    last_match_time: Option<f32>,
}

impl AudioSystem {
    /// A system with the default output device, or a silent one when the
    /// machine has none. `cfg`'s volumes and mute flags are applied at once.
    pub fn new(game: impl AsRef<Path>, cfg: &Cfg) -> AudioSystem {
        Self::with_audio(game, cfg, Audio::new())
    }

    /// The same policy over a silent [`Audio`], for the offscreen runners and
    /// tests. It records exactly what the windowed client would ask for.
    pub fn new_silent(game: impl AsRef<Path>, cfg: &Cfg) -> AudioSystem {
        Self::with_audio(game, cfg, Audio::without_device())
    }

    fn with_audio(game: impl AsRef<Path>, cfg: &Cfg, mut audio: Audio) -> AudioSystem {
        audio.apply_cfg(cfg);
        AudioSystem {
            game: game.as_ref().to_path_buf(),
            audio,
            current_music: None,
            current_screen: None,
            listener: None,
            problems: Vec::new(),
            events: Vec::new(),
            last_match_time: None,
        }
    }

    /// The same policy over a capturing [`Audio`]: no device, but every start,
    /// stop and fade is timed so [`AudioSystem::render_wav`] can write what the
    /// client would have played.
    pub fn new_capturing(game: impl AsRef<Path>, cfg: &Cfg) -> AudioSystem {
        Self::with_audio(game, cfg, Audio::capturing())
    }

    /// A capturing system when `RA2_AUDIO_WAV` names an output file, otherwise
    /// the silent one. The offscreen runners use this so any of them can write
    /// the audio the client would have played (see [`AudioSystem::finish_capture`]).
    pub fn new_offscreen(game: impl AsRef<Path>, cfg: &Cfg) -> AudioSystem {
        if std::env::var_os("RA2_AUDIO_WAV").is_some() {
            Self::new_capturing(game, cfg)
        } else {
            Self::new_silent(game, cfg)
        }
    }

    /// Write the captured audio to `RA2_AUDIO_WAV`, if that is set and this
    /// system is capturing. `RA2_AUDIO_MUSIC_OFFSET` (seconds) starts every
    /// music track that far into its file, to line a render up with a recording
    /// that began mid-track. `seconds` is the length to render.
    pub fn finish_capture(&mut self, seconds: f32) -> Result<(), String> {
        let Some(path) = std::env::var_os("RA2_AUDIO_WAV") else {
            return Ok(());
        };
        if self.audio.capture().is_none() {
            return Ok(());
        }
        if let Some(offset) = std::env::var("RA2_AUDIO_MUSIC_OFFSET")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
        {
            if let Some(capture) = self.audio.capture_mut() {
                capture.set_music_offset(offset);
            }
        }
        // `RA2_AUDIO_START` drops the first seconds of the render, so a match
        // render can start where the recording does (the end of the countdown).
        let start = std::env::var("RA2_AUDIO_START")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.0)
            .max(0.0);
        let path = std::path::PathBuf::from(path);
        let capture = self.audio.capture().ok_or("not capturing")?;
        let mut samples = capture.render(start + seconds);
        samples.drain(
            ..((start * ra2_audio::capture::RENDER_RATE as f32) as usize * 2).min(samples.len()),
        );
        ra2_audio::capture::write_wav(&path, &samples)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        println!(
            "audio: wrote {} ({seconds:.2} s from {start:.2} s; {} voices)",
            path.display(),
            capture.voice_count()
        );
        Ok(())
    }

    /// Advance the audio clock by `dt` seconds (fades, capture timestamps).
    pub fn advance(&mut self, dt: f32) {
        self.audio.advance(dt);
    }

    /// Mix the captured timeline into a 48 kHz stereo WAV, `seconds` long.
    pub fn render_wav(&self, path: &Path, seconds: f32) -> Result<(), String> {
        let capture = self
            .audio
            .capture()
            .ok_or("render_wav needs AudioSystem::new_capturing")?;
        let samples = capture.render(seconds);
        ra2_audio::capture::write_wav(path, &samples)
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Play a fixed UI/match sound through the SFX category (2D, unity).
    pub fn ui(&mut self, sound: UiSound) {
        let Some(name) = sound.path() else { return };
        match ra2_audio::resolve_game_path(&self.game, &name) {
            Some(path) => {
                if let Err(error) = self.audio.play_2d(&path, 1.0) {
                    self.problems.push(format!("ui sound: {error}"));
                }
            }
            None => self.problems.push(format!(
                "ui sound: {name} is not in {}",
                self.game.display()
            )),
        }
    }

    /// Play what a match step sounded like, heard from `listener`.
    pub fn play_cues(&mut self, cues: Vec<crate::match_sounds::SoundCue>, listener: Listener) {
        use crate::match_sounds::SoundCue;
        for cue in cues {
            match cue {
                SoundCue::Impact {
                    path,
                    position,
                    hundredths_db,
                } => {
                    let gain = ra2_audio::hundredths_db_to_gain(hundredths_db);
                    // The original's match audio is centred (left == right), so an
                    // impact is a flat sound at its volume, not a positioned one;
                    // the position stays in the cue for a future spatial model.
                    let _ = (position, listener);
                    match ra2_audio::resolve_game_path(&self.game, &path) {
                        Some(file) => {
                            if let Err(error) = self.audio.play_2d(&file, gain) {
                                self.problems.push(format!("impact: {error}"));
                            }
                        }
                        None => self
                            .problems
                            .push(format!("impact: {path} is not in the game")),
                    }
                }
                SoundCue::Beep { go } => self.ui(if go {
                    UiSound::CountdownGo
                } else {
                    UiSound::CountdownBeep
                }),
                SoundCue::Critical { .. } => self.ui(UiSound::CriticalHit),
                SoundCue::Chat => self.ui(UiSound::Chat),
                SoundCue::Motor {
                    key,
                    on,
                    path,
                    position: _,
                } => {
                    if on {
                        match ra2_audio::resolve_game_path(&self.game, &path) {
                            Some(file) => {
                                // A motor fades up rather than clicking on.
                                self.audio.set_script_volume(key, -10_000);
                                if let Err(error) = self
                                    .audio
                                    .play_script(key, &file, true, [0.0; 3], listener, 1.0)
                                {
                                    self.problems.push(format!("motor: {error}"));
                                }
                                self.audio.fade_script_to(key, 0, 250.0);
                            }
                            None => self
                                .problems
                                .push(format!("motor: {path} is not in the game")),
                        }
                    } else {
                        self.audio.fade_out_script(key, 250.0);
                    }
                }
            }
        }
    }

    /// The mixer and its log.
    pub fn audio(&self) -> &Audio {
        &self.audio
    }

    /// Re-read the `ra2.cfg` volumes and mute flags, without restarting music.
    pub fn apply_cfg(&mut self, cfg: &Cfg) {
        self.audio.apply_cfg(cfg);
    }

    /// Switch to the screen group's track, or stop when the group is not one
    /// the shipped music directories establish.
    pub fn on_screen(&mut self, screen: &str) {
        // A modal window (options, credits) opens over the screen and leaves
        // its music running: the executable's modal loop never touches it.
        if ra2_shell::bindings::is_window(screen) {
            return;
        }
        // The same screen on a re-render is not a transition. Updating the
        // stored id is still harmless, and it keeps the report's "screen" name
        // accurate when two screens share a track.
        let same_screen = self.current_screen.as_deref() == Some(screen);
        self.current_screen = Some(screen.to_string());

        let Some(track) = music_for_screen(screen) else {
            self.stop_music(&format!("{screen} has no established track"));
            return;
        };
        let path = track.in_game_dir(&self.game);
        if self.current_music.as_deref() == Some(path.as_path()) {
            return;
        }
        if same_screen {
            return;
        }
        self.start_track(track, path);
    }

    /// The same switch for a caller that already mapped the screen, or `None`
    /// when the screen has no track (a match).
    pub fn on_screen_opt(&mut self, screen: Option<&str>) {
        match screen {
            Some(screen) => self.on_screen(screen),
            None => self.stop_music("screen has no UI track"),
        }
    }

    fn start_track(&mut self, track: Track, path: PathBuf) {
        if !path.is_file() {
            self.problems.push(format!(
                "music: {} is not in {}",
                track.relative_path(),
                self.game.display()
            ));
            return;
        }
        match self.audio.play_music(&path, true) {
            Ok(()) => {
                self.current_music = Some(path.clone());
                self.events
                    .push(format!("music: start {} ({})", path.display(), track.group));
            }
            Err(error) => {
                self.problems.push(format!("music: {error}"));
            }
        }
    }

    /// Stop the current track. A second stop is silent.
    pub fn stop_music(&mut self, reason: &str) {
        // The next `on_screen` must start a track again even if it is the same
        // screen that was playing before the stop (a match interrupted it).
        self.current_screen = None;
        if self.current_music.take().is_some() {
            self.audio.stop_music();
            self.events.push(format!("music: stop ({reason})"));
        }
    }

    /// Play the shipped UI click at unity. The effective gain is the SFX
    /// category's (`SoundVolume`, `SoundMuted`), because it is a 2D sound.
    pub fn click(&mut self) {
        self.ui(UiSound::Click);
    }

    /// Drain this tick's script sounds into the mixer, heard from `listener`.
    ///
    /// `ra2script::take_sound_requests` removes the requests but keeps the
    /// handle table; `SoundQueue::apply_requests` is what resolves `loopSound`
    /// and `stopSound` against it, so both halves stay the script layer's.
    pub fn drain_match(&mut self, listener: Listener) {
        self.listener = Some(listener);
        // The audio clock follows the script host's match time, so fades and
        // capture timestamps are in match seconds whatever the tick rate.
        let now = ra2script::state().time;
        if let Some(last) = self.last_match_time {
            if now > last {
                self.audio.advance(now - last);
            }
        }
        self.last_match_time = Some(now);
        let requests = ra2script::take_sound_requests();
        if requests.is_empty() {
            return;
        }
        let problems = {
            let mut state = ra2script::state();
            state
                .sounds
                .apply_requests(requests, &self.game, listener, &mut self.audio)
        };
        self.problems.extend(problems);
    }

    /// What was played, as lines for the console.
    ///
    /// The first line is always present: on a machine with no device it says so,
    /// and the rest is still the exact call log. `detail` adds one line per
    /// play call (path, kind, effective gain, emitter position).
    pub fn report(&self, detail: bool) -> String {
        let log = self.audio.play_log();
        let music = log
            .entries()
            .iter()
            .filter(|e| e.kind == PlayKind::Music)
            .count();
        let clicks = log
            .entries()
            .iter()
            .filter(|e| e.kind == PlayKind::Sound2d)
            .count();
        let sounds = log
            .entries()
            .iter()
            .filter(|e| e.kind == PlayKind::Sound3d)
            .count();
        let mut out = String::new();
        out.push_str(&format!(
            "audio: {}; {} play call(s): {music} music, {clicks} click, {sounds} match/script\n",
            if self.audio.has_device() {
                "output device opened"
            } else {
                "no output device (silent log only)"
            },
            log.len()
        ));
        if let Some(listener) = self.listener {
            out.push_str(&format!(
                "audio: listener position {:?} forward {:?}\n",
                listener.position, listener.forward
            ));
        }
        for event in &self.events {
            out.push_str(&format!("audio: {event}\n"));
        }
        if detail {
            for (index, entry) in log.entries().iter().enumerate() {
                let kind = match entry.kind {
                    PlayKind::Music => "music",
                    PlayKind::Sound2d => "click",
                    PlayKind::Sound3d => "sound",
                };
                match entry.position {
                    Some(position) => out.push_str(&format!(
                        "audio[{index}] {kind} {} gain {:.6} position {:?}\n",
                        entry.path.display(),
                        entry.gain,
                        position
                    )),
                    None => out.push_str(&format!(
                        "audio[{index}] {kind} {} gain {:.6} position -\n",
                        entry.path.display(),
                        entry.gain
                    )),
                }
            }
        }
        for problem in &self.problems {
            out.push_str(&format!("audio problem: {problem}\n"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn game_dir() -> Option<PathBuf> {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join("local/game");
            if candidate.is_dir() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    fn cfg_with_unity_volumes() -> Cfg {
        Cfg::parse(
            "[Begin RA2 Config]\r\nSoundVolume=0\r\nMusicVolume=0\r\n\
             SoundMuted=0\r\nMusicMuted=0\r\n[End RA2 Config]\r\n",
        )
    }

    #[test]
    fn a_screen_change_switches_music_and_a_render_does_not() {
        let Some(game) = game_dir() else { return };
        let cfg = cfg_with_unity_volumes();
        let mut audio = AudioSystem::new_silent(&game, &cfg);

        audio.on_screen("mainmenu/mainmenu.gib");
        audio.on_screen("mainmenu/mainmenu.gib");
        assert_eq!(audio.audio.play_log().len(), 1, "one track start");

        audio.on_screen("teamhq/select_team.gib");
        audio.on_screen("teamhq/teamhq.gib");
        assert_eq!(audio.audio.play_log().len(), 2, "same HQ group, no restart");

        audio.on_screen("botlab/botlabwin.gib");
        assert_eq!(audio.audio.play_log().len(), 3, "the workshop track");
        let paths: Vec<_> = audio
            .audio
            .play_log()
            .entries()
            .iter()
            .map(|e| e.path.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(paths[0].ends_with("Music/Main UI/Enter the Battlefield.mp3"));
        assert!(paths[1].ends_with("Music/Team HQ/Robot Tribal Dance.mp3"));
        assert!(paths[2].ends_with("Music/Workshop/Deep inside the Bot Lab.mp3"));

        audio.on_screen("help/help.gib");
        assert!(audio.audio.music_path().is_none(), "unmapped screen stops");
        let report = audio.report(true);
        assert!(report.contains("music: stop"), "{report}");

        // A match stops the shell music without changing the screen; when the
        // shell comes back to the same screen it must start again.
        audio.stop_music("the match is on screen");
        audio.on_screen("botlab/botlabwin.gib");
        assert_eq!(audio.audio.play_log().len(), 4, "the track restarts");
    }

    #[test]
    fn one_click_is_one_play_and_it_obeys_the_sfx_gain() {
        let Some(game) = game_dir() else { return };
        let mut audio = AudioSystem::new_silent(&game, &cfg_with_unity_volumes());
        audio.click();
        audio.click();
        let entries = audio.audio.play_log().entries();
        assert_eq!(entries.len(), 2);
        for entry in entries {
            assert_eq!(entry.kind, PlayKind::Sound2d);
            assert!(entry.path.ends_with("Sounds/button_click.wav"));
            assert!((entry.gain - 1.0).abs() < 1e-6);
        }

        let muted = Cfg::parse("[Begin RA2 Config]\r\nSoundMuted=1\r\n[End RA2 Config]\r\n");
        audio.apply_cfg(&muted);
        audio.click();
        assert_eq!(audio.audio.play_log().last().unwrap().gain, 0.0);
    }

    #[test]
    fn the_report_always_says_what_the_device_state_is() {
        let Some(game) = game_dir() else { return };
        let audio = AudioSystem::new_silent(&game, &cfg_with_unity_volumes());
        let report = audio.report(true);
        assert!(
            report.contains("no output device (silent log only)"),
            "{report}"
        );
        assert_eq!(
            report.lines().filter(|l| l.starts_with("audio: ")).count(),
            1
        );
    }
}
