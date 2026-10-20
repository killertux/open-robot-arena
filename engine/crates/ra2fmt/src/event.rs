//! The event list: `Events.txt`, the game's own definition of every event a
//! season can hold.
//!
//! A line-based file, one *record* per event, and every record is the same
//! shape — which is what makes it parseable by a reader that consumes the whole
//! file and can say so:
//!
//! ```text
//! 0                                       <- the event's id
//! F.O.R.C.E. Tournament                   <- name
//! <text font fg_mcn14>…</text>            <- description, as `DisplayBox` markup
//! 4                                       <- how many losing tiers follow
//! Arenas\event_results\prizes\…gmf        <- tier 1's prize model
//! <text font fg_mcn14>…</text>            <- tier 1's message
//! …                                       <- and so on, two lines per tier
//! Arenas\event_results\prizes\Gold_Cup_Trophy\Gold_Cup_Trophy.gmf  <- the trophy
//! <text font fg_mcn14>Wow! You won…</text>                         <- winning message
//! UI\teamhq\events\trophies\Gold Cup Trophy.tga                    <- the trophy art
//! UI\teamhq\events\ev_force.bmp                                    <- the event's art
//! Combat Arena                                                     <- the arena
//! Arenas\box\box_preview.bmp                                       <- its preview
//! 2                                                                <- bots per match
//! 1                                                                <- advance per match
//! 1                                                                <- game type
//! 1                                                                <- the next event's id
//! ```
//!
//! The reader is `FUN_00469350` (one record per 0x140-byte event object, built
//! by `FUN_0046d980` for `Events.txt` and `Events_2Player.txt`): the id into
//! `+4`, name and description, the tier count into `+8` and `count + 1`
//! model/message pairs (the last is the trophy's), four strings (trophy art,
//! event art, arena, preview), then three numbers: `+0x10c` the robots in each
//! match, `+0x110` the robots of a match that go on to the next round, and
//! `+0x118` the game type. So a record is `4 + 2 × tiers + 9` lines, and the
//! "fourth number" of earlier readings is the next record's id: the last
//! record is not one short. The description and the messages are the same markup The description and the messages are the same markup
//! the shell's `DisplayBox`es carry, which is what lets a cell show them without
//! re-wording anything.

/// One event, as `Events.txt` states it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// The record's own id line (`+4`): the shipped file numbers them 0..24.
    pub id: i32,
    /// The event's name — `F.O.R.C.E. Tournament`.
    pub name: String,
    /// Its description, as markup.
    pub description: String,
    /// The losing tiers, each a prize model and its message.
    pub tiers: Vec<(String, String)>,
    /// The trophy's model, its winning message, and its art.
    pub trophy: (String, String, String),
    /// The event's own art — `UI\teamhq\events\ev_force.bmp`.
    pub art: String,
    /// The arena it is fought in, and that arena's preview.
    pub arena: (String, String),
    /// The three numbers that close the record: robots per match (`+0x10c`),
    /// robots advancing from each match (`+0x110`), game type (`+0x118`).
    pub settings: Vec<i32>,
}

impl Event {
    /// Robots in each of the event's matches (`+0x10c`, getter `0x469990`):
    /// 2 or 4 in the shipped file.
    pub fn bots_per_match(&self) -> Option<i32> {
        self.settings.first().copied()
    }

    /// Robots of a match that go on to the next round (`+0x110`, getter
    /// `0x4699a0`; `FUN_004698f0` sizes the next round as `advance / bots`
    /// of this one). 1 in every shipped event.
    pub fn advancing_per_match(&self) -> Option<i32> {
        self.settings.get(1).copied()
    }

    /// The game type code (`+0x118`, getter `0x46bfe0`). The bracket window
    /// maps it through the jump table at `0x46bfa4` to the match's game type
    /// string: 0 `BATTLE ROYAL`, 1 `DEATHMATCH`, 2 `KING OF THE HILL`,
    /// 3 `TABLETOP`.
    pub fn game_type_code(&self) -> Option<i32> {
        self.settings.get(2).copied()
    }

    /// [`Event::game_type_code`] as the match's game type string.
    pub fn game_type_name(&self) -> Option<&'static str> {
        match self.game_type_code()? {
            0 => Some("BATTLE ROYAL"),
            1 => Some("DEATHMATCH"),
            2 => Some("KING OF THE HILL"),
            3 => Some("TABLETOP"),
            _ => None,
        }
    }
}

/// Read the file.
///
/// It is **Windows-1252, not UTF-8**, so a plain `read_to_string` refuses it. It
/// carries exactly one high byte in all 481 lines — `0x92`, the apostrophe in
/// *Hot Dog Harry’s Downtown Rumble* — and Windows-1252 is what makes it an
/// apostrophe rather than the Latin-1 `U+0092`, which no font can draw.
pub fn read(path: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(bytes
        .iter()
        .map(|&byte| match byte {
            0x92 => '\u{2019}',
            other => other as char,
        })
        .collect())
}

/// The next line, or where the file ran out.
fn take(lines: &[&str], at: &mut usize, what: &str) -> Result<String, String> {
    let line = lines
        .get(*at)
        .ok_or_else(|| format!("line {}: the file ends before {what}", *at + 1))?;
    *at += 1;
    Ok(line.to_string())
}

/// Parse the file, or say which line it stopped understanding.
///
/// The reader is strict on purpose: it must consume the file exactly, so a wrong
/// guess about the record's shape shows up as a leftover line rather than as a
/// plausible-looking half-parse.
pub fn parse(text: &str) -> Result<Vec<Event>, String> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
    let mut at = 0;
    let mut out = Vec::new();
    while at < lines.len() {
        if lines[at..].iter().all(|line| line.trim().is_empty()) {
            break;
        }
        let id_line = at + 1;
        let id: i32 = take(&lines, &mut at, "the event id")?
            .trim()
            .parse()
            .map_err(|_| {
                format!(
                    "line {id_line}: {:?} is not an event id",
                    lines.get(id_line - 1)
                )
            })?;
        let name = take(&lines, &mut at, "the name")?;
        let description = take(&lines, &mut at, "the description")?;
        let count_line = at + 1;
        let tiers: usize = take(&lines, &mut at, "the tier count")?
            .trim()
            .parse()
            .map_err(|e| format!("line {count_line}: {e}"))?;
        let mut prize = Vec::new();
        for _ in 0..tiers {
            let model = take(&lines, &mut at, "a prize model")?;
            let message = take(&lines, &mut at, "a prize message")?;
            prize.push((model, message));
        }
        let trophy = (
            take(&lines, &mut at, "the trophy")?,
            take(&lines, &mut at, "the winning message")?,
            take(&lines, &mut at, "the trophy art")?,
        );
        let art = take(&lines, &mut at, "the event art")?;
        let arena = (
            take(&lines, &mut at, "the arena")?,
            take(&lines, &mut at, "the arena preview")?,
        );
        let mut settings = Vec::new();
        for what in [
            "the robots per match",
            "the advancing robots",
            "the game type",
        ] {
            let line = at + 1;
            let number: i32 = take(&lines, &mut at, what)?
                .trim()
                .parse()
                .map_err(|_| format!("line {line}: {what} of {name:?} is not a number"))?;
            settings.push(number);
        }
        out.push(Event {
            id,
            name,
            description,
            tiers: prize,
            trophy,
            art,
            arena,
            settings,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn game_dir() -> Option<PathBuf> {
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

    fn events() -> Option<Vec<Event>> {
        let game = game_dir()?;
        Some(parse(&read(&game.join("Events.txt")).unwrap()).expect("the event list"))
    }

    /// The whole file, consumed exactly — which is the parser's own claim, since
    /// a wrong reading of the record's shape would leave lines over or run out.
    /// Twenty-five events, each with a name, a description, a trophy with its art,
    /// an event art the game has, and an arena.
    #[test]
    fn the_shipped_event_list_parses_to_the_end() {
        let Some(game) = game_dir() else { return };
        let Some(events) = events() else { return };
        assert_eq!(events.len(), 25, "the events the game ships");

        // The tiers are the record's own count, and the list uses two of them.
        let mut tiers: BTreeMap<usize, usize> = BTreeMap::new();
        for event in &events {
            *tiers.entry(event.tiers.len()).or_default() += 1;
        }
        assert_eq!(tiers, [(2, 11), (4, 14)].into_iter().collect());

        for event in &events {
            assert!(!event.name.is_empty(), "{event:?}");
            assert!(event.description.starts_with("<text"), "{}", event.name);
            for (_, message) in &event.tiers {
                assert!(message.starts_with("<text"), "{}", event.name);
            }
            assert!(event.trophy.0.ends_with(".gmf"), "{}", event.name);
            assert!(event.trophy.1.starts_with("<text"), "{}", event.name);
            assert!(event.trophy.2.ends_with(".tga"), "{}", event.name);
            let art = event.art.replace('\\', "/");
            assert!(
                crate::find_ci(&game, &art).is_some(),
                "{}: {} is not in the game",
                event.name,
                event.art
            );
            assert!(!event.arena.0.is_empty(), "{}", event.name);
        }

        // The one place the shipped file is not itself regular, named rather
        // than smoothed over: one prize model left as the prizes directory.
        let unfinished: Vec<&str> = events
            .iter()
            .flat_map(|e| {
                e.tiers
                    .iter()
                    .map(move |(model, _)| (e.name.as_str(), model))
            })
            .filter(|(_, model)| !model.ends_with(".gmf"))
            .map(|(name, _)| name)
            .collect();
        assert_eq!(unfinished, ["Robo-Depot"]);
        assert!(events.iter().all(|e| e.settings.len() == 3));
        // The ids run 0..24 in file order.
        assert!(events.iter().enumerate().all(|(i, e)| e.id == i as i32));
        // Bots and game type agree with the arenas: a deathmatch is always
        // one against one (Half Circle Arena's 2-bot event is a battle
        // royal), and one robot of each match advances.
        for e in &events {
            assert_eq!(e.advancing_per_match(), Some(1), "{}", e.name);
            let bots = e.bots_per_match().unwrap();
            let kind = e.game_type_name().unwrap();
            assert!(bots == 2 || bots == 4, "{}", e.name);
            if kind == "DEATHMATCH" {
                assert_eq!(bots, 2, "{}", e.name);
            }
        }

        // And the one high byte in the file is an apostrophe, not `U+0092`.
        assert!(events
            .iter()
            .any(|e| e.name == "Hot Dog Harry\u{2019}s Downtown Rumble"));
        assert!(!events.iter().any(|e| e.name.contains('\u{92}')));
    }

    /// A file that stops in the middle says where, rather than returning a short
    /// list.
    #[test]
    fn a_truncated_list_says_where_it_stopped() {
        let text = "0\nOnly Event\n<text font fg_mcn14>…</text>\n4\n";
        let err = parse(text).unwrap_err();
        assert!(err.contains("line 5"), "{err}");
    }
}
