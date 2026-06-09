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
        let mut stream = Vec::new();
        for _ in 0..200 {
            stream.extend(sim.tick());
        }
        stream
    };
    // Same seed → identical event stream; different seed → different scatter.
    assert_eq!(run(123), run(123));
    assert_ne!(run(123), run(456));
}

#[test]
fn two_teams_of_seven() {
    let sim = Simulation::new(0);
    assert_eq!(sim.agents().len(), 14);
    assert_eq!(sim.agents().iter().filter(|a| a.team == 0).count(), 7);
    assert_eq!(sim.agents().iter().filter(|a| a.team == 1).count(), 7);
}

#[test]
fn loose_soul_gets_claimed() {
    let mut sim = Simulation::new(7);
    let mut claimed = false;
    for _ in 0..500 {
        if sim
            .tick()
            .iter()
            .any(|e| matches!(e, Event::PossessionGained { .. }))
        {
            claimed = true;
            break;
        }
    }
    assert!(claimed, "the loose soul should be claimed early");
}

#[test]
fn strips_cause_turnovers() {
    let mut sim = Simulation::new(1);
    let mut saw_successful_strip = false;
    for _ in 0..5000 {
        for event in sim.tick() {
            if let Event::StripAttempt { success: true, .. } = event {
                saw_successful_strip = true;
            }
        }
    }
    assert!(
        saw_successful_strip,
        "expected at least one successful strip over 5000 ticks"
    );
}

#[test]
fn a_team_scores() {
    let mut sim = Simulation::new(2);
    let mut scored = false;
    for _ in 0..20_000 {
        if sim.tick().iter().any(|e| matches!(e, Event::Scored { .. })) {
            scored = true;
            break;
        }
    }
    assert!(scored, "expected a touch-in score within 20000 ticks");
}

#[test]
fn match_ends_with_a_winner() {
    let mut sim = Simulation::new(3);
    let mut ticks = 0;
    while sim.winner().is_none() && ticks < 200_000 {
        sim.tick();
        ticks += 1;
    }
    let winner = sim.winner().expect("the match should reach a winner");
    assert!(sim.score()[winner as usize] >= 11);
    // After the match ends, ticking is inert.
    assert!(sim.tick().is_empty());
}
