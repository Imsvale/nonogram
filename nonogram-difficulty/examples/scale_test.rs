//! Ad hoc calibration probe: does revisit_density / yield_friction stay
//! roughly flat as a *fixed shape* (same logical complexity) is upscaled by
//! nearest-neighbor pixel repetition, or does it drift with sheer size?
//!
//! If revisit_density/yield_friction are truly measuring "cross-referential
//! tangledness, not size" (their stated design goal), a shape that's easy at
//! 10x10 should stay easy — near-flat revisit/yield — at 100x100, with only
//! size_tax climbing. If they drift upward with scale, some of what the
//! formula attributes to "friction" is actually latent size effect that
//! should move into size_tax instead.
//!
//! Not a permanent test — a throwaway probe for `discussions/DifficultyRating.md`.

use nonogram_core::Puzzle;
use nonogram_difficulty::rate;

fn clues_from_line(line: &[bool]) -> Vec<u32> {
    let mut clues = Vec::new();
    let mut run = 0u32;
    for &b in line {
        if b {
            run += 1;
        } else if run > 0 {
            clues.push(run);
            run = 0;
        }
    }
    if run > 0 {
        clues.push(run);
    }
    clues
}

fn upscale(base: &[Vec<bool>], k: usize, name: &str) -> Puzzle {
    let h0 = base.len();
    let w0 = base[0].len();
    let h = h0 * k;
    let w = w0 * k;
    let mut grid = vec![false; h * w];
    for r in 0..h {
        for c in 0..w {
            grid[r * w + c] = base[r / k][c / k];
        }
    }
    let row_clues: Vec<Vec<u32>> = (0..h).map(|r| clues_from_line(&grid[r * w..(r + 1) * w])).collect();
    let col_clues: Vec<Vec<u32>> = (0..w)
        .map(|c| clues_from_line(&(0..h).map(|r| grid[r * w + c]).collect::<Vec<_>>()))
        .collect();
    Puzzle {
        name: format!("{name}-{k}x"),
        answer: None,
        width: w,
        height: h,
        row_clues,
        col_clues,
        solution: None,
    }
}

fn run_series(name: &str, base: &[Vec<bool>], scales: &[usize]) {
    println!("--- {name} ---");
    println!("{:<10} {:<6} {:<8} {:<9} {:<9} {:<9}", "size", "rating", "size_tax", "revisit", "yield", "branch?");
    for &k in scales {
        let p = upscale(base, k, name);
        match rate(&p) {
            Ok(r) => println!(
                "{:<10} {:<6.2} {:<8.3} {:<9.3} {:<9.3} {}",
                format!("{}x{}", p.width, p.height),
                r.rating,
                r.size_tax,
                r.revisit_density,
                r.yield_friction,
                r.requires_branching
            ),
            Err(e) => println!("k={k}: {:?}", e),
        }
    }
    println!();
}

fn main() {
    // Shape A: a simple blob/diamond, single contiguous run per line.
    // Generically easy for propagation (edges pin down inward).
    let blob: Vec<Vec<bool>> = vec![
        vec![false, false, false, true, true, true, true, false, false, false],
        vec![false, false, true, true, true, true, true, true, false, false],
        vec![false, true, true, true, true, true, true, true, true, false],
        vec![true, true, true, true, true, true, true, true, true, true],
        vec![true, true, true, true, true, true, true, true, true, true],
        vec![false, true, true, true, true, true, true, true, true, false],
        vec![false, false, true, true, true, true, true, true, false, false],
        vec![false, false, false, true, true, true, true, false, false, false],
        vec![false, false, false, false, true, true, false, false, false, false],
        vec![false, false, false, false, true, true, false, false, false, false],
    ];

    // Shape B: multiple disjoint blocks per line (checkerboard-of-blocks),
    // meant to force genuine cross-line ambiguity, not just edge-in overlap.
    let mut tangled = vec![vec![false; 12]; 12];
    for r in 0..12 {
        for c in 0..12 {
            // two blocks per line, offset by a diagonal shear so rows/cols
            // can't each be resolved from overlap alone at small scale.
            let band1 = (c + r / 3) % 12 < 3;
            let band2 = (c + r / 3 + 6) % 12 < 2;
            tangled[r][c] = band1 || band2;
        }
    }

    let scales = [1, 2, 4, 6, 8, 10];
    run_series("blob", &blob, &scales);
    run_series("tangled", &tangled, &scales);
}
