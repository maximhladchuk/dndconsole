//! A tiny deterministic random number generator.
//!
//! Sound selection needs shuffling, not cryptography, and it needs to be reproducible
//! in tests. A PCG-XSH-RR generator is ~20 lines and removes a dependency whose API
//! churns between releases, so we own it.
//!
//! Reference: O'Neill, "PCG: A Family of Better Random Number Generators" (2014).

pub struct Rng {
    state: u64,
    increment: u64,
}

impl Rng {
    pub fn from_seed(seed: u64) -> Self {
        let mut rng = Self {
            state: 0,
            // Any odd constant works as the stream selector.
            increment: (seed << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    /// Seeded from the system clock. Used in the running application, never in tests.
    pub fn from_entropy() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x853c_49e6_748f_ea9b);
        Self::from_seed(nanos)
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(self.increment);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform in `0.0..1.0`.
    pub fn next_f32(&mut self) -> f32 {
        // 24 bits of mantissa is the most an f32 can represent exactly.
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform in `0..bound`. Returns 0 when `bound` is 0.
    ///
    /// Uses rejection sampling so the distribution stays uniform rather than being
    /// biased towards low values by a plain modulo.
    pub fn below(&mut self, bound: usize) -> usize {
        if bound <= 1 {
            return 0;
        }
        let bound32 = bound as u32;
        let threshold = bound32.wrapping_neg() % bound32;
        loop {
            let candidate = self.next_u32();
            if candidate >= threshold {
                return (candidate % bound32) as usize;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_produces_the_same_sequence() {
        let mut a = Rng::from_seed(42);
        let mut b = Rng::from_seed(42);
        for _ in 0..64 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::from_seed(1);
        let mut b = Rng::from_seed(2);
        let differs = (0..16).any(|_| a.next_u32() != b.next_u32());
        assert!(differs);
    }

    #[test]
    fn floats_stay_in_range() {
        let mut rng = Rng::from_seed(7);
        for _ in 0..10_000 {
            let v = rng.next_f32();
            assert!((0.0..1.0).contains(&v), "{v} out of range");
        }
    }

    #[test]
    fn below_stays_in_range_and_hits_every_value() {
        let mut rng = Rng::from_seed(99);
        let mut seen = [0usize; 5];
        for _ in 0..5_000 {
            let v = rng.below(5);
            assert!(v < 5);
            seen[v] += 1;
        }
        assert!(
            seen.iter().all(|&count| count > 700),
            "distribution skewed: {seen:?}"
        );
    }

    #[test]
    fn below_handles_degenerate_bounds() {
        let mut rng = Rng::from_seed(3);
        assert_eq!(rng.below(0), 0);
        assert_eq!(rng.below(1), 0);
    }
}
