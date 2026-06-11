//! `log`: run one match and print a human-readable play-by-play with the odds.
//!
//! The narrated log is a consumer of the event stream — the "receipts culture"
//! / "why did he do that" pillar (§2). Per-tick position spam is filtered out;
//! what's left is the contests, passes, offerings, and scores, each with the
//! probability the sim rolled against.

use tithe_sim::{Event, Role};

pub fn run(args: &[String]) {
    let seed = crate::flag_or(args, "--seed", 1u64);
    let max_ticks = crate::flag_or(args, "--max-ticks", 100_000u64);
    let setup = crate::load_setup(args);

    let mut sim = crate::build_sim(&setup, seed);
    // Display names indexed by agent id, so the play-by-play reads in roster
    // names instead of raw ids.
    let names: Vec<String> = sim.agents().iter().map(|a| a.name.clone()).collect();

    println!("== Tithe match log (seed {seed}) ==");
    println!("roster (role, Fin/Strip/Cont/Pass 0–99):");
    for a in sim.agents() {
        let at = &a.attributes;
        let pct = |f: tithe_sim::Fx| (f * tithe_sim::Fx::from_num(100)).to_num::<u32>();
        println!(
            "  {:<8} team{} {:<9} F{:>2} S{:>2} C{:>2} P{:>2}",
            a.name,
            a.team,
            role_label(a.role),
            pct(at.finishing),
            pct(at.stripping),
            pct(at.contesting),
            pct(at.passing),
        );
    }
    println!();
    let mut ticks = 0;
    while sim.winner().is_none() && ticks < max_ticks {
        let events = sim.tick();
        let tick = sim.tick_count();
        for event in &events {
            if let Some(line) = describe(event, &names) {
                println!("[t={tick:>5}] {line}");
            }
        }
        ticks += 1;
    }
}

/// The display name for an agent id (falls back to the id if out of range).
fn name_of(names: &[String], id: u32) -> String {
    names
        .get(id as usize)
        .cloned()
        .unwrap_or_else(|| format!("P{id}"))
}

/// A short human label for a role.
fn role_label(role: Role) -> &'static str {
    match role {
        Role::Finisher => "finisher",
        Role::Playmaker => "playmaker",
        Role::Presser => "presser",
        Role::Anchor => "anchor",
        Role::Rover => "rover",
    }
}

/// A play-by-play line for a notable event, or `None` to filter it out.
fn describe(event: &Event, names: &[String]) -> Option<String> {
    let who = |id| name_of(names, id);
    match event {
        Event::NewSoul => Some("──────── new soul ────────".to_string()),
        Event::SoulClaimed { agent } => Some(format!("{} claims the loose soul", who(*agent))),
        Event::PassMade { from, to, chance } => {
            Some(format!("{} → {} ({chance}%)", who(*from), who(*to)))
        }
        Event::PassIntercepted { by } => Some(format!("    ...intercepted by {}!", who(*by))),
        Event::StripAttempt {
            defender,
            carrier,
            chance,
            success,
        } => Some(if *success {
            format!("{} STRIPS {} ({chance}%)", who(*defender), who(*carrier))
        } else {
            format!(
                "{} fails to challenge {} ({chance}%)",
                who(*defender),
                who(*carrier)
            )
        }),
        Event::OfferingStarted { carrier } => {
            Some(format!("{} winds up an offering...", who(*carrier)))
        }
        Event::OfferingResolved {
            carrier,
            chance,
            scored,
        } => Some(if *scored {
            format!("{} offers ({chance}%) — GOAL", who(*carrier))
        } else {
            format!("{} offers ({chance}%) — rejected, rebound", who(*carrier))
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
