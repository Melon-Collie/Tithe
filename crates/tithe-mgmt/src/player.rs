//! Persistent players: a stable identity plus the intrinsic ratings the sim
//! consumes. A [`Player`] is *career* state; the sim's per-match agent is built
//! from it at matchup time (see [`crate::club::Club::to_team_setup`]).

use serde::{Deserialize, Serialize};
use tithe_sim::{Attribute, Rng};

/// A player's stable, career-long identity. Distinct from the match-local agent
/// id the sim mints per match (see the crate docs) — this one persists across
/// matches, transfers, and seasons, and is globally unique within a career.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub u32);

/// A player's intrinsic capabilities — the nine attributes as integer
/// percentiles `0..=100`, the same wire form the sim's [`tithe_sim::PlayerSetup`]
/// authors (80 → the `Fx` `0.80` the sim consumes). Named fields (not a bare
/// `[u8; 9]`) so a save file is self-describing and adding an attribute can't
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
}

impl Ratings {
    /// Build from an array in [`Attribute`] canonical order. The single positional
    /// → named mapping in this crate; it keys off the sim's `Attribute` SSOT, so a
    /// reorder there is caught by `tithe-sim`'s own `attribute_keys_match_fields`
    /// test rather than silently misaligning here.
    pub fn from_canonical(a: [u8; 9]) -> Self {
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
        }
    }

    /// Generate a varied player: a `30..70` baseline with one or two attributes
    /// spiked to `80..100`, so each generated player has a readable identity (a
    /// sniper, a checker, a passer). Deterministic from the threaded [`Rng`].
    /// A placeholder for the real archetype-first generation (design doc §4).
    fn generate(rng: &mut Rng) -> Self {
        let mut a = [0u8; 9];
        for v in a.iter_mut() {
            *v = 30 + rng.below(40) as u8; // 30..70
        }
        for _ in 0..1 + rng.below(2) {
            a[rng.below(9) as usize] = 80 + rng.below(20) as u8; // a spike or two
        }
        Ratings::from_canonical(a)
    }
}

/// A persistent player: stable identity, a name, an age, and intrinsic
/// [`Ratings`]. Roles and formation are *not* here — those are the coach's
/// per-match inputs (see [`crate::club::Tactics`]), not properties of the player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub age: u8,
    pub ratings: Ratings,
}

impl Player {
    /// Generate a player with the given identity and name. Age is drawn in a
    /// plausible `18..=32` window. Deterministic from the threaded [`Rng`].
    pub fn generate(id: PlayerId, name: &str, rng: &mut Rng) -> Self {
        Player {
            id,
            name: name.to_string(),
            age: 18 + rng.below(15) as u8, // 18..=32
            ratings: Ratings::generate(rng),
        }
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
        ] {
            assert!(v <= 100, "rating {v} out of range");
        }
        assert!((18..=32).contains(&a.age));
    }

    #[test]
    fn from_canonical_maps_each_attribute_to_its_field() {
        // Encode each attribute's canonical index as its value, then assert the
        // named field picked up the value at that index.
        let a: [u8; 9] = std::array::from_fn(|i| i as u8);
        let r = Ratings::from_canonical(a);
        assert_eq!(r.accuracy, Attribute::Accuracy as u8);
        assert_eq!(r.awareness, Attribute::Awareness as u8);
        assert_eq!(r.positioning, Attribute::Positioning as u8);
    }
}
