//! CSV dump of the standard test set (same files as `standard_set.rs`), one
//! row per puzzle, for external graphing. Includes both each dimension's
//! raw (unweighted) value and its weighted contribution to `raw`, so a
//! stacked/grouped chart can show what's actually driving each puzzle's
//! score, not just the final number.
//!
//! Usage (from the workspace root): cargo run -p nonogram-difficulty --example csv_report > report.csv

use nonogram_difficulty::{rate, DifficultyWeights};

/// Same standard test set as `standard_set.rs`: the two hand-timed anchor
/// files, plus every puzzle file under `puzzles/Katana/`.
fn files() -> Vec<String> {
    let mut files = vec!["puzzles/cult.txt".to_string(), "puzzles/Kaldo.txt".to_string()];
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

fn main() {
    let weights = DifficultyWeights::default();
    println!(
        "file,name,width,height,cells,lines,rating,raw,size_tax,revisit_density,revisit_contrib,yield_friction,yield_contrib,narrowness,narrowness_contrib,requires_branching"
    );
    for path in files() {
        let path = path.as_str();
        let puzzles = match nonogram_core::parse_file(path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping {path}: {e:?}");
                continue;
            }
        };
        for entry in &puzzles {
            let Some(puzzle) = entry.as_puzzle() else { continue };
            let r = match rate(puzzle) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("skipping {}: {:?}", puzzle.name, e);
                    continue;
                }
            };
            let revisit_contrib = r.revisit_density * weights.revisit_weight;
            let yield_contrib = r.yield_friction * weights.yield_weight;
            let narrowness_contrib = r.narrowness.unwrap_or(0.0) * weights.narrowness_weight;
            let raw = r.size_tax + revisit_contrib + yield_contrib + narrowness_contrib;
            println!(
                "{},{:?},{},{},{},{},{:.2},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{:.3},{}",
                path,
                puzzle.name,
                puzzle.width,
                puzzle.height,
                puzzle.width * puzzle.height,
                puzzle.width + puzzle.height,
                r.rating,
                raw,
                r.size_tax,
                r.revisit_density,
                revisit_contrib,
                r.yield_friction,
                yield_contrib,
                r.narrowness.map(|n| format!("{n:.3}")).unwrap_or_default(),
                narrowness_contrib,
                r.requires_branching,
            );
        }
    }
}
