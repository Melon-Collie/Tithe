//! `init`: write an editable example match-setup file.
//!
//! This is the hand-authoring on-ramp for the coach-input boundary while the
//! game's roster/tactics UI doesn't exist yet — it serializes
//! [`MatchSetup::default_match`] to TOML so you have a valid, runnable file to
//! edit and feed back via `--setup`.

use tithe_sim::MatchSetup;

pub fn run(args: &[String]) {
    let out = crate::flag(args, "--out").unwrap_or_else(|| "setup.toml".to_string());

    let setup = MatchSetup::default_match();
    // Compact (not pretty): keeps the [x, y] anchor arrays inline on one line,
    // which is far friendlier to hand-edit than the pretty form's nested blocks.
    let toml = toml::to_string(&setup).expect("serialize default setup");
    let body = format!("{HEADER}{toml}");
    std::fs::write(&out, body).unwrap_or_else(|e| {
        eprintln!("error: cannot write '{out}': {e}");
        std::process::exit(2);
    });

    println!("wrote {out} — edit it and run e.g.  tithe log --setup {out}");
}

/// A short comment block at the top of the emitted file explaining the format.
const HEADER: &str = "\
# Tithe match setup - the two coach inputs (formations + roles) plus players.
#
# [formations.NAME]  a positioning template: one [x, y] anchor per slot,
#                     authored in the team-0 frame (own goal at -x). A team
#                     using it on the +x side is mirrored automatically.
# [[teams]]           exactly two; players are listed in slot order and fill
#                     the formation's slots 1:1. Attributes are 0..=100.
#                     role = finisher | playmaker | presser | anchor | rover
#                     (a label today; it shows in the play-by-play log).

";
