//! The scripted input file.
//!
//! The runner is headless, so its input is a file: a click at a point, a click
//! on a widget *by name* (found by hit-testing the placed tree), an idle wait,
//! and a screenshot. Blank lines and `#` comments are ignored.
//!
//! ```text
//! # main menu -> team HQ
//! click 639 544
//! shot 01-teamhq.png
//! wait 1
//! shot 02-same.png
//! ```
//!
//! The point forms are the measurement: a click is never looked up in a table of
//! transitions, it is hit-tested against the placed screen and the widget's own
//! `Notes` decides what happens. `click Name` is sugar for finding a point that
//! reaches that widget, for routes that should not depend on one pixel.

/// One line of input.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// A pixel click, in the design screen's coordinates.
    Click { x: f32, y: f32 },
    /// A click that reaches the named widget, found by hit-testing it.
    ClickNamed { name: String },
    /// Idle for `ticks`; the shell has no time-dependent drawing, so this
    /// changes nothing and is kept for pacing and for the report.
    Wait { ticks: u32 },
    /// Draw the current screen to `<out>/<name>`.
    Shot { name: String },
}

/// Parse a script.
pub fn parse(text: &str) -> Result<Vec<Command>, String> {
    let mut out = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (verb, rest) = line
            .split_once(char::is_whitespace)
            .map(|(a, b)| (a, b.trim()))
            .unwrap_or((line, ""));
        let command = match verb.to_ascii_lowercase().as_str() {
            "click" => parse_click(rest).map_err(|e| format!("line {}: {e}", number + 1))?,
            "wait" => Command::Wait {
                ticks: rest.parse().map_err(|_| {
                    format!(
                        "line {}: wait needs a whole number, got {rest:?}",
                        number + 1
                    )
                })?,
            },
            "shot" => {
                if rest.is_empty() {
                    return Err(format!("line {}: shot needs a file name", number + 1));
                }
                Command::Shot {
                    name: rest.to_string(),
                }
            }
            other => {
                return Err(format!(
                    "line {}: unknown command {other:?} (click, wait, shot)",
                    number + 1
                ))
            }
        };
        out.push(command);
    }
    Ok(out)
}

fn parse_click(rest: &str) -> Result<Command, String> {
    if rest.is_empty() {
        return Err("click needs a point or a quoted widget name".into());
    }
    if let Some(inner) = rest.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        if inner.is_empty() {
            return Err("click has an empty widget name".into());
        }
        return Ok(Command::ClickNamed {
            name: inner.to_string(),
        });
    }
    let numbers: Vec<&str> = rest.split_whitespace().collect();
    if numbers.len() == 2 {
        let x = numbers[0]
            .parse()
            .map_err(|_| format!("click x is not a number: {:?}", numbers[0]))?;
        let y = numbers[1]
            .parse()
            .map_err(|_| format!("click y is not a number: {:?}", numbers[1]))?;
        return Ok(Command::Click { x, y });
    }
    // An unquoted widget name is allowed as long as it is not one number and a
    // word: the names in this shell contain spaces, so a two-token name would
    // otherwise be read as two coordinates.
    if numbers.len() == 1 && numbers[0].parse::<f32>().is_err() {
        return Ok(Command::ClickNamed {
            name: rest.to_string(),
        });
    }
    Err(format!(
        "click needs `x y` or a widget name in quotes, got {rest:?}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_commands_and_a_comment_parse() {
        let commands = parse(
            "# boot\n\
             click 639 466\n\
             click \"Team HQ button\"\n\
             wait 2\n\
             shot 01-mainmenu.png\n",
        )
        .unwrap();
        assert_eq!(
            commands,
            vec![
                Command::Click { x: 639.0, y: 466.0 },
                Command::ClickNamed {
                    name: "Team HQ button".into()
                },
                Command::Wait { ticks: 2 },
                Command::Shot {
                    name: "01-mainmenu.png".into()
                }
            ]
        );
    }

    #[test]
    fn a_bad_line_names_itself() {
        let error = parse("click 1\n").unwrap_err();
        assert!(error.contains("line 1"), "{error}");
        let error = parse("jump 2\n").unwrap_err();
        assert!(error.contains("unknown command"), "{error}");
    }
}
