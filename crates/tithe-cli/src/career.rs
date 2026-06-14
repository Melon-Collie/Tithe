//! `career`: the management layer's first consumer — generate a demo career,
//! play one match through the sim, and persist the save.
//!
//! This is the headless proof of the management seam (CLAUDE.md → the GM-coach
//! loop): `tithe-mgmt` builds the matchup and ingests the result; this binary
//! does the file I/O the pure crate avoids. The save format (JSON here) is a
//! consumer choice, not baked into the management layer.

use tithe_mgmt::Career;

pub fn run(args: &[String]) {
    let seed = crate::flag_or(args, "--seed", 1u64);
    let out = crate::flag(args, "--out").unwrap_or_else(|| "career.json".to_string());

    let mut career = Career::demo(seed);
    println!("== Career (seed {seed}) ==");
    for club in &career.clubs {
        println!(
            "  {} — {} players (e.g. {})",
            club.name,
            club.players.len(),
            standouts(club),
        );
    }

    // Play the one fixture: club 0 (home) vs club 1 (away).
    let result = career.play(0, 1);
    let (home, away) = (&career.clubs[0], &career.clubs[1]);
    let score = result.record.score;
    let winner = match result.record.winner {
        Some(0) => home.name.as_str(),
        Some(1) => away.name.as_str(),
        _ => "(unresolved)",
    };
    println!(
        "\n  {} {}–{} {}  → {} win",
        home.name, score[0], score[1], away.name, winner
    );

    // Ingest the box score: the agent ids line up with roster slot order — home
    // is agents 0..7, away 7..14 (the projection's team-0-first ordering).
    if let Some((agent, line)) = top_scorer(&result.box_score) {
        let (club, slot) = if agent < 7 {
            (home, agent)
        } else {
            (away, agent - 7)
        };
        let name = &club.players[slot].name;
        println!(
            "  top scorer: {} ({}) — {} on {} offerings",
            name, club.name, line.goals, line.offerings
        );
    }

    // Persist, then reload — proving the save round-trips losslessly.
    let json = serde_json::to_string_pretty(&career).expect("serialize career");
    std::fs::write(&out, &json).unwrap_or_else(|e| {
        eprintln!("error: cannot write '{out}': {e}");
        std::process::exit(2);
    });
    let reloaded: Career = {
        let text = std::fs::read_to_string(&out).unwrap_or_else(|e| {
            eprintln!("error: cannot read '{out}': {e}");
            std::process::exit(2);
        });
        serde_json::from_str(&text).unwrap_or_else(|e| {
            eprintln!("error: corrupt save '{out}': {e}");
            std::process::exit(2);
        })
    };
    let ok = serde_json::to_string(&reloaded).ok() == serde_json::to_string(&career).ok();
    println!(
        "\n  saved to {out} ({} matches in history); reload round-trips: {}",
        reloaded.history.len(),
        if ok { "yes" } else { "NO" }
    );
}

/// A club's two highest-rated players' standout attributes, for a legible line.
fn standouts(club: &tithe_mgmt::Club) -> String {
    club.players
        .iter()
        .take(2)
        .map(|p| format!("{} a{}/p{}", p.name, p.ratings.accuracy, p.ratings.passing))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The agent id and line of the match's top scorer (most goals), if anyone scored.
fn top_scorer(box_score: &tithe_sim::BoxScore) -> Option<(usize, &tithe_sim::PlayerLine)> {
    box_score
        .players
        .iter()
        .enumerate()
        .filter(|(_, l)| l.goals > 0)
        .max_by_key(|(_, l)| l.goals)
}
