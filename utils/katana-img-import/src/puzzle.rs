pub fn format_puzzle(name: &str, answer: &str, grid: &[Vec<bool>]) -> String {
    let rows = grid.len() as u32;
    let cols = grid.first().map_or(0, |r| r.len() as u32);

    let col_clues: Vec<Vec<u32>> = (0..cols)
        .map(|c| runs((0..rows).map(|r| grid[r as usize][c as usize])))
        .collect();

    let row_clues: Vec<Vec<u32>> = grid
        .iter()
        .map(|row| runs(row.iter().copied()))
        .collect();

    format!(
        "{};C:{}/R:{};{}",
        name,
        fmt_clue_list(&col_clues),
        fmt_clue_list(&row_clues),
        answer,
    )
}

fn fmt_clue_list(clues: &[Vec<u32>]) -> String {
    clues.iter()
        .map(|c| c.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("|")
}

fn runs(cells: impl Iterator<Item = bool>) -> Vec<u32> {
    let mut result = Vec::new();
    let mut run = 0u32;
    for filled in cells {
        if filled {
            run += 1;
        } else if run > 0 {
            result.push(run);
            run = 0;
        }
    }
    if run > 0 {
        result.push(run);
    }
    if result.is_empty() {
        result.push(0);
    }
    result
}
