# nonogram-cli

Binary entry point for the nonogram workspace. Parses puzzle files, dispatches to the chosen solver, and renders results.

## Usage

```
nonogram-cli <file> [--solver propagation|human-by-ai|graph-search|human] [--progress] [--verbose]
```

Default solver: `propagation` (category 3 — full per-line hard-logic deduction, see below).

## Search Progress Observability

`--progress` and `--verbose` expose `GraphSearchSolver::solve_with_progress` (see
`discussions/SearchProgressObservability.md`). Both are graph-search-only —
gated the same way `--all` is, with an error telling the user to switch
solvers — and not yet supported together with `--all` (`solve_all` has no
progress hooks).

- `--progress`: attaches an `on_progress` callback that overwrites a single
  status line on stderr (`pushed=… expanded=… heap=… min_count=… known=…/…
  elapsed=…s`), throttled via `ProgressConfig::snapshot_interval`
  (`Duration::from_millis(200)`) plus `emit_start_snapshot` for an immediate
  first line. Suppressed by `--quiet`, same as the rest of stdout output.
- `--verbose`: sets `ProgressConfig::log_steps` and `log_meta_interval`, and
  installs `env_logger` with a default filter (`nonogram_graph_search=debug`)
  so `log`-crate output shows up without the user having to set `RUST_LOG`.
  This goes to stderr independently of `--quiet` — quiet only controls the
  normal result printing (`print_result`), not the logger.
- Both flags reuse `SolveResult` directly rather than going through `dyn
  Solver`, since `solve_with_progress` is a `GraphSearchSolver` inherent
  method, not part of the `Solver` trait (deliberately — see the discussion
  doc's "Why no core change" section). `run_puzzle`/`run_puzzle_progress` both
  funnel into a shared `print_result` so output formatting stays in one place.

## Dispatch

`SolverChoice` is a `clap::ValueEnum`. Each variant maps to a concrete solver struct from a solver crate. New solvers: add a variant and a `Box::new(...)` arm in `main`.

```rust
let solver: Box<dyn Solver> = match cli.solver {
    SolverChoice::Propagation => Box::new(PropagationSolver),   // nonogram_propagation — category 3
    SolverChoice::HumanByAi   => Box::new(HumanByAiSolver),     // nonogram_human_by_ai — category 2
    SolverChoice::GraphSearch => Box::new(GraphSearchSolver),
    SolverChoice::Human       => Box::new(HumanSolver),
};
```

**`--solver propagation` changed meaning**, per the resolution in
`discussions/SolverTaxonomy.md`: it used to select the named-technique solver
(category 2); it now selects the newly extracted `nonogram-propagation`
(category 3, full per-line hard-logic deduction — strictly stronger,
never weaker, since category 3 proves a superset of what category 2's named
passes prove). The user's own framing: the flag value stays the same, the
solver behind it upgrades — "entirely transparent." Category 2 (today's
named-technique solver, now living in `nonogram-human-by-ai`) is still
reachable, via the new `--solver human-by-ai` value. Both crates briefly
exported a struct literally named `PropagationSolver` right after the split
(an artifact of the move, not a deliberate choice) — `nonogram-human-by-ai`'s
was since renamed to `HumanByAiSolver`, matching the
`GraphSearchSolver`/`HumanSolver`/`PropagationSolver` naming convention, so
no import aliasing is needed here anymore.

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
