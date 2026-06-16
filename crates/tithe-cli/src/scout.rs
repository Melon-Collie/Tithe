//! `scout`: print scouting reports for a club — honest bands over each player's
//! true ratings, tightened by how much he's been seen. Play a few seasons first
//! (`--seasons`) to watch the fog lift: current bands narrow with exposure, while
//! a volatile player's ceiling band stays a wide bet (the irreducible dial).
//!
//! A consumer of the management layer's scouting view (`tithe-mgmt`); the fog is a
//! pure function over the players' true ratings, which it never touches.

use tithe_mgmt::{Career, ScoutReport};
use tithe_sim::Attribute;

const LEAGUE: &[&str] = &["Embers", "Wardens", "Cinders", "Wraiths"];

pub fn run(args: &[String]) {
    let seed = crate::flag_or(args, "--seed", 1u64);
    let seasons = crate::flag_or(args, "--seasons", 0u32);

    let mut career = Career::new(seed);
    for name in LEAGUE {
        career.add_generated_club(name);
    }
    // Optional exposure: play seasons so appearances (and confidence) accrue.
    for _ in 0..seasons {
        career.start_season(false);
        career.play_season();
        career.advance_season();
    }

    println!(
        "== Scouting — {} (seed {seed}, after {seasons} seasons) ==",
        career.clubs[0].name
    );
    println!("  player    age  seen  vol   now (overall)  ceiling");
    let club = &career.clubs[0];
    let mut youngest = club.roster[0];
    for &id in &club.roster {
        let p = career.player(id);
        let r = career.scout(id);
        let (cl, ch) = overall(&r, true);
        let (kl, kh) = overall(&r, false);
        println!(
            "  {:<8} {:>3}  {:>3}%  {:>3}   {:>2}–{:<2}          {:>2}–{:<2}",
            p.name, p.age, r.observation, r.volatility, cl, ch, kl, kh
        );
        if p.age < career.player(youngest).age {
            youngest = id;
        }
    }

    // The full report for the least-proven prospect — the fuzziest, where you're
    // betting most on ceiling.
    let p = career.player(youngest);
    let r = career.scout(youngest);
    println!(
        "\n  -- {} · age {} · seen {}% · volatility {} --",
        p.name, p.age, r.observation, r.volatility
    );
    println!("    attr    now     ceiling");
    for a in Attribute::ALL {
        let c = r.current(a);
        let k = r.ceiling(a);
        println!(
            "    {:<5} {:>2}–{:<2}   {:>2}–{:<2}",
            a.short(),
            c.lo,
            c.hi,
            k.lo,
            k.hi
        );
    }
    println!(
        "\n  The true value is always inside the band. Exposure narrows the\n  \
              'now' bands; high volatility keeps the ceiling a wide bet."
    );
}

/// The overall (mean-of-attributes) band, current or ceiling, as `(lo, hi)`.
fn overall(report: &ScoutReport, current: bool) -> (u8, u8) {
    let mut lo = 0u32;
    let mut hi = 0u32;
    for a in Attribute::ALL {
        let b = if current {
            report.current(a)
        } else {
            report.ceiling(a)
        };
        lo += b.lo as u32;
        hi += b.hi as u32;
    }
    ((lo / 9) as u8, (hi / 9) as u8)
}
