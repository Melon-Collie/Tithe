//! WASM bindings — run the sim in the browser, no server.
//!
//! The in-browser front-end target (CLAUDE.md → the sim's native+WASM duality:
//! "browser-vs-desktop is a packaging choice, not a rewrite"). Exposes the same
//! three calls the dev server (`tithe-web`) serves, but the sim runs *in-process*
//! in the page — same [`tithe_replay`] export, JSON across the JS boundary. The
//! sim is a pure function of `(inputs, seed)`, so this produces byte-identical
//! results to the native server.

use tithe_sim::MatchSetup;
use wasm_bindgen::prelude::*;

/// Run an authored matchup (a `MatchSetup` as JSON) and return the playback
/// export as JSON. `seed` drives all match dynamics; `max_ticks` is clamped to a
/// safety ceiling. Errors are authoring mistakes in the setup or bad JSON.
#[wasm_bindgen]
pub fn run_match(setup_json: &str, seed: u64, max_ticks: u64) -> Result<String, JsValue> {
    let setup: MatchSetup = serde_json::from_str(setup_json).map_err(err)?;
    let export = tithe_replay::build_export(&setup, seed, max_ticks).map_err(err)?;
    serde_json::to_string(&export).map_err(err)
}

/// The editable starting matchup, as JSON (the same authored example as
/// `tithe init` / the dev server's `/api/default-setup`).
#[wasm_bindgen]
pub fn default_setup() -> Result<String, JsValue> {
    serde_json::to_string(&MatchSetup::default_match()).map_err(err)
}

/// The static editor draw-data — hex board + per-role footprint shapes — as JSON
/// (the dev server's `/api/meta`).
#[wasm_bindgen]
pub fn meta() -> Result<String, JsValue> {
    serde_json::to_string(&tithe_replay::meta()).map_err(err)
}

fn err<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from_str(&e.to_string())
}
