//! `tithe-web` — a local **dev** front end: edit both teams' formations in the
//! browser, run the native sim, and watch the result.
//!
//! A thin axum wrapper over [`tithe_replay`] (the shared render-boundary export).
//! The browser POSTs a [`MatchSetup`]; this server builds and runs the sim and
//! returns the playback JSON. It's a dev convenience — the *distributed* front
//! ends run the sim in-process (WASM in the browser, a Tauri command on
//! desktop), so only the "call the sim" glue differs (see `tithe-wasm`).

use axum::{
    extract::Json,
    http::StatusCode,
    response::Html,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use tithe_replay::{build_export, meta, MatchExport, Meta};
use tithe_sim::MatchSetup;

/// The single-page UI (formation editor + playback viewer), served at `/`.
const INDEX_HTML: &str = include_str!("../web/index.html");

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(index))
        .route("/api/default-setup", get(default_setup))
        .route("/api/meta", get(api_meta))
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

/// Static editor draw-data (board + per-role footprint shapes).
async fn api_meta() -> Json<Meta> {
    Json(meta())
}

/// Body of `POST /api/run`: an authored matchup plus the run parameters.
#[derive(Deserialize)]
struct RunRequest {
    setup: MatchSetup,
    seed: u64,
    max_ticks: u64,
}

/// Run the authored matchup and return the playback, or `400` with the authoring
/// error if the setup is malformed.
async fn run_match(Json(req): Json<RunRequest>) -> Result<Json<MatchExport>, (StatusCode, String)> {
    let export = build_export(&req.setup, req.seed, req.max_ticks)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(export))
}
