//! Decode and play the shipped Robot Arena 2 music and sounds.
//!
//! The game's assets are one MP3 per screen group under `Music/` and a large
//! set of PCM WAVs under `Sounds/`. This crate wraps [`rodio`] and keeps the
//! one property CI needs: **a missing audio device is not an error**. When
//! `cpal` cannot open an output device, [`Audio`] still constructs, every play
//! call succeeds as a no-op, and the attempt is recorded in its [`PlayLog`].
//! Tests therefore exercise the decoder and the mixing policy without a sound
//! card, and the windowed client can still run on a machine with no audio.
//!
//! # Volume
//!
//! `ra2.cfg` stores `SoundVolume` and `MusicVolume` in **hundredths of a
//! decibel**, DirectSound's `DSBVOLUME` convention: the range is `-10000`
//! (silence) to `0` (unity), so `-600` is -6 dB. The game's own writer prints
//! them with `SoundVolume=%d` / `MusicVolume=%d`, and its defaults are `-600`;
//! the shipped `ReadMe.txt` does not discuss volume, and no shipped string
//! contradicts this reading. [`hundredths_db_to_gain`] converts a value to a
//! linear amplitude:
//!
//! ```text
//! gain = 10^(value / 100 / 20)
//! ```
//!
//! with `value <= -10000` clamped to `0` (true silence, not `0.00001`) and
//! `value >= 0` clamped to `1`. `-600` is therefore about `0.501`. The mute
//! flags are independent booleans and override the volume: a muted category
//! plays at gain `0` even if its volume key is `0`.
//!
//! The per-call `gain` arguments are also linear amplitudes (`1.0` = unity);
//! the effective gain in the [`PlayLog`] is
//! `per_call * category * mute`, so a test can assert exactly what was asked
//! for.
//!
//! # 3D sound
//!
//! [`Audio::play_3d`] uses [`rodio::source::Spatial`]'s positional model. The
//! source is summed to mono and copied to two ears placed
//! `2 × 0.075 m` apart around the [`Listener`]'s position, along its right
//! vector. Each ear's amplitude is
//!
//! ```text
//! pan(left or right) × min(1, 1 / d²)
//! ```
//!
//! where `d` is the distance from that ear to the emitter (world units, so
//! one unit is one metre) and `pan` is rodio's ear-distance modifier,
//! `clamp(((d_ear - d_other) / d_ears + 1) / 4 + 0.5, .., 1)`. Sounds closer
//! than one unit are not boosted; beyond that they fall off with the inverse
//! square. There is no Doppler, occlusion or reverb: the shipped game's sound
//! system is positional only. The log records the emitter position, not the
//! ears.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use ra2fmt::cfg::Cfg;
use rodio::source::Spatial;
use rodio::{Decoder, MixerDeviceSink, Player, Source};

pub mod music;

pub use music::{music_for_screen, Track, MAIN_UI, TEAM_HQ, WORKSHOP};

/// DirectSound's minimum volume in hundredths of a decibel (silence).
pub const MIN_VOLUME_HUNDREDTHS_DB: i32 = -10_000;
/// DirectSound's maximum volume in hundredths of a decibel (unity).
pub const MAX_VOLUME_HUNDREDTHS_DB: i32 = 0;
/// Half the distance between the listener's ears, in world units (7.5 cm).
pub const EAR_HALF_SPACING: f32 = 0.075;

/// Convert `ra2.cfg`'s hundredths-of-a-decibel volume to a linear amplitude.
///
/// `-10000` is silence (`0.0`), `-600` is -6 dB (about `0.501`), `0` is unity,
/// and positive values are clamped to unity. Values below `-10000` are
/// clamped to silence. This is the DirectSound `DSBVOLUME` interpretation;
/// see the crate docs.
pub fn hundredths_db_to_gain(value: i32) -> f32 {
    if value <= MIN_VOLUME_HUNDREDTHS_DB {
        return 0.0;
    }
    if value >= MAX_VOLUME_HUNDREDTHS_DB {
        return 1.0;
    }
    10f32.powf(value as f32 / 100.0 / 20.0)
}

/// What a play call was: a 2D sound, a positional sound, or music.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayKind {
    /// [`Audio::play_2d`].
    Sound2d,
    /// [`Audio::play_3d`].
    Sound3d,
    /// [`Audio::play_music`].
    Music,
}

/// One recorded play call.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayEntry {
    /// The path the caller asked for.
    pub path: PathBuf,
    /// Which entry point was used.
    pub kind: PlayKind,
    /// The effective linear gain: per-call gain × category gain, or `0` when
    /// muted.
    pub gain: f32,
    /// The emitter position for [`Audio::play_3d`]; `None` for 2D and music.
    pub position: Option<[f32; 3]>,
}

/// Every play call made through an [`Audio`], in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayLog {
    entries: Vec<PlayEntry>,
}

impl PlayLog {
    /// The entries, in call order.
    pub fn entries(&self) -> &[PlayEntry] {
        &self.entries
    }

    /// The most recent entry.
    pub fn last(&self) -> Option<&PlayEntry> {
        self.entries.last()
    }

    /// How many play calls were recorded.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no play call has been recorded.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Forget every recorded call.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    fn push(&mut self, entry: PlayEntry) {
        self.entries.push(entry);
    }
}

/// Where the listener is and which way it faces, for [`Audio::play_3d`].
///
/// `forward` and `up` need not be normalised; they define the right vector as
/// `normalize(up × forward)`. A degenerate pair falls back to `forward = +Z`,
/// `up = +Y`, `right = +X`, which is Direct3D's left-handed convention (the
/// one the game's engine uses).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    /// The listener's position in world units.
    pub position: [f32; 3],
    /// The direction the listener faces.
    pub forward: [f32; 3],
    /// The listener's up direction.
    pub up: [f32; 3],
}

impl Listener {
    /// A listener at `position`, facing `forward` with `up` as its up vector.
    pub const fn new(position: [f32; 3], forward: [f32; 3], up: [f32; 3]) -> Listener {
        Listener {
            position,
            forward,
            up,
        }
    }

    /// A listener at `position` looking down Direct3D's +Z with +Y up.
    pub const fn looking_forward(position: [f32; 3]) -> Listener {
        Listener::new(position, [0.0, 0.0, 1.0], [0.0, 1.0, 0.0])
    }

    /// The positions of the left and right ears, `2 × EAR_HALF_SPACING` apart.
    pub fn ears(&self) -> ([f32; 3], [f32; 3]) {
        let forward = normalize_or(self.forward, [0.0, 0.0, 1.0]);
        let up = normalize_or(self.up, [0.0, 1.0, 0.0]);
        let right = normalize_or(cross(up, forward), [1.0, 0.0, 0.0]);
        let offset = [
            right[0] * EAR_HALF_SPACING,
            right[1] * EAR_HALF_SPACING,
            right[2] * EAR_HALF_SPACING,
        ];
        (
            [
                self.position[0] - offset[0],
                self.position[1] - offset[1],
                self.position[2] - offset[2],
            ],
            [
                self.position[0] + offset[0],
                self.position[1] + offset[1],
                self.position[2] + offset[2],
            ],
        )
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize_or(value: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let length = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    if length.is_finite() && length > f32::EPSILON {
        [value[0] / length, value[1] / length, value[2] / length]
    } else {
        fallback
    }
}

/// The audio system: one output device (or none), one music player, a set of
/// one-shot sound players, and the play log.
pub struct Audio {
    device: Option<MixerDeviceSink>,
    music: Option<Player>,
    sfx: Vec<Player>,
    log: PlayLog,
    sfx_gain: f32,
    music_gain: f32,
    muted: bool,
    sound_muted: bool,
    music_muted: bool,
    music_path: Option<PathBuf>,
}

impl Audio {
    /// Open the default output device, or fall back to a silent instance.
    ///
    /// This never panics and never fails: if `cpal` has no device (CI, a
    /// headless server, a machine with no sound card) the returned instance
    /// has [`Audio::device`] `None` and every play call is recorded but does
    /// nothing.
    pub fn new() -> Audio {
        Self::with_device(rodio::DeviceSinkBuilder::open_default_sink().ok())
    }

    /// A silent instance with no output device, useful for tests and for
    /// running the shell with `--mute`-style behaviour.
    pub fn without_device() -> Audio {
        Self::with_device(None)
    }

    fn with_device(mut device: Option<MixerDeviceSink>) -> Audio {
        if let Some(device) = &mut device {
            // Keep rodio's normal "Dropping DeviceSink" notice out of the
            // client's stderr; dropping is expected when the app exits.
            device.log_on_drop(false);
        }
        Audio {
            device,
            music: None,
            sfx: Vec::new(),
            log: PlayLog::default(),
            sfx_gain: 1.0,
            music_gain: 1.0,
            muted: false,
            sound_muted: false,
            music_muted: false,
            music_path: None,
        }
    }

    /// The output device, or `None` when this instance is silent.
    pub fn device(&self) -> Option<&MixerDeviceSink> {
        self.device.as_ref()
    }

    /// Whether an output device was opened.
    pub fn has_device(&self) -> bool {
        self.device.is_some()
    }

    /// Everything played through this instance, in call order.
    pub fn play_log(&self) -> &PlayLog {
        &self.log
    }

    /// The configured SFX gain, before mute.
    pub fn sfx_gain(&self) -> f32 {
        self.sfx_gain
    }

    /// The configured music gain, before mute.
    pub fn music_gain(&self) -> f32 {
        self.music_gain
    }

    /// The gain SFX actually plays at (zero when globally or per-category
    /// muted).
    pub fn output_sfx_gain(&self) -> f32 {
        if self.muted || self.sound_muted {
            0.0
        } else {
            self.sfx_gain
        }
    }

    /// The gain music actually plays at (zero when globally or per-category
    /// muted).
    pub fn output_music_gain(&self) -> f32 {
        if self.muted || self.music_muted {
            0.0
        } else {
            self.music_gain
        }
    }

    /// Whether the global mute is on.
    pub fn is_muted(&self) -> bool {
        self.muted
    }

    /// The currently playing music path, if any.
    pub fn music_path(&self) -> Option<&Path> {
        self.music_path.as_deref()
    }

    /// Apply the `ra2.cfg` volumes and mute flags.
    ///
    /// `SoundVolume`/`MusicVolume` are converted with
    /// [`hundredths_db_to_gain`]; `SoundMuted`/`MusicMuted` then override the
    /// result. The global [`Audio::set_muted`] is left as it is, and a later
    /// [`Audio::set_sfx_gain`] or [`Audio::set_music_gain`] replaces the
    /// configured gain while the per-category mute still overrides it.
    pub fn apply_cfg(&mut self, cfg: &Cfg) {
        self.sfx_gain = hundredths_db_to_gain(cfg.sound_volume());
        self.music_gain = hundredths_db_to_gain(cfg.music_volume());
        self.sound_muted = cfg.sound_muted();
        self.music_muted = cfg.music_muted();
        self.refresh_live_volumes();
    }

    /// Set the SFX gain (linear, `1.0` = unity, negative values clamp to zero).
    pub fn set_sfx_gain(&mut self, gain: f32) {
        self.sfx_gain = linear(gain);
        self.refresh_live_volumes();
    }

    /// Set the music gain (linear, `1.0` = unity, negative values clamp to zero).
    pub fn set_music_gain(&mut self, gain: f32) {
        self.music_gain = linear(gain);
        self.refresh_live_volumes();
    }

    /// Mute or unmute everything, music and live SFX alike.
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.refresh_live_volumes();
    }

    /// Play a non-positional sound (a button click, a UI cue, an announcer).
    ///
    /// `gain` is a linear per-call amplitude. Without a device the call is a
    /// successful no-op and only the log entry is produced; the path is not
    /// touched.
    pub fn play_2d(&mut self, path: impl AsRef<Path>, gain: f32) -> Result<(), String> {
        let path = path.as_ref().to_path_buf();
        let gain = linear(gain);
        self.log.push(PlayEntry {
            path: path.clone(),
            kind: PlayKind::Sound2d,
            gain: gain * self.output_sfx_gain(),
            position: None,
        });
        let Some(device) = &self.device else {
            return Ok(());
        };
        let source = open_decoder(&path)?;
        let player = Player::connect_new(device.mixer());
        player.append(source.amplify(gain));
        player.set_volume(self.output_sfx_gain());
        self.sfx.retain(|player| !player.empty());
        self.sfx.push(player);
        Ok(())
    }

    /// Play a positional sound.
    ///
    /// `position` is the emitter and `listener` carries the ears' position and
    /// orientation; see the crate-level docs for the distance model. `gain` is
    /// a linear per-call amplitude. Without a device this is a successful
    /// no-op recorded in the log.
    pub fn play_3d(
        &mut self,
        path: impl AsRef<Path>,
        position: [f32; 3],
        listener: Listener,
        gain: f32,
    ) -> Result<(), String> {
        let path = path.as_ref().to_path_buf();
        let gain = linear(gain);
        self.log.push(PlayEntry {
            path: path.clone(),
            kind: PlayKind::Sound3d,
            gain: gain * self.output_sfx_gain(),
            position: Some(position),
        });
        let Some(device) = &self.device else {
            return Ok(());
        };
        let source = open_decoder(&path)?;
        let (left, right) = listener.ears();
        let player = Player::connect_new(device.mixer());
        player.append(Spatial::new(source, position, left, right).amplify(gain));
        player.set_volume(self.output_sfx_gain());
        self.sfx.retain(|player| !player.empty());
        self.sfx.push(player);
        Ok(())
    }

    /// Start the music track at `path`, optionally looping it.
    ///
    /// Any previous track is stopped first. The track is played at
    /// [`Audio::output_music_gain`]. Without a device this is a successful
    /// no-op recorded in the log.
    pub fn play_music(&mut self, path: impl AsRef<Path>, looping: bool) -> Result<(), String> {
        let path = path.as_ref().to_path_buf();
        self.stop_music();
        self.log.push(PlayEntry {
            path: path.clone(),
            kind: PlayKind::Music,
            gain: self.output_music_gain(),
            position: None,
        });
        let Some(device) = &self.device else {
            self.music_path = Some(path);
            return Ok(());
        };
        let file = File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let reader = BufReader::new(file);
        let player = Player::connect_new(device.mixer());
        if looping {
            let decoder =
                Decoder::new_looped(reader).map_err(|e| format!("{}: {e}", path.display()))?;
            player.append(decoder);
        } else {
            let decoder = Decoder::new(reader).map_err(|e| format!("{}: {e}", path.display()))?;
            player.append(decoder);
        }
        player.set_volume(self.output_music_gain());
        self.music = Some(player);
        self.music_path = Some(path);
        Ok(())
    }

    /// Stop the current music track, if any.
    pub fn stop_music(&mut self) {
        if let Some(player) = self.music.take() {
            player.stop();
        }
        self.music_path = None;
    }

    fn refresh_live_volumes(&self) {
        if let Some(music) = &self.music {
            music.set_volume(self.output_music_gain());
        }
        for player in &self.sfx {
            player.set_volume(self.output_sfx_gain());
        }
    }
}

impl Default for Audio {
    fn default() -> Audio {
        Audio::new()
    }
}

fn linear(gain: f32) -> f32 {
    if gain.is_finite() {
        gain.max(0.0)
    } else {
        0.0
    }
}

fn open_decoder(path: &Path) -> Result<Decoder<BufReader<File>>, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Decoder::new(BufReader::new(file)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-5,
            "expected {expected}, got {actual}"
        );
    }

    /// Duration from the header when the container has one; otherwise decode
    /// the stream and count samples. The shipped MP3s are raw MPEG frames with
    /// no Xing/VBRI header, so `rodio` (Symphonia) reports no duration for
    /// them — counting is the only honest measurement.
    fn duration_of<S: Source>(mut source: S) -> Duration {
        if let Some(duration) = source.total_duration() {
            return duration;
        }
        let channels = u64::from(source.channels().get());
        let rate = u64::from(source.sample_rate().get());
        let mut samples = 0u64;
        while source.next().is_some() {
            samples += 1;
        }
        Duration::from_secs_f64(samples as f64 / (channels * rate) as f64)
    }

    fn game_dir() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("RA2_GAME_DIR") {
            let dir = PathBuf::from(dir);
            if dir.join("Music").is_dir() {
                return Some(dir);
            }
        }
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join("local/game");
            if candidate.join("Music").is_dir() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    #[test]
    fn hundredths_of_a_decibel_convert_to_directsound_gain() {
        assert_eq!(hundredths_db_to_gain(0), 1.0);
        assert_eq!(hundredths_db_to_gain(-10_000), 0.0);
        assert_eq!(hundredths_db_to_gain(-20_000), 0.0);
        assert_eq!(hundredths_db_to_gain(600), 1.0);
        assert_close(hundredths_db_to_gain(-600), 0.501_187_2);
        assert_close(hundredths_db_to_gain(-2_000), 0.1);
        assert_close(hundredths_db_to_gain(-1_000), 0.316_227_77);
    }

    #[test]
    fn constructing_with_the_default_device_never_panics() {
        // On CI there is no device and `device()` is `None`; on a workstation
        // it opens the default device. Both are valid, and neither may panic.
        let audio = Audio::new();
        let _ = audio.device();
    }

    #[test]
    fn a_missing_device_is_safe_and_plays_are_logged() {
        let mut audio = Audio::without_device();
        assert!(audio.device().is_none());
        assert!(!audio.has_device());
        // With no device these do not touch the (missing) files.
        audio.play_2d("does-not-exist.wav", 0.5).unwrap();
        audio
            .play_3d(
                "also-missing.wav",
                [1.0, 2.0, 3.0],
                Listener::looking_forward([0.0; 3]),
                0.25,
            )
            .unwrap();
        audio.play_music("missing.mp3", true).unwrap();

        let log = audio.play_log();
        assert_eq!(log.len(), 3);
        assert_eq!(log.entries()[0].kind, PlayKind::Sound2d);
        assert_eq!(log.entries()[0].path, Path::new("does-not-exist.wav"));
        assert_eq!(log.entries()[1].kind, PlayKind::Sound3d);
        assert_eq!(log.entries()[1].position, Some([1.0, 2.0, 3.0]));
        assert_eq!(log.entries()[2].kind, PlayKind::Music);
    }

    #[test]
    fn play_log_records_the_effective_gain_and_position() {
        let mut audio = Audio::without_device();
        audio.set_sfx_gain(0.5);
        audio.set_music_gain(0.25);

        audio.play_2d("click.wav", 0.8).unwrap();
        audio
            .play_3d(
                "hit.wav",
                [4.0, 0.0, 0.0],
                Listener::looking_forward([0.0; 3]),
                0.5,
            )
            .unwrap();
        audio.play_music("theme.mp3", false).unwrap();

        {
            let log = audio.play_log();
            assert_close(log.entries()[0].gain, 0.4);
            assert_close(log.entries()[1].gain, 0.25);
            assert_eq!(log.entries()[1].position, Some([4.0, 0.0, 0.0]));
            assert_close(log.entries()[2].gain, 0.25);
        }

        audio.set_muted(true);
        audio.play_2d("click.wav", 1.0).unwrap();
        assert_eq!(audio.play_log().last().unwrap().gain, 0.0);
    }

    #[test]
    fn cfg_volumes_and_mutes_are_applied() {
        let cfg = Cfg::parse(
            "[Begin RA2 Config]\r\n\
SoundVolume=-600\r\n\
MusicVolume=-2000\r\n\
MusicMuted=1\r\n\
SoundMuted=0\r\n\
[End RA2 Config]\r\n",
        );
        let mut audio = Audio::without_device();
        audio.apply_cfg(&cfg);

        assert_close(audio.output_sfx_gain(), 0.501_187_2);
        assert_eq!(audio.output_music_gain(), 0.0);

        audio.apply_cfg(&Cfg::parse(
            "[Begin RA2 Config]\r\nSoundVolume=0\r\nMusicVolume=0\r\nSoundMuted=1\r\n[End RA2 Config]\r\n",
        ));
        assert_eq!(audio.output_sfx_gain(), 0.0);
        assert_eq!(audio.output_music_gain(), 1.0);
    }

    #[test]
    fn listener_ears_are_a_stereo_pair() {
        let (left, right) = Listener::looking_forward([0.0, 0.0, 0.0]).ears();
        assert!(
            left[0] < right[0],
            "left ear is -X for a +Z-facing listener"
        );
        assert_close((right[0] - left[0]) / 2.0, EAR_HALF_SPACING);
        assert!((left[1] - right[1]).abs() < 1e-6);
        assert!((left[2] - right[2]).abs() < 1e-6);

        let (left, right) = Listener::new([1.0, 2.0, 3.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]).ears();
        assert_ne!(left, right, "degenerate orientation still has two ears");
    }

    #[test]
    fn the_three_music_tracks_decode_with_plausible_formats() {
        let Some(game) = game_dir() else { return };
        for track in [MAIN_UI, TEAM_HQ, WORKSHOP] {
            let path = track.in_game_dir(&game);
            let decoder = open_decoder(&path).unwrap_or_else(|e| panic!("{e}"));
            let channels = decoder.channels().get();
            let rate = decoder.sample_rate().get();
            let duration = duration_of(decoder);
            assert!(
                (1..=2).contains(&channels),
                "{} has {channels} channels",
                path.display()
            );
            assert_eq!(rate, 44_100, "{} sample rate", path.display());
            assert!(
                duration > Duration::from_secs(10) && duration < Duration::from_secs(600),
                "{} is {duration:?}",
                path.display()
            );
        }
    }

    #[test]
    fn a_sample_of_the_shipped_wavs_decodes() {
        let Some(game) = game_dir() else { return };
        let sample = [
            "Sounds/cmp_generic.wav",
            "Sounds/cannon_fire.wav",
            "Sounds/bigmotor_loop.wav",
            "Sounds/crowd/normal_loop.wav",
            "Sounds/announcers/Arena_Combat_Enter.wav",
            "Sounds/button_click.wav",
        ];
        for relative in sample {
            let path = game.join(relative);
            let decoder = open_decoder(&path).unwrap_or_else(|e| panic!("{e}"));
            let channels = decoder.channels().get();
            let rate = decoder.sample_rate().get();
            let duration = duration_of(decoder);
            assert!(
                (1..=2).contains(&channels),
                "{} has {channels} channels",
                path.display()
            );
            assert!(
                (8_000..=96_000).contains(&rate),
                "{} has sample rate {rate}",
                path.display()
            );
            assert!(
                duration > Duration::from_millis(10),
                "{} is {duration:?}",
                path.display()
            );
        }
    }
}
