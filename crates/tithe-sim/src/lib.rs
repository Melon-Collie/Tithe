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
//! ## Status (Slice 4 — a complete match)
//!
//! What exists: fixed-point math, the two-clock tick loop, the utility-scored
//! decision model, two teams contesting one soul, the strip verb (win = clean
//! possession, whiff = stagger), turnovers, carriers advancing toward their
//! home goals, the touch-in offering, between-souls reset, and a first-to-X
//! winner. What's still open: cast-from-range offerings (needs the Finishing
//! attribute), passing, the anti-loiter aura, stamina, and player attributes
//! (design doc §1, §4, §13).

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
/// Inputs go in, an event stream comes out (see [`Simulation::tick`]). Slice 3
/// runs two teams contesting one soul under the two-clock loop.
#[derive(Debug, Clone)]
pub struct Simulation {
    seed: Seed,
    tick: u64,
    config: SimConfig,
    goals: [Vec2; 2],
    agents: Vec<Agent>,
    soul: Soul,
    rng: Rng,
    score: [u32; 2],
    winner: Option<u8>,
}

impl Simulation {
    /// Start a fresh run from `seed`: two default teams, a loose soul at center.
    pub fn new(seed: Seed) -> Self {
        let config = SimConfig::default();
        let formation = Formation::default_seven();
        let goals = config.goals();
        let rng = Rng::new(seed);
        let agents = world::build_two_teams(&formation);
        let soul = Soul::loose_at(Vec2::default());
        Self {
            seed,
            tick: 0,
            config,
            goals,
            agents,
            soul,
            rng,
            score: [0, 0],
            winner: None,
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

    /// The running score, `[team0, team1]`.
    pub fn score(&self) -> [u32; 2] {
        self.score
    }

    /// The winning team, once the match is over.
    pub fn winner(&self) -> Option<u8> {
        self.winner
    }

    /// The simulation's configuration (for consumers that need arena/goal
    /// geometry, e.g. a renderer).
    pub fn config(&self) -> &SimConfig {
        &self.config
    }

    /// Advance one fixed timestep, returning the events emitted this tick.
    /// Once the match is over, ticking is a no-op (an empty stream).
    pub fn tick(&mut self) -> Vec<Event> {
        if self.winner.is_some() {
            return Vec::new();
        }

        self.tick += 1;
        let mut events = Vec::with_capacity(self.agents.len() + 4);
        events.push(Event::Tick { tick: self.tick });

        // Slow decision clock: at the first tick and each window boundary,
        // agents (re)choose an intent and commit to it; committed lunges for
        // the strip also resolve here (a strip is a deliberate commitment).
        if self.tick == 1 || self.tick.is_multiple_of(self.config.decision_interval) {
            self.run_decisions();
            self.resolve_strips(&mut events);
        }

        self.advance_motion(&mut events);
        self.claim_loose_soul(&mut events);
        self.carry_soul();

        // The offering: a carrier reaching its own goal banks the soul. On the
        // X-th soul the match ends; otherwise a fresh soul begins.
        if !self.attempt_offering(&mut events) {
            events.push(Event::SoulMoved { pos: self.soul.pos });
        }
        events
    }

    /// Each agent scores the situation and commits to a target for the window.
    fn run_decisions(&mut self) {
        let carrier = self.carrier_info();
        for agent in self.agents.iter_mut() {
            let intent = decide::choose_intent(agent, &self.soul, carrier, &self.config);
            agent.target = decide::target_for(intent, agent, &self.soul, carrier, self.goals);
        }
    }

    /// Resolve committed strips in id order. A defender within `strip_radius` of
    /// an enemy carrier lunges: a win turns the soul over (and ends the contest
    /// for this window); a whiff staggers the defender.
    fn resolve_strips(&mut self, events: &mut Vec<Event>) {
        let strip_radius = self.config.strip_radius;
        let success_pct = u64::from(self.config.strip_success_pct);
        let stagger_ticks = self.config.stagger_ticks;

        for i in 0..self.agents.len() {
            if !self.agents[i].is_active() {
                continue;
            }
            let Possession::Held(carrier_id) = self.soul.possession else {
                return; // no carrier to contest
            };
            let carrier_idx = carrier_id as usize;
            if carrier_idx == i || self.agents[carrier_idx].team == self.agents[i].team {
                continue; // can't strip yourself or a teammate
            }
            let carrier_pos = self.agents[carrier_idx].pos;
            if self.agents[i].pos.distance_to(carrier_pos) > strip_radius {
                continue; // not in lunge range
            }

            let defender_id = self.agents[i].id;
            if self.rng.below(100) < success_pct {
                self.soul.possession = Possession::Held(defender_id);
                self.soul.pos = self.agents[i].pos;
                events.push(Event::StripAttempt {
                    defender: defender_id,
                    carrier: carrier_id,
                    success: true,
                });
                events.push(Event::PossessionGained { agent: defender_id });
                return; // one turnover per decision window
            }

            self.agents[i].stagger = stagger_ticks;
            events.push(Event::StripAttempt {
                defender: defender_id,
                carrier: carrier_id,
                success: false,
            });
        }
    }

    /// Move every active agent toward its target; staggered agents recover a
    /// tick instead. Either way, emit the agent's position.
    fn advance_motion(&mut self, events: &mut Vec<Event>) {
        let max_speed = self.config.max_speed;
        for i in 0..self.agents.len() {
            if self.agents[i].stagger > 0 {
                self.agents[i].stagger -= 1;
            } else {
                self.agents[i].pos =
                    world::step_toward(self.agents[i].pos, self.agents[i].target, max_speed);
            }
            events.push(Event::AgentMoved {
                agent: self.agents[i].id,
                pos: self.agents[i].pos,
            });
        }
    }

    /// Claim a loose soul. Every agent within `pickup_radius` this tick is a
    /// candidate; the winner is drawn by the seeded RNG, so a symmetric faceoff
    /// is a fair draw rather than an automatic lowest-id (team 0) win.
    fn claim_loose_soul(&mut self, events: &mut Vec<Event>) {
        if !self.soul.is_loose() {
            return;
        }
        let pickup_radius = self.config.pickup_radius;
        let mut reached: Vec<u32> = Vec::new();
        for agent in &self.agents {
            if agent.pos.distance_to(self.soul.pos) <= pickup_radius {
                reached.push(agent.id);
            }
        }
        if reached.is_empty() {
            return;
        }
        let winner = reached[self.rng.below(reached.len() as u64) as usize];
        self.soul.possession = Possession::Held(winner);
        events.push(Event::PossessionGained { agent: winner });
    }

    /// A held soul rides with its carrier.
    fn carry_soul(&mut self) {
        if let Possession::Held(id) = self.soul.possession {
            self.soul.pos = self.agents[id as usize].pos;
        }
    }

    /// Touch-in offering: if the carrier has reached its own goal, bank the
    /// soul. Returns `true` if this score ended the match (so the caller skips
    /// the trailing `SoulMoved`); otherwise resets for the next soul.
    fn attempt_offering(&mut self, events: &mut Vec<Event>) -> bool {
        let Possession::Held(id) = self.soul.possession else {
            return false;
        };
        let team = self.agents[id as usize].team;
        let pos = self.agents[id as usize].pos;
        if pos.distance_to(self.goals[team as usize]) > self.config.offering_radius {
            return false;
        }

        self.score[team as usize] += 1;
        events.push(Event::Scored {
            team,
            score: self.score,
        });

        if self.score[team as usize] >= self.config.souls_to_win {
            self.winner = Some(team);
            events.push(Event::MatchOver { winner: team });
            return true;
        }

        self.reset_for_next_soul();
        false
    }

    /// Reset between souls: a fresh loose soul at center, every agent back on
    /// its anchor and recovered. (The §1 "round" boundary; in the full game
    /// this is also the manager's substitution beat.)
    fn reset_for_next_soul(&mut self) {
        self.soul = Soul::loose_at(Vec2::default());
        for agent in self.agents.iter_mut() {
            agent.pos = agent.anchor;
            agent.target = agent.anchor;
            agent.stagger = 0;
        }
    }

    /// A snapshot of the current carrier (if any), for the decision scorer.
    fn carrier_info(&self) -> Option<decide::CarrierInfo> {
        match self.soul.possession {
            Possession::Held(id) => {
                let carrier = &self.agents[id as usize];
                Some(decide::CarrierInfo {
                    id: carrier.id,
                    team: carrier.team,
                    pos: carrier.pos,
                })
            }
            Possession::Loose => None,
        }
    }
}
