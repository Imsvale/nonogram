# Nonogram Workspace

## Crate Map

| Crate | Kind | Role |
|---|---|---|
| `nonogram-core` | lib | Shared types, parser, `Solver` trait — no solving logic |
| `nonogram-propagation` | lib | Technique-driven constraint propagation solver |
| `nonogram-graph-search` | lib | Best-first search with MRV heuristic |
| `nonogram-human` | lib | Author-written human-logic solver (AI-free implementation) |
| `nonogram-cli` | bin | CLI entry point; dispatches to any solver |
| `nonogram-convert` | bin | Format converter; letter-encoded → native `name\|col/row` |

## Key Conventions

- **`CellState`** is the canonical cell-state type, defined in `nonogram-core`. Use it at all crate boundaries. Do not introduce `Cell`, `CellStatus`, or similar aliases that cross boundaries.
- **Clue type**: `Vec<u32>` in `nonogram-core` and at all public APIs. Solver internals may cast to `usize` for arithmetic but must convert back at the boundary.
- **`Puzzle`** is immutable (clues + metadata only). Solvers own their own mutable grid state.
- **`SolveStep`** is defined in core for future tracing support. Solvers that do not yet trace steps return `steps: vec![]`.

## Cross-cutting Changes

When modifying the `Solver` trait or any type in `nonogram-core`, update all implementing crates before committing. The compiler will identify every affected site — follow the errors.

## Git Commits

Scope determines who commits. **Claude-Main** (this session, working from the workspace root) owns the commit when:

- Any file in `nonogram-core` is modified — changes there ripple into all solver crates and must land atomically in a single commit.
- The workspace `Cargo.toml` or this root `CLAUDE.md` changes.
- A change spans more than one crate for any other reason.

Crate-local Claudes commit when their change is entirely self-contained within their own crate and the workspace compiles without touching anything outside it.

If a crate-local Claude is asked by the user to commit something cross-cutting, the right move is to flag it and hand off to Claude-Main. If Claude-Main is asked to commit something genuinely isolated to one crate, defer to that crate's Claude instead.

## What Lives Where

- Solving logic → solver crates, never in `nonogram-core`
- Puzzle file parsing → `nonogram-core::parse_file`
- CLI rendering and dispatch → `nonogram-cli`
- `LineMeta` and propagation-specific bookkeeping → `nonogram-propagation` only
