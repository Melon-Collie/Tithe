//! `box`: run one match and print the per-player box score.
//!
//! A consumer of the event stream like `log`, but it accumulates the events into
//! a [`BoxScore`] (the sim-side pure derivation) and prints the stat line per
//! player instead of the play-by-play.

use crate::log::{attack_label, defend_label};
use tithe_sim::{BoxScore, Event, PlayerLine};

pub fn run(args: &[String]) {
    let seed = crate::flag_or(args, "--seed", 1u64);
    let max_ticks = crate::flag_or(args, "--max-ticks", 100_000u64);
    let setup = crate::load_setup(args);

    let mut sim = crate::build_sim(&setup, seed);
    let n = sim.agents().len();

    // Collect the whole stream, then derive the box score from it.
    let mut events: Vec<Event> = Vec::new();
    let mut ticks = 0;
    while sim.winner().is_none() && ticks < max_ticks {
        events.extend(sim.tick());
        ticks += 1;
    }
    let bx = BoxScore::from_events(&events, n);

    let score = sim.score();
    let team_name = |t: u8| {
        setup
            .as_ref()
            .and_then(|s| s.teams.get(t as usize))
            .map_or_else(|| format!("team {t}"), |ts| ts.name.clone())
    };
    println!(
        "== Box score (seed {seed}) — {} {}–{} {} ==",
        team_name(0),
        score[0],
        score[1],
        team_name(1)
    );

    // Snapshot the metadata we need before borrowing the box score for printing.
    let agents: Vec<_> = sim
        .agents()
        .iter()
        .map(|a| (a.team, a.name.clone(), a.attack_role, a.defend_role))
        .collect();

    for team in 0..2u8 {
        println!("\nteam {team}  {}", team_name(team));
        println!(
            "  {:<8} {:<17} {:>2} {:>3} {:>5}  {:>7} {:>5}  {:>3}  {:>7} {:>5}  {:>4} {:>3}",
            "name",
            "role",
            "G",
            "Off",
            "cvt%",
            "pass",
            "cmp%",
            "int",
            "strip",
            "win%",
            "lost",
            "rec"
        );
        for (i, (t, name, attack, defend)) in agents.iter().enumerate() {
            if *t != team {
                continue;
            }
            let p = &bx.players[i];
            let roles = format!("{}/{}", attack_label(*attack), defend_label(*defend));
            println!(
                "  {:<8} {:<17} {:>2} {:>3} {:>5}  {:>3}/{:<3} {:>5}  {:>3}  {:>3}/{:<3} {:>5}  {:>4} {:>3}",
                name,
                roles,
                p.goals,
                p.offerings,
                pct(p.goals, p.offerings),
                p.passes_completed,
                p.passes,
                pct(p.passes_completed, p.passes),
                p.interceptions,
                p.strips_won,
                p.strips_attempted,
                pct(p.strips_won, p.strips_attempted),
                p.strips_suffered,
                p.recoveries,
            );
        }
        let total = team_total(&bx, &agents, team);
        println!(
            "  {:<8} {:<17} {:>2} {:>3} {:>5}  {:>3}/{:<3} {:>5}  {:>3}  {:>3}/{:<3} {:>5}  {:>4} {:>3}",
            "TOTAL",
            "",
            total.goals,
            total.offerings,
            pct(total.goals, total.offerings),
            total.passes_completed,
            total.passes,
            pct(total.passes_completed, total.passes),
            total.interceptions,
            total.strips_won,
            total.strips_attempted,
            pct(total.strips_won, total.strips_attempted),
            total.strips_suffered,
            total.recoveries,
        );
    }
}

/// A percentage `num/den` as a right-padded string, or `-` when there's nothing
/// attempted (so an empty stat doesn't read as 0%).
pub(crate) fn pct(num: u32, den: u32) -> String {
    match (num * 100 + den / 2).checked_div(den) {
        Some(p) => format!("{p}%"),
        None => "-".to_string(),
    }
}

/// Sum a team's player lines (agent ids whose team matches).
pub(crate) fn team_total(
    bx: &BoxScore,
    agents: &[(
        u8,
        String,
        tithe_sim::InPossessionRole,
        tithe_sim::OutOfPossessionRole,
    )],
    team: u8,
) -> PlayerLine {
    let mut total = PlayerLine::default();
    for (i, (t, ..)) in agents.iter().enumerate() {
        if *t == team {
            total.goals += bx.players[i].goals;
            total.offerings += bx.players[i].offerings;
            total.passes += bx.players[i].passes;
            total.passes_completed += bx.players[i].passes_completed;
            total.interceptions += bx.players[i].interceptions;
            total.strips_won += bx.players[i].strips_won;
            total.strips_attempted += bx.players[i].strips_attempted;
            total.strips_suffered += bx.players[i].strips_suffered;
            total.recoveries += bx.players[i].recoveries;
        }
    }
    total
}
