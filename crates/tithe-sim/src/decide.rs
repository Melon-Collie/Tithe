//! The agents' decision model — a small utility scorer.
//!
//! This is the seam for §2's "one simple shared utility algorithm": every
//! agent scores the same candidate intents against its perceived world and
//! picks the best. Intelligence is *input quality, not compute* — a genius and
//! a rookie run this identical scorer; later, attribute-modulated perception
//! noise on the inputs is what separates them.
//!
//! The shape is borrowed from Dave Mark's **Infinite Axis Utility System**:
//! each intent's score is a product of considerations in `[0, 1]`. Slice 2 has
//! only two intents and one real consideration (proximity); more axes and the
//! IAUS compensation factor arrive as behavior grows.
//!
//! Determinism: scores are fixed-point and compared with a total order; ties
//! resolve toward the earlier intent, so the choice is reproducible.

use crate::fx::{Fx, Vec2};
use crate::world::{Agent, Formation, SimConfig, Soul};

/// What an agent has decided to do for the current decision window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// Collapse on a loose soul.
    ChaseSoul,
    /// Hold the coach's formation anchor.
    HoldAnchor,
}

/// Score the intents and return the winner. `ChaseSoul` must *beat*
/// `HoldAnchor` to be chosen, so a tie holds shape.
pub fn choose_intent(agent: &Agent, soul: &Soul, config: &SimConfig) -> Intent {
    if score_chase(agent, soul, config) > score_hold(soul, config) {
        Intent::ChaseSoul
    } else {
        Intent::HoldAnchor
    }
}

/// Map a chosen intent to the point the agent should move toward.
pub fn target_for(intent: Intent, agent: &Agent, soul: &Soul, formation: &Formation) -> Vec2 {
    match intent {
        Intent::ChaseSoul => soul.pos,
        Intent::HoldAnchor => formation.anchors[agent.anchor as usize],
    }
}

/// Urge to chase: zero unless the soul is loose, otherwise a proximity
/// consideration that is 1 on the soul and falls to 0 at `chase_max_dist`.
fn score_chase(agent: &Agent, soul: &Soul, config: &SimConfig) -> Fx {
    if !soul.is_loose() {
        return Fx::from_num(0);
    }
    let dist = agent.pos.distance_to(soul.pos);
    if dist >= config.chase_max_dist {
        return Fx::from_num(0);
    }
    Fx::from_num(1) - dist / config.chase_max_dist
}

/// Urge to hold the anchor: full while someone owns the soul, a small baseline
/// while it's loose (so only agents with a stronger chase urge break shape).
fn score_hold(soul: &Soul, config: &SimConfig) -> Fx {
    if soul.is_loose() {
        config.hold_base
    } else {
        Fx::from_num(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Possession;

    fn agent_at(x: i32, y: i32) -> Agent {
        Agent {
            pos: Vec2::new(Fx::from_num(x), Fx::from_num(y)),
            target: Vec2::default(),
            anchor: 0,
        }
    }

    #[test]
    fn chases_a_nearby_loose_soul() {
        let config = SimConfig::default();
        let soul = Soul::loose_at(Vec2::default());
        assert_eq!(
            choose_intent(&agent_at(3, 0), &soul, &config),
            Intent::ChaseSoul
        );
    }

    #[test]
    fn holds_anchor_when_soul_is_possessed() {
        let config = SimConfig::default();
        let soul = Soul {
            pos: Vec2::default(),
            possession: Possession::Held(5),
        };
        assert_eq!(
            choose_intent(&agent_at(3, 0), &soul, &config),
            Intent::HoldAnchor
        );
    }

    #[test]
    fn holds_anchor_when_soul_is_far() {
        let config = SimConfig::default();
        let soul = Soul::loose_at(Vec2::default());
        // Beyond chase_max_dist (120): chase urge is 0, below the hold baseline.
        assert_eq!(
            choose_intent(&agent_at(200, 0), &soul, &config),
            Intent::HoldAnchor
        );
    }
}
