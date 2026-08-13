//! Rate one puzzle, or every puzzle in a file, for manual sanity-checking
//! of the difficulty formula against real puzzles.
//!
//! Usage:
//!   cargo run -p nonogram-difficulty --example rate -- <file>          (table of every puzzle in the file, sorted by rating)
//!   cargo run -p nonogram-difficulty --example rate -- <file> <index>  (full breakdown for one puzzle, 1-based)

use nonogram_difficulty::rate;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: rate <file> [1-based index]");
    let index: Option<usize> = args.next().map(|s| s.parse().expect("index must be a number"));

    let puzzles = nonogram_core::parse_file(&path).expect("failed to read/parse puzzle file");

    if let Some(idx) = index {
        let entry = puzzles.get(idx - 1).expect("index out of range");
        let puzzle = entry.as_puzzle().expect("invalid puzzle entry");
        match rate(puzzle) {
            Ok(r) => {
                println!("{} ({}x{})", puzzle.name, puzzle.width, puzzle.height);
                println!("  rating:            {:.1}", r.rating);
                println!("  size_tax:          {:.3}", r.size_tax);
                println!("  revisit_density:   {:.3}", r.revisit_density);
                println!("  yield_friction:    {:.3}", r.yield_friction);
                println!("  narrowness:        {}", r.narrowness.map(|n| format!("{n:.3}")).unwrap_or_else(|| "n/a (pending telemetry)".into()));
                println!("  requires_branching:{}", r.requires_branching);
            }
            Err(e) => println!("{}: {:?}", puzzle.name, e),
        }
        return;
    }

    let mut rows: Vec<(f32, String, usize, usize, bool, f32, f32)> = Vec::new();
    for entry in &puzzles {
        let Some(puzzle) = entry.as_puzzle() else { continue };
        match rate(puzzle) {
            Ok(r) => rows.push((r.rating, puzzle.name.clone(), puzzle.width, puzzle.height, r.requires_branching, r.revisit_density, r.yield_friction)),
            Err(e) => eprintln!("skipping {}: {:?}", puzzle.name, e),
        }
    }
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    println!("{:<6} {:<8} {:<28} {:<9} {:<9} {:<9}", "rating", "size", "name", "branch?", "revisit", "yield");
    for (rating, name, w, h, branching, revisit, yield_f) in &rows {
        println!(
            "{:<6.1} {:<8} {:<28} {:<9} {:<9.3} {:<9.3}",
            rating, format!("{w}x{h}"), name, if *branching { "yes" } else { "" }, revisit, yield_f
        );
    }
    println!("\n{} puzzle(s) rated", rows.len());
}
