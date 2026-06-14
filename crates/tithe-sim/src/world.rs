//! The world the sim moves: arena/config, the coach's formation, the two teams
//! of agents, the soul, and the kinematics that carry an agent toward its
//! target.
//!
//! Two teams contest the soul from a faceoff; a defender can **strip** an enemy
//! carrier (the §1 challenge — win = clean possession, whiff = a brief
//! stagger), and a carrier advances toward its own home goal to offer it.
//! Coordination lives in the formation, never in the agents.

use crate::fx::{Fx, Vec2};
use crate::rng::Rng;
use serde::{Deserialize, Serialize};

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
    /// Hex size (center-to-vertex) of the board grid (§14 redesign). The oval
    /// board is the hexes of this size inside the `arena_half_*` ellipse.
    pub hex_size: Fx,
    /// How close an agent must get to a loose soul to claim it.
    pub pickup_radius: Fx,
    /// Farthest a player will chase a loose soul (even its team's nearest holds
    /// shape beyond this, rather than abandoning the formation).
    pub chase_max_dist: Fx,
    /// X-coordinate of a home goal (team 0 attacks -goal_x, team 1 +goal_x).
    pub goal_x: Fx,
    /// How close a defender must be to an enemy carrier to lunge for a strip.
    pub strip_radius: Fx,
    /// How close an enemy carrier must be before a defender breaks shape to
    /// close it down (larger than strip_radius — close first, then lunge).
    pub contest_range: Fx,
    /// How far goal-side of the carrier the pressing defender sits (containment
    /// — take away the straight line so the carrier can't stroll through).
    pub pressure_containment_dist: Fx,
    /// How far ahead a carry route looks toward the goal.
    pub carry_lookahead: Fx,
    /// Lateral spacing of the carry-route candidates (to go around pressure).
    pub carry_lateral: Fx,
    /// Distance from a carry route within which a defender threatens it (the
    /// risk that routes the carrier around pressure).
    pub carry_contest_radius: Fx,
    /// Strip-win percent when defender Stripping equals carrier Handling — the
    /// even-match baseline the attribute gap swings around.
    pub strip_even_pct: Fx,
    /// Percentage points the strip chance shifts per unit of `Stripping −
    /// Handling` gap (the gap is in `[-1, 1]`).
    pub strip_spread_pct: Fx,
    /// Ticks a whiffed defender is staggered (beaten, can't act).
    pub stagger_ticks: u32,
    /// Ticks an offering takes to resolve (the wind-up defenders can arrive in).
    pub offering_windup: u32,
    /// Base offering success at point-blank (before Accuracy and contest).
    pub offering_base: Fx,
    /// Extra success from the carrier's Accuracy (added to base, ×accuracy).
    pub offering_accuracy_gain: Fx,
    /// A shot's effective range at Range 0 — the distance at which success
    /// reaches zero for a player with no ranged ability.
    pub shot_base_range: Fx,
    /// How much the carrier's Range attribute extends the effective shot range.
    pub shot_range_gain: Fx,
    /// The value of scoring (in value-field units) — how a shot's expected
    /// payoff weighs against carrying or passing in the on-ball decision.
    pub shot_value: Fx,
    /// How close an enemy must be to harry an offering.
    pub offering_contest_radius: Fx,
    /// Maximum total contest (cap on summed Contesting) — leaves a slim chance.
    pub offering_contest_max: Fx,
    /// How far a missed offering's soul is spat back out from the goal.
    pub rebound_distance: Fx,
    /// Souls a team must bank to win the match (first-to-X).
    pub souls_to_win: u32,
    /// Speed a passed soul travels in flight (faster than a runner).
    pub pass_speed: Fx,
    /// Maximum distance over which a carrier will attempt a pass.
    pub pass_max_dist: Fx,
    /// How much a receiver's value must beat the carrier's for a pass to fire.
    pub pass_value_margin: Fx,
    /// Minimum completion chance (0..1) for a pass to be attempted at all.
    pub pass_min_lane: Fx,
    /// Weight on turnover cost in the carry-vs-pass decision — how much a player
    /// fears losing the soul (×the value the opponent would gain).
    pub turnover_aversion: Fx,
    /// Distance at which goal-closeness value reaches zero (the value field's span).
    pub value_span: Fx,
    /// Radius within which an enemy contributes to a spot's pressure.
    pub pressure_radius: Fx,
    /// Enemy-count (distance-weighted) that fully smothers a spot's openness.
    pub pressure_max: Fx,
    /// Perpendicular distance within which a defender blocks a pass lane (cuts
    /// the completion estimate).
    pub lane_radius: Fx,
    /// Perpendicular distance within which a defender can *pick off* a pass whose
    /// completion roll failed — wider than `lane_radius` (he reads a bad ball from
    /// a step away). A failed roll with no defender this close still reaches the
    /// receiver (a genuinely open lane), so only contested passes get picked.
    pub intercept_lane_radius: Fx,
    /// Floor on the interception "read" — the pick chance for a Positioning-0
    /// defender who's downfield in the lane (rises to 1 at Positioning 1). Keeps
    /// picks common enough that Passing still bites while making Positioning the
    /// read skill: `read = floor + (1 − floor)·Positioning`.
    pub intercept_read_floor: Fx,
    /// Stamina lost per tick just by being on the field (active).
    pub stamina_drain_base: Fx,
    /// Extra stamina lost per unit of distance moved (effort — pressers tire fastest).
    pub stamina_drain_per_unit: Fx,
    /// Speed multiplier at empty stamina (full stamina = 1.0). Gassed = slower.
    pub stamina_speed_floor: Fx,
    /// Speed multiplier at Pace 0 and Pace 1 — a player's top speed is
    /// interpolated between these (Pace 0.5 ≈ the average 1.0).
    pub pace_floor: Fx,
    pub pace_ceil: Fx,
    /// Soft-edge firmness on the footprint (§14 Phase 3). A spot *outside* the
    /// role's footprint ellipse has its appeal divided by `1 + this·(d − 1)`,
    /// where `d` is the squared normalized ellipse distance (`d ≤ 1` inside). A
    /// bigger value pins agents harder inside their zone; the edge is soft, not a
    /// wall — a much better spot just past the line is still worth stepping to.
    pub footprint_edge_softness: Fx,
    /// Hard outer bound on the soft edge: a spot with `d` beyond this is never
    /// considered (caps how far an agent — or a carrier's route — strays).
    pub footprint_edge_max: Fx,
    /// Floor on the carrier's soft tether (§14 Phase 3): the minimum carry
    /// appetite a carrier keeps even when fully outside his footprint. On-ball is
    /// *reluctance, not refusal* — "you can leave your zone with the ball, you're
    /// just less inclined to." `1.0` disables the tether; `0.0` makes a strayed
    /// carrier give it up entirely. (Off-ball positioning has no floor — the
    /// defensive shape stays firm.)
    pub carry_tether_floor: Fx,
    /// Ball-watching pull for a Positioning-0 player (scales with `1 −
    /// positioning`). Off the ball, a low-Positioning player's hex appeal is
    /// dragged toward the soul — he drifts to the ball and off his man/lane; a
    /// high-Positioning player ignores the ball's gravity and holds the right
    /// spot. Positioning is *skill* (a corrupted objective), not jitter. `0`
    /// makes Positioning a no-op.
    pub positioning_ball_pull: Fx,
    /// Spacing weight for a Positioning-1 player (scales *with* positioning). Off
    /// the ball, a high-Positioning player is penalized for choosing a hex near a
    /// teammate, so he spreads to fill gaps and cover distinct threats; a low one
    /// ignores it and bunches. The mirror of `positioning_ball_pull`: poor
    /// positioning clumps on the ball, elite spreads into space. `0` disables it.
    pub positioning_spacing: Fx,
    /// Perception error a Awareness-0 player has reading an object at
    /// `awareness_ref_dist` (scales with `1 − awareness` and with distance).
    pub awareness_noise_max: Fx,
    /// Reference distance for perception error — error grows with distance/this.
    pub awareness_ref_dist: Fx,
    /// Agents closer than this push apart (kept below strip_radius so it never
    /// blocks a legitimate contest).
    pub separation_radius: Fx,
    /// Maximum separation push applied per tick.
    pub separation_step: Fx,
    /// Per-in-possession-role weighting on the carry/pass/shoot decision — the
    /// role-tuning table (see [`RoleBiases`]).
    pub role_biases: RoleBiases,
    /// Per-out-of-possession-role weighting on defending (see [`DefenseBiases`]).
    pub defense_biases: DefenseBiases,
}

impl SimConfig {
    /// The on-ball bias for an in-possession role (shorthand for the table).
    pub fn on_ball_bias(&self, role: InPossessionRole) -> OnBallBias {
        self.role_biases.for_role(role)
    }

    /// The defensive bias for an out-of-possession role (shorthand for the table).
    pub fn defense_bias(&self, role: OutOfPossessionRole) -> DefenseBias {
        self.defense_biases.for_role(role)
    }

    /// Soft-edge appeal multiplier for a spot at squared ellipse distance `d`
    /// from a footprint (§14 Phase 3): `1` inside (`d ≤ 1`), decaying as
    /// `1/(1 + softness·(d − 1))` outside, and `None` past the hard outer bound
    /// (the caller skips that spot). Lets agents leak just past the edge for a
    /// much better spot while pinning them inside the zone otherwise.
    pub fn footprint_falloff(&self, d: Fx) -> Option<Fx> {
        if d > self.footprint_edge_max {
            return None;
        }
        let one = Fx::from_num(1);
        if d <= one {
            Some(one)
        } else {
            Some(one / (one + self.footprint_edge_softness * (d - one)))
        }
    }
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            decision_interval: 12,
            max_speed: Fx::from_num(2),
            arena_half_x: Fx::from_num(50),
            arena_half_y: Fx::from_num(30),
            hex_size: Fx::from_num(4), // ~14 hexes across × ~10 tall, oval-clipped

            pickup_radius: Fx::from_num(2),
            chase_max_dist: Fx::from_num(120),
            goal_x: Fx::from_num(45),
            strip_radius: Fx::from_num(3),
            contest_range: Fx::from_num(25),
            pressure_containment_dist: Fx::from_num(2), // within strip_radius (3): contain AND strip
            carry_lookahead: Fx::from_num(15),
            carry_lateral: Fx::from_num(6),
            carry_contest_radius: Fx::from_num(7),
            strip_even_pct: Fx::from_num(50), // even Stripping vs Handling ≈ coin flip
            strip_spread_pct: Fx::from_num(60), // a 0.2 attribute edge ≈ ±12 points
            stagger_ticks: 15,
            offering_windup: 8,
            offering_base: Fx::from_num(4) / Fx::from_num(10), // 0.4
            offering_accuracy_gain: Fx::from_num(5) / Fx::from_num(10), // 0.5 → accuracy 0.5 ⇒ 0.65 peak
            // Range 0 ⇒ effective to ~5 units; Range 0.85 ⇒ ~26 (a perimeter threat).
            shot_base_range: Fx::from_num(5),
            shot_range_gain: Fx::from_num(25),
            shot_value: Fx::from_num(3), // tuned against AI-vs-AI shot/score rates
            offering_contest_radius: Fx::from_num(7),
            offering_contest_max: Fx::from_num(9) / Fx::from_num(10), // 0.9
            rebound_distance: Fx::from_num(20),
            souls_to_win: 11,
            pass_speed: Fx::from_num(8),
            pass_max_dist: Fx::from_num(40),
            pass_value_margin: Fx::from_num(5) / Fx::from_num(100), // 0.05
            pass_min_lane: Fx::from_num(4) / Fx::from_num(10),      // 0.4
            turnover_aversion: Fx::from_num(1),
            value_span: Fx::from_num(90),
            pressure_radius: Fx::from_num(12),
            pressure_max: Fx::from_num(2),
            lane_radius: Fx::from_num(4),
            intercept_lane_radius: Fx::from_num(9), // ~2 hexes — a defender reads a bad pass
            intercept_read_floor: Fx::from_num(5) / Fx::from_num(10), // 0.5 — picks common, Positioning swings them
            stamina_drain_base: Fx::from_num(5) / Fx::from_num(10000), // 0.0005
            stamina_drain_per_unit: Fx::from_num(25) / Fx::from_num(10000), // 0.0025
            stamina_speed_floor: Fx::from_num(55) / Fx::from_num(100), // 0.55
            pace_floor: Fx::from_num(75) / Fx::from_num(100),         // 0.75 (Pace 0)
            pace_ceil: Fx::from_num(125) / Fx::from_num(100),         // 1.25 (Pace 1)
            footprint_edge_softness: Fx::from_num(3),                 // firm but soft at the edge
            footprint_edge_max: Fx::from_num(4), // never stray past 2× the radius
            carry_tether_floor: Fx::from_num(6) / Fx::from_num(10), // 0.6 — reluctance, not refusal
            positioning_ball_pull: Fx::from_num(8) / Fx::from_num(10), // 0.8 pull at Positioning 0
            positioning_spacing: Fx::from_num(2) / Fx::from_num(10), // 0.2 — A/B off vs on
            awareness_noise_max: Fx::from_num(6), // Awareness 0 at ref dist ⇒ ±6
            awareness_ref_dist: Fx::from_num(20),
            separation_radius: Fx::from_num(5) / Fx::from_num(2), // 2.5 (< strip_radius 3)
            separation_step: Fx::from_num(1),
            // In-possession role table (§14). Tendency: carry/pass/shoot in %,
            // shoot gate in % score-chance. Footprint: half_x × half_y (field
            // units) + forward lean. NOTE: x is the goal-to-goal (play) axis —
            // the arena's long axis — and y is across the field. So half_x > half_y
            // is a lane *along* the field; half_y > half_x is a band *across* it.
            // Edit a row to change how a role plays.
            role_biases: {
                let bias =
                    |carry: u32, pass: u32, shoot: u32, gate: u32, hx: u32, hy: u32| OnBallBias {
                        carry_mult: Fx::from_num(carry) / Fx::from_num(100),
                        pass_mult: Fx::from_num(pass) / Fx::from_num(100),
                        shoot_mult: Fx::from_num(shoot) / Fx::from_num(100),
                        min_shoot_prob: Fx::from_num(gate) / Fx::from_num(100),
                        footprint: Footprint {
                            half_x: Fx::from_num(hx),
                            half_y: Fx::from_num(hy),
                            lean: Fx::from_num(0),
                        },
                    };
                RoleBiases {
                    //                  carry pass shoot gate  hx  hy   shape
                    box_to_box: bias(110, 100, 90, 0, 16, 6), // long lane along the field
                    roamer: bias(100, 100, 90, 0, 6, 16),     // wide flat band across
                    playmaker: bias(90, 140, 80, 0, 9, 9),    // compact
                    outlet: bias(40, 130, 70, 0, 8, 8),       // compact, recycles
                    finisher: bias(90, 90, 140, 0, 9, 9),     // shell, shoots
                }
            },
            // Out-of-possession role table (§14). contest-range in %, lunge gate
            // in % strip-chance, footprint half_x × half_y + forward lean.
            defense_biases: {
                let def =
                    |contest: u32, lunge_gate: u32, hx: u32, hy: u32, lean: u32| DefenseBias {
                        contest_range_mult: Fx::from_num(contest) / Fx::from_num(100),
                        lunge_min_prob: Fx::from_num(lunge_gate) / Fx::from_num(100),
                        footprint: Footprint {
                            half_x: Fx::from_num(hx),
                            half_y: Fx::from_num(hy),
                            lean: Fx::from_num(lean),
                        },
                    };
                DefenseBiases {
                    //              contest lunge  hx  hy  lean   shape
                    destroyer: def(100, 0, 6, 6, 0), // tiny, trigger-happy
                    presser: def(160, 0, 8, 8, 10),  // forward-leaning, aggressive
                    warden: def(80, 35, 16, 14, 0),  // large, patient
                    sweeper: def(60, 40, 6, 18, 0),  // wide band across the last line
                    cheat: def(0, 90, 8, 8, 0),      // no challenge; offensive positioning
                    tracker: def(100, 20, 16, 6, 0), // long lane along the field
                }
            },
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
    /// A placeholder ~7-player full-court shape for team 0, **biased toward the
    /// opponent's goal** (+x — the goal team 0 must deny): a deep safety near
    /// its own goal, a spine contesting center, and a forward press up top.
    /// Team 1 mirrors it across x. The exact geometry, team size, and zone
    /// banding are tuning dials (design doc §13), not commitments.
    pub fn default_seven() -> Self {
        let p = |x: i32, y: i32| Vec2::new(Fx::from_num(x), Fx::from_num(y));
        Self {
            anchors: vec![
                p(-30, 0), // deep safety (near own goal)
                p(-5, -15),
                p(-5, 15), // midfield
                p(10, 0),  // spine — contests the center soul
                p(28, -16),
                p(28, 0),
                p(28, 16), // forward press (denying the opponent's goal)
            ],
        }
    }
}

/// The nine player attributes in their **canonical order** — the one source of
/// truth for any code that must iterate or index attributes positionally rather
/// than name them (the coach's role-want vectors, scout summaries, the
/// management UI's stat columns). The discriminant doubles as that index, so
/// `Attribute::Handling as usize` is the slot Handling occupies in any
/// `[_; 9]` attribute array. Keep this in lockstep with the fields of
/// [`Attributes`] and [`PlayerSetup`]; the `attribute_keys_match_fields` test
/// fails loudly if they ever drift, so a reorder can't silently misalign.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attribute {
    Accuracy,
    Range,
    Handling,
    Stripping,
    Contesting,
    Passing,
    Positioning,
    Pace,
    Awareness,
}

impl Attribute {
    /// Every attribute in canonical order — drive positional loops off this
    /// rather than hand-writing `0..9`, so the order lives in exactly one place.
    pub const ALL: [Attribute; 9] = [
        Attribute::Accuracy,
        Attribute::Range,
        Attribute::Handling,
        Attribute::Stripping,
        Attribute::Contesting,
        Attribute::Passing,
        Attribute::Positioning,
        Attribute::Pace,
        Attribute::Awareness,
    ];

    /// Short scout/box-score label.
    pub fn short(self) -> &'static str {
        match self {
            Attribute::Accuracy => "Acc",
            Attribute::Range => "Rng",
            Attribute::Handling => "Han",
            Attribute::Stripping => "Str",
            Attribute::Contesting => "Con",
            Attribute::Passing => "Pas",
            Attribute::Positioning => "Pos",
            Attribute::Pace => "Pace",
            Attribute::Awareness => "Awr",
        }
    }
}

/// Per-player capabilities. A stat exists only because some sim step consumes
/// it (§4): shooting splits into `accuracy` (point-blank conversion quality) and
/// `range` (how far that quality holds up — the perimeter threat); `handling`
/// → resists a strip (the carrier's side of the strip contest); `stripping`
/// → strip-the-carrier success (the defender's side); `contesting` → pass
/// interception + offering contest (and the lane area a defender covers);
/// `passing` → pass completion; `positioning` → anchor discipline (a low score
/// adds noise to where the player thinks his spot is). All in `[0, 1]`.
#[derive(Debug, Clone, Copy)]
pub struct Attributes {
    /// Point-blank shot conversion quality (the high-Accuracy interior finisher).
    pub accuracy: Fx,
    /// How slowly shot success falls off with distance — a high-Range player
    /// stays a threat from the perimeter.
    pub range: Fx,
    /// Protects the soul against a strip — the carrier's side of the strip
    /// contest. A high-Handling carrier keeps it through pressure.
    pub handling: Fx,
    pub stripping: Fx,
    pub contesting: Fx,
    /// Raises a passer's completion chance — better passers complete more, so
    /// the expected-value decision has them pass more often.
    pub passing: Fx,
    /// Anchor discipline — how faithfully the player holds his assigned spot. A
    /// low score adds positional noise to his working anchor each window (he
    /// drifts off his mark); a high score sits dead on it. Distinct from the
    /// role's intentional drift.
    pub positioning: Fx,
    /// Top speed — scales how fast the player moves (the burner reaches the soul
    /// first and closes down harder). Maps to a speed multiplier between
    /// `pace_floor` and `pace_ceil`.
    pub pace: Fx,
    /// Perception quality — how accurately the player reads the world he decides
    /// against. A low score perceives the ball, teammates, and opponents at noisy
    /// positions (worse the further away), so he shades wrong, blows assignments,
    /// and throws into coverage he didn't see (§2). The genius layer.
    pub awareness: Fx,
}

impl Attributes {
    /// Every player identical and mid-range. Kept for tests; live play uses
    /// [`Attributes::random`].
    pub fn uniform() -> Self {
        let mid = Fx::from_num(1) / Fx::from_num(2); // 0.5
        Self {
            accuracy: mid,
            range: mid,
            handling: mid,
            stripping: mid,
            contesting: mid,
            passing: mid,
            positioning: mid,
            pace: mid,
            awareness: mid,
        }
    }

    /// Independent draws per attribute in `[0.25, 0.85]`, so players differ and
    /// rough archetypes emerge (a sniper, a checker, a passer). A placeholder
    /// for the real archetype-first, sim-validated generation (§4); deterministic
    /// from the threaded [`Rng`].
    pub fn random(rng: &mut Rng) -> Self {
        Self {
            accuracy: draw_attribute(rng),
            range: draw_attribute(rng),
            handling: draw_attribute(rng),
            stripping: draw_attribute(rng),
            contesting: draw_attribute(rng),
            passing: draw_attribute(rng),
            positioning: draw_attribute(rng),
            pace: draw_attribute(rng),
            awareness: draw_attribute(rng),
        }
    }

    /// Read one attribute by its canonical [`Attribute`] key — the named bridge
    /// for code that iterates or indexes attributes (so it never has to assume a
    /// positional order).
    pub fn get(&self, a: Attribute) -> Fx {
        match a {
            Attribute::Accuracy => self.accuracy,
            Attribute::Range => self.range,
            Attribute::Handling => self.handling,
            Attribute::Stripping => self.stripping,
            Attribute::Contesting => self.contesting,
            Attribute::Passing => self.passing,
            Attribute::Positioning => self.positioning,
            Attribute::Pace => self.pace,
            Attribute::Awareness => self.awareness,
        }
    }
}

/// One attribute value, uniform in `[0.25, 0.85]` in 0.01 steps.
fn draw_attribute(rng: &mut Rng) -> Fx {
    Fx::from_num(25 + rng.below(61)) / Fx::from_num(100)
}

/// A player's **in-possession** casting (§14) — a fixed-shape footprint + an
/// on-ball tendency. The footprint (a `Footprint` from [`SimConfig`]) sizes the
/// off-ball zone; the tendency ([`OnBallBias`]) weights carry/pass/shoot. Same
/// dumb scorer, different appetites; the *role* is tendency, the shooting
/// *attributes* (Accuracy/Range) are capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InPossessionRole {
    /// Tall/narrow column — balanced, carries the lane (wide = winger, central = B2B).
    BoxToBox,
    /// Wide/flat band — balanced carry vs pass, drifts laterally (the default).
    #[default]
    Roamer,
    /// Compact — pass-first, forward.
    Playmaker,
    /// Compact, sits deep — safe recycle, low carry.
    Outlet,
    /// Finisher shell — shoots (Accuracy/Range + placement = the style).
    Finisher,
}

/// A player's **out-of-possession** casting (§14) — a fixed-shape footprint + a
/// challenge tendency ([`DefenseBias`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutOfPossessionRole {
    /// Tiny symmetric zone — trigger-happy challenge.
    Destroyer,
    /// Forward-leaning zone — aggressive, pushes up into the buildup.
    Presser,
    /// Large area — patient/contain (needs Positioning + Awareness) — the default.
    #[default]
    Warden,
    /// Wide/flat band — patient last line.
    Sweeper,
    /// Compact, sits high — ~no challenge; positions by *offensive* value (the
    /// counter outlet), defending a man down for the break.
    Cheat,
    /// Tall/narrow column — covers a vertical lane.
    Tracker,
}

/// A role's footprint shape — an ellipse of legal hexes around the anchor, with
/// an optional forward `lean`. `half_x` × `half_y` is the size×aspect grammar
/// (tall/narrow vs wide/flat vs blob); `lean` shifts it toward the enemy goal
/// (the Presser's directional push). The off-ball decision picks the best board
/// hex inside it.
#[derive(Debug, Clone, Copy)]
pub struct Footprint {
    pub half_x: Fx,
    pub half_y: Fx,
    pub lean: Fx,
}

impl Footprint {
    /// Where the footprint sits: the anchor shifted by `lean` toward the enemy
    /// goal. `fwd` is the sign of the enemy-goal direction (`+1` / `-1`).
    pub fn center(&self, anchor: Vec2, fwd: Fx) -> Vec2 {
        Vec2::new(anchor.x + self.lean * fwd, anchor.y)
    }

    /// Squared normalized ellipse distance of `point` from the footprint placed
    /// at `center`: `≤ 1` inside the shape, `> 1` outside (the soft-edge metric).
    pub fn dist_sq(&self, center: Vec2, point: Vec2) -> Fx {
        let nx = (point.x - center.x) / self.half_x;
        let ny = (point.y - center.y) / self.half_y;
        nx * nx + ny * ny
    }
}

/// Per-role weighting on the carry/pass/shoot decision **plus** the off-ball
/// footprint shape — the whole in-possession role-tuning surface. The multipliers
/// scale the *upside* of an action (never the net EV, so risk stays honest).
#[derive(Debug, Clone, Copy)]
pub struct OnBallBias {
    /// Scales the value of carrying.
    pub carry_mult: Fx,
    /// Scales the value of passing (Playmaker/Outlet ↑).
    pub pass_mult: Fx,
    /// Scales the value of shooting (Finisher ↑).
    pub shoot_mult: Fx,
    /// A shooter below this score chance won't pull the trigger (0 = no gate).
    pub min_shoot_prob: Fx,
    /// The off-ball footprint shape for this role.
    pub footprint: Footprint,
}

/// The tunable per-[`InPossessionRole`] bias table (a [`SimConfig`] dial). To
/// change how a role plays, edit its row in [`SimConfig::default`].
#[derive(Debug, Clone, Copy)]
pub struct RoleBiases {
    pub box_to_box: OnBallBias,
    pub roamer: OnBallBias,
    pub playmaker: OnBallBias,
    pub outlet: OnBallBias,
    pub finisher: OnBallBias,
}

impl RoleBiases {
    /// The bias for a given in-possession role.
    pub fn for_role(&self, role: InPossessionRole) -> OnBallBias {
        match role {
            InPossessionRole::BoxToBox => self.box_to_box,
            InPossessionRole::Roamer => self.roamer,
            InPossessionRole::Playmaker => self.playmaker,
            InPossessionRole::Outlet => self.outlet,
            InPossessionRole::Finisher => self.finisher,
        }
    }
}

/// Per-[`OutOfPossessionRole`] weighting on defending — the challenge tendency
/// **plus** the off-ball footprint shape. Like [`OnBallBias`], the whole
/// defensive role-tuning surface.
#[derive(Debug, Clone, Copy)]
pub struct DefenseBias {
    /// Scales how far the carrier must be before this defender breaks shape to
    /// close down (Presser ↑ hounds from distance, Sweeper ↓ holds until close;
    /// 0 = never breaks shape, like a Cheat).
    pub contest_range_mult: Fx,
    /// A defender won't commit a lunge whose strip chance is below this — a
    /// patient role *contains* (no whiff, no seam) rather than gambling.
    pub lunge_min_prob: Fx,
    /// The off-ball footprint shape for this role.
    pub footprint: Footprint,
}

/// The tunable per-[`OutOfPossessionRole`] bias table (a [`SimConfig`] dial).
#[derive(Debug, Clone, Copy)]
pub struct DefenseBiases {
    pub destroyer: DefenseBias,
    pub presser: DefenseBias,
    pub warden: DefenseBias,
    pub sweeper: DefenseBias,
    pub cheat: DefenseBias,
    pub tracker: DefenseBias,
}

impl DefenseBiases {
    /// The bias for a given out-of-possession role.
    pub fn for_role(&self, role: OutOfPossessionRole) -> DefenseBias {
        match role {
            OutOfPossessionRole::Destroyer => self.destroyer,
            OutOfPossessionRole::Presser => self.presser,
            OutOfPossessionRole::Warden => self.warden,
            OutOfPossessionRole::Sweeper => self.sweeper,
            OutOfPossessionRole::Cheat => self.cheat,
            OutOfPossessionRole::Tracker => self.tracker,
        }
    }
}

/// A single agent: identity (id + display `name`), team, its coach-assigned
/// roles (`attack_role` in-possession, `defend_role` out-of-possession), where
/// it is, where it's headed, its **phase-conditioned** home anchors, how many
/// ticks it remains staggered (0 = active), its stamina (1 = fresh, draining
/// over a soul), and its attributes.
///
/// Two coach formations give each player two homes: `attack_anchor` (used while
/// its team holds the soul) and `defend_anchor` (used otherwise — enemy-held or
/// loose). `anchor` is whichever is *active* this decision window; the sim
/// reselects it at each decision boundary, so the shape morphs between phases on
/// the slow clock, never mid-motion (§1, §2).
#[derive(Debug, Clone)]
pub struct Agent {
    pub id: u32,
    pub name: String,
    pub team: u8,
    pub attack_role: InPossessionRole,
    pub defend_role: OutOfPossessionRole,
    pub pos: Vec2,
    pub target: Vec2,
    pub anchor: Vec2,
    pub attack_anchor: Vec2,
    pub defend_anchor: Vec2,
    pub stagger: u32,
    pub stamina: Fx,
    pub attributes: Attributes,
}

impl Agent {
    /// Select the home anchor for this decision window: the attacking shape when
    /// `in_possession` (my team holds the soul), the defending shape otherwise.
    /// Called at each decision boundary so the shape transitions cleanly.
    pub fn apply_phase(&mut self, in_possession: bool) {
        self.anchor = if in_possession {
            self.attack_anchor
        } else {
            self.defend_anchor
        };
    }
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
    /// A pass in flight, homing toward the agent with this id. The outcome is
    /// decided at release: `to` is the receiver (`intercepted` false) or the
    /// intercepting defender (`intercepted` true).
    InFlight { to: u32, intercepted: bool },
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

/// Build both teams in formation: team 0 from the base anchors, team 1 mirrored
/// across x. Every agent starts *on* its anchor (a consistent faceoff for every
/// soul — no random scatter). Ids index into the returned vec (team 0 first).
///
/// This is the default, RNG-rolled roster ([`Simulation::new`](crate::Simulation::new));
/// an *authored* roster comes through [`crate::setup::build_agents`].
pub fn build_two_teams(base: &Formation, rng: &mut Rng) -> Vec<Agent> {
    let mut agents = Vec::with_capacity(base.anchors.len() * 2);
    for &anchor in &base.anchors {
        push_agent(&mut agents, 0, anchor, rng);
    }
    for &anchor in &base.anchors {
        push_agent(&mut agents, 1, Vec2::new(-anchor.x, anchor.y), rng);
    }
    agents
}

fn push_agent(agents: &mut Vec<Agent>, team: u8, anchor: Vec2, rng: &mut Rng) {
    let id = agents.len() as u32;
    agents.push(Agent {
        id,
        name: format!("P{id}"),
        team,
        attack_role: InPossessionRole::default(),
        defend_role: OutOfPossessionRole::default(),
        pos: anchor,
        target: anchor,
        anchor,
        // The default teams use one formation, so both phase homes coincide —
        // play is identical to the pre-phase behavior.
        attack_anchor: anchor,
        defend_anchor: anchor,
        stagger: 0,
        stamina: Fx::from_num(1),
        attributes: Attributes::random(rng),
    });
}
