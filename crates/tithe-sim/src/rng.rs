//! Deterministic, seedable pseudo-random number generation.
//!
//! [`Rng`] is **SplitMix64** (Steele, Lea & Flood, 2014; the reference
//! generator distributed with Vigna's xoshiro/xoroshiro suite). It is chosen
//! for the determinism contract: integer-only, no platform-dependent floats,
//! tiny and obviously-correct, with published test vectors that pin it
//! byte-for-byte (see `tests/golden_seed.rs`).
//!
//! The RNG is **threaded explicitly** through the sim — never a global or
//! thread-local generator. Cloning an [`Rng`] forks the stream deterministically.

use crate::Seed;

/// A deterministic SplitMix64 generator.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Seed a new generator. Same seed → same sequence, everywhere.
    pub fn new(seed: Seed) -> Self {
        Self { state: seed }
    }

    /// The next raw 64-bit value (SplitMix64).
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A uniform value in `[0, bound)`, or `0` if `bound == 0`.
    ///
    /// Uses Lemire's multiply-shift (a 128-bit widening multiply, then take the
    /// high half). Integer-only and deterministic. This is the fast, very
    /// slightly biased form — fine for game logic; swap in the rejection-loop
    /// debiased variant here if a use site ever needs exact uniformity.
    pub fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            return 0;
        }
        let product = u128::from(self.next_u64()) * u128::from(bound);
        (product >> 64) as u64
    }
}
