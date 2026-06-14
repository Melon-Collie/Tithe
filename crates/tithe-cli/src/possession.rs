//! `possession`: measure how strongly **possession predicts winning**.
//!
//! All ball sports reward keeping the ball; ours may not (a midfield steal is
//! low-leverage, scoring lives in the goal-mouth offering cycle). Before changing
//! any mechanic, measure the baseline: across a batch, how much of the match is
//! actually *held*, and does the team that holds it more tend to win? Then a
//! mechanic change is judged by whether this correlation *strengthens*.
//!
//! Possession = ticks the soul is `Held` by a team (loose, in-flight, and the
//! brief offering wind-up that isn't held count as nobody's). Floats live here at
//! the analysis boundary, never in `tithe-sim`.

use tithe_sim::{Event, Possession};

pub fn run(args: &[String]) {
    let matches = crate::flag_or(args, "--matches", 100u64);
    let seed0 = crate::flag_or(args, "--seed", 1u64);
    let setup = crate::load_setup(args);

    let mut decided = 0u64; // matches that reached a winner
    let mut held_ticks = 0u64; // total ticks the ball was held by someone
    let mut total_ticks = 0u64;
    let mut winner_share_sum = 0.0f64; // Σ winner's share of held time
    let mut more_poss_wins = 0u64; // won by the team that held it more
    let mut dom55 = (0u64, 0u64); // (times a team held ≥55%, of those it won)
    let mut dom60 = (0u64, 0u64);
    // Did the offerer get the ball via a pass (off-ball play made the chance) or
    // win it himself and carry it in solo? Tells us if off-ball offense matters.
    let mut assisted = 0u64;
    let mut solo = 0u64;

    for seed in seed0..seed0 + matches {
        let mut sim = crate::build_sim(&setup, seed);
        let mut poss = [0u64, 0u64];
        let mut ticks = 0u64;
        let mut pending_pass_to: Option<u32> = None; // intended receiver of a live pass
        let mut held_via_pass = false; // current holder gained possession by catching a pass
        while sim.winner().is_none() && ticks < 200_000 {
            for ev in sim.tick() {
                match ev {
                    Event::PassMade { to, .. } => pending_pass_to = Some(to),
                    Event::PossessionGained { agent } => {
                        held_via_pass = pending_pass_to == Some(agent); // catch vs strip/interception
                        pending_pass_to = None;
                    }
                    Event::SoulClaimed { .. } => held_via_pass = false, // loose recovery = solo
                    Event::OfferingStarted { .. } => {
                        if held_via_pass {
                            assisted += 1;
                        } else {
                            solo += 1;
                        }
                    }
                    _ => {}
                }
            }
            ticks += 1;
            if let Possession::Held(id) = sim.soul().possession {
                poss[sim.agents()[id as usize].team as usize] += 1;
            }
        }
        total_ticks += ticks;
        held_ticks += poss[0] + poss[1];

        let Some(w) = sim.winner() else { continue };
        let held = poss[0] + poss[1];
        if held == 0 {
            continue;
        }
        decided += 1;
        let w = w as usize;
        let winner_share = poss[w] as f64 / held as f64;
        winner_share_sum += winner_share;
        if poss[w] > poss[1 - w] {
            more_poss_wins += 1;
        }
        // Did dominant possession win? Look at whichever team held the most.
        let top_share = (poss[0].max(poss[1])) as f64 / held as f64;
        let top_won = poss[w] >= poss[1 - w];
        if top_share >= 0.55 {
            dom55.0 += 1;
            if top_won {
                dom55.1 += 1;
            }
        }
        if top_share >= 0.60 {
            dom60.0 += 1;
            if top_won {
                dom60.1 += 1;
            }
        }
    }

    let pct = |n: u64, d: u64| {
        if d == 0 {
            0.0
        } else {
            100.0 * n as f64 / d as f64
        }
    };
    println!(
        "== Possession vs winning ({matches} matches, seeds {seed0}..{}{}) ==",
        seed0 + matches,
        if setup.is_some() {
            ", authored"
        } else {
            ", RNG teams"
        }
    );
    println!("  decided:                  {decided}");
    println!(
        "  ball held:                {:.0}% of ticks  (rest loose / in-flight / offering)",
        pct(held_ticks, total_ticks)
    );
    println!(
        "  winner's avg share of held time: {:.1}%   (50% = possession is irrelevant)",
        100.0 * winner_share_sum / decided.max(1) as f64
    );
    println!(
        "  higher-possession team won:      {}/{}  ({:.0}%)",
        more_poss_wins,
        decided,
        pct(more_poss_wins, decided)
    );
    println!(
        "  when a team held >=55%, it won:  {}/{}  ({:.0}%)",
        dom55.1,
        dom55.0,
        pct(dom55.1, dom55.0)
    );
    println!(
        "  when a team held >=60%, it won:  {}/{}  ({:.0}%)",
        dom60.1,
        dom60.0,
        pct(dom60.1, dom60.0)
    );
    println!(
        "  offerings assisted (off a pass): {}/{}  ({:.0}%)   (rest are solo carry-ins)",
        assisted,
        assisted + solo,
        pct(assisted, assisted + solo)
    );
}
