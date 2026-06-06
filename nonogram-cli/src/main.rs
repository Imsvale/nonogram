//! Command-line interface for the nonogram workspace.
//!
//! Usage: nonogram-cli <file> [--solver propagation|graph-search|human]
//!
//! Parses every puzzle in the file, solves each with the chosen solver, and
//! prints the result. If a puzzle has a known solution attached, the grid is
//! verified against it.

use anyhow::Result;
use clap::{Parser, ValueEnum};
use nonogram_core::{AllSolutions, CellState, ExhaustiveSolver, Outcome, Puzzle, Solver, SolveContext, parse_file};
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

    /// Enumerate all solutions (graph-search only).
    #[arg(long)]
    all: bool,

    /// Solve only puzzle N from the file (1-indexed).
    #[arg(short, long)]
    number: Option<usize>,
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
        Outcome::NoSolution    => println!("No solution found."),
        Outcome::InvalidPuzzle => println!("Invalid puzzle — structural contradiction, not attempted."),
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

    if cli.all {
        match cli.solver {
            SolverChoice::GraphSearch => {}
            SolverChoice::Propagation => { eprintln!("error: --all is not supported by the 'propagation' solver; use --solver graph-search"); std::process::exit(1); }
            SolverChoice::Human       => { eprintln!("error: --all is not supported by the 'human' solver; use --solver graph-search"); std::process::exit(1); }
        }
        let solver = GraphSearchSolver;
        for puzzle in &puzzles {
            let result = solver.solve_all(puzzle, &SolveContext::default());
            run_puzzle_all(puzzle, &result, cli.quiet);
        }
        return Ok(());
    }

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
