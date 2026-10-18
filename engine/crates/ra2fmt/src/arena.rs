//! Parser for the RA2 arena scripts (`Scripts/*.py`, `Arenas/*.py`).
//!
//! Arena *logic* is Python, but the scene it describes is data: a main `.gmf`
//! model plus extra props (`AddXtra` / `AddXtraSound`), a shadow light source,
//! flat shadow-receiver triangles and a play-field `extent`. This module does a
//! light static scan of the script text for those calls — enough to rebuild the
//! scene without a Python runtime.

/// A parsed arena script.
#[derive(Debug, Clone, Default)]
pub struct ArenaScript {
    pub name: String,
    /// The arena class's docstring: the text the picker shows under the preview
    /// (`plus.ArenaDetail().description`).
    pub description: String,
    /// `preview = "box/box_preview.bmp"` — the art the arena picker hands the C++
    /// beside the name (`plus.ArenaDetail().preview`).
    pub preview: Option<String>,
    /// `in_list` — whether the picker offers it. `Arenas.SuperArena` sets it true
    /// and only the prize scene turns it off, so a script that says nothing is
    /// offered.
    pub in_list: bool,
    /// Whether the script registers an arena (`Arenas.register(SomeArena)`), which
    /// is what makes it one rather than the package or a helper.
    pub registered: bool,
    /// `game_types = ['DEATHMATCH', …]` — the types the arena offers. The
    /// original's `Arenas.enumAvailable(type)` filters on this list.
    pub game_types: Vec<String>,
    /// Main arena model path, as written in the script.
    pub model: Option<String>,
    /// `extent = (x0, z0, x1, z1)` — the play field in the horizontal plane.
    pub extent: Option<[f32; 4]>,
    pub shadow_source: Option<[f32; 3]>,
    pub shadow_triangles: Vec<[[f32; 3]; 3]>,
    pub xtras: Vec<Xtra>,
    pub static_cameras: Vec<Camera>,
}

/// An extra scene object added by the script.
#[derive(Debug, Clone, Default)]
pub struct Xtra {
    pub name: String,
    pub gmf: String,
    pub material: String,
    /// Present for `AddXtraSound`.
    pub sound: Option<String>,
}

/// A camera the script places in the arena (`AddStaticCamera`).
#[derive(Debug, Clone, Default)]
pub struct Camera {
    pub name: String,
    pub position: [f32; 3],
}

impl ArenaScript {
    /// Xtras whose name or mesh path matches `needle` (case-insensitive
    /// substring), or all when `needle` is `all`/empty, or none.
    pub fn xtras_matching(&self, needle: &str) -> Vec<&Xtra> {
        if needle.is_empty() || needle.eq_ignore_ascii_case("all") {
            self.xtras.iter().collect()
        } else if needle.eq_ignore_ascii_case("none") {
            Vec::new()
        } else {
            let n = needle.to_ascii_lowercase();
            self.xtras
                .iter()
                .filter(|x| {
                    x.name.to_ascii_lowercase().contains(&n)
                        || x.gmf.replace('\\', "/").to_ascii_lowercase().contains(&n)
                })
                .collect()
        }
    }
}

/// Parse an arena script's text.
pub fn parse_script(text: &str) -> ArenaScript {
    // The base class offers an arena, so a script that says nothing is offered.
    let mut a = ArenaScript {
        in_list: true,
        ..ArenaScript::default()
    };

    // `name = "..."` (first assignment wins).
    if let Some(line) = text.lines().find(|l| l.trim_start().starts_with("name")) {
        if let Some(s) = str_lits(line).into_iter().next() {
            a.name = s;
        }
    }

    // The class docstring, the first string line after `class ...:`.
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.trim_start().starts_with("class ") && line.trim_end().ends_with(':') {
            if let Some(next) = lines.next() {
                let next = next.trim();
                if next.starts_with('"') {
                    if let Some(s) = str_lits(next).into_iter().next() {
                        a.description = s;
                        break;
                    }
                }
            }
        }
    }

    // `preview = "box/box_preview.bmp"`.
    if let Some(line) = text.lines().find(|l| l.trim_start().starts_with("preview")) {
        if let Some(s) = str_lits(line).into_iter().next() {
            a.preview = Some(s);
        }
    }

    // `in_list = False` — what the prize scene says and the arenas do not.
    if let Some(line) = text.lines().find(|l| l.trim_start().starts_with("in_list")) {
        a.in_list = !line.contains("False");
    }

    // `Arenas.register(BoxArena)` — the game's own way of declaring an arena.
    a.registered = text.contains("Arenas.register");

    // `game_types = ['DEATHMATCH', 'BATTLE ROYAL', …]` — single or double
    // quotes; the list may wrap onto more than one line.
    if let Some(pos) = text.find("game_types") {
        let rest = &text[pos..];
        let end = rest.find(']').map(|i| i + 1).unwrap_or(rest.len());
        a.game_types = quoted_lits(&rest[..end]);
    }

    // Main model: the first quoted string inside an `__init__(self, "...")` call.
    for args in call_args(text, "__init__") {
        if let Some(s) = str_lits(args).into_iter().next() {
            a.model = Some(s);
            break;
        }
    }

    // `extent = ( ... )`.
    if let Some(pos) = text.find("extent") {
        let rest = &text[pos..];
        if let Some(eq) = rest.find('=') {
            let tail = &rest[eq..];
            let end = tail.find('\n').unwrap_or(tail.len());
            let f = floats(&tail[..end]);
            if f.len() >= 4 {
                a.extent = Some([f[0], f[1], f[2], f[3]]);
            }
        }
    }

    // Shadow source.
    if let Some(args) = call_args(text, "SetShadowSource").first() {
        let f = floats(args);
        if f.len() >= 3 {
            a.shadow_source = Some([f[0], f[1], f[2]]);
        }
    }

    // Shadow receiver triangles.
    for args in call_args(text, "AddShadowTriangle") {
        let f = floats(args);
        if f.len() >= 9 {
            a.shadow_triangles
                .push([[f[0], f[1], f[2]], [f[3], f[4], f[5]], [f[6], f[7], f[8]]]);
        }
    }

    // Static cameras.
    for args in call_args(text, "AddStaticCamera") {
        let s = str_lits(args);
        let f = floats(args);
        if !s.is_empty() && f.len() >= 3 {
            a.static_cameras.push(Camera {
                name: s[0].clone(),
                position: [f[0], f[1], f[2]],
            });
        }
    }

    // Extra props. `AddXtra` and `AddXtraSound` share the "AddXtra" prefix.
    for args in call_args(text, "AddXtra") {
        let s = str_lits(args);
        if s.len() >= 3 {
            a.xtras.push(Xtra {
                name: s[0].clone(),
                gmf: s[1].clone(),
                material: s[2].clone(),
                sound: s.get(3).cloned(),
            });
        }
    }

    a
}

/// Read and parse an arena script from disk.
pub fn parse_path(path: &std::path::Path) -> Result<ArenaScript, crate::Error> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| crate::Error::Structure(format!("{}: {e}", path.display())))?;
    Ok(parse_script(&text))
}

// ---------------------------------------------------------------------------

/// All argument lists for calls named `name` (matches `name(...)`, balancing
/// parentheses). Returns the text between the outermost parentheses.
fn call_args<'a>(text: &'a str, name: &str) -> Vec<&'a str> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut idx = 0usize;
    while let Some(pos) = text[idx..].find(name) {
        let mut i = idx + pos + name.len();
        // Skip to the '('.
        while i < b.len() && b[i] != b'(' {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        let mut depth = 0i32;
        let mut j = i;
        while j < b.len() {
            match b[j] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        out.push(&text[i + 1..j.min(b.len())]);
        idx = (j + 1).min(b.len());
        if idx >= b.len() {
            break;
        }
    }
    out
}

/// Quoted string literals with either quote character. The arena scripts spell
/// their `game_types` list with single quotes, which [`str_lits`] does not read.
fn quoted_lits(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let quote = chars[i];
        if quote != '\'' && quote != '"' {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut j = start;
        while j < chars.len() && chars[j] != quote {
            if chars[j] == '\\' {
                j += 1;
            }
            j += 1;
        }
        let text: String = chars[start..j.min(chars.len())].iter().collect();
        out.push(unescape(&text));
        i = j + 1;
    }
    out
}

/// Extract all double-quoted string literals (with Python-style `\\`/`\"`
/// unescaping applied).
fn str_lits(s: &str) -> Vec<String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'"' {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && b[j] != b'"' {
                if b[j] == b'\\' {
                    j += 1; // skip the escaped character
                }
                j += 1;
            }
            out.push(unescape(&s[start..j.min(b.len())]));
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Undo the common Python string escapes found in arena scripts.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Extract numeric literals (integers/floats, including exponents).
fn floats(s: &str) -> Vec<f32> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_digit() || c == b'-' || c == b'+' || c == b'.' {
            let start = i;
            let mut j = i;
            if b[j] == b'-' || b[j] == b'+' {
                j += 1;
            }
            let mantissa = j;
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.') {
                j += 1;
            }
            if j > mantissa && j < b.len() && (b[j] == b'e' || b[j] == b'E') {
                j += 1;
                if j < b.len() && (b[j] == b'-' || b[j] == b'+') {
                    j += 1;
                }
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
            }
            if j > mantissa {
                if let Ok(f) = s[start..j].parse::<f32>() {
                    out.push(f);
                }
            }
            i = j.max(start + 1);
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_practice_arena() {
        let src = r#"
class PracticeArena(Arenas.SuperArena):
    name = "Practice Arena"
    game_types = ['DEATHMATCH', 'BATTLE ROYAL']
    def __init__(self):
        Arenas.SuperArena.__init__(self, "Arenas/practice arena/practice_arena.gmf")
    def AddShadowReceivers(self):
        self.SetShadowSource(0.653, 6.894, -0.418)
        self.AddShadowTriangle((-6.7, 0.01, 17.1), (-6.7, 0.01, -5.5), (6.2, 0.01, 17.1))
    def setObstacle(self, name):
        self.RemoveXtra("Obstacle")
        self.AddXtraSound("Obstacle", "arenas\\crates\\crates.gmf", "metal", "Sounds\\crate.wav")
        self.AddXtra("Obstacle", "arenas\\ramps\\ramps.gmf", "metal")
"#;
        let a = parse_script(src);
        assert_eq!(a.name, "Practice Arena");
        assert_eq!(a.game_types, vec!["DEATHMATCH", "BATTLE ROYAL"]);
        assert_eq!(
            a.model.as_deref(),
            Some("Arenas/practice arena/practice_arena.gmf")
        );
        assert_eq!(a.shadow_source, Some([0.653, 6.894, -0.418]));
        assert_eq!(a.shadow_triangles.len(), 1);
        assert_eq!(a.xtras.len(), 2);
        assert_eq!(a.xtras[0].gmf, "arenas\\crates\\crates.gmf");
        assert_eq!(a.xtras[0].sound.as_deref(), Some("Sounds\\crate.wav"));
        assert!(a.xtras[1].sound.is_none());
    }
}
