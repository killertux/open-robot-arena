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
//! The last line is `<type id> <weight per square unit> <unused> <unused>
//! <hardness>`. Weight drives the chassis mass, hardness the chassis hitpoints.

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
    /// Hardness — the chassis' hitpoints.
    pub hardness: f32,
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
}
