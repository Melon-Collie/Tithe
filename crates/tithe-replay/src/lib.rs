//! Render-boundary export: turn a [`MatchSetup`] + seed into the JSON the canvas
//! viewer/editor consume.
//!
//! A consumer of the sim's event stream (CLAUDE.md → Committed architecture),
//! shared by every front end that drives the web UI — the local dev server
//! (`tithe-web`, native) and the in-browser build (`tithe-wasm`, WASM). The sim
//! is a pure function of `(inputs, seed)`, so all targets produce identical
//! output; only the "call the sim" glue differs. Floats appear here at the
//! boundary (`Fx` → `f32`), never in `tithe-sim`.

use serde::Serialize;
use std::collections::BTreeMap;
use tithe_sim::{
    Board, Event, InPossessionRole, MatchSetup, OutOfPossessionRole, Possession, SetupError,
    SimConfig, Simulation, Vec2,
};

/// Hard ceiling on a run, so a pathological setup can't spin forever.
pub const MAX_TICKS_CAP: u64 = 200_000;

/// The full per-match playback export the viewer renders.
#[derive(Serialize)]
pub struct MatchExport {
    pub arena: Arena,
    pub board: BoardExport,
    pub souls_to_win: u32,
    pub agents: Vec<AgentMeta>,
    pub frames: Vec<Frame>,
    pub events: Vec<MatchEvent>,
    pub winner: Option<u8>,
}

/// Static view data the editor needs to *draw* (not part of the wire setup): the
/// hex board and the per-role footprint shapes (§14). Read straight off
/// [`SimConfig::default`] — the sim stays the source of truth for the shapes.
#[derive(Serialize)]
pub struct Meta {
    pub arena: Arena,
    pub board: BoardExport,
    /// In-possession role → `[half_x, half_y, lean]`, keyed by the role's serde
    /// name (snake_case) so the editor can look it up by the same string.
    pub attack: BTreeMap<String, [f32; 3]>,
    /// Out-of-possession role → `[half_x, half_y, lean]`.
    pub defend: BTreeMap<String, [f32; 3]>,
}

/// The hex board for the viewer/editor (§14): hex size + the in-bounds cells,
/// each as its axial coords and field center (the editor places players by hex).
#[derive(Serialize)]
pub struct BoardExport {
    pub hex_size: f32,
    pub hexes: Vec<HexCell>,
}

#[derive(Serialize)]
pub struct HexCell {
    pub q: i32,
    pub r: i32,
    pub x: f32,
    pub y: f32,
}

/// A narrated play-by-play line, tagged with its tick (for syncing to playback),
/// a `kind` (for colour), and the field position where it happened (for the
/// viewer's on-canvas flash).
#[derive(Serialize)]
pub struct MatchEvent {
    pub tick: u64,
    pub kind: &'static str,
    pub text: String,
    pub pos: [f32; 2],
}

#[derive(Serialize)]
pub struct Arena {
    pub half_x: f32,
    pub half_y: f32,
    pub goal_x: f32,
}

#[derive(Serialize)]
pub struct AgentMeta {
    pub id: u32,
    pub number: u32,
    pub name: String,
    pub team: u8,
    pub attack_role: InPossessionRole,
    pub defend_role: OutOfPossessionRole,
    pub attrs: AgentAttrs,
}

/// A player's attributes as 0..=100 percentiles, for the viewer's hover panel.
#[derive(Serialize)]
pub struct AgentAttrs {
    pub acc: u32,
    pub rng: u32,
    pub han: u32,
    pub str: u32,
    pub con: u32,
    pub pas: u32,
    pub pos: u32,
    pub pace: u32,
    pub awr: u32,
}

#[derive(Serialize)]
pub struct Frame {
    pub tick: u64,
    pub soul: [f32; 2],
    pub carrier: Option<u32>,
    pub score: [u32; 2],
    pub agents: Vec<AgentFrame>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scored: Option<u8>,
}

#[derive(Serialize)]
pub struct AgentFrame {
    pub x: f32,
    pub y: f32,
    pub stagger: bool,
    pub stamina: f32,
}

fn xy(p: Vec2) -> [f32; 2] {
    [p.x.to_num::<f32>(), p.y.to_num::<f32>()]
}

/// A fixed-point attribute as a 0..=100 percentile (render boundary).
fn pct(f: tithe_sim::Fx) -> u32 {
    (f * tithe_sim::Fx::from_num(100)).to_num()
}

/// Build the board export from a [`Board`] (shared by the viewer and the editor).
fn board_export(board: &Board) -> BoardExport {
    BoardExport {
        hex_size: board.size().to_num(),
        hexes: board
            .cells()
            .iter()
            .map(|&h| {
                let c = board.center(h);
                HexCell {
                    q: h.q,
                    r: h.r,
                    x: c.x.to_num(),
                    y: c.y.to_num(),
                }
            })
            .collect(),
    }
}

/// The static editor draw-data (board + per-role footprint shapes).
pub fn meta() -> Meta {
    let cfg = SimConfig::default();
    let board = Board::oval(cfg.hex_size, cfg.arena_half_x, cfg.arena_half_y);
    let fp = |f: tithe_sim::Footprint| {
        [
            f.half_x.to_num::<f32>(),
            f.half_y.to_num::<f32>(),
            f.lean.to_num::<f32>(),
        ]
    };
    // Keys must match the enums' serde rename (snake_case) — the same strings the
    // roster dropdowns and AgentMeta use.
    let mut attack = BTreeMap::new();
    for (key, role) in [
        ("box_to_box", InPossessionRole::BoxToBox),
        ("roamer", InPossessionRole::Roamer),
        ("playmaker", InPossessionRole::Playmaker),
        ("outlet", InPossessionRole::Outlet),
        ("finisher", InPossessionRole::Finisher),
    ] {
        attack.insert(key.to_string(), fp(cfg.on_ball_bias(role).footprint));
    }
    let mut defend = BTreeMap::new();
    for (key, role) in [
        ("destroyer", OutOfPossessionRole::Destroyer),
        ("presser", OutOfPossessionRole::Presser),
        ("warden", OutOfPossessionRole::Warden),
        ("sweeper", OutOfPossessionRole::Sweeper),
        ("cheat", OutOfPossessionRole::Cheat),
        ("tracker", OutOfPossessionRole::Tracker),
    ] {
        defend.insert(key.to_string(), fp(cfg.defense_bias(role).footprint));
    }
    Meta {
        arena: Arena {
            half_x: cfg.arena_half_x.to_num(),
            half_y: cfg.arena_half_y.to_num(),
            goal_x: cfg.goal_x.to_num(),
        },
        board: board_export(&board),
        attack,
        defend,
    }
}

/// Build the playback export by running the authored matchup to a winner (or the
/// tick cap), snapshotting one [`Frame`] per tick. `max_ticks` is clamped to
/// [`MAX_TICKS_CAP`]. Errors are authoring mistakes in the setup.
pub fn build_export(
    setup: &MatchSetup,
    seed: u64,
    max_ticks: u64,
) -> Result<MatchExport, SetupError> {
    let max_ticks = max_ticks.min(MAX_TICKS_CAP);
    let mut sim = Simulation::from_setup(setup, seed)?;

    let cfg = sim.config();
    let arena = Arena {
        half_x: cfg.arena_half_x.to_num(),
        half_y: cfg.arena_half_y.to_num(),
        goal_x: cfg.goal_x.to_num(),
    };
    let souls_to_win = cfg.souls_to_win;
    let goals = sim.config().goals();

    let board = board_export(&sim.board());

    let names: Vec<String> = sim.agents().iter().map(|a| a.name.clone()).collect();
    let agent_teams: Vec<u8> = sim.agents().iter().map(|a| a.team).collect();
    let mut agents = Vec::new();
    let mut numbers = [0u32, 0u32]; // per-team jersey counters
    for a in sim.agents() {
        numbers[a.team as usize] += 1;
        let at = &a.attributes;
        agents.push(AgentMeta {
            id: a.id,
            number: numbers[a.team as usize],
            name: a.name.clone(),
            team: a.team,
            attack_role: a.attack_role,
            defend_role: a.defend_role,
            attrs: AgentAttrs {
                acc: pct(at.accuracy),
                rng: pct(at.range),
                han: pct(at.handling),
                str: pct(at.stripping),
                con: pct(at.contesting),
                pas: pct(at.passing),
                pos: pct(at.positioning),
                pace: pct(at.pace),
                awr: pct(at.awareness),
            },
        });
    }

    let mut frames = Vec::new();
    let mut narrated: Vec<MatchEvent> = Vec::new();
    let mut soul_no = 0u32;
    let mut ticks = 0;
    while sim.winner().is_none() && ticks < max_ticks {
        let events = sim.tick();
        ticks += 1;
        let tick = sim.tick_count();
        let scored = events.iter().find_map(|e| match e {
            Event::Scored { team, .. } => Some(*team),
            _ => None,
        });
        for ev in &events {
            // A new soul (opening kickoff or after a bank) numbers the round.
            if matches!(ev, Event::NewSoul) {
                soul_no += 1;
                narrated.push(MatchEvent {
                    tick,
                    kind: "soul",
                    text: format!("Soul {soul_no} begins"),
                    pos: xy(sim.soul().pos),
                });
                continue;
            }
            if let Some((kind, text)) = narrate(ev, &names, &agent_teams) {
                // Where to flash: the goal on a score (the soul has already reset
                // to center by now), the shooter's spot on a miss (where he took
                // it — telling for perimeter bombs), the ball's spot otherwise.
                let pos = match ev {
                    Event::OfferingResolved {
                        carrier,
                        scored: true,
                        ..
                    } => xy(goals[sim.agents()[*carrier as usize].team as usize]),
                    Event::OfferingResolved { carrier, .. } => {
                        xy(sim.agents()[*carrier as usize].pos)
                    }
                    _ => xy(sim.soul().pos),
                };
                narrated.push(MatchEvent {
                    tick,
                    kind,
                    text,
                    pos,
                });
            }
        }
        frames.push(Frame {
            tick: sim.tick_count(),
            soul: xy(sim.soul().pos),
            carrier: match sim.soul().possession {
                Possession::Held(id) => Some(id),
                Possession::Loose | Possession::InFlight { .. } => None,
            },
            score: sim.score(),
            agents: sim
                .agents()
                .iter()
                .map(|a| AgentFrame {
                    x: a.pos.x.to_num(),
                    y: a.pos.y.to_num(),
                    stagger: a.stagger > 0,
                    stamina: a.stamina.to_num(),
                })
                .collect(),
            scored,
        });
    }

    Ok(MatchExport {
        arena,
        board,
        souls_to_win,
        agents,
        frames,
        events: narrated,
        winner: sim.winner(),
    })
}

/// The team accent colour, for tinting names in the play-by-play.
fn team_color(team: u8) -> &'static str {
    if team == 0 {
        "#e8643c"
    } else {
        "#4c9be8"
    }
}

/// A player's name wrapped in a team-coloured span (the ticker renders it as
/// HTML); inline colour so it beats the line's kind colour.
fn who(names: &[String], teams: &[u8], id: u32) -> String {
    let name = names.get(id as usize).map_or("?", String::as_str);
    let team = teams.get(id as usize).copied().unwrap_or(0);
    format!("<span style=\"color:{}\">{name}</span>", team_color(team))
}

/// Narrate a notable event for the play-by-play ticker — `(kind, text)`, or
/// `None` for the per-tick noise (positions, routine pickups, whiffed strips).
/// `NewSoul` is handled by the caller (it numbers the round).
fn narrate(event: &Event, names: &[String], teams: &[u8]) -> Option<(&'static str, String)> {
    let who = |id| who(names, teams, id);
    match event {
        Event::SoulClaimed { agent } => Some(("info", format!("{} gathers it", who(*agent)))),
        Event::PassMade { from, to, chance } => {
            Some(("pass", format!("{} → {} ({chance}%)", who(*from), who(*to))))
        }
        Event::PassIntercepted { by } => {
            Some(("turnover", format!("↳ intercepted by {}!", who(*by))))
        }
        Event::StripAttempt {
            defender,
            carrier,
            chance,
            success: true,
        } => Some((
            "turnover",
            format!("{} strips {} ({chance}%)", who(*defender), who(*carrier)),
        )),
        Event::OfferingResolved {
            carrier,
            chance,
            scored,
        } => Some(if *scored {
            ("goal", format!("⚑ {} SCORES ({chance}%)", who(*carrier)))
        } else {
            (
                "shot",
                format!("{} offers ({chance}%) — no good", who(*carrier)),
            )
        }),
        Event::MatchOver { winner } => Some(("end", format!("FINAL — team {winner} wins"))),
        _ => None,
    }
}
