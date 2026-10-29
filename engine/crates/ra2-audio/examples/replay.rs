//! Render the ORIGINAL's own sound-call log (`tools/oracle/audio_events.py` TSV) through
//! the model our client uses (flat, centred, DirectSound hundredths of a dB gain, loops,
//! linear-in-dB fades), so its level can be set against the recording of the same run.
//!
//! ```text
//! replay EVENTS.tsv SOUNDS_DIR OUT.wav [seconds]
//! ```
//! What is left over between this render and the recording is mixer-level (3D, pitch,
//! a gain rule we are missing); the events themselves are the original's.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

use rodio::{Decoder, Source};

const RATE: usize = 48_000;

/// Samples at 48 kHz and the file's own rate.
fn load(path: &Path) -> (Vec<f32>, f32) {
    let dec = Decoder::new(BufReader::new(File::open(path).unwrap())).unwrap();
    let ch = dec.channels().get() as usize;
    let rate = dec.sample_rate().get() as f64;
    let raw: Vec<f32> = dec.collect();
    let frames = raw.len() / ch;
    let n = (frames as f64 * RATE as f64 / rate) as usize;
    let samples: Vec<f32> = (0..n)
        .map(|i| {
            let p = i as f64 * rate / RATE as f64;
            let i0 = (p as usize).min(frames - 1);
            let i1 = (i0 + 1).min(frames - 1);
            let f = (p - i0 as f64) as f32;
            let m = |k: usize| (0..ch).map(|c| raw[k * ch + c]).sum::<f32>() / ch as f32;
            m(i0) * (1.0 - f) + m(i1) * f
        })
        .collect();
    (samples, rate as f32)
}

fn find(dir: &Path, name: &str, out: &mut Option<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            find(&p, name, out);
        } else if p.file_name().unwrap().to_string_lossy().to_lowercase() == name {
            *out = Some(p);
        }
    }
}

#[derive(Default)]
struct Handle {
    file: String,
    /// (start, end, from hdb, to hdb)
    gain: Vec<(f64, f64, f64, f64)>,
    /// (start, end or inf, looping)
    spans: Vec<(f64, f64, bool)>,
    /// (time, Hz): `SetFrequency` calls
    freq: Vec<(f64, f64)>,
}

impl Handle {
    fn hdb_at(&self, t: f64) -> f64 {
        let mut v = 0.0;
        for &(a, b, f, g) in &self.gain {
            if a > t {
                break;
            }
            v = if b <= a || t >= b {
                g
            } else {
                f + (g - f) * (t - a) / (b - a)
            };
        }
        v
    }
    /// Playback rate relative to the file's own, at `t`.
    fn rate_at(&self, t: f64, native: f64) -> f64 {
        let i = self.freq.partition_point(|&(ft, _)| ft <= t);
        if i == 0 {
            1.0
        } else {
            self.freq[i - 1].1 / native
        }
    }
    fn cur(&self, t: f64) -> f64 {
        self.hdb_at(t)
    }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let dur: f64 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(150.0);
    let mut handles: HashMap<String, Handle> = HashMap::new();
    for line in std::fs::read_to_string(&a[0]).unwrap().lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        let t: f64 = f[0].parse().unwrap();
        let h = f[4].to_string();
        let vol: Option<f64> = f[3].parse().ok();
        let note = f.get(5).copied().unwrap_or("");
        match f[1] {
            "create" => {
                let e = handles.entry(h).or_default();
                e.file = f[2].to_lowercase();
                e.gain.clear();
                e.spans.clear();
                e.gain.push((t, t, vol.unwrap_or(0.0), vol.unwrap_or(0.0)));
            }
            "play" | "loop" => {
                if let Some(e) = handles.get_mut(&h) {
                    let looping = f[1] == "loop";
                    let busy = e.spans.last().is_some_and(|s| s.1 > t);
                    if busy {
                        continue;
                    }
                    e.spans.push((t, f64::INFINITY, looping));
                }
            }
            "stop" => {
                if let Some(e) = handles.get_mut(&h) {
                    if let Some(s) = e.spans.last_mut() {
                        s.1 = s.1.min(t);
                    }
                }
            }
            "freq" => {
                if let (Some(e), Some(hz)) = (handles.get_mut(&h), vol) {
                    e.freq.push((t, hz));
                }
            }
            "setvolume" => {
                if let Some(e) = handles.get_mut(&h) {
                    let ms: f64 = note.parse().unwrap_or(0.0);
                    let from = e.cur(t);
                    let to = vol.unwrap_or(0.0);
                    e.gain.push((t, t + ms / 1000.0, from, to));
                }
            }
            "fadein" => {
                if let Some(e) = handles.get_mut(&h) {
                    let ms: f64 = note.parse().unwrap_or(0.0);
                    e.gain
                        .push((t, t + ms / 1000.0, -10000.0, vol.unwrap_or(0.0)));
                    e.spans.push((t, f64::INFINITY, true));
                }
            }
            "fadeout" => {
                if let Some(e) = handles.get_mut(&h) {
                    let ms: f64 = note.parse().unwrap_or(0.0);
                    let from = e.cur(t);
                    e.gain.push((t, t + ms / 1000.0, from, -10000.0));
                    if let Some(s) = e.spans.last_mut() {
                        s.1 = s.1.min(t + ms / 1000.0);
                    }
                }
            }
            _ => {}
        }
    }
    let mut cache: HashMap<String, (Vec<f32>, f32)> = HashMap::new();
    let n = (dur * RATE as f64) as usize;
    let mut mix = vec![0f32; n * 2];
    let mut miss = 0;
    for h in handles.values() {
        if h.file.is_empty() || h.spans.is_empty() {
            continue;
        }
        let key = h.file.clone();
        if !cache.contains_key(&key) {
            let mut p = None;
            find(Path::new(&a[1]), &key, &mut p);
            let Some(p) = p else {
                miss += 1;
                eprintln!("missing: {key}");
                cache.insert(key.clone(), (Vec::new(), 1.0));
                continue;
            };
            cache.insert(key.clone(), load(&p));
        }
        let (wav, native) = &cache[&key];
        let native = *native as f64;
        if wav.is_empty() {
            continue;
        }
        for &(s, e, looping) in &h.spans {
            let first = (s * RATE as f64) as usize;
            let last = ((e.min(dur)) * RATE as f64) as usize;
            let mut i = first;
            let mut pos = 0.0f64;
            while i < last.min(n) {
                if pos as usize + 1 >= wav.len() {
                    if !looping || pos == 0.0 {
                        break;
                    }
                    pos = 0.0;
                }
                let k = pos as usize;
                let frac = (pos - k as f64) as f32;
                let sample = wav[k] * (1.0 - frac) + wav[k + 1] * frac;
                let hdb = h.hdb_at(i as f64 / RATE as f64);
                let g = if hdb <= -10000.0 {
                    0.0
                } else {
                    10f32.powf(hdb as f32 / 2000.0)
                };
                let v = sample * g;
                mix[i * 2] += v;
                mix[i * 2 + 1] += v;
                i += 1;
                pos += if h.freq.is_empty() {
                    1.0
                } else {
                    h.rate_at(i as f64 / RATE as f64, native)
                };
            }
        }
    }
    if miss > 0 {
        eprintln!("{miss} files not found");
    }
    let mut f = File::create(&a[2]).unwrap();
    let dl = (mix.len() * 2) as u32;
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + dl).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&(RATE as u32).to_le_bytes()).unwrap();
    f.write_all(&(RATE as u32 * 4).to_le_bytes()).unwrap();
    f.write_all(&4u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&dl.to_le_bytes()).unwrap();
    for s in &mix {
        f.write_all(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
            .unwrap();
    }
}
