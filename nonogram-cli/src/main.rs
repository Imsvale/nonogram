//! Command-line interface for the nonogram workspace.
//!
//! Usage: nonogram-cli <file> [--solver propagation|graph-search|human]
//!
//! Parses every puzzle in the file, solves each with the chosen solver, and
//! prints the result. If a puzzle has a known solution attached, the grid is
//! verified against it.

use anyhow::Result;
use clap::{Parser, ValueEnum};
use nonogram_core::{CellState, Outcome, Puzzle, Solver, SolveContext, parse_file};
use nonogram_graph_search::GraphSearchSolver;
use nonogram_human::HumanSolver;
use nonogram_propagation::PropagationSolver;

#[derive(Clone, ValueEnum)]
enum SolverChoice {
    Propagation,
    GraphSearch,
    Human,
}

#[derive(Parser)]
#[command(name = "nonogram-cli", about = "Solve nonogram puzzles")]
struct Cli {
    /// Path to a puzzle file (UTF-8, one puzzle per line).
    file: String,

    /// Which solver to use (default: propagation).
    #[arg(short, long, value_enum, default_value = "propagation")]
    solver: SolverChoice,

    /// Suppress all stdout output.
    #[arg(short, long)]
    quiet: bool,
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
    if quiet { solver.solve(puzzle, &SolveContext::default()); return; }
    println!("=== {} ({}×{}) ===", puzzle.name, puzzle.width, puzzle.height);
    let result = solver.solve(puzzle, &SolveContext::default());
    match result.outcome {
        Outcome::Solved => {
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
        }
        Outcome::Stuck => {
            println!("Stuck — partial grid:");
            if !result.grid.is_empty() { print_grid(puzzle, &result.grid, false); }
        }
        Outcome::NoSolution => println!("No solution found."),
    }
    println!();
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let puzzles = parse_file(&cli.file)?;

    let solver: Box<dyn Solver> = match cli.solver {
        SolverChoice::Propagation => Box::new(PropagationSolver),
        SolverChoice::GraphSearch => Box::new(GraphSearchSolver),
        SolverChoice::Human       => Box::new(HumanSolver),
    };

    for puzzle in &puzzles {
        run_puzzle(solver.as_ref(), puzzle, cli.quiet);
    }

    Ok(())
}
