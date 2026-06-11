# Tithe — Design Doc

> **Intent/mindset snapshot, not a behavior spec.** This captures the original design grain and the *why*; it has drifted from how the code now behaves and isn't kept in lockstep with it. The code and its tests in `crates/tithe-sim` are authoritative on behavior — verify there, never audit code against this doc. See `CLAUDE.md` → *Source of truth*.

*Working draft. **Tithe** is a pure-manager sim for an invented ball sport — box-lacrosse bones under a fire-and-souls fiction — where the whole game is reading shapes, building identity, and out-coaching the league. The sport and the game share the name.*

---

## 0. Concept in one breath

You never control a player. You build a roster of distinctive athletes, scout their shapes and trajectories, develop them by how you deploy them, and coach a tactical identity that a fast, watchable, deterministic sim turns into something that *narrates as a sport*. The sport is **Tithe** — carry a soul-flame home and offer it to your patron's fire — and the name is the core verb: you *tithe* souls to win. Every save is a blank-slate league genesis. The bread and butter is tactics; the heart is the players you invest in.

---

## 1. The Sport

**Box lacrosse's skeleton, with the scoring model swapped and a fire-fiction overcoat.** One-line spec for a hockey/lacrosse person: *box lacrosse, but instead of beating a goalie at the far net you carry the soul home and offer it to your own fire — and you win a race to X souls, with no clock at all.* The novelty budget goes to the scoring model, the race-to-X length, and the fiction; everything else stays familiar so it reads instantly as a sport.

**Arena & flow**
- Large walled **oval**, no out-of-bounds, play caroms off the boards, continuous flow, no big stoppages. Visible zone bands (the sim's discretization made diegetic — the tactics canvas is the field markings).
- **~7 a side.** **Substitutions happen between souls, not during play** — each soul is a round, and the break between souls is the manager's decision beat (set the unit, tweak tactics). A unit is committed to a whole soul, so a tired unit can get caught grinding a long one. (Between-round, not "rolling.")

**Object, carry & defense**
- The ball is a **soul** — a flame cradled in a **torch-crosse** (a lacrosse stick that carries fire). Cradle to retain, pass as arcs of flame.
- **Defense is strip-based — the codified challenge:** a committed lunge with explicit outcomes — win = clean possession, whiff = a short, visible beaten/stagger. With no goalie, winning the ball back *is* the defensive game.

**Scoring — the offering (no goalie; a skill check)**
- **Omnidirectional floating goals, one per team** — a suspended, 360°-scorable fire. With no front, it can't be guarded by positioning — hence no goalie, and hence defense must strip the carrier. Allows wraparound / feed-from-behind offerings.
- Score by **offering** the soul into *your own* home goal: a basketball-style skill check, not beating a keeper. **Touch-in** (drive close, near-automatic, but exposed to strips the whole way) or **cast from range** (Finishing-gated; commits before defenders close; high-Finishing players unlock long/angled offerings).
- Defense contests by **arriving** (the aura forbids camping): a recovered defender lunges during the offering's wind-up — strip it and the soul **reverses** (they carry the other way), harry it and the odds drop, fail to arrive and it's a clean score. 2-on-1 logic: clean breaks score, recovered defense contests.
- **Missed offering** (leaning): the fire rejects it and spits it back into open play — no cheap put-back, a fresh scramble. *(Alt under consideration: a loose rebound.)*

**Direction & possession**
- Souls are neutral and contested; the holder attacks *toward their own home goal*, and direction flips on every turnover (transition like basketball/hockey, just aimed home). Topologically a standard invasion-transition sport with goal ownership inverted — familiar to play and watch, fresh to experience; the inversion's value is fictional (scoring = an offering to your patron).

**Resolution & match length (no clock anywhere)**
- **Souls are live until banked** — like a volleyball rally, a soul has no timer; it stays in play until someone offers it. You can't stall, because you must score to win (no clock, no draws), so holding only delays your own victory.
- **Stamina is the forcing function.** A soul that rages on gasses both sides (no mid-soul subs), and a tired defense eventually concedes a clean break — which scores. The longer a soul runs uncontained, the more inevitable a bank, so fatigue ends rallies the way the floor does in volleyball. No artificial per-soul timer needed. (Residual: convergence is statistical, not guaranteed — an elite-D-vs-elite-D grind is the pathology to watch for in AI-vs-AI testing; tune break-conversion/stamina-drain up if it appears, with a very-long soft cap held in reserve.)
- **Match length = first to X souls** — volleyball-bounded at ~2X souls, each resolving in a score. No game clock means no stalling, no garbage time, alive until the opponent reaches X (comeback-friendly), and the game ends exactly when it's decided. Win-by-1 (crisp) or win-by-2 with a ceiling (wire drama).

**Fiction framing** — magic *embodies* rules, never decorates: the soul is the ball is the offering; the torch is how you hold fire; the goal's anti-loiter aura is why nobody camps it. Self-justifies the league — no realistic-but-nonsensical uncanny valley.

**Open / tuning dials** (set by prototype + AI-vs-AI sim, not on paper): **team size** is really a *density* dial (team size × arena size) — tune it for watch-legibility and off-ball-AI tractability, lean ~7 (odd — kills the lazy 3-3 / 2-2-2 mirrors, raises the formation floor to a three-band-with-spine, and creates a recurring "where does the +1 go" spare-man decision; cost is legibility/AI, the prototype arbitrates, drop to 6 if the watch view muddies); **X (souls to win)** is the *game-length* dial (X × average soul duration ≈ match time), lean ~11. Plus: arena dimensions and zone banding; how a missed offering resolves; whether a cast flame is contestable in flight; zone-occupancy laws and other balancing instruments, added only if the sim shows a dominant shape. (The earlier "entry gate" is dropped unless a zone rule revives it.)

---

## 2. The Sim

The make-or-break pillar. Success bar: **the output narrates as play, not as graphs.** If a viewer can say "he pressured, forced the turnover, they broke out weak-side for a 3-on-2," it's a sport.

- **Deterministic, seeded, event-stream sim.** Fixed timestep, seeded RNG; the engine emits a state/event stream and rendering is just one consumer. Headless discards the stream; watch mode draws it; fast-forward renders every Nth tick. The sim never knows it's being watched.
- **Coarse mind, continuous body (two timescales).** Agents *perceive and decide* on a slow clock against a coarse zone model (cheap, tactics-legible); they *execute* with continuous motion on a fast clock (looks like a sport). Committing to an intent for a window is what makes motion read as a real player — and it's why a mid-game tactic change applies cleanly at the next decision boundary rather than mid-motion. (Distinct from the sport's resolution model in §1 — souls are live-until-banked, match length is first-to-X.)
- **Dumb agents on purpose.** One simple, shared utility algorithm — no lookahead, no search. Intelligence is modeled as *input quality*, not compute: the genius perceives an accurate world and weights his role's considerations well; the rookie perceives noise and weights badly. Same algorithm, different data. Errors come from attribute-modulated perception, so mistakes *narrate* (lost his man, bit on the fake) instead of looking like bugs.
- **Two behavior modes, both dumb.** *Structured*: hold your phase-conditioned field-relative anchor, do your role's verb. *Scramble* (loose/rejected soul, broken play): greedy heuristics — chase-nearest, contest-the-carrier, collapse on the offering. No game state requires a clever agent.
- **Coordination lives in the formation, not the agents.** Anchors are **field-relative with bounded drift** (the placed grid position is home; the ball only *shades* a player off it, never makes him chase), phase-conditioned; gaps emerge from the *coach's* shape, and dumb agents can't paper over a bad shape — which is exactly what makes the shape the skill. Drift amount ("elasticity") is a per-role/zone parameter spanning low (hold the zone) to high (track a target = man-marking), so the field-relative frame subsumes ball-relative as its high-elasticity extreme and unifies the zonal/man coverage knob.
- **We don't need realism.** Tune for legibility and drama, not fidelity — a sport engineered to be watched and understood, which real sports aren't.

**The exhaust (free byproducts of a real sim):**
- **Replays, condensed games, shared-seed leagues** — all free from seed + inputs.
- **Organic analytics.** Every stat falls out of real events. An invented WAR with a *literally computable* replacement level (sim a replacement-level player in the slot, measure the delta) — something no real-sport game can do honestly.
- **Receipts culture.** Trade value ledgers ("see where you got scammed"), draft-class lookbacks, development timelines. The game keeps score on your *judgment*.
- **"Why did he do that?"** Utility scores are inspectable — click a replay moment, see what his model scored and why.

---

## 3. Tactics (the bread and butter)

Two inputs from the coach, plus the players' stats. That's the whole input space.

- **Positioning templates on a grid (BAF-style), per phase.** In-possession and out-of-possession at minimum (the proven floor), with inheritance so you can specialize more cells later. Anchors are **field-relative**: the placed dot is home base, and the ball only shades a player off it (bounded drift), never tethers him to it — keeping the canvas honest, the shapes legible, and the gaps stable and exploitable.
- **Roles** — chunked policy bundles assigned per player. A role defines the considerations, the trigger verb (challenge / hold / outlet / etc.), the coverage scheme (zonal vs. man are *role properties*, not separate dials), and the role's *intended* tendency (the finisher role shoots). Roles are how a player's shape becomes a job, and the shared tactical vocabulary an invented sport otherwise lacks.
- **Stats** modulate everything: awareness = perception quality; positioning = anchor discipline; the role's stat (tackling, passing) = the verb's success; physical/technical = resolution; stamina = how long before the third-period version of him is a rumor.

**Diagnosis maps to three verbs.** A player keeps failing → is it the *spot* (move him), the *casting* (recast — his shape was never this role), or the *player* (develop / bench / trade)? Move, recast, upgrade.

**Counterplay is emergent.** A posted, aggressive role is also a posted vulnerability — bait the destroyer, draw the lunge, hit the seam during his stagger. Every placement is a stance with a back.

**Live layer.** Real-time sim with between-soul substitutions and tactic tweaks (at each soul break) to reward watching and iterating — but watching must be *optional*, never a 60-game obligation. The solution: **programmable standing orders / an auto-coach you author** (sub rules, conditional tactics). Watching live is manual override of your own policy; the delta is small and fair. (This same auto-coach is the multiplayer attendance proxy — see §8.) Stamina must be *visible in behavior* (gassed = closes slower, presses lazier), not just a bar. Watch throttle: full / key-moments / headless.

**Pre-match gameplanning.** Scout the opponent's tendencies and formation history; counter-position. (The TFT scout-the-board habit, at league scale.)

**Parked:** team vs. per-unit formations; how much phase behavior lives in the role vs. the matrix; optional role-variant/"focus" dials for tweaks without full orthogonality.

---

## 4. Players & Scouting

Players are **shapes** — correlated attribute templates — not stat vectors. Attribute list is derived strictly from the sim loop: a stat exists only if some step consumes it (perceive → hold anchor → trigger verb → resolve → endure).

- **Fog law: uncertainty perturbs magnitude, never shape.** Scouts always see *what kind* of player he is; what's fuzzy is how tall the bars are and where they cap. You bet on degree, not kind — so identity-building never becomes a slot machine.
- **Two fog dials:** scouting accuracy (epistemic — narrows with exposure and scout investment) vs. development risk (aleatoric — the player's own volatility, a property of him).
- **Trajectory / grain is a scoutable property,** with shape: "projects to round out" vs. "projects to sharpen." Development moves are *local* in shape-space — you draft a neighborhood, not a point. A well-rounded prospect can be read as leaning offensive or defensive.
- **Fixable vs. structural flaws.** Low-now/high-ceiling = a project; low-ceiling = a permanent weakness priced in at draft time. Same current value, opposite meaning.
- **Card tense.** Prospects are read ceiling-first (current bars are noise); veterans current-first; the current/ceiling gap is a depleting resource you watch close.
- **Tweeners / generalists** are flagged high-variance picks — "scouts disagree on what he is" — and *you get a vote* in which neighbor they become via usage.
- **Innate tendency** (e.g., the elite shooter who personally won't shoot) is a scouted *player trait*, distinct from the role's intended tendency. The friction between them is the role-fit/coaching content; usage can slowly bend the prior.
- **Anything load-bearing is scoutable; anything not is cut.** No hidden-but-load-bearing attributes (FHM's worst quadrant). No second-class attributes (OOTP's fielding/running gap) — the full apparatus (current, ceiling, grain) applies uniformly.

**Generation** is archetype-and-species-first, then **sim-validated**: a generated player's rating is his *measured simmed contribution*, not a heuristic. This is the cure for FHM's player-coherence decay — incoherent players can't hide because there's no heuristic to fool, and AI GMs valuing off the same simmed model trade coherently by construction. Pools need *texture* (scarcity curves, real archetype tradeoffs), not gray goo.

---

## 5. Cast (human-only, for now)

Human-only. The fantasy lives in the *fiction* — souls, fire, the offering to a patron, the folk-eerie register — not in the cast; non-humans aren't needed to carry any of it. Player texture comes entirely from the **individual player model** (shapes, the round-out-vs-sharpen trajectory, fixable-vs-structural flaws, personality tags), which already produces the late bloomer, the boom-bust prospect, the durable anchor, the sniper-who-won't-shoot — as individual variation rather than species tags. One **shared aging model** for everyone, with individual variation inside it.

**Why not species (yet).** The strategic prize species promised — drafting for different career arcs — is already delivered at the individual level by the scoutable trajectory/grain property. Species-arcs would duplicate that gameplay at the group level while paying its entire cost: divergent lifespans are the hard-to-balance, immersion-straining part, and the only part that was actually difficult. Cutting them loses almost nothing.

**The door is open, on these terms.** If the pool ever feels samey, add species as **shape-and-silhouette priors on the shared timeline** — leaning *animals*, because silhouette is the payoff (a roster of herons reads as a different construction philosophy than a roster of badgers, at a glance on the broadcast — serving the legibility pillar), biology pre-teaches *shape* with zero lore, and gait variety feeds the watch view. Discipline if re-added: priors not boundaries (heavy overlap), shape and look only — **no divergent ages, no per-kind rule exceptions, mental attributes stay individual.** Keep the cheap high-value part (look, texture, identity-at-a-glance); never re-import the expensive part (different lifespans and peak timings).

---

## 6. Development

- **In-season: usage-driven.** Players improve at what they *do*, up to their cap — every pass nudges passing a little. Deployment and role assignment are the primary growth lever, so lineup decisions and development decisions are the same decision. Light focused practice exists but isn't the main channel.
- **Offseason: targeted.** More deliberate, directed improvements and role/shape shaping.
- **Aging & decline.** One shared career-curve model with individual variation; decline is **shape-first** — the veteran card shows what he loses first (speed-dependent silhouettes erode before craft), making contract-length a shape-reading skill rather than vibes about age-30 seasons. Plasticity ossifies with age.
- **The cost side.** Playing the kid to grow him trades present wins and a lineup slot — real tension, not a free vending machine.
- **Reserve / development squad.** One shallow pipeline (deliberately not OOTP's eight-team farm hell). It runs on the same sim, so directing a prospect's development is literally assigning him a role down there.
- **The emotional contract: the game may break your heart but never gaslight you.** Busts must exist (or investment is meaningless), but every bust gets a *legible post-mortem* from visible causes (the scouted motor flag, the off-grain deployment). Shape-locality softens the floor: a failed top-liner usually resolves into a useful role player, not a zero. Total craters are rare and pre-flagged.

---

## 7. Finances

Maximally simplified — no business sim. Teams differ *only* by personnel.

- **Flat hard salary cap, with a floor.** Floor is required so tanking-for-development-budget isn't degenerate.
- **NHL-style contract negotiation.**
- **Unspent cap → development budget.** This is the only economic lever, and it makes **competitive cycling an economic law**: rebuilders are under the cap, so their future is funded by their present weakness; contenders pay for now with later. Self-balancing, and it gives a bad team's GM something active to do.
- **No tickets, stadiums, sponsorships, or owners.** Money has exactly two uses: players and player futures.
- **Requirement:** AI GMs must genuinely weigh win-now vs. develop-later, or the league won't cycle believably (the simmed-value foundation gives them a coherent basis).

---

## 8. Multiplayer

Nearly free given the architecture, and personally high-value (managing with friends).

- **Async league (dominant mode):** managers submit setups, the server sims the slate, everyone gets results and replays. Scales to a whole group.
- **Live head-to-head:** two managers watch and sub the same game in lockstep (only inputs cross the wire).
- **Co-GM:** multiple people share one franchise.
- **Shared-seed pools:** draft the same generated class in parallel saves and compare.
- **The key synergy:** the authored auto-coach (§3) is the **async attendance proxy** — when a manager isn't present, their standing orders manage for them. The feature that makes watching optional in single-player is the feature that makes async leagues playable.
- **Requirement:** server-authoritative, reproducible sim + fully serializable coaching inputs (both already wanted). Server-authoritative sidesteps cross-platform floating-point determinism entirely.

---

## 9. Architecture, stack & distribution

Three separable decisions, often conflated: the **architecture** (how the pieces split), the **stack** (what they're written in), and the **distribution** (browser vs. desktop). Keeping them separate keeps every choice but the first reversible.

### Architecture — locked

**The load-bearing, non-negotiable choice: decouple the sim into a standalone, headless, deterministic library with zero rendering dependencies,** behind a clean interface where serialized inputs go in and an event stream comes out. One decision serves season-simming, server-authoritative multiplayer, reproducible replays, and automated testing — and it makes both the front-end and the distribution low-stakes and swappable. The same client/server seam that multiplayer needs is the seam that makes browser-vs-desktop a packaging choice rather than a rewrite.

### Development model — the constraint that picked the stack

Code is **AI-authored, human-directed and human-reviewed.** Jack architects, sets interfaces, and reviews; he doesn't hand-write. This inverts the usual stack calculus: authoring-velocity advantages (e.g., a dynamic language being fast to type) are largely neutralized, and *reviewability + machine-checkable correctness* become the human-side constraints that matter.

### Stack — sim core locked (Rust), front end flexible

- **Sim core: Rust.** The AI absorbs Rust's authoring cost (borrow checker, lifetimes); Jack keeps its review benefits — explicit, typed, readable code, and **a strict compiler acting as a second reviewer** that catches the subtle bugs hardest to eyeball. Critically for a deterministic sim, Rust makes the worst bug class (nondeterminism — unseeded RNG, float drift, unordered iteration → corrupted replays and MP desync) *structurally* hard: easy integer/fixed-point math, explicit ordering, no hidden GC or exceptions. Rigor is concentrated exactly where correctness is least visually verifiable. Compiles to native (desktop/server) and WASM (browser/solo), and pairs naturally with Tauri.
  - **Review workflow:** mandate idiomatic, documented code in Rust's readable subset (no macro/lifetime cleverness in the sim); review at module boundaries, not line-by-line; ask for explanations on demand; and require a **golden-seed regression test** (fixed seed + inputs → fixed output hash) so determinism breaks are caught automatically, not by eye.
- **Front end: not locked, deliberately flexible.** Leaning web (Svelte or React + a 2D canvas/PixiJS watch view) on the merits — best-in-class for the dense, themeable, chart-heavy screens that are ~80% of this game, and *validated by eye* rather than by careful review, so its language matters far less than the sim's. The watch view (dumb agents, smooth top-down motion) is graphically trivial for any 2D canvas. A Rust-native single-language alternative (egui) exists if the two-surface/WASM-boundary overhead grates. Godot is ruled out for the full client — it optimizes the 20% (rendering) and taxes the 80% (data UI). All reversible behind the sim boundary.

### Distribution — deferred, both supportable

Stack ≠ distribution: the same web front end ships as a browser game *or* a desktop app.

- **Pragmatic default — desktop (Tauri → Steam):** no infrastructure to run, the genre's commercial norm, local saves, easiest to finish; multiplayer via player-hosted/commissioner or save-passing.
- **Optional — hosted browser build:** unbeatable for "click this link to join my league" and persistent online leagues, but it commits Jack to *running servers* (cost, ops, save custody) — the operator hat.
- Decision deferred; the architecture preserves both. Flip to browser-first only if managing-with-friends is *the* point rather than a nice-to-have.

### Stacks considered (for the record)

A — Rust everything (Rust + egui); B — **chosen:** web app + Rust core (Rust sim + web UI, Tauri); C — all TypeScript (one-language velocity, weaker determinism guarantees); D — C# unified (Avalonia/Blazor); E — Godot native (lowest start friction, highest UI tax). B won once AI-authoring neutralized C's velocity edge and left Rust's reviewability/determinism guarantees as the deciding factor.

MITTS shares no code with this project either way.

---

## 10. Cross-cutting design laws

- **Every rule earns its place by four tests:** it forces a coaching decision, it creates a conservation tradeoff (cover A → concede B), it's readable in the watch view, and it's cheap in the sim.
- **Magic embodies rules, never decorates them.**
- **Fog perturbs magnitude, never shape.**
- **Intelligence is input quality, not compute** — dumb-but-faithful agents maximize the signal between your decisions and the result.
- **Coordination lives in the coach's formation, not in the agents.**
- **The genius layer belongs to the player (you); the agents just execute.**
- **Borrow familiarity, spend novelty where it's load-bearing** — applies to the sport genome and to the species choice alike.
- **Anything load-bearing is scoutable; anything not is cut.**
- **Diagnosis resolves to move / recast / upgrade.**

---

## 11. Season & league structure

- **Short seasons by default — ~16–32 games (NFL-ish counts), configurable at genesis.** The length protects the core loop: long seasons force sim-skipping, and skipping kills the watch-and-coach loop that is the entire point. Short keeps every game watchable and consequential.
- **NHL-ish structure on NFL-ish counts** — divisions, conferences, and a playoff bracket on top of the short regular season. Familiar shape, set at genesis with everything else (blank-slate ethos).
- **Variance at the season, skill at the franchise.** A short season is deliberately noisy — any single title has luck in it — because the skill the game rewards is *sustained* roster-building across years: the dynasty, not the one run. Individual seasons stay swingy and dramatic; the long game is the real test. Pairs with the cap's competitive-cycling law (§7).

---

## 12. Atmosphere & stakes

The occult, folk-eerie register is **pure atmosphere — never mechanics.** The firewall: **favor is prestige, never power.** Nothing in the fiction layer touches the cap, the sim, or competitive balance; teams still differ *only* by personnel (§7).

- **Stakes are diegetic and player-chosen.** Optional objectives are reframed as **patron vows** the manager chooses to swear — not an owner barking mandates. You opt into the pressure you want; keeping or breaking a vow is a story beat, not a firing.
- **Patrons as identity.** A team's patron is its character and the seed of its rivalries — allegiance and flavor, not a stat package.
- **The chronicle & a favor track.** The franchise's history is recorded (of a piece with the receipts culture in §2), and favor accrues as **prestige** — a record of what you've done, never a resource that buys an edge.

---

## 13. Open / parked questions

- Sport geometry specifics: exact zone banding, goal placement, scoring values, and the win-by-1-vs-win-by-2 finish rule.
- Roster model details: bench size, stamina/sub economy tuning.
- Team vs. per-unit formations (and the EHM-style team-default-with-override inheritance if per-unit).
- How much phase behavior lives in the role vs. the phase matrix.
- Full attribute list (derive by walking the sim loop).
- Genesis onboarding: named coaching philosophies as drafting lenses; the editor's live ghost-preview teaching tactics by demonstration; pool scarcity curves.
- Front-end framework (Svelte vs. React vs. egui) and distribution target (desktop-first vs. browser-first), after a Rust sim-core prototype proves the decoupling and the fun. Sim core (Rust) and architecture (decoupled) are settled.
