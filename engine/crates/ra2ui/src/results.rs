//! The match results: `common/matchresults.gib`'s four slots.
//!
//! The screen is not a grid. Its own `Notes` say, four times, *"load
//! result_cell.gib into this slot with appropriate data"*, and the slots are the
//! four groups it names `result slot 1` … `result slot 4` — 478×80 each, at the
//! positions the file gives. So the rule is the same as [`crate::cells`]: the
//! template is instantiated at the list's own geometry, and a row's values go
//! into the children the template's own `Notes` name.
//!
//! ## What the template names
//!
//! `common/result_cell.gib` says *"load group into result slots; set 'team logo'
//! and 'bot preview' images as well as other noted gui items"*, and its children
//! carry the rest:
//!
//! | child | its note |
//! |---|---|
//! | `team logo` | *"set 'team logo' … images"* |
//! | `bot preview` | *"… and 'bot preview' images"* |
//! | `points` | *"display the bot's total points here"* |
//! | `bot name` | *"display bot name"* |
//! | `team name` | *"display team name"* |
//! | `immobilized text` | *"hide this unless the bot was immobilized during the match"* |
//! | `winner text` | *"show this for the bot(s) who win this match or round"* |
//!
//! The template's constructor (`ResultCell`, `FUN_00487480`) hides
//! `immobilized text`, `winner text` and `Spectator [TextBox]` before anything is
//! filled, even though the file writes `Visible true`. [`fill`] applies those
//! defaults and then shows the two state labels the result calls for.
//!
//! ## The source is the match, and it never ships
//!
//! The game fills these slots from the live match (`FUN_00488120`): one result
//! cell per bot in the match, each taking that bot's own state — team logo and
//! team name from the team, the bot's preview and name, the match points, and
//! the immobilized/winner flags. There is no table in the game files for this;
//! the screen's own sample strings (`248`, `The Wrangler`, `TEAM STINKBOMBS`)
//! are editor placeholders. [`MatchResult`] is the shape the screen needs; the
//! caller supplies it from its match state.

use ra2fmt::gib::{Gib, Widget};

/// The four slots, in the screen's own order and by their own names.
pub const SLOTS: [&str; 4] = [
    "result slot 1",
    "result slot 2",
    "result slot 3",
    "result slot 4",
];

/// The cell template, from the slots' own note.
pub const TEMPLATE: &str = "common/result_cell.gib";

/// The template's root, which is what a slot receives.
pub const CELL: &str = "result group";

/// The children a result cell binds, by the names the template's own notes give
/// them.
pub const TEAM_LOGO: &str = "team logo";
pub const BOT_PREVIEW: &str = "bot preview";
pub const POINTS: &str = "points";
pub const BOT_NAME: &str = "bot name";
pub const TEAM_NAME: &str = "team name";
pub const IMMOBILIZED: &str = "immobilized text";
pub const WINNER: &str = "winner text";
pub const SPECTATOR: &str = "Spectator [TextBox]";

/// The shipped placeholder a bot without a snapshot keeps.
pub const PLACEHOLDER: &str = "exhibition\\sample_bot.bmp";

/// One bot's result in a match: the shape `result_cell.gib` draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchResult {
    /// The team's logo, a game-relative path as `_Filename` wants it.
    pub team_logo: String,
    /// The bot's picture: a game-relative path, or the synthetic key
    /// [`crate::preview::Preview::key`] gives the `.bot`'s own snapshot.
    pub bot_preview: String,
    /// The decoded snapshot behind [`MatchResult::bot_preview`], when the bot
    /// has one. The caller registers it with the renderer's `UiAssets` before
    /// the scene is built; the picture lives in memory and is never written to
    /// a file.
    pub bot_preview_image: Option<crate::preview::Preview>,
    /// The bot's points in the match.
    pub points: i32,
    pub bot_name: String,
    pub team_name: String,
    /// *"hide this unless the bot was immobilized during the match"*.
    pub immobilized: bool,
    /// *"show this for the bot(s) who win this match or round"*.
    pub winner: bool,
}

/// Load one result cell into each of the screen's slots, in order, and bind it.
///
/// Returns how many slots were filled. Slots with no result are left as the file
/// writes them; the match only supplies as many results as there were bots.
pub fn fill(screen: &mut Widget, template: &Gib, results: &[MatchResult]) -> usize {
    let Some(cell) = template.widgets().find(|w| w.name() == CELL) else {
        return 0;
    };
    let mut made = 0;
    for (slot, result) in SLOTS.iter().zip(results) {
        let Some(slot) = crate::workshop::find_mut(screen, slot) else {
            continue;
        };
        // A refill replaces the cell this slot already holds, not the screen's
        // own background/shadow children (there are none in the shipped slots).
        slot.children.retain(|c| c.name() != CELL);
        let mut instance = cell.clone();
        crate::input::set_field(&mut instance, "Position", "0 0 0 0");
        crate::input::set_field(&mut instance, "Size", "1 1 1 1");
        bind(&mut instance, result);
        slot.children.push(instance);
        made += 1;
    }
    made
}

fn bind(cell: &mut Widget, result: &MatchResult) {
    crate::cells::bind_art(cell, TEAM_LOGO, &result.team_logo);
    crate::cells::bind_art(cell, BOT_PREVIEW, &result.bot_preview);
    crate::cells::bind_value(cell, POINTS, &result.points.to_string());
    crate::cells::bind_value(cell, BOT_NAME, &result.bot_name);
    crate::cells::bind_value(cell, TEAM_NAME, &result.team_name);
    // The constructor's defaults: these three start hidden.
    set_visible(cell, IMMOBILIZED, result.immobilized);
    set_visible(cell, WINNER, result.winner);
    set_visible(cell, SPECTATOR, false);
}

fn set_visible(cell: &mut Widget, child: &str, visible: bool) {
    if let Some(widget) = crate::workshop::find_mut(cell, child) {
        crate::input::set_field(widget, "Visible", if visible { "true" } else { "false" });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2fmt::gib::Block;

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

    fn load(game: &std::path::Path, screen: &str) -> Gib {
        crate::workshop::load(&game.join("UI"), screen).unwrap()
    }

    fn root(gib: &mut Gib) -> &mut Widget {
        gib.blocks
            .iter_mut()
            .find_map(|b| match b {
                Block::Widget(w) => Some(w),
                _ => None,
            })
            .unwrap()
    }

    /// The two bots of a duel: the screen fills slots 1 and 2 and leaves 3 and 4
    /// as the file has them. The row's text is read back from the placed tree.
    #[test]
    fn the_match_results_slots_take_the_matchs_own_results() {
        let Some(game) = game_dir() else { return };
        let mut gib = load(&game, "common/matchresults.gib");
        let template = crate::cells::template(&game.join("UI"), TEMPLATE).unwrap();

        // The slots' own geometry, from the screen itself.
        let screen = root(&mut gib);
        assert_eq!(
            screen.notes(),
            "when time runs out, or winner is decided, replace the game window/status bars with this match results window (drawn over top of 3D arena view)"
        );
        let slots: Vec<_> = SLOTS
            .iter()
            .map(|name| {
                let slot = crate::workshop::find(screen, name).expect(name);
                assert_eq!(
                    slot.notes(),
                    "load result_cell.gib into this slot with appropriate data",
                    "{name}"
                );
                (name, slot.size().unwrap())
            })
            .collect();
        assert_eq!(slots[0].1.first.amount, 478.0);
        assert_eq!(slots[0].1.second.amount, 80.0);

        let results = [
            MatchResult {
                team_logo: "Teams\\orange_skull.bmp".into(),
                bot_preview: PLACEHOLDER.into(),
                bot_preview_image: None,
                points: 248,
                bot_name: "The Wrangler".into(),
                team_name: "TEAM STINKBOMBS".into(),
                immobilized: true,
                winner: false,
            },
            MatchResult {
                team_logo: "AI\\red_zone.bmp".into(),
                bot_preview: PLACEHOLDER.into(),
                bot_preview_image: None,
                points: 125,
                bot_name: "Red Menace".into(),
                team_name: "Red Zone".into(),
                immobilized: false,
                winner: true,
            },
        ];
        assert_eq!(fill(screen, &template, &results), 2);

        let placed = crate::resolve(&gib, crate::DESIGN_SCREEN);
        let screen = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "match results window")
            .unwrap();
        for (index, (slot_name, _)) in slots.iter().enumerate() {
            let slot = screen
                .walk()
                .find(|p| p.widget.name() == **slot_name)
                .unwrap();
            if index < results.len() {
                assert_eq!(slot.children.len(), 1, "{slot_name}");
                let cell = &slot.children[0];
                assert_eq!(cell.widget.name(), CELL);
                assert_eq!(cell.frame, slot.frame, "{slot_name} fills its slot");
                let result = &results[index];
                let child = |name: &str| cell.walk().find(|p| p.widget.name() == name).unwrap();
                let text = |name: &str| {
                    child(name)
                        .widget
                        .children
                        .iter()
                        .find(|c| c.kind == ra2fmt::gib::WidgetKind::Text)
                        .and_then(|c| c.field("_Text"))
                        .map(|f| f.text())
                        .unwrap_or("")
                };
                assert_eq!(child(TEAM_LOGO).widget.filename(), Some(&*result.team_logo));
                assert_eq!(
                    child(BOT_PREVIEW).widget.filename(),
                    Some(&*result.bot_preview)
                );
                assert_eq!(text(POINTS), result.points.to_string());
                assert_eq!(text(BOT_NAME), result.bot_name);
                assert_eq!(text(TEAM_NAME), result.team_name);
                assert_eq!(
                    child(IMMOBILIZED).widget.visible(),
                    result.immobilized,
                    "immobilized on row {index}"
                );
                assert_eq!(
                    child(WINNER).widget.visible(),
                    result.winner,
                    "winner on row {index}"
                );
                assert!(!child(SPECTATOR).widget.visible(), "spectator stays hidden");
            } else {
                assert!(slot.children.is_empty(), "{slot_name} has no bot");
            }
        }
        // A refill replaces the cell instead of stacking another one.
        let screen = root(&mut gib);
        assert_eq!(fill(screen, &template, &results[..1]), 1);
        assert_eq!(
            crate::workshop::find(screen, SLOTS[0])
                .unwrap()
                .children
                .len(),
            1
        );
    }

    /// Slot 1 sits at the screen's own position, and the cell is placed at the
    /// slot's, not at the template's zero.
    #[test]
    fn a_cell_is_placed_at_its_slots_geometry() {
        let Some(game) = game_dir() else { return };
        let mut gib = load(&game, "common/matchresults.gib");
        let template = crate::cells::template(&game.join("UI"), TEMPLATE).unwrap();
        let result = MatchResult {
            team_logo: "Teams\\orange_skull.bmp".into(),
            bot_preview: PLACEHOLDER.into(),
            bot_preview_image: None,
            points: 0,
            bot_name: "B".into(),
            team_name: "T".into(),
            immobilized: false,
            winner: true,
        };
        let screen = root(&mut gib);
        assert_eq!(fill(screen, &template, &[result]), 1);
        let placed = crate::resolve(&gib, crate::DESIGN_SCREEN);
        let slot = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == SLOTS[0])
            .unwrap();
        // The screen's own slot offset, not an absolute screen position: the
        // window is centred on the design screen.
        assert_eq!(
            slot.widget.position().unwrap().first.amount,
            22.0,
            "the slot's own x"
        );
        assert_eq!(
            slot.widget.position().unwrap().second.amount,
            36.0,
            "the slot's own y"
        );
        assert_eq!(slot.children[0].frame, slot.frame);
    }

    /// The four slots can each carry a different `.bot`'s own snapshot. The
    /// cell binds the synthetic key and the decoded picture rides beside it, so
    /// the renderer can register the texture without a temp file.
    #[test]
    fn the_four_result_slots_can_carry_four_distinct_previews() {
        let Some(game) = game_dir() else { return };
        let mut gib = load(&game, "common/matchresults.gib");
        let template = crate::cells::template(&game.join("UI"), TEMPLATE).unwrap();
        let mut results = Vec::new();
        for (index, relative) in [
            "AI/Team1/Bot0.bot",
            "AI/Team1/Bot1.bot",
            "AI/Team6/Bot2.bot",
            "AI/Team6/Bot0.bot",
        ]
        .iter()
        .enumerate()
        {
            let bytes = std::fs::read(game.join(relative)).unwrap();
            let bot = ra2fmt::bot::parse(&bytes).unwrap();
            let image = crate::preview::Preview::decode_tga(bot.snapshot.as_deref().unwrap())
                .expect("the bot's TGA");
            assert!(!image.is_white());
            results.push(MatchResult {
                team_logo: String::new(),
                bot_preview: image.key(&format!("result-{index}")),
                bot_preview_image: Some(image),
                points: index as i32,
                bot_name: bot.name,
                team_name: format!("Team {}", index + 1),
                immobilized: false,
                winner: index == 0,
            });
        }
        let screen = root(&mut gib);
        assert_eq!(fill(screen, &template, &results), 4);
        let placed = crate::resolve(&gib, crate::DESIGN_SCREEN);
        for (index, result) in results.iter().enumerate() {
            let slot = placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == SLOTS[index])
                .unwrap();
            let preview = slot
                .walk()
                .find(|p| p.widget.name() == BOT_PREVIEW)
                .expect("the cell's preview image");
            assert_eq!(preview.widget.filename(), Some(&*result.bot_preview));
            assert_ne!(
                result.bot_preview,
                results[(index + 1) % 4].bot_preview,
                "slot {index}'s key is its own"
            );
            assert!(result
                .bot_preview_image
                .as_ref()
                .is_some_and(|p| !p.is_white()));
        }
    }
}
