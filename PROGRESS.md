# PROGRESS: what exists and how sure we are

*Last updated 2026-11-12. Labels: **VERIFIED** = run or measured; **WRITTEN** = code exists, no test exercises it end to end; **OPEN** = unknown or undecided.*

## Snapshot

- **2026-11-12 pathfinding added (same branch, after the logic-skip fixes).** Verified on Linux (Rust 1.99.1): **226 tests pass** (`ianus_sim` 86, `ianus` 77, `ianus_objects` 43, `ianus_engine` 20), clippy zero warnings after a forced re-lint, rustfmt clean, all examples build, both golden hashes unchanged. What exists:
  - `ianus_sim::nav`: `NavGrid` (open tiles, tag bits, costs), `NavFilter` (allow/deny tags), `PathFinder` (A*, 8 directions, no corner cutting, fixed tie-breaking, reused buffers, expansion cap), `segment_clear` and `smooth_path` (swept hitbox checks). Tests: optimal cost equals an independent Dijkstra on 400 random weighted grids with filters; determinism with a reused finder; every tile expanded at most once; 8 of 8 deliberate breakages turned a test red.
  - **Hazard costs (Minecraft-style path malus), added after the first pathfinding commit.** `NavCosts`: per-tag extra cost for stepping onto a tile, per agent (`AgentDef.costs`, `NavAgent.costs`). The search takes the cheapest *total* route, so an agent wades through shallow water when the detour costs more and goes round fire. A tile with several costed tags costs its worst, not the sum. `avoid` still forbids outright and wins over a cost. The map is flattened once per kind of agent into a plain array of step costs (`NavGrid::cost_field`, cached in `NavFields`, rebuilt only when the nav map changes); the search (`PathFinder::find_in_field`) only does array lookups. Route smoothing may only cut across plain-cost tiles, so it cannot undo a detour the search chose. Tests: A* cost equals an independent Dijkstra on 300 random grids with random hazard costs; a three-corridor scenario (fire, water, clean) where the cheapest total wins and the answer flips as costs change; two agents with different costs do not share a field; costs reach the agent from a room file; 7 of 7 deliberate breakages turned a test red.
  - **Speed, measured** (`cargo run --release -p ianus_sim --example bench_nav`, this Linux machine, one core, 25% walls, hazards on, random start/goal pairs): about 90 us per search on 64x64, 320 us on 128x128, 1.4 ms on 256x256, 5.5 ms on 512x512 (about 0.2 us per tile looked at). Keeping the cost field saves 10 to 25% over rebuilding it every search. This is a baseline, **not** "insanely fast": fine for dozens of agents replanning occasionally, not thousands per tick. Not compared with Minecraft's own code (never read it).
  - Per-entity top speed: `accelerate_max`, `step_body_max`, `step_body_solid_max` (defaults call them with `MAX_SPEED`, so nothing else changed).
  - Object model: `Room.nav_tags`, `Room.nav_paint`, `ObjectDef.nav_tags`, `ObjectDef.agent` (`AgentDef`: target, allow, avoid, stop distance). All skipped in the file when empty, so `cellar.room` is byte-identical. Validation covers unknown/duplicate tags with "did you mean", bad paint positions, agents on the wrong kind of object, and risky settings.
  - Runtime (`ianus/src/nav.rs`): `NavMap` (rebuilt only when a solid moved/vanished or a tagged object moved/vanished; an identical grid keeps its version), `NavAgent`, `NavPath`, `NavStatus`, `NavTarget` (None, Controlled, Named, Entity, Tile). Systems in `Sim` after the player: track targets, plan (only for agents that are *due* under `ob_tick`, only when the target changed tile, the map or the walk rules changed, or the agent got stuck), then steer and move with the player's own physics at the agent's `speed_tiles_per_s`. Stuck agents retry on the plain tile route and give up after three strikes. Stop, Play and Restart clear routes, so replays that include agents match.
  - `rooms/chase.room` (generate with `cargo run -p ianus_objects --example write_chase`, open with `cargo run -p ianus_engine --example room -- rooms/chase.room`): a river with a bridge; the Nextbot avoids water, the Wader does not. Guarded by tests.
  - A randomised test builds mazes and checks that an agent arrives exactly when an independent flood fill says the player is reachable (60 mazes in the suite; 900 across three seeds passed when run once by hand). It found a real steering bug (a body clipping the mouth of a one-tile corridor), now fixed by proportional steering.
  - A fix made on the way: `rebuild_collision` no longer replaces the solid map when only a non-solid collider moved. Before, a walking creature would have forced the nav map to rebuild every tick.
  - **Not seen on screen.** No route drawing exists. Nothing in the window crate changed.
  - Limits, plainly: the nav grid is built for the standard 0.75-tile body (bigger hitboxes get a validator warning and may get stuck); agents do not avoid each other; a moving *solid* is treated as a wall; no doors or toggled obstacles; costs change which way an agent walks, not how fast (environment `speed_percent` and mud still do not slow movement); every agent runs its own A* (a flow field would suit hundreds of chasers at one target); target lookup by name scans all entities every tick; `Entity` index order still decides which of several same-named objects is chosen.

- **2026-11-12 logic-skip review: found, fixed, tested (branch `claude/logic-skips-investigation-7vqgu5`, not merged to `main`).** Each fix has a test that was made to fail by breaking the fix once (10 of 10 went red). Golden hashes did not change. **Verified on Linux (Rust 1.99.1) after the fixes: 172 tests pass (`ianus_sim` 71, `ianus` 49, `ianus_objects` 32, `ianus_engine` 20), clippy has zero warnings, rustfmt changes nothing, all examples build.** Golden hashes unchanged. The 161 figure further down is the old Windows count.
  - Despawned or collider-stripped walls kept blocking, and dead entities stayed in `Touching` and `DueObjects` (`Changed<SimBody>` cannot see removals; now also watches `RemovedComponents<Collider>`, and due objects are filtered for liveness).
  - Non-player movers were drawn at their old place (`PrevPos` only updated for the player; now set for every changed body at the start of a tick).
  - `Touching` was one tick late for anything that moved during `Sim` (collision structures are now also refreshed at the start of `Detect`; the cost is a second emptiness check per tick, and a second rebuild on ticks where something moved).
  - A solid player blocked itself through a ghost box from `SolidMap::from_room`.
  - A key tapped while paused fired on resume (taps are dropped while paused, unless a step is requested).
  - Tiny bodies could tunnel through thin solids (movement is now cut into pieces no longer than the body is wide; normal-size bodies are one piece, so behaviour is unchanged). A zero-area body still ignores everything; the validator now says so for entities.
  - `Scheduler::due` reported the full interval as elapsed on an object's first update; it now reports `tick + 1`, so elapsed ticks add up to ticks run.
  - Restart now also resets the RNG and says when it discards a recording; inspector moves in edit mode now update the object's `home` (they were reverted by Restart/F7/F6).
  - Validator: a light with radius 0 and intensity over 100 now gets both warnings; `ob_tick * ob_tick_scale` past the cap now warns.
- **Still open from that review (not fixed):** recordings silently stop growing at 1,000,000 ticks and would replay as DIVERGED; `Touching` order still depends on spawn history; the hash still has no object ID; the world hash only covers `SimBody`s; a body that starts inside a connected mass of solids can leave the room (documented, only whole-tile solids are validated). Window behaviour of the engine-crate changes (inspector `home`, Restart message) is WRITTEN only: no window has been opened.

- **161 tests pass** (`ianus_sim` 69, `ianus` 43, `ianus_objects` 29, `ianus_engine` 20). Clippy: zero warnings. All examples compile. (VERIFIED at the end of the last session.)
- **No window has been opened since the collision/scheduler rewrite.** Recent GUI changes have never been seen on screen.
- **Git repo, pushed to https://github.com/VoidR34ver/Ianus (`main`, one commit, 46 files; `target/` and `spikes/` ignored).** VERIFIED: push succeeded and the remote was empty beforehand. The test suite has **not** been run on the Mac or from a fresh clone, so "builds from a clean clone" is UNVERIFIED. Line endings: LF in the repo (Windows `autocrlf` on).
- Rust 1.99.1, Bevy 0.20.1, Windows 11 (Ryzen 5 7600, RTX 4080 SUPER). Owner also uses a Mac.
- The test counts above are from before the repo move; no code changed since (only docs: `INFO.md` added, `CLAUDE.md`, `HANDOFF.md`, `PROGRESS.md` updated).

## Done and verified

**`ianus_sim` (deterministic core)**
- Integer movement: top speed 341 subunits/tick (~5 tiles/s at 60 Hz), accel 60, friction 40, reversing = accel+friction, diagonals ×181/256, saturating at edges. Golden replay hash pinned.
- `Rng`: xoshiro256** via SplitMix64, `fork`, unbiased `below`. First outputs cross-checked against an independent Python implementation.
- `Replay`: file format (magic `IANR`, seed, one input byte per tick, optional final hash). Truncated/garbage input never panics.
- `Aabb`: half-open boxes, branchless 4-compare overlap (exact for non-empty boxes; `overlaps_checked` otherwise).
- `BroadPhase`: spatial hash, flat compressed-row layout, reused buffers, each box reported once without a dedupe set, deterministic order, out-of-bounds clamped. Matches brute force across 160 random worlds × 4 cell sizes. Default cell 4 tiles (by measurement).
- `Solids` + `step_body_solid`: tile bitmap + box hash; X-then-Y resolution (slides along walls); flush snap; body starting inside a solid walks out; never overlaps a solid over 20,000 busy ticks; no tunnelling. Pre-refactor golden hash unchanged after the rewrite.
- `Scheduler`: tick buckets by rate (`ob_tick`), round-robin dealing, deterministic order, clamp to 216,000. 9 tests.

**`ianus_objects`**
- `ObjectDef { name, ob_tick, collision, solid, hitbox, kind }`; `blocks() = collision && solid`; six `Kind`s with defaults per kind; `Hitbox` in hundredths of a tile; `WorldRules { collision, ob_tick_scale }`.
- RON room file; older files without new fields still load; settings serialise before `kind`.
- `Room::validate`: unknown/duplicate names with "did you mean", bounds, zero sizes, stacked solids, entity inside solid, controlled-entity count, pointless options, contradictory switches, bad tick rates, switches pointing at nothing. Errors refuse the load and keep the old room.

**`ianus` (runtime)**
- Fixed tick, input with **latching** (a tap shorter than a tick still moves one tick), rebindable keys, headless-safe. Interpolation in f64.
- Pause / step (exactly one tick) / speed.
- **Play/Stop** exact snapshot-restore (positions, velocities, tick, RNG); edits before Play survive; things spawned during play are removed. Sabotage-tested.
- Record/replay, IDENTICAL/DIVERGED reporting, works in walled rooms, rooms need no restart hook.
- Room loading with refusal on errors; `FileWatch` for hot-reload.
- Player stops at walls and the room edge (sabotage-tested).

**Benchmarks** (release, one machine, relative numbers)
- Overlap formula: branchless **1.66 ns** vs `&&` **6.1 ns** (3.7×).
- All-pairs, spatial hash vs brute force: **22×** at 10,000 boxes (30.8 ms → 1.4 ms); brute-force figures beyond 10,000 were estimated.
- One player-sized query **54 to 68 ns** vs 37,177 ns scanning 50,000 boxes.
- Rebuild every tick: 10,000 boxes 0.15 ms (0.9% of a tick); 100,000 boxes 2.9 ms (17%).
- Cell-size sweep: 4 tiles best (pairs 97 ns/box, query 54 ns); earlier guess of 2 tiles was wrong.
- Scheduler (100,000 objects): typical room 2.8× faster than check-and-skip, 15× faster than update-all; all-every-60 16.7×; **all-every-tick 0.9× (slightly slower)**.

## Written but not covered by a runtime test

- `Collider` per object (`collision` on and level `collision` on), `SolidMap` built from solid colliders (tile vs box chosen automatically), `ColliderIndex` rebuilt in the `World` phase only when a non-controlled collider moved, `Touching` for the controlled entity.
- `ObjectSchedule` + `DueObjects` (level `ob_tick_scale` applied; 0 stays 0). **Nothing consumes `DueObjects` yet.**
- The phase chain `Input → World → Objects → Sim → Detect` (ordering only exercised indirectly).
- Only helper unit tests exist (`footprint_of`, `classify`, `Collider::for_def`).

## Engine / GUI state

- egui layout: toolbar (Play/Stop, Pause, Step, speed, Record, Replay, Restart), Scene list grouped by category (Wall/Environment folded), Inspector (position edit, category/options read-only), Simulation panel (tick, messages, frame graph). Click-to-select. Camera frames the room while editing and follows the player while playing. Rooms hot-reload on save (wait for Stop if playing).
- Shortcuts: F5 Play/Stop, F1 pause, F2 step, F3 speed, F4 hide panels, F6 replay, F7 record, F8 reload.
- **Seen working on screen once** (room rendered with correct colours per category, panels, 138 fps). **Not seen:** edit-mode camera framing, grouped Scene list, anything since.
- **OPEN BUG: intermittent blank/black game view.** Several early runs showed correct panels but a blank game view; a later run was fine. Unconfirmed theory: the screenshot was taken before render pipelines finished compiling (screenshot at frame ~60 vs 200). Not proven.
- Quiet window mode (unfocused, bottom-most, no taskbar) is default for automated runs and unit-tested. One quiet run showed the game window did not take focus, but launching with `Start-Process` opened a console that did. `tools/quiet-run.ps1` is the fix and **has never been run**.

## Not built (do not assume these exist)

Per-entity speed for the *player* (agents use theirs; the player still uses the global tuning); environment speed percent (mud does nothing); lights, substance sources, interactables (they only exist as data); any object logic or scripting layer; AI beyond chasing and walking to a tile (no wandering, no avoidance between agents, no doors); save files (only replays); editor placement (cannot add/move/save objects from the GUI); anything but the controlled entity and agents moving or colliding; tilemap rendering (placeholder coloured squares); art, audio; multiplayer; Jackdaw integration; CI.

## Untested claims

- **Cross-OS determinism.** Golden hashes exist only from this Windows machine. Run the tests on Linux/macOS before trusting the claim.
- Whether the blank-view fix (plain `Camera2d`, projection edited after) was the real cause.
- Hot-patching / fast rebuild loop for Rust scripting (the biggest technical risk) has not been tried at all.

## Bugs found and fixed (for context)

1. `BroadPhase` panicked when queried before its first rebuild (empty cell table). A room with only the player triggered it. Fixed, regression test.
2. `Aabb::overlaps` was documented as "empty boxes overlap nothing"; a test showed an empty box inside another overlaps. Doc corrected, `overlaps_checked` added; structures skip empty boxes.
3. Scene list order was reversed because `Entity::to_bits` doesn't sort in spawn order. Now `entity.index()`.
4. A weak test (passed vacuously when the tap latch was disabled) was strengthened.
5. First cell-size default (2 tiles) was a guess; data chose 4.

## Documents

`INFO.md` (overview and standing, new), `HANDOFF.md`, `CLAUDE.md`, `ianus-plan.md` (early research; the "sim layer only" pivot in it was rejected by the owner), `ENGINE-HANDOVER.md` (v1 brief for an outside reviewer), `ENGINE-BRIEF-2.md` (v2, sharper). **An outside AI reviewed v1; its answer rewrote the spec instead of answering and contained several mistakes** (see v2 section 9). **v2 was answered on 2026-11-11** by an outside AI; its code-level claims were checked against the source (details in `HANDOFF.md`): the hash-without-ID, `Changed<SimBody>` trigger, entity-index sorting and hitbox rounding claims are VERIFIED in code; several recommendations are overreach. Nothing from the review is implemented yet.

## Open items added 2026-11-11

- **Runtime tests from HANDOFF step 1: PARTLY WRITTEN.** The 2026-11-12 tests cover removal/despawn, `Touching` timing and `DueObjects` liveness. Still missing: per-object collision toggle, `WorldRules.collision = false`, `ob_tick` rates and scale through the real schedule, edit-mode drag then first Play tick, replay in the cellar with the new phases.
- **Known weaknesses confirmed by the review (not fixed):** hash has no object ID; `Touching` order depends on spawn history; hitbox hundredths round; no ambiguity detection on the tick schedule; possible missed rebuild when a collider is removed mid-play (was real; FIXED 2026-11-12, tested).
- **Cross-OS determinism:** still UNVERIFIED until `cargo test --workspace --lib` is run on the Mac.
