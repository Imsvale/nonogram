# nonogram

A nonogram (picross / griddler) toolkit: a desktop GUI for solving puzzles by
hand with assistance, several solvers of varying strength for hands-off
solving, and a small set of format-conversion utilities.

## Download

Prebuilt Windows binaries are on the [Releases page](https://github.com/Imsvale/nonogram/releases) —
grab the zip, it contains `nonogram.exe` (GUI) and `nonogram-cli.exe`.
Other platforms aren't built yet; open an issue if you want one.

## `nonogram` — the GUI

- **Manual solving** with click/drag fill and mark, trial mode, undo/redo,
  and assistance toggles (auto-dim solved clues, auto-cross from edges, and
  more) that help without solving the puzzle for you.
- **Four automated solvers**, selectable per puzzle, so you can compare
  approaches or just get an answer:
  - **Cuttlefish** — best-first graph search with an MRV heuristic; handles
    anything, branching only when it has to.
  - **DeepRed** — full per-line hard-logic deduction (no branching, no named
    techniques); proves everything a per-line argument can prove.
  - **Sensei** — a curated set of named, human-legible techniques (overlap,
    edge-forcing, and others); if it gets stuck, that's a legible statement
    about which techniques ran out, not just "no answer."
  - **Noobie** — a stub for an eventual from-scratch human-logic implementation.
- **Live progress** for Cuttlefish on large/slow puzzles — search depth and
  fill percentage update as it works instead of blocking until done.
- Drag-to-pan grid with a minimap, axis-locked dragging, crosshair row/column
  highlighting, and dual run-length labels.
- Per-puzzle timer (manual start/pause, auto-stop on completion) and
  persistent solved-puzzle tracking across sessions.
- Import from a local file, a puzz.link URL, a puzzle-nonograms.com URL, or
  Puz-Pre v3; export to Puz-Pre v3 or a puzz.link URL.

## `nonogram-cli` — headless solving

```
nonogram-cli <file> [--solver propagation|human-by-ai|graph-search|human] [--progress] [--verbose]
```

Parses every puzzle in a file, solves each with the chosen solver, prints the
grid, and verifies it against the puzzle's embedded solution if one is
present. `--solver` mirrors the GUI's four solvers (`propagation` = DeepRed,
`human-by-ai` = Sensei, `graph-search` = Cuttlefish, `human` = Noobie).
`--progress`/`--verbose` (graph-search only) show a live status line or log
every solve step. `--all` enumerates every solution instead of just one.
Run `nonogram-cli --help` for the full flag list.

## Puzzle file format

One puzzle per line, `;`-separated fields:

```
name;C:col_clues/R:row_clues[;answer[;solution]]
```

Within the clue section, `C:`/`R:` may appear in either order; clue groups
are `|`-separated, values within a group are space-separated. `answer`
(optional) is a title revealed after solving. `solution` (optional, requires
`answer` to be present) is a flat `1`/`0` string used to verify a solve.
Example — a 3×3 puzzle:

```
Example;C:1|1|1/R:1|1|1
```

Lines starting with `#`, and blank lines, are ignored.

## Building from source

Requires a Rust toolchain supporting the 2024 edition (stable 1.85+).

```
git clone https://github.com/Imsvale/nonogram.git
cd nonogram
cargo build --release -p nonogram-gui -p nonogram-cli
```

Binaries land in `target/release/` as `nonogram.exe`/`nonogram` and
`nonogram-cli.exe`/`nonogram-cli`. Run the GUI with
`cargo run -p nonogram-gui` from the workspace root, so it finds a `puzzles/`
directory there if you have one — not required, just where it looks by default.

## Workspace layout

| Crate | Role |
|---|---|
| `nonogram-core` | Shared types, puzzle-file parser, `Solver` trait |
| `nonogram-propagation` | Full per-line hard-logic deduction (DeepRed) |
| `nonogram-human-by-ai` | Named-technique constraint propagation (Sensei) |
| `nonogram-graph-search` | Best-first search with MRV heuristic (Cuttlefish) |
| `nonogram-human` | Human-logic solver stub (Noobie) |
| `nonogram-cli` | Command-line entry point |
| `nonogram-gui` | The desktop GUI, built with [iced](https://github.com/iced-rs/iced) |
| `nonogram-convert` | Dev utility: converts a few external puzzle encodings to the native format |
| `utils/katana-img-import` | Dev utility: image-to-puzzle importer |

## License

MIT — see [LICENSE](LICENSE).
