# Ianus: architecture review brief (v2, for a second reviewer)

*2026-11-11. Written for a strong reasoner reading this cold. A first reviewer already looked at an earlier brief; what went wrong with that review is described in section 9 so you can avoid it. Everything is labelled **VERIFIED** (run or measured), **WRITTEN** (code exists, no test covers it yet) or **OPEN**.*

---

## 1. Your task in one paragraph

Ianus is a deterministic 2D simulation engine in Rust on Bevy. The owner wants to know whether its design is the best way to build what she is building, and where it is wrong. **Answer the numbered questions in section 8, in the format in section 10. Do not re-describe the project, do not write new specifications, and do not present anything as existing that this document does not list as existing.** Disagreement with the current design is wanted, with reasons.

## 2. Constraints you must respect

- **Owner decisions are not yours to make.** She decides target audience, scope (for example 2D sim vs also platformers or 3D), time budget and priorities. If your answer depends on one of these, state the dependency and give the answer for each case. Open owner decisions are listed in section 7.
- **Determinism is non-negotiable in `ianus_sim`:** integer maths only, no floats, no clocks, no OS randomness, no global state, no Bevy. Any proposal that breaks this must say so loudly.
- **Evidence over assertion.** If a claim can be tested with a small headless program, give the test. Label confidence: *certain / likely / guess*.
- **No GUI tests.** Do not propose experiments that open windows; headless only.
- Rust 1.99.1, Bevy 0.20.1. Bevy breaks its API every ~4 months (75 to 100 breaking changes per release); that is a standing cost.

## 3. What the project is

A GameMaker-style 2D engine (objects, rooms, built-in object options, Rust as the scripting language) whose differentiator is a **deterministic fixed-tick simulation**: exact replays, exact stop/restore, shareable bug reports, a path to lockstep multiplayer. First game planned: a small colony sim. Beachhead: simulation games (colony, city, factory) that hit engine limits elsewhere. Realistic early audience: tens of teams (see section 7).

## 4. Architecture as built

Workspace crates, dependency direction `ianus_sim` ← `ianus` ← `ianus_engine`; `ianus_objects` is independent, used by `ianus`.

| Crate | Responsibility | Bevy |
|---|---|---|
| `ianus_sim` | Deterministic core: integer movement, RNG, replay format, hitboxes (`Aabb`), spatial hash (`BroadPhase`), solids and wall stopping, tick scheduler | none |
| `ianus_objects` | Data model: object definition (`ob_tick`, `collision`, `solid`, `hitbox`, `kind`), level `WorldRules`, room file (RON), validator with plain-English errors | none |
| `ianus` | Runtime plugin: fixed tick phases, input, interpolation, record/replay, Play/Stop snapshot-restore, room loading, colliders, `Touching`, due-object list | small sub-crates only |
| `ianus_engine` | Window, egui panels (toolbar, scene list, inspector, simulation), camera, quiet-window mode | full Bevy |

### 4.1 Numerics
`i32` **subunits, 4096 per tile**, +y up. Tile `(tx,ty)` centred on `(tx,ty)*4096`. Movement constants per tick at 60 Hz: top speed 341, accel 60, friction 40, reversing = accel+friction, diagonals ×181/256. Saturating arithmetic at the edges. **VERIFIED** by tests.

### 4.2 Tick phases (FixedUpdate, chained in this order) — WRITTEN, compiles; phase wiring only indirectly tested
`Input` → `World` → `Objects` → `Sim` → `Detect`, then the tick counter advances.
- `Input`: pick the tick's input (live keys, or from a replay). Systems read `TickInput`, never the keyboard.
- `World`: level settings in force; collision structures rebuilt **only if a non-controlled collider's body changed** (a `Changed<SimBody>` emptiness check).
- `Objects`: fill `DueObjects` with objects due this tick via the scheduler.
- `Sim`: movement (every tick) and game logic.
- `Detect`: `Touching` for the controlled entity.

### 4.3 Hitboxes and detection (`aabb.rs`, `broadphase.rs`)
- `Aabb` half-open `[min,max)`; touching edges do not overlap. Overlap = four comparisons joined with `&` (no branches). **Exact only for non-empty boxes**; `overlaps_checked` exists for the rest. A test caught an earlier false claim about this.
- `BroadPhase`: uniform grid, **compressed-row layout** (flat `cell_start`/`cell_items`, two-pass counting build), buffers reused, no allocation after warm-up. A box in several cells is reported once, from the cell holding the near corner of its overlap with the query: no dedupe set, `&self` queries. Out-of-bounds boxes clamp to edge cells. Fixed result order. Default cell 4 tiles (by measurement, below).
- A bug found by a test during this review: a never-rebuilt grid panicked on query (empty cell table). Fixed: a fresh grid is a valid empty grid. **VERIFIED** with a regression test.

### 4.4 Stopping at solids is a separate layer (`solid.rs`)
Detection answers "do these overlap". Stopping is its own thing: `Solids` = tile bitmap (whole-tile solids; one array lookup) + a `BroadPhase` of boxes (everything else). `step_body_solid` shares `accelerate` with the plain stepper, then moves **X then Y**, each resolved separately (bodies slide along walls), snapping flush against the nearest blocking edge. A body starting inside a solid walks out freely. Body hitbox is a `Footprint` (half-size + offset). **VERIFIED:** property test (never overlaps a solid over 20,000 busy ticks, for tiles and boxes), no tunnelling at top speed from four sides, and the pre-refactor golden replay hash is unchanged.

### 4.5 Tick scheduler (`schedule.rs`)
Each object has `ob_tick`: update once every `ob_tick` ticks (`1` every tick, `0` never). Objects are filed into **lanes by rate**; a lane with rate `n` has `n` buckets; on tick `t` only bucket `t % n` is walked; equal-rate objects are dealt round-robin (no spike every n ticks); lanes sorted by rate for a deterministic order; rates clamp to 216,000. **VERIFIED** by 9 tests. Design rule: movement and collision do **not** follow `ob_tick` (updating a mover every Nth tick either crawls or tunnels); `ob_tick` governs an object's own logic only. Level `ob_tick_scale` multiplies every interval.

### 4.6 Object model (`ianus_objects`)
`ObjectDef { name, ob_tick, collision, solid, hitbox, kind }`. `collision` is a per-object **master switch** (off means no hitbox exists: nothing touches it, nothing stops at it, it is noclip itself). `solid` means others stop at it; only matters with collision on. `blocks() = collision && solid`. Hitbox width/height/offset in **integer hundredths of a tile** (75 → exactly half-width 1536; integer division rounds down). `Kind` = Wall, Entity, Interactable, Environment, Light, SubstanceSource, each with category options. `WorldRules { collision, ob_tick_scale }` per level. The owner's latest framing: "objects are just things with hitboxes"; category is a preset. Room file: RON, human-readable, hot-reloaded on save; files lacking new fields still load (tested). Validator: errors and warnings in plain English, typo suggestions, contradictory-switch and tick-rate checks. A room with errors is refused and the previous room stays loaded. **VERIFIED:** 29 tests.

### 4.7 Runtime features
- **Input latching** (a tap shorter than a tick still moves exactly one tick; sabotage-tested), rebindable keys, works headless.
- **Interpolation:** render position blends previous→current tick by the fixed-loop overstep, computed in f64 then cast; spawned bodies don't slide in.
- **Play/Stop:** rooms load frozen. Play snapshots every body, tick and RNG; Stop restores exactly (world-hash equality tested; sabotage-tested). Edits before Play survive Stop; entities spawned during play are removed. **Limits:** only body position/velocity, tick and RNG are snapshotted; entities despawned during play are not restored.
- **Record/replay:** one input byte per tick + seed + final hash; playback reports IDENTICAL/DIVERGED; works in walled rooms; rooms need no restart hook (placed objects remember their start tile).
- **Colliders:** `Collider{footprint, solid}` exists only if the object's and the level's `collision` are on. `SolidMap` rebuilt from solid colliders (tile vs box chosen automatically). `ColliderIndex` = spatial hash of non-controlled colliders; the controlled entity is the *querier*, not stored (it moves every tick). `Touching` = sorted entities overlapped by the controlled entity.
- **Only the controlled entity moves and collides.** Everything else is static.

## 5. Measurements (VERIFIED; one machine: Ryzen 5 7600, release build; relative numbers)

Hitbox overlap formula, 4M random pairs: branchless `&` **1.66 ns**, short-circuit `&&` **6.1 ns** (3.7x), array-lane ties branchless, centre+half-size 2.07 ns and disagrees with the others on 16 pairs from rounding.

Spatial hash vs brute force, all-pairs, 1 box per 4 tiles²: 10,000 boxes **30.8 ms → 1.4 ms (22x)**. 50,000 and 200,000 were *estimated* for brute force (0.4 ns/test; real is nearer 1 ns, so true speed-ups are larger): about 69x and 138x. One player-sized query: **54 to 68 ns** vs 37,177 ns scanning 50,000 boxes. Rebuild every tick: 10,000 boxes **0.15 ms (0.9% of a tick)**, 100,000 boxes **2.9 ms (17%)**.

Cell size (50,000 boxes of 0.4 to 1 tile, three random worlds, median of 7), per box:

| cell | pairs | query |
|---|---|---|
| 1 tile | 176 ns | 74 ns |
| 2 tiles | 126 ns | 58 ns |
| **4 tiles** | **97 ns** | **54 ns** |
| 8 tiles | 115 ns | 64 ns |
| 16 tiles | 149 ns | 112 ns |

The earlier default of 2 tiles was a guess and the data moved it. Optimum tracks box size and density; one density measured.

Scheduler, 100,000 objects: typical room (80% never update, 12% every 30, 5% every 6, 3% every tick): buckets **2.8x** faster than "walk all, skip if not due", 15x faster than updating all; busiest tick 4,238 updates vs 20,005. All every 60 ticks: **16.7x** vs check-and-skip. **Everything every tick: buckets 0.9x (slightly slower).**

Benchmark flaws already found and fixed: a loop the compiler deleted (result unused), a mislabelled overlap share, and a non-monotonic cell sweep that needed more seeds.

## 6. Honest status

| Area | State |
|---|---|
| Tests | **161 pass** (`ianus_sim` 69, `ianus` 43, `ianus_objects` 29, `ianus_engine` 20), clippy clean, all examples compile, as of this brief |
| Runtime coverage gap | **No runtime-level test yet** for `Touching`, `DueObjects`, the per-object `collision` toggle or `WorldRules` in a running app. Only unit tests of helpers (`footprint_of`, `classify`, `Collider::for_def`). The phase ordering is exercised only indirectly |
| Cross-OS determinism | **Untested.** Golden hashes (movement, walled run) were produced on Windows only |
| GUI | Seen working on screen once; an intermittent **blank game view** in some runs is **unresolved** (unconfirmed theory: screenshot before pipeline compilation). Recent camera/scene-list changes **not seen on screen** |
| Not built | per-entity speed (ignored), environment speed (mud does nothing), lights, substance sources, interactables, AI/pathfinding, save files (only replays), editor placement, any scripting layer, objects that consume `DueObjects`, many moving agents, tilemap rendering, art |

## 7. Open owner decisions (do not decide these; state how your answers change)

1. **Scope:** 2D simulation only, or also platformers/Metroidvania or 3D visuals? (The first reviewer proposed both without evidence of demand; Godot and GameMaker already serve platformers well.)
2. **Audience reality:** evidence found: about 221 colony sims, 436 city builders, 477 automation games shipped on Steam in recent windows; colony sims reach 1,000 reviews at 13.7% vs 4.2% overall (one analytics site, tag bias); Bevy Jam entries flat at 64 to 98 since 2022; only three Bevy sims found in development. Is the niche big enough?
3. **Editor:** build on Jackdaw (a community Bevy editor, ~514,000 lines, `main` tracks Bevy main, Windows extension loading self-described as fragile; has a tile editor) vs writing one. Decided: Jackdaw distribution, renovated gradually. Never built or run. Its UI is Bevy's Feathers while ours is egui, so our panels would not port directly.
4. **Rust as scripting:** only feels like scripting if edit→running takes seconds. Bevy hot patching is opt-in and fragile; DLL reload is fragile. Fallback: snapshot → recompile → restore. **Untested; the largest technical risk.**
5. **Time/ownership:** the owner is a student; a coding assistant writes most code. Risk of owning determinism/save-format code she can't debug.

## 8. The questions (answer these)

### Tick model
1. Is `Input → World → Objects → Sim → Detect` the right order for a deterministic sim? What belongs where, and which ordering bugs (one-tick lag, order-dependent results with parallel systems) are likely? Does Bevy's scheduler give a deterministic order from system sets alone, or must we use `.chain()` and make ambiguities errors? How would you *test* that the order is deterministic?
2. Single 60 Hz tick plus `ob_tick`, or also a separate slower world tick (substances, weather)? Argue both.
3. How should an `ob_tick` object's logic be written so it is correct at any rate: catch up by elapsed ticks, or fixed-step? What breaks each approach?

### Scheduling
4. Is the bucket scheduler the best structure? Compare a time wheel, a priority queue of next-due tick, event-driven wake-ups, dirty flags. It loses slightly when everything updates every tick: should there be a fast path or an automatic switch?
5. How should objects that change their own `ob_tick`, spawn or despawn be handled without rebuilding and without breaking determinism?
6. In Bevy ECS terms: is iterating `Vec<(Entity,u32)>` with per-entity `world.get` efficient, or should due objects be marker components, archetype-based, or struct-of-arrays outside the ECS? Estimate the cost of random access.

### Collision at colony scale
7. Only one body moves today. For thousands of movers: full rebuild each tick (measured 0.15 ms per 10,000 boxes), incremental update of moved boxes, quadtree/hierarchy, sweep-and-prune, or per-tile occupancy lists (the other reviewer's suggestion)? Compare correctness, **determinism across save/load** (does iteration order inside a cell depend on move history, and does a loaded save rebuild it in a different order than a continuous run?), cost and simplicity. Give a benchmark design that would decide it.
8. Is X-then-Y axis-separated resolution acceptable? Which artifacts matter (corner catching, X/Y order bias)? When must moving solids (crates, doors) use swept AABB or continuous detection?
9. Would layers/masks pay for themselves? Estimate cost/benefit.
10. Should the grid cell size adapt automatically to object sizes and counts?

### Determinism and saves
11. List every way determinism can silently break in Rust/Bevy: hash-map iteration, system parallelism, `sort_unstable` on non-unique keys, `Entity` ID reuse across Play/Stop and save/load, floating point in `Transform`, allocator-dependent order, `libm` feature, anything else. Which are already handled here (say how to check), and which are not?
12. Design a save format with migration for long colonies: versioned per-component schemas, chunking, compression, forward/backward compatibility, and its relation to replays and the Play/Stop snapshot (today only position/velocity/tick/RNG).
13. What should the world hash include? Ours is an order-independent **sum** of per-body FNV hashes (weak against cancellation, no ordering burden). Alternative: order-dependent hash over a canonical order. How do we localise *where* two runs diverged (per-system hashes, per-tick logs, bisecting)? What does it cost per tick?

### Architecture
14. Is the Bevy-free `ianus_sim` / Bevy runtime boundary right? What should cross it?
15. Should simulation state live in Bevy components at all, or in a flat struct-of-arrays store with Bevy only for rendering/UI? Trade-offs for determinism, performance, inspector/undo tooling, hot reload.
16. Design the Rust scripting loop under Bevy's constraints: hot patching, dylib, rebuild-and-restore using snapshots, or an embedded interpreter for data-only logic. What does a script API look like if scripts must stay deterministic and be restrictable?
17. Data-driven `Kind` enum with built-in options, or objects composed from small behaviours with categories as presets only?
18. Is the per-object `collision` master switch plus separate `solid` the right split? What do mature engines do that we are missing (triggers vs colliders, layers, sensor shapes, one-way platforms)?

### Product
19. Is "deterministic sim layer" the best wedge, or is there a sharper one the evidence supports? What would make Ianus clearly best in its niche rather than another Bevy project?
20. What is the cheapest milestone that tests the biggest risk (question 16)?
21. What would you cut?

## 9. How the first review went wrong (do not repeat)

- It **did not answer the numbered questions**; it rewrote the spec.
- It **stated unbuilt things as facts** (a binary save format, per-tick hash logs, a platformer profile, a render bridge) while the repo has none of them as described.
- It proposed `Vec<Vec<u32>>` per-cell storage (an allocation per cell, the layout rejected here) with `swap_remove`, which makes order inside a cell depend on move history, a save/load divergence trap it never mentioned. It also justified avoiding a per-tick rebuild with no measurement, when the measured rebuild is 0.9% of a tick at 10,000 boxes.
- It converted subunits to `f32` before subtracting (`i32 as f32 / 4096.0`), which loses exactness beyond ~4,096 tiles from the origin; this code subtracts in f64 first.
- It called FNV-1a "cryptographic" (it is a non-cryptographic fingerprint) and used a hash that needs a canonical entity order without saying how to get one.
- Its platformer struct used a negative jump impulse, contradicting the +y-up convention used here, and mixed configuration with live state.
- It said the scheduler is "O(1) skipping"; cost is proportional to the objects that are due.
- It widened the target (3D PBR, Metroidvania) without evidence, which is the owner's decision.

Some of its points were right and are worth keeping in mind: explicit ordering of Bevy systems, avoiding raw `HashMap` iteration and unstable sorts on non-unique keys in deterministic paths, a many-mover headless stress test with a pinned checksum (needs a many-mover step first, and a non-Windows run), and optional per-tick hash logging for desync localisation.

## 10. Required output format

Produce exactly these parts, in this order, and nothing else:

**A. Answers.** For each question 1 to 21: a direct answer in at most 150 words, with a confidence label (*certain / likely / guess*), and the experiment that would confirm it if one exists. If a question depends on an owner decision, give the answer per branch.

**B. The five biggest risks**, ranked, each with the cheapest way to retire it.

**C. Concrete experiments.** For the three most uncertain questions, a small headless test or benchmark design: what to build, what to measure, what result would change the decision.

**D. Disagreements.** Anything in sections 4 to 7 you believe is wrong or misleading, with your reason.

**E. Things you could not judge** from this document, and what information you would need.

Do not summarise this brief back, praise it, or write marketing-style text.

## 11. Facts you can rely on

- Test/lint/bench commands (all headless): `cargo test --workspace --lib`, `cargo clippy --workspace --all-targets`, `cargo run --release -p ianus_sim --example bench_hitboxes`, `... --example bench_schedule`.
- Key files: `crates/ianus_sim/src/{aabb,broadphase,solid,schedule,rng,replay,lib}.rs`; `crates/ianus_objects/src/{lib,validate,tests}.rs`; `crates/ianus/src/{tick,body,input,play,room,collide,objects,session}.rs`; sample level `rooms/cellar.room`.
- Golden hashes are pinned in `ianus_sim` tests; changing simulation behaviour must change them deliberately and say why.
- Machine: Windows 11, Ryzen 5 7600, RTX 4080 SUPER, 31 GB.
- Bevy: official editor does not exist yet; BSN scene format shipped in 0.19 without a file loader (the room file uses RON as an interim format).

## 12. Glossary

**Tick** one sim step (1/60 s). **Subunit** integer length unit, 4096 per tile. **AABB** axis-aligned box. **Broad phase / spatial hash** a grid making "what is near here" a lookup. **Solid** others stop at it. **Collision (switch)** whether the hitbox exists at all. **`ob_tick`** ticks between an object's own updates. **Bucket** one pile of objects due on the same tick phase; piles that are not due are never opened. **Golden hash** a pinned fingerprint of a deterministic run. **Snapshot/restore** saving and re-applying exact sim state (used by Stop). **Quiet window** a window opened unfocused, behind others, out of the taskbar.
