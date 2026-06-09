//! # tithe-sim
//!
//! The headless, deterministic simulation core for Tithe. Serialized inputs go
//! in, an event stream comes out; the sim never knows it's being watched
//! (rendering is just one consumer of the stream). This is the locked,
//! load-bearing architectural seam — see `CLAUDE.md` → *Committed architecture*
//! and the design doc §9.
//!
//! ## Determinism contract (load-bearing)
//!
//! Everything downstream — server-authoritative multiplayer, reproducible
//! replays, shared-seed leagues, golden-seed regression — rests on bit-stable
//! simulation. In this crate:
//!
//! - The sim is a **pure function of `(inputs, seed)`**: no wall-clock, no
//!   ambient RNG, no I/O, no global state inside simulation.
//! - **All randomness is seeded and threaded explicitly** ([`Rng`]). Never a
//!   global or thread-local RNG.
//! - **No floats** — continuous quantities use fixed-point / integer math.
//!   Floats appear only at the front-end/render boundary, never here. The
//!   `clippy::float_arithmetic` deny below enforces this structurally.
//! - **Iteration order is explicit and deterministic** — ordered collections
//!   (`BTreeMap`/`Vec`) or sort-before-iterate; never let `HashMap` iteration
//!   order leak into the event stream.
//!
//! ## Status
//!
//! Scaffold. The fixed-timestep tick loop, the two-timescale agent model (§2),
//! and the sport rules (§1) are **not implemented yet** — the attribute list,
//! sport geometry, and much else are deliberately open (design doc §13). What
//! exists today is the crate skeleton, a deterministic RNG, the event-stream
//! type, and the golden-seed test harness.

// Determinism guards specific to the sim core (front-end Rust crates, if any,
// would legitimately use floats, so these are not workspace-wide).
#![deny(clippy::float_arithmetic, clippy::float_cmp)]

pub mod event;
pub mod rng;

pub use event::Event;
pub use rng::Rng;

/// Opaque seed for a simulation run. Same seed + same inputs → same event
/// stream, on every platform and every replay.
pub type Seed = u64;

/// A headless, deterministic simulation.
///
/// Inputs go in, an event stream comes out (see [`Simulation::tick`]). This is
/// a scaffold: the body advances a fixed-timestep clock and touches the RNG so
/// the determinism harness has something real to pin, but the agent model and
/// sport rules are not built yet.
#[derive(Debug, Clone)]
pub struct Simulation {
    seed: Seed,
    rng: Rng,
    tick: u64,
}

impl Simulation {
    /// Start a fresh run from `seed`.
    pub fn new(seed: Seed) -> Self {
        Self {
            seed,
            rng: Rng::new(seed),
            tick: 0,
        }
    }

    /// The seed this run was started from.
    pub fn seed(&self) -> Seed {
        self.seed
    }

    /// How many ticks have been simulated so far.
    pub fn tick_count(&self) -> u64 {
        self.tick
    }

    /// Advance one fixed timestep, returning the events emitted this tick.
    ///
    /// Placeholder body: advances the clock and draws from the RNG (reserved
    /// for attribute-modulated perception noise, §2) so reproducibility is
    /// already testable. Real behavior lands here as the sim is built.
    pub fn tick(&mut self) -> Vec<Event> {
        self.tick += 1;
        let _perception_noise = self.rng.next_u64();
        vec![Event::Tick { tick: self.tick }]
    }
}
