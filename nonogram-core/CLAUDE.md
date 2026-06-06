# nonogram-core

Shared types and parser for the nonogram workspace. **No solving logic belongs here.**

## Public Surface

### `CellState`
Three-valued enum: `Unknown`, `Filled`, `Empty`. The only cell-state type that crosses crate boundaries. Terminology mapping: `Filled` = Shaded, `Empty` = Blank.

### `Puzzle`
Immutable puzzle definition: `name`, `width`, `height`, `row_clues: Vec<Vec<u32>>`, `col_clues: Vec<Vec<u32>>`, `solution: Option<Vec<CellState>>`. No `cells` field — solvers own their own mutable grid.

### `LineId`
`Row(usize)` / `Col(usize)`. Provided for solvers and the CLI to reference lines without coupling to a specific grid representation.

### `Solver` trait
```rust
pub trait Solver {
    fn solve(&self, puzzle: &Puzzle) -> SolveResult;
}
```
All solver crates implement this. The trait is deliberately thin: input is `&Puzzle` (immutable), output is `SolveResult`. No shared mutable state.

### `SolveResult` / `Outcome`
`SolveResult { outcome: Outcome, grid: Vec<CellState>, steps: Vec<SolveStep> }`.  
`Outcome`: `Solved`, `Stuck`, `NoSolution`.  
`grid` is flat row-major; empty vec when `outcome == NoSolution`.  
`steps` is empty unless the solver explicitly traces its reasoning.

### `SolveStep`
`{ description: String, line: Option<LineId>, cells_changed: Vec<(usize, usize, CellState)> }`. Forward-compatible tracing struct — solvers return `steps: vec![]` until they implement tracing.

### `parse_file(path: &str) -> Result<Vec<Puzzle>, anyhow::Error>`
File format: `name|col_clues/row_clues[|solution]`. Clue groups are comma-separated; numbers within a group are space-separated. `0` normalises to an empty clue list. The optional solution field is a flat binary string (`1`=Filled, `0`=Empty).

## Ownership

This crate is owned by **Claude-Main**, who also holds workspace-wide oversight responsibility. See the root `CLAUDE.md` for the full ownership map and oversight mandate.

## Git Commits

**Changes to this crate are almost always cross-cutting** — every solver crate and the GUI depend on it. Do not commit from here without verifying that all downstream crates still compile.

Commits touching `nonogram-core` belong to **Claude-Main** (workspace root), not to this crate's session. Even if the user asks this Claude to commit, check whether the change ripples outward. If it does, defer to Claude-Main so the fix lands atomically across the whole workspace.

## Rules

- Do not add solver-specific types or logic. If a type is only needed by one solver, it lives in that solver's crate.
- Do not add a `cells` field to `Puzzle`.
- Clue values are `u32` — sufficient for all realistic puzzle sizes and consistent across the workspace.
