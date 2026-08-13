# Nonogram Workspace

## Crate Map

| Crate | Kind | Role | Owner |
|---|---|---|---|
| `nonogram-core` | lib | Shared types, parser, `Solver` trait — no solving logic | **Claude-Main** |
| `nonogram-propagation` | lib | Full per-line hard-logic deduction, no named techniques (category 3; extracted from `nonogram-graph-search`) | **Claude-Propagation** |
| `nonogram-human-by-ai` | lib | Technique-driven, human-legible constraint propagation solver (category 2; content moved from `nonogram-propagation`) | **unassigned** |
| `nonogram-graph-search` | lib | Best-first search with MRV heuristic | **Claude-graph** |
| `nonogram-difficulty` | lib | Puzzle difficulty rating (1.0–10.0) derived from a `nonogram-propagation` solve trace | **Claude-difficulty** |
| `nonogram-human` | lib | Author-written human-logic solver (AI-free implementation) | **User (human author)** |
| `nonogram-cli` | bin | CLI entry point; dispatches to any solver | **Claude-CLI** |
| `nonogram-convert` | bin | Format converter; letter-encoded → native `name\|col/row` | **Claude-Convert** |
| `nonogram-gui` | bin | iced 0.13 graphical front-end | **Claude-GUI** |
| `utils/katana-img-import` | bin | Image→puzzle importer; downloads from wiki, detects grid, emits puzzle format | **User** |

See `discussions/SolverTaxonomy.md` for the four-category taxonomy behind the
`nonogram-propagation` / `nonogram-human-by-ai` split, and its identity-model
correction: agent identity binds to the *path*, not the content that happens
to occupy it at a given time — Claude-Propagation stayed the owner of
`nonogram-propagation` across the swap from category-2 to category-3 content;
Claude-graph performed the extraction labor but does not own the result.

`nonogram-difficulty` is a "fifth thing, not a solver" per
`discussions/DifficultyRating.md`: it depends on `nonogram-core` and
`nonogram-propagation` only (never `nonogram-graph-search` or
`nonogram-human-by-ai` — a branching-aware or technique-capped trace would
corrupt what it's trying to measure), and contains no solving logic of its
own, only interpretation of a propagation trace. Claude-graph scaffolded it
initially (cheaper than a fresh Claude re-deriving the design discussion from
scratch) before handing off; Claude-difficulty is its permanent owner. The
formula/weights are an active, ongoing calibration effort, not a closed
design — that discussion doc is the authority on current status, not this
file or the crate's own `CLAUDE.md`.

## Key Conventions

- **`CellState`** is the canonical cell-state type, defined in `nonogram-core`. Use it at all crate boundaries. Do not introduce `Cell`, `CellStatus`, or similar aliases that cross boundaries.
- **Clue type**: `Vec<u32>` in `nonogram-core` and at all public APIs. Solver internals may cast to `usize` for arithmetic but must convert back at the boundary.
- **`Puzzle`** is immutable (clues + metadata only). Solvers own their own mutable grid state.
- **`SolveStep`** is defined in core for future tracing support. Solvers that do not yet trace steps return `steps: vec![]`.

## Cross-cutting Changes

When modifying the `Solver` trait or any type in `nonogram-core`, update all implementing crates before committing. The compiler will identify every affected site — follow the errors.

## Oversight

**Claude-Main owns `nonogram-core` and has workspace-wide oversight responsibility.**

Every other crate has a dedicated Claude (see Crate Map). Claude-Main's role is to:
- Own all changes to `nonogram-core` and ensure they land atomically with their downstream ripple effects.
- Coordinate cross-crate design decisions via discussion files in `discussions/`.
- Be the point of escalation when a crate-local Claude needs a core change or faces a cross-boundary question.

Claude-Main does **not** make changes inside crate-local Claudes' crates, even when those changes are mechanical ripple effects of a core change. The correct flow is: Claude-Main makes the core change, then signals the affected crate Claudes with the exact sites to update, and they commit those updates themselves.

## Git Commits

Scope determines who commits. **Claude-Main** owns the commit when:

- Any file in `nonogram-core` is modified — changes there ripple into all solver crates and must land atomically in a single commit.
- The workspace `Cargo.toml` or this root `CLAUDE.md` changes.
- A change spans more than one crate for any other reason.

Each crate-local Claude commits when their change is entirely self-contained within their own crate and the workspace compiles without touching anything outside it.

If a crate-local Claude is asked by the user to commit something cross-cutting, the right move is to flag it and hand off to Claude-Main. If Claude-Main is asked to commit something genuinely isolated to one crate, defer to that crate's Claude instead.

## What Lives Where

- Solving logic → solver crates, never in `nonogram-core`
- Puzzle file parsing → `nonogram-core::parse_file`
- CLI rendering and dispatch → `nonogram-cli`
- `LineMeta` and technique-pass bookkeeping → `nonogram-human-by-ai` only
