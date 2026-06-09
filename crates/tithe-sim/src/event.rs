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
}
