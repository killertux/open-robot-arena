//! The match HUD: `common/gamewindow.gib`, bound to a live match.
//!
//! The C++ constructor at `0x4749c0` loads `gamewindow.gib` and binds four
//! `playerN status` groups, each a `statusmeters.gib`. This module does the same
//! through the data: the widget names are the ones the screens write, and the
//! values are the match's own numbers.
//!
//! ## The three bars
//!
//! `statusmeters.gib` draws a 52 px battery bar, a 52 px air bar and a 26 px
//! armour bar. The binder at `0x4b2f20` stores their widget pointers, and the
//! per-frame update (GameWindow update at `0x412fc0` → `0x476b60` →
//! `0x4b3540`/`0x4b3570`/`0x4b35a0`) sets each width to `(int)(fraction *
//! base)` and leaves x alone: the fill is **left-anchored and shrinks to the
//! right**. The armour setter also places the white remainder at `x = 49 +
//! width`, width `26 - width`, which confirms the direction.
//!
//! **Which value each bar receives** is the binary's: battery reads the status
//! object's `+0xa8` (getter `FUN_0043e5f0`), air `+0xa4` (`FUN_0043e5e0`), and
//! armour `+0xb4 / +0xb0` (`FUN_0043e600`), clamped at zero; the dot reads
//! `+0xbc` (`FUN_00416940`, 0 green / 1 yellow / 2 red). `+0xa8`/`+0xa4` are
//! the charge left over the total of the robot's batteries and air tanks, run
//! down by the motors and pistons (`ra2sim::energy`, docs/30), and `+0xbc` is
//! the chassis' critical-hit count. The fill geometry and truncation are the
//! binary's.
//!
//! ## What is not drawn
//!
//! The arena intro (`skip group`) is not implemented, so the group is hidden.
//! The camera name comes from [`set_camera`], because the camera is the app's
//! state, not the match's; `0x4749c0` binds `camera textbox` to `+0x178` and
//! `FUN_00477180` shows it for 3.0 s before `0x4774f1` hides it again.
//!
//! ## The slots
//!
//! `0x4749c0` creates the four status meters into `+0x15c`, `+0x160`, `+0x164`
//! and `+0x168` in that order (`0x474abc`–`0x474af9`), then attaches meter 0 to
//! `player1 status` (`0x474afb`–`0x474b43`), 1 to `player2 status`
//! (`0x474b46`–`0x474b91`), 2 to `player3 status` and 3 to `player4 status`.
//! The file puts `player1` at `0.01 1 0.02 1` (top-left), `player2` at
//! `0.99 1 0.02 1` (top-right), `player3` at `0.01 1 0.98 1` (bottom-left) and
//! `player4` at `0.99 1 0.98 1` (bottom-right). Robot 0 is therefore the
//! top-left corner, and a two-robot match hides the bottom row.
//!
//! The chat group stays hidden until the backslash key: `0x475ee0` shows
//! `+0x1b4` (`chat group`) and focuses `+0x1b8` (`chat entry`) when the key is
//! `\` and the match state is playing. [`set_chat`] is the minimum open/close;
//! no network path exists in this client.

use std::path::Path;

use ra2fmt::gib::{Gib, Widget, WidgetKind};
use ra2sim::r#match::{MatchReport, MatchState};

/// The screen's own file.
pub const SCREEN: &str = "common/gamewindow.gib";
/// The per-bot meters, loaded into each `playerN status` group.
pub const STATUS: &str = "common/statusmeters.gib";
/// The template's root, from its own note *"load 'statusmeters.gib' here"*.
pub const STATUS_GROUP: &str = "status group";

/// The four slots, in the screen's own order. Their positions are the layout's:
/// 1 and 2 are the top corners, 3 and 4 the bottom corners.
pub const PLAYERS: [&str; 4] = [
    "player1 status",
    "player2 status",
    "player3 status",
    "player4 status",
];

/// The time group and its countdown sibling, named by the file's own notes.
pub const TIME_GROUP: &str = "time group";
pub const TIME_BOX: &str = "time";
pub const COUNTDOWN_GROUP: &str = "countdown group";
pub const COUNTDOWN_BOX: &str = "count number";
pub const CAMERA_BOX: &str = "camera textbox";

/// The status dots, in the order `FUN_004b3640` lights them (`0x4b313c`,
/// `0x4b3187`, `0x4b31ce` bind them to `+0xe0`, `+0xe4`, `+0xe8`).
pub const DOTS: [&str; 3] = ["green dot", "yellow dot", "red dot"];
/// What a defeated robot's meters are drawn with (`FUN_004b3850`, constant `0x3e99999a`).
pub const DEFEATED_TINT: &str = "0.3 0.3 0.3 1";
/// The widgets `FUN_004b3850` tints (the status meter's `+0xcc..+0xe8`).
///
/// Not `armor bar` (`+0xd8`): live, its tint stays [`ARMOR_TINT`] after the panel is dimmed (a
/// dump of the status meter's widgets after the out state: the air bar's `+0xbc..+0xc4` went to 0.3,
/// the armour bar's stayed `(1, 0.5, 0.25)`), so a robot put out with armour left shows its bar at
/// full brightness among the dimmed meters.
const DIMMED: [&str; 6] = [
    "main background",
    "battery bar",
    "air bar",
    "green dot",
    "yellow dot",
    "red dot",
];
/// Set `_ForeColor` on every widget called `name` under `widget`, including the bodies of its
/// attachments (`main background` is the `Image` of the status group's `Background`).
fn tint_named(widget: &mut Widget, name: &str, tint: &str) {
    if widget.name() == name {
        ra2ui::input::set_field(widget, "_ForeColor", tint);
    }
    for attachment in widget.attachments.iter_mut() {
        if let Some(body) = attachment.body.as_mut() {
            tint_named(body, name, tint);
        }
    }
    for child in widget.children.iter_mut() {
        tint_named(child, name, tint);
    }
}

/// The armour bar's and its remainder's tint, `(1, 0.5, 0.25, 1)`: live full armour reads
/// `redbar.bmp` rows `(193, 43, 22)..(255, 31, 0)` as `(193, 21, 5)..(255, 15, 0)`, G halved and B
/// quartered (`FUN_0061b4b0(1, 0.5, 0.25, 1)`, the colour `FUN_004b2f20` gives the remainder; the
/// armour widget `+0xd8` holds the same value at `+0xbc..+0xc8` in every dumped state).
pub const ARMOR_TINT: &str = "1 0.5 0.25 1";

/// The bars and their widths in `statusmeters.gib`.
pub const BARS: [(&str, f32); 3] = [
    ("battery bar", 52.0),
    ("air bar", 52.0),
    ("armor bar", 26.0),
];

/// One visible player's bound values, so a test can read what the screen says
/// without walking the tree again.
#[derive(Clone, Debug, PartialEq)]
pub struct HudPlayer {
    /// 0-based robot id (`player1 status` is 0).
    pub slot: usize,
    pub name: String,
    pub points: i32,
    pub battery: f32,
    pub air: f32,
    pub armor: f32,
    /// The lit status dot: 0 green, 1 yellow, 2 red (`FUN_004b3640`).
    pub dot: u8,
    /// The drawn widths after the left-side shrink.
    pub bar_widths: [f32; 3],
    /// Whether this slot's group is visible.
    pub visible: bool,
}

/// What a HUD binding produced.
#[derive(Clone, Debug, PartialEq)]
pub struct Hud {
    pub time: String,
    pub countdown: String,
    pub show_time: bool,
    pub show_countdown: bool,
    pub players: Vec<HudPlayer>,
}

/// Load `gamewindow.gib` with a `statusmeters.gib` in each of its four groups.
///
/// This is the constructor half; [`hud`] is the per-frame binding. The groups
/// keep the layout's own positions — nothing here invents a corner.
pub fn load(ui_root: &Path) -> Result<Gib, String> {
    let mut screen = ra2ui::workshop::load(ui_root, SCREEN)?;
    let template = ra2ui::workshop::load(ui_root, STATUS)?;
    let Some(status) = template
        .widgets()
        .find(|w| w.name() == STATUS_GROUP)
        .cloned()
    else {
        return Err(format!("{STATUS} has no {STATUS_GROUP:?}"));
    };
    for block in screen.blocks.iter_mut() {
        let ra2fmt::gib::Block::Widget(root) = block else {
            continue;
        };
        for slot in PLAYERS {
            let Some(group) = ra2ui::workshop::find_mut(root, slot) else {
                continue;
            };
            group.children.retain(|c| c.name() != STATUS_GROUP);
            group.children.push(status.clone());
        }
        // The intro is not implemented (no `ArenaIntros` player yet), so the
        // data's own instruction applies: hide it once the intro is over.
        if let Some(skip) = ra2ui::workshop::find_mut(root, "skip group") {
            ra2ui::input::set_field(skip, "Visible", "false");
        }
    }
    Ok(screen)
}

/// Bind the live match to the HUD screen.
///
/// `clock` is fight time in seconds (the match's own clock); `countdown_left`
/// is the seconds left before the bell. The returned [`Hud`] is the same data a
/// test reads, in the terms the screen draws.
pub fn hud(
    screen: &mut Widget,
    report: &MatchReport,
    state: MatchState,
    clock: f32,
    countdown_left: f32,
) -> Hud {
    let show_countdown = state == MatchState::Countdown;
    let intro = state == MatchState::Intro;
    // `0x416360` shows `time group` (`FUN_00477460`, the widget at `+0x1c8`)
    // with the match's `+0xd8` timed flag; a "No limit" match never shows the
    // box. `0x4758c0` hides it before the countdown.
    let show_time = !show_countdown && !intro && report.duration_limit.is_some();

    set_visible(screen, COUNTDOWN_GROUP, show_countdown);
    // The arena intro shows the `Skip Intro` button and nothing else of the HUD.
    set_visible(screen, "skip group", intro);
    set_visible(screen, TIME_GROUP, show_time);

    let countdown = countdown_text(countdown_left);
    let time = clock_text(clock, report.duration_limit);
    set_text(screen, COUNTDOWN_BOX, &countdown);
    set_text(screen, TIME_BOX, &time);

    let mut players = Vec::new();
    for (slot, group) in PLAYERS.iter().enumerate() {
        // The original's countdown frames (`arena-*-countdown`) show only the
        // camera name and the countdown box; the status meters appear at the
        // bell with the time box (`arena-*-f2`/`f4`).
        let visible = slot < report.robots.len() && !intro && !show_countdown;
        set_visible(screen, group, visible);
        if !visible {
            continue;
        }
        let robot = &report.robots[slot];
        // The status group is inside the player group, so the find has to
        // start from the group: the same names repeat in all four.
        let Some(group) = ra2ui::workshop::find_mut(screen, group) else {
            continue;
        };
        set_text(group, "name", &robot.bot_name);
        set_text(group, "points", &robot.points.to_string());
        let widths = [
            set_bar(group, BARS[0].0, BARS[0].1, robot.battery),
            set_bar(group, BARS[1].0, BARS[1].1, robot.air),
            set_bar(group, BARS[2].0, BARS[2].1, robot.armor),
        ];
        tint_named(group, "armor bar", ARMOR_TINT);
        set_armor_remainder(group, widths[2], !robot.defeated);
        // `FUN_00476bf0` -> `FUN_004b3850(1)` (from `FUN_00418040`, every kind of "out"): the
        // meters dim to 0.3 grey (`0x3e99999a`) on the background art, the three bars and the
        // three dots; the name and the points text keep their colour.
        let tint = if robot.defeated {
            DEFEATED_TINT
        } else {
            "1 1 1 1"
        };
        for name in DIMMED {
            tint_named(group, name, tint);
        }
        // `FUN_004b3640`: exactly one of the three dots shows.
        for (i, name) in DOTS.iter().enumerate() {
            set_visible(group, name, i == usize::from(robot.dot.min(2)));
        }
        players.push(HudPlayer {
            slot,
            name: robot.bot_name.clone(),
            points: robot.points,
            battery: robot.battery,
            air: robot.air,
            armor: robot.armor,
            dot: robot.dot.min(2),
            bar_widths: widths,
            visible,
        });
    }

    Hud {
        time,
        countdown,
        show_time,
        show_countdown,
        players,
    }
}

/// Put the camera's name in `camera textbox` and show or hide the box.
///
/// The camera belongs to the app, so this is separate from the match report's
/// binding. `visible` is the original's 3.0 s display window: `FUN_00477180`
/// shows the box and writes `+0x17c = 3.0` (`0x4771a2`); `0x4774f1` decrements
/// it and hides the box (`+0x178`) at zero.
pub fn set_camera(screen: &mut Widget, name: &str, visible: bool) {
    set_text(screen, CAMERA_BOX, name);
    set_visible(screen, CAMERA_BOX, visible);
}

/// Show or hide the chat group, put the typed line in `chat entry` and the
/// lines in `chat displaybox`.
///
/// The original opens the group on `\` (`0x475ee0`: show `+0x1b4`, focus
/// `+0x1b8`). `chat displaybox` is a `Glyph` dressed by a `DisplayBox` whose
/// markup is `<text font fg_mcn14>`; its text is the lines, one per row.
pub fn set_chat(screen: &mut Widget, open: bool, entry: &str, lines: &[String]) {
    set_visible(screen, "chat group", open);
    set_text(screen, "chat entry", entry);
    if let Some(display) = chat_display_mut(screen) {
        ra2ui::cells::bind_prose(display, &lines.join("\n"));
    }
}

/// The size of `chat displaybox` (`gamewindow.gib`: 200x100 at (10, 110)).
pub const CHAT_BOX: [f32; 2] = [200.0, 100.0];

/// The chat lines as `chat displaybox` shows them: each wrapped to the box's
/// width and, when they make more rows than the 100 px box holds, only the
/// newest rows (the box is `Scrollable`; the exact rule of the original's
/// display box was not recovered, so this keeps the tail, as a chat does).
pub fn fit_chat_lines(font: &ra2fmt::Font, lines: &[String]) -> Vec<String> {
    let pitch = (font.ascent as f32).max(1.0);
    let fit = (((CHAT_BOX[1] - font.line_height() as f32) / pitch).floor() as usize + 1).max(1);
    let rows: Vec<String> = lines
        .iter()
        .flat_map(|l| ra2ui::wrap_lines(font, l, CHAT_BOX[0]))
        .collect();
    let skip = rows.len().saturating_sub(fit);
    rows.into_iter().skip(skip).collect()
}

/// `chat displaybox`: the file's one `Glyph` (the box's paragraph, dressed by
/// the `DisplayBox`). The widget the file names `chat displaybox` has no name of
/// its own in the reader, so it is found by its class and its `DisplayBox`.
#[cfg(test)]
fn chat_display(screen: &Widget) -> Option<&Widget> {
    screen.children.iter().find(|c| {
        c.kind == WidgetKind::Glyph && ra2ui::cells::display_box(&c.attachments).is_some()
    })
}

fn chat_display_mut(screen: &mut Widget) -> Option<&mut Widget> {
    screen.children.iter_mut().find(|c| {
        c.kind == WidgetKind::Glyph && ra2ui::cells::display_box(&c.attachments).is_some()
    })
}

/// The HUD clock: `m:ss`.
///
/// With a configured match length the box counts down from it (`time counts
/// down here`, the file's own note); with "No limit" it counts up.
///
/// `FUN_004772f0(remaining)` (called every frame from `FUN_00412fc0` once the
/// timed match runs) writes `minutes = __ftol(remaining * 0x6a7c5c)` (the float
/// 1/60, `0x3c888889`) and `seconds = __ftol(remaining - 60 * minutes)`, both
/// truncating, the seconds padded with a `0` under 10 (`0x6f36f4`), joined by
/// `:` (`0x6f36f8`). So 166.9 s left reads `2:46` and the last second reads
/// `0:00` only at zero; the full `3:00` shows only before the first update.
pub fn clock_text(clock: f32, limit: Option<f32>) -> String {
    let (minutes, seconds) = match limit {
        Some(limit) => {
            let remaining = (limit - clock).max(0.0);
            let minutes = (remaining * f32::from_bits(0x3c88_8889)) as u32;
            (minutes, (remaining - (minutes * 60) as f32) as u32)
        }
        None => {
            let s = clock.max(0.0).floor() as u32;
            (s / 60, s % 60)
        }
    };
    format!("{minutes}:{seconds:02}")
}

/// The countdown number: `trunc` of the 4.0 s timer, while positive.
///
/// `0x477480` draws `trunc(+0x1dc)` (`0x63aabc`) and starts the fight when it
/// is zero (`0x4775bc`–`0x47766b`). So `3` shows for `(3, 4]`, `2` for
/// `(2, 3]`, `1` for `[1, 2]`, and the fight begins below 1.0. The file's
/// sample text is `3`; a value under 1.0 returns the empty string because the
/// countdown group is hidden by then.
pub fn countdown_text(left: f32) -> String {
    ra2sim::countdown_number(left)
        .map(|n| n.to_string())
        .unwrap_or_default()
}

/// Shrink a bar from the right, keeping its left edge, and return its width.
///
/// The original's setter (`0x4b3540` battery, `0x4b3570` air, `0x4b35a0`
/// armour) computes `width = (int)(fraction * base)` with `__ftol` and gives
/// that width to the bar; it never moves the bar's x. The armour setter also
/// places the white remainder at `x = 49 + width` with width `26 - width`
/// (`0x4b35e6`–`0x4b3636`), which is only coherent when the coloured fill keeps
/// its left edge and shrinks to the right. The fractional part is dropped, as
/// the original's `__ftol` does.
fn set_bar(group: &mut Widget, name: &str, base_width: f32, fraction: f32) -> f32 {
    let Some(bar) = ra2ui::workshop::find_mut(group, name) else {
        return 0.0;
    };
    let fraction = fraction.clamp(0.0, 1.0);
    let width = (base_width * fraction).trunc();
    let x = bar.position().map(|p| p.first.amount).unwrap_or(0.0);
    let height = bar.size().map(|s| s.second.amount).unwrap_or(4.0);
    let y = bar.position().map(|p| p.second.amount).unwrap_or(0.0);
    // `Size` is what the art is drawn at; the position is unchanged (the left edge).
    ra2ui::input::set_field(bar, "Position", &format!("{x} 0 {y} 0"));
    ra2ui::input::set_field(bar, "Size", &format!("{width} 0 {height} 0"));
    width
}

/// The armour bar's remainder, `+0xdc` of the status meter: `FUN_004b2f20` makes an image of
/// `common\whitebar.bmp` (26 x 4 at (75, 39)) tinted `(1, 0.5, 0.25, 1)` (`FUN_0061b4b0`) and adds it
/// to `status group` after the bars; the armour setter `0x4b35a0` gives it `x = 49 + w` and
/// width `26 - w` for a coloured fill of `w`. Live it is not drawn once the robot is out: a dimmed
/// panel with armour 0 shows the empty track of `status_meters.tga` (rows `(72, 60, 59)..(97, 77, 75)`
/// at 0.3, `(21, 18, 18)..(29, 23, 22)`), not the orange remainder of an undimmed panel with armour 0.
pub const ARMOR_REMAINDER: &str = "armor remainder";
const WHITEBAR_FILE: &str = "common\\whitebar.bmp";

fn set_armor_remainder(group: &mut Widget, fill: f32, shown: bool) {
    if ra2ui::workshop::find_mut(group, ARMOR_REMAINDER).is_none() {
        let Some(parent) = parent_of_mut(group, "armor bar") else {
            return;
        };
        let Some(bar) = parent.children.iter().find(|c| c.name() == "armor bar") else {
            return;
        };
        let mut white = bar.clone();
        ra2ui::input::set_field(&mut white, "Name", ARMOR_REMAINDER);
        ra2ui::input::set_field(&mut white, "_Filename", WHITEBAR_FILE);
        ra2ui::input::set_field(&mut white, "_ForeColor", ARMOR_TINT);
        ra2ui::input::set_field(&mut white, "_Size", "26 0 4 0");
        parent.children.push(white);
    }
    if let Some(white) = ra2ui::workshop::find_mut(group, ARMOR_REMAINDER) {
        ra2ui::input::set_field(white, "Position", &format!("{} 0 39 0", 49.0 + fill));
        ra2ui::input::set_field(white, "Size", &format!("{} 0 4 0", 26.0 - fill));
        ra2ui::input::set_field(
            white,
            "Visible",
            if shown && fill < 26.0 {
                "true"
            } else {
                "false"
            },
        );
    }
}

fn parent_of_mut<'a>(widget: &'a mut Widget, name: &str) -> Option<&'a mut Widget> {
    if widget.children.iter().any(|c| c.name() == name) {
        return Some(widget);
    }
    widget
        .children
        .iter_mut()
        .find_map(|c| parent_of_mut(c, name))
}

fn set_visible(root: &mut Widget, name: &str, visible: bool) {
    if let Some(widget) = ra2ui::workshop::find_mut(root, name) {
        let value = if visible { "true" } else { "false" };
        ra2ui::input::set_field(widget, "Visible", value);
        // The group's `Background`/`Border` bodies were saved with the group's
        // own visibility (`countdown group` and `time group` were hidden in the
        // editor, so their 0.4 black panel and black border bodies say
        // `Visible false`); they show and hide with the owner.
        for attachment in &mut widget.attachments {
            if let Some(body) = attachment.body.as_mut() {
                ra2ui::input::set_field(body, "Visible", value);
            }
        }
    }
}

/// The immobility warning's group, the one the original hangs over a robot that has not
/// moved (`FUN_004749c0` loads it per slot, `FUN_00412390` writes its number).
pub const IMMOBILE_COUNTDOWN: &str = "common/immobility_countdown.gib";
/// The text box of [`IMMOBILE_COUNTDOWN`] that carries the number.
pub const IMMOBILE_NUMBER: &str = "immobile countdown";
/// The card `FUN_00418040` shows over a robot's status group when it goes out, by the state it
/// is called with: 1 a destroyed chassis (the third critical, `FUN_00417cc0`), 2 immobilized
/// (`FUN_00412fc0` at 20 s), 3 eliminated (`plus.eliminatePlayer`, `0x4b9c40`). The three images
/// are loaded by `FUN_004749c0` into `+0x258` (`0x477930`), `+0x268` (`0x477940`) and `+0x248`
/// (`0x477920`) per slot, and shown for [`IMMOBILE_CARD_SECONDS`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutCard {
    ControlBoardDestroyed,
    Immobile,
    Eliminated,
}

impl OutCard {
    /// The card for a robot's state (`FUN_00413810`): 1, 2 or 3; 0 has none.
    pub fn for_state(state: u8) -> Option<OutCard> {
        match state {
            1 => Some(OutCard::ControlBoardDestroyed),
            2 => Some(OutCard::Immobile),
            3 => Some(OutCard::Eliminated),
            _ => None,
        }
    }

    /// The `UI` image the original loads (`0x6f334c`, `0x6f3318`, `0x6f3330`).
    pub fn file(self) -> &'static str {
        match self {
            OutCard::ControlBoardDestroyed => "common\\controlboard_destroyed.tga",
            OutCard::Immobile => IMMOBILE_CARD,
            OutCard::Eliminated => "common\\eliminated.tga",
        }
    }

    /// The image's own size in pixels.
    pub fn size(self) -> [f32; 2] {
        match self {
            OutCard::ControlBoardDestroyed => [112.0, 64.0],
            OutCard::Immobile => IMMOBILE_CARD_SIZE,
            OutCard::Eliminated => [116.0, 32.0],
        }
    }
}

/// `UI\common\immobile.tga`, 104 x 32.
pub const IMMOBILE_CARD: &str = "common\\immobile.tga";
pub const IMMOBILE_CARD_SIZE: [f32; 2] = [104.0, 32.0];
/// How long the card stays (`0x6a5784`, the time `0x4e2660` gives the popup list at `+0x100`,
/// which `FUN_00416210` counts down by `dt` and hides at zero).
pub const IMMOBILE_CARD_SECONDS: f32 = 3.0;
/// The counter hangs `60.0` px above the projected point (`0x6aece8`, `0x5dc5cb`), centred on it.
pub const IMMOBILE_COUNTER_RISE: f32 = 60.0;

/// The number the counter shows after `seconds` of stillness: `FUN_00412390` writes
/// `trunc(10 - (seconds - 10))` capped at 10, and hides the group at 0 or less.
pub fn immobile_number(seconds: f32) -> Option<i32> {
    let n = (20.0 - seconds) as i32;
    (n > 0).then_some(n.min(10))
}

fn immobile_root(gib: &mut Gib) -> Option<&mut Widget> {
    gib.blocks.iter_mut().find_map(|b| match b {
        ra2fmt::gib::Block::Widget(w) => Some(w),
        _ => None,
    })
}

fn place(widget: &mut Widget, at: [f32; 2]) {
    ra2ui::input::set_field(
        widget,
        "Position",
        &format!("{} 0 {} 0", at[0] as i32, at[1] as i32),
    );
}

/// The counter group showing `n`, with its top-left at `at` (screen pixels).
pub fn immobile_counter(base: &Gib, n: i32, at: [f32; 2]) -> Gib {
    let mut gib = base.clone();
    if let Some(root) = immobile_root(&mut gib) {
        place(root, at);
        set_text(root, IMMOBILE_NUMBER, &n.to_string());
    }
    gib
}

/// An out card with its top-left at `at`.
pub fn out_card(base: &Gib, card: OutCard, at: [f32; 2]) -> Gib {
    let mut gib = base.clone();
    if let Some(root) = immobile_root(&mut gib) {
        let [w, h] = card.size();
        let size = format!("{} 0 {} 0", w as i32, h as i32);
        place(root, at);
        ra2ui::input::set_field(root, "Size", &size);
        root.children.retain(|c| c.kind == WidgetKind::Image);
        for image in root.children.iter_mut() {
            ra2ui::input::set_field(image, "_Filename", card.file());
            ra2ui::input::set_field(image, "_Size", &size);
            ra2ui::input::set_field(image, "Size", &size);
        }
    }
    gib
}

/// The `IMMOBILE!` card with its top-left at `at`.
pub fn immobile_card(base: &Gib, at: [f32; 2]) -> Gib {
    out_card(base, OutCard::Immobile, at)
}

/// Where a card goes for a status group at `frame` (x, y, w, h): centred on it in whole
/// pixels, as `FUN_00418040` does it (`pos - (card - group) / 2`, each half truncating).
pub fn out_card_at(card: OutCard, group: [f32; 4]) -> [f32; 2] {
    let [w, h] = card.size();
    let dx = ((w - group[2]) as i32 / 2) as f32;
    let dy = ((h - group[3]) as i32 / 2) as f32;
    [group[0] - dx, group[1] - dy]
}

/// [`out_card_at`] for the `IMMOBILE!` card.
pub fn immobile_card_at(group: [f32; 4]) -> [f32; 2] {
    out_card_at(OutCard::Immobile, group)
}

/// Set the text a `TextBox` draws. The visible value is its `Text` child's
/// `_Text` (every `gamewindow.gib`/`statusmeters.gib` box carries one); a plain
/// label is written directly.
fn set_text(root: &mut Widget, name: &str, value: &str) {
    let Some(widget) = ra2ui::workshop::find_mut(root, name) else {
        return;
    };
    if let Some(text) = widget
        .children
        .iter_mut()
        .find(|c| c.kind == WidgetKind::Text)
    {
        ra2ui::input::set_field(text, "_Text", value);
    } else {
        ra2ui::input::set_field(widget, "_Text", value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2sim::r#match::RobotReport;

    fn game_dir() -> Option<std::path::PathBuf> {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
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

    fn robot(slot: usize, name: &str, points: i32) -> RobotReport {
        RobotReport {
            slot,
            team: slot,
            bot_name: name.into(),
            team_name: format!("Team {}", slot + 1),
            team_logo: String::new(),
            bot_preview: None,
            points,
            damage_dealt: 0.0,
            damage_taken: 0.0,
            battery: 0.5,
            air: 0.25,
            armor: 0.75,
            dot: 1,
            defeated: slot == 1,
            eliminated: false,
            winner: slot == 0,
            immobilized: false,
            components: Vec::new(),
        }
    }

    fn report() -> MatchReport {
        MatchReport {
            game_type: "DEATHMATCH",
            arena: "box".into(),
            state: MatchState::Fight,
            reason: None,
            winners: vec![0],
            duration: 3.0,
            duration_limit: Some(10.0),
            robots: vec![robot(0, "Roly Poly", 7), robot(1, "Little Metal Friend", 3)],
            log: Vec::new(),
        }
    }

    fn text_of(root: &ra2ui::Placement<'_>, name: &str) -> String {
        root.find(name)
            .and_then(|p| {
                p.widget
                    .children
                    .iter()
                    .find(|c| c.kind == WidgetKind::Text)
                    .and_then(|c| c.field("_Text"))
                    .map(|f| f.text().to_string())
            })
            .unwrap_or_default()
    }

    /// The clock is the configured length counting down, and the countdown is
    /// `trunc(timer)` — 3, 2, 1 for one second each, with the fight starting
    /// at 1.0 s remaining (`0x477480`).
    #[test]
    fn the_clock_counts_down_from_the_configured_length() {
        assert_eq!(clock_text(0.0, Some(10.0)), "0:10");
        assert_eq!(
            clock_text(9.5, Some(10.0)),
            "0:00",
            "truncated, not rounded up"
        );
        assert_eq!(clock_text(8.5, Some(10.0)), "0:01");
        assert_eq!(clock_text(13.1, Some(180.0)), "2:46");
        assert_eq!(clock_text(120.0, Some(180.0)), "1:00");
        assert_eq!(clock_text(10.0, Some(10.0)), "0:00");
        assert_eq!(clock_text(65.0, Some(300.0)), "3:55");
        // "No limit" counts up (but the time group is hidden; see below).
        assert_eq!(clock_text(65.0, None), "1:05");

        assert_eq!(countdown_text(4.0), "3");
        assert_eq!(countdown_text(3.5), "3");
        assert_eq!(countdown_text(3.0), "3");
        assert_eq!(countdown_text(2.999), "2");
        assert_eq!(countdown_text(2.0), "2");
        assert_eq!(countdown_text(1.5), "1");
        assert_eq!(countdown_text(1.0), "1");
        assert_eq!(countdown_text(0.999), "", "the fight has begun");
    }

    /// The countdown group's `Background` (0.4 black panel) and `Border` bodies
    /// are saved `Visible false`; they follow the group, or the box has no panel.
    #[test]
    fn the_countdown_panel_and_border_show_with_the_group() {
        let Some(game) = game_dir() else { return };
        let mut screen = load(&game.join("UI")).unwrap();
        let root = screen
            .blocks
            .iter_mut()
            .find_map(|b| match b {
                ra2fmt::gib::Block::Widget(w) => Some(w),
                _ => None,
            })
            .unwrap();
        let bodies = |root: &Widget| {
            let group = ra2ui::workshop::find(root, COUNTDOWN_GROUP).unwrap();
            group
                .attachments
                .iter()
                .filter_map(|a| a.body.as_ref())
                .map(|b| b.visible())
                .collect::<Vec<_>>()
        };
        hud(root, &report(), MatchState::Fight, 3.0, 0.0);
        let hidden = bodies(root);
        assert!(!hidden.is_empty() && hidden.iter().all(|v| !v));
        hud(root, &report(), MatchState::Countdown, 0.0, 3.2);
        let shown = bodies(root);
        assert_eq!(shown.len(), hidden.len());
        assert!(shown.iter().all(|v| *v), "panel and border are drawn");
    }

    /// Countdown and time groups swap exactly as their own notes say, and a
    /// two-bot match shows two of the four status groups.
    #[test]
    fn the_hud_shows_the_countdown_then_the_time_and_only_two_players() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let mut screen = load(&ui).unwrap();
        let root = screen
            .blocks
            .iter_mut()
            .find_map(|b| match b {
                ra2fmt::gib::Block::Widget(w) => Some(w),
                _ => None,
            })
            .unwrap();

        let countdown = hud(root, &report(), MatchState::Countdown, 0.0, 3.2);
        assert!(countdown.show_countdown && !countdown.show_time);
        assert_eq!(countdown.countdown, "3");
        assert!(
            countdown.players.is_empty(),
            "the original's countdown frames draw no status meters"
        );
        assert!(!ra2ui::workshop::find(root, "player1 status")
            .unwrap()
            .visible());
        assert!(ra2ui::workshop::find(root, COUNTDOWN_GROUP)
            .unwrap()
            .visible());
        assert!(!ra2ui::workshop::find(root, TIME_GROUP).unwrap().visible());
        assert!(!ra2ui::workshop::find(root, "player3 status")
            .unwrap()
            .visible());
        assert!(!ra2ui::workshop::find(root, "player4 status")
            .unwrap()
            .visible());
        assert!(!ra2ui::workshop::find(root, "skip group").unwrap().visible());

        let fight = hud(root, &report(), MatchState::Fight, 3.0, 0.0);
        assert!(fight.show_time && !fight.show_countdown);
        assert_eq!(fight.players.len(), 2, "only two bots are in the match");
        assert_eq!(fight.time, "0:07");
        assert!(ra2ui::workshop::find(root, TIME_GROUP).unwrap().visible());
        assert!(!ra2ui::workshop::find(root, COUNTDOWN_GROUP)
            .unwrap()
            .visible());
    }

    /// The names and points land in the slot the player group names, and the
    /// bars fill from the left to the fractions the report carries.
    #[test]
    fn the_status_group_takes_the_reports_names_points_and_bars() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let mut screen = load(&ui).unwrap();
        let report = report();
        let mut assets = ra2_render::ui::UiAssets::open(&game).unwrap();
        let root = screen
            .blocks
            .iter_mut()
            .find_map(|b| match b {
                ra2fmt::gib::Block::Widget(w) => Some(w),
                _ => None,
            })
            .unwrap();
        let bound = hud(root, &report, MatchState::Fight, 3.0, 0.0);
        assert_eq!(bound.players[0].name, "Roly Poly");
        assert_eq!(bound.players[0].points, 7);
        assert_eq!(bound.players[0].bar_widths, [26.0, 13.0, 19.0]);
        assert_eq!(bound.players[0].dot, 1, "one critical hit: the yellow dot");

        let placed = ra2ui::resolve_with(&screen, crate::SHELL_SIZE, Some(assets.faces()));
        let first = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "player1 status")
            .expect("player1 status");
        assert_eq!(text_of(first, "name"), "Roly Poly");
        assert_eq!(text_of(first, "points"), "7");
        let second = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "player2 status")
            .expect("player2 status");
        assert_eq!(text_of(second, "name"), "Little Metal Friend");
        assert_eq!(text_of(second, "points"), "3");

        // Battery: 52 px at 50% = 26 px, and x stays at the file's 23. The
        // original's `(int)(fraction * 52)` leaves the left edge alone.
        let bar = first.find("battery bar").expect("battery bar");
        assert!((bar.frame.w - 26.0).abs() < 0.01);
        assert!(
            (bar.frame.x - first.frame.x - 23.0).abs() < 0.01,
            "the bar keeps its left edge at 23 inside the status group"
        );
        // Armour: 26 * 0.75 = 19.5, truncated to 19 like `__ftol`.
        let armor = first.find("armor bar").expect("armor bar");
        assert!((armor.frame.w - 19.0).abs() < 0.01);
        assert!(
            (armor.frame.x - first.frame.x - 49.0).abs() < 0.01,
            "49 is the armour bar's own left edge"
        );
        // The remainder (`+0xdc`): `49 + w` wide `26 - w`, here 68..75 of the group. The robot
        // is in, so it shows; the robot that is out (slot 2) hides it and keeps the armour bar
        // at its own tint while the panel dims (live dump).
        let rest = first.find(ARMOR_REMAINDER).expect("the armour remainder");
        assert!((rest.frame.w - 7.0).abs() < 0.01, "26 - 19");
        assert!(
            (rest.frame.x - first.frame.x - 68.0).abs() < 0.01,
            "49 + 19"
        );
        assert!(rest.widget.visible());
        let out = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "player2 status")
            .unwrap();
        assert!(!out.find(ARMOR_REMAINDER).unwrap().widget.visible());
        let tint = |p: &ra2ui::Placement, name: &str| {
            p.find(name)
                .and_then(|w| w.widget.field("_ForeColor").map(|f| f.raw.clone()))
        };
        assert_eq!(tint(out, "armor bar").as_deref(), Some(ARMOR_TINT));
        assert_eq!(tint(out, "air bar").as_deref(), Some(DEFEATED_TINT));
        assert_eq!(tint(first, "air bar").as_deref(), Some("1 1 1 1"));
        let _ = assets.scene(&placed, [1280, 1005]);
    }

    #[test]
    fn the_out_cards_sit_centred_on_the_status_group_by_their_own_size() {
        let group = [11.0, 16.0, 84.0, 64.0];
        // 112 x 64 (`controlboard_destroyed`): (112 - 84) / 2 = 14 left, level.
        assert_eq!(
            out_card_at(OutCard::ControlBoardDestroyed, group),
            [-3.0, 16.0]
        );
        // 116 x 32 (`eliminated`): 16 left, (32 - 64) / 2 = -16 so 16 down.
        assert_eq!(out_card_at(OutCard::Eliminated, group), [-5.0, 32.0]);
        assert_eq!(OutCard::for_state(1), Some(OutCard::ControlBoardDestroyed));
        assert_eq!(OutCard::for_state(2), Some(OutCard::Immobile));
        assert_eq!(OutCard::for_state(3), Some(OutCard::Eliminated));
        assert_eq!(OutCard::for_state(0), None);
    }

    fn four_report() -> MatchReport {
        let mut report = report();
        report.robots = vec![
            robot(0, "Roly Poly", 1),
            robot(1, "Little Metal Friend", 2),
            robot(2, "Third Bot", 3),
            robot(3, "Fourth Bot", 4),
        ];
        report
    }

    /// `0x4749c0` attaches the four meters to `player1..4 status` in order,
    /// and the file's corners are 1 top-left, 2 top-right, 3 bottom-left,
    /// 4 bottom-right.
    #[test]
    fn the_four_status_groups_take_the_robots_in_order_from_the_top_left() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let mut screen = load(&ui).unwrap();
        let mut assets = ra2_render::ui::UiAssets::open(&game).unwrap();
        let report = four_report();
        let root = screen
            .blocks
            .iter_mut()
            .find_map(|b| match b {
                ra2fmt::gib::Block::Widget(w) => Some(w),
                _ => None,
            })
            .unwrap();
        let bound = hud(root, &report, MatchState::Fight, 1.0, 0.0);
        assert_eq!(bound.players.len(), 4);
        for (i, player) in bound.players.iter().enumerate() {
            assert_eq!(player.slot, i);
            assert_eq!(player.name, report.robots[i].bot_name);
            assert_eq!(player.points, i as i32 + 1);
            assert!(player.visible);
        }

        let placed = ra2ui::resolve_with(&screen, crate::SHELL_SIZE, Some(assets.faces()));
        let group = |name: &str| {
            placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == name)
                .unwrap_or_else(|| panic!("{name}"))
        };
        let p1 = group("player1 status");
        let p2 = group("player2 status");
        let p3 = group("player3 status");
        let p4 = group("player4 status");
        assert_eq!(text_of(p1, "name"), "Roly Poly");
        assert_eq!(text_of(p2, "name"), "Little Metal Friend");
        assert_eq!(text_of(p3, "name"), "Third Bot");
        assert_eq!(text_of(p4, "name"), "Fourth Bot");
        // 1 and 2 are the top row; 3 and 4 the bottom row.
        let mid_x = crate::SHELL_SIZE[0] / 2.0;
        let mid_y = crate::SHELL_SIZE[1] / 2.0;
        assert!(p1.frame.x < mid_x && p1.frame.y < mid_y, "player1 {p1:?}");
        assert!(p2.frame.x > mid_x && p2.frame.y < mid_y, "player2 {p2:?}");
        assert!(p3.frame.x < mid_x && p3.frame.y > mid_y, "player3 {p3:?}");
        assert!(p4.frame.x > mid_x && p4.frame.y > mid_y, "player4 {p4:?}");
        let _ = assets.scene(&placed, [1280, 1005]);
    }

    /// The camera box carries the camera object's name and is only shown for
    /// the original's 3.0 s after a change (`0x4771a2`, `0x4774f1`).
    #[test]
    fn the_camera_textbox_shows_the_cameras_name_then_hides() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let mut screen = load(&ui).unwrap();
        let root = screen
            .blocks
            .iter_mut()
            .find_map(|b| match b {
                ra2fmt::gib::Block::Widget(w) => Some(w),
                _ => None,
            })
            .unwrap();
        set_camera(root, "Orbit Cam", true);
        let box_ = ra2ui::workshop::find(root, CAMERA_BOX).unwrap();
        assert!(box_.visible());
        assert_eq!(text_of_widget(box_), "Orbit Cam");
        set_camera(root, "Orbit Cam", false);
        assert!(!ra2ui::workshop::find(root, CAMERA_BOX).unwrap().visible());

        assert_eq!(ra2sim::DuelCamera::Watch.display_name(), "Watch Cam");
        assert_eq!(ra2sim::DuelCamera::Static.display_name(), "Static Cam");
        assert_eq!(
            ra2sim::DuelCamera::Orbit { period: 24.0 }.display_name(),
            "Orbit Cam"
        );
        assert_eq!(
            ra2sim::DuelCamera::Chase {
                robot: 0,
                distance: 4.0,
                height: 2.0
            }
            .display_name(),
            "Chase Cam (ours)"
        );
    }

    /// The chat group opens on the backslash key (the file's own note and
    /// `0x475ee0`); this is the open/close minimum, with no network.
    #[test]
    fn the_chat_group_opens_and_closes_with_the_entry_text() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let mut screen = load(&ui).unwrap();
        let root = screen
            .blocks
            .iter_mut()
            .find_map(|b| match b {
                ra2fmt::gib::Block::Widget(w) => Some(w),
                _ => None,
            })
            .unwrap();
        assert!(!ra2ui::workshop::find(root, "chat group").unwrap().visible());
        set_chat(root, true, "hello", &[]);
        assert!(ra2ui::workshop::find(root, "chat group").unwrap().visible());
        assert_eq!(
            text_of_widget(ra2ui::workshop::find(root, "chat entry").unwrap()),
            "hello"
        );
        set_chat(root, false, "", &[]);
        assert!(!ra2ui::workshop::find(root, "chat group").unwrap().visible());

        // `chat displaybox` carries the lines, one per row, in its own
        // `<text font fg_mcn14>` markup.
        let lines = ["[A] one".to_string(), "[B] two".to_string()];
        set_chat(root, false, "", &lines);
        let display = chat_display(root).expect("the chat display glyph");
        let markup = ra2ui::cells::display_box(&display.attachments)
            .and_then(|a| a.field("Display"))
            .map(|f| f.text().to_string())
            .unwrap();
        assert!(markup.starts_with("<text font fg_mcn14>"), "{markup}");
        assert!(markup.contains("[A] one\n[B] two"), "{markup}");
    }

    /// `chat displaybox` is 200x100: lines wrap at 200 px and only the newest
    /// rows that fit in 100 px stay (docs/38 section 11; `chatshot`).
    #[test]
    fn the_chat_box_wraps_at_its_width_and_keeps_the_newest_rows_that_fit() {
        let Some(game) = game_dir() else { return };
        let fonts = ra2ui::Fonts::load(&game.join("RA2.cfz")).unwrap();
        let font = fonts.get("fg_mcn14").expect("fg_mcn14");
        let long = "[A] one two three four five six seven eight nine ten eleven twelve";
        let rows = fit_chat_lines(font, &[long.to_string()]);
        assert!(rows.len() >= 2, "{rows:?}");
        assert!(rows.iter().all(|r| font.extent(r).0 as f32 <= CHAT_BOX[0]));
        let many: Vec<String> = (0..8).map(|i| format!("[{i}] {long}")).collect();
        let rows = fit_chat_lines(font, &many);
        let pitch = font.ascent as f32;
        let last = (rows.len() - 1) as f32 * pitch + font.line_height() as f32;
        assert!(last <= CHAT_BOX[1], "{} rows reach {last}", rows.len());
        assert!(rows.last().unwrap().contains("twelve"), "newest kept");
        assert!(rows.first().unwrap().starts_with("[7]") || !rows[0].starts_with("[0]"));
    }

    /// A "No limit" match never shows the time group: `0x416360` calls
    /// `FUN_00477460` with the timed byte, and `0x4758c0` hid the group before
    /// the countdown.
    #[test]
    fn a_no_limit_match_hides_the_time_group() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let mut screen = load(&ui).unwrap();
        let mut report = report();
        report.duration_limit = None;
        let root = screen
            .blocks
            .iter_mut()
            .find_map(|b| match b {
                ra2fmt::gib::Block::Widget(w) => Some(w),
                _ => None,
            })
            .unwrap();
        let bound = hud(root, &report, MatchState::Fight, 5.0, 0.0);
        assert!(!bound.show_time);
        assert!(
            !ra2ui::workshop::find(root, TIME_GROUP).unwrap().visible(),
            "time group is hidden without a clock"
        );
    }

    fn text_of_widget(widget: &Widget) -> String {
        widget
            .children
            .iter()
            .find(|c| c.kind == WidgetKind::Text)
            .and_then(|c| c.field("_Text"))
            .or_else(|| widget.field("_Text"))
            .map(|f| f.text().to_string())
            .unwrap_or_default()
    }
}
