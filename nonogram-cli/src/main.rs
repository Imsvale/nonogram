//! Command-line interface for the nonogram workspace.
//!
//! Usage: nonogram-cli <file> [--solver propagation|human-by-ai|graph-search|human] [--progress] [--verbose]
//!
//! Parses every puzzle in the file, solves each with the chosen solver, and
//! prints the result. If a puzzle has a known solution attached, the grid is
//! verified against it.

use std::io::Write;
use std::time::Duration;

use anyhow::Result;
use clap::{Parser, ValueEnum};
use nonogram_core::{AllSolutions, CellState, ExhaustiveSolver, ParsedPuzzle, Puzzle, SolutionState, Solver, SolveContext, SolveResult, parse_file};
use nonogram_graph_search::{GraphSearchSolver, ProgressConfig, ProgressUpdate};
use nonogram_human::HumanSolver;
use nonogram_human_by_ai::HumanByAiSolver;
use nonogram_propagation::PropagationSolver;

#[derive(Clone, ValueEnum)]
enum SolverChoice {
    /// Full per-line hard-logic deduction (category 3) — no named techniques,
    /// no search, proves everything a per-line argument can prove.
    Propagation,
    /// Named, human-legible technique passes only (category 2) — what
    /// `--solver propagation` used to mean before the category-3 extraction.
    HumanByAi,
    GraphSearch,
    Human,
}

#[derive(Parser)]
#[command(name = "nonogram-cli", about = "Solve nonogram puzzles")]
struct Cli {
    /// Path to a puzzle file (UTF-8, one puzzle per line).
    file: String,

    /// Which solver to use (default: propagation — full hard-logic deduction).
    #[arg(short, long, value_enum, default_value = "propagation")]
    solver: SolverChoice,

    /// Suppress all stdout output.
    #[arg(short, long)]
    quiet: bool,

    /// Enumerate all solutions (graph-search only).
    #[arg(long)]
    all: bool,

    /// Solve only puzzle N from the file (1-indexed).
    #[arg(short, long)]
    number: Option<usize>,

    /// Show a live search-progress status line on stderr (graph-search only).
    #[arg(long)]
    progress: bool,

    /// Log every productive solve step and a periodic summary to stderr
    /// (graph-search only). Independent of --progress.
    #[arg(short, long)]
    verbose: bool,
}

fn print_grid(puzzle: &Puzzle, grid: &[CellState], complete: bool) {
    for r in 0..puzzle.height {
        let row = &grid[r * puzzle.width..(r + 1) * puzzle.width];
        let line: String = row.iter().map(|c| match c {
            CellState::Filled  => '#',
            CellState::Empty   => if complete { '.' } else { 'x' },
            CellState::Unknown => '.',
        }).collect();
        println!("{}", line);
    }
}

fn verify(puzzle: &Puzzle, grid: &[CellState]) -> Option<Vec<(usize, usize)>> {
    let sol = puzzle.solution.as_ref()?;
    let mismatches: Vec<_> = grid.iter().enumerate()
        .filter(|&(i, &c)| c != sol[i])
        .map(|(i, _)| (i / puzzle.width, i % puzzle.width))
        .collect();
    if mismatches.is_empty() { None } else { Some(mismatches) }
}

fn run_puzzle(solver: &dyn Solver, puzzle: &Puzzle, quiet: bool) {
    let result = solver.solve(puzzle, &SolveContext::default());
    print_result(puzzle, &result, quiet);
}

/// Print a status line of pushed/expanded/heap/known-cell counters, overwriting
/// itself in place via `\r`. Used by `--progress`; ended with a bare `eprintln!()`
/// once the solve finishes so later output starts on a fresh line.
fn print_progress_line(u: &ProgressUpdate) {
    let pct = if u.cells_total > 0 { u.cells_known as f64 / u.cells_total as f64 * 100.0 } else { 0.0 };
    eprint!(
        "\rpushed={} expanded={} heap={} min_count={} known={}/{} ({pct:.1}%) elapsed={:.1}s   ",
        u.nodes_pushed, u.nodes_expanded, u.heap_len, u.best_min_count,
        u.cells_known, u.cells_total, u.elapsed.as_secs_f64(),
    );
    let _ = std::io::stderr().flush();
}

/// Like `run_puzzle`, but drives `GraphSearchSolver::solve_with_progress` so
/// `--progress`/`--verbose` can observe the search. `--quiet` suppresses the
/// live status line along with the rest of the output; it does not affect
/// `--verbose`'s log-crate output, which is configured independently via
/// `RUST_LOG`/`env_logger`.
fn run_puzzle_progress(puzzle: &Puzzle, verbose: bool, progress: bool, quiet: bool) {
    let config = ProgressConfig {
        log_steps: verbose,
        log_meta_interval: verbose.then(|| Duration::from_millis(500)),
        snapshot_interval: (progress && !quiet).then(|| Duration::from_millis(200)),
        emit_start_snapshot: progress && !quiet,
    };
    let mut on_progress = |u: ProgressUpdate| print_progress_line(&u);
    let cb: Option<&mut dyn FnMut(ProgressUpdate)> =
        if config.snapshot_interval.is_some() { Some(&mut on_progress) } else { None };
    let result = GraphSearchSolver.solve_with_progress(puzzle, &SolveContext::default(), &config, cb);
    if config.snapshot_interval.is_some() { eprintln!(); }
    print_result(puzzle, &result, quiet);
}

fn print_result(puzzle: &Puzzle, result: &SolveResult, quiet: bool) {
    if quiet { return; }
    println!("=== {} ({}×{}) ===", puzzle.name, puzzle.width, puzzle.height);
    match &result.state {
        SolutionState::Complete => {
            println!("Solved.");
            print_grid(puzzle, &result.grid, true);
            match verify(puzzle, &result.grid) {
                Some(mm) => {
                    println!("WRONG: {} cell(s) differ from known solution:", mm.len());
                    for (r, c) in mm { println!("  row={r} col={c}"); }
                }
                None if puzzle.solution.is_some() => println!("Verified correct."),
                None => {}
            }
            if let Some(ans) = &puzzle.answer {
                println!("Answer: {ans}");
            }
        }
        SolutionState::Partial => {
            println!("Partial — no further progress:");
            print_grid(puzzle, &result.grid, false);
        }
        SolutionState::Unsolvable => {
            println!("No solution exists.");
            print_grid(puzzle, &result.grid, false);
        }
        SolutionState::Aborted => {
            println!("Aborted — partial grid:");
            print_grid(puzzle, &result.grid, false);
        }
        SolutionState::Invalid(reason) => println!("Invalid puzzle — {reason}"),
    }
    println!();
}

fn run_puzzle_all(puzzle: &Puzzle, result: &AllSolutions, quiet: bool) {
    if quiet { return; }
    let n = result.solutions.len();
    if n == 0 {
        println!("=== {} ({}×{}) ===", puzzle.name, puzzle.width, puzzle.height);
        if result.aborted { println!("Search aborted — no solution found yet."); }
        else { println!("No solution found."); }
        println!();
        return;
    }
    for (i, sol) in result.solutions.iter().enumerate() {
        println!("=== {} ({}×{}) — solution {}/{n} ===", puzzle.name, puzzle.width, puzzle.height, i + 1);
        print_grid(puzzle, &sol.grid, true);
        match verify(puzzle, &sol.grid) {
            Some(mm) => {
                println!("WRONG: {} cell(s) differ from known solution:", mm.len());
                for (r, c) in mm { println!("  row={r} col={c}"); }
            }
            None if puzzle.solution.is_some() => println!("Verified correct."),
            None => {}
        }
        println!();
    }
    let label = if n == 1 { "solution" } else { "solutions" };
    if result.aborted {
        println!("{n} {label} found (search aborted).  nodes_expanded: {}", result.nodes_expanded);
    } else {
        println!("{n} {label} found.  nodes_expanded: {}", result.nodes_expanded);
    }
    println!();
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut puzzles = parse_file(&cli.file)?;

    if let Some(n) = cli.number {
        if n == 0 || n > puzzles.len() {
            eprintln!("error: puzzle number {n} is out of range (file contains {} puzzle(s))", puzzles.len());
            std::process::exit(1);
        }
        puzzles = vec![puzzles.remove(n - 1)];
    }

    if cli.progress || cli.verbose {
        let flag = if cli.progress { "--progress" } else { "--verbose" };
        match cli.solver {
            SolverChoice::GraphSearch => {}
            SolverChoice::Propagation => { eprintln!("error: {flag} is not supported by the 'propagation' solver; use --solver graph-search"); std::process::exit(1); }
            SolverChoice::HumanByAi   => { eprintln!("error: {flag} is not supported by the 'human-by-ai' solver; use --solver graph-search"); std::process::exit(1); }
            SolverChoice::Human       => { eprintln!("error: {flag} is not supported by the 'human' solver; use --solver graph-search"); std::process::exit(1); }
        }
        if cli.all {
            eprintln!("error: {flag} is not supported together with --all yet");
            std::process::exit(1);
        }
    }

    if cli.verbose {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("nonogram_graph_search=debug")).init();
    }

    if cli.all {
        match cli.solver {
            SolverChoice::GraphSearch => {}
            SolverChoice::Propagation => { eprintln!("error: --all is not supported by the 'propagation' solver; use --solver graph-search"); std::process::exit(1); }
            SolverChoice::HumanByAi   => { eprintln!("error: --all is not supported by the 'human-by-ai' solver; use --solver graph-search"); std::process::exit(1); }
            SolverChoice::Human       => { eprintln!("error: --all is not supported by the 'human' solver; use --solver graph-search"); std::process::exit(1); }
        }
        let solver = GraphSearchSolver;
        for entry in &puzzles {
            match entry {
                ParsedPuzzle::Valid(p) => {
                    let result = solver.solve_all(p, &SolveContext::default());
                    run_puzzle_all(p, &result, cli.quiet);
                }
                ParsedPuzzle::Invalid { name, reason, .. } => eprintln!("[INVALID] {name} — {reason}"),
            }
        }
        return Ok(());
    }

    if cli.progress || cli.verbose {
        for entry in &puzzles {
            match entry {
                ParsedPuzzle::Valid(p) => run_puzzle_progress(p, cli.verbose, cli.progress, cli.quiet),
                ParsedPuzzle::Invalid { name, reason, .. } => eprintln!("[INVALID] {name} — {reason}"),
            }
        }
        return Ok(());
    }

    let solver: Box<dyn Solver> = match cli.solver {
        SolverChoice::Propagation => Box::new(PropagationSolver),
        SolverChoice::HumanByAi   => Box::new(HumanByAiSolver),
        SolverChoice::GraphSearch => Box::new(GraphSearchSolver),
        SolverChoice::Human       => Box::new(HumanSolver),
    };

    for entry in &puzzles {
        match entry {
            ParsedPuzzle::Valid(p) => run_puzzle(solver.as_ref(), p, cli.quiet),
            ParsedPuzzle::Invalid { name, reason, .. } => eprintln!("[INVALID] {name} — {reason}"),
        }
    }

    Ok(())
}
