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
//! - **No floats** — continuous quantities use fixed-point ([`fx`]). Floats
//!   appear only at the front-end/render boundary, never here. The
//!   `clippy::float_arithmetic` deny below enforces this structurally.
//! - **Iteration order is explicit and deterministic** — ordered collections
//!   (`Vec`/`BTreeMap`) or sort-before-iterate; never let `HashMap` iteration
//!   order leak into the event stream.
//!
//! ## Status (Slice 1 — substrate)
//!
//! What exists: fixed-point math, the two-clock tick loop, and agents holding
//! field-relative formation anchors, emitting positions to the event stream.
//! What's still open: the soul, possession, the strip verb, scoring, and the
//! utility-scored decision model (design doc §1, §2; much is open in §13).

// Determinism guards specific to the sim core (front-end Rust crates, if any,
// would legitimately use floats, so these are not workspace-wide).
#![deny(clippy::float_arithmetic, clippy::float_cmp)]

pub mod decide;
pub mod event;
pub mod fx;
pub mod rng;
pub mod world;

pub use decide::Intent;
pub use event::Event;
pub use fx::{Fx, Vec2, WideFx};
pub use rng::Rng;
pub use world::{Agent, Formation, Possession, SimConfig, Soul};

/// Opaque seed for a simulation run. Same seed + same inputs → same event
/// stream, on every platform and every replay.
pub type Seed = u64;

/// A headless, deterministic simulation.
///
/// Inputs go in, an event stream comes out (see [`Simulation::tick`]). Slice 1
/// runs the two-clock loop over agents that hold their formation anchors.
#[derive(Debug, Clone)]
pub struct Simulation {
    seed: Seed,
    tick: u64,
    config: SimConfig,
    formation: Formation,
    agents: Vec<Agent>,
    soul: Soul,
}

impl Simulation {
    /// Start a fresh run from `seed`, using the default config and formation,
    /// with a loose soul at the arena's center.
    pub fn new(seed: Seed) -> Self {
        let config = SimConfig::default();
        let formation = Formation::default_seven();
        let mut rng = Rng::new(seed);
        let agents = world::scatter_agents(&mut rng, &formation, &config);
        let soul = Soul::loose_at(Vec2::default());
        Self {
            seed,
            tick: 0,
            config,
            formation,
            agents,
            soul,
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

    /// The agents' current state (for inspection / rendering).
    pub fn agents(&self) -> &[Agent] {
        &self.agents
    }

    /// The soul's current state (for inspection / rendering).
    pub fn soul(&self) -> &Soul {
        &self.soul
    }

    /// Advance one fixed timestep, returning the events emitted this tick.
    pub fn tick(&mut self) -> Vec<Event> {
        self.tick += 1;

        // Slow decision clock: at the first tick and each window boundary,
        // agents (re)choose an intent via the shared utility scorer and commit
        // to it for the window (design doc §2). The scramble↔structured mode
        // is emergent — it's just which intent the scorer picked.
        if self.tick == 1 || self.tick.is_multiple_of(self.config.decision_interval) {
            for agent in self.agents.iter_mut() {
                let intent = decide::choose_intent(agent, &self.soul, &self.config);
                agent.target = decide::target_for(intent, agent, &self.soul, &self.formation);
            }
        }

        let mut events = Vec::with_capacity(self.agents.len() + 2);
        events.push(Event::Tick { tick: self.tick });

        // Fast execution clock: advance every agent toward its committed target.
        for (i, agent) in self.agents.iter_mut().enumerate() {
            agent.pos = world::step_toward(agent.pos, agent.target, self.config.max_speed);
            events.push(Event::AgentMoved {
                agent: i as u32,
                pos: agent.pos,
            });
        }

        // A loose soul is claimed by the first agent (lowest id) to reach it.
        if self.soul.is_loose() {
            for (i, agent) in self.agents.iter().enumerate() {
                if agent.pos.distance_to(self.soul.pos) <= self.config.pickup_radius {
                    self.soul.possession = Possession::Held(i as u32);
                    events.push(Event::PossessionGained { agent: i as u32 });
                    break;
                }
            }
        }

        // A held soul rides with its carrier.
        if let Possession::Held(id) = self.soul.possession {
            self.soul.pos = self.agents[id as usize].pos;
        }
        events.push(Event::SoulMoved { pos: self.soul.pos });

        events
    }
}
