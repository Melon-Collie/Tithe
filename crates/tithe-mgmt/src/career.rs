//! A career: the clubs in a save, and the loop that plays a matchup through the
//! sim and folds the result back in. This is the seam in action — [`Career::play`]
//! builds a [`tithe_sim::MatchSetup`], runs the sim to completion, and records
//! the outcome plus the [`tithe_sim::BoxScore`] (which deployment-based
//! development will later read).

use crate::club::{generate_club, Club};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tithe_sim::{BoxScore, Event, MatchSetup, Rng, Simulation};

/// A safety bound so a pathological match can't loop forever. Real matches finish
/// far short of this (first-to-X souls); hitting it means the match never
/// resolved, which the result surfaces as `winner: None`.
const MAX_TICKS: u64 = 100_000;

/// The persistent state of a save: a seed (the career is reproducible from it),
/// the clubs, and the match history. No I/O lives here — a consumer serializes
/// this with whatever format it likes (the crate just derives serde).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Career {
    /// The career seed. Every match seed is derived from this plus the match
    /// index, so the whole history is reproducible from this one number.
    pub seed: u64,
    pub clubs: Vec<Club>,
    pub history: Vec<MatchRecord>,
}

/// The persisted outcome of one match: who played, the seed it ran under, the
/// final score, and the winner (`None` if it hit [`MAX_TICKS`] without resolving).
/// `home` / `away` index into [`Career::clubs`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRecord {
    pub home: usize,
    pub away: usize,
    pub seed: u64,
    pub score: [u32; 2],
    pub winner: Option<u8>,
}

/// What [`Career::play`] hands back: the persisted [`MatchRecord`] plus the
/// [`BoxScore`] for the match. The box score is a *runtime* product (not stored
/// in the save) — it's the per-player tallies a caller feeds into development.
#[derive(Debug, Clone)]
pub struct MatchResult {
    pub record: MatchRecord,
    pub box_score: BoxScore,
}

impl Career {
    /// A demo career: two generated seven-a-side clubs and an empty history.
    /// Deterministic from `seed`. A stand-in until real career setup (drafts,
    /// generated leagues) exists.
    pub fn demo(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let mut next_id = 0u32;
        let embers = generate_club("Embers", 'E', &mut next_id, &mut rng);
        let wardens = generate_club("Wardens", 'W', &mut next_id, &mut rng);
        Career {
            seed,
            clubs: vec![embers, wardens],
            history: Vec::new(),
        }
    }

    /// Play `home` vs `away`, record the outcome, and return it. The match seed is
    /// derived from the career seed and the current match index, so replaying a
    /// career (same seed, same fixtures in the same order) reproduces every result.
    pub fn play(&mut self, home: usize, away: usize) -> MatchResult {
        let seed = derive_seed(self.seed, self.history.len() as u64);
        let setup = self.build_match_setup(home, away);
        let result = play_setup(&setup, seed, home, away);
        self.history.push(result.record.clone());
        result
    }

    /// Project two clubs into a single [`MatchSetup`]: each club's formations are
    /// registered under namespaced keys, and team 0 is `home`, team 1 is `away`.
    pub fn build_match_setup(&self, home: usize, away: usize) -> MatchSetup {
        let (home_team, home_formations) = self.clubs[home].to_team_setup("home");
        let (away_team, away_formations) = self.clubs[away].to_team_setup("away");
        let mut formations = BTreeMap::new();
        for (name, spec) in home_formations.into_iter().chain(away_formations) {
            formations.insert(name, spec);
        }
        MatchSetup {
            formations,
            teams: vec![home_team, away_team],
        }
    }
}

/// Run one built setup to completion and assemble the result. Separate from
/// [`Career::play`] so it stays a pure `(setup, seed) → result` function.
fn play_setup(setup: &MatchSetup, seed: u64, home: usize, away: usize) -> MatchResult {
    // build_match_setup is correct by construction, so a setup error here is a bug.
    let mut sim = Simulation::from_setup(setup, seed).expect("career-built setup is valid");
    let num_agents = sim.agents().len();

    let mut events: Vec<Event> = Vec::new();
    let mut ticks = 0;
    while sim.winner().is_none() && ticks < MAX_TICKS {
        events.extend(sim.tick());
        ticks += 1;
    }

    let box_score = BoxScore::from_events(&events, num_agents);
    let record = MatchRecord {
        home,
        away,
        seed,
        score: sim.score(),
        winner: sim.winner(),
    };
    MatchResult { record, box_score }
}

/// Derive a per-match seed from the career seed and the match index. Runs the
/// pair through one SplitMix64 step so adjacent matches get well-separated seeds
/// (a plain `seed + index` would give correlated streams).
fn derive_seed(career_seed: u64, match_index: u64) -> u64 {
    Rng::new(career_seed.wrapping_add(match_index)).next_u64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_career_is_deterministic() {
        let a = Career::demo(42);
        let b = Career::demo(42);
        // Compare via serialized form (FormationSpec isn't PartialEq); this also
        // exercises that the whole structure serializes.
        let ja = serde_json::to_string(&a).unwrap();
        let jb = serde_json::to_string(&b).unwrap();
        assert_eq!(ja, jb);
        assert_eq!(a.clubs.len(), 2);
        assert_eq!(a.clubs[0].players.len(), 7);
        // Ids are globally unique across clubs (0..14 here).
        assert_eq!(a.clubs[0].players[0].id.0, 0);
        assert_eq!(a.clubs[1].players[0].id.0, 7);
    }

    #[test]
    fn playing_a_match_resolves_and_records_it() {
        let mut career = Career::demo(1);
        let result = career.play(0, 1);
        // The match resolved within the safety bound.
        assert!(
            result.record.winner.is_some(),
            "match should produce a winner"
        );
        // The winner's score reached the win threshold; the result was recorded.
        assert_eq!(career.history.len(), 1);
        assert_eq!(career.history[0], result.record);
        // The box score is sized to the 14 agents (7 per side).
        assert_eq!(result.box_score.players.len(), 14);
    }

    #[test]
    fn replaying_the_same_fixture_reproduces_the_result() {
        let mut a = Career::demo(5);
        let mut b = Career::demo(5);
        let ra = a.play(0, 1);
        let rb = b.play(0, 1);
        assert_eq!(ra.record, rb.record);
    }

    #[test]
    fn successive_matches_get_distinct_seeds() {
        let mut career = Career::demo(9);
        let first = career.play(0, 1).record.seed;
        let second = career.play(0, 1).record.seed;
        assert_ne!(first, second, "each match index derives its own seed");
    }

    #[test]
    fn career_survives_a_serde_round_trip() {
        let mut career = Career::demo(3);
        career.play(0, 1);
        let json = serde_json::to_string(&career).unwrap();
        let back: Career = serde_json::from_str(&json).unwrap();
        // Re-serialization is stable (FormationSpec isn't PartialEq, so compare
        // the wire form), proving the save round-trips losslessly.
        assert_eq!(json, serde_json::to_string(&back).unwrap());
        assert_eq!(back.history.len(), 1);
    }
}
