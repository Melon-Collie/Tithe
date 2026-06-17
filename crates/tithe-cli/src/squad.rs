//! `squad`: watch a 12-man squad through a congested run of matches — stamina
//! drains and carries between games, the bench soaks up minutes (the sim rotates
//! tired starters off), and rest between matches restores condition. Shows why a
//! deep, durable squad matters over a schedule.
//!
//! A consumer of the management layer (`tithe-mgmt`): the fatigue carryover and
//! the in-match rotation are deterministic in the crate; this just plays and
//! prints.

use tithe_mgmt::Career;

pub fn run(args: &[String]) {
    let seed = crate::flag_or(args, "--seed", 1u64);
    let matches = crate::flag_or(args, "--matches", 6u32);

    let mut career = Career::new(seed);
    career.add_generated_club("Embers");
    career.add_generated_club("Wardens");

    println!("== Squad fatigue (seed {seed}) — Embers over {matches} matches ==");
    print_squad(&career, "fresh");
    for _ in 0..matches {
        career.play(0, 1);
    }
    print_squad(&career, &format!("after {matches} matches"));
    println!(
        "\n  stm = carried stamina, app = appearances, end = Endurance.\n  \
         Starters drain and carry it between games; the sim rotates the bench in\n  \
         as they tire, and rest restores condition — depth + Endurance are the lever."
    );
}

/// Print club 0's squad: lineup role (starter/bench by roster order), name,
/// stamina, appearances, and Endurance.
fn print_squad(career: &Career, label: &str) {
    println!("\n  -- Embers ({label}) --");
    println!(
        "  {:<3} {:<6} {:>3} {:>4} {:>4}",
        "pos", "name", "stm", "app", "end"
    );
    for (i, &id) in career.clubs[0].roster.iter().enumerate() {
        let p: &tithe_mgmt::Player = career.player(id);
        let role = if i < 7 { "ST" } else { "bn" };
        println!(
            "  {:<3} {:<6} {:>3} {:>4} {:>4}",
            role, p.name, p.stamina, p.appearances, p.ratings.endurance,
        );
    }
}
