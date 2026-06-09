//! The sim's output: a stream of events.
//!
//! The simulation emits a state/event stream and never knows it's being
//! watched — headless discards it, watch mode draws it, fast-forward renders
//! every Nth tick. The stream is a **first-class output**, not a debug
//! afterthought: replays, condensed games, organic analytics, and the
//! inspectable "why did he do that?" utility scores all fall out of it
//! (design doc §2; `CLAUDE.md` → Committed architecture).

use crate::fx::Vec2;

/// A single event emitted by the simulation.
///
/// `#[non_exhaustive]` because the real variants — possessions, strips,
/// offerings, the soul banking — are derived from the sim loop and are not
/// designed yet (the event list is open, design doc §13). Slice 1 emits a
/// tick marker plus each agent's new position.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// A fixed-timestep tick advanced. `tick` is the 1-based tick index.
    Tick { tick: u64 },
    /// An agent's position after this tick's motion.
    AgentMoved { agent: u32, pos: Vec2 },
    /// An agent claimed the soul — by reaching it loose, or by a winning strip.
    PossessionGained { agent: u32 },
    /// A committed strip: `defender` lunged at `carrier`. On `success` the soul
    /// turns over; otherwise the defender whiffs and is staggered.
    StripAttempt {
        defender: u32,
        carrier: u32,
        success: bool,
    },
    /// A team banked a soul (touch-in offering at its own goal). `score` is the
    /// running tally `[team0, team1]` after this score.
    Scored { team: u8, score: [u32; 2] },
    /// The match is over — `winner` reached the soul target first.
    MatchOver { winner: u8 },
    /// The soul's position after this tick (loose, or riding its carrier).
    SoulMoved { pos: Vec2 },
}
