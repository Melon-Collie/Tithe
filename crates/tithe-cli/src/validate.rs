//! `validate`: a controlled A/B experiment to confirm an attribute *bites*.
//!
//! The design premise is that every attribute is load-bearing (CLAUDE.md design
//! law 7: "anything load-bearing is scoutable; anything not is cut"). This runs
//! the experiment that proves it: two **identical** teams — same roster, roles,
//! formations, and all other attributes at a flat baseline — differing only in
//! the one attribute under test (team A high, team B low). Over a batch of
//! matches it reports each side's summed box score plus the win split, so the
//! effect (and its size) is visible. A flat result means the attribute doesn't
//! pull its weight and wants tuning — better found here than after we build
//! scouting and development on top of it.

use tithe_sim::{BoxScore, Event, MatchSetup, PlayerLine, PlayerSetup, Simulation};

const ATTRS: [&str; 9] = [
    "accuracy",
    "range",
    "handling",
    "stripping",
    "contesting",
    "passing",
    "positioning",
    "pace",
    "awareness",
];

pub fn run(args: &[String]) {
    let attr = match crate::flag(args, "--attr") {
        Some(a) if ATTRS.contains(&a.as_str()) => a,
        _ => {
            eprintln!("usage: tithe validate --attr <{}>", ATTRS.join("|"));
            eprintln!("       [--hi 90] [--lo 30] [--baseline 50] [--matches 50] [--seed 1]");
            std::process::exit(2);
        }
    };
    let hi = crate::flag_or(args, "--hi", 90u8);
    let lo = crate::flag_or(args, "--lo", 30u8);
    let baseline = crate::flag_or(args, "--baseline", 50u8);
    let matches = crate::flag_or(args, "--matches", 50u64);
    let seed0 = crate::flag_or(args, "--seed", 1u64);

    let setup = controlled(&attr, hi, lo, baseline);
    let teams: Vec<u8> = Simulation::from_setup(&setup, seed0)
        .expect("controlled setup is valid")
        .agents()
        .iter()
        .map(|a| a.team)
        .collect();
    let n = teams.len();

    let mut agg = BoxScore::from_events(&[], n); // zeroed table to accumulate into
    let mut wins = [0u32, 0u32];
    let mut unfinished = 0u32;
    for seed in seed0..seed0 + matches {
        let mut sim = Simulation::from_setup(&setup, seed).expect("controlled setup is valid");
        let mut events: Vec<Event> = Vec::new();
        let mut ticks = 0;
        while sim.winner().is_none() && ticks < 200_000 {
            events.extend(sim.tick());
            ticks += 1;
        }
        match sim.winner() {
            Some(w) => wins[w as usize] += 1,
            None => unfinished += 1,
        }
        agg.add(&BoxScore::from_events(&events, n));
    }

    let a = side_total(&agg, &teams, 0);
    let b = side_total(&agg, &teams, 1);

    println!("== Validation: {attr} ==");
    println!(
        "  team A {attr} = {hi}, team B = {lo}; other attrs = {baseline}; {matches} matches (seeds {seed0}..{})",
        seed0 + matches
    );
    if unfinished > 0 {
        println!("  ({unfinished} matches hit the tick cap without a winner)");
    }
    println!();
    println!("  {:<20} {:>8} {:>8}", "metric", "team A", "team B");
    row("wins", wins[0], wins[1]);
    row("goals", a.goals, b.goals);
    row("offerings", a.offerings, b.offerings);
    rowp("  convert %", a.goals, a.offerings, b.goals, b.offerings);
    row("passes", a.passes, b.passes);
    rowp(
        "  complete %",
        a.passes_completed,
        a.passes,
        b.passes_completed,
        b.passes,
    );
    row("interceptions", a.interceptions, b.interceptions);
    row("strips won", a.strips_won, b.strips_won);
    row("strips attempted", a.strips_attempted, b.strips_attempted);
    rowp(
        "  win %",
        a.strips_won,
        a.strips_attempted,
        b.strips_won,
        b.strips_attempted,
    );
    row("stripped (lost)", a.strips_suffered, b.strips_suffered);
    row("recoveries", a.recoveries, b.recoveries);
}

/// Print a raw count row: `metric  A  B`.
fn row(label: &str, a: u32, b: u32) {
    println!("  {label:<20} {a:>8} {b:>8}");
}

/// Print a percentage row (`num/den` per side).
fn rowp(label: &str, a_num: u32, a_den: u32, b_num: u32, b_den: u32) {
    println!(
        "  {label:<20} {:>8} {:>8}",
        crate::boxscore::pct(a_num, a_den),
        crate::boxscore::pct(b_num, b_den)
    );
}

/// Sum every player line on one side of the controlled match.
fn side_total(bx: &BoxScore, teams: &[u8], team: u8) -> PlayerLine {
    let mut total = PlayerLine::default();
    for (i, &t) in teams.iter().enumerate() {
        if t != team {
            continue;
        }
        let p = &bx.players[i];
        total.goals += p.goals;
        total.offerings += p.offerings;
        total.passes += p.passes;
        total.passes_completed += p.passes_completed;
        total.interceptions += p.interceptions;
        total.strips_won += p.strips_won;
        total.strips_attempted += p.strips_attempted;
        total.strips_suffered += p.strips_suffered;
        total.recoveries += p.recoveries;
    }
    total
}

/// Build the controlled matchup: both teams field the default roster, roles, and
/// shapes; every attribute is flattened to `baseline`, then the tested attribute
/// is set high on team 0 and low on team 1. The only difference between the sides
/// is the one attribute, so any outcome gap is attributable to it.
fn controlled(attr: &str, hi: u8, lo: u8, baseline: u8) -> MatchSetup {
    let mut setup = MatchSetup::default_match();
    let roster = setup.teams[0].players.clone();
    setup.teams[1].players = roster; // identical roster on both sides (sim mirrors team 1)
    for (t, team) in setup.teams.iter_mut().enumerate() {
        let v = if t == 0 { hi } else { lo };
        for p in &mut team.players {
            set_all(p, baseline);
            set_attr(p, attr, v);
        }
    }
    setup
}

fn set_attr(p: &mut PlayerSetup, attr: &str, v: u8) {
    match attr {
        "accuracy" => p.accuracy = v,
        "range" => p.range = v,
        "handling" => p.handling = v,
        "stripping" => p.stripping = v,
        "contesting" => p.contesting = v,
        "passing" => p.passing = v,
        "positioning" => p.positioning = v,
        "pace" => p.pace = v,
        "awareness" => p.awareness = v,
        _ => {}
    }
}

fn set_all(p: &mut PlayerSetup, v: u8) {
    for a in ATTRS {
        set_attr(p, a, v);
    }
}
