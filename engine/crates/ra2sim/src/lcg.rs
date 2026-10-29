//! The engine's linear congruential generator (docs/36 section 9.23).
//!
//! The original keeps one global state at `0x726d58`. `FUN_00419220` steps it,
//! `s = s * 0x41c64e6d + 0x3039` (the ANSI C constants, 32 bits), and
//! `FUN_00419190(n)` turns the new state into a number below `n`:
//! `floor(abs(s) / 2^31 * n)` with `abs(s) = s & 0x7fffffff` (`0x419200`, the
//! double `2^31` at `0x6a5850`), clamped to `n - 1`. The seeds are `GetTickCount()`
//! when the main menu is built (`0x4e456a`) and `time(0)` at start-up
//! (`0x61bb3a`), so a session's first state is the clock; after that the state
//! only moves when something draws: the particle and effect code (inline copies
//! of the step, about 5600 draws in 15 s of a match), the event schedule
//! (`FUN_00500120`), the bracket (`FUN_004695a0`, `FUN_0046c830`). The menus,
//! the Team HQ and the bracket screens draw nothing while they sit idle (75 s
//! were logged with `tools/oracle/capture_lcg.py`: no draw).

/// The engine's random state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lcg(pub u32);

impl Lcg {
    /// Step the state and return it (`FUN_00419220`).
    pub fn step(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(0x41c6_4e6d).wrapping_add(0x3039);
        self.0
    }

    /// A number in `0..n` (`FUN_00419190(n)`).
    pub fn range(&mut self, n: usize) -> usize {
        let abs = self.step() & 0x7fff_ffff;
        let r = f64::from(abs) / 2_147_483_648.0;
        ((r * n as f64) as usize).min(n.saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A live trace (`tools/oracle/frida/ra2_lcg.js`, state set to 4037340146 before
    /// the Blade Tournament bracket was drawn): the first draws of the bracket.
    #[test]
    fn the_step_reproduces_the_live_trace() {
        let mut lcg = Lcg(4_037_340_146);
        let live = [
            2_772_984_387u32,
            1_888_776_384,
            504_279_545,
            638_702_910,
            1_350_885_279,
            414_177_516,
        ];
        for want in live {
            assert_eq!(lcg.step(), want);
        }
    }

    #[test]
    fn a_range_draw_is_the_scaled_31_bit_state() {
        // 2772984387 & 0x7fffffff = 625500739 -> 625500739 / 2^31 * 15 = 4.36
        let mut lcg = Lcg(4_037_340_146);
        assert_eq!(lcg.range(15), 4);
        assert_eq!(lcg.range(15), 13);
        // The top bit of the state never matters to the draws.
        let (mut a, mut b) = (Lcg(1_361_071_406), Lcg(3_508_555_054));
        for _ in 0..100 {
            assert_eq!(a.range(15), b.range(15));
        }
    }
}
