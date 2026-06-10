//! `log`: run one match and print a human-readable play-by-play with the odds.
//!
//! The narrated log is a consumer of the event stream — the "receipts culture"
//! / "why did he do that" pillar (§2). Per-tick position spam is filtered out;
//! what's left is the contests, passes, offerings, and scores, each with the
//! probability the sim rolled against.

use tithe_sim::{Event, Simulation};

pub fn run(args: &[String]) {
    let seed = crate::flag_or(args, "--seed", 1u64);
    let max_ticks = crate::flag_or(args, "--max-ticks", 100_000u64);

    let mut sim = Simulation::new(seed);
    println!("== Tithe match log (seed {seed}) ==");
    let mut ticks = 0;
    while sim.winner().is_none() && ticks < max_ticks {
        let events = sim.tick();
        let tick = sim.tick_count();
        for event in &events {
            if let Some(line) = describe(event) {
                println!("[t={tick:>5}] {line}");
            }
        }
        ticks += 1;
    }
}

/// A play-by-play line for a notable event, or `None` to filter it out.
fn describe(event: &Event) -> Option<String> {
    match event {
        Event::PassMade { from, to } => Some(format!("P{from} → P{to} (pass)")),
        Event::PassIntercepted { by } => Some(format!("    ...intercepted by P{by}!")),
        Event::StripAttempt {
            defender,
            carrier,
            chance,
            success,
        } => Some(if *success {
            format!("P{defender} STRIPS P{carrier} ({chance}%)")
        } else {
            format!("P{defender} fails to challenge P{carrier} ({chance}%)")
        }),
        Event::OfferingStarted { carrier } => Some(format!("P{carrier} winds up an offering...")),
        Event::OfferingResolved {
            carrier,
            chance,
            scored,
        } => Some(if *scored {
            format!("P{carrier} offers ({chance}%) — GOAL")
        } else {
            format!("P{carrier} offers ({chance}%) — rejected, rebound")
        }),
        Event::Scored { team, score } => Some(format!(
            "  ── team {team} scores — {}–{} ──",
            score[0], score[1]
        )),
        Event::MatchOver { winner } => Some(format!("== MATCH OVER — team {winner} wins ==")),
        // Per-tick state (Tick / AgentMoved / SoulMoved) and routine pickups are
        // filtered; the lines above tell the story.
        _ => None,
    }
}
