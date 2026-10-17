//! Print the format of one or more audio files without opening a sound device.
//!
//! ```text
//! cargo run -p ra2-audio --example decode -- "local/game/Music/Main UI/Enter the Battlefield.mp3"
//! ```

use std::fs::File;
use std::io::BufReader;
use std::process::ExitCode;

use rodio::{Decoder, Source};

fn main() -> ExitCode {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: decode <file>...");
        return ExitCode::FAILURE;
    }
    let mut failed = false;
    for path in paths {
        if let Err(error) = decode(&path) {
            eprintln!("{error}");
            failed = true;
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn decode(path: &str) -> Result<(), String> {
    let file = File::open(path).map_err(|e| format!("{path}: {e}"))?;
    let decoder = Decoder::new(BufReader::new(file)).map_err(|e| format!("{path}: {e}"))?;
    let channels = decoder.channels().get();
    let rate = decoder.sample_rate().get();
    let duration = duration_of(decoder);
    let seconds = duration.as_secs_f64();
    let codec = path
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_uppercase())
        .unwrap_or_else(|| "?".to_string());
    println!(
        "{path}\n  {codec}: {:.3} s ({}:{:05.2}), {} Hz, {} channel(s)",
        seconds,
        (seconds / 60.0) as u64,
        seconds % 60.0,
        rate,
        channels
    );
    Ok(())
}

/// The shipped MP3s are raw MPEG frames without a Xing/VBRI header, so the
/// decoder cannot report a duration up front. Decode and count instead.
fn duration_of<S: Source>(mut source: S) -> std::time::Duration {
    if let Some(duration) = source.total_duration() {
        return duration;
    }
    let channels = u64::from(source.channels().get());
    let rate = u64::from(source.sample_rate().get());
    let mut samples = 0u64;
    while source.next().is_some() {
        samples += 1;
    }
    std::time::Duration::from_secs_f64(samples as f64 / (channels * rate) as f64)
}
