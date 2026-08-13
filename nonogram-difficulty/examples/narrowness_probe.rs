//! Throwaway probe: now that `ForceabilityAtRound` exists, decide min-vs-mean
//! for narrowness empirically instead of by pure argument. Prints both
//! summary stats across every puzzle in a file so we can see whether `min`
//! saturates near its floor for nearly every puzzle (an artifact of how
//! solves necessarily end: the last few rounds almost always have very few
//! lines left in play) or actually discriminates.

use nonogram_core::{CellState, Puzzle};
use nonogram_propagation::{ForceabilityAtRound, Propagator};

fn narrowness_series(rounds: &[ForceabilityAtRound]) -> Vec<f32> {
    rounds
        .iter()
        .filter(|r| r.unresolved >= 2)
        .map(|r| 1.0 - (r.forceable as f32 - 1.0) / (r.unresolved as f32 - 1.0))
        .collect()
}

// NOTE: `narrowness_series` values are already "large = narrow" (1 = only
// one line forceable out of many unresolved, 0 = wide open). So the
// single-tightest-bottleneck statistic is the MAX of the series, not the
// min — min would report the *widest-open* round instead. Caught this by
// checking the first run's numbers against cult.txt by hand before trusting
// either column below.
fn stats(puzzle: &Puzzle) -> Option<(f32, f32, usize)> {
    let propagator = Propagator::from_puzzle(puzzle);
    let mut grid = vec![CellState::Unknown; propagator.rows * propagator.cols];
    let (ok, rounds) = propagator.propagate_with_telemetry(&mut grid, &mut Vec::new(), false);
    if !ok {
        return None;
    }
    let series = narrowness_series(&rounds);
    if series.is_empty() {
        return None;
    }
    let max = series.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let mean = series.iter().sum::<f32>() / series.len() as f32;
    Some((max, mean, series.len()))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: narrowness_probe <file>");
    let puzzles = nonogram_core::parse_file(&path).expect("parse failed");

    let mut maxes = Vec::new();
    let mut means = Vec::new();
    println!("{:<28} {:<10} {:<8} {:<8} {:<6}", "name", "size", "max(bottleneck)", "mean", "rounds");
    for entry in &puzzles {
        let Some(p) = entry.as_puzzle() else { continue };
        if let Some((max, mean, n)) = stats(p) {
            println!("{:<28} {:<10} {:<8.3} {:<8.3} {:<6}", p.name, format!("{}x{}", p.width, p.height), max, mean, n);
            maxes.push(max);
            means.push(mean);
        }
    }
    if maxes.is_empty() {
        return;
    }
    maxes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    means.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |v: &[f32], p: f32| v[((v.len() as f32 - 1.0) * p) as usize];
    println!("\n{} puzzles with >=1 narrowness sample", maxes.len());
    println!("max-based:  p10={:.3} p50={:.3} p90={:.3} frac==1.0: {:.1}%", pct(&maxes, 0.1), pct(&maxes, 0.5), pct(&maxes, 0.9),
        100.0 * maxes.iter().filter(|&&v| v >= 0.999).count() as f32 / maxes.len() as f32);
    println!("mean-based: p10={:.3} p50={:.3} p90={:.3} frac==1.0: {:.1}%", pct(&means, 0.1), pct(&means, 0.5), pct(&means, 0.9),
        100.0 * means.iter().filter(|&&v| v >= 0.999).count() as f32 / means.len() as f32);
}
