//! `tithe` — command-line consumers of the sim's event stream.
//!
//! The sim is headless and never knows it's watched; this binary is one of its
//! consumers (CLAUDE.md → Committed architecture). Two subcommands:
//!
//! - `play` — run one match and write a self-contained HTML replay you open in
//!   a browser. The "is it fun to *watch*" test.
//! - `stats` — run a batch of AI-vs-AI matches and print tuning metrics. The
//!   "do the numbers work" test (the doc's stated tuning method).
//!
//! Floats and serialization live here at the render boundary — never in
//! `tithe-sim`.

mod log;
mod replay;
mod stats;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("play") => replay::run(&args[2..]),
        Some("stats") => stats::run(&args[2..]),
        Some("log") => log::run(&args[2..]),
        _ => {
            eprintln!("usage:");
            eprintln!("  tithe play  [--seed N] [--out FILE] [--max-ticks N]");
            eprintln!("  tithe stats [--matches N] [--seed N]");
            eprintln!("  tithe log   [--seed N] [--max-ticks N]");
            std::process::exit(2);
        }
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
