#![allow(clippy::needless_range_loop)]
//! Offline audio analysis used to compare our output with the recordings of the
//! original game's own audio stream.
//!
//! ```text
//! wavtool decode  <in.{mp3,wav}> <out.wav>   # 48 kHz stereo 16-bit
//! wavtool stats   <a.wav>                    # per-second RMS/peak, band levels, onsets
//! wavtool xcorr   <ref.wav> <cand.wav>       # best envelope lag + correlation
//! wavtool spectra <ref.wav> <cand.wav> [t0 t1]  # 1/3-octave-ish band levels, and their correlation
//! ```

use std::fs::File;
use std::io::{BufReader, Write};

use rodio::{Decoder, Source};

pub const RATE: u32 = 48_000;

/// Interleaved stereo f32 at 48 kHz.
pub fn load(path: &str) -> Vec<f32> {
    let file = File::open(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let dec = Decoder::new(BufReader::new(file)).unwrap_or_else(|e| panic!("{path}: {e}"));
    let ch = dec.channels().get() as usize;
    let rate = dec.sample_rate().get();
    let raw: Vec<f32> = dec.collect();
    let frames = raw.len() / ch;
    let get = |f: usize, c: usize| raw[f * ch + c.min(ch - 1)];
    let out_frames = (frames as u64 * RATE as u64 / rate as u64) as usize;
    let mut out = Vec::with_capacity(out_frames * 2);
    for i in 0..out_frames {
        let pos = i as f64 * rate as f64 / RATE as f64;
        let i0 = pos as usize;
        let fr = (pos - i0 as f64) as f32;
        let i1 = (i0 + 1).min(frames - 1);
        for c in 0..2 {
            out.push(get(i0, c) * (1.0 - fr) + get(i1, c) * fr);
        }
    }
    out
}

pub fn write_wav(path: &str, samples: &[f32]) {
    let mut f = File::create(path).unwrap();
    let data_len = (samples.len() * 2) as u32;
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&RATE.to_le_bytes()).unwrap();
    f.write_all(&(RATE * 4).to_le_bytes()).unwrap();
    f.write_all(&4u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    for s in samples {
        f.write_all(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
            .unwrap();
    }
}

fn db(x: f64) -> f64 {
    20.0 * (x + 1e-9).log10()
}

/// Mono envelope, one value per `hop` frames (RMS).
fn envelope(s: &[f32], hop: usize) -> Vec<f64> {
    s.chunks(hop * 2)
        .map(|c| {
            let e: f64 = c.iter().map(|&x| (x as f64) * (x as f64)).sum();
            (e / c.len() as f64).sqrt()
        })
        .collect()
}

/// Band levels (dB) via Goertzel at octave-spaced centres.
const BANDS: [f64; 9] = [
    63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

fn bands(s: &[f32], t0: f64, t1: f64) -> Vec<f64> {
    let a = (t0 * RATE as f64) as usize * 2;
    let b = ((t1 * RATE as f64) as usize * 2).min(s.len());
    let mono: Vec<f64> = s[a.min(b)..b]
        .chunks(2)
        .map(|c| (c[0] as f64 + c[1] as f64) * 0.5)
        .collect();
    BANDS
        .iter()
        .map(|&fc| {
            // average a few Goertzel bins across the octave band
            let mut p = 0.0;
            let n = 5;
            for k in 0..n {
                let f = fc * 2f64.powf((k as f64 / (n - 1) as f64 - 0.5) * 0.9);
                let w = 2.0 * std::f64::consts::PI * f / RATE as f64;
                let coeff = 2.0 * w.cos();
                let (mut s1, mut s2) = (0.0, 0.0);
                for &x in &mono {
                    let s0 = x + coeff * s1 - s2;
                    s2 = s1;
                    s1 = s0;
                }
                p += s1 * s1 + s2 * s2 - coeff * s1 * s2;
            }
            10.0 * (p / n as f64 / (mono.len().max(1) as f64).powi(2) + 1e-18).log10()
        })
        .collect()
}

fn stats(path: &str) {
    let s = load(path);
    let secs = s.len() as f64 / 2.0 / RATE as f64;
    println!("{path}: {secs:.2} s");
    for sec in 0..(secs.ceil() as usize) {
        let a = sec * RATE as usize * 2;
        let b = ((sec + 1) * RATE as usize * 2).min(s.len());
        if a >= b {
            break;
        }
        let seg = &s[a..b];
        let rms = (seg.iter().map(|&x| (x as f64).powi(2)).sum::<f64>() / seg.len() as f64).sqrt();
        let pk = seg.iter().fold(0f32, |m, &x| m.max(x.abs())) as f64;
        println!("  {sec:3}s rms {:6.1} dB  peak {:6.1} dB", db(rms), db(pk));
    }
    let all: f64 = (s.iter().map(|&x| (x as f64).powi(2)).sum::<f64>() / s.len() as f64).sqrt();
    println!("  overall rms {:.1} dB", db(all));
    let bd = bands(&s, 0.0, secs);
    println!(
        "  bands {:?}",
        BANDS
            .iter()
            .zip(&bd)
            .map(|(f, d)| format!("{f}:{d:.0}"))
            .collect::<Vec<_>>()
    );
    // onsets: 10 ms envelope jumps
    let env = envelope(&s, RATE as usize / 100);
    let mut last = -1.0f64;
    print!("  onsets (s):");
    for i in 3..env.len() {
        let prev = env[i - 3..i].iter().cloned().fold(0.0, f64::max).max(1e-4);
        if env[i] > prev * 3.0 && env[i] > 0.05 && i as f64 * 0.01 - last > 0.15 {
            print!(" {:.2}", i as f64 * 0.01);
            last = i as f64 * 0.01;
        }
    }
    println!();
}

fn xcorr(a: &str, b: &str) {
    let (x, y) = (load(a), load(b));
    let hop = RATE as usize / 50;
    let (ex, ey) = (envelope(&x, hop), envelope(&y, hop));
    let norm = |v: &[f64]| -> Vec<f64> {
        let lx: Vec<f64> = v.iter().map(|&e| db(e).max(-80.0)).collect();
        let m = lx.iter().sum::<f64>() / lx.len() as f64;
        lx.iter().map(|&e| e - m).collect()
    };
    let (nx, ny) = (norm(&ex), norm(&ey));
    let max_lag = 50 * 400;
    let mut best = (0i64, -2.0);
    for lag in -(max_lag as i64)..=(max_lag as i64) {
        let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
        let mut overlap = 0usize;
        for i in 0..nx.len() {
            let j = i as i64 + lag;
            if j < 0 || j as usize >= ny.len() {
                continue;
            }
            overlap += 1;
            let (p, q) = (nx[i], ny[j as usize]);
            sxy += p * q;
            sxx += p * p;
            syy += q * q;
        }
        if overlap * 10 >= nx.len().min(ny.len()) * 8 && sxx > 0.0 && syy > 0.0 {
            let r = sxy / (sxx * syy).sqrt();
            if r > best.1 {
                best = (lag, r);
            }
        }
    }
    println!(
        "best lag {:.2} s (cand starts {:.2} s into ref offset), envelope correlation {:.3}",
        best.0 as f64 / 50.0,
        best.0 as f64 / 50.0,
        best.1
    );
}

fn spectra(a: &str, b: &str, t0: Option<f64>, t1: Option<f64>) {
    let (x, y) = (load(a), load(b));
    let secs = |s: &Vec<f32>| s.len() as f64 / 2.0 / RATE as f64;
    let t0 = t0.unwrap_or(0.0);
    let t1 = t1.unwrap_or(secs(&x).min(secs(&y)));
    let (bx, by) = (bands(&x, t0, t1), bands(&y, t0, t1));
    println!("band(Hz)  ref(dB)  cand(dB)  diff");
    for i in 0..BANDS.len() {
        println!(
            "{:7.0} {:8.1} {:8.1} {:6.1}",
            BANDS[i],
            bx[i],
            by[i],
            by[i] - bx[i]
        );
    }
    let mx = bx.iter().sum::<f64>() / bx.len() as f64;
    let my = by.iter().sum::<f64>() / by.len() as f64;
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for i in 0..bx.len() {
        sxy += (bx[i] - mx) * (by[i] - my);
        sxx += (bx[i] - mx).powi(2);
        syy += (by[i] - my).powi(2);
    }
    println!("spectral-shape correlation {:.3}", sxy / (sxx * syy).sqrt());
}

fn fft(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = 2.0 * std::f64::consts::PI / len as f64 * if inverse { 1.0 } else { -1.0 };
        let (wr, wi) = (ang.cos(), ang.sin());
        for i in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let (a, b) = (i + k, i + k + len / 2);
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

/// Mono waveform at 48000/24 = 2 kHz (box-filtered).
fn lowwave(s: &[f32]) -> Vec<f64> {
    s.chunks(48)
        .map(|c| c.iter().map(|&x| x as f64).sum::<f64>() / c.len() as f64)
        .collect()
}

/// Waveform cross-correlation: is `cand` (any offset) the same audio as `refw`?
fn wcorr(a: &str, b: &str) {
    let (mut x, mut y) = (lowwave(&load(a)), lowwave(&load(b)));
    let swapped = y.len() < x.len();
    if swapped {
        std::mem::swap(&mut x, &mut y);
    }
    let n = (x.len() + y.len()).next_power_of_two();
    let pad = |v: &[f64]| {
        let mut r = v.to_vec();
        r.resize(n, 0.0);
        r
    };
    let (mut xr, mut xi) = (pad(&x), vec![0.0; n]);
    let (mut yr, mut yi) = (pad(&y), vec![0.0; n]);
    fft(&mut xr, &mut xi, false);
    fft(&mut yr, &mut yi, false);
    // corr[l] = sum_i x[i] * y[i + l]  => IFFT(conj(X) * Y)
    let mut cr = vec![0.0; n];
    let mut ci = vec![0.0; n];
    for k in 0..n {
        cr[k] = xr[k] * yr[k] + xi[k] * yi[k];
        ci[k] = xr[k] * yi[k] - xi[k] * yr[k];
    }
    fft(&mut cr, &mut ci, true);
    let ex: f64 = x.iter().map(|v| v * v).sum();
    // running energy of y for normalisation
    let mut pre = vec![0.0; y.len() + 1];
    for i in 0..y.len() {
        pre[i + 1] = pre[i] + y[i] * y[i];
    }
    let mut best = (0i64, 0.0f64);
    for lag in -(x.len() as i64 / 10)..=(y.len() as i64) {
        let idx = if lag >= 0 {
            lag as usize
        } else {
            (n as i64 + lag) as usize
        };
        let (lo, hi) = (
            (lag.max(0)) as usize,
            ((lag + x.len() as i64).min(y.len() as i64)).max(0) as usize,
        );
        if hi <= lo || (hi - lo) * 10 < x.len() * 9 {
            continue;
        }
        let ey = pre[hi] - pre[lo];
        let r = cr[idx] / n as f64 / (ex * ey).sqrt().max(1e-12);
        if r > best.1 {
            best = (lag, r);
        }
    }
    println!(
        "waveform: shorter file starts {:.3} s into the longer one ({}), normalised correlation {:.3}",
        best.0 as f64 / 2000.0,
        if swapped { "cand in ref" } else { "ref in cand" },
        best.1
    );
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(String::as_str) {
        Some("decode") => write_wav(&a[2], &load(&a[1])),
        Some("stats") => stats(&a[1]),
        Some("wcorr") => wcorr(&a[1], &a[2]),
        Some("xcorr") => xcorr(&a[1], &a[2]),
        Some("spectra") => spectra(
            &a[1],
            &a[2],
            a.get(3).and_then(|v| v.parse().ok()),
            a.get(4).and_then(|v| v.parse().ok()),
        ),
        _ => eprintln!("usage: wavtool decode|stats|xcorr|spectra ..."),
    }
}
