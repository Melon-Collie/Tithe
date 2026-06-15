//! `develop`: watch a generated squad age. Prints the squad's current vs. ceiling
//! ratings, advances the career several development years, and prints it again —
//! the young grow toward their potential, the old decline shape-first.
//!
//! A consumer of the management layer's development model (`tithe-mgmt`); the
//! progression itself is pure and deterministic in the crate.

use tithe_mgmt::Career;

pub fn run(args: &[String]) {
    let seed = crate::flag_or(args, "--seed", 1u64);
    let seasons = crate::flag_or(args, "--seasons", 8u32);

    let mut career = Career::new(seed);
    career.add_generated_club("Embers");

    println!("== Squad development (seed {seed}) ==");
    print_squad(&career, "start");
    for _ in 0..seasons {
        career.advance_season();
    }
    print_squad(&career, &format!("after {seasons} seasons"));
    println!(
        "\n  cur = current overall, pot = ceiling (potential), Δ = cur − pot.\n  \
         Young players climb toward pot; past their peak they fall away from it (pace first)."
    );
}

/// Print one club's players with age, current overall, ceiling, and the gap.
fn print_squad(career: &Career, label: &str) {
    println!("\n  -- {label} --");
    println!(
        "  {:<6} {:>3}  {:>3} {:>3} {:>4}   {:>3} {:>3}   risk",
        "name", "age", "cur", "pot", "Δ", "pace", "awr"
    );
    let club = &career.clubs[0];
    for &id in &club.roster {
        let p = career.player(id);
        let cur = p.ratings.overall() as i32;
        let pot = p.potential.overall() as i32;
        println!(
            "  {:<6} {:>3}  {:>3} {:>3} {:>+4}   {:>3} {:>3}   {:>3}",
            p.name,
            p.age,
            cur,
            pot,
            cur - pot,
            p.ratings.pace,
            p.ratings.awareness,
            p.development_risk,
        );
    }
}
