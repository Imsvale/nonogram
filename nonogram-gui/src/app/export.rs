use nonogram_core::{CellState, Puzzle};

pub(crate) fn puzzle_to_puzzlink_url(puzzle: &Puzzle) -> String {
    let w = puzzle.width;
    let h = puzzle.height;
    let g_col = (h + 1) / 2;
    let g_row = (w + 1) / 2;

    fn encode_value(v: u32, out: &mut String) {
        if v <= 9 {
            out.push(char::from(b'0' + v as u8));
        } else if v <= 15 {
            out.push(char::from(b'a' + (v - 10) as u8));
        } else {
            out.push('-');
            out.push_str(&format!("{:02x}", v));
        }
    }

    fn encode_group(clues: &[u32], g: usize, out: &mut String) {
        for &v in clues.iter().rev() {
            encode_value(v, out);
        }
        let zeros = g.saturating_sub(clues.len());
        out.push(char::from(b'f' + zeros as u8));
    }

    let mut data = String::new();
    for c in 0..w {
        encode_group(&puzzle.col_clues[c], g_col, &mut data);
    }
    for r in 0..h {
        encode_group(&puzzle.row_clues[r], g_row, &mut data);
    }

    format!("https://puzz.link/p?nonogram/{}/{}/{}", w, h, data)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExportFormat {
    PuzPreV3,
}

impl ExportFormat {
    pub(crate) const ALL: &'static [Self] = &[Self::PuzPreV3];
    pub(crate) fn label(self) -> &'static str {
        match self { Self::PuzPreV3 => "Puz-Pre v3" }
    }
}

pub(crate) fn puzzle_to_file_string(puzzle: &Puzzle) -> String {
    let encode = |clues: &[Vec<u32>]| -> String {
        clues.iter()
            .map(|g| if g.is_empty() { "0".into() } else { g.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(" ") })
            .collect::<Vec<_>>()
            .join("|")
    };
    let mut s = format!("{};C:{}/R:{}", puzzle.name, encode(&puzzle.col_clues), encode(&puzzle.row_clues));
    match (&puzzle.answer, &puzzle.solution) {
        (Some(ans), Some(sol)) => {
            s.push(';'); s.push_str(ans);
            s.push(';');
            for st in sol { s.push(if *st == CellState::Filled { '1' } else { '0' }); }
        }
        (Some(ans), None) => { s.push(';'); s.push_str(ans); }
        (None, Some(sol))  => {
            s.push_str(";;");
            for st in sol { s.push(if *st == CellState::Filled { '1' } else { '0' }); }
        }
        (None, None) => {}
    }
    s
}

/// Puz-Pre v3 format.
///
/// Layout: `(max_col_depth + height)` rows × `(max_row_width + width)` cols,
/// space-separated. Top-left corner is all dots. Top section holds column
/// clues (bottom-aligned). Left section holds row clues (right-aligned).
/// Bottom-right section is the puzzle grid (`.` = empty/unknown, `#` = filled).
/// Clues for fulfilled lines are prefixed with `c`.
pub(crate) fn export_puzprv3(
    row_clues: &[Vec<u32>],
    col_clues: &[Vec<u32>],
    w: usize,
    h: usize,
    grid: Option<&[CellState]>,
    fulfilled_rows: &[bool],
    fulfilled_cols: &[bool],
) -> String {
    let max_cd = (h + 1) / 2;
    let max_rd = (w + 1) / 2;
    let total_rows = max_cd + h;
    let total_cols = max_rd + w;

    let mut out = String::new();
    out.push_str("pzprv3\nnonogram\n");
    out.push_str(&h.to_string()); out.push('\n');
    out.push_str(&w.to_string()); out.push('\n');

    for row_idx in 0..total_rows {
        let mut tokens: Vec<String> = Vec::with_capacity(total_cols);
        for col_idx in 0..total_cols {
            let token = if row_idx < max_cd {
                if col_idx < max_rd {
                    ".".into()
                } else {
                    let c = col_idx - max_rd;
                    let clues = &col_clues[c];
                    let pad = max_cd - clues.len();
                    if row_idx >= pad {
                        let n = clues[row_idx - pad];
                        if fulfilled_cols.get(c).copied().unwrap_or(false) {
                            format!("c{n}")
                        } else {
                            n.to_string()
                        }
                    } else {
                        ".".into()
                    }
                }
            } else {
                let r = row_idx - max_cd;
                if col_idx < max_rd {
                    let clues = &row_clues[r];
                    let pad = max_rd - clues.len();
                    if col_idx >= pad {
                        let n = clues[col_idx - pad];
                        if fulfilled_rows.get(r).copied().unwrap_or(false) {
                            format!("c{n}")
                        } else {
                            n.to_string()
                        }
                    } else {
                        ".".into()
                    }
                } else {
                    let c = col_idx - max_rd;
                    match grid.map(|g| g[r * w + c]).unwrap_or(CellState::Unknown) {
                        CellState::Filled => "#".into(),
                        _ => ".".into(),
                    }
                }
            };
            tokens.push(token);
        }
        out.push_str(&tokens.join(" "));
        out.push('\n');
    }
    out
}
