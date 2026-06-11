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
//! ## Orientation
//!
//! The sport plays AI-vs-AI end to end. One tick is [`Simulation::tick`]; read
//! it top-to-bottom for the whole loop. The pieces live in: [`fx`] (fixed-point
//! math), [`rng`] (seeded RNG), [`world`] (config, formation, agents, the value
//! field's inputs), [`decide`] (the utility scorer and off-ball positioning),
//! [`value`] (the xT-style value field and cover-shadow), and [`event`] (the
//! output stream). The code and its tests are the spec; this crate is the
//! authority on behavior (CLAUDE.md → Source of truth).

// Determinism guards specific to the sim core (front-end Rust crates, if any,
// would legitimately use floats, so these are not workspace-wide).
#![deny(clippy::float_arithmetic, clippy::float_cmp)]

pub mod decide;
pub mod event;
pub mod fx;
pub mod rng;
pub mod setup;
pub mod value;
pub mod world;

pub use decide::Intent;
pub use event::Event;
pub use fx::{Fx, Vec2, WideFx};
pub use rng::Rng;
pub use setup::{MatchSetup, SetupError};
pub use world::Role;
pub use world::{Agent, Formation, Possession, SimConfig, Soul};

/// Opaque seed for a simulation run. Same seed + same inputs → same event
/// stream, on every platform and every replay.
pub type Seed = u64;

/// An offering in progress: which agent is winding up, and ticks left to resolve.
#[derive(Debug, Clone, Copy)]
struct OfferState {
    carrier: u32,
    ticks_left: u32,
}

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
    offering: Option<OfferState>,
}

impl Simulation {
    /// Start a fresh run from `seed`: two default teams (RNG-rolled attributes),
    /// a loose soul at center. For an *authored* matchup, see [`from_setup`].
    ///
    /// [`from_setup`]: Simulation::from_setup
    pub fn new(seed: Seed) -> Self {
        let config = SimConfig::default();
        let formation = Formation::default_seven();
        let mut rng = Rng::new(seed);
        let agents = world::build_two_teams(&formation, &mut rng);
        Self::assemble(seed, config, agents, rng)
    }

    /// Start a run from an authored [`MatchSetup`] (the coach-input boundary) and
    /// `seed`. The roster — attributes, roles, formations — comes from the setup;
    /// the seed still drives all match dynamics. Returns a [`SetupError`] if the
    /// setup is malformed (bad roster size, unknown formation, …).
    pub fn from_setup(setup: &MatchSetup, seed: Seed) -> Result<Self, SetupError> {
        let agents = setup::build_agents(setup)?;
        Ok(Self::assemble(
            seed,
            SimConfig::default(),
            agents,
            Rng::new(seed),
        ))
    }

    /// Shared assembly for both construction paths: a loose soul at center, an
    /// empty score, no winner, no offering in progress.
    fn assemble(seed: Seed, config: SimConfig, agents: Vec<Agent>, rng: Rng) -> Self {
        let goals = config.goals();
        Self {
            seed,
            tick: 0,
            config,
            goals,
            agents,
            soul: Soul::loose_at(Vec2::default()),
            rng,
            score: [0, 0],
            winner: None,
            offering: None,
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
        if self.tick == 1 {
            events.push(Event::NewSoul); // the opening kickoff
        }

        // Slow decision clock: at the first tick and each window boundary,
        // agents (re)choose an intent and commit to it; committed lunges for
        // the strip also resolve here (a strip is a deliberate commitment).
        if self.tick == 1 || self.tick.is_multiple_of(self.config.decision_interval) {
            self.run_decisions();
            // While an offering is winding up, the carrier is committed — defense
            // harries the offer (not a strip) and there's no pass.
            if self.offering.is_none() {
                self.resolve_strips(&mut events);
                self.resolve_on_ball(&mut events);
            }
        }

        self.advance_motion();
        self.apply_separation();
        for agent in &self.agents {
            events.push(Event::AgentMoved {
                agent: agent.id,
                pos: agent.pos,
            });
        }
        self.advance_soul_flight(&mut events);
        self.claim_loose_soul(&mut events);
        self.carry_soul();

        // The offering: a carrier who chose to shoot winds up, then resolves as
        // a skill check. A score on the X-th soul ends the match; a miss spits
        // the soul back into play.
        if !self.update_offering(&mut events) {
            events.push(Event::SoulMoved { pos: self.soul.pos });
        }
        events
    }

    /// Each agent scores the situation and commits to a target for the window.
    /// Off-ball agents shade by the value field (see `decide::off_ball_target`);
    /// active-pursuit intents map straight to their target.
    fn run_decisions(&mut self) {
        let carrier = self.carrier_info();
        let mut team_pos: [Vec<Vec2>; 2] = [Vec::new(), Vec::new()];
        for agent in &self.agents {
            team_pos[agent.team as usize].push(agent.pos);
        }
        let nearest = self.nearest_to_soul();

        for i in 0..self.agents.len() {
            // Commit this window's phase: attacking shape if my team holds the
            // soul, defending shape otherwise (enemy-held or loose). The anchor
            // flips here on the slow clock, so the team morphs between shapes at
            // the decision boundary, never mid-motion.
            let in_possession = carrier.is_some_and(|c| c.team == self.agents[i].team);
            self.agents[i].apply_phase(in_possession);

            let is_nearest = nearest[self.agents[i].team as usize] == Some(self.agents[i].id);
            let intent = decide::choose_intent(
                &self.agents[i],
                &self.soul,
                carrier,
                is_nearest,
                &self.config,
            );
            let agent = &self.agents[i];
            let target = if intent == decide::Intent::HoldAnchor {
                let t = agent.team as usize;
                decide::off_ball_target(
                    agent,
                    carrier,
                    self.goals,
                    &team_pos[t],
                    &team_pos[1 - t],
                    &self.config,
                )
            } else {
                decide::target_for(intent, agent, &self.soul, carrier, self.goals, &self.config)
            };
            self.agents[i].target = target;
        }
    }

    /// Resolve committed strips in id order. A defender within `strip_radius` of
    /// an enemy carrier lunges: a win turns the soul over (and ends the contest
    /// for this window); a whiff staggers the defender. The win chance is an
    /// **opposed contest** — the defender's Stripping against the carrier's
    /// Handling, swinging around the even-match baseline.
    fn resolve_strips(&mut self, events: &mut Vec<Event>) {
        let strip_radius = self.config.strip_radius;
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
            // Opposed contest: baseline + (Stripping − Handling) × spread, in
            // percentage points, always leaving a slim chance either way.
            let gap =
                self.agents[i].attributes.stripping - self.agents[carrier_idx].attributes.handling;
            let pct = (self.config.strip_even_pct + gap * self.config.strip_spread_pct)
                .clamp(Fx::from_num(5), Fx::from_num(95));
            let success_pct = pct.to_num::<u64>();
            let chance = success_pct as u8;
            if self.rng.below(100) < success_pct {
                self.soul.possession = Possession::Held(defender_id);
                self.soul.pos = self.agents[i].pos;
                events.push(Event::StripAttempt {
                    defender: defender_id,
                    carrier: carrier_id,
                    chance,
                    success: true,
                });
                events.push(Event::PossessionGained { agent: defender_id });
                return; // one turnover per decision window
            }

            self.agents[i].stagger = stagger_ticks;
            events.push(Event::StripAttempt {
                defender: defender_id,
                carrier: carrier_id,
                chance,
                success: false,
            });
        }
    }

    /// The id of the agent nearest the soul (loose or carried) on each team.
    /// Only that agent leaves the shape to engage — it contests a loose draw,
    /// or (on the defending team) challenges the carrier.
    fn nearest_to_soul(&self) -> [Option<u32>; 2] {
        let mut best: [Option<(Fx, u32)>; 2] = [None, None];
        for agent in &self.agents {
            if !agent.is_active() {
                continue;
            }
            let dist = agent.pos.distance_to(self.soul.pos);
            let slot = &mut best[agent.team as usize];
            if slot.is_none_or(|(d, _)| dist < d) {
                *slot = Some((dist, agent.id));
            }
        }
        [best[0].map(|(_, id)| id), best[1].map(|(_, id)| id)]
    }

    /// Move every active agent toward its target at a stamina-scaled speed, and
    /// drain stamina (more from effort, so the hardest workers tire first).
    /// Staggered agents recover a tick instead. No events — positions are
    /// emitted after separation resolves.
    fn advance_motion(&mut self) {
        let max_speed = self.config.max_speed;
        let floor = self.config.stamina_speed_floor;
        let drain_base = self.config.stamina_drain_base;
        let drain_per_unit = self.config.stamina_drain_per_unit;
        let one = Fx::from_num(1);
        let zero = Fx::from_num(0);

        for i in 0..self.agents.len() {
            if self.agents[i].stagger > 0 {
                self.agents[i].stagger -= 1;
            } else {
                let speed = max_speed * (floor + (one - floor) * self.agents[i].stamina);
                let from = self.agents[i].pos;
                let to = world::step_toward(from, self.agents[i].target, speed);
                self.agents[i].pos = to;
                let drain = drain_base + drain_per_unit * from.distance_to(to);
                self.agents[i].stamina = (self.agents[i].stamina - drain).max(zero);
            }
        }
    }

    /// Push apart any agents closer than `separation_radius` (bounded per tick),
    /// so they don't stack. Pure positional steering — reads all positions, then
    /// applies displacements, so it's order-independent and deterministic. Kept
    /// below `strip_radius`, so it never blocks a legitimate contest.
    fn apply_separation(&mut self) {
        let radius = self.config.separation_radius;
        let max_step = self.config.separation_step;
        let touching = Fx::from_num(1) / Fx::from_num(100);
        let n = self.agents.len();

        let pushes: Vec<Vec2> = (0..n)
            .map(|i| {
                let me = &self.agents[i];
                let mut push = Vec2::default();
                for (j, other) in self.agents.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    let delta = me.pos - other.pos;
                    let dist = delta.length();
                    if dist >= radius {
                        continue;
                    }
                    if dist > touching {
                        // Away from `other`, stronger the deeper the overlap.
                        push = push + delta.scale((radius - dist) / dist);
                    } else {
                        // Exactly coincident: nudge deterministically by id order.
                        let nudge = if me.id < other.id { radius } else { -radius };
                        push = push + Vec2::new(nudge, Fx::from_num(0));
                    }
                }
                push.clamp_len(max_step)
            })
            .collect();
        for (agent, push) in self.agents.iter_mut().zip(pushes) {
            agent.pos = agent.pos + push;
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
        events.push(Event::SoulClaimed { agent: winner });
    }

    /// A held soul rides with its carrier.
    fn carry_soul(&mut self) {
        if let Possession::Held(id) = self.soul.possession {
            self.soul.pos = self.agents[id as usize].pos;
        }
    }

    /// The carrier weighs **carry vs pass vs shoot** on one shared value field
    /// (expected value). Carrying = my value here minus a turnover cost. A pass
    /// = P(complete) × receiver value − P(lost) × what the opponent gains at the
    /// interception point. A shot = P(score from here) × the value of scoring −
    /// P(miss) × what the rebound concedes. Shooting wins when it's the best
    /// positive option (so a carrier drives in for a high-% look, or lets fly
    /// from range when he can't get closer); otherwise a pass that beats carrying
    /// by a margin launches the soul, else the carrier carries.
    fn resolve_on_ball(&mut self, events: &mut Vec<Event>) {
        let Possession::Held(carrier_id) = self.soul.possession else {
            return;
        };
        let team = self.agents[carrier_id as usize].team;
        let carrier_pos = self.agents[carrier_id as usize].pos;
        let my_goal = self.goals[team as usize];
        let enemy_goal = self.goals[1 - team as usize];
        let attrs = self.agents[carrier_id as usize].attributes;
        let aversion = self.config.turnover_aversion;
        let zero = Fx::from_num(0);
        let one = Fx::from_num(1);

        let enemies: Vec<Vec2> = self.team_positions(1 - team);
        let allies: Vec<Vec2> = self.team_positions(team);

        // Carrying: the best route toward goal that dodges pressure.
        let (carry_target, carry_ev) = self.best_carry_route(
            carrier_pos,
            my_goal,
            enemy_goal,
            &enemies,
            &allies,
            aversion,
        );

        // Shooting from here: score chance falls off with distance (further for
        // high-Range players), against what a missed shot's rebound concedes.
        let distance = carrier_pos.distance_to(my_goal);
        let shoot_contest = self.offering_contest(team, carrier_pos);
        let shoot_prob =
            self.shot_probability(distance, attrs.accuracy, attrs.range, shoot_contest);
        let shoot_ev = shoot_prob * self.config.shot_value
            - (one - shoot_prob)
                * value::value_at(carrier_pos, enemy_goal, &allies, &self.config)
                * aversion;

        // Passing: the best receiver by EV.
        let mut best_pass: Option<(u32, Fx, Option<u32>, Fx)> = None; // recv, completion, blocker, ev
        for agent in &self.agents {
            if agent.team != team || agent.id == carrier_id || !agent.is_active() {
                continue;
            }
            if carrier_pos.distance_to(agent.pos) > self.config.pass_max_dist {
                continue;
            }
            let (lane, blocker) = self.pass_lane(carrier_pos, agent.pos, team);
            // Better passers thread tighter lanes (uniform 0.5 ⇒ lane unchanged).
            let completion =
                (lane * (Fx::from_num(1) / Fx::from_num(2) + attrs.passing)).clamp(zero, one);
            if completion < self.config.pass_min_lane {
                continue;
            }
            let benefit = value::value_at(agent.pos, my_goal, &enemies, &self.config) * completion;
            let loss_point = blocker.map_or(agent.pos, |b| self.agents[b as usize].pos);
            let cost = (one - completion)
                * value::value_at(loss_point, enemy_goal, &allies, &self.config)
                * aversion;
            let ev = benefit - cost;
            if best_pass.is_none_or(|(_, _, _, e)| ev > e) {
                best_pass = Some((agent.id, completion, blocker, ev));
            }
        }
        let best_pass_ev = best_pass.map_or(Fx::from_num(-9999), |(_, _, _, e)| e);

        // Shoot if it's the best positive option…
        if shoot_ev > zero && shoot_ev >= carry_ev && shoot_ev >= best_pass_ev {
            self.offering = Some(OfferState {
                carrier: carrier_id,
                ticks_left: self.config.offering_windup,
            });
            events.push(Event::OfferingStarted {
                carrier: carrier_id,
            });
            return;
        }
        // …else pass if a receiver beats carrying by the margin…
        if best_pass_ev > carry_ev + self.config.pass_value_margin {
            let (receiver, completion, blocker, _) =
                best_pass.expect("best_pass_ev came from Some");
            let pct = (completion * Fx::from_num(100)).to_num::<u64>();
            let completed = self.rng.below(100) < pct;
            let intercepted = !completed && blocker.is_some();
            let to = if intercepted {
                blocker.expect("a failed pass has a blocker")
            } else {
                receiver
            };
            self.soul.possession = Possession::InFlight { to, intercepted };
            events.push(Event::PassMade {
                from: carrier_id,
                to: receiver,
                chance: pct as u8,
            });
            return;
        }
        // …else carry the chosen route around the pressure.
        self.agents[carrier_id as usize].target = carry_target;
    }

    /// The best carry route: among a few waypoints toward the goal, the one with
    /// the highest EV = value at the waypoint − path risk × what the opponent
    /// gains, so the carrier curves around a presser instead of into it.
    fn best_carry_route(
        &self,
        carrier_pos: Vec2,
        my_goal: Vec2,
        enemy_goal: Vec2,
        enemies: &[Vec2],
        allies: &[Vec2],
        aversion: Fx,
    ) -> (Vec2, Fx) {
        let to_goal = my_goal - carrier_pos;
        let unit = to_goal.normalized();
        let look = to_goal.length().min(self.config.carry_lookahead); // don't overshoot the goal
        let lat = self.config.carry_lateral;
        let perp = unit.perpendicular();
        let forward = carrier_pos + unit.scale(look);
        let candidates = [
            forward,
            forward + perp.scale(lat),
            forward - perp.scale(lat),
            forward + perp.scale(lat + lat),
            forward - perp.scale(lat + lat),
        ];

        let mut best = forward;
        let mut best_ev = Fx::from_num(-9999);
        for &waypoint in &candidates {
            let value = value::value_at(waypoint, my_goal, enemies, &self.config);
            // Path risk: the worst enemy sitting on the carrier→waypoint route.
            let mut path_risk = Fx::from_num(0);
            for &enemy in enemies {
                let on_path = value::segment_shadow(
                    enemy,
                    carrier_pos,
                    waypoint,
                    self.config.carry_contest_radius,
                );
                if on_path > path_risk {
                    path_risk = on_path;
                }
            }
            let cost =
                path_risk * value::value_at(waypoint, enemy_goal, allies, &self.config) * aversion;
            let ev = value - cost;
            if ev > best_ev {
                best_ev = ev;
                best = waypoint;
            }
        }
        (best, best_ev)
    }

    /// Lane clearance in `[0, 1]` for a pass `from`→`to`, plus the worst lane
    /// defender (the would-be interceptor). Per-defender block = perp_factor ×
    /// the defender's Contesting; endpoints excluded (a defender on the passer
    /// or receiver isn't *in* the lane).
    fn pass_lane(&self, from: Vec2, to: Vec2, passing_team: u8) -> (Fx, Option<u32>) {
        let seg = to - from;
        let len_sq: WideFx = seg.x.wide_mul(seg.x) + seg.y.wide_mul(seg.y);
        let zero = WideFx::from_num(0);
        if len_sq <= zero {
            return (Fx::from_num(1), None);
        }
        let radius = self.config.lane_radius;
        let mut max_block = Fx::from_num(0);
        let mut blocker = None;
        for agent in &self.agents {
            if agent.team == passing_team || !agent.is_active() {
                continue;
            }
            let off = agent.pos - from;
            let dot: WideFx = off.x.wide_mul(seg.x) + off.y.wide_mul(seg.y);
            if dot <= zero || dot >= len_sq {
                continue; // not strictly between the endpoints
            }
            let t = Fx::saturating_from_num(dot / len_sq);
            let perp = agent.pos.distance_to(from + seg.scale(t));
            if perp >= radius {
                continue;
            }
            let block = (Fx::from_num(1) - perp / radius) * agent.attributes.contesting;
            if block > max_block {
                max_block = block;
                blocker = Some(agent.id);
            }
        }
        (Fx::from_num(1) - max_block, blocker)
    }

    /// The positions of every active agent on `team`.
    fn team_positions(&self, team: u8) -> Vec<Vec2> {
        self.agents
            .iter()
            .filter(|a| a.team == team)
            .map(|a| a.pos)
            .collect()
    }

    /// Advance a pass in flight to its predetermined catcher (receiver on a
    /// completion, interceptor on a pick — decided at release). The soul homes
    /// and is caught within `pickup_radius`; no mid-flight convergence.
    fn advance_soul_flight(&mut self, events: &mut Vec<Event>) {
        let Possession::InFlight { to, intercepted } = self.soul.possession else {
            return;
        };
        let target = self.agents[to as usize].pos;
        self.soul.pos = world::step_toward(self.soul.pos, target, self.config.pass_speed);
        if self.soul.pos.distance_to(target) <= self.config.pickup_radius {
            self.soul.possession = Possession::Held(to);
            if intercepted {
                events.push(Event::PassIntercepted { by: to });
            }
            events.push(Event::PossessionGained { agent: to });
        }
    }

    /// Drive an in-progress offering: count the wind-up down and resolve it as a
    /// skill check. The offering is *started* by the carrier's shoot decision in
    /// [`resolve_on_ball`], not here. Returns `true` if a score ended the match
    /// (so the caller skips the trailing `SoulMoved`).
    ///
    /// [`resolve_on_ball`]: Simulation::resolve_on_ball
    fn update_offering(&mut self, events: &mut Vec<Event>) -> bool {
        if let Some(state) = self.offering {
            if state.ticks_left > 1 {
                self.offering = Some(OfferState {
                    ticks_left: state.ticks_left - 1,
                    ..state
                });
                return false;
            }
            return self.resolve_offering(state.carrier, events);
        }
        false
    }

    /// Summed Contesting of enemies within harry range of a shot, capped — the
    /// contest term that cuts a shot's success.
    fn offering_contest(&self, team: u8, carrier_pos: Vec2) -> Fx {
        let mut contest = Fx::from_num(0);
        for agent in &self.agents {
            if agent.team != team
                && agent.is_active()
                && agent.pos.distance_to(carrier_pos) <= self.config.offering_contest_radius
            {
                contest += agent.attributes.contesting;
            }
        }
        contest.min(self.config.offering_contest_max)
    }

    /// Probability a shot from `distance` scores: peak quality (base +
    /// Accuracy×gain) cut by a linear distance falloff whose reach grows with
    /// Range, then by enemy `contest`. The old point-blank offering is just the
    /// distance-0 case.
    fn shot_probability(&self, distance: Fx, accuracy: Fx, range: Fx, contest: Fx) -> Fx {
        let one = Fx::from_num(1);
        let zero = Fx::from_num(0);
        let effective_range = self.config.shot_base_range + range * self.config.shot_range_gain;
        let dist_factor = (one - distance / effective_range).clamp(zero, one);
        let peak = self.config.offering_base + accuracy * self.config.offering_accuracy_gain;
        (peak * dist_factor * (one - contest)).clamp(zero, one)
    }

    /// Resolve a finished wind-up: a shot whose success follows
    /// [`shot_probability`] from the carrier's distance to its goal, against the
    /// Contesting of enemies who arrived in range. A score may end the match; a
    /// miss spits the soul back into play.
    ///
    /// [`shot_probability`]: Simulation::shot_probability
    fn resolve_offering(&mut self, carrier_id: u32, events: &mut Vec<Event>) -> bool {
        self.offering = None;
        let team = self.agents[carrier_id as usize].team;
        let carrier_pos = self.agents[carrier_id as usize].pos;
        let attrs = self.agents[carrier_id as usize].attributes;

        let distance = carrier_pos.distance_to(self.goals[team as usize]);
        let contest = self.offering_contest(team, carrier_pos);
        let prob = self.shot_probability(distance, attrs.accuracy, attrs.range, contest);
        let success_pct = (prob * Fx::from_num(100)).to_num::<u64>();
        let scored = self.rng.below(100) < success_pct;
        events.push(Event::OfferingResolved {
            carrier: carrier_id,
            chance: success_pct as u8,
            scored,
        });

        if scored {
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
            events.push(Event::NewSoul);
        } else {
            // The fire rejects it — spit the soul back into open play, away from
            // the goal so there's no cheap put-back.
            let goal = self.goals[team as usize];
            let outward = (Vec2::default() - goal).clamp_len(self.config.rebound_distance);
            self.soul = Soul::loose_at(goal + outward);
        }
        false
    }

    /// Reset between souls: a fresh loose soul at center, every agent back on
    /// its anchor and recovered. (The §1 "round" boundary; in the full game
    /// this is also the manager's substitution beat.)
    ///
    /// The soul starts loose, so both teams set up out-of-possession — agents
    /// face off from their defending shape.
    fn reset_for_next_soul(&mut self) {
        self.soul = Soul::loose_at(Vec2::default());
        self.offering = None;
        for agent in self.agents.iter_mut() {
            agent.apply_phase(false); // loose soul → defending shape
            agent.pos = agent.anchor;
            agent.target = agent.anchor;
            agent.stagger = 0;
            agent.stamina = Fx::from_num(1); // fresh unit each soul (subs between souls)
        }
    }

    /// A snapshot of the current carrier (if any), for the decision scorer. A
    /// pass in flight counts its receiver as the carrier, so teammates support
    /// the catch and defenders converge to contest it.
    fn carrier_info(&self) -> Option<decide::CarrierInfo> {
        let id = match self.soul.possession {
            Possession::Held(id) | Possession::InFlight { to: id, .. } => id,
            Possession::Loose => return None,
        };
        let carrier = &self.agents[id as usize];
        Some(decide::CarrierInfo {
            id: carrier.id,
            team: carrier.team,
            pos: carrier.pos,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shot's success falls off with distance, and a high-Range shooter holds
    /// his chance much further out than a low-Range one (the Sniper vs Perimeter
    /// distinction).
    #[test]
    fn shot_probability_falls_off_with_distance_and_range_extends_it() {
        let sim = Simulation::new(0);
        let acc = Fx::from_num(7) / Fx::from_num(10); // 0.70
        let lo = Fx::from_num(3) / Fx::from_num(10); // low Range
        let hi = Fx::from_num(9) / Fx::from_num(10); // high Range
        let no_contest = Fx::from_num(0);
        let d = |n| Fx::from_num(n);

        // Point-blank: distance doesn't bite yet, Range barely matters.
        assert!(sim.shot_probability(d(0), acc, lo, no_contest) > Fx::from_num(0));
        // Closer beats further for the same shooter.
        assert!(
            sim.shot_probability(d(4), acc, lo, no_contest)
                > sim.shot_probability(d(12), acc, lo, no_contest)
        );
        // At a real distance, the high-Range shooter is still dangerous where the
        // low-Range one has dropped to nothing.
        assert_eq!(
            sim.shot_probability(d(20), acc, lo, no_contest),
            Fx::from_num(0)
        );
        assert!(sim.shot_probability(d(20), acc, hi, no_contest) > Fx::from_num(0));
        // Contest cuts the chance.
        assert!(
            sim.shot_probability(d(2), acc, hi, Fx::from_num(6) / Fx::from_num(10))
                < sim.shot_probability(d(2), acc, hi, no_contest)
        );
    }
}
