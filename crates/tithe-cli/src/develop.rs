//! `develop`: watch a squad develop **by deployment**. Builds a small league,
//! then repeatedly plays a season and ages everyone — so each player's ratings
//! drift toward his ceiling in the attributes his role actually exercises (the
//! finisher's shooting, the destroyer's checking), while the old fade pace-first.
//!
//! A consumer of the management layer's season + development models (`tithe-mgmt`);
//! the progression itself is pure and deterministic in the crate.

use tithe_mgmt::Career;
use tithe_sim::Attribute;

const LEAGUE: &[&str] = &["Embers", "Wardens", "Cinders", "Wraiths"];

pub fn run(args: &[String]) {
    let seed = crate::flag_or(args, "--seed", 1u64);
    let seasons = crate::flag_or(args, "--seasons", 5u32);

    let mut career = Career::new(seed);
    for name in LEAGUE {
        career.add_generated_club(name);
    }

    println!("== Development by deployment (seed {seed}) ==");
    print_club(&career, 0, "start");
    for _ in 0..seasons {
        career.start_season(false); // a single round-robin
        career.play_season(); // every fixture → box scores → season usage
        career.advance_season(); // age + develop, biased by that usage
    }
    print_club(&career, 0, &format!("after {seasons} seasons of play"));
    println!(
        "\n  Ratings climb toward each player's ceiling in the attributes his role\n  \
         exercises (Fin shoots → Acc/Rng; Des checks → Str), and past his peak they\n  \
         fade pace-first. Columns are the nine attributes."
    );
}

/// Print one club's squad: name, age, attack role, and the nine current ratings.
fn print_club(career: &Career, idx: usize, label: &str) {
    let club = &career.clubs[idx];
    println!("\n  -- {} ({label}) --", club.name);
    print!("  {:<6} {:>3} {:<4}", "name", "age", "role");
    for a in Attribute::ALL {
        print!(" {:>3}", a.short());
    }
    println!();
    for (slot, &id) in club.roster.iter().enumerate() {
        let p = career.player(id);
        let role = crate::log::attack_label(club.tactics.roles[slot].0);
        // First three letters of the role keep the table narrow.
        print!(
            "  {:<6} {:>3} {:<4}",
            p.name,
            p.age,
            &role[..role.len().min(4)]
        );
        for a in Attribute::ALL {
            print!(" {:>3}", p.ratings.get(a));
        }
        println!();
    }
}
