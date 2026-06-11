//! `stats`: run a batch of AI-vs-AI matches and print tuning metrics.
//!
//! This is the doc's stated tuning method — numbers set by AI-vs-AI sim, not on
//! paper. The headline number to watch is **matches that hit the tick cap**: a
//! non-terminating match is the §1 elite-D-vs-elite-D grind, the pathology we
//! tune against. Reporting math uses floats — fine, this is a consumer.

use tithe_sim::Event;

const MAX_TICKS: u64 = 200_000;

pub fn run(args: &[String]) {
    let matches = crate::flag_or(args, "--matches", 100u32);
    let base_seed = crate::flag_or(args, "--seed", 1u64);
    let setup = crate::load_setup(args);

    let mut finished = 0u32;
    let mut total_ticks = 0u64;
    let mut total_souls = 0u64;
    let mut total_margin = 0u64;
    let mut strip_attempts = 0u64;
    let mut strip_wins = 0u64;
    let mut passes = 0u64;
    let mut intercepts = 0u64;
    let mut offers = 0u64;
    let mut offers_missed = 0u64;

    for m in 0..matches {
        let mut sim = crate::build_sim(&setup, base_seed.wrapping_add(u64::from(m)));
        let mut ticks = 0u64;
        while sim.winner().is_none() && ticks < MAX_TICKS {
            for event in sim.tick() {
                match event {
                    Event::StripAttempt { success, .. } => {
                        strip_attempts += 1;
                        if success {
                            strip_wins += 1;
                        }
                    }
                    Event::PassMade { .. } => passes += 1,
                    Event::PassIntercepted { .. } => intercepts += 1,
                    Event::OfferingResolved { scored, .. } => {
                        offers += 1;
                        if !scored {
                            offers_missed += 1;
                        }
                    }
                    _ => {}
                }
            }
            ticks += 1;
        }
        let score = sim.score();
        total_ticks += ticks;
        total_souls += u64::from(score[0]) + u64::from(score[1]);
        total_margin += u64::from(score[0].abs_diff(score[1]));
        if sim.winner().is_some() {
            finished += 1;
        }
    }

    let n = f64::from(matches.max(1));
    let souls = total_souls.max(1) as f64;
    let attempts = strip_attempts.max(1) as f64;

    println!(
        "Matches:            {matches}  (seeds {base_seed}..{})",
        base_seed + u64::from(matches)
    );
    println!("Finished (no cap):  {finished}/{matches}");
    println!("Avg match length:   {:.0} ticks", total_ticks as f64 / n);
    println!("Avg souls / match:  {:.1}", total_souls as f64 / n);
    println!("Avg ticks / soul:   {:.0}", total_ticks as f64 / souls);
    println!("Avg score margin:   {:.1}", total_margin as f64 / n);
    println!(
        "Strips:             {strip_attempts} attempts, {strip_wins} won ({:.0}%)",
        100.0 * strip_wins as f64 / attempts
    );
    println!(
        "Passes / match:     {:.1} ({:.0}% intercepted)",
        passes as f64 / n,
        100.0 * intercepts as f64 / passes.max(1) as f64
    );
    println!(
        "Offers / match:     {:.1} ({:.0}% scored)",
        offers as f64 / n,
        100.0 * (offers - offers_missed) as f64 / offers.max(1) as f64
    );
}
