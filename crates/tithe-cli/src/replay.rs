//! `play`: run one match and emit a self-contained HTML replay.
//!
//! Builds a frame per tick by snapshotting the sim's public state, converts
//! fixed-point to `f32` at this boundary, and inlines the JSON into the canvas
//! viewer template.

use serde::Serialize;
use tithe_sim::{Event, Possession, Simulation, Vec2};

const TEMPLATE: &str = include_str!("../viewer/template.html");

#[derive(Serialize)]
struct MatchExport {
    arena: Arena,
    souls_to_win: u32,
    agents: Vec<AgentMeta>,
    frames: Vec<Frame>,
    winner: Option<u8>,
}

#[derive(Serialize)]
struct Arena {
    half_x: f32,
    half_y: f32,
    goal_x: f32,
}

#[derive(Serialize)]
struct AgentMeta {
    id: u32,
    team: u8,
}

#[derive(Serialize)]
struct Frame {
    tick: u64,
    soul: [f32; 2],
    carrier: Option<u32>,
    score: [u32; 2],
    agents: Vec<AgentFrame>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scored: Option<u8>,
}

#[derive(Serialize)]
struct AgentFrame {
    x: f32,
    y: f32,
    stagger: bool,
}

fn xy(p: Vec2) -> [f32; 2] {
    [p.x.to_num::<f32>(), p.y.to_num::<f32>()]
}

fn build_export(seed: u64, max_ticks: u64) -> MatchExport {
    let mut sim = Simulation::new(seed);

    let cfg = sim.config();
    let arena = Arena {
        half_x: cfg.arena_half_x.to_num(),
        half_y: cfg.arena_half_y.to_num(),
        goal_x: cfg.goal_x.to_num(),
    };
    let souls_to_win = cfg.souls_to_win;

    let agents = sim
        .agents()
        .iter()
        .map(|a| AgentMeta {
            id: a.id,
            team: a.team,
        })
        .collect();

    let mut frames = Vec::new();
    let mut ticks = 0;
    while sim.winner().is_none() && ticks < max_ticks {
        let events = sim.tick();
        ticks += 1;
        let scored = events.iter().find_map(|e| match e {
            Event::Scored { team, .. } => Some(*team),
            _ => None,
        });
        frames.push(Frame {
            tick: sim.tick_count(),
            soul: xy(sim.soul().pos),
            carrier: match sim.soul().possession {
                Possession::Held(id) => Some(id),
                // In flight: nobody holds it (the soul dot animates between players).
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
                })
                .collect(),
            scored,
        });
    }

    MatchExport {
        arena,
        souls_to_win,
        agents,
        frames,
        winner: sim.winner(),
    }
}

pub fn run(args: &[String]) {
    let seed = crate::flag_or(args, "--seed", 1u64);
    let out = crate::flag(args, "--out").unwrap_or_else(|| "match.html".to_string());
    let max_ticks = crate::flag_or(args, "--max-ticks", 100_000u64);

    let export = build_export(seed, max_ticks);
    let json = serde_json::to_string(&export).expect("serialize match");
    let html = TEMPLATE.replace("__MATCH_DATA__", &json);
    std::fs::write(&out, html).expect("write output file");

    println!(
        "wrote {out}: {} frames, score {:?}, winner {:?}",
        export.frames.len(),
        export.frames.last().map(|f| f.score).unwrap_or([0, 0]),
        export.winner,
    );
}
