use std::path::Path;

pub(crate) async fn scan_puzzle_dirs() -> Vec<(String, String, Option<String>)> {
    let mut out = Vec::new();
    let dir = "puzzles";
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.sort_by(|a, b| natural_cmp(a, b));
        for path in paths {
            if path.is_file() && path.extension().map(|e| e == "txt").unwrap_or(false) {
                let path_str = path.to_string_lossy().into_owned();
                let name = path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path_str.clone());
                out.push((path_str, name, None));
            } else if path.is_dir() {
                let folder_name = path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if let Ok(sub_entries) = std::fs::read_dir(&path) {
                    let mut sub_paths: Vec<_> = sub_entries
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.is_file() && p.extension().map(|e| e == "txt").unwrap_or(false))
                        .collect();
                    sub_paths.sort_by(|a, b| natural_cmp(a, b));
                    for sub_path in sub_paths {
                        let sub_str = sub_path.to_string_lossy().into_owned();
                        let name = sub_path.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| sub_str.clone());
                        out.push((sub_str, name, Some(folder_name.clone())));
                    }
                }
            }
        }
    }
    out
}

fn natural_cmp(a: &Path, b: &Path) -> std::cmp::Ordering {
    natural_str_cmp(&a.to_string_lossy(), &b.to_string_lossy())
}

fn natural_str_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _)    => return std::cmp::Ordering::Less,
            (_, None)    => return std::cmp::Ordering::Greater,
            (Some(ac), Some(bc)) if ac.is_ascii_digit() && bc.is_ascii_digit() => {
                let an = collect_digits(&mut ai);
                let bn = collect_digits(&mut bi);
                match an.cmp(&bn) {
                    std::cmp::Ordering::Equal => {}
                    ord => return ord,
                }
            }
            (Some(ac), Some(bc)) => {
                let ac = ac.to_ascii_lowercase();
                let bc = bc.to_ascii_lowercase();
                ai.next();
                bi.next();
                match ac.cmp(&bc) {
                    std::cmp::Ordering::Equal => {}
                    ord => return ord,
                }
            }
        }
    }
}

fn collect_digits(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> u64 {
    let mut n = 0u64;
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            n = n * 10 + (c as u64 - b'0' as u64);
            chars.next();
        } else {
            break;
        }
    }
    n
}
