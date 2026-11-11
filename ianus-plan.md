# Ianus: Plan, Critique, and the Path to Owning the Niche

*Research date: 2026-11-11. Planning document only, no engine code. All web sources accessed 2026-11-11.*

**Claim labels:** **[V]** verified against a source today · **[U]** unverified (my memory, a weak source, or nobody has measured it) · **[C]** sources conflict · **[J]** my judgement, not a fact.

## Contents

0. [Executive summary](#0-executive-summary)
1. [Name check](#1-name-check-ianus)
2. [State of the world](#2-state-of-the-world)
3. [Niche validation](#3-niche-validation)
4. [Differentiation and the wedge](#4-differentiation-and-the-wedge)
5. [How Ianus becomes king of the hill](#5-how-ianus-becomes-king-of-the-hill)
6. [Architecture options](#6-architecture-options)
7. [Roadmap, cut line, kill criteria](#7-roadmap-cut-line-kill-criteria)
8. [Community and sustainability](#8-community-and-sustainability)
9. [Risk register](#9-risk-register)
10. [Open questions only you can answer](#10-open-questions-only-you-can-answer)
11. [What I could not verify](#11-what-i-could-not-verify)
12. [Sources](#12-sources)

---

## 0. Executive summary

**Direction (owner, 2026-11-11):** Ianus is a **GameMaker Studio 2-style product**: a general 2D engine with its own editor, **Rust as the scripting language**, that **excels at simulation**. The sim-only pivot I first recommended is rejected. Section 5 is rewritten for this direction; sections 2 to 4 are the evidence it has to survive.

**My recommendation for that direction: GO, but in a fixed order and on borrowed foundations.**

- **Sim layer first, editor second.** The sim layer is the "excels at" half and the only part with no competitor. It also makes the editor's hardest feature (play, stop, restore) cheap.
- **Decided 2026-11-11: Ianus is its own editor, started as a Jackdaw distribution and renovated piece by piece** (section 5.3). Own name, look and panels from the start; Jackdaw parts get replaced one at a time, each with a stated reason. Writing a shell from zero is the part that killed space_editor and that Fyrox needed seven years for.
- **"Rust as scripting" is the make-or-break feature.** It only feels like scripting if the edit-to-running loop is seconds. Section 5.2 covers what that needs and where it is fragile today.
- **The audience is not GameMaker's audience.** GameMaker serves beginners; Rust excludes most of them. Ianus serves people who already want Rust and miss GameMaker's convenience. That is a smaller group, and the plan is sized for it.

The evidence behind this:

- **The general editor slot is taken or about to be.** Bevy 0.20 (2026-10-08) has no first-party editor, but the upstream editor MVP is the project's stated top priority, and the community editor Jackdaw already saves `.bsn` scenes with undo/redo. **[V]** A solo student cannot win that race, and does not need to.
- **The pain is real, but it is not "optimised 2D".** RimWorld left Unity for an in-house engine in October 2026 (2 to 4 times faster). Cities: Skylines 2's studio says it overestimated Unity. Factorio, Captain of Industry and shapez 2 all hand-built their sim cores and renderers. **[V]** What breaks is simulation scale, determinism, saves and modding, not sprite drawing.
- **Those sim problems are the gap nobody in Bevy is filling.** Bevy's roadmap is scenes, assets, editor and rendering. Nothing on it targets deterministic sim, save migration or data-table tooling. **[V]** for the roadmap, **[J]** for "nobody".
- **The hill is small.** 221 colony sims and 436 city builders shipped on Steam in 2025, plus 477 automation games in the 18 months to June 2025. **[V]** The Rust share of that is tiny **[U]**, and Bevy Jam entries have stayed between 64 and 98 since 2022. **[V]** Being "king" means being the default for perhaps dozens of serious teams. That is achievable, but it is not a mass-market product.

**Top 5 risks**

1. **Thin niche:** the realistic user base is tens of teams, not thousands.
2. **Solo burnout:** Matura plus an engine plus a game is three jobs.
3. **Bevy churn:** about 100 breaking changes in 0.19 and 75 in 0.20, every 4 to 5 months. **[V]**
4. **Scope creep into a general editor:** the single most likely way this dies.
5. **Sim teams write their own core anyway:** every famous example did.

**Also:** the name collides with **IANUS Simulation GmbH** (Dortmund, simulation software). See section 1.

---

## 1. Name check: "Ianus"

| Where | Result | Label |
|---|---|---|
| crates.io `ianus` | The API returned 404, so the crate name appears free. Confirm by hand before relying on it. | [V] (single check) |
| GitHub `github.com/ianus` | Taken by a personal account with one repo. You would need `ianus-engine` or similar for the org. | [V] |
| Existing game engine or Rust project called Ianus | None found in two searches. | [V] (absence in search, not proof) |
| **IANUS Simulation GmbH** | German company in Dortmund, founded 2006, about 50 staff, sells simulation and AI software (StrömungsRaum). The name stands for "Industrially Applied Numerics and Simulation". | [V] |
| TMview (aggregates Swiss IGE, EUIPO, DPMA, WIPO and others), searched 2026-11-11 | 339 hits including similar names; about 30 exact "IANUS" marks. **None is for games or game software.** Switzerland: one, class 33 (wine). Germany: one, classes 41/42, filed 2006-07-21, **expired** (date matches IANUS Simulation's founding year; owner not checked). WIPO and Canada class 9 (2020): CEIA S.p.A., metal detectors. Software-adjacent and live: Spain (classes 9/42, 2006; class 42, 2014), Italy (35/38/41/42, 2001), Chile (9/42, 2026); owners not checked. | [V] for the list, [U] for owners. TMview is not an official register and this is not legal advice. |
| Swissreg directly | Not searched; TMview includes Swiss data. | [U] |
| "Janus" spelling | Crowded in software (Janus WebRTC gateway, JanusGraph, JanusVR). People will mishear and misspell "Ianus" as "Janus". | [U] (from memory) |

**Critique [J]:** a German-speaking simulation software company with the same name, in your own language region, selling into trademark classes 9 and 42, is the one collision worth taking seriously. An open-source game library is a different market, so this may be fine, but check Swissreg and TMview before buying a domain or printing a logo. It costs ten minutes.

---

## 2. State of the world

### 2.1 Bevy

| Topic | Status on 2026-11-11 | Label |
|---|---|---|
| Latest release | **0.20**, 2026-10-08, 227 contributors, 817 PRs | [V] |
| Cadence | 0.17 on Sept 30 2025, 0.18 on Jan 13 2026, 0.19 on Jun 19 2026, 0.20 on Oct 8 2026. Roughly every 3.5 to 5 months, no formal schedule. | [V] |
| Migration pain | 0.18 to 0.19: about 100 breaking entries (resources became components, scene crate renamed). 0.19 to 0.20: about 75 (shaders must move to WESL, sprites moved to `Mesh2d` + `SpriteMaterial`, observer generics changed, BSN syntax changed). | [V] |
| First-party editor | **Does not exist as a release or preview.** `bevy_editor_prototypes` was archived on 2026-04-16 and points people to Jackdaw. The sixth-birthday post says the upstream editor MVP is top priority, with the author's own caveat that you "probably shouldn't trust" him that it is close. | [V] |
| The conflicting claim | A January 2026 blog says Bevy "has a first-party editor (bevy_editor, experimental)". It is wrong, or describes the prototype repo. Official release notes for 0.19 and 0.20 both say the editor is upcoming. | [C], resolved in favour of the primary source |
| BSN | Shipped in 0.19 as the `bsn!` macro. Syntax changed again in 0.20. **No `.bsn` file loader has shipped**; it is on the "what's next" list. A scene writer was a draft PR. | [V] |
| Feathers | Editor-focused widget set, growing each release (0.20 added colour input, list view, dropdown, lazy menu). Explicitly not meant for game UI. | [V] |
| `bevy_ui` | Headless core widgets, text input (0.19, no undo yet), `em`/`rem` units (0.20). Reactivity is "possible, not blocking the editor". No table or graph widgets mentioned. | [V] |
| Hot patching | Opt-in since 0.17 via Dioxus subsecond. Works for ordinary per-frame systems; startup systems, signature changes and multi-crate workspaces are weak spots. Not mentioned in 0.19 or 0.20 notes. | [V] for 0.17, [U] for current state |
| 2D | First-party `TilemapChunk` (one draw call per chunk) since 0.17. The 0.19 notes admit 2D rendering "started to lag behind 3D"; 0.20 unified the sprite renderer with 3D infrastructure and added sprite materials. | [V] |
| 2D at scale | No trustworthy published numbers for 100k+ sprites. A 2025 thread reports 2D regressions since 0.15. | [U] |
| Funding | Foundation "drastically underfunded"; paid developers at roughly 54% below baseline. | [V] |
| Community size | 23,359 Discord members, 47,529 stars, 1,527 contributors (Aug 2026). | [V] |

**What this means [J]:** Bevy is building exactly the scene, asset and editor foundations a general editor needs, and it is close but late. Anything you build in that space has a shelf life of one or two Bevy releases.

### 2.2 Bevy editors and tools

| Tool | What it does | What it does not do | Alive? | Licence | Label |
|---|---|---|---|---|---|
| **Jackdaw** | 3D scene editor: hierarchy, inspector, brushes, terrain, `.bsn` save/load, undo/redo, extension system, CLI. UI follows the official editor's Figma design. 609 stars, 997 commits. | README still says 3D level editor, but the source at 2026-11-11 has a full tile editor, tilemap kernel, 2D view and 2D starter scene (see `spikes/jackdaw-spike-findings.md`). Out-of-process play, Rust extensions, and a supported "custom editor distribution" seam. No game-code hot patching, no determinism or save tooling. About 514k lines; `main` tracks Bevy `main`. | Yes, and blessed by the Bevy org. | MIT/Apache-2.0 | [V] |
| **bevy_map_editor** | 2D tilemap editor (egui): layers, terrain painting, entity placement, animation editor, dialogue trees, run-game button. Same author as Jackdaw. 62 stars. | Autotiling unfinished; targets Bevy 0.18; no releases published. | Unclear. Author's focus appears to be Jackdaw. | MIT/Apache-2.0/CC0 | [V] features, [U] activity |
| **bevy-inspector-egui** | Runtime entity and resource inspector. | Not an editor. | Tracks Bevy with a lag; 0.20 support not confirmed. | MIT/Apache-2.0 | [U] for 0.20 |
| **bevy-yoleck** | Your game acts as its own level editor. 30 breaking releases. | Not a general tool. | Appears maintained. | MIT/Apache-2.0 | [U] |
| **bevy_ecs_tiled** | Loads Tiled maps. Supports Bevy 0.19. | Editing happens in Tiled. | Yes. | MIT | [V] |
| **bevy_ecs_tilemap** | Entity-per-tile tilemaps. Bevy's own issue tracker calls it hard to maintain and slow at large map sizes. | Does not scale to huge maps. | 0.18 release exists. | MIT | [V] |
| **space_editor** | Prefab editor. | Compatibility table stops at Bevy 0.13. | Effectively dead. | MIT/Apache-2.0 | [V] |
| **bevy_editor_prototypes** | Official experiments. | Archived. | Dead (2026-04-16). | MIT/Apache-2.0 | [V] |
| bevy_workbench, bevy_trenchbroom | Not found in searches. | | | | [U] |

**Pattern [J]:** third-party Bevy editors die when their author stops chasing Bevy releases (space_editor) or get absorbed by the official direction (Jackdaw). There is no empty 2D-editor slot worth fighting for; there is a half-built one (bevy_map_editor) you could contribute to.

### 2.3 Rust 2D engines and frameworks

| Project | Status | Where it stalls | Label |
|---|---|---|---|
| **Fyrox** | 1.0 shipped spring 2026 after 7 years, with a real editor. Feature freeze for a few months after. | Scene-graph design, not ECS; 2D is a side feature; essentially one core developer. | [V] release, [U] the rest (from memory) |
| **macroquad** | 0.4.16, about 1.96M downloads. Simple immediate-mode 2D. | No editor, no ECS, no tooling. | [V] version, [U] activity |
| **ggez** | 0.10.0 released 2026-06-03. | Framework only. | [V] (via aggregator) |
| **godot-bevy** | Bevy ECS running inside Godot 4, 0.11.0, targets `bevy_ecs` 0.18. | Sync overhead; depends on two moving projects. | [V] |
| **gdext (godot-rust)** | Rust bindings for Godot. | FFI overhead; one community benchmark found little gain when code mostly calls the Godot API. | [V] exists, [U] benchmark quality |

**godot-bevy matters most to you [J].** "Rust sim core, Godot for editor and UI" is the cheapest way to get everything Ianus promises, using the editor you already know. Ianus has to beat that, not Unity.

### 2.4 Incumbents: where they measurably struggle

| Engine | Evidence | Label |
|---|---|---|
| **Unity (classic)** | RimWorld community docs describe it as mostly single-core. Ludeon announced the in-house **Waylith** engine for update 1.7 (Oct 2026): 2 to 4 times faster overall, 2.2 times on a late-game colony, 2.3 times faster loading with 1,500 mods. | [V] numbers via 80.lv; Ludeon's stated reasons [U] (primary Steam post would not load) |
| **Unity DOTS** | Cities: Skylines 2's CEO said they "completely overestimated" Unity and banked on unproven tech. Independent analysis: the DOTS sim itself ran well; the failures were rendering (no LODs, no culling) because DOTS lacked a ready render path. | [V] |
| **Unity, worked around** | Captain of Industry replaced Unity's renderers with custom instanced rendering. shapez 2 added a multithreaded sim (up to 40 times faster). | [V] |
| **Godot** | Official docs: node overhead becomes a problem at "thousands" to "tens of thousands" of nodes; the fix is to bypass nodes and use the servers directly. A forum user at about 8,000 nodes got a boost just by removing nodes. | [V] |
| **Custom engines** | Factorio: deterministic lockstep because per-tick state change is too large to sync; deterministic multithreading described as especially hard. | [V] |
| **GameMaker** | Not researched. | [U] |

**The pattern [J]:** every successful large sim ends up with a hand-written data-oriented core and an instanced renderer, *inside or outside* an engine. Engines fail them on sim architecture, not on drawing 2D.

---

## 3. Niche validation

### 3.1 Answering the open challenges

**"Is optimised 2D a real pain point?"** Not as stated. **[J]** Godot's limit is around thousands of nodes and has a documented escape hatch. The pain appears at 10k+ simulated agents, large late-game saves, deterministic multiplayer, and heavy modding.

**"Name one real sim that hit an engine wall."** Several, see 2.4. The cleanest is **RimWorld to Waylith**, and it happened a month ago. The most instructive is **Cities: Skylines 2**: the ECS sim was fine, and the missing *tooling around the ECS* sank it. That is close to an argument for Ianus.

**"Audience or contributors?"** Mostly contributors and hobbyists. **[J]** Evidence:

| Signal | Number | Label |
|---|---|---|
| Bevy Jam entries, Jam 1 to Jam 7 | 73, 84, ?, 87, 77, 98, 64. No growth since 2022. (Jam 7 count may be a pre-judging snapshot.) | [V] |
| Bevy Discord | 23,359, up 6% in a year | [V] |
| Named Bevy sims in development | Settletopia (colony sim), Times of Progress (city builder), Exofactory (automation) | [V] for the first two, [U] that Exofactory is Bevy |
| Steam colony sims released | 151 in 2024, 221 in 2025 | [V] (single analytics site) |
| Steam city builders released | 342 in 2024, 436 in 2025 | [V] (same) |
| Steam automation games released | 477 between Jan 2024 and Jun 2025 | [V] (same) |
| Share reaching 1,000 reviews | Colony sim 13.7%, city builder 10.5%, automation 8.4%, versus 4.2% for all of Steam | [V] (same; tags flatter hit rates) |
| Sim developers using Rust | No data exists. My estimate: low tens of serious projects worldwide. | [U] |

**"Sims are UI- and data-heavy."** Confirmed as the weakest point. `bevy_ui` has no table, graph or dense-tooltip widgets on its roadmap. **[V]** egui has them and works with Bevy 0.19 (`bevy_egui` 0.41). **[V]**

### 3.2 What these developers put up with today

- Unity or Godot for editor and UI, then a hand-rolled sim core bolted on the side.
- Or a full custom engine (Factorio, Songs of Syx, now RimWorld).
- In Rust: Bevy plus a pile of crates that each break on a different schedule.

**Would they switch? [J]** Established teams will not. New Rust-curious teams starting a sim might, if Ianus saves them the three months everyone spends on save/load, determinism and a debug UI. That is your user.

### 3.3 Is the niche too thin?

**For a product: yes. For a tool you use yourself and share: no.** **[J]** The genre is commercially healthy (2 to 3 times Steam's hit rate) but the Rust slice is small and the best teams roll their own. Plan for 5 to 20 external users in the first 18 months and treat anything more as a bonus.

---

## 4. Differentiation and the wedge

| Candidate | Real? | Already covered? | Defensible? | Verdict |
|---|---|---|---|---|
| ECS-native | Yes | By Bevy itself | No | Not yours to claim |
| Huge-entity rendering | Partly | Bevy `TilemapChunk`, sprite renderer unification | No | Use Bevy's, benchmark it |
| Diff-friendly text scenes | Yes | BSN, once the loader lands | No | Do not build |
| General 2D editor | Yes | The shell: Jackdaw and the upcoming official editor. The 2D room workflow: only the half-built bevy_map_editor. | The shell no, the 2D workflow somewhat | **Build the 2D workflow on someone else's shell** |
| Rust as the scripting language, with a seconds-long loop | Yes | Fyrox (DLL hot reload, documented as fragile); Bevy hot patching (opt-in, limited) | Yes, if it actually works reliably | **Co-headline with the sim layer** |
| Licence and trust | Yes | Bevy, Godot | No | Table stakes |
| **Deterministic sim layer** | Yes. Bevy needs the `libm` feature and explicit system ordering for cross-platform determinism; nothing packages this. | Only for physics (Avian, Rapier) | Yes, through tests and discipline | **Core of the wedge** |
| **Large-save serialization and migration** | Yes. RimWorld's 1.6-to-1.7 two-way save compatibility shows what users expect. | No first-party story found | Yes | **Core of the wedge** |
| **Sim debugging tools** (time control, replay, entity timeline, perf budget per system) | Yes | bevy-inspector-egui covers a small part | Yes | **Core of the wedge** |
| **Data-table content editing** (items, recipes, jobs) | Yes | No | Medium | Second phase |
| Modding hooks | Yes | bevy_mod_scripting exists (Lua, Luau, Rhai; no WASM; latest confirmed pairing is Bevy 0.18) | Medium | Later, lean on the existing crate |

### The sharpest wedge the evidence supports

> **Ianus is GameMaker for people who want Rust: general 2D with a room editor and Rust scripts, and the only 2D tool with a deterministic, save-safe, inspectable simulation tick built in.**

The sim layer is what makes this more than "Jackdaw for 2D". The editor is what makes it more than a crate. **[J]**

### Competitive response: when Bevy's editor ships

- If Ianus owns its own editor shell: the shell is obsolete on that day and you carry it alone.
- If Ianus is 2D and sim tooling on a shared shell: the official editor is **good news**. You port panels and delete window code.

This is why section 5 insists on a borrowed shell.

---

## 5. How Ianus becomes king of the hill

The hill: **"the 2D tool Rust developers reach for, and the obvious choice for simulation games."** **[J]**

### 5.1 What "like GameMaker Studio 2" means, piece by piece

| GameMaker piece | Ianus equivalent | Build, borrow or cut |
|---|---|---|
| Room editor | Jackdaw's tile editor and 2D view | **Borrow** (updated after the spike). Verify by using it before relying on it. |
| Objects and events | Components plus Rust "scripts" (5.2) | **Build.** This is the core API design. |
| GML | Rust | **Build** the loop that makes it feel like scripting. |
| Sprite editor | Aseprite or LibreSprite, with import and hot reload | **Borrow.** Never build a pixel editor. |
| Animation and sequences | Sprite-sheet animation panel | Later. bevy_map_editor has one. |
| Run button | Jackdaw already plays the game out of process into a panel | **Borrow.** |
| One-click export | `cargo` plus a CLI wrapper for Windows and Linux | Build thin, desktop only. |
| Marketplace, visual scripting, mobile, consoles | | **Cut.** |

### 5.2 Rust as the scripting language: the make-or-break

GameMaker's appeal is that you change a line and see it. Rust fights that. What exists today:

| Mechanism | State | Problem |
|---|---|---|
| Bevy hot patching (subsecond) | Opt-in since 0.17 | Startup systems do not re-run; signature changes need opt-in; multi-crate workspaces are slow or fail. [V] for 0.17 |
| Fyrox-style DLL reload | Shipping in Fyrox 1.0 | Fyrox's own book says to fall back to static linking when it misbehaves; vtables break on unload; Rust has no stable ABI. [V], notes may predate 1.0 |
| Full rebuild and restart with state restore | Always works | Slow unless the game crate is tiny |

**How to make it work [J]:**

- **Define a narrow "script" shape:** a per-tick function over components, in one game crate, with no startup-only logic. That is exactly the shape hot patching handles today.
- **Use the sim layer as the safety net.** When a patch cannot apply, snapshot the world, rebuild, restore. Deterministic saves turn "restart" into a 5-second blip instead of lost state. This is where the two halves of Ianus reinforce each other, and it is something neither GameMaker nor Fyrox has.
- **Keep the engine prebuilt.** The user's crate should be the only thing compiling. Measure this in Phase 0; if a one-line change takes over about 5 seconds on a laptop, the pitch does not hold.
- **Put content in data.** Items, recipes and stats in hot-reloaded tables so most tuning never touches the compiler.

**Hole I cannot close [J]:** the LogLog Games retrospective (three years of Rust, left over iteration speed) is about exactly this. Nobody has yet shown a Rust workflow that iterates like GML. You would be the first, which is either the moat or the grave.

### 5.3 Why a borrowed shell

Jackdaw already has hierarchy, inspector, undo/redo, `.bsn` save/load, Rust extensions for panels and operators, and out-of-process play. **[V]** It follows the official editor's design, so work done there is likely to carry over. It has no 2D workflow at all, which is your opening.

Open risks **[U]**: whether Jackdaw's extension API is rich enough for a 2D viewport and tile tools, whether its maintainer wants 2D upstream or as a separate distribution, and how stable that API is. **Ask before designing.**

**Spike result, 2026-11-11 (source reading only, nothing built; full notes in `spikes/jackdaw-spike-findings.md`):**

- **Correction:** Jackdaw *does* have a 2D workflow. Its current source holds a tile editor of about 36,000 lines, a tilemap kernel, tilesets, a 2D view and a 2D project template. "No 2D workflow, which is your opening" above is wrong.
- **No fork needed.** `JackdawEditorPlugins` is a documented seam for custom editor distributions; runtime extensions add panels and operators and hot-reload.
- **Consequence for the plan:** Phase 2 (room editor) shrinks to integration work. The table in 5.1 changes: room editor and run button are both "borrow".
- **Consequence for the pitch:** the editor is no longer a differentiator at all. What only Ianus has is the sim layer, the Rust script loop with snapshot-restore (Jackdaw has no game-code hot patching), and sim panels. If those are thin, Ianus is a skin on Jackdaw.
- **New risks:** Jackdaw `main` tracks Bevy `main`; Windows extension loading is listed by Jackdaw itself as fragile.

**Decision (owner, 2026-11-11): Ianus is its own editor, built as a Jackdaw distribution, renovated piece by piece.**

How "renovation" works **[J]**:

1. **Start as a thin distribution.** A small `ianus` binary that adds `JackdawEditorPlugins` plus Ianus plugins. Own name, icon, default layout and project template from day one.
2. **Add before you replace.** New Ianus panels (sim time control, replay, tick budget, data tables) go in as plugins beside Jackdaw's.
3. **Replace one plugin at a time, only with a reason.** The plugin group lets you disable a Jackdaw plugin and supply your own. Each replacement needs a stated reason ("Jackdaw's X cannot do Y that the dogfood game needs"), because every replaced part is one you maintain alone from then on.
4. **Send generic fixes upstream.** Anything not specific to Ianus goes to Jackdaw as a PR, so your private diff stays small.
5. **Never fork the repo unless forced.** A fork means merging a 514k-line moving codebase by hand. Depend on released crates instead.

Rules to keep it from becoming option 2 by accident: at most one renovation in progress at a time; none before the sim layer and script loop exist; and a running list of replaced parts with their reasons, so the maintenance bill is visible.

Unverified: how cleanly individual Jackdaw plugins can be swapped in practice. The book says normal `PluginGroup` controls work; whether the plugins depend on each other internally needs a build to find out. **[U]**

The Phase 0 spike must answer three things:

1. Can an extension host a 2D viewport with tile painting, or does it need a fork?
2. Does the maintainer want 2D upstream, as an extension, or not at all?
3. How much does Jackdaw itself break between releases?

If two of three come back bad, the fallback is a minimal egui shell (hierarchy, inspector, save/load only) and the effort for Phase 2 roughly doubles.

### 5.4 Who it is for

| Group | Fit |
|---|---|
| GameMaker and Godot beginners | Poor. Rust is the barrier. |
| Rust developers making 2D games with Bevy and no editor | **Good.** This is the general-2D audience. |
| Developers starting a colony sim, city builder or factory game | **Best.** This is where Ianus is meant to be unbeatable. |
| Your own studio | First user. |

### 5.5 The five moves that still apply

Winning the sim half takes five things, in this order.

1. **Ship a game on it.** Fyrox took seven years to 1.0 and still gets asked "what shipped on it?". One finished small sim beats any feature list. I found three Bevy sims in development and could not confirm that any has fully released.
2. **Publish numbers nobody else has.** There is no trustworthy benchmark of Bevy vs Godot vs godot-bevy at 10k, 50k and 100k agents. Run it, publish the method, and keep it current each Bevy release. This alone makes Ianus the reference people link to.
3. **Own determinism with proof.** A CI job that replays a recorded sim on Windows, Linux and macOS and compares state hashes each tick. Factorio's developers call deterministic multithreading especially hard; a public green badge for it is a moat that takes others months to copy.
4. **Be first on every Bevy release.** Ecosystem crates lag by weeks to months (bevy_mod_scripting, bevy-inspector-egui). Supporting a new Bevy within two weeks, with a written migration note, is cheap for a small codebase and very visible.
5. **Stay small enough to be adopted piecemeal.** Separate crates (`ianus_tick`, `ianus_save`, `ianus_inspect`) that work alone. People adopt one, then the rest. A monolith asks for trust you have not earned yet.

**What would falsify this plan:** if the Phase 0 benchmark shows Godot with servers, or godot-bevy, within about 2 times of Bevy at 50k agents, the performance half of the pitch is gone and only determinism and saves remain.

---

## 6. Architecture options

### 6.1 Project shape

| Option | Pros | Cons | Effort | Risk |
|---|---|---|---|---|
| **A. Plugin set on Bevy** | Small surface; rides Bevy's renderer, BSN and future editor; piecemeal adoption | Exposed to Bevy churn; "not a real engine" optics | Low | Low |
| B. Separate engine product | Own identity, own stability promise | Must wrap or fork Bevy; you own the editor forever; directly competes with upstream | Very high | Very high |
| C. Contribute upstream only | Maximum leverage, no maintenance | Bevy's roadmap has no room for sim features right now; slow review; no project of your own | Medium | Low |
| **A + C** | Sim layer as crates; fix generic problems upstream as you hit them | Needs discipline about what goes where | Low to medium | Low |

**Under the GameMaker-style direction:** the runtime (tick, save, script API, 2D helpers) is still option A, a set of crates on Bevy. The editor is a Jackdaw distribution on top. The user-facing product is "Ianus"; underneath it is A + C plus a borrowed shell, not option B.

**Recommendation [J]: A + C.** Evidence: space_editor's death, the prototype repo's archival, and Jackdaw's absorption show what happens to option B-shaped projects.

### 6.2 Editor architecture

With Jackdaw as the shell, several rows below are decided by Jackdaw rather than by you (toolkit, process model, undo, scene format). They are kept because the fallback shell would need them, and because Ianus panels must fit Jackdaw's choices. Which UI toolkit Jackdaw extensions use is something the Phase 0 spike must confirm. **[U]**

| Question | Options | Assessment |
|---|---|---|
| UI toolkit | egui vs Feathers | **egui now.** It has tables, plots and docking today. Feathers is the long-term home but lacks dense-data widgets. Keep panel logic separate from drawing so a port is possible. [J] |
| Process model | In-process vs separate process over Bevy Remote Protocol | In-process is far simpler and enough for sim tools. BRP is the path the official editor is taking; revisit when it ships. [U] on BRP details |
| Authored data vs runtime state | One world vs two | For sims, authored data is mostly **tables** (definitions), not placed entities. Keep definitions as assets, runtime state as ECS. This sidesteps most of the classic scene-editor problem. [J] |
| Scene format | BSN vs custom | **Use BSN for anything scene-shaped.** Do not invent a format. Wait for the file loader. |
| Undo/redo | Command pattern vs snapshots | For table edits, snapshots of small documents are fine. For world state you already need snapshots for save/load, so reuse them. Bevy's own text input has no undo yet. [V] |
| Play-in-editor | Shared world vs sub-world vs restart | With deterministic snapshots, "play" is: snapshot, run, restore. The save system *is* the isolation mechanism. This is the one place the sim focus makes an editor problem cheaper. [J] |
| Hot reload | Assets vs code | Asset hot reload for data tables is standard Bevy. Code hot patching is opt-in and fragile across crates; do not promise it. [V] |

### 6.3 Scripting

| Option | Iteration speed | Fit | Note |
|---|---|---|---|
| **Rust only** | Slow compile, partially offset by hot patching | Matches the niche Rust crowd | **Decided: Rust is the scripting language.** See 5.2 for the loop. |
| Lua/Luau via bevy_mod_scripting | Fast | Good for modding | Crate lags Bevy by a release; no WASM. Adds a dependency that can die. |
| Data-only modding (tables, RON/BSN) | Fast | Covers most sim modding (items, recipes) | Cheapest and deterministic |

The LogLog Games post (2024) is the strongest counter-evidence to Rust-only: three years in, they left over iteration speed. A Bevy maintainer's reading was that they really wanted first-class scripting. **[V]** Take that seriously: **data-driven first, scripting later, never in the deterministic core without a sandbox.** **[J]**

### 6.4 Sim-specific needs

| Need | Approach to evaluate | Hard part |
|---|---|---|
| Deterministic tick | Fixed schedule, explicit system ordering, ambiguity detection as a hard error, `libm` math or fixed-point, seeded RNG resource | Parallel systems and float behaviour across platforms |
| Saves | Versioned schema per component, migration functions, chunked and compressed snapshots | Migration ergonomics; resources-as-components (0.19) changed the ground under this |
| Spatial index | Grid or hashed grid as a resource, updated in the tick | Keeping it deterministic under parallelism |
| Pathfinding | Flow fields and hierarchical A* behind one trait | Oxygen Not Included players attribute most late-game cost to pathfinding [U] |
| Dense UI | egui tables and plots for tools; game UI is the game's business | Do not try to fix `bevy_ui` |
| Modding | Data tables first | Mod load order and save compatibility |

### 6.5 2D specifics

Use Bevy's: `TilemapChunk`, sprites, `SpriteMaterial` (0.20), text (parley since 0.19). **[V]** Ianus adds nothing here in v1 except benchmarks and a "many agents" instanced path *if* the benchmark says Bevy's sprites are not enough. Lighting and pixel-perfect: out of scope.

### 6.6 Dependency strategy

- **Pin to one Bevy minor per Ianus minor.** Publish a compatibility table.
- **Depend on `bevy_ecs`, `bevy_app`, `bevy_reflect` in core crates**, not on `bevy`. Rendering and UI churn (WESL, sprite rewrite) then cannot break the sim core. **[J]**, supported by where the 0.20 breaking changes landed.
- **One adapter module** for everything touching scenes and UI.
- **Hard dependency budget:** `bevy_egui` and at most two others. Each extra crate is a release you wait on.
- **Upgrade window:** two weeks after each Bevy release; skip a release if school says so, and say so publicly.

### 6.7 Platforms

Desktop first (Windows, Linux, which covers Steam Deck). Web is a trap for v1: threads, memory limits and scripting crates all have gaps there. **[J]**, **[V]** for bevy_mod_scripting's lack of WASM.

---

## 7. Roadmap, cut line, kill criteria

Effort numbers are my estimates for one person at roughly 8 to 10 hours a week. **[J]** Halve the calendar time with 2 to 3 steady contributors; it does not shrink further because review becomes the bottleneck.

| Phase | Work | Exit criteria | Calendar |
|---|---|---|---|
| **0. Validate** | Three spikes: (a) agent benchmark, Bevy vs Godot vs godot-bevy at 10k/50k/100k; (b) **script-loop timing**: one-line change to running, with hot patching and with rebuild-and-restore; (c) a throwaway Jackdaw extension that draws a 2D tile view. Ask Jackdaw's maintainer about 2D. Talk to 5 Rust 2D or sim developers. Name check. | Benchmark published. Script loop under about 5 seconds. Jackdaw answer in hand. | 6 to 8 weeks |
| **1. Sim layer and dogfood game** | A small sim game, code only. `ianus_tick` and `ianus_save` extracted as they stabilise. The script shape from 5.2 defined by using it. | 20k agents at a stable tick; week-1 save still loads; bit-identical replay on two OSes; snapshot-rebuild-restore works. | 6 to 9 months |
| **2. Room editor** | 2D viewport, tile painting, entity placement, play button, on the borrowed shell. Sim panels: time control, replay, per-system budget. | You rebuild one level of the dogfood game in the editor without touching level code. One Bevy upgrade done inside two weeks. | 5 to 8 months (double without Jackdaw) |
| **3. Second game, not a sim** | A small non-sim 2D game (your call which) to test the "general 2D" claim. Template project and docs. | The second game needs no new engine features beyond the script API. | 3 to 4 months |
| **4. External alpha** | 3 to 5 outside users, CLI export. | Two outside projects on it for 2+ months; one outside PR merged. | 4 to 6 months |

**Total [J]:** roughly 2 to 2.5 years solo at 8 to 10 hours a week before outsiders use it. The sim-only version was about 1.5. The editor is where the extra year goes, and Phase 3 exists because "general 2D" is otherwise an untested claim.

**Dogfood suggestion (clearly my alternative, your call):** a tile-grid logistics sim with hauling agents and production chains, capped at about ten building types. It exercises pathfinding, the tick, saves and tables without combat or physics. The subgenre is question 3 in section 10.

### Cut line for v1

In: room editor (tiles and entities), Rust script loop, sim layer, sim panels, play button, desktop export.

Out: own editor shell, sprite or pixel editor, visual scripting, Lua or any second language, lighting, path tracing, web, mobile, consoles, multiplayer netcode (determinism yes, networking no), physics editor, marketplace, a custom scene format, 3D.

### Kill and pivot criteria

- **Stop:** Phase 0 benchmark shows no meaningful gap *and* developers do not name the wedge pains.
- **Drop "Rust as scripting" as a headline:** the Phase 0 script loop cannot get under about 5 seconds. Ianus is then a Rust 2D framework with an editor, which is a weaker but honest pitch.
- **Fall back to sim layer only:** Phase 2 runs past 12 months. Ship the crates, keep the editor as a branch.
- **Merge into Jackdaw or upstream:** the official editor ships with an extension API before Phase 2. Rebuild the tools as extensions.
- **Pivot to godot-bevy:** you find yourself rebuilding UI and editor features you already have in Godot.
- **Pause:** two consecutive Bevy releases missed, or the dogfood game untouched for 8 weeks. Announce it instead of going silent.
- **Shrink to a single crate:** no outside user after 18 months. Keep `ianus_save` or `ianus_tick` alive, drop the rest.

---

### Build log

**2026-11-11, engine foundation (owner lifted "no engine code" and asked for the engine before any game).** Two crates in `C:\Ianus\crates`, 18 tests passing, clippy and rustfmt clean. Rust 1.99.1, Bevy 0.20.1.

- `ianus_sim`: Bevy-free, float-free movement core. Integer fixed-point (4096 subunits per tile), 60 Hz tick, acceleration and friction, faster reversals, diagonals normalised with integer maths, saturating at world edges, FNV state hash. A 20,000-tick scripted replay is pinned to a golden hash. **That hash has only been produced on this Windows machine; the cross-OS claim is untested until CI runs it on Linux and macOS.**
- `ianus`: Bevy plugin on small sub-crates (no renderer in the dependency tree). Fixed tick, WASD and arrow input read right before the fixed loop (no one-frame lag), tap-latching so a press shorter than a tick is never lost, rebindable keys, render interpolation, and idle bodies skipped without flagging change detection.
- Tested through the real Bevy schedules with a manually driven clock. One test was weak (passed vacuously), found by deliberately disabling the latch, and tightened.
- **Not measured:** actual input latency, frame pacing on a real window, and performance at scale. "Optimised" so far means designed for it, not benchmarked.
- **Not built:** any game, any window, editor integration, saves, replay recording.

**2026-11-11, later: GUI, quiet windows, object model.**

- `ianus_engine` opens a window with an egui editor layout (toolbar, Scene list, Inspector, Simulation panel with frame graph) and click-to-select in the game view. Seen working once in a screenshot. **Open bug:** the game view rendered blank in several runs and correctly in others; my unconfirmed theory is the screenshot being taken before render pipelines finish compiling. Not resolved.
- A test window stole focus from the owner's ranked match. Windows now have a `quiet` mode (unfocused, bottom-most, no taskbar) that is on by default for automated runs. Unit-tested; **not yet confirmed on screen**. Rule saved: ask before opening any window.
- `ianus_objects`: the six object categories with built-in options, the room file (RON), and `Room::validate` with plain-English messages and typo suggestions. 20 tests. No collision, lighting or substance behaviour exists yet; walls are labels only.
- Agreed order: object model and room file (done), play/stop with snapshot-restore, iteration (data hot reload, rebuild-and-resume), editor placement, then real behaviours per category. Error messages are built in at each step.
- 72 tests across 4 crates pass.

**2026-11-11, rooms load, play/stop.** Step 2 of the agreed order.

- `ianus`: `load_room` (refuses rooms with errors, leaves the world untouched), `PlacedObject`, `PlayMode` (Editing/Playing), `play`/`stop` with snapshot-restore (positions, velocities, tick, RNG; things spawned during play are removed; things despawned during play are **not** brought back), `step_once`, `FileWatch`. Rooms need no restart hook: placed objects remember their start tile. 33 tests; Stop's restore was sabotaged on purpose and the tests caught it.
- `ianus_engine`: loads and watches a room file, reloads on save (waits for Stop if playing), draws objects by category plus a floor, Play/Stop toolbar button, EDITING tag, room header and Reload button, inspector shows the object's category and options (read-only). Shortcuts moved: F5 Play/Stop, F7 record, F8 reload. `examples/room.rs`, sample `rooms/cellar.room` (guarded by a test).
- Validator gained a check that a switch's Toggle target exists.
- **Not yet verified on screen:** none of the new GUI or room drawing has been looked at; the earlier intermittent blank game view is also unresolved. Windows are only opened with the owner's OK.
- **Still not true:** walls do not block, per-entity speed is ignored (global tuning applies), lights/substances/interactables do nothing yet.
- 98 tests across 4 crates pass.

**Verified on screen (one owner-approved quiet run, frame 200):** the room renders with the right colours per category, the floor and border, the Play button, EDITING tag, room header and the "loaded room" message, at 138 fps. The earlier blank game view did **not** reproduce in this run, which fits the pipeline-compile-time theory but does not prove it (one run).

**Focus finding:** the engine's quiet window did not take focus, but launching the exe with `Start-Process` opened a console window that did. `tools/quiet-run.ps1` launches with no console and reports whether the foreground window changed; it has **not yet been run**.

**After that run:** in edit mode the camera frames the whole room (it was following the player and cutting the room off); the Scene list groups by category, with Wall and Environment folded, in room-file order. 100 tests pass; neither change has been looked at on screen.

**2026-11-11, walls are solid.** Headless only; nothing here has been seen on screen.

- `ianus_sim::solid`: `SolidGrid` and `step_body_solid`, integer-only. Body is a 0.75-tile square; movement resolves X then Y, so bodies slide along walls; the room's edge is solid; a body starting inside a wall walks out freely instead of being thrown back. Property test: never overlaps a wall over 20,000 ticks of busy input; no tunnelling at top speed from all four sides. Golden hash for walled runs pinned on this Windows machine only (same caveat as the others). Refactor kept the original movement golden hash unchanged.
- `ianus`: `SolidMap` is built when a room loads (Walls, plus Environment objects that are not walkable) and the controlled entity uses it. With no room loaded, movement is open ground as before. Replays in a walled room still match. Sabotaging the wiring made three tests fail.
- Still ignored: per-entity speed, the Environment speed percentage (mud does nothing yet), lights, substances, interactables. Only the player moves; crates do not get pushed.
- 117 tests across 4 crates pass.

## 8. Community and sustainability

**How these projects live and die**

| Project | Outcome | Lesson |
|---|---|---|
| Bevy | 1,527 contributors, still underfunded | Even the winner cannot pay market salaries. Do not plan on funding. [V] |
| Fyrox | 1.0 after 7 years | Persistence works; it also takes seven years. [V] |
| Amethyst | Wound down October 2021, repo archived 2022 | Large scope and volunteer governance without a shipping game. Reasons not verified. [V] dates, [U] causes |
| space_editor | Stalled at Bevy 0.13 | Editors die on Bevy churn. [V] |
| Jackdaw | The archived official prototype repo and the birthday post both point to it | Aligning with the official design gets you adopted. [V] |

**Practical choices [J]**

- **Licence:** MIT/Apache-2.0 dual. Every tool in section 2.2 uses it; anything else blocks upstreaming.
- **Versioning:** 0.x tied to Bevy minors, with a public compatibility table.
- **Channels:** a thread or channel inside the Bevy Discord before your own server. The 23k people are already there.
- **Language:** English for code, docs and issues. German is a real asset for devlogs and a DACH meetup audience, not for the main channel.
- **Bus factor:** a second maintainer with publish rights before the external alpha; a written "how to release" page; CI that does the determinism check so a human does not have to.
- **Workload honesty:** docs, triage and moderation typically cost as much time as code once outsiders arrive. Phase 3's estimate includes this.
- **Funding:** GitHub Sponsors is fine as a signal. Bevy's own merch nets about $3 an item. [V]

---

## 9. Risk register

| Risk | Likelihood | Impact | Evidence | Mitigation | Early warning |
|---|---|---|---|---|---|
| Bevy churn | Certain | Medium | ~100 and ~75 breaking entries in the last two releases | Core crates on `bevy_ecs` only; adapter module; two-week upgrade rule | An upgrade takes more than two weekends |
| Official editor overlaps | High within 12 to 18 months | Low if sim-layer, fatal if general editor | Stated top priority; Jackdaw already saves `.bsn` | Build panels, not an editor; follow the Figma design | Editor Working Group forms |
| Thin niche | High | High | Jam entries flat; three known Bevy sims | Tool for your own game first; success measured in single digits | Phase 0 interviews come back lukewarm |
| Solo burnout | High | Fatal | Matura; Fyrox took 7 years | Fixed weekly cap; pause criteria; second maintainer | Eight weeks without a commit |
| Editor scope creep | High | High | It is in the project's original name | Written cut line; every UI feature must serve the dogfood game | You are writing a tile painter |
| UI immaturity | Certain for `bevy_ui` | Medium | No tables or graphs on roadmap | egui for tools | egui support lags a Bevy release |
| Dependency death | Medium | Medium | space_editor; scripting crate one release behind | Dependency budget of three | A dependency skips a Bevy release |
| Hardware and platform limits | Low for 2D desktop | Low | No published scale numbers | Phase 0 benchmark on a Steam Deck-class machine | Tick time grows faster than agent count |
| Learning Rust and engine design at once | Depends on you (question 1) | High | LogLog's three-year retrospective | Game first, extraction second | Refactoring the framework more than building the game |
| Compile times vs iteration | High | Medium | LogLog; hot patching still limited | Data-driven content with hot reload; small crates | Incremental builds over about 10 seconds |
| Name collision | Medium | Low to medium | IANUS Simulation GmbH | Register check now; cheap to rename before launch | Any letter from Dortmund |
| Users roll their own anyway | High | High | Factorio, RimWorld, Captain of Industry | Small adoptable crates rather than a framework | People copy ideas but not the dependency |

---

## 10. Open questions only you can answer

**Answered 2026-11-11:**

- **Q1, Rust: both.** "I want a game engine I can use but I love Rust." A usable engine is the goal; Rust is the preferred way to get there.
- **Q2, audience: own games first,** but usable as a product and built to be expanded.
- **Q6, time: "Claude codes while I Matura."** Claude Code writes most of the code; your hours go to direction, review and playing the results.

**What Q6 changes [J]:**

- The hour-based estimates in section 7 assumed you typing the code. They no longer apply as written. Code volume stops being the bottleneck; your review, decisions and hands-on testing become it. I have not re-estimated, because how much review time you have is still unknown.
- The burnout risk shrinks. A new one replaces it: **owning a codebase you did not write.** If you cannot explain why the tick or the save format works, you cannot debug it at 2 a.m. or defend it to contributors. Mitigation: small reviewable changes, a written design note per crate, tests that state the intended behaviour, and you playing every build.
- Things I cannot do: feel whether the editor is pleasant, judge whether the game is fun, talk to Jackdaw's maintainer or other developers, or build community trust. Those stay yours.
- The brief's "no engine code in this phase" still stands until you lift it.

- **Q3, first game: colony sim.** This sets what the sim layer builds first: agent needs and job assignment, pathfinding on a tile grid, a spatial index, and saves that survive long-running colonies. Determinism matters for replay and debugging; belts-and-machines throughput does not. The earlier logistics-sim suggestion in section 7 is superseded. Keep it small **[J]**: one map, a handful of needs and job types, no combat.

**Still open:** Q4 and Q5 (success at 6 and 18 months), Q7b, Q8 to Q10.

1. **Is Rust the point or the means?** If learning is the point, almost any version of this is worth doing and the kill criteria loosen. If the engine is the point, godot-bevy deserves an honest trial first.
2. **Tool for your games first, or product for others from day one?** The plan above assumes "mine first".
3. **Which subgenre is the proving ground:** colony sim, city builder, or factory? (Colony sims have the best hit rate; factory games stress determinism hardest.)
4. **What counts as success at 6 months?** A published benchmark and a playable prototype? Something else?
5. **And at 18 months?** A shipped small game? Three outside users? A talk at a Rust meetup?
6. **How many hours a week, honestly, until the Matura is done, and after?**
7. ~~Are you willing to drop the word "editor" from the pitch?~~ **Answered 2026-11-11: no. GameMaker-style editor, general 2D, Rust scripting, simulation as the strength.**
7a. ~~Are you willing to build on Jackdaw instead of your own shell?~~ **Answered 2026-11-11: yes, Jackdaw, subject to the Phase 0 spike.**
7b. **What do you mean by "polymath simulation"?** I read it as many interacting systems at once (agents, economy, logistics, environment). If you mean something narrower, the dogfood game changes.
8. **Would you rather own a small project or be a known contributor to Bevy's editor?** Both are open; they are different lives.
9. **Is deterministic multiplayer a goal for your own games?** It doubles the value of the wedge and also its difficulty.
10. **Keep the name if a trademark check comes back grey?**

---

## 11. What I could not verify

- Ludeon's own stated reasons for leaving Unity (the Steam post did not load; numbers come from 80.lv).
- Whether Exofactory is built on Bevy (one forum post plus a mention in Bevy's birthday post).
- Current state of hot patching in 0.20, and Bevy Remote Protocol details.
- Any reliable 2D scale benchmark for Bevy, Godot or godot-bevy. **This is the biggest hole and Phase 0 exists to fill it.**
- bevy_workbench, bevy_trenchbroom, GameMaker limits, Songs of Syx and Stardeus developer statements.
- Trademark registers for "Ianus".
- How many sim developers use Rust. Nobody publishes this.
- Why Amethyst wound down.

---

## 12. Sources

**Bevy (primary)**
- Bevy 0.20 release notes, 2026-10-08: https://bevy.org/news/bevy-0-20/
- Bevy 0.19 release notes, 2026-06-19: https://bevy.org/news/bevy-0-19/
- Bevy's Sixth Birthday, 2026-08-10: https://bevy.org/news/bevys-sixth-birthday/
- Migration guide 0.18 to 0.19: https://bevy.org/learn/migration-guides/0-18-to-0-19/
- Migration guide 0.19 to 0.20: https://bevy.org/learn/migration-guides/0-19-to-0-20/
- bevy_editor_prototypes (archived): https://github.com/bevyengine/bevy_editor_prototypes
- Tilemap chunk example: https://bevy.org/examples/2d-rendering/tilemap-chunk/
- Built-in tilemap issue: https://github.com/bevyengine/bevy/issues/13782
- Hot patching workshop notes: https://vleue.github.io/bevy_workshop-eurorust-2025/0-intro/hot-patching.html

**Bevy ecosystem**
- Jackdaw: https://github.com/jbuehler23/jackdaw
- bevy_map_editor: https://github.com/jbuehler23/bevy_map_editor
- bevy_ecs_tiled: https://github.com/adrien-bon/bevy_ecs_tiled
- space_editor: https://github.com/rewin123/space_editor
- bevy_egui 0.41.1: https://docs.rs/crate/bevy_egui/0.41.1
- bevy_mod_scripting: https://github.com/makspll/bevy_mod_scripting
- godot-bevy: https://docs.rs/crate/godot-bevy/latest
- Jackdaw book: https://jbuehler23.github.io/jackdaw/
- Fyrox book, code hot reloading: https://fyrox-book.github.io/beginning/hot_reloading.html
- Rapier determinism guide: https://rapier.rs/docs/user_guides/templates/determinism/
- Avian fixed-timestep PR: https://github.com/Jondolf/avian/pull/457
- Bevy Jam entries: https://itch.io/jam/bevy-jam-6/entries and https://itch.io/jam/bevy-jam-7/entries

**Other Rust engines**
- Fyrox 1.0: https://fyrox.rs/blog/post/fyrox-game-engine-1-0-0/
- Are We Game Yet, engines: https://arewegameyet.rs/ecosystem/engines/
- Amethyst wind-down, This Month in Rust GameDev #27: https://gamedev.rs/news/027/

**Engine walls**
- RimWorld on the Waylith engine (80.lv, 2026-10-13): https://80.lv/articles/rimworld-runs-2-4-times-better-on-new-in-house-engine/
- Ludeon's announcement (did not load): https://store.steampowered.com/news/app/294100/view/672882362547178633
- Cities: Skylines 2 CEO on Unity (PC Gamer): https://www.pcgamer.com/games/sim/cities-skylines-2-boss-says-they-completely-overestimated-the-unity-engines-capabilities
- Cities: Skylines 2 performance autopsy (Tom's Hardware): https://www.tomshardware.com/pc-components/gpus/cities-skylines-2-performance-autopsy-highlights-massive-optimization-failures
- Captain of Industry, Captain's diary #37: https://coigame.com/Blog/cd-37
- shapez 2 changelog: https://shapez2.wiki.gg/wiki/Changelog
- Factorio FFF #415: https://direct.factorio.com/blog/post/fff-415
- Factorio FFF #76: https://factorio.com/blog/post/fff-76
- Godot CPU optimisation docs: https://docs.godotengine.org/en/latest/tutorials/performance/cpu_optimization.html
- Godot forum, 2D limits: https://forum.godotengine.org/t/what-are-godots-performance-limitations-in-2d/27061

**Market**
- Colony sim niche report (Sept 2026): https://www.steampageanalyzer.com/niches/colony-sim
- City builder: https://www.steampageanalyzer.com/niches/city-builder
- Automation: https://www.steampageanalyzer.com/niches/automation
- Settletopia: https://ppakalns.itch.io/settletopia
- LogLog Games discussion (r/rust, 2024-04-26): https://redlib.hbubli.cc/r/rust/comments/1cdqdsi/lessons_learned_after_3_years_of_fulltime_rust/l1eyoqn

**Name**
- IANUS Simulation (Hannover Messe): https://www.hannovermesse.de/en/news/news-articles/simulations-with-minimal-effort-and-maximum-significance
- IANUS Simulation (chemeurope): https://www.chemeurope.com/en/companies/1045806
- GitHub account: https://github.com/ianus

**Weak sources I used with caution:** steampageanalyzer.com (single site, tag bias), lib.rs and arewegameyet.rs (aggregators), Reddit mirrors. **Discarded:** two dev.to articles citing a "Godot 5.0" and a January 2026 blog claiming a first-party Bevy editor exists.
