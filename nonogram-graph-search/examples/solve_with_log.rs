//! Dev tool: solve one puzzle from a puzzle file with progress logged to a
//! file, for watching a slow or possibly-stuck search from the outside.
//!
//! Usage: cargo run -p nonogram-graph-search --example solve_with_log -- \
//!            <puzzle-file> [puzzle-name] [log-file]
//!
//! `puzzle-name` defaults to the first puzzle in the file; `log-file`
//! defaults to `solve.log` (appended to, not overwritten).

use nonogram_core::{ParsedPuzzle, SolveContext};
use nonogram_graph_search::{init_file_logger, GraphSearchSolver, ProgressConfig};
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let puzzle_path = args.next()
        .expect("usage: solve_with_log <puzzle-file> [puzzle-name] [log-file]");
    let puzzle_name = args.next();
    let log_path = args.next().unwrap_or_else(|| "solve.log".to_string());

    init_file_logger(&log_path, log::LevelFilter::Debug)
        .expect("failed to open log file (is a logger already installed?)");
    println!("logging to {log_path}");

    let puzzles = nonogram_core::parse_file(&puzzle_path)
        .expect("failed to read/parse puzzle file");
    let puzzle = match &puzzle_name {
        Some(name) => puzzles.iter().find(|p| p.name() == name),
        None => puzzles.first(),
    }
        .and_then(ParsedPuzzle::as_puzzle)
        .expect("puzzle not found, or structurally invalid");

    println!("solving {} ({}x{})...", puzzle.name, puzzle.width, puzzle.height);

    let config = ProgressConfig {
        log_steps: true,
        log_meta_interval: Some(Duration::from_secs(1)),
        snapshot_interval: None,
        ..ProgressConfig::default()
    };
    let ctx = SolveContext::default();
    let start = std::time::Instant::now();
    let result = GraphSearchSolver.solve_with_progress(puzzle, &ctx, &config, None);

    println!("done in {:.1}s: {:?} ({} step(s) recorded)", start.elapsed().as_secs_f32(), result.state, result.steps.len());
}
