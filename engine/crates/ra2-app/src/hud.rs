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
//! `+0xbc` (`FUN_00416940`, 0 green / 1 yellow / 2 red). What those fields are
//! made of — the energy model behind `Battery`/`AirTank` — is not established,
//! so the fraction each bar receives here is still named in
//! [`ra2sim::match::RobotReport`] and is our component-hitpoint mapping. The
//! fill geometry and truncation are the binary's.
//!
//! ## What is not drawn
//!
//! The arena intro (`skip group`) is not implemented, so the group is hidden.
//! The chat widgets are left hidden as the file writes them. The camera name
//! comes from [`set_camera`], because the camera is the app's state, not the
//! match's.
//!
//! **Which robot gets which corner** is not established: `0x4749c0` loads all
//! four `playerN status` groups, but the recon did not trace the assignment.
//! This port puts robot 0 in `player1 status`, robot 1 in `player2 status`, and
//! hides 3/4 when the match has two robots; that is named as our reading.

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
    let show_time = !show_countdown;

    set_visible(screen, COUNTDOWN_GROUP, show_countdown);
    set_visible(screen, TIME_GROUP, show_time);

    let countdown = countdown_text(countdown_left);
    let time = clock_text(clock, report.duration_limit);
    set_text(screen, COUNTDOWN_BOX, &countdown);
    set_text(screen, TIME_BOX, &time);

    let mut players = Vec::new();
    for (slot, group) in PLAYERS.iter().enumerate() {
        let visible = slot < report.robots.len();
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
        players.push(HudPlayer {
            slot,
            name: robot.bot_name.clone(),
            points: robot.points,
            battery: robot.battery,
            air: robot.air,
            armor: robot.armor,
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

/// Put the camera's config name in `camera textbox`. The camera belongs to the
/// app, so this is separate from the match report's binding.
pub fn set_camera(screen: &mut Widget, name: &str) {
    set_text(screen, CAMERA_BOX, name);
}

/// The HUD clock: `m:ss`.
///
/// With a configured match length the box counts down from it (`time counts
/// down here`, the file's own note); with "No limit" it counts up. The remaining
/// seconds are rounded up so the first frame shows the full length.
pub fn clock_text(clock: f32, limit: Option<f32>) -> String {
    let seconds = match limit {
        Some(limit) => (limit - clock).max(0.0).ceil() as u32,
        None => clock.max(0.0).floor() as u32,
    };
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The countdown number: 3, 2, 1 over the recovered 4.0 s.
///
/// The last second keeps showing `1`; the exact per-second schedule inside
/// `0x477480` was not traced beyond the 4.0 s timer, and the file's sample text
/// is `3`.
pub fn countdown_text(left: f32) -> String {
    (((left - 1.0).ceil() as i32).clamp(1, 3)).to_string()
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
    // `_Size` is what the art is drawn at; `Size` keeps the record consistent
    // for a later reader, and the position is unchanged (the left edge).
    ra2ui::input::set_field(bar, "Position", &format!("{x} 0 {y} 0"));
    ra2ui::input::set_field(bar, "Size", &format!("{width} 0 {height} 0"));
    ra2ui::input::set_field(bar, "_Size", &format!("{width} 0 {height} 0"));
    width
}

fn set_visible(root: &mut Widget, name: &str, visible: bool) {
    if let Some(widget) = ra2ui::workshop::find_mut(root, name) {
        ra2ui::input::set_field(widget, "Visible", if visible { "true" } else { "false" });
    }
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
    /// 3/2/1 over the recovered 4.0 s.
    #[test]
    fn the_clock_counts_down_from_the_configured_length() {
        assert_eq!(clock_text(0.0, Some(10.0)), "0:10");
        assert_eq!(clock_text(9.5, Some(10.0)), "0:01");
        assert_eq!(clock_text(10.0, Some(10.0)), "0:00");
        assert_eq!(clock_text(65.0, Some(300.0)), "3:55");
        // "No limit" counts up.
        assert_eq!(clock_text(65.0, None), "1:05");

        assert_eq!(countdown_text(4.0), "3");
        assert_eq!(countdown_text(3.5), "3");
        assert_eq!(countdown_text(3.0), "2");
        assert_eq!(countdown_text(2.0), "1");
        assert_eq!(countdown_text(0.5), "1");
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
        assert_eq!(countdown.players.len(), 2, "only two bots are in the match");
        assert!(countdown.players.iter().all(|p| p.visible));
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
        let _ = assets.scene(&placed, [1280, 1005]);
    }
}
