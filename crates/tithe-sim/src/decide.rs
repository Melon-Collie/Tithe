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
use crate::hex::Board;
use crate::value;
use crate::world::{Agent, OutOfPossessionRole, SimConfig, Soul};

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
        // hold their defensive shape rather than swarming the ball. How far the
        // defender will break shape to engage scales with its role (a Presser
        // hounds from distance, an Anchor only when the carrier is close).
        Some(c) if c.team != agent.team => {
            let range =
                config.contest_range * config.defense_bias(agent.defend_role).contest_range_mult;
            if is_team_nearest && agent.pos.distance_to(c.pos) <= range {
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

/// Where an off-ball agent should shade — the best **hex** within its footprint
/// (§14): the board cells whose centers lie within the role's drift of the
/// (noised) anchor. The off-ball decision now lives on the hex grid.
///
/// - **Attacking** (my team has the soul): be a great pass target — maximize
///   `value_at × lane_clear(from the carrier)`. Open, advanced, and reachable.
/// - **Defending / loose:** cover the threat — the hex that shadows the carrier's
///   most dangerous lane. Getting into lanes falls out of denying that value.
///
/// A pointwise *perception* of where to be, not lookahead or search (§2). Phase
/// 2a: the footprint is the drift disc discretized to hexes; role-specific
/// shapes (the locked vocabulary) replace the disc in 2b.
pub fn off_ball_target(
    agent: &Agent,
    carrier: Option<CarrierInfo>,
    goals: [Vec2; 2],
    allies: &[Vec2],
    enemies: &[Vec2],
    board: &Board,
    config: &SimConfig,
) -> Vec2 {
    // Loose soul: hold the anchor (the nearest agent chases it; everyone holds).
    let Some(carrier) = carrier else {
        return agent.anchor;
    };
    let attacking = carrier.team == agent.team;
    let my_goal = goals[agent.team as usize];
    let enemy_goal = goals[1 - agent.team as usize];

    // The role's footprint — an ellipse (size × aspect + a forward lean) placed
    // at the (noised) anchor. Attacking uses the in-possession role's shape;
    // defending uses the out-of-possession role's.
    let footprint = if attacking {
        config.on_ball_bias(agent.attack_role).footprint
    } else {
        config.defense_bias(agent.defend_role).footprint
    };
    let cheat = !attacking && agent.defend_role == OutOfPossessionRole::Cheat;
    // The lean shifts the footprint toward the enemy goal (the Presser's push).
    let fwd = if enemy_goal.x >= Fx::from_num(0) {
        Fx::from_num(1)
    } else {
        Fx::from_num(-1)
    };
    let center_x = agent.anchor.x + footprint.lean * fwd;
    let center_y = agent.anchor.y;
    let one = Fx::from_num(1);

    let mut best = agent.anchor;
    let mut best_score = Fx::from_num(-1);
    for &hex in board.cells() {
        let candidate = board.center(hex);
        // Inside the footprint ellipse?
        let nx = (candidate.x - center_x) / footprint.half_x;
        let ny = (candidate.y - center_y) / footprint.half_y;
        if nx * nx + ny * ny > one {
            continue;
        }
        let score = if attacking {
            // Be a great pass target: open, advanced, reachable from the carrier.
            let reachable = value::lane_clear(carrier.pos, candidate, enemies, config);
            value::value_at(candidate, my_goal, enemies, config) * reachable
        } else if cheat {
            // Cheat: position by *offensive* value — an advanced, open counter
            // outlet toward my own goal, ignoring the enemy carrier.
            value::value_at(candidate, my_goal, enemies, config)
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
        let board = Board::oval(cfg.hex_size, cfg.arena_half_x, cfg.arena_half_y);
        let target = off_ball_target(&me, carrier, goals, &[], &enemies, &board, &cfg);
        // I should not stay on the crowded anchor...
        assert_ne!(target, me.anchor);
        // ...and the chosen hex stays within the role's footprint ellipse.
        let fp = cfg.on_ball_bias(me.attack_role).footprint;
        let nx = (target.x - me.anchor.x) / fp.half_x;
        let ny = (target.y - me.anchor.y) / fp.half_y;
        assert!(nx * nx + ny * ny <= Fx::from_num(1) + Fx::from_num(1) / Fx::from_num(100));
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
