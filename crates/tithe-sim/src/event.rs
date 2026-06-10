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
    /// A fresh soul has begun — the opening kickoff or after a score.
    NewSoul,
    /// An agent claimed a *loose* soul — the faceoff draw or a rebound recovery
    /// (distinct from a pass catch or a strip, which are turnovers).
    SoulClaimed { agent: u32 },
    /// An agent's position after this tick's motion.
    AgentMoved { agent: u32, pos: Vec2 },
    /// An agent claimed the soul — by reaching it loose, or by a winning strip.
    PossessionGained { agent: u32 },
    /// A committed strip: `defender` lunged at `carrier` with a `chance`% to win.
    /// On `success` the soul turns over; otherwise the defender whiffs and is
    /// staggered.
    StripAttempt {
        defender: u32,
        carrier: u32,
        chance: u8,
        success: bool,
    },
    /// A carrier launched a pass toward `to` with a `chance`% to complete.
    PassMade { from: u32, to: u32, chance: u8 },
    /// An enemy picked off a pass in flight (a turnover).
    PassIntercepted { by: u32 },
    /// A carrier reached its goal and began an offering (the wind-up).
    OfferingStarted { carrier: u32 },
    /// An offering resolved with a `chance`% to score. On a miss the soul is
    /// spat back into open play (no cheap put-back; a fresh scramble).
    OfferingResolved {
        carrier: u32,
        chance: u8,
        scored: bool,
    },
    /// A team banked a soul (a successful offering). `score` is the running
    /// tally `[team0, team1]` after this score.
    Scored { team: u8, score: [u32; 2] },
    /// The match is over — `winner` reached the soul target first.
    MatchOver { winner: u8 },
    /// The soul's position after this tick (loose, or riding its carrier).
    SoulMoved { pos: Vec2 },
}
