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
use crate::value;
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

/// Score the situation and return the chosen intent. `is_team_nearest` is true
/// when this agent is the closest on its team to the soul (loose or carried) —
/// only that agent leaves the shape to engage; everyone else holds. This is what
/// keeps the formation intact instead of the whole team chasing the ball.
pub fn choose_intent(
    agent: &Agent,
    soul: &Soul,
    carrier: Option<CarrierInfo>,
    is_team_nearest: bool,
    config: &SimConfig,
) -> Intent {
    match carrier {
        // Loose soul: only the nearest teammate (if it's worth leaving the shape
        // for) contests the draw; everyone else holds.
        None => {
            if is_team_nearest && agent.pos.distance_to(soul.pos) <= config.chase_max_dist {
                Intent::ChaseSoul
            } else {
                Intent::HoldAnchor
            }
        }
        // I'm the carrier.
        Some(c) if c.id == agent.id => Intent::CarryToGoal,
        // An enemy carries it: only our nearest defender closes down; the rest
        // hold their defensive shape rather than swarming the ball.
        Some(c) if c.team != agent.team => {
            if is_team_nearest && agent.pos.distance_to(c.pos) <= config.contest_range {
                Intent::ContestCarrier
            } else {
                Intent::HoldAnchor
            }
        }
        // A teammate carries it.
        Some(_) => Intent::HoldAnchor,
    }
}

/// Map an *active-pursuit* intent to the point the agent should move toward.
/// Off-ball positioning (`HoldAnchor`) is value-driven — see [`off_ball_target`].
pub fn target_for(
    intent: Intent,
    agent: &Agent,
    soul: &Soul,
    carrier: Option<CarrierInfo>,
    goals: [Vec2; 2],
    config: &SimConfig,
) -> Vec2 {
    match intent {
        Intent::ChaseSoul => soul.pos,
        Intent::HoldAnchor => agent.anchor, // static fallback; the sim uses off_ball_target
        // The carrier's straight-line default; the sim overrides it with a
        // pressure-aware carry route in resolve_on_ball.
        Intent::CarryToGoal => goals[agent.team as usize],
        // Contain: sit goal-side of the carrier (between it and its goal), so
        // the straight route into the goal is the one the carrier's EV avoids.
        Intent::ContestCarrier => match carrier {
            Some(c) => {
                let to_goal = goals[c.team as usize] - c.pos;
                c.pos + to_goal.normalized().scale(config.pressure_containment_dist)
            }
            None => agent.anchor,
        },
    }
}

/// Where an off-ball agent should shade, chosen by the value field within a
/// bounded drift of its anchor (the shape breathes, never dissolves):
///
/// - **Attacking** (my team has the soul): be a great pass target — maximize
///   `value_at × lane_clear(from the carrier)`. Open, advanced, and reachable.
/// - **Defending / loose:** cover the threat — go to the spot of highest
///   *enemy* value (close to their goal, currently uncovered by us). Getting
///   into lanes and challenging falls out of denying that value.
///
/// We read the value field at a few stand-positions and pick the best — a
/// pointwise *perception* of where to be, not lookahead or search (§2).
pub fn off_ball_target(
    agent: &Agent,
    carrier: Option<CarrierInfo>,
    goals: [Vec2; 2],
    allies: &[Vec2],
    enemies: &[Vec2],
    config: &SimConfig,
) -> Vec2 {
    // Loose soul: hold the anchor (the nearest agent chases it; everyone holds).
    let Some(carrier) = carrier else {
        return agent.anchor;
    };
    let attacking = carrier.team == agent.team;
    let my_goal = goals[agent.team as usize];
    let enemy_goal = goals[1 - agent.team as usize];

    // The role's drift appetite tightens or loosens how far it shades off its
    // anchor (Stay-at-home hugs his shape; Dangler roams).
    let drift = config.drift_radius * config.on_ball_bias(agent.attack_role).drift_mult;
    let mut best = agent.anchor;
    let mut best_score = Fx::from_num(-1);
    for offset in candidate_offsets(drift) {
        let candidate = agent.anchor + offset;
        let score = if attacking {
            // Be a great pass target: open, advanced, reachable from the carrier.
            let reachable = value::lane_clear(carrier.pos, candidate, enemies, config);
            value::value_at(candidate, my_goal, enemies, config) * reachable
        } else {
            // Cover-shadow the carrier's most dangerous lane: maximize the threat
            // removed = the best (shadow × receiver xT) over enemy receivers.
            let mut removed = Fx::from_num(0);
            for &receiver in enemies {
                if receiver == carrier.pos {
                    continue; // the carrier itself, not a receiver
                }
                let shadow =
                    value::segment_shadow(candidate, carrier.pos, receiver, config.lane_radius);
                if shadow <= Fx::from_num(0) {
                    continue;
                }
                let threat = value::value_at(receiver, enemy_goal, allies, config);
                let removed_here = shadow * threat;
                if removed_here > removed {
                    removed = removed_here;
                }
            }
            removed
        };
        if score > best_score {
            best_score = score;
            best = candidate;
        }
    }
    best
}

/// The bounded set of stand-positions an off-ball agent considers: its anchor
/// plus eight compass offsets at the drift radius.
fn candidate_offsets(drift: Fx) -> [Vec2; 9] {
    let zero = Fx::from_num(0);
    let diag = drift * Fx::from_num(7) / Fx::from_num(10); // ~0.7·drift, so the diagonal ≈ drift
    [
        Vec2::new(zero, zero),
        Vec2::new(drift, zero),
        Vec2::new(-drift, zero),
        Vec2::new(zero, drift),
        Vec2::new(zero, -drift),
        Vec2::new(diag, diag),
        Vec2::new(diag, -diag),
        Vec2::new(-diag, diag),
        Vec2::new(-diag, -diag),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Attributes, InPossessionRole, OutOfPossessionRole, Possession};

    fn agent(id: u32, team: u8, x: i32, y: i32) -> Agent {
        let anchor = Vec2::new(Fx::from_num(99), Fx::from_num(0));
        Agent {
            id,
            name: format!("P{id}"),
            team,
            attack_role: InPossessionRole::default(),
            defend_role: OutOfPossessionRole::default(),
            pos: Vec2::new(Fx::from_num(x), Fx::from_num(y)),
            target: Vec2::default(),
            anchor,
            attack_anchor: anchor,
            defend_anchor: anchor,
            stagger: 0,
            stamina: Fx::from_num(1),
            attributes: Attributes::uniform(),
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
            choose_intent(&agent(0, 0, 3, 0), &soul, None, true, &cfg),
            Intent::ChaseSoul
        );
    }

    #[test]
    fn carrier_advances_toward_its_own_goal() {
        let cfg = SimConfig::default();
        let me = agent(2, 0, 5, 5);
        let (soul, carrier) = held_by(2, 0, me.pos);
        assert_eq!(
            choose_intent(&me, &soul, carrier, false, &cfg),
            Intent::CarryToGoal
        );
        let goals = cfg.goals();
        assert_eq!(
            target_for(Intent::CarryToGoal, &me, &soul, carrier, goals, &cfg),
            goals[0]
        );
    }

    #[test]
    fn off_ball_attacker_shades_toward_a_better_pass_target() {
        let cfg = SimConfig::default();
        let goals = cfg.goals(); // team 0 goal at (-45, 0)
        let mut me = agent(1, 0, 0, 0);
        me.anchor = Vec2::new(Fx::from_num(0), Fx::from_num(0)); // within the value span
                                                                 // My team has the soul; an enemy sits right on my anchor crowding it.
        let enemies = [me.anchor];
        let carrier = Some(CarrierInfo {
            id: 2,
            team: 0,
            pos: Vec2::new(Fx::from_num(80), Fx::from_num(0)),
        });
        let target = off_ball_target(&me, carrier, goals, &[], &enemies, &cfg);
        // I should not stay on the crowded anchor...
        assert_ne!(target, me.anchor);
        // ...and the shade stays within the drift budget.
        assert!(
            (target - me.anchor).length() <= cfg.drift_radius + Fx::from_num(1) / Fx::from_num(100)
        );
    }

    #[test]
    fn defender_contests_a_nearby_enemy_carrier() {
        let cfg = SimConfig::default();
        let me = agent(7, 1, 5, 0);
        let (soul, carrier) = held_by(2, 0, Vec2::new(Fx::from_num(8), Fx::from_num(0)));
        // I'm my team's nearest to the carrier, so I close down.
        assert_eq!(
            choose_intent(&me, &soul, carrier, true, &cfg),
            Intent::ContestCarrier
        );
    }

    #[test]
    fn defender_holds_shape_when_enemy_carrier_is_far() {
        let cfg = SimConfig::default();
        let me = agent(7, 1, 0, 0);
        // Nearest, but 40 units away — beyond contest_range (25), so hold.
        let (soul, carrier) = held_by(2, 0, Vec2::new(Fx::from_num(40), Fx::from_num(0)));
        assert_eq!(
            choose_intent(&me, &soul, carrier, true, &cfg),
            Intent::HoldAnchor
        );
    }

    #[test]
    fn supports_when_a_teammate_carries() {
        let cfg = SimConfig::default();
        let me = agent(1, 0, 5, 0);
        let (soul, carrier) = held_by(2, 0, Vec2::new(Fx::from_num(6), Fx::from_num(0)));
        assert_eq!(
            choose_intent(&me, &soul, carrier, false, &cfg),
            Intent::HoldAnchor
        );
    }
}
