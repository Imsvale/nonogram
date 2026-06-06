# nonogram-cli

Binary entry point for the nonogram workspace. Parses puzzle files, dispatches to the chosen solver, and renders results.

## Usage

```
nonogram-cli <file> [--solver propagation|graph-search|human]
```

Default solver: `propagation`.

## Dispatch

`SolverChoice` is a `clap::ValueEnum`. Each variant maps to a concrete solver struct from a solver crate. New solvers: add a variant and a `Box::new(...)` arm in `main`.

```rust
let solver: Box<dyn Solver> = match cli.solver {
    SolverChoice::Propagation => Box::new(PropagationSolver),
    SolverChoice::GraphSearch => Box::new(GraphSearchSolver),
    SolverChoice::Human       => Box::new(HumanSolver),
};
```

## Rendering

`print_grid` uses `#` for `Filled` and `.` for `Empty`. For partial grids (`Stuck`), confirmed `Empty` cells are shown as `x` and `Unknown` as `.`.

`verify` compares the solved grid against the puzzle's embedded solution field and reports mismatched cells by `(row, col)`.

## What Belongs Here

- CLI argument parsing and dispatch
- Grid rendering and solution verification
- Any output formatting (future: colour, Unicode block characters, step replay)

## What Does Not Belong Here

- Solving logic — goes in solver crates
- Puzzle parsing — goes in `nonogram-core::parse_file`
- Shared types — go in `nonogram-core`

## Ownership

This crate is owned by **Claude-CLI**. For cross-crate changes or design questions involving `nonogram-core`, coordinate with **Claude-Main**.

## Git Commits

**Before committing, verify scope.**

- Change is entirely within this crate and the workspace compiles without touching anything else → commit from here.
- Change was triggered by a `nonogram-core` update, touches files in other crates, or modifies the workspace `Cargo.toml` → defer to **Claude-Main** (workspace root).

This rule applies even if the user explicitly asks this Claude to commit. Git has no technical barrier — any Claude can commit from any working directory in the repo — but a partial cross-boundary commit leaves the workspace broken. When in doubt, check scope first.

## Future: Step Replay

`SolveResult::steps` is populated by solvers that support tracing. When non-empty, the CLI can walk through the steps interactively or dump them. Add a `--trace` flag and a rendering loop over `result.steps` when ready.
