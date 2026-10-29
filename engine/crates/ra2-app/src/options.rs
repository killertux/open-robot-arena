//! The options screen (M7.6): the shipped `common/options.gib` and its two
//! content fragments, driven from `ra2.cfg`.
//!
//! The shell owns the screen: its `Options button` note opens
//! `common/options.gib`, the `video tab`/`audio tab` notes load the shipped
//! fragments into `options content group`, and `ok button`/`cancel button`
//! return to the main menu. This module is the settings half — it stages a
//! [`Cfg`], maps a click's controls to the keys the screen names, mirrors the
//! values back into the shipped records before a frame, and writes the file.
//!
//! # Which control edits which key
//!
//! | Screen and control | Key | Notes |
//! |---|---|---|
//! | `effects volume slider` | `SoundVolume` | the slider's left edge is `-10000`, its right edge `0`; **our scale** (the screen states no scale) |
//! | `music volume slider` | `MusicVolume` | same scale |
//! | `effects checkbox` | `SoundMuted` | checked = effects on, so muted is the inverse |
//! | `music checkbox` | `MusicMuted` | checked = music on |
//! | `intro off`/`intro on` | `ArenaIntros` | the shipped `Arena Introductions` group |
//! | `fullscreen checkbox` | `FullScreen` | |
//! | `Camera dropdown` | `DefaultCamera` | opens a list of the binary's `watch`/`orbit`/`static` (`ra2ui::dropdown`) |
//! | `effects off`/`effects on` | `GraphicEffects` | |
//! | `damage off`/`damage on` | `GraphicMessages` | the screen's "damage" group is the status-message switch |
//! | `2 player`/`4 player` | `EventNumBots` | |
//! | `detail low`/`med`/`high` | `GraphicDetail` | `0`/`1`/`2` is **our numbering**; the screen's labels are the evidence, not the integers |
//! | `resolution dropdown` | `FSDisplayWidth`/`Height` (+ the `FS3D` pair) | opens the list of the display's modes (`FUN_004a00b0` walks the device's mode list and writes `WxH`); the window supplies them (`set_display_modes`) |
//!
//! `UpdateInterval`, `BitsPerPixel`, `FSDisplayWidth`/`FSDisplayHeight` and the
//! keys that exist only in the graphics/input sections are not on the screen, so
//! this module does not invent controls for them.
//!
//! # Writing
//!
//! The file is written with [`Cfg::write`], which changes only the edited
//! values. A change writes immediately (so the file and the live mix agree) and
//! `ok button` writes again as a check; `cancel button` restores the bytes that
//! were on disk when the window opened.

use std::path::{Path, PathBuf};

use ra2_shell::Shell;
use ra2fmt::cfg::Cfg;
use ra2fmt::gib::{Block, Gib, Widget, WidgetKind};

/// The screen the main menu's `Options button` opens.
pub const SCREEN: &str = "common/options.gib";
/// The note on the main menu's `Options button`.
pub const OPEN_NOTE: &str = "click to bring up options dialog";
/// The note on `ok button`.
pub const OK_NOTE: &str = "click to close options window";
/// The note on `cancel button`.
pub const CANCEL_NOTE: &str = "click to close options window but NOT apply changes";

/// Read the install's `ra2.cfg`; a missing file (or key) keeps the documented
/// defaults. This never writes.
pub fn read_cfg(game: &Path) -> Cfg {
    let path = settings_path(game);
    Cfg::read(&path)
        .or_else(|_| Cfg::read(&game.join("ra2.cfg")))
        .unwrap_or_else(|_| Cfg::parse(""))
}

/// Where the client keeps the player's settings: `ra2.cfg` in the save
/// directory ([`ra2fmt::saves`]; `RA2_CONFIG` overrides the file), never the install, which is
/// read-only data. The first read falls back to the install's shipped
/// `ra2.cfg`; the first change creates this file from it.
pub fn settings_path(game: &Path) -> PathBuf {
    if let Some(path) = std::env::var_os("RA2_CONFIG") {
        return PathBuf::from(path);
    }
    ra2fmt::saves::target(game, "ra2.cfg")
}

/// The initial window size from `FSDisplayWidth`/`FSDisplayHeight`, with zero
/// clamped to one (a zero-sized window cannot open).
pub fn display_size(cfg: &Cfg) -> [u32; 2] {
    [
        cfg.fs_display_width().max(1),
        cfg.fs_display_height().max(1),
    ]
}

/// The display modes the video tab offers, smallest first.
pub const RESOLUTIONS: [[u32; 2]; 5] = [
    [640, 480],
    [800, 600],
    [1024, 768],
    [1280, 1024],
    [1600, 1200],
];

/// The staged settings and the file they came from.
pub struct Options {
    path: PathBuf,
    cfg: Cfg,
    /// The exact bytes on disk when this instance opened, for `cancel`.
    original: String,
    existed: bool,
    /// The display modes the resolution list offers (the device's list the
    /// window reads at run time, `FUN_004a00b0`); [`RESOLUTIONS`] until set.
    modes: Vec<[u32; 2]>,
}

impl Options {
    /// Open the player's settings (see [`settings_path`]), starting from the
    /// install's `ra2.cfg` when the player has none yet. The install is only
    /// ever read.
    pub fn open_game(game: &Path) -> Options {
        let path = settings_path(game);
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if !path.is_file() {
            if let Ok(shipped) = std::fs::read_to_string(game.join("ra2.cfg")) {
                // Seed with the shipped values; `cancel` on a fresh install
                // then restores exactly these bytes.
                let _ = std::fs::write(&path, shipped);
            }
        }
        Options::open(path)
    }

    /// Open a settings file. A missing file starts from the documented
    /// defaults and remembers that it did not exist, so `cancel` can remove it
    /// again.
    pub fn open(path: impl Into<PathBuf>) -> Options {
        let path = path.into();
        let existed = path.is_file();
        let original = std::fs::read_to_string(&path).unwrap_or_default();
        let cfg = Cfg::parse(&original);
        Options {
            path,
            cfg,
            original,
            existed,
            modes: RESOLUTIONS.to_vec(),
        }
    }

    /// The display modes of the screen the game runs on, in the order the
    /// device lists them; the resolution dropdown's rows. An empty list keeps
    /// the standard set.
    pub fn set_display_modes(&mut self, modes: &[[u32; 2]]) {
        if modes.is_empty() {
            return;
        }
        self.modes = modes.to_vec();
    }

    /// Hand the shell the rows of the options window's two dropdowns
    /// (`resolution dropdown`: `WxH` per mode; `Camera dropdown`: the
    /// executable's `watch`, `orbit`, `static`).
    pub fn register_dropdowns(&self, shell: &mut Shell) {
        shell.set_dropdown_items(
            "resolution dropdown",
            DEPTHS
                .iter()
                .flat_map(|depth| self.modes.iter().map(move |m| mode_text(*m, *depth)))
                .collect(),
        );
        // `FUN_004a00b0` sets the rows' colour before it fills them:
        // `FUN_0061b4b0(1.0, 0.5, 0, 1.0)` (orange) into `FUN_005f0f90`; the
        // camera list (`FUN_004a0400`) is orange too (capture).
        shell.set_dropdown_row_color("resolution dropdown", ORANGE);
        shell.set_dropdown_items(
            "Camera dropdown",
            CAMERAS.iter().map(|c| c.to_string()).collect(),
        );
        shell.set_dropdown_row_color("Camera dropdown", ORANGE);
    }

    /// Apply the rows the shell reports picked from the options window's
    /// dropdowns (`Shell::take_dropdown_picks`). Returns the changes made.
    pub fn absorb_picks(&mut self, picks: &[(String, String)]) -> Vec<String> {
        let mut changes = Vec::new();
        for (name, text) in picks {
            if name == "resolution dropdown" {
                // `WxHxDepth` (the capture lists `1024x768x32`).
                let mut parts = text.split('x');
                let parsed = parts
                    .next()
                    .zip(parts.next())
                    .and_then(|(w, h)| Some([w.trim().parse().ok()?, h.trim().parse().ok()?]));
                if let Some(mode) = parsed {
                    self.set_mode(mode);
                    changes.push(format!("FSDisplay={}x{}", mode[0], mode[1]));
                }
            } else if name == "Camera dropdown" {
                if let Some(camera) = CAMERAS.iter().find(|c| c.eq_ignore_ascii_case(text)) {
                    self.cfg.set_default_camera(camera);
                    changes.push(format!("DefaultCamera={camera}"));
                }
            }
        }
        changes
    }

    /// Write a chosen display mode: both the 2D and 3D keys.
    fn set_mode(&mut self, mode: [u32; 2]) {
        self.cfg.set_fs_display_width(mode[0]);
        self.cfg.set_fs_display_height(mode[1]);
        self.cfg
            .set_or_insert("FS3DDisplayWidth", &mode[0].to_string());
        self.cfg
            .set_or_insert("FS3DDisplayHeight", &mode[1].to_string());
    }

    /// The staged values, including any change not yet written.
    pub fn cfg(&self) -> &Cfg {
        &self.cfg
    }

    /// Re-read the file, discarding staged changes.
    pub fn reload(&mut self) -> Result<(), String> {
        let text = std::fs::read_to_string(&self.path)
            .map_err(|error| format!("{}: {error}", self.path.display()))?;
        self.original = text;
        self.cfg = Cfg::parse(&self.original);
        self.existed = true;
        Ok(())
    }

    /// Write the staged values. Only the edited lines differ from what was
    /// read; `Cfg::write` preserves everything else.
    pub fn save(&mut self) -> Result<(), String> {
        self.cfg.write(&self.path)?;
        Ok(())
    }

    /// Restore the bytes from when the window opened and reload the staged
    /// values. A file that did not exist is removed again.
    pub fn cancel(&mut self) -> Result<(), String> {
        if self.existed {
            // A change writes immediately; compare the file to the opening
            // bytes rather than relying on an unsaved-change flag.
            let current = std::fs::read_to_string(&self.path).unwrap_or_default();
            if current != self.original {
                std::fs::write(&self.path, &self.original)
                    .map_err(|error| format!("{}: {error}", self.path.display()))?;
            }
            self.reload()
        } else {
            let _ = std::fs::remove_file(&self.path);
            self.cfg = Cfg::parse("");
            Ok(())
        }
    }

    /// Whether `screen` is the options window.
    pub fn is_screen(screen: &str) -> bool {
        screen.eq_ignore_ascii_case(SCREEN)
    }

    /// Apply a control click's staged edit. `None` means the click changed no
    /// setting (a tab, a read-only dropdown, or empty chrome).
    pub fn click(&mut self, hit: &Hit) -> Option<String> {
        if let Some((name, fraction)) = &hit.slider {
            let value = volume_from_fraction(*fraction);
            if name == "effects volume slider" {
                self.cfg.set_sound_volume(value);
                return Some(Self::changed(format!("SoundVolume={value}")));
            }
            if name == "music volume slider" {
                self.cfg.set_music_volume(value);
                return Some(Self::changed(format!("MusicVolume={value}")));
            }
        }

        let names = &hit.names;
        let any = |wanted: &str| names.iter().any(|name| name == wanted);

        if any("effects checkbox") {
            let value = !self.cfg.sound_muted();
            self.cfg.set_sound_muted(value);
            return Some(Self::changed(format!("SoundMuted={}", u8::from(value))));
        }
        if any("music checkbox") {
            let value = !self.cfg.music_muted();
            self.cfg.set_music_muted(value);
            return Some(Self::changed(format!("MusicMuted={}", u8::from(value))));
        }
        if any("fullscreen checkbox") {
            let value = !self.cfg.full_screen();
            self.cfg.set_full_screen(value);
            return Some(Self::changed(format!("FullScreen={}", u8::from(value))));
        }
        if any("intro off checkbox") || any("intro on checkbox") {
            let value = any("intro on checkbox");
            self.cfg.set_arena_intros(value);
            return Some(Self::changed(format!("ArenaIntros={}", u8::from(value))));
        }
        if any("effects off checkbox") || any("effects on checkbox") {
            let value = any("effects on checkbox");
            self.cfg.set_graphic_effects(value);
            return Some(Self::changed(format!("GraphicEffects={}", u8::from(value))));
        }
        if any("damage off checkbox") || any("damage on checkbox") {
            let value = any("damage on checkbox");
            self.cfg.set_graphic_messages(value);
            return Some(Self::changed(format!(
                "GraphicMessages={}",
                u8::from(value)
            )));
        }
        if any("2 player checkbox") || any("4 player checkbox") {
            let value = if any("4 player checkbox") { 4 } else { 2 };
            self.cfg.set_event_num_bots(value);
            return Some(Self::changed(format!("EventNumBots={value}")));
        }
        if any("detail low checkbox") || any("detail med checkbox") || any("detail high checkbox") {
            let value = if any("detail high checkbox") {
                2
            } else if any("detail med checkbox") {
                1
            } else {
                0
            };
            self.cfg.set_graphic_detail(value);
            return Some(Self::changed(format!("GraphicDetail={value}")));
        }
        None
    }

    fn changed(message: String) -> String {
        message
    }

    /// Mirror the staged values into the shipped records, so the sliders,
    /// checkboxes and dropdown values draw the settings the file holds.
    ///
    /// A control the fragment does not contain is skipped; this is the same
    /// screen that the shell loaded, so the audio and video fragments only
    /// update when their tab is the one on screen.
    pub fn sync(&self, gib: &mut Gib) {
        for block in &mut gib.blocks {
            let Block::Widget(root) = block else { continue };
            set_slider(
                root,
                "effects volume slider",
                volume_fraction(self.cfg.sound_volume()),
            );
            set_slider(
                root,
                "music volume slider",
                volume_fraction(self.cfg.music_volume()),
            );

            set_checkbox(root, "effects checkbox", !self.cfg.sound_muted());
            set_checkbox(root, "music checkbox", !self.cfg.music_muted());
            set_checkbox(root, "fullscreen checkbox", self.cfg.full_screen());

            select(
                root,
                "Intro optiongrp",
                if self.cfg.arena_intros() {
                    "intro on checkbox"
                } else {
                    "intro off checkbox"
                },
            );
            select(
                root,
                "effects optiongrp",
                if self.cfg.graphic_effects() {
                    "effects on checkbox"
                } else {
                    "effects off checkbox"
                },
            );
            select(
                root,
                "damage optiongrp",
                if self.cfg.graphic_messages() {
                    "damage on checkbox"
                } else {
                    "damage off checkbox"
                },
            );
            select(
                root,
                "event size optiongrp",
                if self.cfg.event_num_bots() >= 4 {
                    "4 player checkbox"
                } else {
                    "2 player checkbox"
                },
            );
            select(
                root,
                "detail optiongrp",
                match self.cfg.graphic_detail() {
                    0 => "detail low checkbox",
                    1 => "detail med checkbox",
                    _ => "detail high checkbox",
                },
            );

            if let Some(dropdown) = find_mut(root, "Camera dropdown") {
                set_dropdown_text(dropdown, self.cfg.default_camera());
            }
            if let Some(dropdown) = find_mut(root, "resolution dropdown") {
                set_dropdown_text(
                    dropdown,
                    &mode_text(
                        [self.cfg.fs_display_width(), self.cfg.fs_display_height()],
                        32,
                    ),
                );
            }
        }
    }
}

/// A control click's path names and, when it landed on a slider, the slider's
/// horizontal fraction (`0` = left edge, `1` = right edge).
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub names: Vec<String>,
    pub slider: Option<(String, f32)>,
}

/// Hit-test a point on the shell's current screen, with the same control rule
/// the shell uses.
pub fn hit(shell: &Shell, point: [f32; 2]) -> Option<Hit> {
    let placed = shell.placements();
    let path = ra2ui::input::hit_control(&placed, point[0], point[1])?;
    let mut names = Vec::new();
    let mut chain = Vec::new();
    let first = *path.first()?;
    let mut current = &placed[first];
    names.push(current.widget.name().to_string());
    chain.push(current);
    for index in &path[1..] {
        current = &current.children[*index];
        names.push(current.widget.name().to_string());
        chain.push(current);
    }
    let slider = chain
        .iter()
        .find(|placement| placement.widget.kind == WidgetKind::Slider)
        .map(|placement| {
            let frame = placement.frame;
            let fraction = if frame.w > 0.0 {
                ((point[0] - frame.x) / frame.w).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (placement.widget.name().to_string(), fraction)
        });
    Some(Hit { names, slider })
}

/// `-10000..=0` hundredths of a decibel from a slider's horizontal position.
/// This is the client's scale; the shipped screen states none.
pub fn volume_from_fraction(fraction: f32) -> i32 {
    let fraction = if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        0.0
    };
    (-10_000.0 * (1.0 - fraction)).round() as i32
}

/// The inverse of [`volume_from_fraction`].
pub fn volume_fraction(value: i32) -> f32 {
    (1.0 - (value as f32 / -10_000.0)).clamp(0.0, 1.0)
}

/// The config's three camera names, in the executable's order.
pub const CAMERAS: [&str; 3] = ["watch", "orbit", "static"];

/// The rows' colour (orange, `FUN_0061b4b0(1, 0.5, 0, 1)` in `FUN_004a00b0`).
const ORANGE: &str = "1 0.5 0 1";

/// A display mode as the list writes it: `WxHxDepth` (`FUN_004a00b0`
/// formats the width, `x`, the height, `x`, the colour depth); the captured
/// list shows 32-bit modes only.
fn mode_text(mode: [u32; 2], depth: u32) -> String {
    format!("{}x{}x{depth}", mode[0], mode[1])
}

/// The depths the device lists, each with every size (capture: five 32-bit
/// rows, then the same five at 16 bits).
const DEPTHS: [u32; 2] = [32, 16];

/// The next camera name after `current`, or `watch` for a value the client does
/// not know.
pub fn next_camera(current: &str) -> &'static str {
    // The C key's cycle (`watch`, `static`, `orbit`), not the dropdown's order.
    const CYCLE: [&str; 3] = ["watch", "static", "orbit"];
    let current = current.trim();
    match CYCLE
        .iter()
        .position(|name| name.eq_ignore_ascii_case(current))
    {
        Some(index) => CYCLE[(index + 1) % CYCLE.len()],
        None => CYCLE[0],
    }
}

fn set_checkbox(root: &mut Widget, name: &str, checked: bool) {
    if let Some(widget) = find_mut(root, name) {
        ra2ui::input::set_state(widget, checked);
    }
}

fn select(root: &mut Widget, group: &str, child: &str) {
    ra2ui::input::select_option(root, group, child);
}

fn set_slider(root: &mut Widget, name: &str, fraction: f32) {
    let Some(slider) = find_mut(root, name) else {
        return;
    };
    let width = slider.size().map(|size| size.first.amount).unwrap_or(168.0);
    let Some(thumb) = slider
        .children
        .iter_mut()
        .find(|child| child.kind == WidgetKind::Image)
    else {
        return;
    };
    let thumb_width = thumb.size().map(|size| size.first.amount).unwrap_or(16.0);
    let x = (fraction.clamp(0.0, 1.0) * (width - thumb_width).max(0.0)).max(0.0);
    ra2ui::input::set_field(thumb, "Position", &format!("{x} 0 0 0"));
}

fn set_dropdown_text(dropdown: &mut Widget, text: &str) {
    if let Some(field) = first_text_mut(dropdown) {
        ra2ui::input::set_field(field, "_Text", text);
    }
}

fn first_text_mut(widget: &mut Widget) -> Option<&mut Widget> {
    if widget.kind == WidgetKind::Text {
        return Some(widget);
    }
    widget.children.iter_mut().find_map(first_text_mut)
}

fn find_mut<'a>(widget: &'a mut Widget, name: &str) -> Option<&'a mut Widget> {
    if widget.name() == name {
        return Some(widget);
    }
    widget
        .children
        .iter_mut()
        .find_map(|child| find_mut(child, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> &'static str {
        "[Begin Graphics Config]\r\n\
         FullScreen=1\r\n\
         FSDisplayWidth=1280\r\n\
         FSDisplayHeight=1024\r\n\
         [End Graphics Config]\r\n\
         [Begin RA2 Config]\r\n\
         SoundVolume=-600\r\n\
         MusicVolume=-600\r\n\
         MusicMuted=0\r\n\
         SoundMuted=0\r\n\
         DefaultCamera=watch\r\n\
         ArenaIntros=1\r\n\
         GraphicEffects=1\r\n\
         GraphicMessages=1\r\n\
         GraphicDetail=2\r\n\
         EventNumBots=4\r\n\
         [End RA2 Config]\r\n"
    }

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

    fn hit(names: &[&str]) -> Hit {
        Hit {
            names: names.iter().map(|name| name.to_string()).collect(),
            slider: None,
        }
    }

    #[test]
    fn the_display_keys_are_the_initial_window_size() {
        let defaults = Cfg::parse("[Begin RA2 Config]\r\n[End RA2 Config]\r\n");
        assert_eq!(display_size(&defaults), [1280, 1024]);
        let mut cfg = defaults.clone();
        cfg.set_fs_display_width(1920);
        cfg.set_fs_display_height(1080);
        assert_eq!(display_size(&cfg), [1920, 1080]);
        cfg.set_fs_display_width(0);
        assert_eq!(display_size(&cfg)[0], 1);
    }

    /// The shipped file itself: change a volume and the camera, write, re-read.
    /// Every other line (and the CRLF/final newline) is byte-identical.
    #[test]
    fn the_shipped_cfg_survives_a_volume_and_camera_change() {
        let Some(game) = game_dir() else { return };
        let source = game.join("ra2.cfg");
        let Ok(original) = std::fs::read_to_string(&source) else {
            return;
        };
        let path = std::env::temp_dir().join(format!(
            "ra2-options-shipped-{}-{}.cfg",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, &original).unwrap();

        let mut options = Options::open(&path);
        options.click(&Hit {
            names: vec!["music volume slider".into()],
            slider: Some(("music volume slider".into(), 0.55)),
        });
        options.absorb_picks(&[("Camera dropdown".to_string(), "static".to_string())]);
        options.save().unwrap();

        let reread = Options::open(&path);
        assert_eq!(reread.cfg().music_volume(), -4_500);
        assert_eq!(reread.cfg().default_camera(), "static");
        assert_eq!(reread.cfg().sound_volume(), -600);
        let written = std::fs::read_to_string(&path).unwrap();
        let before: Vec<_> = original.lines().collect();
        let after: Vec<_> = written.lines().collect();
        assert_eq!(before.len(), after.len());
        let changed: Vec<_> = before
            .iter()
            .zip(&after)
            .filter(|(a, b)| a != b)
            .map(|(a, b)| (*a, *b))
            .collect();
        assert_eq!(changed.len(), 2, "{changed:?}");
        assert_eq!(changed[0].0, "MusicVolume=-600");
        assert_eq!(changed[0].1, "MusicVolume=-4500");
        assert_eq!(changed[1].0, "DefaultCamera=watch");
        assert_eq!(changed[1].1, "DefaultCamera=static");
        assert!(written.contains("\r\n"));
        assert!(written.ends_with("\r\n"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn the_slider_scale_is_the_directsound_range() {
        assert_eq!(volume_from_fraction(0.0), -10_000);
        assert_eq!(volume_from_fraction(1.0), 0);
        assert_eq!(volume_from_fraction(0.5), -5_000);
        assert_eq!(volume_from_fraction(-1.0), -10_000);
        assert_eq!(volume_from_fraction(f32::NAN), -10_000);
        assert!((volume_fraction(-600) - 0.94).abs() < 1e-6);
        assert_eq!(volume_fraction(-10_000), 0.0);
        assert_eq!(volume_fraction(0), 1.0);
    }

    #[test]
    fn the_visible_controls_stage_the_keys_the_screen_exposes() {
        let path = std::env::temp_dir().join(format!(
            "ra2-options-unit-{}-{}.cfg",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, sample()).unwrap();
        let mut options = Options::open(&path);

        assert_eq!(
            options.click(&hit(&["effects checkbox"])).as_deref(),
            Some("SoundMuted=1")
        );
        assert!(options.cfg().sound_muted());

        options.click(&hit(&["music checkbox"]));
        assert!(options.cfg().music_muted());

        options.click(&hit(&["intro off checkbox"]));
        assert!(!options.cfg().arena_intros());
        options.click(&hit(&["intro on checkbox"]));
        assert!(options.cfg().arena_intros());

        options.click(&hit(&["fullscreen checkbox"]));
        assert!(!options.cfg().full_screen());

        options.click(&hit(&["effects off checkbox"]));
        assert!(!options.cfg().graphic_effects());
        options.click(&hit(&["effects on checkbox"]));
        assert!(options.cfg().graphic_effects());

        options.click(&hit(&["damage off checkbox"]));
        assert!(!options.cfg().graphic_messages());
        options.click(&hit(&["damage on checkbox"]));
        assert!(options.cfg().graphic_messages());

        options.click(&hit(&["2 player checkbox"]));
        assert_eq!(options.cfg().event_num_bots(), 2);
        options.click(&hit(&["4 player checkbox"]));
        assert_eq!(options.cfg().event_num_bots(), 4);

        options.click(&hit(&["detail low checkbox"]));
        assert_eq!(options.cfg().graphic_detail(), 0);
        options.click(&hit(&["detail med checkbox"]));
        assert_eq!(options.cfg().graphic_detail(), 1);
        options.click(&hit(&["detail high checkbox"]));
        assert_eq!(options.cfg().graphic_detail(), 2);

        assert_eq!(next_camera("watch"), "static");
        assert_eq!(next_camera("static"), "orbit");
        assert_eq!(next_camera("orbit"), "watch");
        assert_eq!(next_camera("chase"), "watch", "unknown starts at watch");

        assert_eq!(
            options
                .click(&hit(&["slider", "effects volume slider"]))
                .as_deref()
                .map(|s| s.to_string()),
            None,
            "a slider without a fraction is not a change"
        );
        let volume = options.click(&Hit {
            names: vec!["effects volume slider".into()],
            slider: Some(("effects volume slider".into(), 0.94)),
        });
        assert_eq!(volume.as_deref(), Some("SoundVolume=-600"));

        assert_eq!(
            options.absorb_picks(&[("resolution dropdown".to_string(), "1600x1200".to_string())]),
            vec!["FSDisplay=1600x1200".to_string()],
            "a picked row is the display mode"
        );

        options.save().unwrap();
        options.cancel().unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after, sample(), "cancel restores the exact bytes");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_change_writes_only_the_edited_lines_and_round_trips() {
        let path = std::env::temp_dir().join(format!(
            "ra2-options-roundtrip-{}-{}.cfg",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, sample()).unwrap();
        let mut options = Options::open(&path);
        options.click(&Hit {
            names: vec!["music volume slider".into()],
            slider: Some(("music volume slider".into(), 0.7)),
        });
        options.absorb_picks(&[("Camera dropdown".to_string(), "static".to_string())]);
        options.save().unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        let reread = Options::open(&path);
        assert_eq!(reread.cfg().music_volume(), -3_000);
        assert_eq!(reread.cfg().default_camera(), "static");
        assert_eq!(reread.cfg().sound_volume(), -600, "untouched");
        assert_eq!(reread.cfg().event_num_bots(), 4, "untouched");

        let before: Vec<_> = sample().lines().collect();
        let after: Vec<_> = written.lines().collect();
        assert_eq!(before.len(), after.len());
        let changed: Vec<_> = before.iter().zip(&after).filter(|(a, b)| a != b).collect();
        assert_eq!(changed.len(), 2, "{changed:?}");
        assert!(changed[0].1.starts_with("MusicVolume="));
        assert!(changed[1].1.starts_with("DefaultCamera="));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn sync_mirrors_the_values_into_the_shipped_fragments() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let mut cfg = Cfg::parse(sample());
        cfg.set_sound_volume(-2_500);
        cfg.set_music_volume(-5_000);
        cfg.set_sound_muted(true);
        cfg.set_full_screen(false);
        cfg.set_default_camera("orbit");
        cfg.set_graphic_detail(1);
        cfg.set_event_num_bots(2);

        let path = std::env::temp_dir().join(format!(
            "ra2-options-sync-{}-{}.cfg",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        cfg.write(&path).unwrap();
        let options = Options::open(&path);

        let mut audio = ra2ui::workshop::load(&ui, "common/audio_options.gib").unwrap();
        options.sync(&mut audio);
        let root = audio
            .blocks
            .iter_mut()
            .find_map(|block| match block {
                Block::Widget(widget) => Some(widget),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            ra2ui::workshop::find(root, "effects checkbox")
                .unwrap()
                .field("Checked")
                .unwrap()
                .text(),
            "false"
        );
        assert_eq!(
            ra2ui::workshop::find(root, "Intro optiongrp")
                .unwrap()
                .field("Selection")
                .unwrap()
                .text(),
            "intro on checkbox"
        );
        let thumb = &ra2ui::workshop::find(root, "effects volume slider")
            .unwrap()
            .children[0];
        let x = thumb.field("Position").unwrap().numbers()[0];
        assert!((x - volume_fraction(-2_500) * 152.0).abs() < 0.01, "{x}");

        let mut video = ra2ui::workshop::load(&ui, "common/video_options.gib").unwrap();
        options.sync(&mut video);
        let root = video
            .blocks
            .iter_mut()
            .find_map(|block| match block {
                Block::Widget(widget) => Some(widget),
                _ => None,
            })
            .unwrap();
        let camera = ra2ui::workshop::find(root, "Camera dropdown")
            .unwrap()
            .children
            .iter()
            .find(|child| child.kind == WidgetKind::TextBox)
            .unwrap();
        let text = camera
            .children
            .iter()
            .find(|child| child.kind == WidgetKind::Text)
            .unwrap();
        assert_eq!(text.field("_Text").unwrap().text(), "orbit");
        assert_eq!(
            ra2ui::workshop::find(root, "fullscreen checkbox")
                .unwrap()
                .field("Checked")
                .unwrap()
                .text(),
            "false"
        );
        assert_eq!(
            ra2ui::workshop::find(root, "event size optiongrp")
                .unwrap()
                .field("Selection")
                .unwrap()
                .text(),
            "2 player checkbox"
        );
        assert_eq!(
            ra2ui::workshop::find(root, "detail optiongrp")
                .unwrap()
                .field("Selection")
                .unwrap()
                .text(),
            "detail med checkbox"
        );
        std::fs::remove_file(&path).ok();
    }
}
