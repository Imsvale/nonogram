# nonogram-gui

Binary crate — graphical front-end for the nonogram workspace, built with **iced 0.13**.

Run from the workspace root:
```
cargo run -p nonogram-gui
```

The working directory at runtime must be the workspace root so that the default `puzzles/` path resolves correctly.

## Dependencies

| Crate | Role |
|---|---|
| `iced 0.13` (feature `tokio`) | GUI framework + async runtime |
| `rfd 0.15` | Native file-open dialogs (`AsyncFileDialog`) |
| `tokio 1` (feature `rt-multi-thread`) | `spawn_blocking` for sync solvers |
| `nonogram-core` | `Puzzle`, `CellState`, `SolveResult`, `Solver` trait, `parse_file` |
| `nonogram-propagation` | `PropagationSolver` |
| `nonogram-graph-search` | `GraphSearchSolver` |
| `nonogram-human` | `HumanSolver` (currently a stub) |

## Module Map

| File | Contents |
|---|---|
| `src/main.rs` | Entry point; `iced::application(...)` wiring only |
| `src/app.rs` | `App` struct, `Message` enum, `update`, `view`, all view helpers, grid renderer |
| `src/solver.rs` | `SolverKind` enum — the solver registry |
| `src/convert.rs` | `convert_letter_content` — letter-encoded → `Vec<Puzzle>` |

## Solver Discovery

Solvers are **not** discovered dynamically. `SolverKind` is a static enum:

```rust
pub enum SolverKind { Propagation, GraphSearch, Human }
impl SolverKind {
    pub const ALL: &'static [Self] = &[...];
    pub fn solve(self, puzzle: &Puzzle) -> SolveResult { match self { ... } }
}
```

To add a solver: add a variant to `SolverKind`, a `Cargo.toml` dependency, and a match arm. The pick-list in the toolbar reads `SolverKind::ALL`.

## App State

```rust
pub struct App {
    files: Vec<LoadedFile>,           // loaded puzzle files, in order
    selected: HashSet<Key>,           // (file_idx, puzzle_idx) pairs chosen for solving
    results: HashMap<Key, SolveResult>, // outcomes, keyed by (file_idx, puzzle_idx)
    solver: SolverKind,               // active solver
    focused: Option<Key>,             // puzzle shown in the detail view
    status: String,                   // status bar text
    busy: bool,                       // true while a Task is in flight
}

pub struct LoadedFile { path: String, name: String, puzzles: Vec<Puzzle> }
type Key = (usize, usize); // (file_idx, puzzle_idx)
```

`Puzzle` indices are stable for the lifetime of a `LoadedFile`. Removing files is not yet supported.

## Message Flow

```
ImportClicked / ConvertClicked
  → Task::perform(rfd::AsyncFileDialog) → FileChosen / ConvertChosen
  → Task::perform(parse_file / convert_letter_content) → FileLoaded / ConvertLoaded

SolveSelected / SolveAll
  → Task::perform(tokio::spawn_blocking(solver)) → SolveDone

App::new()
  → Task::perform(read puzzles/) → DefaultsLoaded
```

All blocking I/O (file reads, solving) runs through `Task::perform` so the UI thread is never blocked. Solvers are sync, so they're wrapped in `tokio::task::spawn_blocking`.

**`busy`**: set to `true` when a Task is launched, `false` on completion. Solve buttons are inert (`no on_press`) while `busy` is set.

## View Structure

```
column![
    view_toolbar()          ← buttons, solver pick-list
    horizontal_rule(1)
    row![
        view_left_panel()   ← file tree with checkboxes  (fixed 310 px)
        vertical_rule(1)
        view_right_panel()  ← puzzle detail OR results table (fills remaining)
    ].height(Fill)
    horizontal_rule(1)
    view_status_bar()       ← status text, busy indicator
]
```

`view_right_panel()` delegates to:
- `view_puzzle_detail(fi, pi)` — when a puzzle is focused (`App::focused = Some(key)`)
- `view_results_table()` — when results exist but nothing is focused
- Empty-state text — otherwise

## Grid Renderer (`view_grid`)

Free function at the bottom of `app.rs`. Widget-based (no Canvas).

Layout:
```
[corner spacer]  [col clue numbers, bottom-padded, C px wide per column]
[row clue numbers, right-padded, N px wide per number] [grid cells, C×C px each]
```

Constants: `C = 26.0` (cell px), `N = 22.0` (clue-number cell px).

Cell colours:
- `Filled`  → dark near-black `(0.10, 0.10, 0.15)`
- `Empty`   → white
- `Unknown` → slate `(0.72, 0.76, 0.82)`

1 px visual borders between cells: the grid column/row uses `spacing(1)`; a container with a mid-grey background wraps the whole grid, so the gaps show through as border lines.

The grid is wrapped in a `scrollable` — large puzzles (e.g. 40×20) scroll both axes.

## Iced 0.13 Conventions Used Here

- Button with no action: omit `.on_press()`; the widget renders as disabled.
- Transparent "text-link" button style: inline closure returning `button::Style { background: None / hover tint, text_color: Color::BLACK, border: Border::default(), shadow: Shadow::default() }`.
- Container background fill: `container(...).style(|_| container::Style { background: Some(color.into()), ..Default::default() })`.
- `container::Style` and `button::Style` both implement `Default`.
- `text(owned_string)` moves the `String` in; use this to avoid lifetime conflicts when the string is computed locally in a view function. Do **not** use `text(&local_string)`.
- `Space::with_width(Length::Fill)` as a flex spacer inside `row![]`.

## What Belongs Here

- All GUI state, layout, and interaction logic
- Solver dispatch (via `SolverKind::solve`)
- Puzzle file loading and letter-format conversion
- Future: step replay (`SolveResult::steps`), zoom controls, export

## What Does Not Belong Here

- Solving logic — goes in solver crates
- Puzzle parsing — `nonogram_core::parse_file`
- Shared types (`Puzzle`, `CellState`, etc.) — `nonogram-core`

## Current Limitations / Known TODOs

- **No file removal**: once a file is loaded it stays for the session.
- **No export**: there is an `ExportClicked` message stub in the original design but it is not wired. Add an export button if needed.
- **`HumanSolver` is a stub**: returns `Outcome::Stuck` for all inputs. The solver pick-list still shows it; solving with it will always show "Partial".
- **Grid cell borders**: the 1 px border lines come from `spacing(1)` + the grey container background. Thicker "box" borders every 5 cells (common in nonogram apps) are not implemented.
- **No zoom**: the cell size is fixed at 26 px. Very large puzzles (50+) may render impractically small.
- **Scrollable direction**: the puzzle grid scrollable defaults to vertical only. For wide puzzles, add `.direction(scrollable::Direction::Both { ... })` if needed.
- **No theming**: uses iced's default light theme.

## Git Commits

**Before committing, verify scope.**

- Change is entirely within this crate and the workspace compiles without touching anything else → commit from here.
- Change was triggered by a `nonogram-core` update, touches files in other crates, or modifies the workspace `Cargo.toml` → defer to **Claude-Main** (workspace root).

This rule applies even if the user explicitly asks this Claude to commit. Git has no technical barrier — any Claude can commit from any working directory in the repo — but a partial cross-boundary commit leaves the workspace broken. When in doubt, check scope first.
