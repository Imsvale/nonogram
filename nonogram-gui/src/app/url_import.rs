use nonogram_core::{ParsedPuzzle, Puzzle};

// ── puzzle-nonograms.com HTML format ────────────────────────────────────────
//
// The page embeds puzzle data as:
//   var task = 'col1clue1.col1clue2/.../colN.../row1clue1.row1clue2/.../rowM...';
//   ...
//   puzzleWidth: W, puzzleHeight: H
//
// The single slash-delimited string contains W+H groups; first W are columns,
// next H are rows. Within each group, clues are dot-separated integers.

fn extract_str_between<'a>(html: &'a str, before: &str, after: char) -> Option<&'a str> {
    let start = html.find(before)? + before.len();
    let end = html[start..].find(after)?;
    Some(&html[start..start + end])
}

fn extract_usize_after(html: &str, marker: &str) -> Option<usize> {
    let pos = html.find(marker)? + marker.len();
    let tail = html[pos..].trim_start_matches(|c: char| c == ' ' || c == ':' || c == '\t');
    let end = tail.find(|c: char| !c.is_ascii_digit()).unwrap_or(tail.len());
    if end == 0 { return None; }
    tail[..end].parse().ok()
}

fn extract_page_title(html: &str) -> Option<String> {
    let start = html.find("<title>")? + 7;
    let end   = html[start..].find("</title>")?;
    let title = html[start..start + end].trim();
    if title.is_empty() { None } else { Some(title.to_string()) }
}

pub(crate) fn parse_puzzle_nonograms_html(
    html: &str,
    url: &str,
) -> Result<(String, Vec<ParsedPuzzle>), String> {
    let task_str = extract_str_between(html, "var task = '", '\'')
        .ok_or_else(|| "Could not find `var task = '...'` in page source".to_string())?;

    let width  = extract_usize_after(html, "puzzleWidth:")
        .ok_or_else(|| "Could not find puzzleWidth in page source".to_string())?;
    let height = extract_usize_after(html, "puzzleHeight:")
        .ok_or_else(|| "Could not find puzzleHeight in page source".to_string())?;

    let groups: Vec<Vec<u32>> = task_str.split('/')
        .map(|g| g.split('.').filter_map(|n| n.parse::<u32>().ok()).collect())
        .collect();

    if groups.len() != width + height {
        return Err(format!(
            "Expected {} groups ({}×{}) but got {}",
            width + height, width, height, groups.len()
        ));
    }

    let col_clues = groups[..width].to_vec();
    let row_clues = groups[width..].to_vec();

    let site_title = extract_page_title(html)
        .filter(|t| !t.to_lowercase().contains("nonogram"))
        .unwrap_or_else(|| "puzzle-nonograms.com".to_string());

    let id = url.find("pl=")
        .map(|i| &url[i + 3..])
        .and_then(|s| s.split('&').next())
        .map(|s| if s.len() > 12 { &s[..12] } else { s })
        .unwrap_or("imported");

    let name = format!("{} {}×{} #{}", site_title, width, height, id);

    let puzzle = Puzzle {
        name: name.clone(),
        width,
        height,
        row_clues,
        col_clues,
        answer:   None,
        solution: None,
    };

    Ok((name, vec![ParsedPuzzle::Valid(puzzle)]))
}

pub(crate) async fn fetch_puzzle_from_url(
    url: String,
) -> Result<(String, Vec<ParsedPuzzle>), String> {
    let body = reqwest::get(&url)
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {e}"))?;

    parse_puzzle_nonograms_html(&body, &url)
}
