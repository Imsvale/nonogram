//! Claude-graph's prototype for an alternative combination/rescale, tested
//! against the real standard-set corpus using only nonogram-difficulty's
//! already-public DifficultyReport fields. Not wired into the crate's real
//! API — this is discussion-doc-only exploration, matching the precedent
//! set by scale_test.rs/narrowness_probe.rs. See discussions/DifficultyRating.md.

use nonogram_difficulty::{rate, DifficultyReport};

/// Every real puzzle corpus in the tree (Test-3's "full set") — the Test-2
/// anchors + all of Katana/, plus the other real puzzle files at the
/// `puzzles/` root. Excludes `puzzles/test_*.txt` and `file_test.txt`
/// (parser/technique fixtures, not puzzles meant to carry a difficulty
/// opinion) and `coolbutuseless.zip` (not a puzzle file).
fn files() -> Vec<String> {
    let mut files = vec![
        "puzzles/cult.txt".to_string(),
        "puzzles/Kaldo.txt".to_string(),
        "puzzles/coolbutuseless.txt".to_string(),
        "puzzles/rosettacode.txt".to_string(),
        "puzzles/pib.txt".to_string(),
        "puzzles/selection.txt".to_string(),
    ];
    let mut katana: Vec<String> = std::fs::read_dir("puzzles/Katana")
        .expect("puzzles/Katana not found")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("txt"))
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();
    katana.sort();
    files.extend(katana);
    files
}

// Current (unmodified) formula, for the "before" column.
fn current_raw(r: &DifficultyReport) -> f32 {
    r.size_tax + r.revisit_density * 2.0 + r.yield_friction * 4.0 + r.narrowness.unwrap_or(0.0) * 3.0
}
fn current_rating(r: &DifficultyReport) -> f32 {
    (1.0 + current_raw(r)).clamp(1.0, 10.0)
}

// Proposed alternative (arrived at after two false starts - see the
// write-up in discussions/DifficultyRating.md for the full path, including
// the two things that didn't work):
//  - revisit_density is left RAW, not bounded per-dimension. First attempt
//    bounded it via revisit_density/(revisit_density+C) so no single
//    dimension could dominate the sum - but that specifically suppressed
//    the signal that makes cult.txt distinctive (7.03 vs a "hard ordinary"
//    puzzle's ~2-3 compress to a much smaller gap once both are run through
//    the same saturating curve). Compression should happen exactly ONCE,
//    on the combined sum, not per-dimension - a dimension that's supposed
//    to discriminate extreme cases needs to keep its raw dynamic range
//    until it reaches that final step.
//  - size_tax's shape had to move together with this, not be deferred:
//    log2(cells) is too flat to carry cross-category separation at all
//    (5x5->100x100 only spans ~2.9x). Linear in (rows+cols) has enough
//    range (~6.7x over the same span) but at too large a coefficient it
//    overshoots and inverts the one ordering check available from the
//    user's own hand-timed data (cult's 50x50, 8h, must outrank Kaldogram
//    #3's 100x100, 3h18m) - purely by giving the 100x100 too much of a
//    free size-based boost. SIZE_TAX_K below is the smallest value found
//    that keeps that ordering correct against the whole corpus.
//  - final asymptotic squash, so nothing ever *needs* a hard clamp - the
//    half-scale constant is derived from the corpus's own median (see
//    below), never from cult.txt or any other single named puzzle.
const W_REVISIT: f32 = 1.0;
const W_YIELD: f32 = 1.0;
const W_NARROW: f32 = 1.0;

// size_tax's own shape is being reconsidered here too, not deferred:
// log2(cells) is too flat to carry cross-category separation (5x5 to
// 100x100 only spans a ~2.9x ratio in log2(cells)*k terms) - trying
// k * (rows+cols) instead, which spans a ~6.7x ratio over the same range
// and ties the tax to "how many lines you're tracking simultaneously",
// arguably closer to the original bookkeeping-load reasoning anyway.
const SIZE_TAX_K: f32 = 0.008;

fn proposed_size_tax(propagator_rows_plus_cols: f32) -> f32 {
    SIZE_TAX_K * propagator_rows_plus_cols
}

fn proposed_raw(r: &DifficultyReport, rows_plus_cols: f32) -> f32 {
    // NOT bounding revisit_density here anymore - see write-up. Leave it
    // raw so it retains its full discriminative power for genuinely
    // extreme cases; let the *final* transform (applied once, to the
    // whole sum) do the compression instead of doing it per-dimension.
    proposed_size_tax(rows_plus_cols)
        + r.revisit_density * W_REVISIT
        + r.yield_friction * W_YIELD
        + r.narrowness.unwrap_or(0.0) * W_NARROW
}

fn rescale(raw: f32, half_scale: f32) -> f32 {
    1.0 + 9.0 * raw / (raw + half_scale)
}

struct Row { file: String, name: String, w: usize, h: usize, old_rating: f32, new_raw: f32 }

fn main() {
    let mut rows = Vec::new();
    for path in files() {
        let puzzles = nonogram_core::parse_file(&path).expect("failed to read/parse puzzle file");
        for entry in &puzzles {
            let Some(puzzle) = entry.as_puzzle() else { continue };
            let Ok(report) = rate(puzzle) else { continue };
            let rows_plus_cols = (puzzle.width + puzzle.height) as f32;
            rows.push(Row {
                file: path.clone(),
                name: puzzle.name.clone(),
                w: puzzle.width,
                h: puzzle.height,
                old_rating: current_rating(&report),
                new_raw: proposed_raw(&report, rows_plus_cols),
            });
        }
    }

    // Half-scale (the raw value landing at rating 5.5, the curve's
    // midpoint) is derived from the corpus's own median raw score, times a
    // fixed multiplier - not from cult.txt or any other single named
    // puzzle. "Corpus" here excludes cult.txt itself only so a single
    // known-extreme outlier can't drag its own reference point around;
    // every other file (the full Katana set plus Kaldo.txt) counts.
    let mut bulk: Vec<f32> = rows.iter().filter(|r| r.file != "puzzles/cult.txt").map(|r| r.new_raw).collect();
    bulk.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = bulk[bulk.len() / 2];
    let p90 = bulk[(bulk.len() as f32 * 0.90) as usize];
    let half_scale = median * 8.0;

    println!("bulk corpus (excl. cult.txt) new_raw: p50={:.3} p90={:.3} p99={:.3} max={:.3}",
        median, p90, bulk[(bulk.len() as f32 * 0.99) as usize], bulk.last().unwrap());
    println!("half_scale (= 8 * bulk median) = {half_scale:.3}\n");

    // Per-file summary, old vs new.
    let mut by_file: std::collections::BTreeMap<String, Vec<&Row>> = Default::default();
    for r in &rows { by_file.entry(r.file.clone()).or_default().push(r); }

    println!("{:<55} {:>3} {:>13} {:>13} {:>13}", "file", "n", "old(min/mean/max)", "new_raw(min/mean/max)", "new_rating(min/mean/max)");
    for (file, group) in &by_file {
        let n = group.len();
        let old_min = group.iter().map(|r| r.old_rating).fold(f32::MAX, f32::min);
        let old_mean = group.iter().map(|r| r.old_rating).sum::<f32>() / n as f32;
        let old_max = group.iter().map(|r| r.old_rating).fold(f32::MIN, f32::max);
        let new_ratings: Vec<f32> = group.iter().map(|r| rescale(r.new_raw, half_scale)).collect();
        let new_min = new_ratings.iter().cloned().fold(f32::MAX, f32::min);
        let new_mean = new_ratings.iter().sum::<f32>() / n as f32;
        let new_max = new_ratings.iter().cloned().fold(f32::MIN, f32::max);
        println!(
            "{:<55} {:>3} {:>4.1}/{:>4.1}/{:>4.1} {:>18} {:>4.1}/{:>4.1}/{:>4.1}",
            file, n, old_min, old_mean, old_max, "", new_min, new_mean, new_max
        );
    }

    println!("\ncult.txt under new scheme:");
    let cult = rows.iter().find(|r| r.file == "puzzles/cult.txt").unwrap();
    println!("  new_raw={:.3} new_rating={:.2}", cult.new_raw, rescale(cult.new_raw, half_scale));

    println!("\ntop 5 by new_raw, whole corpus (incl. cult):");
    let mut all: Vec<&Row> = rows.iter().collect();
    all.sort_by(|a, b| b.new_raw.partial_cmp(&a.new_raw).unwrap());
    for r in all.iter().take(5) {
        println!("  {:.3}  {} {}x{} [{}]", r.new_raw, r.name, r.w, r.h, r.file);
    }
}
