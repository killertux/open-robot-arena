//! The event list: `Events.txt`, the game's own definition of every event a
//! season can hold.
//!
//! A line-based file, one *record* per event, and every record is the same
//! shape — which is what makes it parseable by a reader that consumes the whole
//! file and can say so:
//!
//! ```text
//! 0                                       <- the file's own header
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
//! 2                                                                <- four numbers
//! 1
//! 1
//! 1
//! ```
//!
//! So a record is `3 + 2 × tiers + 8` lines, and the four numbers at the end are
//! the event's own settings. The description and the messages are the same markup
//! the shell's `DisplayBox`es carry, which is what lets a cell show them without
//! re-wording anything.

/// One event, as `Events.txt` states it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
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
    /// The numbers that close the record. Four in every shipped event, and three
    /// in the last — `Events.txt` ends one line early, which is a property of the
    /// file rather than of the shape.
    pub settings: Vec<i32>,
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
    // The header is a single number, and it is zero in the shipped file.
    let header: i32 = take(&lines, &mut at, "the header")?
        .trim()
        .parse()
        .map_err(|_| format!("line 1: {:?} is not the header", lines.first()))?;
    if header != 0 {
        return Err(format!("header is {header}, not 0"));
    }
    let mut out = Vec::new();
    while at < lines.len() {
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
        // The settings are the run of numbers that follows, up to four: a name
        // is never a number, so the run ends where the next record begins — or
        // where the file does, which is what makes the last record three.
        let mut settings = Vec::new();
        while settings.len() < 4 {
            let next = lines.get(at).map(|line| line.trim()).unwrap_or("");
            let Ok(number) = next.parse::<i32>() else {
                break;
            };
            settings.push(number);
            at += 1;
        }
        if settings.is_empty() {
            return Err(format!("line {}: no settings for {name:?}", at + 1));
        }
        out.push(Event {
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

        // The three places the shipped file is not itself regular, named rather
        // than smoothed over: one prize model left as the prizes directory, and
        // the last event one number short because the file ends.
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
        assert!(events[..events.len() - 1]
            .iter()
            .all(|e| e.settings.len() == 4));
        assert_eq!(events.last().unwrap().settings.len(), 3);

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
