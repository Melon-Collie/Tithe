//! `season`: stand up a league, play a full round-robin season, and print the
//! final table. A consumer of the management layer's season API — `tithe-mgmt`
//! schedules, plays each fixture through the sim, and tallies; this binary does
//! the file I/O and the table formatting.

use crate::career::LEAGUE_NAMES;
use tithe_mgmt::Career;

pub fn run(args: &[String]) {
    let seed = crate::flag_or(args, "--seed", 1u64);
    let clubs = crate::flag_or(args, "--clubs", 4usize).max(2);
    let double = args.iter().any(|a| a == "--double");
    let out = crate::flag(args, "--out");

    // Build the league, then run a full season over it.
    let mut career = Career::new(seed);
    for i in 0..clubs {
        career.add_generated_club(LEAGUE_NAMES.get(i).copied().unwrap_or("Club"));
    }
    career.start_season(double);
    let fixtures = career.season().map_or(0, |s| s.schedule.fixture_count());
    career.play_season();

    let legs = if double { "double" } else { "single" };
    println!(
        "== Season (seed {seed}) — {} clubs, {legs} round-robin, {fixtures} fixtures ==\n",
        career.clubs.len()
    );

    // The final table.
    println!(
        "  {:>2}  {:<10} {:>3} {:>3} {:>3} {:>4} {:>4} {:>5} {:>4}",
        "#", "club", "P", "W", "L", "SF", "SA", "diff", "pts"
    );
    for (pos, s) in career.standings().iter().enumerate() {
        println!(
            "  {:>2}  {:<10} {:>3} {:>3} {:>3} {:>4} {:>4} {:>+5} {:>4}",
            pos + 1,
            career.clubs[s.club].name,
            s.played,
            s.won,
            s.lost,
            s.souls_for,
            s.souls_against,
            s.soul_diff(),
            s.points(),
        );
    }

    if let Some(out) = out {
        let json = serde_json::to_string_pretty(&career).expect("serialize career");
        std::fs::write(&out, &json).unwrap_or_else(|e| {
            eprintln!("error: cannot write '{out}': {e}");
            std::process::exit(2);
        });
        println!("\n  saved to {out}");
    }
}
