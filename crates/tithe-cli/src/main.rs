//! `tithe` — command-line consumers of the sim's event stream.
//!
//! The sim is headless and never knows it's watched; this binary is one of its
//! consumers (CLAUDE.md → Committed architecture). Subcommands:
//!
//! - `play` — run one match and write a self-contained HTML replay you open in
//!   a browser. The "is it fun to *watch*" test.
//! - `stats` — run a batch of AI-vs-AI matches and print tuning metrics. The
//!   "do the numbers work" test (the doc's stated tuning method).
//! - `log` — print a narrated play-by-play of one match.
//! - `box` — print the per-player box score for one match.
//! - `validate` — a controlled A/B that confirms one attribute *bites*.
//! - `init` — write an editable example match-setup file (the coach-input
//!   boundary), the stand-in for the game's roster/tactics screens.
//!
//! `play`/`stats`/`log`/`box` accept `--setup FILE` to run an authored matchup
//! instead of the default RNG-rolled teams.
//!
//! Floats and serialization live here at the render boundary — never in
//! `tithe-sim`.

mod boxscore;
mod init;
mod log;
mod possession;
mod replay;
mod stats;
mod validate;

use tithe_sim::{MatchSetup, Simulation};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("play") => replay::run(&args[2..]),
        Some("stats") => stats::run(&args[2..]),
        Some("log") => log::run(&args[2..]),
        Some("box") => boxscore::run(&args[2..]),
        Some("validate") => validate::run(&args[2..]),
        Some("possession") => possession::run(&args[2..]),
        Some("init") => init::run(&args[2..]),
        _ => {
            eprintln!("usage:");
            eprintln!("  tithe play     [--seed N] [--out FILE] [--max-ticks N] [--setup FILE]");
            eprintln!("  tithe stats    [--matches N] [--seed N] [--setup FILE]");
            eprintln!("  tithe log      [--seed N] [--max-ticks N] [--setup FILE]");
            eprintln!("  tithe box      [--seed N] [--max-ticks N] [--setup FILE]");
            eprintln!(
                "  tithe validate --attr <name> [--hi N --lo N --baseline N --matches N --seed N]"
            );
            eprintln!("  tithe possession [--matches N] [--seed N] [--setup FILE]");
            eprintln!("  tithe init     [--out FILE]   # write an editable example setup");
            std::process::exit(2);
        }
    }
}

/// Load the `--setup FILE` matchup if the flag is present; `None` means use the
/// default RNG-rolled teams. Exits with a clear message on a missing or invalid
/// file, so authoring mistakes fail loudly rather than silently.
pub(crate) fn load_setup(args: &[String]) -> Option<MatchSetup> {
    let path = flag(args, "--setup")?;
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("error: cannot read setup '{path}': {e}");
        std::process::exit(2);
    });
    let setup = toml::from_str::<MatchSetup>(&text).unwrap_or_else(|e| {
        eprintln!("error: invalid setup '{path}': {e}");
        std::process::exit(2);
    });
    Some(setup)
}

/// Build a simulation for `seed`, from an authored `setup` if one was loaded,
/// else from the default teams. Exits on a malformed setup (bad roster size,
/// unknown formation, …).
pub(crate) fn build_sim(setup: &Option<MatchSetup>, seed: u64) -> Simulation {
    match setup {
        Some(setup) => Simulation::from_setup(setup, seed).unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(2);
        }),
        None => Simulation::new(seed),
    }
}

/// Value of a `--flag value` option, if present.
pub(crate) fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Parse a `--flag value` option, falling back to `default`.
pub(crate) fn flag_or<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> T {
    flag(args, name)
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}
