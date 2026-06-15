//! Seasons and scheduling: a round-robin [`Schedule`] over the league's clubs, a
//! [`Season`] that plays through it and accumulates results, and the [`Standing`]
//! table those results derive into. The [`Career`](crate::Career) drives a season
//! (it owns the clubs and the sim); this module is the pure scheduling + tallying
//! around it.

use crate::career::MatchRecord;
use serde::{Deserialize, Serialize};

/// One scheduled match: club indices into [`Career::clubs`](crate::Career::clubs).
/// `home` is team 0 in the built match, `away` team 1. (There is no mechanical
/// home advantage in the sim yet, so this is identity, not edge — but the slot is
/// here for when there is.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fixture {
    pub home: usize,
    pub away: usize,
}

/// A fixture list grouped into rounds (matchdays): in each round every club plays
/// at most once. Flattened in round order, `rounds` is the order fixtures are
/// played.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    pub rounds: Vec<Vec<Fixture>>,
}

impl Schedule {
    /// A round-robin schedule over `num_clubs` clubs by the **circle method**: pair
    /// the ends of a list, fix one club, and rotate the rest each round. Single
    /// round-robin is `num_clubs - 1` rounds (each pair meets once); `double` adds
    /// a mirrored second leg with home/away swapped. An odd club count gets a
    /// rotating bye (one club rests each round). Fewer than two clubs yields no
    /// fixtures.
    pub fn round_robin(num_clubs: usize, double: bool) -> Self {
        let mut rounds = Vec::new();
        if num_clubs >= 2 {
            let odd = num_clubs % 2 == 1;
            // Work over an even slot count; the extra slot is the "bye" when odd.
            let count = if odd { num_clubs + 1 } else { num_clubs };
            let bye = count - 1;
            let half = count / 2;
            let mut slots: Vec<usize> = (0..count).collect();

            for r in 0..count - 1 {
                let mut round = Vec::new();
                for i in 0..half {
                    let a = slots[i];
                    let b = slots[count - 1 - i];
                    if odd && (a == bye || b == bye) {
                        continue; // the resting club
                    }
                    // Alternate home/away by round+position for rough balance.
                    let (home, away) = if (r + i) % 2 == 0 { (a, b) } else { (b, a) };
                    round.push(Fixture { home, away });
                }
                rounds.push(round);
                // Fix slot 0, rotate the rest one step — the circle method.
                slots[1..].rotate_right(1);
            }

            if double {
                let first_leg = rounds.clone();
                for round in first_leg {
                    let mirror = round
                        .iter()
                        .map(|f| Fixture {
                            home: f.away,
                            away: f.home,
                        })
                        .collect();
                    rounds.push(mirror);
                }
            }
        }
        Schedule { rounds }
    }

    /// Every fixture in the order it is played (round by round).
    pub fn all_fixtures(&self) -> impl Iterator<Item = &Fixture> {
        self.rounds.iter().flatten()
    }

    /// Total number of fixtures in the schedule.
    pub fn fixture_count(&self) -> usize {
        self.rounds.iter().map(Vec::len).sum()
    }
}

/// A season in progress: its [`Schedule`] and the results of the fixtures played
/// so far, in schedule order. The next fixture to play is the one at index
/// `results.len()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Season {
    pub schedule: Schedule,
    pub results: Vec<MatchRecord>,
}

impl Season {
    /// A fresh season over `schedule`, nothing played yet.
    pub fn new(schedule: Schedule) -> Self {
        Season {
            schedule,
            results: Vec::new(),
        }
    }

    /// The next unplayed fixture, or `None` if the season is complete.
    pub fn next_fixture(&self) -> Option<Fixture> {
        self.schedule
            .all_fixtures()
            .nth(self.results.len())
            .copied()
    }

    /// Whether every fixture has been played.
    pub fn is_complete(&self) -> bool {
        self.results.len() >= self.schedule.fixture_count()
    }

    /// Record a played fixture's result (appended in schedule order).
    pub(crate) fn record(&mut self, record: MatchRecord) {
        self.results.push(record);
    }

    /// The league table derived from the results so far, best club first. Sorted
    /// by wins, then soul difference, then souls for, then club index (a total,
    /// deterministic order). `num_clubs` sizes the table so clubs yet to play
    /// still appear.
    pub fn standings(&self, num_clubs: usize) -> Vec<Standing> {
        let mut table: Vec<Standing> = (0..num_clubs).map(Standing::new).collect();
        for rec in &self.results {
            let [home_souls, away_souls] = rec.score;
            let home = &mut table[rec.home];
            home.played += 1;
            home.souls_for += home_souls;
            home.souls_against += away_souls;
            let away = &mut table[rec.away];
            away.played += 1;
            away.souls_for += away_souls;
            away.souls_against += home_souls;
            match rec.winner {
                Some(0) => {
                    table[rec.home].won += 1;
                    table[rec.away].lost += 1;
                }
                Some(1) => {
                    table[rec.away].won += 1;
                    table[rec.home].lost += 1;
                }
                _ => {} // unresolved (hit the tick bound): counts as played only
            }
        }
        table.sort_by(|a, b| {
            b.won
                .cmp(&a.won)
                .then(b.soul_diff().cmp(&a.soul_diff()))
                .then(b.souls_for.cmp(&a.souls_for))
                .then(a.club.cmp(&b.club))
        });
        table
    }
}

/// One club's line in the league table. A match is first-to-X souls, so there are
/// no draws; `points` is simply wins (1 per win) until a richer points model earns
/// its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    pub club: usize,
    pub played: u32,
    pub won: u32,
    pub lost: u32,
    pub souls_for: u32,
    pub souls_against: u32,
}

impl Standing {
    fn new(club: usize) -> Self {
        Standing {
            club,
            played: 0,
            won: 0,
            lost: 0,
            souls_for: 0,
            souls_against: 0,
        }
    }

    /// Souls scored minus conceded (the table's primary tiebreaker).
    pub fn soul_diff(&self) -> i32 {
        self.souls_for as i32 - self.souls_against as i32
    }

    /// League points — one per win (no draws in a first-to-X sport).
    pub fn points(&self) -> u32 {
        self.won
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Every distinct pair meets exactly once in a single round-robin, and each
    /// round has each club at most once — checked across even and odd sizes.
    #[test]
    fn single_round_robin_pairs_everyone_once() {
        for n in 2..=7 {
            let schedule = Schedule::round_robin(n, false);
            assert_eq!(
                schedule.fixture_count(),
                n * (n - 1) / 2,
                "n={n} fixture count"
            );

            let mut pairs = BTreeSet::new();
            for round in &schedule.rounds {
                let mut seen = BTreeSet::new();
                for f in round {
                    // No club appears twice in a round.
                    assert!(seen.insert(f.home), "n={n} dup club in round");
                    assert!(seen.insert(f.away), "n={n} dup club in round");
                    let pair = (f.home.min(f.away), f.home.max(f.away));
                    assert!(pairs.insert(pair), "n={n} pair {pair:?} twice");
                }
            }
            assert_eq!(pairs.len(), n * (n - 1) / 2, "n={n} all pairs present");
        }
    }

    #[test]
    fn double_round_robin_doubles_and_mirrors() {
        let single = Schedule::round_robin(4, false);
        let double = Schedule::round_robin(4, true);
        assert_eq!(double.fixture_count(), 2 * single.fixture_count());
        // Each club plays every other home and away once: 2*(n-1) games each.
        let mut games = [0u32; 4];
        for f in double.all_fixtures() {
            games[f.home] += 1;
            games[f.away] += 1;
        }
        assert!(games.iter().all(|&g| g == 2 * 3));
    }

    #[test]
    fn standings_rank_by_wins_then_soul_diff() {
        // Three clubs; hand a season known results and check the order.
        let schedule = Schedule::round_robin(3, false);
        let mut season = Season::new(schedule);
        // 0 beats 1 big, 0 beats 2 narrow, 2 beats 1 — so 0 first, then 2, then 1.
        season.record(MatchRecord {
            home: 0,
            away: 1,
            seed: 0,
            score: [11, 2],
            winner: Some(0),
        });
        season.record(MatchRecord {
            home: 0,
            away: 2,
            seed: 0,
            score: [11, 9],
            winner: Some(0),
        });
        season.record(MatchRecord {
            home: 2,
            away: 1,
            seed: 0,
            score: [11, 5],
            winner: Some(0),
        });
        let table = season.standings(3);
        assert_eq!(table[0].club, 0);
        assert_eq!(table[0].won, 2);
        assert_eq!(table[1].club, 2); // one win, positive diff
        assert_eq!(table[2].club, 1); // no wins
        assert_eq!(table[1].points(), 1);
    }

    #[test]
    fn next_fixture_advances_then_completes() {
        let schedule = Schedule::round_robin(2, false);
        let mut season = Season::new(schedule);
        assert!(!season.is_complete());
        let f = season.next_fixture().expect("one fixture");
        season.record(MatchRecord {
            home: f.home,
            away: f.away,
            seed: 0,
            score: [11, 0],
            winner: Some(0),
        });
        assert!(season.is_complete());
        assert!(season.next_fixture().is_none());
    }
}
