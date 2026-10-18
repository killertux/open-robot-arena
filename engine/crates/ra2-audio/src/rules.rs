//! Playback rules recovered from `Robot Arena 2.exe`.
//!
//! # Impact sounds (`FUN_0045f3a0`)
//!
//! A component's hit effect (the victim's vtable slot `+0x6c`, reached from
//! `FUN_00417510` for every contact the arena prices, with the damage as its
//! first argument) does two independent things:
//!
//! * **Sparks**, only when `10.0 < damage` (and the effects option is on).
//! * **Sound**, when the mapped volume is above -1500: the volume is the
//!   `CRect(4.0, 100.0, -1500.0, 0.0)` mapping the match object builds at
//!   `0x412570` from the floats at `0x6a5794`, `0x6a5798`, `0x6a578c`,
//!   `0x6a5790` (`FUN_004194f0`: a clamped linear interpolation), in DirectSound
//!   hundredths of a decibel. A damage of 4 or less is silent, 100 or more is
//!   full volume, and in between the level rises linearly in decibels.
//!
//! The file is a random element of the victim component's `damagesounds`
//! list (`FUN_0045cad0`: random index into a vector of 16-byte strings at the
//! component object `+4..+8`), played positional at the contact point.

/// Damage at or below which a hit makes no sound (`0x6a5794`).
pub const IMPACT_DAMAGE_QUIET: f32 = 4.0;
/// Damage at which an impact reaches full volume (`0x6a5798`).
pub const IMPACT_DAMAGE_LOUD: f32 = 100.0;
/// The impact volume at [`IMPACT_DAMAGE_QUIET`], in hundredths of a decibel
/// (`0x6a578c`); a sound at or below it is not started.
pub const IMPACT_VOLUME_FLOOR_HDB: f32 = -1500.0;
/// Damage above which a hit also throws sparks (`FUN_0045f3a0`'s `10.0 <`).
pub const SPARK_DAMAGE_MIN: f32 = 10.0;

/// The volume an impact of `damage` plays at, in hundredths of a decibel, or
/// `None` when it is too quiet to start.
pub fn impact_volume_hdb(damage: f32) -> Option<i32> {
    if !damage.is_finite() {
        return None;
    }
    let k = ((damage - IMPACT_DAMAGE_QUIET) / (IMPACT_DAMAGE_LOUD - IMPACT_DAMAGE_QUIET))
        .clamp(0.0, 1.0);
    let volume = IMPACT_VOLUME_FLOOR_HDB * (1.0 - k);
    (volume > IMPACT_VOLUME_FLOOR_HDB).then_some(volume as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impacts_below_the_floor_are_silent_and_loud_ones_are_unity() {
        assert_eq!(impact_volume_hdb(4.0), None);
        assert_eq!(impact_volume_hdb(1.0), None);
        assert_eq!(impact_volume_hdb(100.0), Some(0));
        assert_eq!(impact_volume_hdb(5000.0), Some(0));
        let mid = impact_volume_hdb(52.0).unwrap();
        assert!((-760..=-740).contains(&mid), "{mid}");
        assert!(impact_volume_hdb(f32::NAN).is_none());
    }
}
