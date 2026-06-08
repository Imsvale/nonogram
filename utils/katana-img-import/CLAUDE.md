# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Run

```sh
# from workspace root
cargo build -p katana-img-import
cargo run -p katana-img-import -- <subcommand> [args]

# or from this directory
cargo run -- <subcommand> [args]
```

## Three Subcommands

**`download <page_title> <output_dir>`**
Fetches the Nonograms Katana wiki page, parses puzzle metadata from wikitext, downloads all puzzle images into `<output_dir>/<category>/`, and writes `<output_dir>/manifest.tsv`. Filenames and directory names are sanitized (alphanumeric + `-`; everything else → `_`). The manifest preserves original unsanitized puzzle names in column 5.

**`parse <image_path> <width> <height> [name] [debug_out.png]`**
Single-image parse. Detects grid bounds, samples the grid, prints bounds and ASCII grid to stderr, and puzzle string to stdout. Pass a debug output path to save an annotated overlay image (green grid boundary, orange cell dividers, red/cyan cell centers).

**`parse-all <manifest_tsv> <images_dir> <output_file>`**
Batch-processes all images listed in the manifest and writes one puzzle per line to the output file. Skips and logs failures without panicking.

## Architecture

```
main.rs       — arg dispatch, cmd_download / cmd_parse / cmd_parse_all, sanitize()
img.rs        — detect_grid_bounds(), sample_grid(), draw_overlay()
puzzle.rs     — format_puzzle(), runs() (run-length encode), fmt_clue_list()
wiki.rs       — fetch_puzzles(), parse_wikitext(), fetch_image_urls(), url_encode()
```

Images are served as WebP despite `.png` extension; open with `image::ImageReader::with_guessed_format()`.

## Manifest Format (5-column TSV)

```
sanitized_category  sanitized_filename  width  height  original_name
```

`parse-all` reads col 1 as category (for path construction), col 2 as filename, cols 3–4 as dimensions, col 5 as the display name passed to `format_puzzle`.

## Grid Detection (`img.rs`)

`detect_grid_bounds` returns `(x_start, y_start, x_end, y_end)` — the pixel rectangle of the solution grid.

**Tan detection** (`is_tan`): `r > 100 && (r - b) > 20`. This handles:
- Muted tan (R ≈ 130, Golden Mean series)
- Light tan (r − b ≈ 29, some 20×20 images)
- Excludes white cells (r − b ≈ 0–4) and dark content (R < 100)

**Multi-sample scanning** guards against landing on grid dividers:
- `y_start`: 7 x positions across the right half; take maximum `last_tan_row`. Puzzle images have minor gridlines between every cell and a major gridline every 5 cells — a single scan position can fall on a vertical gridline (yielding `y_start = 0`). The major gridline position depends on clue header depth, so no fixed offset is safe.
- `x_start`: 3 band centers × 7 y positions (spaced 3 px apart); take maximum rightmost-tan x.
- `y_end`: 5 x positions in the row-clue area; take maximum.

`sample_grid` uses float cell positions `(x0 + (c + 0.5) * cw).round()` to avoid accumulated rounding error at the last row/column.

## Puzzle Output Format

```
Name|col1,col2,.../row1,row2,...
```

Each clue is space-separated run lengths. Empty rows/columns emit `0`.
