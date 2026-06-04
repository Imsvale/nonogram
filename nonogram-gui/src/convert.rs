//! Converts letter-encoded puzzle files to Vec<Puzzle>.
//! Same A=1…Z=26 encoding as nonogram-convert.

use nonogram_core::Puzzle;

fn decode_token(token: &str) -> Option<Vec<u32>> {
    token.chars().map(|c| {
        if c.is_ascii_uppercase() {
            Some(c as u32 - b'A' as u32 + 1)
        } else {
            None
        }
    }).collect()
}

fn decode_line(line: &str) -> Option<Vec<Vec<u32>>> {
    line.split_whitespace().map(decode_token).collect()
}

fn normalize(clues: Vec<Vec<u32>>) -> Vec<Vec<u32>> {
    clues.into_iter().map(|mut c| { c.retain(|&n| n != 0); c }).collect()
}

/// Parse letter-encoded `content` into `Puzzle`s. Blocks separated by blank lines.
/// Line 1 of each block = row clues, line 2 = col clues.
pub fn convert_letter_content(content: &str, name_prefix: &str) -> Vec<Puzzle> {
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    let mut current: Vec<&str> = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            if !current.is_empty() {
                blocks.push(std::mem::take(&mut current));
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }

    blocks.iter().enumerate().filter_map(|(i, block)| {
        if block.len() != 2 { return None; }
        let row_clues = normalize(decode_line(block[0])?);
        let col_clues = normalize(decode_line(block[1])?);
        let width  = col_clues.len();
        let height = row_clues.len();
        Some(Puzzle {
            name: format!("{} {}", name_prefix, i + 1),
            width,
            height,
            row_clues,
            col_clues,
            solution: None,
        })
    }).collect()
}
