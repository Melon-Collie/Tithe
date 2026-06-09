//! The agents' decision model — a small utility scorer.
//!
//! This is the seam for §2's "one simple shared utility algorithm": every agent
//! scores the same candidate intents against its perceived world and picks the
//! best. Intelligence is *input quality, not compute* — a genius and a rookie
//! run this identical scorer; later, attribute-modulated perception noise on
//! the inputs is what separates them.
//!
//! The shape is borrowed from Dave Mark's **Infinite Axis Utility System**:
//! considerations score in `[0, 1]`. Slice 3's logic is still small (proximity
//! plus possession/team checks); more axes and the IAUS compensation factor
//! arrive as behavior grows.
//!
//! Determinism: scores are fixed-point, compared with a total order, so the
//! choice is reproducible.

use crate::fx::{Fx, Vec2};
use crate::world::{Agent, SimConfig, Soul};

/// A snapshot of whoever currently carries the soul — passed to the scorer so
/// it never has to borrow the whole agent list mid-decision.
#[derive(Debug, Clone, Copy)]
pub struct CarrierInfo {
    pub id: u32,
    pub team: u8,
    pub pos: Vec2,
}

/// What an agent has decided to do for the current decision window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// Collapse on a loose soul.
    ChaseSoul,
    /// Hold the coach's formation anchor.
    HoldAnchor,
    /// I carry the soul — advance it toward my home goal.
    CarryToGoal,
    /// An enemy carries the soul and I'm close — close down and lunge.
    ContestCarrier,
}

/// Score the situation and return the chosen intent.
pub fn choose_intent(
    agent: &Agent,
    soul: &Soul,
    carrier: Option<CarrierInfo>,
    config: &SimConfig,
) -> Intent {
    match carrier {
        // Loose soul: collapse on it if the proximity urge beats holding shape.
        None => {
            if proximity(agent.pos, soul.pos, config.chase_max_dist) > config.hold_base {
                Intent::ChaseSoul
            } else {
                Intent::HoldAnchor
            }
        }
        // I'm the carrier.
        Some(c) if c.id == agent.id => Intent::CarryToGoal,
        // An enemy carries it: close down if near enough, else hold shape.
        Some(c) if c.team != agent.team => {
            if agent.pos.distance_to(c.pos) <= config.contest_range {
                Intent::ContestCarrier
            } else {
                Intent::HoldAnchor
            }
        }
        // A teammate carries it.
        Some(_) => Intent::HoldAnchor,
    }
}

/// Map a chosen intent to the point the agent should move toward.
pub fn target_for(
    intent: Intent,
    agent: &Agent,
    soul: &Soul,
    carrier: Option<CarrierInfo>,
    goals: [Vec2; 2],
) -> Vec2 {
    match intent {
        Intent::ChaseSoul => soul.pos,
        Intent::HoldAnchor => agent.anchor,
        Intent::CarryToGoal => goals[agent.team as usize],
        Intent::ContestCarrier => carrier.map_or(agent.anchor, |c| c.pos),
    }
}

/// Proximity consideration: 1 on the point, falling linearly to 0 at `max_dist`.
fn proximity(from: Vec2, to: Vec2, max_dist: Fx) -> Fx {
    let dist = from.distance_to(to);
    if dist >= max_dist {
        Fx::from_num(0)
    } else {
        Fx::from_num(1) - dist / max_dist
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Possession;

    fn agent(id: u32, team: u8, x: i32, y: i32) -> Agent {
        Agent {
            id,
            team,
            pos: Vec2::new(Fx::from_num(x), Fx::from_num(y)),
            target: Vec2::default(),
            anchor: Vec2::new(Fx::from_num(99), Fx::from_num(0)),
            stagger: 0,
        }
    }

    fn held_by(id: u32, team: u8, pos: Vec2) -> (Soul, Option<CarrierInfo>) {
        (
            Soul {
                pos,
                possession: Possession::Held(id),
            },
            Some(CarrierInfo { id, team, pos }),
        )
    }

    #[test]
    fn chases_a_nearby_loose_soul() {
        let cfg = SimConfig::default();
        let soul = Soul::loose_at(Vec2::default());
        assert_eq!(
            choose_intent(&agent(0, 0, 3, 0), &soul, None, &cfg),
            Intent::ChaseSoul
        );
    }

    #[test]
    fn carrier_advances_toward_its_own_goal() {
        let cfg = SimConfig::default();
        let me = agent(2, 0, 5, 5);
        let (soul, carrier) = held_by(2, 0, me.pos);
        assert_eq!(
            choose_intent(&me, &soul, carrier, &cfg),
            Intent::CarryToGoal
        );
        let goals = cfg.goals();
        assert_eq!(
            target_for(Intent::CarryToGoal, &me, &soul, carrier, goals),
            goals[0]
        );
    }

    #[test]
    fn defender_contests_a_nearby_enemy_carrier() {
        let cfg = SimConfig::default();
        let me = agent(7, 1, 5, 0);
        let (soul, carrier) = held_by(2, 0, Vec2::new(Fx::from_num(8), Fx::from_num(0)));
        assert_eq!(
            choose_intent(&me, &soul, carrier, &cfg),
            Intent::ContestCarrier
        );
    }

    #[test]
    fn defender_holds_shape_when_enemy_carrier_is_far() {
        let cfg = SimConfig::default();
        let me = agent(7, 1, 0, 0);
        // 40 units away, beyond contest_range (25).
        let (soul, carrier) = held_by(2, 0, Vec2::new(Fx::from_num(40), Fx::from_num(0)));
        assert_eq!(choose_intent(&me, &soul, carrier, &cfg), Intent::HoldAnchor);
    }

    #[test]
    fn supports_when_a_teammate_carries() {
        let cfg = SimConfig::default();
        let me = agent(1, 0, 5, 0);
        let (soul, carrier) = held_by(2, 0, Vec2::new(Fx::from_num(6), Fx::from_num(0)));
        assert_eq!(choose_intent(&me, &soul, carrier, &cfg), Intent::HoldAnchor);
    }
}
