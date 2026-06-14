//! `tournament`: archetype round-robin under a fixed attribute budget.
//!
//! The mirror A/B (`validate`) measures an attribute's *marginal* value but does
//! not conserve budget — the high team just has more total points. Real balance
//! is a *build* question: under the same point pool, are different allocations
//! viable against each other? Every player here has the same attribute sum (450,
//! = flat 50s), spent differently per archetype. A healthy game shows no build
//! running away with it — ideally rock-paper-scissors. Roles and formations are
//! held to the default roster so this isolates the *attribute* allocation.

use tithe_sim::{InPossessionRole, MatchSetup, PlayerSetup, Simulation};

/// A team identity: every player gets this 9-attribute profile (sum 450).
/// Order: accuracy, range, handling, stripping, contesting, passing,
/// positioning, pace, awareness.
struct Archetype {
    name: &'static str,
    short: &'static str,
    attrs: [u8; 9],
}

// Attribute indices into the profile.
const ACC: usize = 0;
const RNG: usize = 1;
const HAN: usize = 2;
const STR: usize = 3;
const CON: usize = 4;
const PAS: usize = 5;
const POS: usize = 6;
const PACE: usize = 7;
const AWR: usize = 8;

/// Build a profile: `primaries` set to `hi`, the rest to `lo`. Chosen so the sum
/// is 450 for both 2-primary (2·85 + 7·40) and 3-primary (3·74 + 6·38) builds.
fn profile(primaries: &[usize], hi: u8, lo: u8) -> [u8; 9] {
    let mut a = [lo; 9];
    for &i in primaries {
        a[i] = hi;
    }
    a
}

fn archetypes() -> Vec<Archetype> {
    vec![
        Archetype {
            name: "Burners (Pace/Positioning)",
            short: "Burn",
            attrs: profile(&[PACE, POS], 85, 40),
        },
        Archetype {
            name: "Finishers (Accuracy/Range)",
            short: "Fin",
            attrs: profile(&[ACC, RNG], 85, 40),
        },
        Archetype {
            name: "Enforcers (Strip/Contest/Handling)",
            short: "Enf",
            attrs: profile(&[STR, CON, HAN], 74, 38),
        },
        Archetype {
            name: "Conductors (Passing/Awareness/Handling)",
            short: "Cond",
            attrs: profile(&[PAS, AWR, HAN], 74, 38),
        },
        Archetype {
            name: "Balanced (flat 50)",
            short: "Bal",
            attrs: [50; 9],
        },
    ]
}

/// A matchup: team 0 gets `a`'s profile, team 1 gets `b`'s — both on the default
/// roster's roles and formations, so only the attribute allocation differs. When
/// `forward_finisher` is set, each team's Finisher-role player is given a real
/// finishing profile regardless of archetype — so every build has *someone* who
/// can convert a good chance, and the archetype flavors the other six.
fn build_setup(a: &Archetype, b: &Archetype, forward_finisher: bool) -> MatchSetup {
    let mut setup = MatchSetup::default_match();
    let roster = setup.teams[0].players.clone();
    setup.teams[1].players = roster;
    for p in &mut setup.teams[0].players {
        set_attrs(p, &a.attrs);
    }
    for p in &mut setup.teams[1].players {
        set_attrs(p, &b.attrs);
    }
    if forward_finisher {
        let fin = profile(&[ACC, RNG], 85, 40); // a capable forward, same budget
        for team in &mut setup.teams {
            for p in &mut team.players {
                if p.attack_role == InPossessionRole::Finisher {
                    set_attrs(p, &fin);
                }
            }
        }
    }
    setup
}

fn set_attrs(p: &mut PlayerSetup, a: &[u8; 9]) {
    p.accuracy = a[ACC];
    p.range = a[RNG];
    p.handling = a[HAN];
    p.stripping = a[STR];
    p.contesting = a[CON];
    p.passing = a[PAS];
    p.positioning = a[POS];
    p.pace = a[PACE];
    p.awareness = a[AWR];
}

/// Play `matches` games of `a` (team 0) vs `b` (team 1) from `seed0`, returning
/// `(a_wins, b_wins)`.
fn play(a: &Archetype, b: &Archetype, seed0: u64, matches: u64, fwd: bool) -> (u32, u32) {
    let setup = build_setup(a, b, fwd);
    let mut wins = (0u32, 0u32);
    for seed in seed0..seed0 + matches {
        let mut sim = Simulation::from_setup(&setup, seed).expect("valid archetype setup");
        let mut ticks = 0;
        while sim.winner().is_none() && ticks < 200_000 {
            sim.tick();
            ticks += 1;
        }
        match sim.winner() {
            Some(0) => wins.0 += 1,
            Some(_) => wins.1 += 1,
            None => {}
        }
    }
    wins
}

pub fn run(args: &[String]) {
    let matches = crate::flag_or(args, "--matches", 30u64);
    let seed0 = crate::flag_or(args, "--seed", 1u64);
    // Default: each team keeps a real forward finisher. `--uniform` makes every
    // player identical (the pure attribute-allocation test).
    let fwd = !args.iter().any(|a| a == "--uniform");
    let arch = archetypes();
    let n = arch.len();
    // wins[i][j] = i's wins over j, summed over both team-orderings.
    let mut wins = vec![vec![0u32; n]; n];
    for i in 0..n {
        for j in (i + 1)..n {
            // i as team 0, then j as team 0 — cancels any side bias.
            let (iw1, jw1) = play(&arch[i], &arch[j], seed0, matches, fwd);
            let (jw2, iw2) = play(&arch[j], &arch[i], seed0, matches, fwd);
            wins[i][j] += iw1 + iw2;
            wins[j][i] += jw1 + jw2;
        }
    }

    let per_pair = matches * 2;
    println!(
        "== Archetype tournament ({} games per matchup, seeds {seed0}..{}, budget 450, {}) ==",
        per_pair,
        seed0 + matches,
        if fwd { "shared forward finisher" } else { "uniform" }
    );
    for a in &arch {
        println!("  {:<5} {}", a.short, a.name);
    }
    // Win matrix: row = archetype, cell = its win% vs the column.
    print!("\n  {:<6}", "");
    for a in &arch {
        print!("{:>6}", a.short);
    }
    println!("{:>8}", "TOTAL");
    for i in 0..n {
        print!("  {:<6}", arch[i].short);
        let mut total_w = 0u32;
        let mut total_g = 0u32;
        for j in 0..n {
            if i == j {
                print!("{:>6}", "·");
            } else {
                let w = wins[i][j];
                print!("{:>5}%", w * 100 / per_pair as u32);
                total_w += w;
                total_g += per_pair as u32;
            }
        }
        println!("{:>7}%", total_w * 100 / total_g);
    }
    println!("\n  (cell = row's win% vs column; TOTAL = row's overall win%)");
}
