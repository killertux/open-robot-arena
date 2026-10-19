//! Offline capture and rendering of everything an [`Audio`](crate::Audio) was
//! asked to play, to a WAV file.
//!
//! The original's audio can only be judged against recordings of its own
//! stream, so the engine needs the same thing from its side: a capturing
//! [`Audio`](crate::Audio) has no output device, keeps a clock that the caller
//! advances with simulated time, and records every start, stop and gain change
//! with its time. [`Capture::render`] then mixes those into 48 kHz stereo `f32`
//! with the same decoders and the same positional model as the live path.
//!
//! The mix is a model of the mixer, not a bit-exact DirectSound: gains are
//! piecewise-linear between the recorded points, a looped voice repeats its
//! file, and a positional voice uses the listener it started with.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

use rodio::source::Spatial;
use rodio::{Decoder, Source};

use crate::Listener;

/// The render rate and channel count.
pub const RENDER_RATE: u32 = 48_000;

/// What kind of voice started.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VoiceKind {
    /// Music: the decoder's own stereo, no panning.
    Music,
    /// A non-positional sound: the decoder's own channels.
    Flat,
    /// A positional sound heard from `listener`.
    Positional {
        position: [f32; 3],
        listener: Listener,
    },
}

#[derive(Clone, Debug)]
struct Voice {
    start: f32,
    stop: Option<f32>,
    path: PathBuf,
    looping: bool,
    kind: VoiceKind,
    /// `(time, linear gain)` points, in time order; the first is at `start`.
    gain: Vec<(f32, f32)>,
    /// Seconds into the file at which playback starts.
    offset: f32,
    /// `(time, playback rate)` points: a pitch change (`SetFrequency`) is a
    /// rate change of the whole voice. Empty means unity.
    speed: Vec<(f32, f32)>,
}

/// The recorded timeline.
#[derive(Clone, Debug, Default)]
pub struct Capture {
    voices: Vec<Voice>,
    live_script: BTreeMap<u64, usize>,
    live_music: Option<usize>,
}

impl Capture {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start(
        &mut self,
        t: f32,
        path: &Path,
        looping: bool,
        kind: VoiceKind,
        gain: f32,
        script_id: Option<u64>,
        is_music: bool,
    ) {
        let index = self.voices.len();
        self.voices.push(Voice {
            start: t,
            stop: None,
            path: path.to_path_buf(),
            looping,
            kind,
            gain: vec![(t, gain)],
            offset: 0.0,
            speed: Vec::new(),
        });
        if let Some(id) = script_id {
            self.live_script.insert(id, index);
        }
        if is_music {
            self.live_music = Some(index);
        }
    }

    pub(crate) fn set_gain(&mut self, t: f32, script_id: Option<u64>, music: bool, gain: f32) {
        let index = if music {
            self.live_music
        } else {
            script_id.and_then(|id| self.live_script.get(&id).copied())
        };
        if let Some(voice) = index.and_then(|i| self.voices.get_mut(i)) {
            if voice.stop.is_none() {
                voice.gain.push((t, gain));
            }
        }
    }

    pub(crate) fn set_speed(&mut self, t: f32, script_id: u64, speed: f32) {
        let index = self.live_script.get(&script_id).copied();
        if let Some(voice) = index.and_then(|i| self.voices.get_mut(i)) {
            if voice.stop.is_none() {
                if voice.speed.is_empty() {
                    voice.speed.push((voice.start, 1.0));
                }
                voice.speed.push((t, speed));
            }
        }
    }

    pub(crate) fn stop_script(&mut self, t: f32, id: u64) {
        if let Some(i) = self.live_script.remove(&id) {
            self.voices[i].stop.get_or_insert(t);
        }
    }

    pub(crate) fn stop_music(&mut self, t: f32) {
        if let Some(i) = self.live_music.take() {
            self.voices[i].stop.get_or_insert(t);
        }
    }

    /// Skip `seconds` into the file of every music voice (used to line a render
    /// up with a recording that began mid-track).
    pub fn set_music_offset(&mut self, seconds: f32) {
        for voice in &mut self.voices {
            if voice.kind == VoiceKind::Music {
                voice.offset = seconds;
            }
        }
    }

    /// Whether a script voice started and has not been stopped.
    pub fn is_script_live(&self, id: u64) -> bool {
        self.live_script.contains_key(&id)
    }

    /// The number of voices started.
    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }

    /// How many playback-rate points were recorded across all voices.
    pub fn speed_points(&self) -> usize {
        self.voices.iter().map(|v| v.speed.len()).sum()
    }

    /// Start times and paths, for a report.
    pub fn starts(&self) -> Vec<(f32, PathBuf)> {
        self.voices
            .iter()
            .map(|v| (v.start, v.path.clone()))
            .collect()
    }

    /// Mix `seconds` of the timeline into interleaved 48 kHz stereo.
    pub fn render(&self, seconds: f32) -> Vec<f32> {
        let frames = (seconds * RENDER_RATE as f32) as usize;
        let mut out = vec![0.0f32; frames * 2];
        for voice in &self.voices {
            self.mix_voice(voice, &mut out);
        }
        out
    }

    fn mix_voice(&self, voice: &Voice, out: &mut [f32]) {
        let first = (voice.start * RENDER_RATE as f32) as usize;
        if first * 2 >= out.len() {
            return;
        }
        let Some((samples, rate)) = decode_voice(voice) else {
            return;
        };
        let frames_out = out.len() / 2;
        let stop = voice
            .stop
            .map(|s| (s * RENDER_RATE as f32) as usize)
            .unwrap_or(frames_out)
            .min(frames_out);
        let skip = (voice.offset * rate as f32) as usize;
        let frames_in = samples.len() / 2;
        if frames_in == 0 {
            return;
        }
        let mut gi = 0usize;
        let mut si = 0usize;
        // Position in input frames; advanced by the current rate each output
        // frame so a pitch change bends the voice instead of jumping it.
        let mut pos = skip as f32;
        for f in first..stop {
            let t = f as f32 / RENDER_RATE as f32;
            while gi + 1 < voice.gain.len() && voice.gain[gi + 1].0 <= t {
                gi += 1;
            }
            let gain = if gi + 1 < voice.gain.len() {
                let (t0, g0) = voice.gain[gi];
                let (t1, g1) = voice.gain[gi + 1];
                if t1 > t0 {
                    g0 + (g1 - g0) * ((t - t0) / (t1 - t0)).clamp(0.0, 1.0)
                } else {
                    g1
                }
            } else {
                voice.gain[gi].1
            };
            let speed = if voice.speed.is_empty() {
                1.0
            } else {
                while si + 1 < voice.speed.len() && voice.speed[si + 1].0 <= t {
                    si += 1;
                }
                if si + 1 < voice.speed.len() {
                    let (t0, v0) = voice.speed[si];
                    let (t1, v1) = voice.speed[si + 1];
                    if t1 > t0 {
                        v0 + (v1 - v0) * ((t - t0) / (t1 - t0)).clamp(0.0, 1.0)
                    } else {
                        v1
                    }
                } else {
                    voice.speed[si].1
                }
            };
            let here = pos;
            pos += rate as f32 / RENDER_RATE as f32 * speed;
            let pos = here;
            let mut i0 = pos as usize;
            let frac = pos - i0 as f32;
            if voice.looping {
                i0 %= frames_in;
            } else if i0 >= frames_in {
                continue;
            }
            let i1 = if voice.looping {
                (i0 + 1) % frames_in
            } else {
                (i0 + 1).min(frames_in - 1)
            };
            for c in 0..2 {
                let a = samples[i0 * 2 + c];
                let b = samples[i1 * 2 + c];
                out[f * 2 + c] += (a + (b - a) * frac) * gain;
            }
        }
    }
}

/// Decode a voice to interleaved stereo at its own rate, applying the
/// positional model for a positional voice.
fn decode_voice(voice: &Voice) -> Option<(Vec<f32>, u32)> {
    let file = File::open(&voice.path).ok()?;
    let decoder = Decoder::new(BufReader::new(file)).ok()?;
    let rate = decoder.sample_rate().get();
    let channels = decoder.channels().get() as usize;
    match voice.kind {
        VoiceKind::Positional { position, listener } => {
            let (left, right) = listener.ears();
            let spatial = Spatial::new(decoder, position, left, right);
            let rate = spatial.sample_rate().get();
            // DirectSound3D rolls amplitude off as 1/distance beyond one unit
            // where the spatial model alone gives 1/distance squared.
            let reach = ((position[0] - listener.position[0]).powi(2)
                + (position[1] - listener.position[1]).powi(2)
                + (position[2] - listener.position[2]).powi(2))
            .sqrt()
            .max(1.0);
            Some((spatial.map(|s| s * reach).collect(), rate))
        }
        _ => {
            let raw: Vec<f32> = decoder.collect();
            if channels == 2 {
                Some((raw, rate))
            } else if channels == 1 {
                Some((raw.iter().flat_map(|&s| [s, s]).collect(), rate))
            } else {
                Some((
                    raw.chunks(channels)
                        .flat_map(|c| [c[0], c[channels.min(2) - 1]])
                        .collect(),
                    rate,
                ))
            }
        }
    }
}

/// Write interleaved 48 kHz stereo `f32` as a 16-bit WAV.
pub fn write_wav(path: &Path, samples: &[f32]) -> std::io::Result<()> {
    let mut f = File::create(path)?;
    let data_len = (samples.len() * 2) as u32;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&RENDER_RATE.to_le_bytes())?;
    f.write_all(&(RENDER_RATE * 4).to_le_bytes())?;
    f.write_all(&4u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for s in samples {
        f.write_all(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
    }
    Ok(())
}
