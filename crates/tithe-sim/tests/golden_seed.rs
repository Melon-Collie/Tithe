//! Golden-seed regression: fixed seed + inputs → fixed output. A mandatory
//! determinism anchor (CLAUDE.md → Determinism rules). An unexpected change to
//! any assertion here is a determinism break until proven otherwise.

use tithe_sim::{Event, Rng, Simulation};

/// Published SplitMix64 reference vectors for seed 0 (Vigna's `splitmix64.c`).
/// These pin our generator byte-for-byte against the canonical algorithm.
const SPLITMIX64_SEED0: [u64; 5] = [
    0xE220_A839_7B1D_CDAF,
    0x6E78_9E6A_A1B9_65F4,
    0x06C4_5D18_8009_454F,
    0xF88B_B8A8_724C_81EC,
    0x1B39_896A_51A8_749B,
];

#[test]
fn rng_matches_reference_vectors() {
    let mut rng = Rng::new(0);
    for &expected in &SPLITMIX64_SEED0 {
        assert_eq!(rng.next_u64(), expected);
    }
}

#[test]
fn rng_same_seed_same_sequence() {
    let mut a = Rng::new(0x1234_5678_9ABC_DEF0);
    let mut b = Rng::new(0x1234_5678_9ABC_DEF0);
    for _ in 0..1000 {
        assert_eq!(a.next_u64(), b.next_u64());
    }
}

#[test]
fn rng_below_stays_in_bounds() {
    let mut rng = Rng::new(42);
    for _ in 0..10_000 {
        assert!(rng.below(7) < 7);
    }
    assert_eq!(rng.below(0), 0);
}

#[test]
fn simulation_is_reproducible() {
    let run = |seed| {
        let mut sim = Simulation::new(seed);
        (0..100).flat_map(|_| sim.tick()).collect::<Vec<Event>>()
    };
    assert_eq!(run(42), run(42));
    assert_eq!(run(42).len(), 100);
}
