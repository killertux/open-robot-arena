//! Parser for `Armor_Definitions.txt` (shipped inside `RA2.cfz`).
//!
//! The file is four lines per armour type:
//!
//! ```text
//! Plastic
//! Plastic armor is very lightweight, but offers little protection from attack.
//! botlab\armor_plastic.tga
//! 0 8 0 0 80
//! ```
//!
//! The last line is `<type id> <weight per square unit> <puncture> <concussion>
//! <heat>` — the same four stats the botlab's armour cell displays
//! (`FUN_0043c090` reads the rows at `0x6f1278` and labels them "weight value",
//! "puncture value", "concussion value" and "heat value"). Weight drives the
//! chassis mass; the last number (called hardness) multiplies the chassis
//! surface area to give its hitpoints (`FUN_0044d710`: `trunc(area × hardness)`). No stock plate declares a puncture or
//! concussion resistance — all four rows carry `0 0` — so there is **no**
//! per-armour damage multiplier in this file.

/// One entry of `Armor_Definitions.txt`.
#[derive(Debug, Clone, PartialEq)]
pub struct ArmourDef {
    pub name: String,
    pub description: String,
    pub texture: String,
    /// 0 plastic, 1 aluminium, 2 steel, 3 titanium.
    pub type_id: i32,
    /// Weight per square unit of chassis surface.
    pub weight: f32,
    /// Puncture resistance (0 on every stock plate).
    pub puncture: f32,
    /// Concussion resistance (0 on every stock plate).
    pub concussion: f32,
    /// Hardness (the 4th stat): chassis hitpoints = trunc(surface area × this).
    pub hardness: f32,
}

impl ArmourDef {
    /// The multiplier this plate applies to a raw hit.
    ///
    /// **1.0, and there is no datum for anything else.** `FUN_00489620` (the
    /// function docs/19 pointed at) calls the component's vtable `+0x1c`, which
    /// is `FUN_0045f2b0` — plain hitpoint subtraction with a fracture test, not
    /// a scale — and the armour rows' only damage-shaped fields (puncture and
    /// concussion) are zero on every stock plate. The hit formula's `scale`
    /// belongs to the **contact material** (`FUN_00489470` reads
    /// `*(*(contact + 0x64) + 4)`), not to the armour. Kept as a named method so
    /// the next read has one place to land.
    pub fn damage_scale(&self) -> f32 {
        1.0
    }
}

/// Parse `Armor_Definitions.txt`. Unparseable lines are skipped rather than
/// failing, so a modded file still yields whatever it does define.
pub fn parse(text: &str) -> Vec<ArmourDef> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let nums: Vec<f32> = line
            .split_whitespace()
            .filter_map(|t| t.parse().ok())
            .collect();
        if nums.len() != 5 || i < 3 {
            continue;
        }
        out.push(ArmourDef {
            name: lines[i - 3].trim().to_string(),
            description: lines[i - 2].trim().to_string(),
            texture: lines[i - 1].trim().to_string(),
            type_id: nums[0] as i32,
            weight: nums[1],
            puncture: nums[2],
            concussion: nums[3],
            hardness: nums[4],
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_shipped_file() {
        let text = "Plastic\nPlastic armor is very lightweight.\nbotlab\\armor_plastic.tga\n0 8 0 0 80\n\
                    Aluminum\nAluminum armor is a standard-weight metal.\nbotlab\\armor_aluminum.tga\n1 12 0 0 100\n\
                    Titanium\nTitanium armor is heavier than aluminum.\nbotlab\\armor_titanium.tga\n3 16 0 0 150\n\
                    Steel\nSteel armor is quite heavy.\nbotlab\\armor_steel.tga\n2 20 0 0 175\n";
        let defs = parse(text);
        assert_eq!(defs.len(), 4);
        assert_eq!(defs[0].name, "Plastic");
        assert_eq!(defs[0].type_id, 0);
        assert_eq!(defs[0].weight, 8.0);
        assert_eq!(defs[0].hardness, 80.0);
        // The botlab labels these two "puncture value" and "concussion value",
        // and every stock plate declares neither.
        assert!(defs
            .iter()
            .all(|d| d.puncture == 0.0 && d.concussion == 0.0));
        // Note the file order: titanium is listed before steel but is type 3.
        assert_eq!(defs[2].name, "Titanium");
        assert_eq!(defs[2].type_id, 3);
        assert_eq!(defs[3].name, "Steel");
        assert_eq!(defs[3].type_id, 2);
        assert_eq!(defs[3].hardness, 175.0);
    }

    #[test]
    fn tolerates_junk() {
        assert!(parse("not an armour file\nat all\n").is_empty());
        assert!(parse("").is_empty());
    }

    /// F7: there is **no per-plate damage multiplier** in the shipped data.
    ///
    /// The botlab's armour cell (`FUN_0043c090`, rows at `0x6f1278`) labels the
    /// row `<type> <weight> <puncture> <concussion> <heat>`; all four plates
    /// carry puncture 0 and concussion 0, and the hit formula (`FUN_00489470`)
    /// has no armour term at all — its `scale` is the *contact material's*
    /// float at `material + 4`, not this row. The multiplier is therefore the
    /// same for every plate, and this test measures it as 1.0 rather than
    /// letting a later pass invent four numbers.
    #[test]
    fn no_stock_plate_scales_damage() {
        let text = "Plastic\nP\nbotlab\\a.tga\n0 8 0 0 80\n\
                    Aluminum\nA\nbotlab\\b.tga\n1 12 0 0 100\n\
                    Titanium\nT\nbotlab\\c.tga\n3 16 0 0 150\n\
                    Steel\nS\nbotlab\\d.tga\n2 20 0 0 175\n";
        let defs = parse(text);
        let mut printed = Vec::new();
        for d in &defs {
            let scale = d.damage_scale();
            printed.push(format!(
                "{}: weight {:.0}, puncture {:.0}, concussion {:.0}, heat {:.0}, damage x{scale:.2}",
                d.name, d.weight, d.puncture, d.concussion, d.hardness
            ));
            assert_eq!(
                scale, 1.0,
                "{} must not scale a hit: the row carries no multiplier",
                d.name
            );
        }
        println!("{}", printed.join("\n"));
        assert_eq!(defs.len(), 4);
    }

    /// The same measurement on the **shipped** archive, when the game is
    /// present. The unit test above pins the file's shape; this one proves the
    /// four rows in `RA2.cfz` really are the zeros the parser expects.
    #[test]
    fn the_shipped_armor_table_has_no_multiplier() {
        let Some(root) = std::env::var_os("RA2_GAME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                loop {
                    let c = p.join("local/game");
                    if c.is_dir() {
                        return Some(c);
                    }
                    if !p.pop() {
                        return None;
                    }
                }
            })
        else {
            eprintln!("skipping the shipped armour table: no local/game");
            return;
        };
        let cfl = crate::cfl::Cfl::open_path(&root.join("RA2.cfz")).expect("RA2.cfz");
        let text = cfl
            .get_text("Armor_Definitions.txt")
            .expect("Armor_Definitions.txt");
        let defs = parse(&text);
        assert_eq!(defs.len(), 4);
        for d in &defs {
            assert_eq!(d.damage_scale(), 1.0, "{}", d.name);
            assert_eq!(d.puncture, 0.0, "{} puncture", d.name);
            assert_eq!(d.concussion, 0.0, "{} concussion", d.name);
            println!(
                "{}: weight {:.0}, heat {:.0}, puncture {:.0}, concussion {:.0}, damage x{:.2}",
                d.name,
                d.weight,
                d.hardness,
                d.puncture,
                d.concussion,
                d.damage_scale()
            );
        }
    }
}
