use std::collections::HashSet;
use std::path::{Path, PathBuf};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

/// Returns `full_path` relative to the current working directory, using forward
/// slashes. Falls back to the original string if it isn't under the cwd.
pub(crate) fn relative_path(full_path: &str) -> String {
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(rel) = Path::new(full_path).strip_prefix(&cwd) {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }
    full_path.replace('\\', "/")
}

fn solved_path() -> Option<PathBuf> {
    ProjectDirs::from("", "nonogram", "nonogram-gui")
        .map(|pd| pd.config_dir().join("solved.json"))
}

// New grouped format: one entry per file, puzzle names listed under it.
#[derive(Serialize, Deserialize)]
struct SolvedFile { path: String, puzzles: Vec<String> }

// Legacy flat format kept only for reading old solved.json files.
#[derive(Deserialize)]
struct SolvedEntryFlat { path: String, name: String }

pub(crate) fn load_solved() -> HashSet<(String, String)> {
    let path = match solved_path() { Some(p) => p, None => return HashSet::new() };
    let content = match std::fs::read_to_string(&path) { Ok(s) => s, Err(_) => return HashSet::new() };

    // Try the current grouped format first; fall back to the old flat format.
    if let Ok(files) = serde_json::from_str::<Vec<SolvedFile>>(&content) {
        return files.into_iter()
            .flat_map(|f| f.puzzles.into_iter().map(move |n| (f.path.clone(), n)))
            .collect();
    }
    if let Ok(entries) = serde_json::from_str::<Vec<SolvedEntryFlat>>(&content) {
        return entries.into_iter().map(|e| (e.path, e.name)).collect();
    }
    HashSet::new()
}

pub(crate) fn save_solved(solved: &HashSet<(String, String)>) {
    let Some(path) = solved_path() else { return };
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }

    let mut grouped: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
    for (p, n) in solved {
        grouped.entry(p.as_str()).or_default().push(n.as_str());
    }
    for names in grouped.values_mut() { names.sort_unstable(); }
    let files: Vec<SolvedFile> = grouped.into_iter()
        .map(|(p, ns)| SolvedFile { path: p.to_owned(), puzzles: ns.iter().map(|&s| s.to_owned()).collect() })
        .collect();

    if let Ok(json) = serde_json::to_string_pretty(&files) {
        let _ = std::fs::write(&path, json);
    }
}

fn session_path() -> Option<PathBuf> {
    ProjectDirs::from("", "nonogram", "nonogram-gui")
        .map(|pd| pd.config_dir().join("session.json"))
}

#[derive(Serialize, Deserialize)]
struct SessionEntry { path: String, name: String }

pub(crate) fn save_session(rel_path: &str, puzzle_name: &str) {
    let Some(path) = session_path() else { return };
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let entry = SessionEntry { path: rel_path.to_string(), name: puzzle_name.to_string() };
    if let Ok(json) = serde_json::to_string(&entry) { let _ = std::fs::write(&path, json); }
}

pub(crate) fn load_session() -> Option<(String, String)> {
    let path = session_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    let entry: SessionEntry = serde_json::from_str(&content).ok()?;
    Some((entry.path, entry.name))
}

// ---------------------------------------------------------------------------
// Window state
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct SavedWindowState {
    x: f32, y: f32, width: f32, height: f32,
    #[serde(default)]
    maximized: bool,
}

fn window_state_path() -> Option<PathBuf> {
    ProjectDirs::from("", "nonogram", "nonogram-gui")
        .map(|pd| pd.config_dir().join("window.json"))
}

/// Returns `(width, height, Option<(x, y)>, maximized)`. Position is `None`
/// when no saved state exists so the caller can apply `window::Position::Default`.
pub(crate) fn load_window_state() -> (f32, f32, Option<(f32, f32)>, bool) {
    let path = match window_state_path() { Some(p) => p, None => return (1200.0, 780.0, None, false) };
    let content = match std::fs::read_to_string(&path) { Ok(s) => s, Err(_) => return (1200.0, 780.0, None, false) };
    let s: SavedWindowState = match serde_json::from_str(&content) { Ok(s) => s, Err(_) => return (1200.0, 780.0, None, false) };
    (s.width, s.height, Some((s.x, s.y)), s.maximized)
}

pub(crate) fn save_window_state(width: f32, height: f32, x: f32, y: f32, maximized: bool) {
    let Some(path) = window_state_path() else { return };
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let saved = SavedWindowState { x, y, width, height, maximized };
    if let Ok(json) = serde_json::to_string(&saved) { let _ = std::fs::write(&path, json); }
}

/// Write the solved grid back into the puzzle file as the fourth `;`-field.
/// Matches the puzzle line by name (first field). Skips gracefully on I/O errors.
pub(crate) fn save_solution_to_file(file_path: &str, puzzle_name: &str, solution: &str) {
    let Ok(content) = std::fs::read_to_string(file_path) else { return };
    let trailing_newline = content.ends_with('\n');
    let new_content: String = content.lines().map(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            return line.to_string();
        }
        let first = trimmed.splitn(2, ';').next().unwrap_or("").trim();
        if first != puzzle_name {
            return line.to_string();
        }
        let parts: Vec<&str> = trimmed.splitn(4, ';').collect();
        if parts.len() < 2 { return line.to_string(); }
        let answer = parts.get(2).unwrap_or(&"").trim();
        format!("{};{};{};{}", parts[0], parts[1], answer, solution)
    }).collect::<Vec<_>>().join("\n");
    let new_content = if trailing_newline { format!("{new_content}\n") } else { new_content };
    let _ = std::fs::write(file_path, new_content);
}

