use nonogram_core::{CellState, ParsedPuzzle, Puzzle};

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
        if zeros <= 20 {
            out.push(char::from(b'f' + zeros as u8));
        } else {
            out.push('z');
            out.push(char::from(b'f' + (zeros - 20) as u8));
        }
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
/// Decode a puzz.link nonogram URL into a puzzle list.
///
/// Accepts the full URL form `https://puzz.link/p?nonogram/W/H/data` or any
/// string containing that path segment. The decode is purely local — no HTTP.
pub(crate) fn parse_puzzlink_url(url: &str) -> Result<(String, Vec<ParsedPuzzle>), String> {
    let marker = "nonogram/";
    let pos = url.find(marker).ok_or("Not a puzz.link nonogram URL")?;
    let rest = &url[pos + marker.len()..];

    let mut parts = rest.splitn(3, '/');
    let w: usize = parts.next().ok_or("Missing width")?.parse().map_err(|_| "Invalid width")?;
    let h: usize = parts.next().ok_or("Missing height")?.parse().map_err(|_| "Invalid height")?;
    let data = parts.next().ok_or("Missing encoded data")?;

    if w == 0 || h == 0 { return Err("Zero-dimension puzzle".into()); }

    let g_col = (h + 1) / 2;
    let g_row = (w + 1) / 2;

    let mut chars = data.chars().peekable();

    let mut col_clues: Vec<Vec<u32>> = Vec::with_capacity(w);
    for c in 0..w {
        col_clues.push(
            decode_pz_group(&mut chars, g_col)
                .map_err(|e| format!("col {}: {e}", c + 1))?
        );
    }

    let mut row_clues: Vec<Vec<u32>> = Vec::with_capacity(h);
    for r in 0..h {
        row_clues.push(
            decode_pz_group(&mut chars, g_row)
                .map_err(|e| format!("row {}: {e}", r + 1))?
        );
    }

    let name = format!("{}×{}", w, h);
    let puzzle = Puzzle {
        name:      name.clone(),
        width:     w,
        height:    h,
        col_clues,
        row_clues,
        answer:    None,
        solution:  None,
    };

    Ok((format!("puzz.link {name}"), vec![ParsedPuzzle::Valid(puzzle)]))
}

fn decode_pz_group(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    g: usize,
) -> Result<Vec<u32>, String> {
    let mut values: Vec<u32> = Vec::new();

    let zeros = loop {
        // When all g slots are filled, the next char must be 'f' (0-zeros suffix).
        if values.len() == g {
            match chars.next() {
                Some('f') => break 0,
                Some(c)   => return Err(format!("expected 'f' suffix, got '{c}'")),
                None      => return Err("unexpected end of data".into()),
            }
        }
        match chars.next() {
            None => return Err("unexpected end of data in group".into()),
            // Suffix character ≥ 'g': single-char or two-char 'z?' form.
            // 'f' never appears here when values.len() < g because that would
            // imply 0 zeros = g − values.len() > 0, a contradiction.
            Some(c) if c >= 'g' => {
                break if c == 'z' {
                    // Peek: if next char ≥ 'g', it's a 2-char suffix (zeros 21+).
                    match chars.peek().copied() {
                        Some(c2) if c2 >= 'g' => {
                            chars.next();
                            20 + (c2 as u8 - b'f') as usize
                        }
                        _ => 20,
                    }
                } else {
                    (c as u8 - b'f') as usize
                };
            }
            Some(c @ '0'..='9') => values.push(c as u32 - '0' as u32),
            Some(c @ 'a'..='f') => values.push(10 + c as u32 - 'a' as u32),
            Some('-') => {
                let h1 = chars.next().ok_or("truncated hex value")?;
                let h2 = chars.next().ok_or("truncated hex value")?;
                let hex = format!("{h1}{h2}");
                values.push(u32::from_str_radix(&hex, 16)
                    .map_err(|_| format!("invalid hex: -{hex}"))?);
            }
            Some(c) => return Err(format!("unexpected char '{c}'")),
        }
    };

    if values.len() + zeros != g {
        return Err(format!(
            "group size mismatch: {} values + {zeros} zeros ≠ {g}",
            values.len()
        ));
    }
    values.reverse();
    Ok(values)
}

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
