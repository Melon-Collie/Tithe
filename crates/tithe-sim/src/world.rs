//! The world the sim moves: arena/config, the coach's formation, the agents,
//! and the kinematics that carry an agent toward its target.
//!
//! Slice 1 scope: agents hold field-relative anchors and move to them. There
//! is no soul, no possession, no decisions yet — that lands in later slices.
//! Coordination lives in the formation (the coach's shape), never in the
//! agents (CLAUDE.md → Committed architecture).

use crate::fx::{Fx, Vec2};
use crate::rng::Rng;

/// Tunable simulation parameters. These are dials set by prototype + AI-vs-AI
/// sim, not commitments (design doc §13) — hence config, not baked-in.
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
        }
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
    /// not commitments — this just gives Slice 1 a shape to hold.
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

/// A single agent: where it is, where it's headed, and which anchor it owns.
#[derive(Debug, Clone)]
pub struct Agent {
    pub pos: Vec2,
    pub target: Vec2,
    pub anchor: u32,
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

/// Scatter `count` agents to deterministic random positions, each owning the
/// matching anchor as its initial target.
pub fn scatter_agents(rng: &mut Rng, formation: &Formation, config: &SimConfig) -> Vec<Agent> {
    formation
        .anchors
        .iter()
        .enumerate()
        .map(|(i, &anchor)| {
            let pos = crate::fx::random_point_in_box(rng, config.arena_half_x, config.arena_half_y);
            Agent {
                pos,
                target: anchor,
                anchor: i as u32,
            }
        })
        .collect()
}
