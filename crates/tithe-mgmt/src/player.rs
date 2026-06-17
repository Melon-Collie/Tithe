//! Persistent players: a stable identity plus the intrinsic ratings the sim
//! consumes. A [`Player`] is *career* state; the sim's per-match agent is built
//! from it at matchup time (see [`crate::club::Club::to_team_setup`]).

use crate::development::Usage;
use crate::finance::Contract;
use serde::{Deserialize, Serialize};
use tithe_sim::{Attribute, Rng};

/// A player's stable, career-long identity. Distinct from the match-local agent
/// id the sim mints per match (see the crate docs) — this one persists across
/// matches, transfers, and seasons, and is globally unique within a career.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub u32);

/// A player's intrinsic capabilities — the ten attributes as integer
/// percentiles `0..=100`, the same wire form the sim's [`tithe_sim::PlayerSetup`]
/// authors (80 → the `Fx` `0.80` the sim consumes). Named fields (not a bare
/// `[u8; 10]`) so a save file is self-describing and adding an attribute can't
/// silently shift an array. The canonical ordering still lives in exactly one
/// place — [`Attribute`] — which [`Ratings::from_canonical`] uses as the bridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ratings {
    pub accuracy: u8,
    pub range: u8,
    pub handling: u8,
    pub stripping: u8,
    pub contesting: u8,
    pub passing: u8,
    pub positioning: u8,
    pub pace: u8,
    pub awareness: u8,
    pub endurance: u8,
}

impl Ratings {
    /// Build from an array in [`Attribute`] canonical order. The single positional
    /// → named mapping in this crate; it keys off the sim's `Attribute` SSOT, so a
    /// reorder there is caught by `tithe-sim`'s own `attribute_keys_match_fields`
    /// test rather than silently misaligning here.
    pub fn from_canonical(a: [u8; 10]) -> Self {
        Ratings {
            accuracy: a[Attribute::Accuracy as usize],
            range: a[Attribute::Range as usize],
            handling: a[Attribute::Handling as usize],
            stripping: a[Attribute::Stripping as usize],
            contesting: a[Attribute::Contesting as usize],
            passing: a[Attribute::Passing as usize],
            positioning: a[Attribute::Positioning as usize],
            pace: a[Attribute::Pace as usize],
            awareness: a[Attribute::Awareness as usize],
            endurance: a[Attribute::Endurance as usize],
        }
    }

    /// Read one rating by its canonical [`Attribute`] key — the named bridge for
    /// code that iterates attributes (development, scouting summaries).
    pub fn get(&self, a: Attribute) -> u8 {
        match a {
            Attribute::Accuracy => self.accuracy,
            Attribute::Range => self.range,
            Attribute::Handling => self.handling,
            Attribute::Stripping => self.stripping,
            Attribute::Contesting => self.contesting,
            Attribute::Passing => self.passing,
            Attribute::Positioning => self.positioning,
            Attribute::Pace => self.pace,
            Attribute::Awareness => self.awareness,
            Attribute::Endurance => self.endurance,
        }
    }

    /// The mean of the ten ratings — a single legible "how good is he" number for
    /// summaries and tests. Not a sim input (the sim reads each attribute).
    pub fn overall(&self) -> u8 {
        let sum: u32 = Attribute::ALL.iter().map(|&a| self.get(a) as u32).sum();
        (sum / Attribute::ALL.len() as u32) as u8
    }

    /// Generate a varied ceiling: a `30..70` baseline with one or two attributes
    /// spiked to `80..100`, so each generated player has a readable identity (a
    /// sniper, a checker, a passer) — this is his *potential* shape (his "grain").
    /// Deterministic from the threaded [`Rng`]. A placeholder for the real
    /// archetype-first generation (design doc §4).
    pub(crate) fn generate(rng: &mut Rng) -> Self {
        let mut a = [0u8; 10];
        for v in a.iter_mut() {
            *v = 30 + rng.below(40) as u8; // 30..70
        }
        for _ in 0..1 + rng.below(2) {
            a[rng.below(10) as usize] = 80 + rng.below(20) as u8; // a spike or two
        }
        Ratings::from_canonical(a)
    }
}

/// A persistent player: stable identity, a name, an age, his current
/// [`Ratings`], the ceiling those grow toward ([`potential`](Player::potential)),
/// and his [`development_risk`](Player::development_risk). Roles and formation are
/// *not* here — those are the coach's per-match inputs (see
/// [`crate::club::Tactics`]), not properties of the player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub age: u8,
    /// What he can do *now* — the values the sim consumes.
    pub ratings: Ratings,
    /// The per-attribute **ceiling** his ratings grow toward (his lifetime peak,
    /// fixed at generation). Current rises toward it when young and falls away
    /// from it in decline, so the current/ceiling gap means "headroom" for a
    /// prospect and "what's been lost" for a veteran (design doc §3).
    pub potential: Ratings,
    /// His own developmental volatility (`0..=100`, aleatoric — a property of
    /// *him*, not of scouting). Higher means his yearly progression swings more,
    /// so high-risk prospects boom or bust while low-risk ones track projection.
    pub development_risk: u8,
    /// What he's done this season — the deployment record (accumulated from match
    /// box scores) that biases which attributes grow when the season is advanced
    /// (design doc §6). Consumed and reset by [`Career::advance_season`]. Zero for
    /// a player who hasn't featured.
    ///
    /// [`Career::advance_season`]: crate::Career::advance_season
    pub season_usage: Usage,
    /// Career matches featured in — never reset. Drives **scouting**: the more a
    /// player has been seen, the tighter his scouted bands (design doc §3, the
    /// exposure dial). See [`crate::scouting`].
    pub appearances: u32,
    /// His [`Contract`] if he's signed to a club, or `None` if he's a free agent.
    /// A roster move (sign/release) is the only thing that changes it; generation
    /// makes an unsigned player and the club that rosters him gives him a deal.
    pub contract: Option<Contract>,
    /// His carried stamina (`0..=100`, 100 = fresh) — his condition as of his last
    /// match's end. Matches drain it; rest between matches restores it. This is
    /// what makes a deep 12-man squad matter: ride your starters and they wear
    /// down across a season; rotate and they stay fresh (design doc §6 fatigue).
    pub stamina: u8,
}

/// A generated player's assumed matches-per-year before the career starts, so a
/// generated veteran reads as well-scouted and a fresh prospect as an unknown.
const PRESUMED_GAMES_PER_YEAR: u32 = 12;

impl Player {
    /// Generate a player with the given identity and name. Draws a ceiling and an
    /// age, then **ages him up** from 18 through the default development model, so
    /// a generated veteran is a coherent, declined version of his own peak rather
    /// than random numbers. Deterministic from the threaded [`Rng`].
    pub fn generate(id: PlayerId, name: &str, rng: &mut Rng) -> Self {
        let model = crate::development::DevelopmentModel::default();
        let target_age = 18 + rng.below(15) as u8; // 18..=32
        let potential = Ratings::generate(rng);
        let development_risk = rng.below(101) as u8;
        // Born raw at 18, then lived forward to his current age via the same
        // model that ages everyone — booms and busts already baked in.
        let mut player = Player {
            id,
            name: name.to_string(),
            ratings: model.youth_ratings(&potential),
            potential,
            development_risk,
            age: 18,
            season_usage: Usage::default(),
            appearances: 0,
            contract: None, // unsigned until a club rosters him
            stamina: 100,   // generated fresh
        };
        // No match history when synthesizing a career — uniform growth toward the
        // ceiling, so a generated veteran is a coherent aged-up youngster.
        while player.age < target_age {
            model.advance(&mut player, &Usage::uniform(), rng);
        }
        // Credit a plausible pre-career playing history for his age, so scouting
        // starts him at a realistic confidence (a vet known, a rookie unknown).
        player.appearances = (target_age - 18) as u32 * PRESUMED_GAMES_PER_YEAR;
        player
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic_and_in_range() {
        let a = Player::generate(PlayerId(0), "A", &mut Rng::new(7));
        let b = Player::generate(PlayerId(0), "A", &mut Rng::new(7));
        assert_eq!(a, b, "same seed → same player");

        let r = a.ratings;
        for v in [
            r.accuracy,
            r.range,
            r.handling,
            r.stripping,
            r.contesting,
            r.passing,
            r.positioning,
            r.pace,
            r.awareness,
            r.endurance,
        ] {
            assert!(v <= 100, "rating {v} out of range");
        }
        assert!((18..=32).contains(&a.age));
    }

    #[test]
    fn from_canonical_maps_each_attribute_to_its_field() {
        // Encode each attribute's canonical index as its value, then assert the
        // named field picked up the value at that index.
        let a: [u8; 10] = std::array::from_fn(|i| i as u8);
        let r = Ratings::from_canonical(a);
        assert_eq!(r.accuracy, Attribute::Accuracy as u8);
        assert_eq!(r.awareness, Attribute::Awareness as u8);
        assert_eq!(r.positioning, Attribute::Positioning as u8);
        assert_eq!(r.endurance, Attribute::Endurance as u8);
    }
}
