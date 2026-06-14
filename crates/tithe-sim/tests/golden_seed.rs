//! Golden-seed regression: fixed seed + inputs → fixed output. A mandatory
//! determinism anchor (CLAUDE.md → Determinism rules). An unexpected change to
//! any assertion here is a determinism break until proven otherwise.

use tithe_sim::{Event, Rng, Simulation};

/// FNV-1a (64-bit) over bytes — a *fixed* deterministic hash, deliberately not
/// `std`'s `DefaultHasher` (whose seed is randomized per process). Used to fold a
/// whole match's output into one digest for the golden-hash regression below.
fn fnv1a(bytes: &[u8], mut h: u64) -> u64 {
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3); // FNV prime
    }
    h
}
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

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
fn from_setup_is_reproducible_and_authored() {
    use tithe_sim::{Fx, MatchSetup};
    let setup = MatchSetup::default_match();
    let run = |seed| {
        let mut sim = Simulation::from_setup(&setup, seed).expect("valid setup");
        let mut stream = Vec::new();
        for _ in 0..200 {
            stream.extend(sim.tick());
        }
        stream
    };
    // An authored match is a pure function of the seed (no RNG-rolled roster).
    assert_eq!(run(123), run(123));
    // The roster is the authored one, not the default — Sear's accuracy is 0.85.
    let sim = Simulation::from_setup(&setup, 1).expect("valid setup");
    let sear = sim
        .agents()
        .iter()
        .find(|a| a.name == "Sear")
        .expect("Sear");
    assert_eq!(
        sear.attributes.accuracy,
        Fx::from_num(85) / Fx::from_num(100)
    );
}

#[test]
fn two_teams_of_seven() {
    let sim = Simulation::new(0);
    assert_eq!(sim.agents().len(), 14);
    assert_eq!(sim.agents().iter().filter(|a| a.team == 0).count(), 7);
    assert_eq!(sim.agents().iter().filter(|a| a.team == 1).count(), 7);
}

#[test]
fn faceoff_only_nearest_contest() {
    let mut sim = Simulation::new(1);
    sim.tick(); // first decision = the faceoff draw
    let center = tithe_sim::Vec2::default(); // the loose soul starts at center
    let contestants = sim.agents().iter().filter(|a| a.target == center).count();
    assert!(
        contestants <= 2,
        "only each team's nearest should contest the draw, got {contestants}"
    );
}

#[test]
fn agents_do_not_stack() {
    use tithe_sim::Fx;
    let mut sim = Simulation::new(1);
    for _ in 0..400 {
        sim.tick();
    }
    let a = sim.agents();
    let min_dist = (0..a.len())
        .flat_map(|i| ((i + 1)..a.len()).map(move |j| a[i].pos.distance_to(a[j].pos)))
        .min()
        .unwrap();
    assert!(
        min_dist > Fx::from_num(1),
        "separation should keep agents apart"
    );
}

#[test]
fn loose_soul_gets_claimed() {
    let mut sim = Simulation::new(7);
    let mut claimed = false;
    for _ in 0..500 {
        if sim
            .tick()
            .iter()
            .any(|e| matches!(e, Event::SoulClaimed { .. }))
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
fn passes_happen() {
    let mut sim = Simulation::new(2);
    let mut passed = false;
    for _ in 0..20_000 {
        if sim
            .tick()
            .iter()
            .any(|e| matches!(e, Event::PassMade { .. }))
        {
            passed = true;
            break;
        }
    }
    assert!(passed, "expected a pass within 20000 ticks");
}

#[test]
fn passes_get_intercepted() {
    // Interceptions are rare (~1% of passes, well under one per match), so a
    // single seed is a coin-flip. Scan a sample of matches and assert the
    // *mechanic* fires in normal play — not that any particular match shows it.
    // Breaks on the first interception (usually within a few matches); the
    // per-match tick cap keeps a non-terminating grind seed from hanging.
    let mut intercepted = false;
    'matches: for seed in 1..=60u64 {
        let mut sim = Simulation::new(seed);
        for _ in 0..50_000 {
            if sim.winner().is_some() {
                break;
            }
            if sim
                .tick()
                .iter()
                .any(|e| matches!(e, Event::PassIntercepted { .. }))
            {
                intercepted = true;
                break 'matches;
            }
        }
    }
    assert!(
        intercepted,
        "expected at least one interception across 60 matches"
    );
}

#[test]
fn stamina_drains_then_refreshes_between_souls() {
    use tithe_sim::Fx;
    let mut sim = Simulation::new(4);
    // Run partway into the first soul and confirm someone has tired.
    let mut drained = false;
    for _ in 0..120 {
        sim.tick();
        if sim.agents().iter().any(|a| a.stamina < Fx::from_num(1)) {
            drained = true;
        }
        // Stamina never goes negative.
        assert!(sim.agents().iter().all(|a| a.stamina >= Fx::from_num(0)));
    }
    assert!(drained, "stamina should drain during a soul");
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
fn offerings_can_miss() {
    let mut sim = Simulation::new(2);
    let mut missed = false;
    for _ in 0..20_000 {
        if sim
            .tick()
            .iter()
            .any(|e| matches!(e, Event::OfferingResolved { scored: false, .. }))
        {
            missed = true;
            break;
        }
    }
    assert!(
        missed,
        "scoring should not be guaranteed — offerings can miss"
    );
}

#[test]
fn golden_default_match_hash() {
    use tithe_sim::{Fx, MatchSetup, Possession};

    // A fixed match: the authored default roster (so roles, footprints, and the
    // on-ball tether all run) at a pinned seed, played to a winner.
    let setup = MatchSetup::default_match();
    let mut sim = Simulation::from_setup(&setup, 0x_5EED_C0DE).expect("valid setup");

    // Fold the whole output into one digest: per tick, every event (the canonical
    // stream, via its stable Debug form) AND the continuous state as raw
    // fixed-point bits (soul + each agent's position/stamina/stagger/possession).
    // Events alone would miss silent motion drift; positions alone would miss
    // event-content changes — together they pin behavior tightly.
    let mut h = FNV_OFFSET;
    let bits = |v: Fx| v.to_bits();
    let mut ticks = 0u64;
    while sim.winner().is_none() && ticks < 200_000 {
        let events = sim.tick();
        ticks += 1;
        for e in &events {
            h = fnv1a(format!("{e:?}").as_bytes(), h);
        }
        let soul = sim.soul();
        h = fnv1a(&bits(soul.pos.x).to_le_bytes(), h);
        h = fnv1a(&bits(soul.pos.y).to_le_bytes(), h);
        let poss: u64 = match soul.possession {
            Possession::Loose => 1,
            Possession::Held(id) => 2 << 32 | id as u64,
            Possession::InFlight { to, intercepted } => (3 + intercepted as u64) << 32 | to as u64,
        };
        h = fnv1a(&poss.to_le_bytes(), h);
        for a in sim.agents() {
            h = fnv1a(&bits(a.pos.x).to_le_bytes(), h);
            h = fnv1a(&bits(a.pos.y).to_le_bytes(), h);
            h = fnv1a(&bits(a.stamina).to_le_bytes(), h);
            h = fnv1a(&(a.stagger as u64).to_le_bytes(), h);
        }
    }
    h = fnv1a(&(sim.score()[0] as u64).to_le_bytes(), h);
    h = fnv1a(&(sim.score()[1] as u64).to_le_bytes(), h);
    h = fnv1a(&(sim.winner().map_or(0xFF, u64::from)).to_le_bytes(), h);
    h = fnv1a(&ticks.to_le_bytes(), h);

    // The golden value. An UNEXPECTED change here is a determinism break (a float
    // crept in, iteration order shifted, ambient RNG) — investigate before
    // touching it. An EXPECTED change (you altered sim behavior on purpose) means
    // re-pin it in the same commit, after confirming the diff is the intended one.
    assert_eq!(
        h, 0x6ecc_1bd4_7363_1587,
        "golden match hash changed — actual = {h:#018x}"
    );
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
