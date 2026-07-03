//! A tiny deterministic PRNG (SplitMix64), so seeded pair sampling is
//! reproducible without pulling in the `rand` dependency. The output need not
//! match any particular reference generator — only be self-consistent for a
//! given seed.

/// SplitMix64 generator. Fast, good enough for unbiased-enough sampling of a
/// few hundred corpus inputs.
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..n`. `n` must be non-zero. Uses modulo — the bias is
    /// negligible for the small `n` (corpus sizes) this is used with.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_a_seed() {
        let mut a = SplitMix64::new(0);
        let mut b = SplitMix64::new(0);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn below_is_in_range() {
        let mut r = SplitMix64::new(42);
        for _ in 0..1000 {
            assert!(r.below(7) < 7);
        }
    }
}
