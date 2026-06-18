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

#[derive(Serialize, Deserialize)]
struct SolvedEntry { path: String, name: String }

pub(crate) fn load_solved() -> HashSet<(String, String)> {
    let path = match solved_path() { Some(p) => p, None => return HashSet::new() };
    let content = match std::fs::read_to_string(&path) { Ok(s) => s, Err(_) => return HashSet::new() };
    let entries: Vec<SolvedEntry> = match serde_json::from_str(&content) { Ok(e) => e, Err(_) => return HashSet::new() };
    entries.into_iter().map(|e| (e.path, e.name)).collect()
}

pub(crate) fn save_solved(solved: &HashSet<(String, String)>) {
    let Some(path) = solved_path() else { return };
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let mut entries: Vec<SolvedEntry> = solved.iter()
        .map(|(p, n)| SolvedEntry { path: p.clone(), name: n.clone() })
        .collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path).then(a.name.cmp(&b.name)));
    if let Ok(json) = serde_json::to_string_pretty(&entries) {
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

