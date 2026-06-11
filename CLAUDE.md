# Tithe

A pure-manager sim for an invented ball sport — box-lacrosse bones under a fire-and-souls fiction. You never control a player; you build a roster, scout shapes, develop by deployment, and coach a tactical identity that a fast, deterministic sim narrates as a sport. The whole input space is **two coach inputs (positioning templates + role assignments) plus the players' stats.**

The canonical design source is [`fantasy-sport-sim-design.md`](./fantasy-sport-sim-design.md). Open forks live there under **§13 Open / parked questions** — don't duplicate or re-litigate them here; when one is settled, it's settled in the doc. Section references below (§9, §10, etc.) point into that doc.

## Status

The headless Rust sim core (`crates/tithe-sim`) and a CLI consumer (`crates/tithe-cli` — HTML replay, AI-vs-AI stats batch, play-by-play log) are built: the invented sport plays AI-vs-AI end to end (faceoff → strip → EV carry/pass → wind-up offering → first-to-X), on a shared value field with per-player attributes and zonal defense. **The code and its tests are the authority on how it behaves;** this file and the design doc capture *intent and rationale*. Front end and distribution are still unbuilt and open.

## Source of truth

Code and its tests are the only authority on how the system *currently behaves*. Every doc here — this file, the design doc, memory, any plan — captures **intent, rationale, and invariants**, never a description of current implementation. **When a doc and the code disagree, the code wins** and the doc is stale (fix it or delete it). Before relying on any doc's claim about how something works, **verify it in the code/tests.** Never audit code against a plan or the design doc as if the doc were ground truth.

Consequences for how we work (learned the hard way on a sibling project where stale plan docs misled audits):
- **No standing roadmap/plan doc.** Sequencing lives in an ephemeral task list that *shrinks* as work completes — a finished task is removed, never left as "done" to diverge from what the code became.
- **Tests are the living spec.** The sim is pure and headlessly testable, so behavior is pinned by tests that fail when they rot. Prefer a test over prose when documenting behavior.
- **Prune forward-looking spec as code lands.** When a section here starts describing something the code now owns, shrink it to the load-bearing invariant or delete it.
- **The design doc is a point-in-time *mindset* snapshot, not a living spec.** It has already drifted from how the code behaves, and that's expected — keeping a prose design doc in sync with bugfixes and behavior tweaks is a losing game (a pain learned the hard way). Read it for the *grain and the why*, never for what the code does. The honest division of labor: **self-documenting code** carries behavior (favor a clear name / doc-comment / test over external prose); **CLAUDE.md** carries the load-bearing constraints and the non-obvious things you couldn't reconstruct from the code; the **design doc** carries original intent.

## Workflow

- **Claude can build and test the whole sim layer headlessly.** The sim core is pure Rust with zero rendering dependencies, so `cargo build`, `cargo test`, `cargo clippy`, and `cargo fmt` all run without a front end or a windowing system. This is the primary verification loop — use it freely after touching sim/rules code, and prefer it to prose for pinning behavior.
- **Front-end / Tauri behavior is harder to verify from here.** Rendering, the watch view, the tactics canvas, and device input need the user (or a browser/desktop run). After touching front-end code, name what to verify; the user runs it and reports back. (This split exists because the watch view is *validated by eye*, not by review — §9.)
- **Push discipline.** Feature branches (e.g. `claude/*`) may be pushed after committing so the user can pull and test. **Never push `main` without the user testing locally first.** Merging a feature branch into `main` is done by the user via PR — do not `git merge` into `main` directly. For work on `main`, stop at commit and wait for explicit confirmation before `git push`.
- **If you spot a bug or smell while working on something else, flag it.** Don't silently fix it (out of scope), don't silently ignore it (it'll rot), don't tack it onto the current commit (muddies the diff). Surface it with a one-line description and let the user decide: fix now as a small follow-up, defer, or capture as a Known Issue.

## Working philosophy

**Design-led, code-authoritative.** This is a design-led project, and the doc is unusually complete on *intent* — read it for the grain before proposing a system: the pillars and cross-cutting laws (§10), the sport genome (§1), the sim model (§2). But it has drifted from how the code behaves and is **not a behavior spec** — verify behavior in the code/tests, never audit code against the doc (see Source of truth). When you build something the doc didn't anticipate, that's expected; surface the gap rather than freelancing a rule that contradicts the doc's grain.

**Complexity tolerance.** *"It's simpler"* is not, by itself, an argument. The default question is *"what gives the best feel / correctness / longevity?"* — not *"what's the smallest thing that works?"* Don't pre-emptively offer *"we could simplify for the prototype and clean up later"* without flagging it as a deviation from the principled-from-day-one stance. (The sibling projects ship genuinely complex netcode/sim by choice.) The counterweight specific to this project: **legibility is itself a design goal** (§2, §10) — complexity that the watch view can't narrate is the wrong kind. Spend novelty where it's load-bearing; borrow familiarity everywhere else (§10).

**Research-grounded design.** For any non-trivial system (the utility-AI agent model, deterministic fixed-timestep sim, formation/anchor representation, fixed-point math, player generation), look at prior art first — find what credible sources converge on, present options with the influence named, then pick. Don't invent novel architecture where prior art exists.

## Committed architecture (load-bearing — §9)

Locked at the design level. Don't violate without explicit discussion.

**The non-negotiable decision: the sim is a standalone, headless, deterministic library with zero rendering dependencies,** behind a clean interface — serialized inputs go in, an event stream comes out. One decision serves season-simming, server-authoritative multiplayer, reproducible replays, and automated testing, and it makes the front end and the distribution low-stakes and swappable. The client/server seam multiplayer needs is the same seam that makes browser-vs-desktop a packaging choice, not a rewrite.

**Rendering is just one consumer of the event stream.** The sim emits a state/event stream and never knows it's being watched: headless discards the stream, watch mode draws it, fast-forward renders every Nth tick. Never let a rendering, UI, or I/O concern reach back into the sim.

**Two timescales inside the sim (§2).** Agents *perceive and decide* on a slow clock against a coarse zone model (cheap, tactics-legible); they *execute* with continuous motion on a fast clock (reads as a sport). Committing to an intent for a window is what makes motion look like a real player and what makes a mid-game tactic change apply cleanly at the next decision boundary, never mid-motion. (Distinct from the sport's *resolution* model — souls are live-until-banked, match length is first-to-X.)

**Dumb agents on purpose (§2, §10).** One simple shared utility algorithm — no lookahead, no search. Intelligence is modeled as *input quality, not compute*: the genius perceives an accurate world and weights his role well; the rookie perceives noise. Same algorithm, different data, so mistakes *narrate* (lost his man, bit on the fake) instead of looking like bugs. **Coordination lives in the coach's formation, not in the agents** — anchors are field-relative with bounded drift; gaps emerge from the shape, and dumb agents can't paper over a bad shape, which is what makes the shape the skill. Don't push intelligence into the agents; push it into perception quality and the formation.

**The event stream is a first-class output, not a debug afterthought.** Replays, condensed games, shared-seed leagues, organic analytics, and the "why did he do that?" inspectable utility scores (§2) all fall out of seed + inputs + the stream. Treat it as load-bearing.

## Determinism rules (load-bearing — §2, §9)

The whole architecture — server-authoritative multiplayer, reproducible replays, shared-seed leagues, automated regression — rests on bit-stable simulation. These are non-negotiable in sim-core code:

- **The sim is a pure function of `(serialized inputs, seed)`.** No wall-clock, no ambient RNG, no I/O, no global state inside simulation. Same inputs + seed → same event stream, every machine, every replay.
- **All randomness is seeded and threaded explicitly.** No global/thread-local RNG; thread the seed through. Same seed → same picks (attribute-modulated perception noise included).
- **No floats in the sim — fixed-point / integer math only.** Floats drift across platforms and corrupt replays and multiplayer. Continuous quantities (position, velocity, stamina) use fixed-point; the front-end/render boundary is the *only* place floats appear. (The sibling Sports project runs its sim on Q16.16 fixed-point for exactly this reason.)
- **Iteration order is explicit and deterministic.** No iterating a `HashMap`/`HashSet` in a way that affects the event stream — use ordered collections (`BTreeMap`/`Vec`) or sort before iterating. Unordered iteration is a classic determinism break.
- **Golden-seed regression test is mandatory** (§9 review workflow): fixed seed + inputs → fixed output hash, so determinism breaks are caught automatically, not by eye. Add/extend it whenever sim behavior changes intentionally; an unexpected golden-hash change is a determinism bug until proven otherwise.

## Stack & code conventions (§9)

- **Sim core: Rust, locked.** Chosen for reviewability and for making nondeterminism *structurally* hard, not for authoring speed. Compiles to native (desktop/server) and WASM (browser/solo).
- **Write Rust in its readable, reviewable subset.** Code is AI-authored, human-reviewed at module boundaries — so **no macro/lifetime/trait cleverness in the sim.** Idiomatic, documented, explicit, typed code that reads top-to-bottom. A strict compiler is a second reviewer; don't defeat it with `unsafe`, and justify any `unsafe` explicitly. Prefer clarity the user can audit over cleverness that saves a few lines.
- **Warnings are errors / a clean build is zero warnings.** Fix the underlying issue rather than `#[allow(...)]`-suppressing; justify any allow.
- **`cargo fmt` + `cargo clippy` clean.** Run both before considering work done; default to clippy's guidance unless there's a determinism reason not to.
- **Front end: not locked.** Leaning web (Svelte/React + 2D canvas/PixiJS), Tauri for desktop; `egui` held in reserve. It's validated by eye, so its language matters far less than the sim's — and it stays entirely behind the sim boundary. Don't bake front-end assumptions into the sim or its interface.
- **Get the mechanic working, then tune numbers.** The doc's numbers (team size ~7, X ~11 souls, ~16–32 games) are initial tuning dials set by prototype + AI-vs-AI sim, *not* commitments — keep them in data/config, not baked into logic prose.

## Design laws (drift-checks — §10)

Use these to sanity-check any proposed feature or implementation (full text in §10):

1. **Every rule earns its place by four tests:** it forces a coaching decision, it creates a conservation tradeoff (cover A → concede B), it's readable in the watch view, and it's cheap in the sim.
2. **Magic embodies rules, never decorates them.** The soul *is* the ball *is* the offering; the anti-loiter aura is *why* nobody camps the goal. Atmosphere never touches the cap, the sim, or competitive balance — **favor is prestige, never power** (§12).
3. **Fog perturbs magnitude, never shape (§4).** Scouts always see *what kind* of player he is; what's fuzzy is how tall the bars are. You bet on degree, not kind.
4. **Intelligence is input quality, not compute.** Dumb-but-faithful agents maximize the signal between the coach's decisions and the result.
5. **Coordination lives in the coach's formation, not in the agents.** The genius layer belongs to the player (you); agents just execute.
6. **Borrow familiarity, spend novelty where it's load-bearing** — the sport genome and the (parked) species choice alike.
7. **Anything load-bearing is scoutable; anything not is cut (§4).** No hidden-but-load-bearing attributes; no second-class attributes.
8. **Diagnosis resolves to move / recast / upgrade (§3).** Wrong spot, wrong role, or wrong player.
9. **Teams differ only by personnel (§7).** Finances are maximally simplified — money has exactly two uses, players and player futures. No business sim, no fiction-layer power.

## What goes in this file vs. elsewhere

CLAUDE.md is for **load-bearing constraints that prevent the wrong move**: workflow rules, architectural invariants, determinism rules, design laws, conventions. Things *true and persistent* about the project.

Does **not** belong here:
- **Status snapshots** (test counts, "currently building X", milestone-in-progress) — git, tests, and the design doc are authoritative; status lines rot by the next commit. (The pre-implementation notice above is the one temporary exception, deleted when code lands.)
- **Milestone direction / ordering and open design questions** — that's design-doc territory (`fantasy-sport-sim-design.md`, §13).
- **Anything derivable from `git log`, the file tree, or running the tests.**
