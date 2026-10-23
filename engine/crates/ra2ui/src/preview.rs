//! The robot pictures the game stores inside a `.bot` file.
//!
//! A `.bot` file carries a raw uncompressed TGA snapshot (the format's
//! `snapshot` blob, normally 128×128). The original draws that picture in the
//! result cell's `bot preview` and the HQ inventory's `bot snapshot img`; it is
//! not a file name, so no `.gib` can resolve it on its own.
//!
//! This module decodes the blob into the RGBA8 rows the UI renderer takes, and
//! gives it a synthetic texture key. The key is content-addressed, so a long
//! lived client can cache it: the same picture always has the same key, and a
//! different picture never reuses one.

/// A decoded picture: RGBA8, top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Preview {
    /// Decode a `.bot`'s embedded snapshot. `None` when the blob is not a TGA
    /// the image crate can read.
    pub fn decode_tga(bytes: &[u8]) -> Option<Preview> {
        let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Tga).ok()?;
        let image = image.into_rgba8();
        let (width, height) = image.dimensions();
        if width == 0 || height == 0 {
            return None;
        }
        Some(Preview {
            width,
            height,
            rgba: image.into_raw(),
        })
    }

    /// The synthetic texture key this picture is registered under.
    ///
    /// `label` distinguishes the same picture in two places (the results cell
    /// and the inventory), so the key is stable per use and per content. The
    /// format is `#bot-preview:<label>-<hash>`; `#` keeps it out of the game's
    /// file namespace, and `UiAssets` resolves keys it has registered before it
    /// looks at the filesystem.
    pub fn key(&self, label: &str) -> String {
        format!("#bot-preview:{label}-{:016x}", self.digest())
    }

    /// FNV-1a over the dimensions and pixels. A stable, dependency-free content
    /// hash is enough: the key only has to change when the picture does.
    fn digest(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in self
            .width
            .to_le_bytes()
            .iter()
            .chain(self.height.to_le_bytes().iter())
            .chain(self.rgba.iter())
        {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    /// Whether every pixel is opaque white. The shipped pictures are not; a
    /// blank decode is what a test uses to catch a key that resolved to the
    /// one-pixel white fill instead of the picture.
    pub fn is_white(&self) -> bool {
        self.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p == &[255, 255, 255, 255])
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn game_dir() -> Option<PathBuf> {
        ra2fmt::saves::isolate_for_tests();
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

    fn snapshot(game: &std::path::Path, relative: &str) -> Preview {
        let bytes = std::fs::read(game.join(relative)).unwrap();
        let bot = ra2fmt::bot::parse(&bytes).unwrap();
        Preview::decode_tga(bot.snapshot.as_deref().expect("a snapshot"))
            .expect("a TGA the image crate reads")
    }

    /// A shipped bot's snapshot decodes at the dimensions the file stores, and
    /// it is a real picture rather than the white fill.
    #[test]
    fn a_shipped_bots_snapshot_decodes_to_its_stored_dimensions() {
        let Some(game) = game_dir() else { return };
        let picture = snapshot(&game, "AI/Team1/Bot0.bot");
        assert_eq!((picture.width, picture.height), (128, 128));
        assert_eq!(picture.rgba.len(), 128 * 128 * 4);
        assert!(!picture.is_white(), "the snapshot has real pixels");
        assert!(
            picture
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[0] != p[1] || p[1] != p[2]),
            "the snapshot carries colour, not a grey silhouette"
        );
    }

    /// Two different robots get different keys for the same label, and the
    /// same picture always gets the same key.
    #[test]
    fn a_preview_key_is_content_addressed() {
        let Some(game) = game_dir() else { return };
        let one = snapshot(&game, "AI/Team1/Bot0.bot");
        let two = snapshot(&game, "AI/Team1/Bot1.bot");
        assert_eq!(one.key("result-0"), one.key("result-0"));
        assert_ne!(one.key("result-0"), two.key("result-0"));
        assert_ne!(one.key("result-0"), one.key("hq-0"));
        assert!(one.key("result-0").starts_with("#bot-preview:result-0-"));
    }
}
