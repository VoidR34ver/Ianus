# Ianus engine: full handover and design brief

*Written 2026-11-11. For an AI model (or a person) to read cold, understand the whole project, and then reason about the best design. Everything below is labelled **VERIFIED** (run or measured), **WRITTEN** (code exists, not yet run), or **OPEN** (undecided or unknown). Nothing here is padded: where something is unfinished, it says so.*

---

## 0. How to use this document

**Your job, as the reader:** act as a senior engine architect. Do the reasoning yourself. Find the best way to structure a 2D, simulation-focused, deterministic game engine in Rust, then tell the owner where this project's current design is right, wrong, or missing something. Do not just agree with it.

**Please do:**
1. Read sections 1 to 6 to understand the goal and the current design.
2. Read section 8 (what is measured) and section 9 (open questions).
3. Answer section 10 directly, with reasons and, where you can, evidence or a small experiment.
4. Say plainly when you are unsure. Label claims as certain, likely, or guess.

**Please do not:** invent features the owner did not ask for, assume the code works where this document says it is unverified, or rewrite the whole thing in a different style without saying why.

**About the owner:** Zephie, a student in Switzerland, runs a small games studio, has made several games in Godot, is learning Rust, and has limited time (school exams). Writes in English and German. Wants critique more than flattery. A coding assistant (Claude) writes most of the code while she directs.

**Hard rules from the owner:**
- Never open GUI windows or run focus-stealing programs without asking first (a test window once interrupted a ranked game). Prefer headless tests.
- Cite and label claims; say when uncertain.
- Decisions about goals and priorities are hers, not yours or the assistant's.

---

## 1. What Ianus is

A **2D game engine in Rust, built on Bevy**, aimed at **simulation-style games** (colony sims, city builders, factory games, agent-heavy sims) that do not depend on hitbox-and-physics combat. Product shape, chosen by the owner:

- Like **GameMaker Studio 2**: a general 2D engine with its own editor, objects with built-in options, rooms (levels), and **Rust as the scripting language**.
- Its strength is **simulation**: a deterministic fixed-rate tick, exact replays, exact save/restore.
- The editor is meant to be built as a distribution of an existing Bevy editor (Jackdaw) and renovated piece by piece. **No editor beyond a small egui panel exists yet.**
- Audience: her own games first, usable by others later. Not aimed at absolute beginners (Rust is a real barrier).

**Why a niche engine at all:** large simulation games repeatedly hit engine limits (Unity's single-threaded simulation, Godot's per-node overhead at tens of thousands of nodes) and many end up writing custom engines (Factorio, RimWorld's move off Unity in Oct 2026). The bet is that determinism + data-oriented design + good tools is a gap worth filling. **OPEN:** whether the audience is big enough (see section 9).

---

## 2. A plain explanation of how a Rust/Bevy engine works

*(The owner asked for this because she is unsure how the engine "updates".)*

### 2.1 It does not "update every object"
In Bevy, data lives in an **ECS** (Entity-Component-System):
- **Entity**: just an ID (a number).
- **Component**: a piece of data attached to an entity (`Position`, `Health`, `Sprite`).
- **System**: a plain function that runs on a schedule and works on *all entities that have certain components* (e.g. "for every entity with `Position` and `Velocity`, move it").

So there is no per-object `update()` method. A system is a loop over a *query*. Data is stored in tightly packed arrays by component combination, which is why this is fast (cache-friendly, easy to run in parallel).

### 2.2 The frame loop vs the simulation tick
Bevy has two clocks:
- **Frame loop** (`Update`): once per rendered frame, variable speed (60, 144 fps...).
- **Fixed loop** (`FixedUpdate`): runs a fixed number of times per second (Ianus: **60 ticks per second**), however fast frames come. If a frame is slow, it runs several ticks to catch up; if frames are fast, some frames run none.

Ianus keeps **all gameplay in the fixed loop** so the simulation is the same on every machine, and uses the frame loop only for drawing, input reading, and the GUI. Rendering **interpolates** between the last two ticks so motion looks smooth at any refresh rate.

### 2.3 Determinism
Determinism means: same starting state + same inputs per tick = bit-identical results, on any machine. Ianus gets it by:
- No floating point in the simulation (integer fixed-point only).
- No clocks, no OS randomness; a seeded RNG (xoshiro256**) instead.
- Systems read a per-tick input value, never the keyboard directly.
- Explicit ordering of systems.
Payoffs: exact replays (record one input byte per tick), exact stop/restore, shareable bug reports, and a path to lockstep multiplayer.

### 2.4 Why Rust matters here
- Ownership/borrowing lets the ECS prove at compile time that parallel systems do not race.
- No garbage collector: no pauses, predictable memory.
- Cost: slow compiles, steep learning curve, and Bevy's API changes every ~4 months (about 75 to 100 breaking changes per release).

---

## 3. Repository map

Workspace root: `C:\Ianus` (Rust 1.99.1, Bevy 0.20.1, Windows 11; not a git repo yet).

| Crate / path | Role | Needs Bevy? |
|---|---|---|
| `crates/ianus_sim` | **Deterministic core.** Integer movement, RNG, replay format, hitboxes, spatial hash, wall stopping, tick scheduler. | No |
| `crates/ianus_objects` | **Data model.** Object categories, hitbox/collision settings, room file format, validator with plain-English errors. | No |
| `crates/ianus` | **Runtime.** Bevy plugin: fixed tick, input, interpolation, record/replay, play/stop, room loading, colliders, touching, due-object list. Uses only small Bevy sub-crates (no renderer). | Sub-crates |
| `crates/ianus_engine` | **The window.** Full Bevy + egui GUI (toolbar, scene list, inspector, simulation panel), camera, quiet-window mode. | Full Bevy |
| `rooms/cellar.room` | Sample level, 20x12 tiles, one object of each category. | |
| `tools/quiet-run.ps1` | Launches a program with no console window and reports foreground-window changes. **WRITTEN, never run.** | |
| `ianus-plan.md` | Earlier research and planning document (market, Bevy state, risks). | |
| `spikes/` | Throwaway experiments (a cloned Jackdaw source, screenshots). Not part of the engine. | |

**Dependency direction:** `ianus_sim` ← `ianus_objects`? No: `ianus_objects` is independent; `ianus` depends on both; `ianus_engine` depends on `ianus` and `ianus_sim`. The sim core never depends on Bevy, by design, so it is testable in milliseconds and cannot be broken by Bevy's rendering churn.

---

## 4. The simulation core (`ianus_sim`)

### 4.1 Numbers and space
- Positions and velocities are `i32` **subunits**. **4096 subunits = 1 tile.** `+x` right, `+y` up.
- Tile `(tx, ty)` is **centred** on `(tx, ty) * 4096` and covers half a tile each way.
- Movement tuning (per tick, 60 Hz): top speed 341 subunits (about 5 tiles/s); accel 60 (full speed in 6 ticks); friction 40; reversing gets accel+friction (snappier); diagonals scaled by 181/256 (integer, under 0.1% error). All integer.
- Saturating arithmetic at world edges so debug/release cannot disagree.
- **VERIFIED** by tests, including a pinned "golden" replay hash (see 4.5).

### 4.2 Hitboxes (`Aabb`) and the overlap formula
- `Aabb { min_x, min_y, max_x, max_y }`, **half-open** `[min, max)`: boxes that only share an edge do **not** overlap (so a body resting flush against a wall is "touching" but not "inside").
- Overlap = four comparisons combined with bitwise `&` (not `&&`): no branches.
- **Precondition:** exact only when both boxes are non-empty. An empty box inside another would wrongly overlap. Everything that stores or probes boxes (spatial hash, solids) skips empty boxes itself; `overlaps_checked` exists for direct callers. This was a real bug in an earlier claim, caught by a test.
- **Measured (VERIFIED, release build, one machine):** branchless 1.66 ns/test vs short-circuit `&&` 6.1 ns (3.7x). Array-lane formulation ties branchless. Centre+half-size form is slower (2.07 ns) and disagrees on 16 of 4M pairs from rounding, so rejected.

### 4.3 Spatial hash (`BroadPhase`)
- Uniform grid over the room; each box is listed in every cell it covers.
- **Compressed-row layout** (flat arrays `cell_start`/`cell_items`, counting-sort build in two passes), not a vector per cell. Rebuild reuses buffers: no allocation after warm-up.
- A box covering several cells is reported **once**, from the cell holding the near corner of its overlap with the query; so no dedupe set is needed and queries take `&self`.
- Results come out in a fixed order (cell-major, insertion order within a cell): deterministic.
- Boxes outside the grid are clamped into edge cells (never lost).
- **Cell size chosen by data, not guess** (VERIFIED; 50,000 boxes of 0.4 to 1 tile, 1 per 4 tiles², three random worlds, median of 7). Per box: pairs / one query.

| cell | pairs | query |
|---|---|---|
| 1 tile | 176 ns | 74 ns |
| 2 tiles | 126 ns | 58 ns |
| **4 tiles (default)** | **97 ns** | **54 ns** |
| 8 tiles | 115 ns | 64 ns |
| 16 tiles | 149 ns | 112 ns |

  Caveat: optimum depends on box size and density; one density measured.
- **Measured vs brute force (VERIFIED):** all-pairs at 10,000 boxes: 30.8 ms → 1.4 ms (22x). At 50,000 and 200,000 brute force was *estimated* (0.4 ns/test, actually closer to 1 ns, so true speed-ups are larger): about 69x and 138x. One player-sized query: 54 to 68 ns vs 37,177 ns scanning 50,000 boxes. Full rebuild of 10,000 boxes: 0.15 ms (0.9% of a tick); 100,000 boxes: 2.9 ms (17%).
- Correctness: matched against brute force across 160 random worlds and 4 cell sizes (including huge boxes spanning many cells). **VERIFIED.**

### 4.4 Stopping at solids is a separate layer
Owner's design: *detection* (do these overlap?) and *stopping at walls* are independent. `solid.rs`:
- **Solids** has two storage forms chosen automatically: a **tile bitmap** for exact whole-tile solids (one array lookup) and a **BroadPhase of boxes** for everything else (pillars, offset hitboxes).
- `step_body_solid(body, input, solids, footprint)`: same acceleration as plain `step_body` (shared function), then moves **X then Y**, each resolved separately, so bodies *slide* along walls. On collision it snaps the body **flush** against the nearest blocking edge (min/max over everything overlapped).
- A body that starts inside a solid walks out freely (no teleport back).
- A body moves under a tile per tick, so it cannot tunnel.
- The room's edge is solid.
- A `Footprint` (half-size + offset) generalises the body's hitbox.
- **VERIFIED:** property test (never inside a solid across 20,000 busy ticks, for tiles and for boxes); no-tunnelling at top speed from four sides; the walled-run golden hash is **unchanged** after rewriting walls on top of the new layers (strong regression proof).

### 4.5 RNG and replay
- RNG: xoshiro256** seeded via SplitMix64, `fork(stream)` for independent streams, unbiased `below(n)` (Lemire). First outputs cross-checked against an independent Python implementation. **VERIFIED.**
- Replay file: magic `IANR`, version, seed, tick count, **one input byte per tick**, optional final state hash. Truncation/garbage never panics (tested). Playback reports IDENTICAL / DIVERGED by comparing a world hash at the end.
- **Golden hashes** are pinned in tests (movement replay, walled replay). **Caveat:** produced only on this Windows machine; the cross-OS determinism claim is **untested** until CI runs on Linux/macOS.

### 4.6 Tick scheduler (`schedule.rs`) — the "buckets" idea
Each object has `ob_tick`: it updates once every `ob_tick` ticks (`1` = every tick, `0` = never).
- Objects are filed into **lanes by rate**; a lane with rate `n` has `n` **buckets**; on tick `t` only bucket `t % n` is walked. Objects of equal rate are dealt round-robin, so 1,000 objects at rate 10 become 100 per tick, not 1,000 every tenth tick (no spikes).
- Not-due objects are never visited, not even to be skipped.
- Lanes are sorted by rate, so output order does not depend on insertion order of rates. Rates clamp to 216,000 (one hour).
- **VERIFIED** (9 tests: exact once-per-interval, even spreading, clamping, huge tick numbers).
- **Measured (VERIFIED, 100,000 objects):** typical room (80% never update, 12% every 30 ticks, 5% every 6, 3% every tick): buckets **2.8x** faster than "walk everything and skip if not due", 15x faster than updating everything; busiest tick 4,238 updates vs 20,005. All objects every 60 ticks: **16.7x** vs check-and-skip. **Worst case, everything every tick: buckets are slightly *slower* (0.9x)** (nothing to skip, pay for structure).
- **Design decision (the owner proposed `ob_tick`):** movement and collision must **not** follow `ob_tick`. A body updated only every Nth tick either crawls at 1/N speed or jumps N times as far and tunnels through walls. So movement and collision run every tick; `ob_tick` governs an object's own logic.

---

## 5. The object model (`ianus_objects`)

**Owner's latest direction:** *objects are just things with hitboxes*; category is a preset, not a different kind of thing. Collision is a per-object master toggle; "solid" (stopping things) is separate.

### 5.1 ObjectDef (settings first, in this order in the file)
| Field | Meaning |
|---|---|
| `name` | Unique within the room. |
| `ob_tick` | Update interval in ticks; 0 = never. Default per kind: walls/environment/lights 0, others 1. |
| `collision` | **Master switch.** Off: no hitbox exists. Nothing touches it, nothing stops at it, and it does not stop at anything either (noclip). |
| `solid` | Others stop at this. Only matters if `collision` is on. Walls and non-walkable ground default on. |
| `hitbox` | `width`, `height` in **hundredths of a tile** (100 = 1 tile), plus `offset_x/y`. Integer so there are no floats in authoring. 75 converts to exactly half-width 1536 subunits. |
| `kind` | The category and its options (below). |

`blocks() = collision && solid`.

### 5.2 Categories (`Kind`) with built-in options
- **Wall**: `durability: Option<u32>`.
- **Entity**: `controlled`, `speed_tiles_per_s`. (Only one controlled entity per room; **speed is currently ignored**, global tuning applies.)
- **Interactable**: `action` (`Message(text)` or `Toggle(object name)`), `radius_tiles`, `once`.
- **Environment**: `walkable`, `speed_percent`. (**Speed percent is currently ignored**, so mud does nothing.)
- **Light**: `radius_tiles`, `color [u8;3]`, `intensity_percent`. (**Does nothing yet.**)
- **SubstanceSource**: `substance` (Water/Heat/Gas/Smoke), `rate_per_tick`. (**Does nothing yet.**)

### 5.3 WorldRules (level-wide, read first)
- `collision`: level master switch; off = nothing in the level has a hitbox.
- `ob_tick_scale`: multiplies every object's interval (`2` = everything updates half as often); 0 means nothing ever updates (validator warns).

### 5.4 Room file
RON text (`rooms/cellar.room`): `name`, `width`, `height`, `world`, `objects` (definitions), `placed` (name + tile x,y). Human-readable, diff-friendly, **hot-reloaded** when saved. Older files without the new fields still load with defaults (tested). RON is an interim choice: Bevy's BSN scene format has no file loader yet (OPEN: migrate when it does).

### 5.5 Validator (`Room::validate`)
Plain-English problems, errors before warnings, typo suggestions ("did you mean 'Wall'?" via edit distance). Covers: unknown/duplicate names, out-of-bounds placement, zero-size room/hitbox, stacked solids, entity placed inside a solid, no/multiple controlled entities, pointless options (radius 0 light, rate 0 source, 0% speed ground), contradictory switches (solid but collision off; wall not solid), switches pointing at nothing, tick-rate mistakes, `ob_tick_scale` 0. A room with errors is **refused** and the previous room stays loaded. **VERIFIED:** 29 tests.

---

## 6. The runtime (`ianus`)

### 6.1 Tick phases (WRITTEN, compiles, not yet run in tests)
`FixedUpdate`, in order (a Bevy system-set chain):
1. **Input**: decide this tick's input (live keys, or from a replay).
2. **World**: level settings in force; collision structures rebuilt **only if something that could change them moved**.
3. **Objects**: fill `DueObjects` with objects due this tick (via the scheduler). Not-due objects untouched.
4. **Sim**: movement (every tick) and game logic. Game logic should iterate `DueObjects`, not all objects.
5. **Detect**: `Touching` = what the controlled entity's hitbox overlaps.
Then the tick counter advances.

### 6.2 Input handling (VERIFIED)
- Keys are read right before the fixed loop (no one-frame lag).
- **Latching:** a key tapped shorter than a tick is never lost (a tap moves exactly one tick). Held vs latched state; tested by sabotage (disabling the latch made the tap test fail).
- Rebindable; WASD + arrows default. Works headless with no keyboard.

### 6.3 Interpolation (VERIFIED)
Rendering blends previous→current tick by the fixed-loop overstep fraction (in f64, then cast), so motion is smooth at any refresh rate; spawned bodies do not slide in from the origin.

### 6.4 Pause / step / speed
`SimControl { paused, step_requests, speed }`; pausing may run one in-flight tick (documented quirk); step runs exactly one tick.

### 6.5 Play/Stop and restore (VERIFIED)
- Rooms load in **edit mode** (frozen). **Play** snapshots every body, tick, and RNG. **Stop** restores them exactly (world-hash equality tested). Edits made *before* Play survive Stop; things spawned during play are removed. Limitation: entities despawned during play are not restored; only body positions/velocities, tick and RNG are snapshotted. Sabotaging the restore made three tests fail.
- **Restart hook:** games register one "reset me" system; rooms need none (placed objects remember their start tile).

### 6.6 Record/replay in the runtime (VERIFIED)
Record in a running app, restart via hook, play back: reports IDENTICAL. Tampered hash or different start world: reports DIVERGED. Replays in walled rooms match.

### 6.7 Colliders and detection (WRITTEN)
- `Collider { footprint, solid }` exists on an object only if its own `collision` **and** the level's are on.
- `SolidMap` = `Solids` built from solid colliders; whole-tile ones go to the bitmap, others to boxes. Rebuilt in the **World** phase when any non-controlled collider's body changed (an emptiness check on `Changed<SimBody>`), so a wall dragged in the editor is correct on the first tick of Play.
- `ColliderIndex` = spatial hash of all non-controlled colliders. The controlled entity is the *querier*, not stored (it moves every tick).
- `Touching` = sorted list of entities overlapped by the controlled entity this tick.
- Only the **controlled entity** moves and collides today; crates are inert.

### 6.8 Due objects (WRITTEN)
`ObjectSchedule` (built at room load; interval × level `ob_tick_scale`, 0 stays 0) and `DueObjects` (reused vector, refilled each tick with `(entity, ticks since last update)`). **No object has any logic yet**, so nothing consumes `DueObjects`; it is the hook for scripts.

---

## 7. The window and GUI (`ianus_engine`)

egui panels around the game view (toolbar, Scene list grouped by category, Inspector, Simulation panel with frame-time graph), click-to-select, camera that frames the room while editing and follows the player while playing, text status messages. Auto-reload of room file. Shortcuts: F5 Play/Stop, F1 pause, F2 step, F3 speed, F4 hide panels, F6 replay, F7 record, F8 reload.

- **VERIFIED on screen once** (a single owner-approved run): room rendering with correct colours per category; panels; 138 fps.
- **Unresolved bug:** in several earlier runs the game view rendered blank or black while the panels drew fine. A later run was fine. Unconfirmed theory: screenshot taken before render pipelines finish compiling. **OPEN.**
- **Not seen on screen:** edit-mode camera framing, grouped Scene list.
- Bevy 0.20.1 quirk found: spawning a camera with a custom `Projection` at spawn time coincided with blank rendering in some runs; the code now spawns a plain `Camera2d` and edits the projection afterwards. Whether that was the real cause is unconfirmed.
- **Quiet windows:** automated runs create the window unfocused, bottom-most, no taskbar (because a test window once stole focus mid-game). Launching a console exe with `Start-Process` also opens a console window that steals focus, hence `tools/quiet-run.ps1` (untested).
- A launcher/tooling note: the GUI is egui (not Bevy's own UI) because egui has tables, plots, and docking today. Jackdaw (the planned editor base) uses Bevy's Feathers instead, so egui panels would not port directly. **OPEN.**

---

## 8. Honest status

| Area | State |
|---|---|
| `ianus_sim` | **Verified.** 59+ tests passing at last run (hitboxes, spatial hash, solids, RNG, replay, scheduler), plus two headless benchmarks (`bench_hitboxes`, `bench_schedule`). |
| `ianus_objects` | **Verified.** 29 tests passing at last run, sample room regenerated with the new fields. |
| `ianus` runtime | **Last confirmed green before the collision/scheduler rewrite (about 38 tests).** The new `collide.rs`, `objects.rs`, new tick phases and the `lib.rs` wiring **compiled without errors on the last build, but its tests have not been run since**, and no tests yet exist for `Touching`, `DueObjects`, the per-object `collision` toggle, or `WorldRules`. Existing room tests may need small expectation updates. |
| `ianus_engine` | Compiled and ran earlier; **not rebuilt or checked since the collision/object-model changes.** May need fixes. |
| Whole-workspace check | **Not run** since these changes. Do not assume clean. |
| Cross-OS determinism | **Untested** (only run on Windows). |
| Benchmarks | One machine (AMD Ryzen 5 7600, RTX 4080 SUPER), one density each. Relative numbers, not absolute. Brute-force numbers beyond 10,000 boxes were estimated, not run. |

**Known gaps (not bugs, just unbuilt):** mud/speeds ignored; lights, substance sources, interactables do nothing; only the player moves; no AI/pathfinding; no save files (only replays); no editor placement (cannot add/move/save objects from the GUI); no real scripting layer; no object behaviours that consume `DueObjects`; no tilemap rendering (placeholder coloured squares); no art/audio; no multiplayer.

---

## 9. Open questions and risks (the owner's and the assistant's)

1. **Is the niche big enough?** Evidence: about 221 colony sims, 436 city builders, 477 automation games shipped on Steam in recent windows, with colony sims hitting 1,000 reviews at 13.7% (vs 4.2% overall); Bevy Jam entries flat at 64 to 98 since 2022; only three Bevy sims in development found. Realistic user base: tens of teams.
2. **Rust as "scripting" is the make-or-break feature.** It only feels like scripting if edit→running takes seconds. Bevy hot patching is opt-in and fragile (startup systems, multi-crate workspaces); Fyrox's DLL reload is documented as fragile. Plan: a narrow "script shape" (per-tick function over components) plus **snapshot-rebuild-restore** as the fallback. **Untested; the single biggest technical risk.**
3. **Build on Jackdaw's editor or write one?** Decided: distribution of Jackdaw, renovated gradually. Jackdaw already has a tile editor, 2D view and "custom editor distribution" seam (about 514,000 lines; `main` tracks Bevy's main branch; Windows extension loading listed by its own docs as fragile). **Never built or run** (Rust was not installed then; since installed).
4. **Bevy churn:** about 100 breaking changes in 0.19, 75 in 0.20; core crates deliberately avoid rendering/UI crates.
5. **Burnout / ownership of code she did not write:** the owner delegates coding; risk is not understanding the determinism or save format when it breaks.
6. **Name:** "Ianus" collides with IANUS Simulation GmbH (Dortmund, simulation software). TMview search found no IANUS mark for games or game software; not legal advice.
7. **Single controlled mover.** Everything (collision, solids, due objects) is built around one moving body plus static things. Many moving agents (the real colony-sim case) need a different design (below).

---

## 10. Questions for you to answer (do the reasoning)

Please think carefully and give reasoned answers. Where a claim can be tested with a small program, say what test.

### A. The tick model
1. Is "world rules → due objects → movement → detection" the right phase order for a deterministic sim? What belongs in each phase? What common ordering bugs (one-tick lag, order-dependent results) should we guard against, especially with parallel systems? Does Bevy's scheduler guarantee a deterministic order given system sets, or do we need explicit `.chain()`/ambiguity detection as errors?
2. Is fixed 60 Hz right? Should the sim support multiple rates (e.g. slow AI think at 10 Hz) via `ob_tick` only, or also a separate slower "world" tick (for substances/weather)?
3. How should `ob_tick` objects' work be shaped so a game can write logic once and have it run correctly at any rate? ("catch up" by `elapsed ticks` vs fixed-step.)

### B. Scheduling and optimisation
4. Is the bucket scheduler (lane per rate, `t % n` bucket, round-robin dealing) the best structure? Alternatives: a time wheel, a priority queue of next-due ticks, event-driven wake-ups ("sleeping" objects woken by events), dirty-flag propagation. When does each win? Our measurements say buckets lose slightly when everything updates every tick: should there be a fast path?
5. What is the best way to handle **dynamic** schedules (an object changes its own `ob_tick`, spawns, despawns) without rebuilding or losing determinism?
6. In Bevy ECS terms, is iterating `DueObjects: Vec<(Entity, u32)>` and looking components up per entity the efficient way, or should due objects be tagged via a marker component/archetype change, or processed as struct-of-arrays outside the ECS? Cost of random-access `world.get` vs iterating a query?

### C. Collision at colony-sim scale
8. Only one body moves today. For thousands of moving agents, what is the best design: rebuild the spatial hash every tick (our measured cost: 0.15 ms for 10,000 boxes, 2.9 ms for 100,000), incremental update of only moved boxes, a hierarchy/quadtree, sweep and prune, or tile-occupancy grids with per-tile lists (likely best for tile-based colony sims)? Compare on correctness, determinism, cost, and simplicity.
9. Is our **X-then-Y axis-separated** resolution acceptable, or does it produce artifacts (corner catching, order bias between X and Y) that matter? Should movement against moving solids (crates, doors) use a different method (swept AABB / continuous collision)?
10. Layers/masks: should hitboxes have `layer` and `mask` bits to skip pairs early? What is the cost/benefit?
11. Is the **4-tile default cell** reasonable across densities, or should the grid adapt (auto-tune from object sizes/counts)?

### D. Determinism and saves
12. List every way our determinism could silently break in Bevy/Rust (HashMap iteration order, system parallelism, `sort_unstable` on non-unique keys, `Entity` ordering/ID reuse across Play/Stop, floating-point in Bevy's `Transform`, `libm` feature, allocator-dependent iteration). Which are already handled? Which are not?
13. Design a **save-game format with migration** for long-running colonies: versioned per-component schemas, chunking, compression, forward/backward compatibility, and how it should interact with replay and Play/Stop snapshots. (Today only positions/velocities/tick/RNG are snapshotted.)
14. What should the **world hash** include to be a trustworthy divergence detector, and how can we localise *where* two runs diverged (per-system hashes, hash-per-tick logs)?

### E. Architecture
15. Is splitting a Bevy-free `ianus_sim` from the Bevy runtime the right boundary? What should move across it (e.g. should the scheduler and spatial hash be generic over entity IDs)?
16. Should simulation state live in Bevy components at all, or in a separate flat data store (struct-of-arrays) with Bevy only for rendering/UI? Trade-offs for determinism, performance, tooling (inspector, undo), and hot reload.
17. How should **Rust scripting with a seconds-long loop** be designed given Bevy's constraints? Options: hot patching (subsecond), dylib plugins, rebuild-and-restore using snapshots, or an embedded interpreter for data-only logic. What does the "script API" look like so that scripts stay deterministic and can be restricted to safe operations?
18. Is a **data-driven object model** (our `Kind` enum with built-in options) the right abstraction, or should objects be composed from small behaviours (components) users mix and match, with categories as presets only? (The owner just said "objects are just things with hitboxes".)

### F. Product and plan
19. Review sections 1 and 9 critically. Is there a better wedge than "deterministic sim layer"? What would make this engine clearly the best in its niche rather than just another Bevy project?
20. What is the smallest next milestone that tests the biggest risk (question 17) cheaply?
21. What would you cut?

---

## 11. Facts and constraints you should rely on

- Language/engine: **Rust 1.99.1, Bevy 0.20.1** (released 2026-10-08; releases every ~4 to 5 months, each with 75 to 100 breaking changes). Official Bevy editor does **not** exist yet (stated top priority; BSN scene notation shipped in 0.19 without a file loader). Jackdaw is the community editor with a tile editor.
- Machine: Windows 11, AMD Ryzen 5 7600 (6 cores), RTX 4080 SUPER, 31 GB RAM.
- Determinism rules enforced in `ianus_sim`: **no floats, no Bevy, no clocks, no global state.** Keep these.
- All world scale in the sim is integer subunits (4096/tile). Authoring uses integer hundredths of a tile.
- Existing results are pinned by **golden hashes**; changing simulation behaviour must change them deliberately and say why.
- Test commands (headless, safe): `cargo test --workspace --lib`, `cargo clippy --workspace --all-targets`, `cargo run --release -p ianus_sim --example bench_hitboxes`, `... --example bench_schedule`. Opening windows (`ianus_engine` examples) needs the owner's permission.

---

## 12. Glossary

- **Tick**: one step of the simulation (1/60 s).
- **Subunit**: the sim's integer length unit; 4096 = one tile.
- **AABB**: axis-aligned bounding box (a rectangle that is not rotated).
- **Broad phase / spatial hash**: a grid that makes "what is near here?" a lookup instead of a scan.
- **Solid**: others stop at it. **Collision**: whether the hitbox exists at all (master switch).
- **ob_tick**: how many ticks between an object's own updates.
- **Bucket**: one pile of objects due on the same tick-phase, so non-due piles are never opened.
- **Golden hash**: a pinned fingerprint of a deterministic run; if it changes, behaviour changed.
- **Interpolation**: drawing a thing between its last two ticks for smooth motion.
- **ECS**: Entity-Component-System, Bevy's data model.
- **Snapshot/restore**: saving the exact sim state and putting it back (used by Stop).
- **Quiet window**: a window opened unfocused, behind others, out of the taskbar.
