//! `tithe-web` — a local dev front end: edit both teams' formations in the
//! browser, run the **native** sim, and watch the result.
//!
//! It is a consumer of the sim's event stream (CLAUDE.md → Committed
//! architecture), one of several swappable front ends. The browser POSTs a
//! [`MatchSetup`] to `/api/run`; this server builds and runs the sim and returns
//! the per-tick frames as JSON for the canvas viewer. Floats live here at the
//! render boundary (Fx → f32), never in `tithe-sim`.
//!
//! This server transport is intentionally thin: the same page/UI moves to a
//! WASM or Tauri host later with only the "call the sim" glue changing.

use axum::{
    extract::Json,
    http::StatusCode,
    response::Html,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tithe_sim::{
    Event, InPossessionRole, MatchSetup, OutOfPossessionRole, Possession, Simulation, Vec2,
};

/// The single-page UI (formation editor + playback viewer), served at `/`.
const INDEX_HTML: &str = include_str!("../web/index.html");

/// Hard ceiling on a run, so a pathological setup can't spin forever.
const MAX_TICKS_CAP: u64 = 200_000;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(index))
        .route("/api/default-setup", get(default_setup))
        .route("/api/run", post(run_match));

    let addr = "127.0.0.1:8770";
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {addr}: {e}"));
    println!("tithe-web running — open http://{addr}/ in your browser");
    axum::serve(listener, app).await.expect("server error");
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

/// The editable starting point: the same authored example as `tithe init`.
async fn default_setup() -> Json<MatchSetup> {
    Json(MatchSetup::default_match())
}

/// Body of `POST /api/run`: an authored matchup plus the run parameters.
#[derive(Deserialize)]
struct RunRequest {
    setup: MatchSetup,
    seed: u64,
    max_ticks: u64,
}

/// Run the authored matchup and return the frames, or `400` with the authoring
/// error if the setup is malformed.
async fn run_match(Json(req): Json<RunRequest>) -> Result<Json<MatchExport>, (StatusCode, String)> {
    let max_ticks = req.max_ticks.min(MAX_TICKS_CAP);
    let export = build_export(&req.setup, req.seed, max_ticks)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(export))
}

// ---- Render-boundary export (mirrors tithe-cli's replay export) -------------

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
    name: String,
    team: u8,
    attack_role: InPossessionRole,
    defend_role: OutOfPossessionRole,
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
    stamina: f32,
}

fn xy(p: Vec2) -> [f32; 2] {
    [p.x.to_num::<f32>(), p.y.to_num::<f32>()]
}

/// Build the playback export by running the authored matchup to a winner (or the
/// tick cap), snapshotting one [`Frame`] per tick.
fn build_export(
    setup: &MatchSetup,
    seed: u64,
    max_ticks: u64,
) -> Result<MatchExport, tithe_sim::SetupError> {
    let mut sim = Simulation::from_setup(setup, seed)?;

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
            name: a.name.clone(),
            team: a.team,
            attack_role: a.attack_role,
            defend_role: a.defend_role,
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
        souls_to_win,
        agents,
        frames,
        winner: sim.winner(),
    })
}
