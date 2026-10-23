//! The game's own settings file, `ra2.cfg`.
//!
//! The shipped file is a small INI-like document written by the game's own
//! options screens. It is deliberately *not* re-serialised from a decoded
//! schema: the game only ever edits a key's value, so a settings writer that
//! rewrites the whole file would reorder sections, re-case keys and discard
//! unknown keys the client does not understand. This module instead keeps every
//! physical line, in order, and edits only the value of the requested key.
//!
//! # Shape
//!
//! ```text
//! [Begin Graphics Config]
//! FullScreen=1
//! ...
//! [End Graphics Config]
//! [Begin RA2 Config]
//! SoundVolume=-600
//! ...
//! [End RA2 Config]
//! [Begin Multiplayer Config]
//! [End Multiplayer Config]
//! [Begin Input Config]
//! [End Input Config]
//! ```
//!
//! Sections are `[Begin <name>]` / `[End <name>]`; inside one, every ordinary
//! line is `Key=Value`. The shipped file has four sections, two of them empty,
//! and no blank lines or comments. Sections, keys and duplicate keys are all
//! kept in the order they were read; a line that is none of the above (a
//! comment, a blank line) is kept verbatim as an uninterpreted line.
//!
//! # Line endings and the final newline
//!
//! `local/game/ra2.cfg` is **CRLF throughout**, and its last `[End Input
//! Config]` line *is* terminated by CRLF: 649 bytes, 32 lines, 32 CRLFs. The
//! parser detects CRLF (any `\r\n` at all) and remembers whether the text ended
//! with a newline, so [`Cfg::to_text`] reproduces the input byte-for-byte as
//! long as no key was changed. A file with no `\r\n` is written with LF.
//!
//! # Typed accessors
//!
//! The client needs a handful of keys; [`Cfg::full_screen`],
//! [`Cfg::sound_volume`] and the other typed getters parse them and fall back to
//! the defaults the shipped file establishes when a key is missing (or its
//! value is not understood). The defaults are documented on each getter and
//! match the shipped `ra2.cfg`, so a fresh file and a missing key behave the
//! same way.
//!
//! Values are not interpreted beyond their type: in particular the volume keys
//! stay in the file's own hundredths-of-a-decibel units. Converting them to a
//! linear gain is the audio layer's job (`ra2-audio` documents DirectSound's
//! convention).

/// The shipped file's default fullscreen setting (`FullScreen=1`).
pub const DEFAULT_FULL_SCREEN: bool = true;
/// The shipped file's default fullscreen width (`FSDisplayWidth=1280`).
pub const DEFAULT_FS_DISPLAY_WIDTH: u32 = 1280;
/// The shipped file's default fullscreen height (`FSDisplayHeight=1024`).
pub const DEFAULT_FS_DISPLAY_HEIGHT: u32 = 1024;
/// The shipped file's default colour depth (`BitsPerPixel=32`).
pub const DEFAULT_BITS_PER_PIXEL: u32 = 32;
/// The shipped file's default camera (`DefaultCamera=watch`).
pub const DEFAULT_CAMERA: &str = "watch";
/// The shipped file's default sound volume in hundredths of a decibel.
pub const DEFAULT_SOUND_VOLUME: i32 = -600;
/// The shipped file's default music volume in hundredths of a decibel.
pub const DEFAULT_MUSIC_VOLUME: i32 = -600;
/// The shipped file's default sound-mute flag (`SoundMuted=0`).
pub const DEFAULT_SOUND_MUTED: bool = false;
/// The shipped file's default music-mute flag (`MusicMuted=0`).
pub const DEFAULT_MUSIC_MUTED: bool = false;
/// The shipped file's default single-player event size (`EventNumBots=4`).
pub const DEFAULT_EVENT_NUM_BOTS: u32 = 4;
/// The shipped file's default arena-intro flag (`ArenaIntros=1`).
pub const DEFAULT_ARENA_INTROS: bool = true;
/// The shipped file's default simulation tick in seconds (`UpdateInterval=0.050000`).
pub const DEFAULT_UPDATE_INTERVAL: f32 = 0.05;
/// The shipped file's default graphic detail (`GraphicDetail=2`).
pub const DEFAULT_GRAPHIC_DETAIL: u32 = 2;
/// The shipped file's default effects flag (`GraphicEffects=1`).
pub const DEFAULT_GRAPHIC_EFFECTS: bool = true;
/// The shipped file's default damage-messages flag (`GraphicMessages=1`).
pub const DEFAULT_GRAPHIC_MESSAGES: bool = true;

/// The line ending the document was read with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Eol {
    Crlf,
    Lf,
}

impl Eol {
    fn as_str(self) -> &'static str {
        match self {
            Eol::Crlf => "\r\n",
            Eol::Lf => "\n",
        }
    }
}

/// What a physical line is.
#[derive(Clone, Debug, PartialEq, Eq)]
enum LineKind {
    /// `[Begin <name>]`.
    Begin(String),
    /// `[End <name>]`.
    End(String),
    /// `Key=Value`.
    Entry { key: String, value: String },
    /// Anything else, kept verbatim.
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Line {
    /// The line's bytes without its terminator, exactly as read.
    raw: String,
    kind: LineKind,
}

/// A parsed `ra2.cfg`, preserving the order and spelling of everything it read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cfg {
    lines: Vec<Line>,
    eol: Eol,
    trailing_newline: bool,
}

impl Cfg {
    /// Parse the text of a `ra2.cfg`.
    ///
    /// This never fails: a line it does not recognise is kept as
    /// [`LineKind::Other`] and written back unchanged.
    pub fn parse(text: &str) -> Cfg {
        let eol = if text.contains("\r\n") {
            Eol::Crlf
        } else {
            Eol::Lf
        };
        let trailing_newline = text.ends_with('\n');
        let mut lines = Vec::new();
        for chunk in text.split_inclusive('\n') {
            let has_newline = chunk.ends_with('\n');
            let mut raw = &chunk[..chunk.len() - usize::from(has_newline)];
            if eol == Eol::Crlf {
                raw = raw.strip_suffix('\r').unwrap_or(raw);
            }
            lines.push(Line {
                kind: classify(raw),
                raw: raw.to_string(),
            });
        }
        Cfg {
            lines,
            eol,
            trailing_newline,
        }
    }

    /// Read a `ra2.cfg` from disk.
    ///
    /// The file is ASCII in every shipped install; invalid UTF-8 is reported
    /// rather than silently replaced, because replacing bytes would break the
    /// byte-for-byte round-trip.
    pub fn read(path: &std::path::Path) -> Result<Cfg, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Cfg::parse(&text))
    }

    /// The document as text, with the line endings and final newline it was
    /// read with.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (index, line) in self.lines.iter().enumerate() {
            if index > 0 {
                out.push_str(self.eol.as_str());
            }
            out.push_str(&line.raw);
        }
        if self.trailing_newline && !self.lines.is_empty() {
            out.push_str(self.eol.as_str());
        }
        out
    }

    /// Write the document back to disk.
    pub fn write(&self, path: &std::path::Path) -> Result<(), String> {
        std::fs::write(path, self.to_text()).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The line-ending sequence this document is written with.
    pub fn line_ending(&self) -> &'static str {
        self.eol.as_str()
    }

    /// Whether the text ended with a line terminator.
    pub fn has_trailing_newline(&self) -> bool {
        self.trailing_newline
    }

    /// Every section name, in the order its `[Begin ...]` line appears.
    ///
    /// Empty sections are included. The list does not check that a matching
    /// `[End ...]` exists; the shipped files always have one.
    pub fn sections(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().filter_map(|line| match &line.kind {
            LineKind::Begin(name) => Some(name.as_str()),
            _ => None,
        })
    }

    /// The value of the first `Key=Value` line named `key`, ignoring ASCII
    /// case (the game's own writer is exactly cased, but Windows configuration
    /// keys are not). Surrounding whitespace is trimmed.
    ///
    /// A duplicate key returns its first occurrence; [`Cfg::values`] returns
    /// all of them, in file order.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.lines.iter().find_map(|line| match &line.kind {
            LineKind::Entry { key: k, value } if k.eq_ignore_ascii_case(key) => {
                Some(value.as_str())
            }
            _ => None,
        })
    }

    /// Every value written for `key`, in file order.
    pub fn values<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.lines.iter().filter_map(move |line| match &line.kind {
            LineKind::Entry { key: k, value } if k.eq_ignore_ascii_case(key) => {
                Some(value.as_str())
            }
            _ => None,
        })
    }

    /// Change the value of the first line named `key`; return whether a line
    /// was found.
    ///
    /// The key's spelling and any whitespace around `=` are preserved; only
    /// the text after `=` is replaced. Nothing else in the document changes.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        for line in &mut self.lines {
            let LineKind::Entry {
                key: k,
                value: current,
            } = &mut line.kind
            else {
                continue;
            };
            if !k.eq_ignore_ascii_case(key) {
                continue;
            }
            *current = value.to_string();
            let prefix = line.raw.split_once('=').map_or(key, |(left, _)| left);
            line.raw = format!("{prefix}={value}");
            return true;
        }
        false
    }

    /// Change the first line named `key`, or add it to the `RA2 Config` section
    /// when the key is not present.
    ///
    /// The shipped file always has an `RA2 Config` section. If a document does
    /// not, the new line is appended at the end.
    pub fn set_or_insert(&mut self, key: &str, value: &str) {
        if self.set(key, value) {
            return;
        }
        let line = Line {
            raw: format!("{key}={value}"),
            kind: LineKind::Entry {
                key: key.to_string(),
                value: value.to_string(),
            },
        };
        let at = self
            .lines
            .iter()
            .position(|line| matches!(&line.kind, LineKind::End(name) if name == "RA2 Config"))
            .unwrap_or(self.lines.len());
        self.lines.insert(at, line);
    }

    fn bool_value(&self, key: &str, default: bool) -> bool {
        match self
            .get(key)
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("1" | "true" | "yes" | "on") => true,
            Some("0" | "false" | "no" | "off") => false,
            _ => default,
        }
    }

    fn u32_value(&self, key: &str, default: u32) -> u32 {
        self.get(key)
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(default)
    }

    fn i32_value(&self, key: &str, default: i32) -> i32 {
        self.get(key)
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(default)
    }

    fn f32_value(&self, key: &str, default: f32) -> f32 {
        self.get(key)
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(default)
    }

    /// `FullScreen`; defaults to [`DEFAULT_FULL_SCREEN`].
    pub fn full_screen(&self) -> bool {
        self.bool_value("FullScreen", DEFAULT_FULL_SCREEN)
    }

    /// `FSDisplayWidth`; defaults to [`DEFAULT_FS_DISPLAY_WIDTH`].
    pub fn fs_display_width(&self) -> u32 {
        self.u32_value("FSDisplayWidth", DEFAULT_FS_DISPLAY_WIDTH)
    }

    /// `FSDisplayHeight`; defaults to [`DEFAULT_FS_DISPLAY_HEIGHT`].
    pub fn fs_display_height(&self) -> u32 {
        self.u32_value("FSDisplayHeight", DEFAULT_FS_DISPLAY_HEIGHT)
    }

    /// `BitsPerPixel`; defaults to [`DEFAULT_BITS_PER_PIXEL`].
    pub fn bits_per_pixel(&self) -> u32 {
        self.u32_value("BitsPerPixel", DEFAULT_BITS_PER_PIXEL)
    }

    /// `DefaultCamera`; defaults to [`DEFAULT_CAMERA`] (`watch`). The game's
    /// other values are `chase` and `top`.
    pub fn default_camera(&self) -> &str {
        self.get("DefaultCamera")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_CAMERA)
    }

    /// `SoundVolume`, in hundredths of a decibel (`-600` = -6 dB); defaults to
    /// [`DEFAULT_SOUND_VOLUME`]. The range is DirectSound's `-10000..=0`;
    /// converting it to a linear gain belongs to the audio layer.
    pub fn sound_volume(&self) -> i32 {
        self.i32_value("SoundVolume", DEFAULT_SOUND_VOLUME)
    }

    /// `MusicVolume`, in hundredths of a decibel; defaults to
    /// [`DEFAULT_MUSIC_VOLUME`]. Same convention as [`Cfg::sound_volume`].
    pub fn music_volume(&self) -> i32 {
        self.i32_value("MusicVolume", DEFAULT_MUSIC_VOLUME)
    }

    /// `SoundMuted`; defaults to [`DEFAULT_SOUND_MUTED`].
    pub fn sound_muted(&self) -> bool {
        self.bool_value("SoundMuted", DEFAULT_SOUND_MUTED)
    }

    /// `MusicMuted`; defaults to [`DEFAULT_MUSIC_MUTED`].
    pub fn music_muted(&self) -> bool {
        self.bool_value("MusicMuted", DEFAULT_MUSIC_MUTED)
    }

    /// `EventNumBots`; defaults to [`DEFAULT_EVENT_NUM_BOTS`] (`4`; the other
    /// shipped value is `2`).
    pub fn event_num_bots(&self) -> u32 {
        self.u32_value("EventNumBots", DEFAULT_EVENT_NUM_BOTS)
    }

    /// `ArenaIntros`; defaults to [`DEFAULT_ARENA_INTROS`].
    pub fn arena_intros(&self) -> bool {
        self.bool_value("ArenaIntros", DEFAULT_ARENA_INTROS)
    }

    /// `UpdateInterval`, the simulation tick in seconds; defaults to
    /// [`DEFAULT_UPDATE_INTERVAL`] (`0.05` = the fixed 50 ms tick).
    pub fn update_interval(&self) -> f32 {
        self.f32_value("UpdateInterval", DEFAULT_UPDATE_INTERVAL)
    }

    /// `GraphicDetail`; defaults to [`DEFAULT_GRAPHIC_DETAIL`] (`2`).
    pub fn graphic_detail(&self) -> u32 {
        self.u32_value("GraphicDetail", DEFAULT_GRAPHIC_DETAIL)
    }

    /// `GraphicEffects`; defaults to [`DEFAULT_GRAPHIC_EFFECTS`].
    pub fn graphic_effects(&self) -> bool {
        self.bool_value("GraphicEffects", DEFAULT_GRAPHIC_EFFECTS)
    }

    /// `GraphicMessages`, the damage-message switch; defaults to
    /// [`DEFAULT_GRAPHIC_MESSAGES`].
    pub fn graphic_messages(&self) -> bool {
        self.bool_value("GraphicMessages", DEFAULT_GRAPHIC_MESSAGES)
    }

    /// Set `FullScreen`.
    pub fn set_full_screen(&mut self, value: bool) {
        self.set_or_insert("FullScreen", bool_text(value));
    }

    /// Set `FSDisplayWidth`.
    pub fn set_fs_display_width(&mut self, value: u32) {
        self.set_or_insert("FSDisplayWidth", &value.to_string());
    }

    /// Set `FSDisplayHeight`.
    pub fn set_fs_display_height(&mut self, value: u32) {
        self.set_or_insert("FSDisplayHeight", &value.to_string());
    }

    /// Set `BitsPerPixel`.
    pub fn set_bits_per_pixel(&mut self, value: u32) {
        self.set_or_insert("BitsPerPixel", &value.to_string());
    }

    /// Set `DefaultCamera`.
    pub fn set_default_camera(&mut self, value: &str) {
        self.set_or_insert("DefaultCamera", value);
    }

    /// Set `SoundVolume` in hundredths of a decibel.
    pub fn set_sound_volume(&mut self, value: i32) {
        self.set_or_insert("SoundVolume", &value.to_string());
    }

    /// Set `MusicVolume` in hundredths of a decibel.
    pub fn set_music_volume(&mut self, value: i32) {
        self.set_or_insert("MusicVolume", &value.to_string());
    }

    /// Set `SoundMuted`.
    pub fn set_sound_muted(&mut self, value: bool) {
        self.set_or_insert("SoundMuted", bool_text(value));
    }

    /// Set `MusicMuted`.
    pub fn set_music_muted(&mut self, value: bool) {
        self.set_or_insert("MusicMuted", bool_text(value));
    }

    /// Set `EventNumBots`.
    pub fn set_event_num_bots(&mut self, value: u32) {
        self.set_or_insert("EventNumBots", &value.to_string());
    }

    /// Set `ArenaIntros`.
    pub fn set_arena_intros(&mut self, value: bool) {
        self.set_or_insert("ArenaIntros", bool_text(value));
    }

    /// Set `UpdateInterval` in seconds.
    pub fn set_update_interval(&mut self, value: f32) {
        self.set_or_insert("UpdateInterval", &format!("{value}"));
    }

    /// Set `GraphicDetail`.
    pub fn set_graphic_detail(&mut self, value: u32) {
        self.set_or_insert("GraphicDetail", &value.to_string());
    }

    /// Set `GraphicEffects`.
    pub fn set_graphic_effects(&mut self, value: bool) {
        self.set_or_insert("GraphicEffects", bool_text(value));
    }

    /// Set `GraphicMessages`.
    pub fn set_graphic_messages(&mut self, value: bool) {
        self.set_or_insert("GraphicMessages", bool_text(value));
    }
}

fn bool_text(value: bool) -> &'static str {
    if value {
        "1"
    } else {
        "0"
    }
}

fn classify(raw: &str) -> LineKind {
    if let Some(name) = raw
        .strip_prefix("[Begin ")
        .and_then(|rest| rest.strip_suffix(']'))
    {
        return LineKind::Begin(name.to_string());
    }
    if let Some(name) = raw
        .strip_prefix("[End ")
        .and_then(|rest| rest.strip_suffix(']'))
    {
        return LineKind::End(name.to_string());
    }
    if let Some((key, value)) = raw.split_once('=') {
        if !key.trim().is_empty() {
            return LineKind::Entry {
                key: key.trim().to_string(),
                value: value.trim().to_string(),
            };
        }
    }
    LineKind::Other
}

impl std::fmt::Display for Cfg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A short fixture with the same CRLF shape as the shipped file.
    const SAMPLE: &str = "[Begin Graphics Config]\r\n\
FullScreen=1\r\n\
FSDisplayWidth=1280\r\n\
FSDisplayHeight=1024\r\n\
BitsPerPixel=32\r\n\
[End Graphics Config]\r\n\
[Begin RA2 Config]\r\n\
EventNumBots=4\r\n\
GraphicDetail=2\r\n\
GraphicEffects=1\r\n\
GraphicMessages=1\r\n\
ArenaIntros=1\r\n\
SoundVolume=-600\r\n\
MusicVolume=-600\r\n\
MusicMuted=0\r\n\
SoundMuted=0\r\n\
DefaultCamera=watch\r\n\
UpdateInterval=0.050000\r\n\
[End RA2 Config]\r\n\
[Begin Multiplayer Config]\r\n\
[End Multiplayer Config]\r\n";

    fn game_dir() -> Option<PathBuf> {
        crate::saves::isolate_for_tests();
        if let Some(dir) = std::env::var_os("RA2_GAME_DIR") {
            let dir = PathBuf::from(dir);
            if dir.join("ra2.cfg").is_file() {
                return Some(dir);
            }
        }
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join("local/game");
            if candidate.join("ra2.cfg").is_file() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// The shipped file is CRLF throughout, including its final line, and an
    /// unmodified parse writes the exact same bytes back.
    #[test]
    fn the_shipped_file_round_trips_byte_identically() {
        let Some(game) = game_dir() else { return };
        let path = game.join("ra2.cfg");
        let bytes = std::fs::read(&path).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();

        // The property the module documents: no bare LF, final CRLF.
        assert!(text.contains("\r\n"), "{} is not CRLF", path.display());
        assert!(
            text.ends_with("\r\n"),
            "{} has no final newline",
            path.display()
        );
        assert!(
            !text.replace("\r\n", "").contains('\n'),
            "bare LF in the file"
        );

        let cfg = Cfg::parse(text);
        assert_eq!(cfg.line_ending(), "\r\n");
        assert!(cfg.has_trailing_newline());
        assert_eq!(cfg.to_text().as_bytes(), bytes.as_slice());
        assert_eq!(
            cfg.sections().collect::<Vec<_>>(),
            [
                "Graphics Config",
                "RA2 Config",
                "Multiplayer Config",
                "Input Config"
            ]
        );
        assert_eq!(cfg.get("FullScreen"), Some("1"));
        assert_eq!(cfg.get("SoundVolume"), Some("-600"));
        assert_eq!(cfg.get("DefaultCamera"), Some("watch"));
    }

    /// Changing one value rewrites only that line; re-parsing reads the new
    /// value and every other line is byte-identical.
    #[test]
    fn a_modified_value_round_trips_and_leaves_the_rest_alone() {
        let mut cfg = Cfg::parse(SAMPLE);
        cfg.set_sound_volume(-300);
        cfg.set_default_camera("chase");
        cfg.set_music_muted(true);

        let written = cfg.to_text();
        let reparsed = Cfg::parse(&written);

        assert_eq!(reparsed.sound_volume(), -300);
        assert_eq!(reparsed.music_volume(), DEFAULT_MUSIC_VOLUME);
        assert_eq!(reparsed.default_camera(), "chase");
        assert!(reparsed.music_muted());
        assert_eq!(reparsed.to_text(), written);

        let before: Vec<_> = SAMPLE.lines().collect();
        let after: Vec<_> = written.lines().collect();
        assert_eq!(before.len(), after.len());
        let changed: Vec<_> = before
            .iter()
            .zip(&after)
            .filter(|(a, b)| a != b)
            .map(|(a, b)| (*a, *b))
            .collect();
        assert_eq!(
            changed,
            [
                ("SoundVolume=-600", "SoundVolume=-300"),
                ("MusicMuted=0", "MusicMuted=1"),
                ("DefaultCamera=watch", "DefaultCamera=chase"),
            ]
        );
    }

    /// Missing keys get the shipped file's defaults, not zero/empty values.
    #[test]
    fn missing_keys_fall_back_to_documented_defaults() {
        let cfg = Cfg::parse("[Begin RA2 Config]\r\n[End RA2 Config]\r\n");
        assert_eq!(cfg.full_screen(), DEFAULT_FULL_SCREEN);
        assert_eq!(cfg.fs_display_width(), DEFAULT_FS_DISPLAY_WIDTH);
        assert_eq!(cfg.fs_display_height(), DEFAULT_FS_DISPLAY_HEIGHT);
        assert_eq!(cfg.bits_per_pixel(), DEFAULT_BITS_PER_PIXEL);
        assert_eq!(cfg.default_camera(), DEFAULT_CAMERA);
        assert_eq!(cfg.sound_volume(), DEFAULT_SOUND_VOLUME);
        assert_eq!(cfg.music_volume(), DEFAULT_MUSIC_VOLUME);
        assert_eq!(cfg.sound_muted(), DEFAULT_SOUND_MUTED);
        assert_eq!(cfg.music_muted(), DEFAULT_MUSIC_MUTED);
        assert_eq!(cfg.event_num_bots(), DEFAULT_EVENT_NUM_BOTS);
        assert_eq!(cfg.arena_intros(), DEFAULT_ARENA_INTROS);
        assert_eq!(cfg.update_interval(), DEFAULT_UPDATE_INTERVAL);
        assert_eq!(cfg.graphic_detail(), DEFAULT_GRAPHIC_DETAIL);
        assert_eq!(cfg.graphic_effects(), DEFAULT_GRAPHIC_EFFECTS);
        assert_eq!(cfg.graphic_messages(), DEFAULT_GRAPHIC_MESSAGES);
    }

    /// A key the client does not know — and a comment — survive a write, and a
    /// missing known key is inserted into the RA2 section instead of at EOF.
    #[test]
    fn unknown_lines_survive_and_new_keys_land_in_the_ra2_section() {
        let source =
            "[Begin RA2 Config]\r\nUnknownKey=hello\r\n# a comment\r\n[End RA2 Config]\r\n";
        let mut cfg = Cfg::parse(source);
        assert_eq!(cfg.get("UnknownKey"), Some("hello"));

        cfg.set_sound_volume(-900);
        let written = cfg.to_text();
        assert_eq!(
            written,
            "[Begin RA2 Config]\r\nUnknownKey=hello\r\n# a comment\r\nSoundVolume=-900\r\n[End RA2 Config]\r\n"
        );
        let reparsed = Cfg::parse(&written);
        assert_eq!(reparsed.get("UnknownKey"), Some("hello"));
        assert_eq!(reparsed.sound_volume(), -900);
    }

    /// Duplicate keys are all kept; `get` reads the first and `values` reads
    /// the file order.
    #[test]
    fn duplicate_keys_are_preserved() {
        let cfg = Cfg::parse("[Begin RA2 Config]\r\nSoundVolume=-600\r\nSoundVolume=-900\r\n");
        assert_eq!(cfg.get("SoundVolume"), Some("-600"));
        assert_eq!(
            cfg.values("SoundVolume").collect::<Vec<_>>(),
            ["-600", "-900"]
        );
    }

    /// A file with LF stays LF and keeps its missing final newline; the writer
    /// does not silently "fix" either.
    #[test]
    fn line_endings_are_not_normalised() {
        let cfg = Cfg::parse("[Begin RA2 Config]\nSoundVolume=-600");
        assert_eq!(cfg.line_ending(), "\n");
        assert!(!cfg.has_trailing_newline());
        assert_eq!(cfg.to_text(), "[Begin RA2 Config]\nSoundVolume=-600");
    }
}
