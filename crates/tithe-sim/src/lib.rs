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
//! ## Status (a playable sport with contested scoring)
//!
//! What exists: fixed-point math, the two-clock tick loop, the utility decision
//! model with a shared value field and value-driven off-ball positioning, two
//! teams, a faceoff draw (nearest contests), the strip verb (win/whiff/stagger),
//! turnovers, passing with in-flight interception, stamina, agent separation,
//! per-player attributes (Finishing/Stripping/Contesting, uniform for now), the
//! offering as a wind-up skill check (Finishing-gated, contestable, miss →
//! rebound), and a first-to-X winner. What's still open: per-player attribute
//! variation + generation, the anti-loiter aura, and phase-conditioned (with/
//! without ball) formations (design doc §1, §3, §4, §13).

// Determinism guards specific to the sim core (front-end Rust crates, if any,
// would legitimately use floats, so these are not workspace-wide).
#![deny(clippy::float_arithmetic, clippy::float_cmp)]

pub mod decide;
pub mod event;
pub mod fx;
pub mod rng;
pub mod value;
pub mod world;

pub use decide::Intent;
pub use event::Event;
pub use fx::{Fx, Vec2, WideFx};
pub use rng::Rng;
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
    /// Start a fresh run from `seed`: two default teams, a loose soul at center.
    pub fn new(seed: Seed) -> Self {
        let config = SimConfig::default();
        let formation = Formation::default_seven();
        let goals = config.goals();
        let mut rng = Rng::new(seed);
        let agents = world::build_two_teams(&formation, &mut rng);
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

        // The offering: a carrier at its goal winds up, then resolves as a
        // skill check. A score on the X-th soul ends the match; a miss spits the
        // soul back into play.
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
    /// for this window); a whiff staggers the defender.
    fn resolve_strips(&mut self, events: &mut Vec<Event>) {
        let strip_radius = self.config.strip_radius;
        let strip_max = Fx::from_num(self.config.strip_success_max_pct);
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
            // Strip-win chance scales with the defender's Stripping attribute.
            let success_pct = (self.agents[i].attributes.stripping * strip_max).to_num::<u64>();
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

    /// The carrier weighs carrying vs passing on one shared value field
    /// (expected value). Carrying = my offering value here, minus a turnover
    /// cost (how pressured I am × what the opponent gains from this spot). A
    /// pass = P(complete) × the receiver's value − P(lost) × what the opponent
    /// gains at the interception point. The best option that beats carrying (by
    /// a margin) launches the soul; the completion roll decides the outcome at
    /// release. Better passers complete more, so they pass more.
    fn resolve_on_ball(&mut self, events: &mut Vec<Event>) {
        let Possession::Held(carrier_id) = self.soul.possession else {
            return;
        };
        let team = self.agents[carrier_id as usize].team;
        let carrier_pos = self.agents[carrier_id as usize].pos;
        let my_goal = self.goals[team as usize];
        let enemy_goal = self.goals[1 - team as usize];
        let passing = self.agents[carrier_id as usize].attributes.passing;
        let aversion = self.config.turnover_aversion;

        let enemies: Vec<Vec2> = self.team_positions(1 - team);
        let allies: Vec<Vec2> = self.team_positions(team);

        // Carrying: pick the best carry *route* — a waypoint toward goal whose
        // path dodges pressure (so the carrier curves around a presser instead
        // of strolling into the strip).
        let (carry_target, carry_ev) = self.best_carry_route(
            carrier_pos,
            my_goal,
            enemy_goal,
            &enemies,
            &allies,
            aversion,
        );
        let mut best_ev = carry_ev + self.config.pass_value_margin;

        let mut chosen: Option<(u32, Fx, Option<u32>)> = None; // receiver, completion, blocker
        for agent in &self.agents {
            if agent.team != team || agent.id == carrier_id || !agent.is_active() {
                continue;
            }
            if carrier_pos.distance_to(agent.pos) > self.config.pass_max_dist {
                continue;
            }
            let (lane, blocker) = self.pass_lane(carrier_pos, agent.pos, team);
            // Better passers thread tighter lanes (uniform 0.5 ⇒ lane unchanged).
            let completion = (lane * (Fx::from_num(1) / Fx::from_num(2) + passing))
                .clamp(Fx::from_num(0), Fx::from_num(1));
            if completion < self.config.pass_min_lane {
                continue;
            }
            let benefit = value::value_at(agent.pos, my_goal, &enemies, &self.config) * completion;
            let loss_point = blocker.map_or(agent.pos, |b| self.agents[b as usize].pos);
            let cost = (Fx::from_num(1) - completion)
                * value::value_at(loss_point, enemy_goal, &allies, &self.config)
                * aversion;
            let ev = benefit - cost;
            if ev > best_ev {
                best_ev = ev;
                chosen = Some((agent.id, completion, blocker));
            }
        }

        if let Some((receiver, completion, blocker)) = chosen {
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
        } else {
            // No worthwhile pass — carry the chosen route around the pressure.
            self.agents[carrier_id as usize].target = carry_target;
        }
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

    /// Drive the offering: start one when a carrier reaches its goal, count the
    /// wind-up down, and resolve it as a skill check. Returns `true` if a score
    /// ended the match (so the caller skips the trailing `SoulMoved`).
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

        // Not offering yet: begin one if a carrier has reached its own goal.
        if let Possession::Held(id) = self.soul.possession {
            let team = self.agents[id as usize].team;
            let at_goal = self.agents[id as usize]
                .pos
                .distance_to(self.goals[team as usize])
                <= self.config.offering_radius;
            if at_goal {
                self.offering = Some(OfferState {
                    carrier: id,
                    ticks_left: self.config.offering_windup,
                });
                events.push(Event::OfferingStarted { carrier: id });
            }
        }
        false
    }

    /// Resolve a finished wind-up: success = (base + Finishing×gain) × (1 −
    /// contest), where contest sums the Contesting of enemies who arrived within
    /// range. A score may end the match; a miss spits the soul back into play.
    fn resolve_offering(&mut self, carrier_id: u32, events: &mut Vec<Event>) -> bool {
        self.offering = None;
        let team = self.agents[carrier_id as usize].team;
        let carrier_pos = self.agents[carrier_id as usize].pos;
        let finishing = self.agents[carrier_id as usize].attributes.finishing;

        let mut contest = Fx::from_num(0);
        for agent in &self.agents {
            if agent.team != team
                && agent.is_active()
                && agent.pos.distance_to(carrier_pos) <= self.config.offering_contest_radius
            {
                contest += agent.attributes.contesting;
            }
        }
        let contest = contest.min(self.config.offering_contest_max);
        let raw = self.config.offering_base + finishing * self.config.offering_finish_gain;
        let prob = (raw * (Fx::from_num(1) - contest)).clamp(Fx::from_num(0), Fx::from_num(1));
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
    fn reset_for_next_soul(&mut self) {
        self.soul = Soul::loose_at(Vec2::default());
        self.offering = None;
        for agent in self.agents.iter_mut() {
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
