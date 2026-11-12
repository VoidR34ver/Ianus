# INFO: what Ianus is and where it stands

*Written 2026-11-11. Short overview for a new reader (a person, or an AI picking this up). Rules live in `CLAUDE.md`, next steps in `HANDOFF.md`, detailed status in `PROGRESS.md`. Repo: https://github.com/VoidR34ver/Ianus*

Labels used everywhere: **verified** (run or measured), **written** (code exists, no end-to-end test), **unverified**, **conflicting**.

## The project in one paragraph

Ianus is a 2D game engine in Rust on Bevy 0.20.1, aimed at **deterministic simulation games** (colony sims first). The model is GameMaker-style: objects and rooms, with Rust as the scripting language and an editor planned on top of Jackdaw. The owner (Zephie, student in Switzerland, learning Rust, limited time) directs; an AI writes most of the code. Review time, not typing, is the bottleneck.

## The core idea

Same start state plus the same per-tick input gives **bit-identical results on any machine**. That is what makes replays, "paste a bug report and reproduce it", Play/Stop, headless batch runs and (maybe) lockstep multiplayer possible. To keep it true, the simulation crate uses integer maths only: no floats, no Bevy, no clocks, no OS randomness. Positions are `i32` subunits, 4096 per tile.

## How it is built

| Crate | Job |
|---|---|
| `ianus_sim` | Deterministic core: movement, seeded RNG, replay file, boxes, spatial hash, wall stopping, tick scheduler. No Bevy. |
| `ianus_objects` | Data model: objects, hitboxes, level rules, room file (RON), plain-English validation. No Bevy. |
| `ianus` | Bevy runtime plugin without a renderer: tick phases, input, record/replay, Play/Stop, room loading, colliders, `Touching`, `DueObjects`. |
| `ianus_engine` | The window: egui panels, camera, quiet mode, room hot-reload. |

Each tick runs `Input → World → Objects → Sim → Detect`. Objects are "things with hitboxes"; detection (who touches whom) and stopping (solids) are separate; collision is a per-object switch plus a level switch; each object has an `ob_tick` update rate.

## Where it stands (2026-11-11)

**Works and verified:** deterministic movement with a pinned golden hash; replay record/playback; exact Play/Stop restore; the player stops at walls and the room edge; room loading with refusal on errors; the spatial hash (matches brute force on 160 random worlds); the tick scheduler; benchmarks (hash rebuild for 10,000 boxes costs 0.15 ms). 161 tests passed and clippy was clean on Windows before the 2026-11-12 review; the review branch `claude/logic-skips-investigation-7vqgu5` adds fixes and tests (see `PROGRESS.md`).

**Written but not tested end to end:** per-object colliders, `Touching`, `DueObjects`, the phase chain. Nothing consumes `DueObjects` yet.

**Built on the review branch:** pathfinding (A* with walk-on and avoid tags) and agents that chase the player or walk to a tile; see `PROGRESS.md`.

**Not built:** anything but the controlled entity and agents moving; avoidance between agents; save files (only replays); placing objects from the GUI; scripting layer; lights, substances, interactables (data only); art and audio; CI; Jackdaw integration (never built).

**Unproven, biggest risks:**
1. The Rust edit-and-run loop (does "Rust as scripting" feel fast enough?). Untried.
2. Determinism at the Bevy boundary: logic lives on the Bevy side, snapshots are incomplete.
3. Cross-OS determinism: golden hashes come from Windows only. The first Mac run is the first evidence.
4. Colony scale: no many-mover stress test yet. Pathfinding exists but runs one A* per agent; a flow field is the likely next step for crowds.
5. Ownership: keep invariants mechanical (tests, lints, CI) so a broken rule fails loudly.

## Outside review of the design (answered 2026-11-11)

`ENGINE-BRIEF-2.md` was answered by an outside AI. Treated as hypotheses, checked against the code:

- **Confirmed in the code:** the world hash is a plain sum with no object ID (swapping two bodies' states goes unnoticed); the collider rebuild is triggered by `Changed<SimBody>`; `Touching` is sorted by `Entity` index, which depends on spawn history; hitbox hundredths of a tile are not exact in subunits (75 happens to be); negative offsets truncate toward zero.
- **Overreach or wrong:** moving all sim state out of Bevy is close to the "sim layer only" pivot already rejected; "tunnelling cannot happen" assumes a minimum hitbox size nobody checked; `Changed<T>` is risky mainly for a *missed* rebuild.
- Details are in `HANDOFF.md`.

## Open decisions (owner's, not the AI's)

1. Scope: 2D simulation only, or also platformers / 3D.
2. Where sim state lives: Bevy ECS, or a separate Bevy-free store. A middle path: move only the logic (`Touching`, due objects, collider rebuild) into `ianus_sim`, keep state in the ECS.
3. Phase order: keep the current one, or the reviewer's Commit/Move/Detect/Logic with queued commands.
4. Hitbox units: hundredths of a tile (friendly) or 1/64 tile (exact).
5. `Kind` enum at runtime, or editor presets over small composed parts; layers and masks.
6. Jackdaw: on hold until a Rust scripting spike passes?
7. What success means at 6 and 18 months, and hours per week.

## Next steps (recommended)

1. Finish the headless runtime tests in `HANDOFF.md` step 1 (partly written on the review branch), then sabotage each to prove it can fail.
2. Run `cargo test --workspace --lib` on the Mac and compare golden hashes.
3. Small fixes that do not depend on the big decisions: put an object ID into the per-body hash, add lints banning floats and `HashMap` in `ianus_sim`, fix the "rounds down" doc comment.
4. Then pick: many movers plus stress test (recommended), or the scripting-loop spike.

## Working on it from another machine

The repo is on GitHub (`main`). On a Mac: install Rust 1.99.1 via rustup, clone, `cargo test --workspace --lib`. The first Bevy build takes about 10 minutes. The window examples need the owner's OK before they are run. Line endings are LF in the repo.
