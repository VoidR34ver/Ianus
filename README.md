# Ianus

A 2D game engine in Rust, built on Bevy, aimed at simulation games. Planning
and research: [ianus-plan.md](ianus-plan.md).

## Crates

| Crate | What it is | Depends on Bevy? |
|---|---|---|
| `ianus_sim` | The deterministic core: integer movement, seeded RNG, replay file format. No floats, no clocks, bit-identical on every OS. | No |
| `ianus` | The runtime: fixed 60 Hz tick, input that never drops a quick tap, interpolated rendering, pause/step/speed, record and replay, game restart hook. | Small sub-crates only (no renderer) |
| `ianus_objects` | GameMaker-style objects and rooms: six categories (Wall, Entity, Interactable, Environment, Light, Substance source) with built-in options, a human-readable room file, and a validator that explains mistakes in plain English with "did you mean" hints. | No |
| `ianus_engine` | The engine you run: window, 2D camera that follows the player, editor-style GUI (toolbar, scene list, inspector, simulation panel), debug keys. | Full Bevy |

## Try it

```text
cargo run -p ianus_engine --example room
```

Opens `rooms/cellar.room` in the editor window. The first build compiles Bevy
and takes several minutes; later builds take seconds.

The room starts in **edit mode**: nothing moves. **Play** runs it, **Stop**
puts everything back exactly as it was when you pressed Play. Edit and save
the room file in any text editor and it reloads by itself (while playing it
waits for Stop). If the file has a mistake, the Messages panel explains it in
plain English and the previous room stays loaded.

To remake the sample room: `cargo run -p ianus_objects --example write_cellar`.

| Key | Does |
|-----|------|
| WASD / arrows | move (while playing) |
| F5 | Play; press again to Stop and restore |
| F1 | pause / resume (from edit mode: starts playing) |
| F2 | advance exactly one tick (from edit mode: plays paused) |
| F3 | speed: 1x, 2x, 4x, 0.5x |
| F4 | show / hide the editor panels |
| F6 | restart and play `last.ianr` back; reports IDENTICAL or DIVERGED |
| F7 | start / stop recording (stopping saves `last.ianr`) |
| F8 | reload the room file now |

## Tests

```text
cargo test --workspace
```

## Rules of the simulation

* Nothing inside the simulation uses floats, the clock, or OS randomness.
* Systems read `TickInput`, never the keyboard, so live play and replay run the
  same code.
* A game must register a restart hook (`App::set_restart_hook`) that puts
  every simulated thing back to its start, or replays will diverge. Playback
  tells you when they do.
