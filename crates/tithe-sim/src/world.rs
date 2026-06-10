//! The world the sim moves: arena/config, the coach's formation, the two teams
//! of agents, the soul, and the kinematics that carry an agent toward its
//! target.
//!
//! Two teams contest the soul from a faceoff; a defender can **strip** an enemy
//! carrier (the §1 challenge — win = clean possession, whiff = a brief
//! stagger), and a carrier advances toward its own home goal to offer it.
//! Coordination lives in the formation, never in the agents.

use crate::fx::{Fx, Vec2};

/// Tunable simulation parameters — dials set by prototype + AI-vs-AI sim, not
/// commitments (design doc §13), hence config rather than baked-in.
#[derive(Debug, Clone)]
pub struct SimConfig {
    /// Ticks between decision-clock boundaries (the slow clock). Agents commit
    /// to an intent for this window; motion runs every tick (the fast clock).
    pub decision_interval: u64,
    /// Maximum distance an agent moves per tick.
    pub max_speed: Fx,
    /// Half-width of the arena (x half-extent).
    pub arena_half_x: Fx,
    /// Half-height of the arena (y half-extent).
    pub arena_half_y: Fx,
    /// How close an agent must get to a loose soul to claim it.
    pub pickup_radius: Fx,
    /// Farthest a player will chase a loose soul (even its team's nearest holds
    /// shape beyond this, rather than abandoning the formation).
    pub chase_max_dist: Fx,
    /// X-coordinate of a home goal (team 0 attacks -goal_x, team 1 +goal_x).
    pub goal_x: Fx,
    /// How close a defender must be to an enemy carrier to lunge for a strip.
    pub strip_radius: Fx,
    /// How close an enemy carrier must be before a defender breaks shape to
    /// close it down (larger than strip_radius — close first, then lunge).
    pub contest_range: Fx,
    /// Percent chance (0..100) a committed strip wins clean possession;
    /// otherwise the defender whiffs and is staggered.
    pub strip_success_pct: u32,
    /// Ticks a whiffed defender is staggered (beaten, can't act).
    pub stagger_ticks: u32,
    /// How close a carrier must get to its own goal to offer (touch-in score).
    pub offering_radius: Fx,
    /// Souls a team must bank to win the match (first-to-X).
    pub souls_to_win: u32,
    /// Speed a passed soul travels in flight (faster than a runner).
    pub pass_speed: Fx,
    /// Maximum distance over which a carrier will attempt a pass.
    pub pass_max_dist: Fx,
    /// How much a receiver's value must beat the carrier's for a pass to fire.
    pub pass_value_margin: Fx,
    /// Distance at which goal-closeness value reaches zero (the value field's span).
    pub value_span: Fx,
    /// Radius within which an enemy contributes to a spot's pressure.
    pub pressure_radius: Fx,
    /// Enemy-count (distance-weighted) that fully smothers a spot's openness.
    pub pressure_max: Fx,
    /// Perpendicular distance within which a defender blocks a pass lane.
    pub lane_radius: Fx,
    /// Distance from the in-flight soul's path within which an enemy picks it off.
    pub intercept_radius: Fx,
    /// Stamina lost per tick just by being on the field (active).
    pub stamina_drain_base: Fx,
    /// Extra stamina lost per unit of distance moved (effort — pressers tire fastest).
    pub stamina_drain_per_unit: Fx,
    /// Speed multiplier at empty stamina (full stamina = 1.0). Gassed = slower.
    pub stamina_speed_floor: Fx,
    /// How far an off-ball agent may shade off its anchor toward the play
    /// (bounded drift / elasticity — the shape breathes but never dissolves).
    pub drift_radius: Fx,
    /// Agents closer than this push apart (kept below strip_radius so it never
    /// blocks a legitimate contest).
    pub separation_radius: Fx,
    /// Maximum separation push applied per tick.
    pub separation_step: Fx,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            decision_interval: 12,
            max_speed: Fx::from_num(2),
            arena_half_x: Fx::from_num(50),
            arena_half_y: Fx::from_num(30),
            pickup_radius: Fx::from_num(2),
            chase_max_dist: Fx::from_num(120),
            goal_x: Fx::from_num(45),
            strip_radius: Fx::from_num(3),
            contest_range: Fx::from_num(25),
            strip_success_pct: 35,
            stagger_ticks: 15,
            offering_radius: Fx::from_num(3),
            souls_to_win: 11,
            pass_speed: Fx::from_num(8),
            pass_max_dist: Fx::from_num(40),
            pass_value_margin: Fx::from_num(5) / Fx::from_num(100), // 0.05
            value_span: Fx::from_num(90),
            pressure_radius: Fx::from_num(12),
            pressure_max: Fx::from_num(2),
            lane_radius: Fx::from_num(4),
            intercept_radius: Fx::from_num(3),
            stamina_drain_base: Fx::from_num(5) / Fx::from_num(10000), // 0.0005
            stamina_drain_per_unit: Fx::from_num(25) / Fx::from_num(10000), // 0.0025
            stamina_speed_floor: Fx::from_num(55) / Fx::from_num(100), // 0.55
            drift_radius: Fx::from_num(10),
            separation_radius: Fx::from_num(5) / Fx::from_num(2), // 2.5 (< strip_radius 3)
            separation_step: Fx::from_num(1),
        }
    }
}

impl SimConfig {
    /// The two home goals, indexed by team: team 0 attacks -x, team 1 +x.
    pub fn goals(&self) -> [Vec2; 2] {
        [
            Vec2::new(-self.goal_x, Fx::from_num(0)),
            Vec2::new(self.goal_x, Fx::from_num(0)),
        ]
    }
}

/// The coach's positioning template: one field-relative anchor per player.
#[derive(Debug, Clone)]
pub struct Formation {
    pub anchors: Vec<Vec2>,
}

impl Formation {
    /// A placeholder ~7-player full-court shape for team 0, **biased toward the
    /// opponent's goal** (+x — the goal team 0 must deny): a deep safety near
    /// its own goal, a spine contesting center, and a forward press up top.
    /// Team 1 mirrors it across x. The exact geometry, team size, and zone
    /// banding are tuning dials (design doc §13), not commitments.
    pub fn default_seven() -> Self {
        let p = |x: i32, y: i32| Vec2::new(Fx::from_num(x), Fx::from_num(y));
        Self {
            anchors: vec![
                p(-30, 0), // deep safety (near own goal)
                p(-5, -15),
                p(-5, 15), // midfield
                p(10, 0),  // spine — contests the center soul
                p(28, -16),
                p(28, 0),
                p(28, 16), // forward press (denying the opponent's goal)
            ],
        }
    }
}

/// A single agent: identity, team, where it is, where it's headed, its home
/// anchor, how many ticks it remains staggered (0 = active), and its stamina
/// (1 = fresh, draining over a soul).
#[derive(Debug, Clone)]
pub struct Agent {
    pub id: u32,
    pub team: u8,
    pub pos: Vec2,
    pub target: Vec2,
    pub anchor: Vec2,
    pub stagger: u32,
    pub stamina: Fx,
}

impl Agent {
    /// Whether the agent can move and act this tick (not staggered).
    pub fn is_active(&self) -> bool {
        self.stagger == 0
    }
}

/// Who, if anyone, holds the soul.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Possession {
    /// In open play — claimable by whoever reaches it.
    Loose,
    /// Carried by the agent with this id.
    Held(u32),
    /// A pass in flight, homing toward the agent with this id (the receiver).
    InFlight { to: u32 },
}

/// The soul (the ball): a position, and who holds it.
#[derive(Debug, Clone)]
pub struct Soul {
    pub pos: Vec2,
    pub possession: Possession,
}

impl Soul {
    /// A loose soul resting at `pos`.
    pub fn loose_at(pos: Vec2) -> Self {
        Self {
            pos,
            possession: Possession::Loose,
        }
    }

    /// Whether the soul is in open play (not carried).
    pub fn is_loose(&self) -> bool {
        matches!(self.possession, Possession::Loose)
    }
}

/// Move `pos` toward `target` by at most `max_step`, snapping on arrival.
///
/// Constant-speed steering, deterministic (fixed-point only). When the target
/// is within one step, the agent lands exactly on it (no asymptotic drift).
pub fn step_toward(pos: Vec2, target: Vec2, max_step: Fx) -> Vec2 {
    let delta = target - pos;
    let dist = delta.length();
    if dist <= max_step {
        target
    } else {
        let fraction = max_step / dist;
        pos + delta.scale(fraction)
    }
}

/// Build both teams in formation: team 0 from the base anchors, team 1 mirrored
/// across x. Every agent starts *on* its anchor (a consistent faceoff for every
/// soul — no random scatter). Ids index into the returned vec (team 0 first).
pub fn build_two_teams(base: &Formation) -> Vec<Agent> {
    let mut agents = Vec::with_capacity(base.anchors.len() * 2);
    for &anchor in &base.anchors {
        push_agent(&mut agents, 0, anchor);
    }
    for &anchor in &base.anchors {
        push_agent(&mut agents, 1, Vec2::new(-anchor.x, anchor.y));
    }
    agents
}

fn push_agent(agents: &mut Vec<Agent>, team: u8, anchor: Vec2) {
    let id = agents.len() as u32;
    agents.push(Agent {
        id,
        team,
        pos: anchor,
        target: anchor,
        anchor,
        stagger: 0,
        stamina: Fx::from_num(1),
    });
}
