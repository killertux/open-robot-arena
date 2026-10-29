//! The robot copy a peer of the original sends in `giAddArenaBotMessage`
//! (capture `mp-payloads-joiner__live__000` in `local/oracle/original`,
//! docs/38 section 15): the shipped `.bot` in the current format, 1.12, with each
//! picture stored as TGA type `0x17` (the size, then a zlib stream). Skipped when the
//! capture or the game files are not on this machine.

use std::path::PathBuf;

use ra2fmt::bot;

fn up_to(name: &str) -> Option<PathBuf> {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// The bot text of the capture's one `0x12d` message.
fn captured_bot() -> Option<Vec<u8>> {
    let text = std::fs::read_to_string(up_to(
        "local/oracle/original/mp-payloads-joiner__live__000.txt",
    )?)
    .ok()?;
    for line in text.lines() {
        let hex = line.split(' ').nth(4)?;
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        if bytes.len() > 16 && bytes[..4] == [0x2d, 0x01, 0, 0] {
            return Some(bytes[16..].to_vec());
        }
    }
    None
}

#[test]
fn the_original_s_robot_copy_is_the_shipped_bot_with_compressed_pictures() {
    let (Some(wire), Some(file)) = (captured_bot(), up_to("local/game/Teams/Team0/Bot0.bot"))
    else {
        return;
    };
    let shipped = bot::parse(&std::fs::read(file).unwrap()).unwrap();
    let sent = bot::parse(&wire).unwrap();
    assert_eq!(sent.name, "C.R.U.S.H.");
    assert_eq!(sent.version, "1.12");
    // Loaded and saved again in the current format, the pictures plain. The
    // preview is the one change: the shipped file stores alpha 0 for the
    // background, the copy that travels 255, and the picture is drawn again.
    let (mut theirs, mut ours) = (sent.clone(), shipped.upgraded());
    let (a, b) = (
        theirs.snapshot.take().unwrap(),
        ours.snapshot.take().unwrap(),
    );
    same(&theirs.write(), &ours.write());
    assert_eq!(a.len(), b.len());
    let rgb =
        |p: &[u8]| -> Vec<u8> { p[18..].chunks(4).flat_map(|c| [c[0], c[1], c[2]]).collect() };
    assert!(a[..18] == b[..18], "header {:?} {:?}", &a[..18], &b[..18]);
    let (ra, rb) = (rgb(&a), rgb(&b));
    let n = ra.iter().zip(&rb).filter(|(x, y)| x != y).count();
    let first = ra.iter().zip(&rb).position(|(x, y)| x != y);
    // The preview is drawn again for the copy: the background and most of the
    // robot are the file's picture, not all of it.
    assert!(n * 4 < ra.len(), "{n} differing bytes, first at {first:?}");
    assert!(a[18..].chunks(4).all(|c| c[3] == 255));
    // Ours writes the wire form the original reads: the same robot, the same
    // pictures; zlib's output may differ in bytes, never in content.
    let mine = shipped.write_wire();
    let back = bot::parse(&mine).unwrap();
    assert!(back.snapshot == shipped.snapshot);
    same(&back.write(), &shipped.upgraded().write());
    assert!(back.components.len() == sent.components.len());
    assert!(
        mine.windows(3).any(|w| w == [0, 0, 0x17]),
        "type 0x17 pictures"
    );
    assert!(
        (mine.len() as f64 - wire.len() as f64).abs() < wire.len() as f64 * 0.05,
        "a compressed copy: {} vs {}",
        mine.len(),
        wire.len()
    );
}

/// Equal bytes, or where they first differ (not the bytes: they are large).
fn same(a: &[u8], b: &[u8]) {
    let at = a.iter().zip(b).position(|(x, y)| x != y);
    assert!(
        at.is_none() && a.len() == b.len(),
        "differ at {at:?} (lengths {} and {})",
        a.len(),
        b.len()
    );
}
