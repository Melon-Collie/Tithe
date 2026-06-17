//! The player development model: how a [`Player`]'s ratings move over a career —
//! growing toward his ceiling when young, plateauing at a peak age, then
//! declining **shape-first** (athletic attributes erode before craft), all
//! perturbed by his own [`development_risk`](Player::development_risk).
//!
//! This is the substrate the design doc's §6 development rests on: improvement is
//! capped at potential, and aging is a shared career curve with individual
//! variation. *Usage*-driven development (which attributes a player improves based
//! on how he's deployed) layers on top of this in a later slice — here every
//! attribute grows toward its ceiling uniformly.
//!
//! The dials live in [`DevelopmentModel`] (not baked into the logic), defaulted
//! today; a league could carry its own later. All progression is integer math and
//! takes its randomness from a threaded [`Rng`], so a career is reproducible.

use crate::player::{Player, Ratings};
use serde::{Deserialize, Serialize};
use tithe_sim::{Attribute, PlayerLine, Rng};

/// How much each attribute was *exercised* over a season — the deployment record
/// that biases development (design doc §6: "players improve at what they do, up to
/// their cap"). Accumulated from match box scores via [`add_line`](Usage::add_line)
/// and consumed when the season is advanced. Counts are in [`Attribute`] order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    counts: [u32; 10],
}

impl Usage {
    /// Equal exercise of every attribute — the "no deployment signal" baseline
    /// that makes [`advance`](DevelopmentModel::advance) grow uniformly toward the
    /// ceiling. Used when aging a generated player up, where there is no match
    /// history to bias by.
    pub fn uniform() -> Self {
        Usage { counts: [1; 10] }
    }

    /// Fold one match's box-score line into the running totals. The mapping ties
    /// each attribute to the actions the sim uses it for — a first cut, tunable:
    /// shooting reps exercise accuracy and range, possessions handling, passes
    /// passing, strips stripping, picked passes contesting, recoveries
    /// positioning, athletic actions pace, and general involvement awareness.
    pub fn add_line(&mut self, line: &PlayerLine) {
        let possessions = line.offerings + line.passes;
        let athletic = line.recoveries + line.strips_attempted;
        let involvement =
            possessions + line.strips_attempted + line.interceptions + line.recoveries;
        self.counts[Attribute::Accuracy as usize] += line.offerings;
        self.counts[Attribute::Range as usize] += line.offerings;
        self.counts[Attribute::Handling as usize] += possessions;
        self.counts[Attribute::Passing as usize] += line.passes;
        self.counts[Attribute::Stripping as usize] += line.strips_attempted;
        self.counts[Attribute::Contesting as usize] += line.interceptions;
        self.counts[Attribute::Positioning as usize] += line.recoveries;
        self.counts[Attribute::Pace as usize] += athletic;
        self.counts[Attribute::Awareness as usize] += involvement;
    }

    fn get(&self, a: Attribute) -> u32 {
        self.counts[a as usize]
    }

    fn max(&self) -> u32 {
        self.counts.iter().copied().max().unwrap_or(0)
    }
}

/// The tunable dials of the development curve. Defaults are an initial tuning
/// guess (design doc: get the mechanic working, then tune), not commitments.
#[derive(Debug, Clone)]
pub struct DevelopmentModel {
    /// The age a player stops growing and begins to decline. Before it he closes
    /// the gap to his ceiling; after it he erodes.
    pub peak_age: u8,
    /// Percent of the remaining gap to the ceiling a young player closes per year.
    /// Applying it to the *gap* means growth is fast when raw and slows as he
    /// nears his ceiling — an asymptotic approach without an extra age term.
    pub growth_rate: u32,
    /// Base decline scalar; the yearly loss is `(decline_rate + years_past_peak) ×
    /// attribute_fragility / 100`, so decline accelerates with age and bites
    /// athletic attributes hardest (see [`fragility`](DevelopmentModel::fragility)).
    pub decline_rate: u32,
    /// An 18-year-old's current rating as a percent of his ceiling — how raw a
    /// fresh prospect starts before he develops.
    pub youth_fraction: u32,
    /// How much a player's `development_risk` may swing a year's change, in
    /// percent at max risk. At risk 100 a year's delta varies by up to
    /// `±risk_swing%`; at risk 0 progression is exactly the projection.
    pub risk_swing: u32,
    /// Growth an *unused* attribute still gets, as a percent of the full
    /// age-curve growth (light practice). The most-used attribute grows at 100%;
    /// the rest scale between this floor and 100% by their share of usage.
    pub usage_floor: u32,
}

impl Default for DevelopmentModel {
    fn default() -> Self {
        DevelopmentModel {
            peak_age: 27,
            growth_rate: 25,
            decline_rate: 2,
            youth_fraction: 65,
            risk_swing: 60,
            usage_floor: 30,
        }
    }
}

impl DevelopmentModel {
    /// A fresh 18-year-old's current ratings: each attribute at `youth_fraction`
    /// of its ceiling. The raw starting point generation ages forward from.
    pub fn youth_ratings(&self, potential: &Ratings) -> Ratings {
        let scaled = Attribute::ALL.map(|a| {
            let ceiling = potential.get(a) as u32;
            (ceiling * self.youth_fraction / 100) as u8
        });
        Ratings::from_canonical(scaled)
    }

    /// How fast an attribute decays in decline, `0..=100` (higher = erodes
    /// sooner). Decline is **shape-first** (design doc §6): speed-dependent
    /// athletic traits go before craft, so a fading burner loses pace and his
    /// checking long before a cerebral playmaker loses passing or awareness.
    pub fn fragility(attr: Attribute) -> u32 {
        match attr {
            Attribute::Pace => 100,       // the first thing to go
            Attribute::Endurance => 85,   // the engine fades early too (conditioning)
            Attribute::Stripping => 70,   // athletic defending
            Attribute::Contesting => 70,  // athletic defending
            Attribute::Handling => 50,    // half touch, half athleticism
            Attribute::Range => 30,       // mostly a learned stroke
            Attribute::Accuracy => 20,    // craft
            Attribute::Passing => 20,     // craft
            Attribute::Positioning => 15, // reading the game
            Attribute::Awareness => 10,   // the genius layer — ages best
        }
    }

    /// Advance a player one development year: grow toward his ceiling if young,
    /// erode shape-first if past peak, then add a year of age. `usage` biases
    /// *growth* toward the attributes he actually exercised (pass [`Usage::uniform`]
    /// for no signal); decline is physical and isn't trained away, so it ignores
    /// usage. Randomness (the risk swing) is drawn from `rng`, so the result is
    /// deterministic given it.
    pub fn advance(&self, player: &mut Player, usage: &Usage, rng: &mut Rng) {
        let max_usage = usage.max();
        let next = Attribute::ALL.map(|a| {
            let current = player.ratings.get(a) as i32;
            let ceiling = player.potential.get(a) as i32;
            let base = self.projected_delta(a, current, ceiling, player.age);
            let projected = if base > 0 {
                base * self.usage_multiplier(usage, max_usage, a) as i32 / 100
            } else {
                base // decline: usage doesn't apply
            };
            let delta = apply_risk(projected, player.development_risk, self.risk_swing, rng);
            (current + delta).clamp(0, ceiling.max(current)) as u8
        });
        player.ratings = Ratings::from_canonical(next);
        player.age = player.age.saturating_add(1);
    }

    /// Growth multiplier (percent) for an attribute from its share of the season's
    /// usage: the most-used attribute grows at 100%, unused ones at `usage_floor`,
    /// the rest scaled between. With uniform usage every attribute is at the max,
    /// so all grow fully — the no-signal baseline.
    fn usage_multiplier(&self, usage: &Usage, max_usage: u32, attr: Attribute) -> u32 {
        // No usage signal (max 0 → checked_div None) leaves only the floor.
        let scaled = ((100 - self.usage_floor) * usage.get(attr))
            .checked_div(max_usage)
            .unwrap_or(0);
        self.usage_floor + scaled
    }

    /// The projected (pre-risk) one-year change for an attribute: close a fraction
    /// of the gap while young, lose a fragility-weighted amount once past peak, and
    /// hold steady at exactly the peak age (a plateau year).
    fn projected_delta(&self, attr: Attribute, current: i32, ceiling: i32, age: u8) -> i32 {
        if age < self.peak_age {
            let gap = (ceiling - current).max(0);
            (gap * self.growth_rate as i32) / 100
        } else if age > self.peak_age {
            let years_past = (age - self.peak_age) as i32;
            let loss = (self.decline_rate as i32 + years_past) * Self::fragility(attr) as i32 / 100;
            -loss
        } else {
            0 // exactly the peak age — a plateau year
        }
    }
}

/// Perturb a projected change by a player's developmental volatility. The swing is
/// proportional to `risk`, so a high-risk prospect's growth (or a veteran's
/// decline) varies year to year while a steady player tracks the projection. Risk
/// 0 returns the projection unchanged.
fn apply_risk(delta: i32, risk: u8, risk_swing: u32, rng: &mut Rng) -> i32 {
    if delta == 0 {
        return 0;
    }
    let max_pct = (risk as u32 * risk_swing / 100) as i32;
    if max_pct == 0 {
        return delta;
    }
    // A signed swing in [-max_pct, max_pct] percent, applied to the delta.
    let roll = rng.below((2 * max_pct + 1) as u64) as i32 - max_pct;
    delta + delta * roll / 100
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{Player, PlayerId};

    /// A player with a ceiling above his current, well short of peak age, grows
    /// toward that ceiling — and never past it.
    #[test]
    fn the_young_grow_toward_their_ceiling() {
        let model = DevelopmentModel::default();
        let potential = Ratings::from_canonical([80; 10]);
        let mut p = Player {
            id: PlayerId(0),
            name: "Prospect".into(),
            age: 18,
            ratings: model.youth_ratings(&potential), // ~52
            potential: potential.clone(),
            development_risk: 0, // no noise: track the projection exactly
            season_usage: Usage::default(),
            appearances: 0,
            contract: None,
            stamina: 100,
        };
        let start = p.ratings.overall();
        for _ in 0..6 {
            model.advance(&mut p, &Usage::uniform(), &mut Rng::new(1));
        }
        assert!(p.ratings.overall() > start, "should have grown");
        // Never exceeds the ceiling.
        for a in Attribute::ALL {
            assert!(p.ratings.get(a) <= potential.get(a));
        }
    }

    /// Past peak age, ratings decline — and pace (fragile) falls faster than
    /// awareness (craft): decline is shape-first.
    #[test]
    fn the_old_decline_shape_first() {
        let model = DevelopmentModel::default();
        let level = Ratings::from_canonical([80; 10]);
        let mut p = Player {
            id: PlayerId(0),
            name: "Veteran".into(),
            age: 30,
            ratings: level.clone(),
            potential: level,
            development_risk: 0,
            season_usage: Usage::default(),
            appearances: 0,
            contract: None,
            stamina: 100,
        };
        for _ in 0..5 {
            model.advance(&mut p, &Usage::uniform(), &mut Rng::new(1));
        }
        let pace_lost = 80 - p.ratings.pace as i32;
        let awareness_lost = 80 - p.ratings.awareness as i32;
        assert!(pace_lost > 0, "pace should decline");
        assert!(
            pace_lost > awareness_lost,
            "pace ({pace_lost}) should erode faster than awareness ({awareness_lost})"
        );
    }

    /// At exactly peak age a player neither grows nor declines (a plateau year).
    #[test]
    fn the_peak_is_a_plateau() {
        let model = DevelopmentModel::default();
        let level = Ratings::from_canonical([70; 10]);
        let mut p = Player {
            id: PlayerId(0),
            name: "Peak".into(),
            age: model.peak_age,
            ratings: level.clone(),
            potential: Ratings::from_canonical([90; 10]), // headroom, but no growth at peak
            development_risk: 100,
            season_usage: Usage::default(),
            appearances: 0,
            contract: None,
            stamina: 100,
        };
        model.advance(&mut p, &Usage::uniform(), &mut Rng::new(42));
        assert_eq!(p.ratings, level, "no change at exactly the peak age");
    }

    /// Risk only adds variance: a risk-0 player's progression is exactly the
    /// projection regardless of the rng stream.
    #[test]
    fn zero_risk_is_deterministic_projection() {
        let model = DevelopmentModel::default();
        let potential = Ratings::from_canonical([75; 10]);
        let make = || Player {
            id: PlayerId(0),
            name: "Steady".into(),
            age: 20,
            ratings: model.youth_ratings(&potential),
            potential: potential.clone(),
            development_risk: 0,
            season_usage: Usage::default(),
            appearances: 0,
            contract: None,
            stamina: 100,
        };
        let mut a = make();
        let mut b = make();
        model.advance(&mut a, &Usage::uniform(), &mut Rng::new(1));
        model.advance(&mut b, &Usage::uniform(), &mut Rng::new(999)); // different stream, same result
        assert_eq!(a.ratings, b.ratings);
    }

    /// Usage steers growth: a young player who only takes offerings grows his
    /// shooting (accuracy) toward its ceiling, while an attribute he never uses
    /// (passing) only creeps up at the practice floor.
    #[test]
    fn usage_biases_growth_toward_what_is_used() {
        let model = DevelopmentModel::default();
        let potential = Ratings::from_canonical([90; 10]);
        let make = || Player {
            id: PlayerId(0),
            name: "Shooter".into(),
            age: 19,
            ratings: model.youth_ratings(&potential),
            potential: potential.clone(),
            development_risk: 0, // isolate the usage effect from noise
            season_usage: Usage::default(),
            appearances: 0,
            contract: None,
            stamina: 100,
        };

        // A season of nothing but offerings (exercises shooting, not passing).
        let mut shooting = Usage::default();
        shooting.add_line(&PlayerLine {
            offerings: 20,
            ..PlayerLine::default()
        });

        let mut shooter = make();
        let mut idle = make();
        for _ in 0..3 {
            model.advance(&mut shooter, &shooting, &mut Rng::new(1));
            model.advance(&mut idle, &Usage::default(), &mut Rng::new(1));
        }
        // The used attribute outgrew the unused one, and outgrew the idle player's
        // floor-only growth of the same attribute.
        assert!(shooter.ratings.accuracy > shooter.ratings.passing);
        assert!(shooter.ratings.accuracy > idle.ratings.accuracy);
    }
}
