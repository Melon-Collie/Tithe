//! `finances`: show the league's money — each club's payroll against the cap, and
//! the development budget (unspent cap) that is the one economic lever. Then a
//! worked roster move to show how trimming payroll opens up that budget.
//!
//! A consumer of the management layer's finance API (`tithe-mgmt`). No AI GM here
//! — it just reads the accounting and performs a couple of explicit moves.

use tithe_mgmt::{Career, PlayerId};

const LEAGUE: &[&str] = &["Embers", "Wardens", "Cinders", "Wraiths"];

pub fn run(args: &[String]) {
    let seed = crate::flag_or(args, "--seed", 1u64);

    let mut career = Career::new(seed);
    for name in LEAGUE {
        career.add_generated_club(name);
    }

    println!(
        "== Finances (seed {seed}) — cap {}, floor {} ==",
        career.salary_cap, career.salary_floor
    );
    print_table(&career);

    // Lever illustration: trim a club's two priciest players and watch its
    // development budget (and the growth it funds) open up.
    let mut by_wage: Vec<(PlayerId, u32)> = career.clubs[0]
        .roster
        .iter()
        .map(|&id| (id, wage(&career, id)))
        .collect();
    by_wage.sort_by_key(|&(_, w)| std::cmp::Reverse(w));

    println!(
        "\n  {} release their two priciest players:",
        career.clubs[0].name
    );
    for (id, _) in by_wage.iter().take(2) {
        career.release(0, *id).expect("on roster");
    }
    print_table(&career);

    println!(
        "\n  budget = cap − payroll = the development budget. A leaner roster\n  \
         develops its players faster (rebuild cheap), a capped-out one stalls\n  \
         (pay to win now). That trade is the league's only economic lever."
    );
}

/// A club's finance line: payroll, development budget, and budget as a share of
/// the cap (the development boost the unspent cap buys).
fn print_table(career: &Career) {
    println!(
        "  {:<8} {:>4} {:>7} {:>7} {:>5}",
        "club", "sqd", "payroll", "budget", "dev%"
    );
    for (c, club) in career.clubs.iter().enumerate() {
        let payroll = career.payroll(c);
        let budget = career.development_budget(c);
        let dev_pct = budget * 100 / career.salary_cap.max(1);
        println!(
            "  {:<8} {:>4} {:>7} {:>7} {:>4}%",
            club.name,
            club.roster.len(),
            payroll,
            budget,
            dev_pct
        );
    }
}

/// A pooled player's wage (0 if somehow uncontracted).
fn wage(career: &Career, id: PlayerId) -> u32 {
    career.player(id).contract.as_ref().map_or(0, |c| c.wage)
}
