//! CSV dump of Claude-graph's rescale prototype (see `rescale_prototype.rs`
//! for the formula and reasoning), one row per puzzle, run against the full
//! real puzzle corpus ("Test-3"). Same constants as `rescale_prototype.rs`
//! — kept duplicated rather than shared, matching this crate's existing
//! precedent for throwaway investigation examples (`csv_report.rs` vs.
//! `standard_set.rs`).
//!
//! Usage (from the workspace root): cargo run -p nonogram-difficulty --release --example rescale_prototype_csv > report.csv

use nonogram_difficulty::{rate, DifficultyReport};

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

fn current_raw(r: &DifficultyReport) -> f32 {
    r.size_tax + r.revisit_density * 2.0 + r.yield_friction * 4.0 + r.narrowness.unwrap_or(0.0) * 3.0
}
fn current_rating(r: &DifficultyReport) -> f32 {
    (1.0 + current_raw(r)).clamp(1.0, 10.0)
}

const W_REVISIT: f32 = 1.0;
const W_YIELD: f32 = 1.0;
const W_NARROW: f32 = 1.0;
const SIZE_TAX_K: f32 = 0.008;

fn proposed_size_tax(rows_plus_cols: f32) -> f32 {
    SIZE_TAX_K * rows_plus_cols
}

fn proposed_raw(r: &DifficultyReport, rows_plus_cols: f32) -> f32 {
    proposed_size_tax(rows_plus_cols)
        + r.revisit_density * W_REVISIT
        + r.yield_friction * W_YIELD
        + r.narrowness.unwrap_or(0.0) * W_NARROW
}

fn rescale(raw: f32, half_scale: f32) -> f32 {
    1.0 + 9.0 * raw / (raw + half_scale)
}

struct Row {
    file: String,
    name: String,
    w: usize,
    h: usize,
    old_rating: f32,
    new_raw: f32,
    revisit_density: f32,
    yield_friction: f32,
    narrowness: Option<f32>,
    requires_branching: bool,
}

fn main() {
    let mut rows = Vec::new();
    for path in files() {
        let puzzles = match nonogram_core::parse_file(&path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping {path}: {e:?}");
                continue;
            }
        };
        for entry in &puzzles {
            let Some(puzzle) = entry.as_puzzle() else { continue };
            let report = match rate(puzzle) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("skipping {}: {:?}", puzzle.name, e);
                    continue;
                }
            };
            let rows_plus_cols = (puzzle.width + puzzle.height) as f32;
            rows.push(Row {
                file: path.clone(),
                name: puzzle.name.clone(),
                w: puzzle.width,
                h: puzzle.height,
                old_rating: current_rating(&report),
                new_raw: proposed_raw(&report, rows_plus_cols),
                revisit_density: report.revisit_density,
                yield_friction: report.yield_friction,
                narrowness: report.narrowness,
                requires_branching: report.requires_branching,
            });
        }
    }

    // Same half_scale derivation as rescale_prototype.rs: 8x the bulk
    // corpus's median new_raw, cult.txt excluded so a single known-extreme
    // outlier can't drag its own reference point around.
    let mut bulk: Vec<f32> = rows.iter().filter(|r| r.file != "puzzles/cult.txt").map(|r| r.new_raw).collect();
    bulk.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = bulk[bulk.len() / 2];
    let half_scale = median * 8.0;
    eprintln!("half_scale (= 8 * bulk median, n={}) = {half_scale:.3}", bulk.len());

    println!("file,name,width,height,cells,lines,old_rating,new_raw,new_rating,revisit_density,yield_friction,narrowness,requires_branching");
    for r in &rows {
        let new_rating = rescale(r.new_raw, half_scale);
        println!(
            "{},{:?},{},{},{},{},{:.2},{:.3},{:.2},{:.3},{:.3},{},{}",
            r.file,
            r.name,
            r.w,
            r.h,
            r.w * r.h,
            r.w + r.h,
            r.old_rating,
            r.new_raw,
            new_rating,
            r.revisit_density,
            r.yield_friction,
            r.narrowness.map(|n| format!("{n:.3}")).unwrap_or_default(),
            r.requires_branching,
        );
    }
}
