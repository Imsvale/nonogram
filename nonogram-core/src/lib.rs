//! Shared types, puzzle parser, and solver trait for the nonogram workspace.
//!
//! Every crate in the workspace depends on this one. It owns the puzzle
//! *definition* (clues, dimensions, optional known solution) but contains no
//! solving logic — that lives entirely inside the individual solver crates.

use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Cell state
// ---------------------------------------------------------------------------

/// The three-valued state of a single grid cell.
///
/// Corresponds to the domain terminology: `Filled` = Shaded, `Empty` = Blank.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CellState {
    Unknown,
    Filled,
    Empty,
}

// ---------------------------------------------------------------------------
// Puzzle definition (immutable)
// ---------------------------------------------------------------------------

/// Immutable puzzle definition: dimensions, clues, optional known solution.
///
/// Contains no solving state. Each solver constructs its own internal working
/// representation from a `&Puzzle`.
#[derive(Clone, Debug)]
pub struct Puzzle {
    pub name: String,
    /// The real title, revealed after solving. `None` if not provided in the file.
    pub answer: Option<String>,
    pub width: usize,
    pub height: usize,
    /// `row_clues[r]` is the clue list for row `r`.
    pub row_clues: Vec<Vec<u32>>,
    /// `col_clues[c]` is the clue list for column `c`.
    pub col_clues: Vec<Vec<u32>>,
    /// Known solution for test validation; absent for unsolved puzzles.
    /// Flat, row-major. `1` = Filled, `0` = Empty in the source file.
    pub solution: Option<Vec<CellState>>,
}

// ---------------------------------------------------------------------------
// Line identifier
// ---------------------------------------------------------------------------

/// Identifies a row or column — used in solve steps and solver internals.
#[derive(Clone, Copy, Debug)]
pub enum LineId {
    Row(usize),
    Col(usize),
}

// ---------------------------------------------------------------------------
// Solve result and step tracing
// ---------------------------------------------------------------------------

/// One logical deduction or branching decision made during solving.
///
/// Solvers that support tracing populate this; others return an empty `steps`
/// vec. The GUI / investigation layer can replay steps to show the reasoning.
#[derive(Clone, Debug)]
pub struct SolveStep {
    /// Human-readable description of what happened (e.g. "overlap on row 3").
    pub description: String,
    /// The line this step acted on, if applicable.
    pub line: Option<LineId>,
    /// Cells whose state changed as a result of this step: (row, col, new_state).
    pub cells_changed: Vec<(usize, usize, CellState)>,
}

/// The state of a solve attempt.
///
/// Variants are mutually exclusive and exhaustive. Every solve ends in exactly one.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SolutionState {
    /// Every cell was determined and the grid is consistent.
    Complete,
    /// The solver ran out of techniques; solvability is unknown.
    /// A stronger solver may make further progress.
    Partial,
    /// The search space was exhausted — no valid grid exists. This is a proof.
    /// The returned grid holds all cells that propagation determined before the contradiction.
    Unsolvable,
    /// The solve was interrupted by a `CancelToken`.
    /// The returned grid is valid partial progress up to the abort point.
    Aborted,
    /// The puzzle is structurally malformed; no solve was attempted.
    /// The string describes the specific contradiction.
    Invalid(String),
}

/// Everything a solver returns: solution state, final grid, and optional trace.
///
/// `grid` is always populated except when `state == Invalid`, where it is empty.
#[derive(Clone, Debug)]
pub struct SolveResult {
    pub state: SolutionState,
    /// Flat row-major grid. Empty only when `state == Invalid`.
    pub grid: Vec<CellState>,
    /// Ordered trace of deductions. Empty if the solver does not support tracing.
    pub steps: Vec<SolveStep>,
}

// ---------------------------------------------------------------------------
// Cancellation
// ---------------------------------------------------------------------------

/// Cloneable cancellation token. Clone before spawning a solver task; call
/// `cancel()` from the controlling thread to request cooperative abort.
///
/// Solvers check `ctx.cancel.is_cancelled()` at natural loop boundaries and
/// return early with `SolveResult { aborted: true, .. }` when set.
#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
    pub fn reset(&self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

/// Context passed to `Solver::solve`. Extensible without further trait changes.
#[derive(Clone, Default)]
pub struct SolveContext {
    pub cancel: CancelToken,
}

// ---------------------------------------------------------------------------
// Solver trait
// ---------------------------------------------------------------------------

pub trait Solver {
    fn solve(&self, puzzle: &Puzzle, ctx: &SolveContext) -> SolveResult;
}

// ---------------------------------------------------------------------------
// Exhaustive search
// ---------------------------------------------------------------------------

/// All solutions found by an exhaustive search.
///
/// `solutions.len()` is the outcome: 0 = no solution, 1 = unique, ≥2 = ambiguous.
/// When `aborted` is true the search was cancelled early and `solutions` is partial.
#[derive(Clone, Debug)]
pub struct AllSolutions {
    /// Every valid solution found, each with its own grid and step trace.
    pub solutions: Vec<SolveResult>,
    /// Heap nodes popped and expanded (completions generated).
    pub nodes_expanded: usize,
    /// Heap nodes pushed (candidates generated). High push:expand ratio → wide branching.
    pub nodes_pushed: usize,
    /// `true` if the search was cut short by the `CancelToken`.
    pub aborted: bool,
}

/// Implemented by solvers capable of enumerating all solutions to a puzzle.
///
/// Currently only `GraphSearchSolver`. The human-by-ai and human solvers
/// cannot enumerate solutions without fundamental rethinking.
pub trait ExhaustiveSolver {
    fn solve_all(&self, puzzle: &Puzzle, ctx: &SolveContext) -> AllSolutions;
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Errors that can occur while parsing a single puzzle line.
#[derive(Error, Debug, Clone)]
pub enum ParseError {
    #[error("missing clue section")]
    MissingClues,
    #[error("empty puzzle name")]
    EmptyName,
    #[error("invalid clue number: {0}")]
    InvalidClue(String),
    #[error("clue section missing C: or R: prefix: \"{0}\"")]
    InvalidClueSection(String),
    #[error("solution length {got} does not match grid {expected}")]
    SolutionLength { got: usize, expected: usize },
    #[error("row clues sum to {row_total} but column clues sum to {col_total}")]
    ClueTotalMismatch { row_total: u32, col_total: u32 },
}

/// Result of parsing a single puzzle entry.
///
/// `parse_file` returns a `Vec<ParsedPuzzle>`. I/O failures propagate as the
/// outer `Err`; structural invalidity (bad clue format, total mismatch, etc.)
/// yields `Invalid` for that entry and parsing continues to the next line.
#[derive(Debug, Clone)]
pub enum ParsedPuzzle {
    Valid(Puzzle),
    /// Structurally invalid puzzle entry. `puzzle` is `Some` when the clue
    /// structure parsed successfully but the totals mismatched — the `Puzzle`
    /// is fully formed except for that inconsistency. `puzzle` is `None` for
    /// hard parse failures (missing sections, bad tokens, etc.).
    Invalid { name: String, reason: ParseError, puzzle: Option<Puzzle> },
}

impl ParsedPuzzle {
    pub fn name(&self) -> &str {
        match self {
            ParsedPuzzle::Valid(p) => &p.name,
            ParsedPuzzle::Invalid { name, .. } => name,
        }
    }
    pub fn as_puzzle(&self) -> Option<&Puzzle> {
        match self {
            ParsedPuzzle::Valid(p) => Some(p),
            ParsedPuzzle::Invalid { .. } => None,
        }
    }
    pub fn is_valid(&self) -> bool {
        matches!(self, ParsedPuzzle::Valid(_))
    }
}

/// Parse a pipe-delimited clue section (after stripping the `C:` or `R:` prefix).
///
/// `"3 1|1 1 1|5"` → `[[3, 1], [1, 1, 1], [5]]`.
fn parse_pipe_group(s: &str) -> Result<Vec<Vec<u32>>, ParseError> {
    s.split('|')
        .map(|line_clues| {
            line_clues
                .split_whitespace()
                .map(|n| n.parse::<u32>().map_err(|_| ParseError::InvalidClue(n.to_string())))
                .collect()
        })
        .collect()
}

/// Strip the `C:` or `R:` prefix from a clue section string.
fn split_section_prefix(s: &str) -> Result<(char, &str), ParseError> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("C:") {
        Ok(('C', rest))
    } else if let Some(rest) = s.strip_prefix("R:") {
        Ok(('R', rest))
    } else {
        Err(ParseError::InvalidClueSection(s.chars().take(20).collect()))
    }
}

/// Strip zero sentinels: `[0]` → `[]` (entirely empty line).
fn normalize_clues(clues: Vec<Vec<u32>>) -> Vec<Vec<u32>> {
    clues
        .into_iter()
        .map(|mut c| {
            if c.iter().any(|&n| n == 0) {
                c.retain(|&n| n != 0);
            }
            c
        })
        .collect()
}

/// Parse one puzzle line in the format:
/// `name;C:col_clues/R:row_clues[;answer[;solution]]`
///
/// Field count determines optional fields:
/// - 2: name + clues only
/// - 3: name + clues + answer (no solution)
/// - 4: name + clues + answer (may be empty) + solution
///
/// `C:` and `R:` prefixes are mandatory; either order is accepted.
/// Clue groups within each section are `|`-separated; values within a group are space-separated.
fn parse_puzzle_line(line: &str) -> Result<Puzzle, ParseError> {
    let parts: Vec<&str> = line.splitn(4, ';').collect();
    if parts.len() < 2 { return Err(ParseError::MissingClues); }

    let name = parts[0].trim().to_string();
    if name.is_empty() { return Err(ParseError::EmptyName); }

    let sections: Vec<&str> = parts[1].trim().splitn(2, '/').collect();
    if sections.len() != 2 { return Err(ParseError::MissingClues); }

    let (a_tag, a_str) = split_section_prefix(sections[0])?;
    let (b_tag, b_str) = split_section_prefix(sections[1])?;

    let (col_clues, row_clues) = match (a_tag, b_tag) {
        ('C', 'R') => (parse_pipe_group(a_str)?, parse_pipe_group(b_str)?),
        ('R', 'C') => (parse_pipe_group(b_str)?, parse_pipe_group(a_str)?),
        _ => return Err(ParseError::InvalidClueSection(parts[1].chars().take(20).collect())),
    };

    let col_clues = normalize_clues(col_clues);
    let row_clues = normalize_clues(row_clues);

    let width = col_clues.len();
    let height = row_clues.len();

    let answer = parts.get(2).and_then(|s| {
        let s = s.trim();
        if s.is_empty() { None } else { Some(s.to_string()) }
    });

    let solution = if parts.len() == 4 {
        let sol: Result<Vec<CellState>, _> = parts[3]
            .trim()
            .chars()
            .map(|c| match c {
                '1' => Ok(CellState::Filled),
                '0' => Ok(CellState::Empty),
                _ => Err(ParseError::InvalidClue(c.to_string())),
            })
            .collect();
        let sol = sol?;
        if sol.len() != width * height {
            return Err(ParseError::SolutionLength { got: sol.len(), expected: width * height });
        }
        Some(sol)
    } else {
        None
    };

    Ok(Puzzle { name, answer, width, height, row_clues, col_clues, solution })
}

/// Read and parse every puzzle from a UTF-8 text file.
///
/// Lines starting with `#` and blank lines are skipped. I/O errors propagate
/// as `Err`; structural invalidity in a single puzzle yields `ParsedPuzzle::Invalid`
/// for that entry and parsing continues to the next line.
///
/// Format: `name;C:col_clues/R:row_clues[;answer[;solution]]`
///
/// - Fields are `;`-separated (up to 4).
/// - The clue section uses `C:` / `R:` prefixes (either order); clue groups within
///   each section are `|`-separated; values within a group are space-separated.
/// - `answer` is the real title, revealed after solving. Empty string is treated as absent.
/// - `solution` is a flat binary string (`1`=Filled, `0`=Empty). Present only in field 4.
pub fn parse_file(path: &str) -> Result<Vec<ParsedPuzzle>, anyhow::Error> {
    let content = std::fs::read_to_string(path)?;
    let mut results = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }

        match parse_puzzle_line(line) {
            Ok(puzzle) => {
                let row_total: u32 = puzzle.row_clues.iter().flat_map(|r| r.iter()).sum();
                let col_total: u32 = puzzle.col_clues.iter().flat_map(|c| c.iter()).sum();
                if row_total != col_total {
                    let reason = ParseError::ClueTotalMismatch { row_total, col_total };
                    results.push(ParsedPuzzle::Invalid {
                        name: puzzle.name.clone(),
                        reason,
                        puzzle: Some(puzzle),
                    });
                } else {
                    results.push(ParsedPuzzle::Valid(puzzle));
                }
            }
            Err(reason) => {
                let name = line.splitn(2, ';').next()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| line.chars().take(40).collect());
                results.push(ParsedPuzzle::Invalid { name, reason, puzzle: None });
            }
        }
    }

    Ok(results)
}
