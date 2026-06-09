# Tithe

A pure-manager sports sim for an invented ball sport — box-lacrosse bones under a fire-and-souls fiction. You never control a player: you build a roster of distinctive athletes, scout their shapes, develop them by how you deploy them, and coach a tactical identity that a fast, deterministic sim turns into something that *narrates as a sport*. The sport and the game share the name — to **tithe** is to carry a soul-flame home and offer it to your patron's fire.

The canonical design source is [`fantasy-sport-sim-design.md`](./fantasy-sport-sim-design.md).

## Status

Design phase — no code yet. The design is unusually complete; implementation begins with a headless Rust sim-core prototype to prove the decoupling and the fun.

## Stack

- **Sim core: Rust** — a standalone, headless, deterministic library with zero rendering dependencies. Serialized inputs in, an event stream out. This decoupling is the locked, load-bearing architectural decision (§9).
- **Front end: not locked** — leaning a web app (Svelte/React + a 2D canvas/PixiJS watch view), packaged for desktop via Tauri. An all-Rust `egui` alternative is held in reserve. Everything front-end is reversible behind the sim boundary.
- **Distribution: deferred** — desktop-first (Tauri → Steam) is the pragmatic default; a hosted browser build stays supportable. The architecture preserves both.

Development model: **AI-authored, human-directed and human-reviewed.** The stack was chosen for reviewability and machine-checkable correctness over authoring velocity (§9).
