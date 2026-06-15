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
use crate::development::DevelopmentModel;
use crate::player::{Player, PlayerId, Ratings};
use crate::scouting::{ScoutConfig, ScoutReport};
use crate::season::{Schedule, Season, Standing};
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
const DEVELOPMENT_SEED_SALT: u64 = 0xDE7E_109D_DE7E_109D;

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
    /// Matches played **outside** a season (exhibitions / friendlies), in play
    /// order. Season fixtures live on the [`Season`]; both share one monotonic
    /// match index for seed derivation (see `match_index`).
    pub history: Vec<MatchRecord>,
    /// The season in progress, if one has been started.
    season: Option<Season>,
    /// How many development years have elapsed (each [`advance_season`] call) —
    /// the index that seeds each year's per-player development, so aging is
    /// reproducible.
    ///
    /// [`advance_season`]: Career::advance_season
    seasons_advanced: u32,
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
            season: None,
            seasons_advanced: 0,
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
    /// player enters the world. An authored player is taken as *finished* — his
    /// potential equals his current ratings and his development risk is zero — so
    /// what you author is exactly what you get (development can be set explicitly
    /// afterward via the pool if a test wants a prospect).
    fn add_player(&mut self, p: NewPlayer) -> PlayerId {
        let id = self.mint_id();
        self.players.push(Player {
            id,
            name: p.name,
            age: p.age,
            potential: p.ratings.clone(),
            ratings: p.ratings,
            development_risk: 0,
            season_usage: crate::development::Usage::default(),
            // A plausible pre-career history for his age, so he reads as scouted.
            appearances: (p.age.saturating_sub(18)) as u32 * 12,
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

    /// Play an **exhibition** `home` vs `away`, record it to `history`, and return
    /// it. (For league play, start a season and use
    /// [`play_next_fixture`](Career::play_next_fixture).) The match seed is derived
    /// from the career seed and the running match index, so replaying a career
    /// (same seed, same matches in the same order) reproduces every result.
    pub fn play(&mut self, home: usize, away: usize) -> MatchResult {
        let result = self.play_fixture(home, away);
        self.accumulate_usage(home, away, &result.box_score);
        self.history.push(result.record.clone());
        result
    }

    /// Generate a round-robin schedule over the current clubs and make it the
    /// active season (replacing any in progress). `double` plays each pairing home
    /// and away. Needs at least two clubs to produce fixtures.
    pub fn start_season(&mut self, double: bool) {
        let schedule = Schedule::round_robin(self.clubs.len(), double);
        self.season = Some(Season::new(schedule));
    }

    /// Play the next unplayed fixture of the active season, recording it on the
    /// season, or `None` if there is no season or it is already complete.
    pub fn play_next_fixture(&mut self) -> Option<MatchResult> {
        let fixture = self.season.as_ref()?.next_fixture()?;
        let result = self.play_fixture(fixture.home, fixture.away);
        self.accumulate_usage(fixture.home, fixture.away, &result.box_score);
        self.season
            .as_mut()
            .expect("season present")
            .record(result.record.clone());
        Some(result)
    }

    /// Fold a played match's box score into the involved players' season usage, so
    /// [`advance_season`](Career::advance_season) can bias each player's growth
    /// toward what he actually did. Agent ids run the home roster then the away
    /// roster (team-0-first, the projection's ordering).
    fn accumulate_usage(&mut self, home: usize, away: usize, box_score: &BoxScore) {
        let mut roster_ids: Vec<PlayerId> = Vec::with_capacity(box_score.players.len());
        roster_ids.extend(self.clubs[home].roster.iter().copied());
        roster_ids.extend(self.clubs[away].roster.iter().copied());
        for (id, line) in roster_ids.iter().zip(&box_score.players) {
            if let Some(player) = self.players.iter_mut().find(|p| p.id == *id) {
                player.season_usage.add_line(line);
                player.appearances += 1; // exposure → tighter scouting
            }
        }
    }

    /// A scouting report on a pooled player — fuzzy bands over his true ratings,
    /// tightened by how much he's been seen (see [`crate::scouting`]). The career
    /// seed fixes the bands, so a report is stable across looks. Panics if the id
    /// isn't in the pool.
    pub fn scout(&self, id: PlayerId) -> ScoutReport {
        ScoutConfig::default().scout(self.player(id), self.seed)
    }

    /// Play out the rest of the active season's fixtures, in schedule order.
    pub fn play_season(&mut self) {
        while self.play_next_fixture().is_some() {}
    }

    /// Advance every pooled player one development year (a season's aging): the
    /// young grow toward their ceilings — **biased toward the attributes they
    /// were deployed to use** (design doc §6) — the old decline shape-first, each
    /// perturbed by his own risk (see [`DevelopmentModel`]). Each player's season
    /// usage is consumed and reset here. Players are seeded from the career seed,
    /// the development year, and their id, so aging is reproducible and
    /// independent of pool order. Does not touch the match schedule — call it when
    /// a season ends to roll the league forward.
    pub fn advance_season(&mut self) {
        let model = DevelopmentModel::default();
        let year_seed = derive_seed(
            self.seed ^ DEVELOPMENT_SEED_SALT,
            self.seasons_advanced as u64,
        );
        for player in &mut self.players {
            // Take the season's deployment record (resetting it for next season).
            let usage = std::mem::take(&mut player.season_usage);
            let mut rng = Rng::new(derive_seed(year_seed, player.id.0 as u64));
            model.advance(player, &usage, &mut rng);
        }
        self.seasons_advanced += 1;
    }

    /// The active season, if one has been started.
    pub fn season(&self) -> Option<&Season> {
        self.season.as_ref()
    }

    /// The current league table (best club first), or empty if no season exists.
    pub fn standings(&self) -> Vec<Standing> {
        self.season
            .as_ref()
            .map_or_else(Vec::new, |s| s.standings(self.clubs.len()))
    }

    /// Build and run one matchup without recording it. The seed comes from the
    /// running match index, so exhibitions and season fixtures alike get unique,
    /// reproducible seeds. The caller records the result where it belongs.
    fn play_fixture(&self, home: usize, away: usize) -> MatchResult {
        let seed = derive_seed(self.seed, self.match_index());
        let setup = self.build_match_setup(home, away);
        play_setup(&setup, seed, home, away)
    }

    /// How many matches have been played so far (exhibitions plus season
    /// fixtures) — the index the next match's seed derives from.
    fn match_index(&self) -> u64 {
        let season_played = self.season.as_ref().map_or(0, |s| s.results.len());
        (self.history.len() + season_played) as u64
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

    /// A built league plays a full single round-robin and every fixture lands in
    /// the table: each of N clubs plays N-1 games and the table is win-sorted.
    #[test]
    fn season_plays_every_fixture_into_a_full_table() {
        let mut career = Career::new(11);
        for name in ["A", "B", "C", "D"] {
            career.add_generated_club(name);
        }
        career.start_season(false);
        career.play_season();

        let season = career.season().expect("season started");
        assert!(season.is_complete());
        assert_eq!(season.results.len(), 4 * 3 / 2); // 6 fixtures

        let table = career.standings();
        assert_eq!(table.len(), 4);
        // Every club played all three opponents once.
        assert!(table.iter().all(|s| s.played == 3));
        // Total wins across the table equals the number of decisive matches.
        let decisive = season.results.iter().filter(|r| r.winner.is_some()).count();
        assert_eq!(table.iter().map(|s| s.won).sum::<u32>(), decisive as u32);
        // The table is sorted: wins non-increasing.
        assert!(table.windows(2).all(|w| w[0].won >= w[1].won));
    }

    #[test]
    fn season_is_reproducible_from_the_seed() {
        let build = |seed| {
            let mut c = Career::new(seed);
            for name in ["A", "B", "C", "D"] {
                c.add_generated_club(name);
            }
            c.start_season(true);
            c.play_season();
            c
        };
        let a = build(8);
        let b = build(8);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

    #[test]
    fn season_survives_a_serde_round_trip_and_resumes() {
        let mut career = Career::new(2);
        for name in ["A", "B", "C", "D"] {
            career.add_generated_club(name);
        }
        career.start_season(false);
        career.play_next_fixture();
        career.play_next_fixture(); // partway through

        let json = serde_json::to_string(&career).unwrap();
        let mut back: Career = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&back).unwrap());
        // The reloaded season resumes and completes.
        assert!(!back.season().unwrap().is_complete());
        back.play_season();
        assert!(back.season().unwrap().is_complete());
    }

    #[test]
    fn advancing_seasons_ages_the_whole_pool_reproducibly() {
        let build = |seed| {
            let mut c = Career::new(seed);
            c.add_generated_club("Embers");
            c.add_free_agents(3);
            c
        };
        let mut a = build(5);
        let ages_before: Vec<u8> = a.players.iter().map(|p| p.age).collect();
        a.advance_season();
        a.advance_season();
        // Everyone — affiliated and free agents — aged by the number of advances.
        for (p, before) in a.players.iter().zip(&ages_before) {
            assert_eq!(p.age, before + 2);
        }
        // Aging is reproducible from the seed and independent of pool order.
        let mut b = build(5);
        b.advance_season();
        b.advance_season();
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

    #[test]
    fn exhibitions_and_season_fixtures_share_one_seed_index() {
        let mut career = Career::new(4);
        career.add_generated_club("A");
        career.add_generated_club("B");
        let exhibition = career.play(0, 1).record.seed; // index 0
        career.start_season(false);
        let fixture = career.play_next_fixture().unwrap().record.seed; // index 1
        assert_ne!(
            exhibition, fixture,
            "the season fixture advances past the exhibition"
        );
    }
}
