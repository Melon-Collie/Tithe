# Tithe

A pure-manager sports sim for an invented ball sport — box-lacrosse bones under a fire-and-souls fiction. You never control a player: you build a roster of distinctive athletes, scout their shapes, develop them by how you deploy them, and coach a tactical identity that a fast, deterministic sim turns into something that *narrates as a sport*. The sport and the game share the name — to **tithe** is to carry a soul-flame home and offer it to your patron's fire.

The canonical design source is [`fantasy-sport-sim-design.md`](./fantasy-sport-sim-design.md).

## Status

The headless Rust sim core plays the invented sport **AI-vs-AI end to end** — faceoff → strip → EV carry/pass/shoot → wind-up offering → first-to-X — on a shared value field with per-player attributes, zonal defense, and a hex-grid positioning model that narrates as a sport. Around it: a command-line consumer (`tithe-cli`) for HTML replays and play-by-play, a local web watch-view (`tithe-web`), measurement tools for balancing the player attributes, and a v1 AI coach that fields a squad into a formation. The sport itself is tuned and playable; the full management game, the polished front end, and desktop distribution are still open (§9).

## Stack

- **Sim core: Rust** — a standalone, headless, deterministic library with zero rendering dependencies. Serialized inputs in, an event stream out. This decoupling is the locked, load-bearing architectural decision (§9).
- **Front end: not locked** — leaning a web app (Svelte/React + a 2D canvas/PixiJS watch view), packaged for desktop via Tauri. An all-Rust `egui` alternative is held in reserve. Everything front-end is reversible behind the sim boundary.
- **Distribution: deferred** — desktop-first (Tauri → Steam) is the pragmatic default; a hosted browser build stays supportable. The architecture preserves both.

Development model: **AI-authored, human-directed and human-reviewed.** The stack was chosen for reviewability and machine-checkable correctness over authoring velocity (§9).

## Layout

A Cargo workspace; crates live under `crates/`. The sim is pure; everything else is a consumer of its event stream.

```
crates/
  tithe-sim/   # headless, deterministic sim core — zero rendering deps
  tithe-cli/   # command-line consumers (replays, play-by-play, balance tools)
  tithe-web/   # local web watch-view: edit formations, run the sim, watch it
```

## Building

Requires the Rust toolchain ([rustup](https://rustup.rs/)); `rust-toolchain.toml` pins the channel and components.

```
cargo test --workspace      # sim + golden-seed determinism tests
cargo clippy --workspace --all-targets   # lints (a clean build is zero warnings)
cargo fmt --all             # format
```

CI (`.github/workflows/test.yml`) runs all three on every push and PR, treating warnings as errors.

## Usage

The CLI (`cargo run -p tithe-cli -- <command>`, or `tithe <command>` once built) consumes the sim's event stream:

| Command | What |
|---|---|
| `play` | run one match and write a self-contained HTML replay you open in a browser |
| `log` / `box` | narrated play-by-play / per-player box score for one match |
| `stats` | batch AI-vs-AI run with tuning metrics |
| `validate --attr <name>` | controlled A/B measuring how much one attribute affects winning |
| `possession` / `tournament` | possession→win correlation / archetype round-robin balance test |
| `coach` | the v1 AI coach: fit a squad into a formation and play it |
| `init` | write an editable example match-setup file |

The web watch-view (edit both teams' formations, run the sim, watch a canvas replay):

```
cargo run -p tithe-web      # then open http://127.0.0.1:8770/
```

