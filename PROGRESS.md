# PROGRESS: what exists and how sure we are

*Last updated 2026-11-11. Labels: **VERIFIED** = run or measured; **WRITTEN** = code exists, no test exercises it end to end; **OPEN** = unknown or undecided.*

## Snapshot

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

Per-entity speed (ignored; global tuning applies); environment speed percent (mud does nothing); lights, substance sources, interactables (they only exist as data); any object logic or scripting layer; AI/pathfinding; save files (only replays); editor placement (cannot add/move/save objects from the GUI); anything but the controlled entity moving or colliding; tilemap rendering (placeholder coloured squares); art, audio; multiplayer; Jackdaw integration; CI.

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

- **Runtime tests from HANDOFF step 1: NOT WRITTEN.** `Collider`, `Touching`, `DueObjects` and phase order remain WRITTEN-only.
- **Known weaknesses confirmed by the review (not fixed):** hash has no object ID; `Touching` order depends on spawn history; hitbox hundredths round; no ambiguity detection on the tick schedule; possible missed rebuild when a collider is removed mid-play (UNVERIFIED, needs a test).
- **Cross-OS determinism:** still UNVERIFIED until `cargo test --workspace --lib` is run on the Mac.
