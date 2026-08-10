# nonogram-convert

Standalone format conversion utility. Reads puzzle files in external encodings
and writes the native `name|col_clues/row_clues` format to stdout, ready to
append to any puzzle file the solver reads.

No dependency on `nonogram-core` — this crate handles raw text transformation
only and does not need any puzzle or cell types.

## Supported Input Formats

### `letter` (default)
Two-line blocks separated by blank lines. Each block is one puzzle:
- Line 1: row clues, space-separated tokens
- Line 2: col clues, space-separated tokens

Token encoding: uppercase ASCII letters, A=1 … Z=26.  
`BA` → [2, 1], `F` → [6], `A` → [1] (single-cell run; empty lines use `0` in output).

This matches the Rosetta Code nonogram encoding convention.

### `json`
A JSON object (or array of such objects, one per puzzle) with `"rows"` and
`"columns"` keys, each an array of clue arrays. Clue values may be JSON
numbers or numeric strings (e.g. `"10"` or `10`).

```json
{ "columns": [["10", "1", "4"], ...], "rows": [["18", "2", "2"], ...] }
```

This matches the `Game.task` shape used by puzzle-nonograms.com and read by
`nonogram-gui`'s URL importer.

### `pzprv3`
pzprjs's "Puz-Pre v3" grid layout — the same format `nonogram-gui`'s
`export_puzprv3` writes. A block is:

```
pzprv3
nonogram
<height>
<width>
<(max_col_depth + height) rows of (max_row_width + width) space-separated tokens>
```

`max_col_depth = (height + 1) / 2`, `max_row_width = (width + 1) / 2` (the
same bound puzz.link URL encoding uses). The top-left corner is `.`. The top
block holds column clues, bottom-aligned per column with `.` padding above;
the left block holds row clues, right-aligned per row with `.` padding to the
left; the bottom-right block is the solved/unsolved grid (`.` / `#`), which
this converter ignores — it only recovers clues. Clue tokens for fulfilled
lines are prefixed with `c` (stripped on read).

One puzzle per file is the usual case, matching what `nonogram-gui` exports.
Multiple puzzles in one file are supported: blocks are separated by a blank
line, and within a block `/` may substitute for newlines (matching pzprjs's
own URL-embedding convention) so a whole puzzle can occupy a single line.

## Usage

```
nonogram-convert <input> [--format letter] [--name "Prefix"]
nonogram-convert rosettacode_raw.txt --name "RC" >> puzzles/mine.txt
```

Output goes to stdout. Malformed blocks print a warning to stderr and are skipped.
A clue-sum mismatch (row total ≠ col total) also warns but still emits the line —
the solver will catch it if it's a real contradiction.

## Adding a New Format

1. Add a variant to `InputFormat`.
2. Write a `convert_<format>(content: &str, name_prefix: &str)` function.
3. Add the match arm in `main`.

## Ownership

This crate is owned by **Claude-Convert**. For cross-crate changes or design questions involving `nonogram-core`, coordinate with **Claude-Main**.

## Git Commits

**Before committing, verify scope.**

- Change is entirely within this crate and the workspace compiles without touching anything else → commit from here.
- Change was triggered by a `nonogram-core` update, touches files in other crates, or modifies the workspace `Cargo.toml` → defer to **Claude-Main** (workspace root).

This rule applies even if the user explicitly asks this Claude to commit. Git has no technical barrier — any Claude can commit from any working directory in the repo — but a partial cross-boundary commit leaves the workspace broken. When in doubt, check scope first.
