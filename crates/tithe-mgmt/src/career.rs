//! A career: the central **player pool**, the league of clubs that draw from it,
//! the builder that stands them up, and the loop that plays a matchup through the
//! sim and folds the result back in.
//!
//! Players live in one pool ([`Career::players`]); a [`Club`] holds only a roster
//! of [`PlayerId`]s into it. A player on no club's roster is a **free agent** —
//! the pool is what makes free agency, transfers, and drafts representable
//! (their *behaviour* — signing, wages, contracts — is the later money slice).
//! The career mints every id from a monotonic counter, so identities stay
//! globally unique and never get reused as players come and go.
//!
//! [`Career::play`] is the seam in action: it builds a [`tithe_sim::MatchSetup`],
//! runs the sim to completion, and records the outcome plus the
//! [`tithe_sim::BoxScore`] (which deployment-based development will later read).

use crate::club::{Club, Tactics};
use crate::player::{Player, PlayerId, Ratings};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tithe_sim::setup::{FormationSpec, TeamSetup};
use tithe_sim::{BoxScore, Event, MatchSetup, PlayerSetup, Rng, Simulation};

/// A safety bound so a pathological match can't loop forever. Real matches finish
/// far short of this (first-to-X souls); hitting it means the match never
/// resolved, which the result surfaces as `winner: None`.
const MAX_TICKS: u64 = 100_000;

/// Domain salts so club-roster, free-agent, and match seeds derived from the same
/// career seed never share a stream (they seed the seed for different purposes).
const CLUB_SEED_SALT: u64 = 0xC1AB_5EED_C1AB_5EED;
const FREE_AGENT_SEED_SALT: u64 = 0xF4EE_A9E7_F4EE_A9E7;

/// The number of players in a generated club. The default tactics are seven-a-side
/// (design doc's initial team size), so generated rosters match that shape.
const SEVEN_A_SIDE: usize = 7;

/// The persistent state of a save: the seed (the career is reproducible from it),
/// the next player id to mint, the **player pool**, the league's clubs, and the
/// match history. No I/O lives here — a consumer serializes this with whatever
/// format it likes (the crate just derives serde).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Career {
    /// The career seed. Every roster and every match seed is derived from this, so
    /// the whole career is reproducible from this one number.
    pub seed: u64,
    /// Monotonic id source. Never reused, so a [`PlayerId`] stays stable even as
    /// players are added or (later) transferred and retired.
    next_player_id: u32,
    /// Every player in the world, club-affiliated or not. Clubs reference these by
    /// id; a player here on no roster is a free agent (see [`free_agents`]).
    ///
    /// [`free_agents`]: Career::free_agents
    pub players: Vec<Player>,
    pub clubs: Vec<Club>,
    pub history: Vec<MatchRecord>,
}

/// A player to add to the pool — everything but the id, which the career mints so
/// identities stay globally unique. Built by a caller (a test, a future roster
/// editor) that wants exact ratings rather than generated ones.
#[derive(Debug, Clone)]
pub struct NewPlayer {
    pub name: String,
    pub age: u8,
    pub ratings: Ratings,
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
    /// An empty career: a seed, an empty pool, no clubs. Add clubs with
    /// [`add_generated_club`](Career::add_generated_club) /
    /// [`add_authored_club`](Career::add_authored_club), and unaffiliated players
    /// with [`add_free_agents`](Career::add_free_agents).
    pub fn new(seed: u64) -> Self {
        Career {
            seed,
            next_player_id: 0,
            players: Vec::new(),
            clubs: Vec::new(),
            history: Vec::new(),
        }
    }

    /// A demo career: an `Embers` vs `Wardens` two-club league. Convenience over
    /// [`new`](Career::new) + two [`add_generated_club`](Career::add_generated_club)
    /// calls, for examples and tests.
    pub fn demo(seed: u64) -> Self {
        let mut career = Career::new(seed);
        career.add_generated_club("Embers");
        career.add_generated_club("Wardens");
        career
    }

    /// Add a seven-a-side club with a generated roster and the default tactics,
    /// returning its index in [`clubs`](Career::clubs). The roster derives from the
    /// career seed and the club's index, so the league is reproducible and a club's
    /// players don't depend on what was added before it.
    pub fn add_generated_club(&mut self, name: &str) -> usize {
        let club_index = self.clubs.len();
        let mut rng = Rng::new(derive_seed(self.seed ^ CLUB_SEED_SALT, club_index as u64));
        let prefix = name.chars().next().unwrap_or('P').to_ascii_uppercase();

        let mut roster = Vec::with_capacity(SEVEN_A_SIDE);
        for i in 0..SEVEN_A_SIDE {
            let id = self.mint_id();
            let player = Player::generate(id, &format!("{prefix}{}", i + 1), &mut rng);
            self.players.push(player);
            roster.push(id);
        }
        self.clubs.push(Club {
            name: name.to_string(),
            roster,
            tactics: Tactics::default_seven(),
        });
        club_index
    }

    /// Add a club with an authored roster and explicit tactics, returning its
    /// index. The career mints each player's id and pools them. The roster length
    /// must match the tactics' slot count, or the match will fail to build at
    /// [`play`](Career::play) time.
    pub fn add_authored_club(
        &mut self,
        name: &str,
        tactics: Tactics,
        roster: Vec<NewPlayer>,
    ) -> usize {
        let club_index = self.clubs.len();
        let roster = roster.into_iter().map(|p| self.add_player(p)).collect();
        self.clubs.push(Club {
            name: name.to_string(),
            roster,
            tactics,
        });
        club_index
    }

    /// Add `count` generated **free agents** to the pool — players on no club.
    /// Each derives from the career seed and its own id, so the set is
    /// reproducible. Returns their ids. Signing them to a club is the later money
    /// slice; this just lets the pool hold the unaffiliated.
    pub fn add_free_agents(&mut self, count: usize) -> Vec<PlayerId> {
        let mut ids = Vec::with_capacity(count);
        for _ in 0..count {
            let id = self.mint_id();
            let mut rng = Rng::new(derive_seed(self.seed ^ FREE_AGENT_SEED_SALT, id.0 as u64));
            let player = Player::generate(id, &format!("FA{}", id.0), &mut rng);
            self.players.push(player);
            ids.push(id);
        }
        ids
    }

    /// Pool one authored player, minting its id. The single place an authored
    /// player enters the world.
    fn add_player(&mut self, p: NewPlayer) -> PlayerId {
        let id = self.mint_id();
        self.players.push(Player {
            id,
            name: p.name,
            age: p.age,
            ratings: p.ratings,
        });
        id
    }

    /// Mint the next globally-unique player id.
    fn mint_id(&mut self) -> PlayerId {
        let id = PlayerId(self.next_player_id);
        self.next_player_id += 1;
        id
    }

    /// Look up a pooled player by id. Panics if the id was never minted by this
    /// career (a programming error — ids come from [`mint_id`](Career::mint_id)).
    pub fn player(&self, id: PlayerId) -> &Player {
        self.players
            .iter()
            .find(|p| p.id == id)
            .expect("player id belongs to this career's pool")
    }

    /// The free agents: pooled players on no club's roster, in pool order.
    pub fn free_agents(&self) -> Vec<&Player> {
        let affiliated: BTreeSet<PlayerId> = self
            .clubs
            .iter()
            .flat_map(|c| c.roster.iter().copied())
            .collect();
        self.players
            .iter()
            .filter(|p| !affiliated.contains(&p.id))
            .collect()
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
        let (home_team, home_formations) = self.build_team_setup(&self.clubs[home], "home");
        let (away_team, away_formations) = self.build_team_setup(&self.clubs[away], "away");
        let mut formations = BTreeMap::new();
        for (name, spec) in home_formations.into_iter().chain(away_formations) {
            formations.insert(name, spec);
        }
        MatchSetup {
            formations,
            teams: vec![home_team, away_team],
        }
    }

    /// Resolve a club's roster ids against the pool and project into the sim's
    /// input form: a [`TeamSetup`] (referencing its formations by `{key}_attack` /
    /// `{key}_defend`) plus the two named [`FormationSpec`]s the caller registers.
    /// `key` namespaces the formations so two clubs' shapes can't collide.
    fn build_team_setup(
        &self,
        club: &Club,
        key: &str,
    ) -> (TeamSetup, Vec<(String, FormationSpec)>) {
        let attack_name = format!("{key}_attack");
        let defend_name = format!("{key}_defend");

        let players = club
            .roster
            .iter()
            .zip(&club.tactics.roles)
            .map(|(&id, &(attack_role, defend_role))| {
                let p = self.player(id);
                let r = &p.ratings;
                PlayerSetup {
                    name: p.name.clone(),
                    attack_role,
                    defend_role,
                    accuracy: r.accuracy,
                    range: r.range,
                    handling: r.handling,
                    stripping: r.stripping,
                    contesting: r.contesting,
                    passing: r.passing,
                    positioning: r.positioning,
                    pace: r.pace,
                    awareness: r.awareness,
                }
            })
            .collect();

        let team = TeamSetup {
            name: club.name.clone(),
            attack_formation: attack_name.clone(),
            defend_formation: defend_name.clone(),
            players,
        };
        let formations = vec![
            (attack_name, club.tactics.attack_formation.clone()),
            (defend_name, club.tactics.defend_formation.clone()),
        ];
        (team, formations)
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

/// Derive a sub-seed from a base seed and an index. Runs the pair through one
/// SplitMix64 step so adjacent indices get well-separated seeds (a plain
/// `base + index` would give correlated streams).
fn derive_seed(base: u64, index: u64) -> u64 {
    Rng::new(base.wrapping_add(index)).next_u64()
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
        assert_eq!(a.players.len(), 14);
        // Ids are globally unique across clubs (0..14 here).
        assert_eq!(a.clubs[0].roster[0].0, 0);
        assert_eq!(a.clubs[1].roster[0].0, 7);
    }

    #[test]
    fn league_of_any_size_mints_unique_sequential_ids() {
        let mut career = Career::new(1);
        career.add_generated_club("Cinders");
        career.add_generated_club("Wraiths");
        career.add_generated_club("Beacons");
        assert_eq!(career.clubs.len(), 3);
        assert_eq!(career.players.len(), 21);

        let ids: Vec<u32> = career.players.iter().map(|p| p.id.0).collect();
        let expected: Vec<u32> = (0..21).collect();
        assert_eq!(ids, expected);
    }

    #[test]
    fn free_agents_are_pooled_players_on_no_roster() {
        let mut career = Career::new(4);
        career.add_generated_club("Embers"); // 7 affiliated
        let fas = career.add_free_agents(3); // 3 unaffiliated
        assert_eq!(career.players.len(), 10);

        let free = career.free_agents();
        assert_eq!(free.len(), 3);
        // The free agents are exactly the ids add_free_agents minted (7, 8, 9).
        let free_ids: Vec<PlayerId> = free.iter().map(|p| p.id).collect();
        assert_eq!(free_ids, fas);
    }

    #[test]
    fn generated_club_roster_is_independent_of_add_order() {
        // A club's roster derives from its index, so the same name at the same
        // index produces the same players regardless of the rest of the league.
        let mut a = Career::new(7);
        a.add_generated_club("First");
        a.add_generated_club("Embers");

        let mut b = Career::new(7);
        b.add_generated_club("Other");
        b.add_generated_club("Embers");

        // Both "Embers" sit at index 1 → identical rosters (ids 7..14 too).
        let ra: Vec<_> = a.clubs[1].roster.iter().map(|&id| a.player(id)).collect();
        let rb: Vec<_> = b.clubs[1].roster.iter().map(|&id| b.player(id)).collect();
        assert_eq!(
            serde_json::to_string(&ra).unwrap(),
            serde_json::to_string(&rb).unwrap()
        );
    }

    #[test]
    fn authored_club_keeps_its_ratings_and_gets_minted_ids() {
        let mut career = Career::new(0);
        career.add_generated_club("Embers"); // ids 0..7
        let sniper = NewPlayer {
            name: "Quill".to_string(),
            age: 24,
            ratings: Ratings::from_canonical([90, 80, 50, 30, 40, 50, 55, 60, 70]),
        };
        let idx = career.add_authored_club(
            "Authored",
            Tactics::default_seven(),
            // One real player plus six fillers to match the seven-a-side shape.
            std::iter::once(sniper)
                .chain((0..6).map(|i| NewPlayer {
                    name: format!("Filler{i}"),
                    age: 25,
                    ratings: Ratings::from_canonical([50; 9]),
                }))
                .collect(),
        );
        let quill_id = career.clubs[idx].roster[0];
        let quill = career.player(quill_id);
        assert_eq!(quill.name, "Quill");
        assert_eq!(quill.ratings.accuracy, 90);
        assert_eq!(quill_id.0, 7); // continues the global sequence after club 0
    }

    #[test]
    fn playing_a_match_resolves_and_records_it() {
        let mut career = Career::demo(1);
        let result = career.play(0, 1);
        assert!(
            result.record.winner.is_some(),
            "match should produce a winner"
        );
        assert_eq!(career.history.len(), 1);
        assert_eq!(career.history[0], result.record);
        assert_eq!(result.box_score.players.len(), 14);
    }

    #[test]
    fn any_two_clubs_in_a_larger_league_can_play() {
        let mut career = Career::new(2);
        for name in ["A", "B", "C", "D"] {
            career.add_generated_club(name);
        }
        let result = career.play(0, 3);
        assert!(result.record.winner.is_some());
        assert_eq!(career.history[0].home, 0);
        assert_eq!(career.history[0].away, 3);
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
        career.add_free_agents(2);
        career.play(0, 1);
        let json = serde_json::to_string(&career).unwrap();
        let back: Career = serde_json::from_str(&json).unwrap();
        // Re-serialization is stable (FormationSpec isn't PartialEq, so compare
        // the wire form), proving the save round-trips losslessly.
        assert_eq!(json, serde_json::to_string(&back).unwrap());
        assert_eq!(back.history.len(), 1);
        assert_eq!(back.free_agents().len(), 2);
    }
}
