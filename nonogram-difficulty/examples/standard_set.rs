//! The user's standing test set for eyeballing difficulty calibration:
//! `cult.txt`, `Kaldo.txt`, and every puzzle file under `Katana/`. Prints a
//! full breakdown for every puzzle in each file, plus a per-file summary, to
//! stdout — redirect to a file for a saved report. Re-run this after any
//! weight/formula change to see how the standard set moves.
//!
//! Usage (from the workspace root): cargo run -p nonogram-difficulty --example standard_set > report.txt

use nonogram_difficulty::{rate, DifficultyReport, DifficultyWeights};

/// Recomputes the pre-`+1`/pre-clamp raw sum from a report's visible
/// components. Not exposed on `DifficultyReport` itself (yet) — duplicated
/// here rather than there because this is throwaway investigation, not a
/// crate API decision.
fn raw(r: &DifficultyReport, w: &DifficultyWeights) -> f32 {
    r.size_tax + r.revisit_density * w.revisit_weight + r.yield_friction * w.yield_weight
        + r.narrowness.unwrap_or(0.0) * w.narrowness_weight
}

/// The standard test set: the two hand-timed anchor files, plus every
/// puzzle file under `puzzles/Katana/` (Test-2 expanded this from the
/// four size-bucket files used in Test-1 to the full Katana set).
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
    for path in files() {
        let path = path.as_str();
        println!("=== {path} ===");
        let puzzles = match nonogram_core::parse_file(path) {
            Ok(p) => p,
            Err(e) => {
                println!("  failed to parse: {e:?}\n");
                continue;
            }
        };

        let weights = DifficultyWeights::default();
        let mut ratings = Vec::new();
        let mut raws = Vec::new();
        for entry in &puzzles {
            let Some(puzzle) = entry.as_puzzle() else { continue };
            match rate(puzzle) {
                Ok(r) => {
                    let raw_score = raw(&r, &weights);
                    println!(
                        "{} ({}x{})",
                        puzzle.name, puzzle.width, puzzle.height
                    );
                    println!("  rating:             {:.1}  (raw={:.2})", r.rating, raw_score);
                    println!("  size_tax:           {:.3}", r.size_tax);
                    println!("  revisit_density:    {:.3}", r.revisit_density);
                    println!("  yield_friction:     {:.3}", r.yield_friction);
                    println!(
                        "  narrowness:         {}",
                        r.narrowness.map(|n| format!("{n:.3}")).unwrap_or_else(|| "n/a".into())
                    );
                    println!("  requires_branching: {}", r.requires_branching);
                    println!();
                    ratings.push(r.rating);
                    raws.push(raw_score);
                }
                Err(e) => println!("{}: {:?}\n", puzzle.name, e),
            }
        }

        if !ratings.is_empty() {
            let n = ratings.len() as f32;
            let mean = ratings.iter().sum::<f32>() / n;
            let min = ratings.iter().cloned().fold(f32::INFINITY, f32::min);
            let max = ratings.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let at_ceiling = ratings.iter().filter(|&&r| r >= 10.0).count();
            let raw_mean = raws.iter().sum::<f32>() / n;
            let raw_max = raws.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            println!(
                "-- {} puzzle(s): min={:.1} mean={:.2} max={:.1} at_ceiling={}/{}  (raw mean={:.2} raw max={:.2})",
                ratings.len(), min, mean, max, at_ceiling, ratings.len(), raw_mean, raw_max
            );
            let mut hist = [0usize; 10];
            for &r in &ratings {
                let bucket = ((r.ceil() as usize).max(1) - 1).min(9);
                hist[bucket] += 1;
            }
            print!("-- histogram: ");
            for (i, count) in hist.iter().enumerate() {
                print!("({}-{}]:{} ", i, i + 1, count);
            }
            println!();
        }
        println!();
    }
}
