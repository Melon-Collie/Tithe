//! Scouting & fog: a fuzzy *view* over a player's true ratings. The truth lives
//! on the [`Player`] (`ratings`, `potential`, `development_risk`); this module
//! never changes it — the sim always runs on true values — it only reports what a
//! scout can currently tell.
//!
//! The law (design doc §10): **fog perturbs magnitude, never shape.** A report is
//! a set of honest **bands** — a `[lo, hi]` range that *always contains the true
//! value* (the game never gaslights you, §6); what's uncertain is where in the
//! band he really sits. You bet on degree, not kind.
//!
//! Two dials (§3):
//! - **Exposure (epistemic):** a player's [`appearances`](Player::appearances)
//!   tighten his bands — watch him enough and you know what he can do now. This
//!   dial closes all the way.
//! - **Development risk (aleatoric):** even fully watched, a volatile player's
//!   *ceiling* can't be pinned, so the ceiling band keeps an irreducible floor
//!   set by his [`development_risk`](Player::development_risk).

use crate::player::Player;
use tithe_sim::{Attribute, Rng};

/// A salt so band offsets don't share a stream with other career-seeded draws.
const SCOUT_SEED_SALT: u64 = 0x5C00_7F06_5C00_7F06;

/// A scouted estimate of one rating: an honest range that contains the true
/// value. A narrower band means a more confident read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Band {
    pub lo: u8,
    pub hi: u8,
}

impl Band {
    /// The middle of the band — the single best-guess number.
    pub fn mid(&self) -> u8 {
        ((self.lo as u16 + self.hi as u16) / 2) as u8
    }

    /// How wide the band is (`hi - lo`) — the visible uncertainty.
    pub fn width(&self) -> u8 {
        self.hi - self.lo
    }
}

/// A full scouting report on a player: how well he's known, his (scoutable)
/// volatility, and per-attribute bands for what he can do now and where he might
/// peak. Bands are indexed by [`Attribute`]; use [`current`](ScoutReport::current)
/// and [`ceiling`](ScoutReport::ceiling).
#[derive(Debug, Clone)]
pub struct ScoutReport {
    /// How well scouted, `0..=100` (from exposure). Higher = tighter bands.
    pub observation: u8,
    /// His scouted volatility (`0..=100`) — the "motor flag": a high value means
    /// even a well-scouted ceiling stays a wide bet (boom or bust).
    pub volatility: u8,
    current: [Band; 10],
    ceiling: [Band; 10],
}

impl ScoutReport {
    /// The band for what he can do *now* in `attr`.
    pub fn current(&self, attr: Attribute) -> Band {
        self.current[attr as usize]
    }

    /// The band for where `attr` might *peak* (his ceiling).
    pub fn ceiling(&self, attr: Attribute) -> Band {
        self.ceiling[attr as usize]
    }
}

/// The tunable dials of scouting fog. Defaults are an initial tuning guess, not
/// commitments (kept in config, not baked into the logic).
#[derive(Debug, Clone)]
pub struct ScoutConfig {
    /// Half-width of a *current* band for a completely unseen player. Shrinks to 0
    /// as observation reaches 100.
    pub current_fog: u32,
    /// Epistemic half-width of a *ceiling* band for an unseen player (shrinks with
    /// observation, like current).
    pub ceiling_fog: u32,
    /// Irreducible ceiling half-width at maximum development risk — the aleatoric
    /// floor that exposure can *never* close.
    pub ceiling_aleatoric: u32,
    /// Appearances at which observation reaches 50% (an asymptotic curve, so a
    /// sliver of fog always remains — scouting is never perfect).
    pub observation_half: u32,
}

impl Default for ScoutConfig {
    fn default() -> Self {
        ScoutConfig {
            current_fog: 18,
            ceiling_fog: 14,
            ceiling_aleatoric: 16,
            observation_half: 15,
        }
    }
}

impl ScoutConfig {
    /// How well a player with `appearances` matches is known, `0..=100`. Asymptotic
    /// (`appearances / (appearances + half)`), so confidence rises fast early then
    /// levels off just short of certainty.
    pub fn observation(&self, appearances: u32) -> u8 {
        ((100 * appearances) / (appearances + self.observation_half)) as u8
    }

    /// Build the report for a player. `seed` (the career seed) fixes the bands so
    /// they're stable across looks and converge toward the truth as observation
    /// rises. Pure: reads the player's truth, changes nothing.
    pub fn scout(&self, player: &Player, seed: u64) -> ScoutReport {
        let obs = self.observation(player.appearances);
        let epistemic = |fog: u32| fog * (100 - obs as u32) / 100;

        let current = Attribute::ALL.map(|a| {
            let half = epistemic(self.current_fog);
            band(
                player.ratings.get(a),
                half,
                seed,
                player,
                a,
                BandKind::Current,
            )
        });
        let ceiling = Attribute::ALL.map(|a| {
            // Epistemic (closes with exposure) plus the aleatoric floor (never does).
            let half = epistemic(self.ceiling_fog)
                + self.ceiling_aleatoric * player.development_risk as u32 / 100;
            band(
                player.potential.get(a),
                half,
                seed,
                player,
                a,
                BandKind::Ceiling,
            )
        });

        ScoutReport {
            observation: obs,
            volatility: player.development_risk,
            current,
            ceiling,
        }
    }
}

/// Which band a draw is for, so current and ceiling get independent offsets.
#[derive(Clone, Copy)]
enum BandKind {
    Current = 0,
    Ceiling = 1,
}

/// One honest band: a `[lo, hi]` of half-width `half` around a center that sits a
/// fixed (seeded) fraction off the true value — so the true value is always
/// inside, you don't know exactly where, and as `half` shrinks the band converges
/// on the truth.
fn band(truth: u8, half: u32, seed: u64, player: &Player, attr: Attribute, kind: BandKind) -> Band {
    if half == 0 {
        return Band {
            lo: truth,
            hi: truth,
        };
    }
    // A stable fraction in [-50, 50]% of the half-width: offsets the center off
    // the truth by at most half the half-width, keeping the truth comfortably in.
    let key = (player.id.0 as u64) << 8 | (attr as u64) << 1 | kind as u64;
    let frac = (Rng::new(seed ^ SCOUT_SEED_SALT ^ key).next_u64() % 101) as i64 - 50;
    let offset = frac * half as i64 / 100;
    let center = (truth as i64 + offset).clamp(0, 100);
    Band {
        lo: (center - half as i64).clamp(0, 100) as u8,
        hi: (center + half as i64).clamp(0, 100) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::development::Usage;
    use crate::player::{Player, PlayerId, Ratings};

    fn probe(appearances: u32, risk: u8) -> Player {
        Player {
            id: PlayerId(3),
            name: "Probe".into(),
            age: 22,
            ratings: Ratings::from_canonical([60; 10]),
            potential: Ratings::from_canonical([80; 10]),
            development_risk: risk,
            season_usage: Usage::default(),
            appearances,
        }
    }

    #[test]
    fn bands_always_contain_the_truth() {
        let cfg = ScoutConfig::default();
        // Across exposure levels and both dials, the true value stays inside.
        for app in [0, 5, 30, 200] {
            let p = probe(app, 80);
            let report = cfg.scout(&p, 42);
            for a in Attribute::ALL {
                let c = report.current(a);
                assert!(
                    c.lo <= 60 && 60 <= c.hi,
                    "current {a:?} excludes truth: {c:?}"
                );
                let k = report.ceiling(a);
                assert!(
                    k.lo <= 80 && 80 <= k.hi,
                    "ceiling {a:?} excludes truth: {k:?}"
                );
            }
        }
    }

    #[test]
    fn exposure_tightens_the_current_band() {
        let cfg = ScoutConfig::default();
        let unseen = cfg.scout(&probe(0, 50), 1);
        let watched = cfg.scout(&probe(200, 50), 1);
        assert!(
            watched.current(Attribute::Accuracy).width()
                < unseen.current(Attribute::Accuracy).width(),
            "more appearances should narrow the current band"
        );
        assert!(watched.observation > unseen.observation);
    }

    #[test]
    fn risk_keeps_the_ceiling_band_open_even_when_watched() {
        let cfg = ScoutConfig::default();
        // Both fully watched; only development risk differs.
        let steady = cfg.scout(&probe(500, 0), 1);
        let volatile = cfg.scout(&probe(500, 100), 1);
        // The steady player's ceiling is essentially pinned; the volatile one's
        // stays a wide bet — the irreducible aleatoric dial.
        assert!(steady.ceiling(Attribute::Accuracy).width() < 3);
        assert!(
            volatile.ceiling(Attribute::Accuracy).width()
                > steady.ceiling(Attribute::Accuracy).width(),
            "risk should keep the ceiling band open"
        );
    }

    #[test]
    fn a_report_is_stable_for_the_same_inputs() {
        let cfg = ScoutConfig::default();
        let p = probe(20, 40);
        let a = cfg.scout(&p, 7);
        let b = cfg.scout(&p, 7);
        for attr in Attribute::ALL {
            assert_eq!(a.current(attr), b.current(attr));
            assert_eq!(a.ceiling(attr), b.ceiling(attr));
        }
    }
}
