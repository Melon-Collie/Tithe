//! The world the sim moves: arena/config, the coach's formation, the two teams
//! of agents, the soul, and the kinematics that carry an agent toward its
//! target.
//!
//! Slice 3 scope: two teams contest the soul; a defender can **strip** an enemy
//! carrier (the §1 challenge — win = clean possession, whiff = a brief
//! stagger), and a carrier advances toward its own home goal so turnovers mean
//! transition. No scoring yet (the offering skill-check and first-to-X land in
//! Slice 4). Coordination lives in the formation, never in the agents.

use crate::fx::{random_point_in_box, Fx, Vec2};
use crate::rng::Rng;

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
    /// Distance at which the "chase the soul" urge fades to zero.
    pub chase_max_dist: Fx,
    /// Baseline pull of holding the anchor while the soul is loose — agents
    /// whose chase urge beats this collapse on the ball; the rest hold shape.
    pub hold_base: Fx,
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
            hold_base: Fx::from_num(15) / Fx::from_num(100), // 0.15
            goal_x: Fx::from_num(45),
            strip_radius: Fx::from_num(3),
            contest_range: Fx::from_num(25),
            strip_success_pct: 35,
            stagger_ticks: 15,
            offering_radius: Fx::from_num(3),
            souls_to_win: 11,
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
    /// A placeholder ~7-player "three-band-with-spine" shape. The exact
    /// geometry, team size, and zone banding are tuning dials (design doc §13),
    /// not commitments — this just gives the slice a shape to hold.
    pub fn default_seven() -> Self {
        let p = |x: i32, y: i32| Vec2::new(Fx::from_num(x), Fx::from_num(y));
        Self {
            anchors: vec![
                p(-30, -12),
                p(-30, 12), // back band
                p(0, -18),
                p(0, 0),
                p(0, 18), // mid spine
                p(30, -12),
                p(30, 12), // front band
            ],
        }
    }
}

/// A single agent: identity, team, where it is, where it's headed, its home
/// anchor, and how many ticks it remains staggered (0 = active).
#[derive(Debug, Clone)]
pub struct Agent {
    pub id: u32,
    pub team: u8,
    pub pos: Vec2,
    pub target: Vec2,
    pub anchor: Vec2,
    pub stagger: u32,
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

/// Build both teams: team 0 from the base formation, team 1 mirrored across x.
/// Ids are the index into the returned vec (team 0 first, then team 1).
pub fn build_two_teams(rng: &mut Rng, base: &Formation, config: &SimConfig) -> Vec<Agent> {
    let mut agents = Vec::with_capacity(base.anchors.len() * 2);
    for &anchor in &base.anchors {
        push_agent(&mut agents, 0, anchor, rng, config);
    }
    for &anchor in &base.anchors {
        let mirrored = Vec2::new(-anchor.x, anchor.y);
        push_agent(&mut agents, 1, mirrored, rng, config);
    }
    agents
}

fn push_agent(agents: &mut Vec<Agent>, team: u8, anchor: Vec2, rng: &mut Rng, config: &SimConfig) {
    let id = agents.len() as u32;
    let pos = random_point_in_box(rng, config.arena_half_x, config.arena_half_y);
    agents.push(Agent {
        id,
        team,
        pos,
        target: anchor,
        anchor,
        stagger: 0,
    });
}
