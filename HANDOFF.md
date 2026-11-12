# HANDOFF: start here

*Written 2026-11-11 for the next agent. Read `CLAUDE.md` (rules), `INFO.md` (overview) and `PROGRESS.md` (state) too. Updated later the same day: repo pushed to GitHub, outside review answered.*

## 2026-11-12: pathfinding (read after the review note below)

Pathfinding is built and tested on the same branch (details in `PROGRESS.md`). Decisions for the owner, with my recommendation:
- **Many chasers at one target:** today each agent runs A*. For hundreds, add a flow field (one search from the target, every agent follows the gradient). Recommended before a colony-scale test, not before.
- **Bigger bodies:** the nav grid assumes a 0.75-tile body. Per-footprint grids (cached) are the fix; only needed once something bigger than a tile exists.
- **Doors and toggled obstacles:** need a rule for whether agents may plan through something that can open. Not decided.
- **Tag costs (mud is slower, not forbidden):** the grid already stores costs and A* uses them; nothing sets them yet. Wanting it means deciding whether environment `speed_percent` should also slow movement.
- **Editor:** painting tags onto tiles and drawing routes needs the window; nothing is built there, so tags are authored in room files or code today.

## 2026-11-12: logic-skip review

Branch `claude/logic-skips-investigation-7vqgu5` holds a review that found nine logic skips, fixes for them and tests (list in `PROGRESS.md` Snapshot). It is **not merged**; the owner decides. Step 1 below is now partly done (see `PROGRESS.md` open items). The Rust toolchain on a fresh machine must be 1.99.1 (`cargo` 1.97 fails on Bevy 0.20 dependencies). Open questions for the owner: minimum hitbox size (validator only warns today); whether a tap during pause should be remembered for stepping; replay length cap behaviour.

## What changed since the first version of this file

- **Repo is on GitHub** (https://github.com/VoidR34ver/Ianus, `main`, one initial commit). The owner will also work from a Mac. Commit/push only on request.
- **The outside review of `ENGINE-BRIEF-2.md` came back** and was checked against the code (below). Nothing from it has been implemented.
- **Step 1 below (runtime tests) is still not written.** I had only read `room_tests.rs`, `collide.rs`, `objects.rs`, `room.rs`, `tick.rs` and `tests.rs` when work stopped. Useful facts from that read: `test_app(1.0)` gives one tick per `app.update()`; `loaded_app()` starts paused in Edit mode, so call `play(app.world_mut())` first; `DueObjects` after an update holds the list for tick `ticks(&app) - 1` (the tick counter advances after `Detect`); a single `ob_tick: n` object is due when `tick % n == phase`, phase dealt round-robin from 0; `Entity` defs default to a 75 hitbox, non-solid; Light/Wall/Environment default to `ob_tick` 0. New tests can go in `room_tests.rs` (or a new `#[cfg(test)] mod` in `lib.rs` next to `mod room_tests;`).

## Outside review: what held up (checked 2026-11-11)

**Confirmed in the code:**
- World hash (`tick.rs:160`) is a wrapping sum of per-body FNV-1a with **no object ID**: swapping two bodies' states is invisible.
- `rebuild_collision` triggers on `Changed<SimBody>` (`collide.rs:178`).
- `Touching` is sorted by `Entity::index()` (`collide.rs:221`), which depends on spawn/despawn history.
- `footprint_of` (`collide.rs:49`) uses `/ 100`, so hundredths round and negative offsets truncate toward zero. 75 is exact (1536); most values are not (1 hundredth = 40.96 subunits).
- No ambiguity detection is configured. `HashSet` is used in `play.rs` for membership only; `HashMap` in `validate.rs` (not sim state).

**Overreach or wrong (my assessment):**
- "Use `div_euclid` for offsets": truncation keeps mirrored offsets mirrored; flooring would not. Pick exact units instead (1/64 tile) if this matters.
- "Tunnelling cannot happen": assumes a minimum hitbox size; whether the validator enforces one is **unverified**.
- `Changed<T>` risk is mainly a *missed* rebuild (collider removed/despawned during play). Add a test.
- "Detect last makes Touching stale": only for logic running after movement in the same tick.
- Experiment 1's rule "any mismatch means leave the ECS" is too strong.
- Full state-out-of-Bevy (Q14/Q15) resembles the rejected "sim layer only" pivot and conflicts with the Jackdaw route. A middle path: move only the logic into `ianus_sim`, keep state in the ECS.
- Guesses to treat as guesses: "live coding with rewind that no other engine has", the ECS cost numbers.

**Cheap fixes that do not depend on the owner's big decisions:** add the object's ID to the per-body hash (this changes golden hashes; update them deliberately and say why); clippy lints banning floats and `HashMap`/`HashSet` in `ianus_sim`; fix the "rounds down" doc comment; run the tests on the Mac (cross-OS check); consider CI (Linux/macOS/Windows).

**Reviewer's three experiments** (determinism harness, scripting-loop spike, many-mover broadphase) are good designs; none is run yet.

## Where we stopped

The owner redesigned collision this session and asked for an optimised, measured design: **objects are things with hitboxes; detection and stopping at walls are separate; collision is a per-object master toggle; objects update at their own `ob_tick`; level-wide rules apply first.** All of that exists in the sim and object model and is tested. The **runtime wiring** (colliders, `Touching`, `DueObjects`, new tick phases) compiles and the whole workspace is green (161 tests, clippy clean), but **no runtime test exercises it end to end**, and nothing has been looked at on screen since.

The owner then asked for briefs for outside AI reviewers (`ENGINE-BRIEF-2.md` is the good one) and for these three files. She has **not** yet said whether to send the brief or what to build next.

## Do these first, in order

### 1. Close the runtime test gap (small, high value, headless)
In `crates/ianus/src/room_tests.rs` (helpers `test_app`, `hold`, `loaded_app`, `cellar` exist) add tests that run the real schedules for:
- **Per-object collision toggle:** a Wall with `collision: false` does not stop the player; a solid Pillar (hitbox 50, off-grid) stops the player at its real edge; a non-solid Entity with collision on does not stop but appears in `Touching`.
- **`Touching`:** walking onto a crate lists it; flush-against is **not** touching; order is by entity index; empty when nothing overlaps; empty (and no panic) when the room has only the player.
- **`WorldRules.collision = false`:** no colliders exist, `Touching` empty, player passes through walls (document that this also ignores the room edge).
- **`DueObjects`:** an `ob_tick: 3` object is due on ticks 0,3,6...; `ob_tick: 0` never; level `ob_tick_scale: 2` doubles the interval; same-rate objects are spread across ticks; `DueObjects` is cleared each tick; due count matches `ObjectSchedule::due_count`.
- **Phase order:** drag a wall in edit mode (change its `SimBody`), press Play, and confirm the **first** tick already respects the new position (the rebuild runs in `World`, before `Sim`).
- **Replay with the new phases:** record/playback in the cellar still matches (extend the existing test).
Then **break the code on purpose** for each (for example skip the rebuild) and confirm the test fails.

### 2. Verify on screen (needs the owner's permission, one quiet run)
Ask first. If yes: `tools/quiet-run.ps1 -Exe target\debug\examples\room.exe` with `IANUS_EXIT_AFTER_FRAMES=260`, `IANUS_SCREENSHOT`, `IANUS_SCREENSHOT_FRAME=200`. Check: foreground window unchanged, room framed whole in edit mode, Scene list grouped, no blank game view. Count orange player pixels instead of eyeballing. This also tests the launcher.

### 3. Then ask the owner what next. Candidates, with my recommendation
- **(Recommended) Many movers + a stress test.** Only one body moves today; a colony sim needs thousands. Build a many-mover step system, then Milestone: 10,000 movers × 10,000 ticks, headless, pinned checksum, **and run it on Linux/macOS** (cross-OS determinism is unproven). Before choosing a dynamic collision structure, **benchmark** the options (rebuild a flat spatial hash each tick, incremental updates, per-tile occupancy lists with flat storage and canonical order). Measured: rebuilding 10,000 boxes costs 0.15 ms, so the simple option may already be enough.
- **Make the existing categories do something:** per-entity speed, environment `speed_percent` (mud), switches toggling their target, lights, substance sources (deterministic diffusion is its own mini-project).
- **Editor placement** (add/move/save objects from the GUI), the owner's GameMaker-style goal.
- **Rust scripting loop spike** (biggest technical risk): time edit→running with Bevy hot patching vs rebuild-and-restore using snapshots. Untested. Target under about 5 seconds.
- **Save files** with versioned schemas and migration (today only replays).

## Decisions that are the owner's (ask, do not decide)

1. **Scope:** 2D simulation only, or also platformers/Metroidvania or 3D visuals? An outside reviewer proposed both without evidence; Godot and GameMaker already serve platformers well.
2. ~~Whether to send `ENGINE-BRIEF-2.md`~~ Done; it was answered and checked (see above). Whether to send it to a *second* AI is open.
2b. **Architecture forks raised by the review:** where sim state lives (ECS vs separate store, or the middle path); phase order (current vs Commit/Move/Detect/Logic with queued commands); hitbox units (hundredths vs 1/64 tile); `Kind` enum at runtime vs composed presets, plus layers/masks; whether Jackdaw waits for a scripting spike. Give options and a recommendation, then let her choose.
3. **Editor route** (decided: Jackdaw distribution, renovated gradually) is untested; Jackdaw was never built. Revisit only after her call.
4. **What "success" means** at 6 and 18 months, and her hours per week. (She said "Claude codes while I do my Matura", so review time is the bottleneck, not typing.)

## Things the owner has been explicit about

- Windows must not open without asking; she lost a ranked match to a focus-stealing test window.
- She wants objects to be "things with hitboxes", collision as a per-object toggle, stopping at walls as its own thing, and `ob_tick` set at the top of each object. **Design rule already applied:** movement and collision run every tick regardless of `ob_tick` (otherwise movers crawl or tunnel); `ob_tick` governs the object's own logic only.
- She wants the engine optimised "as much as possible" and **measured**. Benchmarks beat assumptions here (a guessed cell size was wrong).
- Walls and non-walkable ground are solid by default; Entity hitbox default 75% so it fits one-tile gaps.

## Pitfalls to watch for

- **Order-dependent state across save/load.** Any structure whose internal order depends on history (for example `swap_remove` in per-cell lists) will make a loaded save diverge from a continuous run. Prefer rebuilt-in-canonical-order structures, or sort.
- **`Entity` IDs** are not stable across despawn/respawn; Play/Stop currently restores only existing entities. A save format needs stable IDs.
- **The world hash is an order-independent sum** of per-body hashes (robust to order, weak to cancellation). Per-tick hash logging for desync localisation is not built.
- **Only the controlled entity** moves/collides; `ColliderIndex` excludes it by design. A many-mover design changes that assumption everywhere (`step_controlled`, `rebuild_collision`, `detect_touching`).
- **`ob_tick_scale` of 0** means nothing ever updates (validator warns). `ob_tick` above 216,000 is clamped.
- **Bevy churn:** each Bevy release breaks about 75 to 100 APIs. Keep logic in the Bevy-free crates; keep Bevy-specific code thin.
- The GUI intermittently showed a blank game view early on. If it recurs, suspect pipeline compile timing first (screenshot later), then camera/viewport setup.

## Quick orientation

- Tick phases: `crates/ianus/src/tick.rs` (`IanusSet`), systems in `body.rs` (`step_controlled`, interpolation), `collide.rs`, `objects.rs`, `room.rs`, `play.rs`, `session.rs`.
- Wall stopping: `crates/ianus_sim/src/solid.rs`. Spatial hash: `broadphase.rs`. Scheduler: `schedule.rs`.
- Object/room format and validation: `crates/ianus_objects/src/{lib,validate}.rs`. Sample: `rooms/cellar.room`.
- GUI: `crates/ianus_engine/src/{lib,gui,actions,rooms}.rs`.
- Benchmarks: `crates/ianus_sim/examples/bench_{hitboxes,schedule}.rs`.

## Definition of done for any change

`cargo fmt --all`, `cargo test --workspace --lib` green, `cargo clippy --workspace --all-targets` with zero warnings, the new behaviour has a test that was seen to fail when the code was broken, golden hashes unchanged (or changed deliberately and explained in `PROGRESS.md`), `PROGRESS.md` updated with what is verified vs written, and no window opened without permission.
