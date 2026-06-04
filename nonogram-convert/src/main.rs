//! Convert nonogram puzzle files from external formats to the solver's native format.
//!
//! Reads a file containing one or more puzzles in a supported format and writes
//! each puzzle to stdout as a single `name|col_clues/row_clues` line.
//!
//! # Usage
//! ```text
//! nonogram-convert [OPTIONS] <input>
//! nonogram-convert rosettacode.txt --name "RC" >> puzzles/mine.txt
//! nonogram-convert puzzles.txt --sep comma,colon,dash -r
//! nonogram-convert unknown.txt --detect
//! ```

use clap::Parser;
use std::fs;
use std::path::Path;
use std::process;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

const SEPARATOR_HELP: &str = "\
SEPARATOR TOKENS (for -s/--sep A,B,C[,D])
  Separators in parse-hierarchy order: clue values (A), lines (B),
  section row/col (C), puzzles (D, default: blankline). All must be distinct.

  Named tokens:
    cm  comma                 ,      cl  colon          :
    sc  semi  semicolon       ;      sp  space          (space)
    tb  tab                   (tab)  da  dash hy hyphen -
    sl  slash                 /      nl  new  newline   (LF, normalized)
    bl  blank blankline              (2+ consecutive newlines)

  Multi-character strings (e.g. COLUMNS) are passed as literals.

  Safe as bare literals — no quoting needed in bash/zsh/PowerShell:
    :  -  /  .  +  =  and printable ASCII that is not a shell metacharacter.
  Always use tokens for:  ,  ;  (space)  newline  tab  and blank lines.

  Examples:
    --sep comma,colon,dash            R puzzle-string format (D defaults to blankline)
    --sep semicolon,newline,COLUMNS   expanded semicolon format
    --sep space,newline,COLUMNS       expanded plain format";

#[derive(Parser)]
#[command(
    name = "nonogram-convert",
    about = "Convert nonogram formats to name|col/row",
    after_long_help = SEPARATOR_HELP
)]
struct Cli {
    /// Input file path.
    input: String,

    /// Input format preset (ignored when --sep is given).
    #[arg(short, long, value_enum, default_value = "letter")]
    format: InputFormat,

    /// Prefix for generated puzzle names ("Prefix 1", "Prefix 2", …).
    #[arg(short, long, default_value = "Puzzle")]
    name: String,

    /// Force row-major: the first section contains row clues (default).
    #[arg(short = 'r', long = "row", conflicts_with = "col")]
    row: bool,

    /// Force col-major: the first section contains column clues.
    /// Overrides any ordering hint embedded in the file.
    #[arg(short = 'c', long = "col", conflicts_with = "row")]
    col: bool,

    /// Custom separator spec: A,B,C[,D] — clue, line, section, puzzle.
    /// D defaults to blankline. Overrides --format. Run --help for token reference.
    #[arg(short = 's', long = "sep")]
    sep: Option<String>,

    /// Detect separators from the file and report; do not convert.
    #[arg(short = 'd', long = "detect")]
    detect: bool,
}

#[derive(Clone, clap::ValueEnum)]
enum InputFormat {
    /// Two-line blocks: row clues then col clues, uppercase A=1…Z=26.
    Letter,
    /// One clue per line, semicolons separate run lengths, "COLUMNS" divides rows from cols.
    Semicolon,
    /// One clue per line, spaces separate run lengths, "COLUMNS" divides rows from cols.
    Plain,
}

// ---------------------------------------------------------------------------
// Separator types
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
enum Sep {
    /// Any literal string (e.g. ",", ":", "COLUMNS").
    Literal(String),
    /// A single LF after CRLF normalization.
    Newline,
    /// A run of 2+ consecutive newlines, absorbing whitespace-only lines between them.
    BlankLine,
}

struct SeparatorConfig {
    clue: Sep,    // A — between run-length values within one clue
    line: Sep,    // B — between individual clues (lines)
    section: Sep, // C — between the rows section and the cols section
    puzzle: Sep,  // D — between puzzles in the file
}

fn resolve_token(s: &str) -> Sep {
    match s {
        "cm" | "comma" => Sep::Literal(",".into()),
        "cl" | "colon" => Sep::Literal(":".into()),
        "sc" | "semi" | "semicolon" => Sep::Literal(";".into()),
        "sp" | "space" => Sep::Literal(" ".into()),
        "tb" | "tab" => Sep::Literal("\t".into()),
        "da" | "dash" | "hy" | "hyphen" => Sep::Literal("-".into()),
        "sl" | "slash" => Sep::Literal("/".into()),
        "nl" | "new" | "newline" => Sep::Newline,
        "bl" | "blank" | "blankline" => Sep::BlankLine,
        other => Sep::Literal(other.into()),
    }
}

fn sep_display(sep: &Sep) -> String {
    match sep {
        Sep::Literal(s) => format!("{s:?}"),
        Sep::Newline => "newline".into(),
        Sep::BlankLine => "blankline".into(),
    }
}

fn sep_token(sep: &Sep) -> String {
    match sep {
        Sep::Literal(s) if s == "," => "comma".into(),
        Sep::Literal(s) if s == ":" => "colon".into(),
        Sep::Literal(s) if s == ";" => "semicolon".into(),
        Sep::Literal(s) if s == " " => "space".into(),
        Sep::Literal(s) if s == "\t" => "tab".into(),
        Sep::Literal(s) if s == "-" => "dash".into(),
        Sep::Literal(s) if s == "/" => "slash".into(),
        Sep::Literal(s) => s.clone(),
        Sep::Newline => "newline".into(),
        Sep::BlankLine => "blankline".into(),
    }
}

fn parse_sep_arg(arg: &str) -> Result<SeparatorConfig, String> {
    let parts: Vec<&str> = arg.split(',').collect();
    if parts.len() < 3 || parts.len() > 4 {
        return Err(format!(
            "--sep requires 3 or 4 comma-separated tokens (A,B,C or A,B,C,D), got {}",
            parts.len()
        ));
    }
    let config = SeparatorConfig {
        clue: resolve_token(parts[0]),
        line: resolve_token(parts[1]),
        section: resolve_token(parts[2]),
        puzzle: if parts.len() == 4 { resolve_token(parts[3]) } else { Sep::BlankLine },
    };
    validate_sep_uniqueness(&config)?;
    Ok(config)
}

fn validate_sep_uniqueness(c: &SeparatorConfig) -> Result<(), String> {
    let pairs = [
        (&c.clue, "clue", &c.line, "line"),
        (&c.clue, "clue", &c.section, "section"),
        (&c.clue, "clue", &c.puzzle, "puzzle"),
        (&c.line, "line", &c.section, "section"),
        (&c.line, "line", &c.puzzle, "puzzle"),
        (&c.section, "section", &c.puzzle, "puzzle"),
    ];
    for (a, a_name, b, b_name) in pairs {
        if a == b {
            return Err(format!(
                "{a_name}-sep and {b_name}-sep are both {} — all separators must be distinct",
                sep_display(a)
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Line-ending normalization
// ---------------------------------------------------------------------------

fn check_line_endings(content: &str) -> Result<(), String> {
    let has_crlf = content.contains("\r\n");
    let has_bare_lf = content.replace("\r\n", "").contains('\n');
    if has_crlf && has_bare_lf {
        return Err(
            "mixed line endings (CRLF and bare LF) — convert to a consistent format first \
             (e.g. dos2unix or unix2dos)"
                .into(),
        );
    }
    Ok(())
}

fn normalize_line_endings(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

// ---------------------------------------------------------------------------
// Letter-encoding decoder
// ---------------------------------------------------------------------------

/// Decode a single token like `"BA"` into `[2, 1]`.
fn decode_token(token: &str) -> Result<Vec<u32>, String> {
    token
        .chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                Ok(c as u32 - b'A' as u32 + 1)
            } else {
                Err(format!("unexpected character '{c}' (only A–Z allowed)"))
            }
        })
        .collect()
}

/// Decode a full clue line like `"C BA CB"` into `[[3], [2,1], [3,2]]`.
fn decode_letter_line(line: &str) -> Result<Vec<Vec<u32>>, String> {
    line.split_whitespace().map(decode_token).collect()
}

// ---------------------------------------------------------------------------
// Output formatting
// ---------------------------------------------------------------------------

/// `[[1,5],[3],[2,2]]` → `"1 5,3,2 2"`. An empty inner `Vec` becomes `"0"`.
fn clues_to_str(clues: &[Vec<u32>]) -> String {
    clues
        .iter()
        .map(|line| {
            if line.is_empty() {
                "0".to_string()
            } else {
                line.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(" ")
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn clue_sum(clues: &[Vec<u32>]) -> u32 {
    clues.iter().flat_map(|line| line.iter()).sum()
}

fn emit(name_prefix: &str, num: usize, col_clues: &[Vec<u32>], row_clues: &[Vec<u32>]) {
    let row_total = clue_sum(row_clues);
    let col_total = clue_sum(col_clues);
    if row_total != col_total {
        eprintln!(
            "Warning: puzzle {num} clue mismatch \
             (row total {row_total} ≠ col total {col_total}) — \
             input may be transposed or malformed."
        );
    }
    println!("{} {}|{}/{}", name_prefix, num, clues_to_str(col_clues), clues_to_str(row_clues));
}

// ---------------------------------------------------------------------------
// Splitting utilities
// ---------------------------------------------------------------------------

/// Split `content` into puzzle blocks.
fn split_puzzles(content: &str, sep: &Sep) -> Vec<String> {
    match sep {
        Sep::BlankLine => {
            let mut blocks: Vec<String> = Vec::new();
            let mut current: Vec<&str> = Vec::new();
            for line in content.lines() {
                if line.trim().is_empty() {
                    if !current.is_empty() {
                        blocks.push(current.join("\n"));
                        current.clear();
                    }
                } else {
                    current.push(line);
                }
            }
            if !current.is_empty() {
                blocks.push(current.join("\n"));
            }
            blocks
        }
        Sep::Newline => content.lines().map(str::to_owned).collect(),
        Sep::Literal(s) => content.split(s.as_str()).map(str::to_owned).collect(),
    }
}

/// Split a puzzle block into (first_section, second_section).
///
/// Keyword separators (multi-char or uppercase strings like "COLUMNS") are matched
/// as complete lines. Single-character separators are matched inline.
fn split_section(block: &str, sep: &Sep) -> Option<(String, String)> {
    match sep {
        Sep::Literal(s) => {
            // Try as a full-line keyword: surrounded by newlines (or at block boundaries).
            let mid = format!("\n{s}\n");
            let head = format!("{s}\n");
            let tail = format!("\n{s}");

            if let Some(pos) = block.find(&mid) {
                return Some((block[..pos].into(), block[pos + mid.len()..].into()));
            }
            if block.starts_with(&head) {
                return Some(("".into(), block[head.len()..].into()));
            }
            if block.ends_with(&tail) {
                return Some((block[..block.len() - tail.len()].into(), "".into()));
            }
            // Fall back to inline split (e.g. "-" in R puzzle-string format).
            block.split_once(s.as_str()).map(|(a, b)| (a.to_owned(), b.to_owned()))
        }
        Sep::Newline => block.split_once('\n').map(|(a, b)| (a.to_owned(), b.to_owned())),
        Sep::BlankLine => {
            let lines: Vec<&str> = block.lines().collect();
            let mut i = 0;
            while i < lines.len() {
                if lines[i].trim().is_empty() {
                    let rest = lines[i..].iter().position(|l| !l.trim().is_empty())
                        .map(|j| i + j)
                        .unwrap_or(lines.len());
                    return Some((lines[..i].join("\n"), lines[rest..].join("\n")));
                }
                i += 1;
            }
            None
        }
    }
}

/// Split a section into individual clue lines.
fn split_lines(section: &str, sep: &Sep) -> Vec<String> {
    let raw: Vec<&str> = match sep {
        Sep::Newline => section.lines().collect(),
        Sep::BlankLine => return split_puzzles(section, sep),
        Sep::Literal(s) => section.split(s.as_str()).collect(),
    };
    raw.into_iter().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect()
}

// ---------------------------------------------------------------------------
// Generic clue parser and converter
// ---------------------------------------------------------------------------

fn normalize_clue(runs: Vec<u32>) -> Vec<u32> {
    if runs == [0] { vec![] } else { runs }
}

fn parse_generic_clue(line: &str, sep: &Sep) -> Result<Vec<u32>, String> {
    let parts: Vec<&str> = match sep {
        Sep::Literal(s) => line.split(s.as_str()).collect(),
        Sep::Newline => line.lines().collect(),
        Sep::BlankLine => vec![line],
    };
    parts
        .into_iter()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u32>().map_err(|_| format!("invalid number '{s}'")))
        .collect::<Result<Vec<_>, _>>()
        .map(normalize_clue)
}

fn convert_generic(content: &str, name_prefix: &str, config: &SeparatorConfig, col_major: bool) {
    let mut puzzle_num = 1usize;

    'block: for block in split_puzzles(content, &config.puzzle) {
        let block = block.trim().to_owned();
        if block.is_empty() {
            continue;
        }

        let (first, second) = match split_section(&block, &config.section) {
            Some(pair) => pair,
            None => {
                eprintln!(
                    "Warning: puzzle {puzzle_num} has no {} section separator — skipping.",
                    sep_display(&config.section)
                );
                continue;
            }
        };

        let (row_section, col_section) = if col_major { (second, first) } else { (first, second) };

        let mut row_clues: Vec<Vec<u32>> = Vec::new();
        for (i, line) in split_lines(&row_section, &config.line).iter().enumerate() {
            match parse_generic_clue(line, &config.clue) {
                Ok(c) => row_clues.push(c),
                Err(e) => {
                    eprintln!("Warning: puzzle {puzzle_num} row {i}: {e} — skipping.");
                    continue 'block;
                }
            }
        }

        let mut col_clues: Vec<Vec<u32>> = Vec::new();
        for (i, line) in split_lines(&col_section, &config.line).iter().enumerate() {
            match parse_generic_clue(line, &config.clue) {
                Ok(c) => col_clues.push(c),
                Err(e) => {
                    eprintln!("Warning: puzzle {puzzle_num} col {i}: {e} — skipping.");
                    continue 'block;
                }
            }
        }

        emit(name_prefix, puzzle_num, &col_clues, &row_clues);
        puzzle_num += 1;
    }
}

// ---------------------------------------------------------------------------
// Detect mode
// ---------------------------------------------------------------------------

fn detect_format(content: &str, filename: &str) {
    let nonempty: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    if nonempty.is_empty() {
        eprintln!("Detect: '{filename}' appears to be empty.");
        return;
    }

    // Letter encoding: lines consist only of A–Z and spaces.
    if nonempty.iter().all(|l| l.chars().all(|c| c.is_ascii_uppercase() || c == ' ')) {
        eprintln!("Detected letter encoding (A=1 … Z=26) for '{filename}'.");
        eprintln!("  Use: --format letter");
        return;
    }

    // COLUMNS keyword → expanded format.
    if content.lines().any(|l| l.trim().eq_ignore_ascii_case("COLUMNS")) {
        let has_blank_lines = content.lines().any(|l| l.trim().is_empty());
        let d_tok = if has_blank_lines { "blankline" } else { "newline" };

        let content_lines: Vec<&str> = content
            .lines()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty() && !t.eq_ignore_ascii_case("COLUMNS")
            })
            .take(10)
            .collect();

        let (a_display, a_tok) = detect_clue_sep(&content_lines)
            .map(|s| (sep_display(&s), sep_token(&s)))
            .unwrap_or_else(|| ("unknown".into(), "?".into()));

        eprintln!("Detected separators for '{filename}':");
        eprintln!("  A (clue)   : {a_display}");
        eprintln!("  B (line)   : newline");
        eprintln!("  C (section): \"COLUMNS\" keyword");
        eprintln!("  D (puzzle) : {d_tok}");
        eprintln!();
        eprintln!("  Suggested: --sep {a_tok},newline,COLUMNS,{d_tok}");
        eprintln!("  Note: major order not deduced — add -r (rows first) or -c (cols first)");
        return;
    }

    // R puzzle-string format: each line has exactly one '-', chars are digits/,/:/-
    let sample = &nonempty[..nonempty.len().min(10)];
    if sample.iter().all(|l| looks_like_puzzle_string(l)) {
        eprintln!("Detected separators for '{filename}':");
        eprintln!("  A (clue)   : \",\" (comma)");
        eprintln!("  B (line)   : \":\" (colon)");
        eprintln!("  C (section): \"-\" (dash)");
        eprintln!("  D (puzzle) : newline");
        eprintln!();
        eprintln!("  Suggested: --sep comma,colon,dash,newline");
        eprintln!("  Note: major order not deduced — add -r (rows first) or -c (cols first)");
        return;
    }

    // Fallback: try to report at least the clue separator.
    let clue_hint = detect_clue_sep(sample)
        .map(|s| format!("{} (token: {})", sep_display(&s), sep_token(&s)))
        .unwrap_or_else(|| "unknown".into());

    eprintln!("Detect: could not identify a known format for '{filename}'.");
    eprintln!("  Clue separator hint: {clue_hint}");
    eprintln!("  Inspect the file and specify --sep A,B,C[,D] manually.");
}

fn looks_like_puzzle_string(line: &str) -> bool {
    line.chars().filter(|&c| c == '-').count() == 1
        && line.chars().all(|c| c.is_ascii_digit() || c == ',' || c == ':' || c == '-')
}

fn detect_clue_sep(lines: &[&str]) -> Option<Sep> {
    // Semicolon and comma: simple split, all parts must be numeric, at least one line multi-part.
    for ch in [";", ","] {
        let mut has_multi = false;
        let all_ok = lines.iter().all(|&line| {
            let parts: Vec<_> = line.split(ch).collect();
            if parts.len() >= 2 { has_multi = true; }
            parts.iter().all(|p| p.trim().parse::<u32>().is_ok())
        });
        if all_ok && has_multi {
            return Some(Sep::Literal(ch.to_owned()));
        }
    }

    // Space: use split_whitespace to handle multiple spaces.
    let mut has_multi = false;
    let all_ok = lines.iter().all(|&line| {
        let parts: Vec<_> = line.split_whitespace().collect();
        if parts.len() >= 2 { has_multi = true; }
        parts.iter().all(|p| p.parse::<u32>().is_ok())
    });
    if all_ok && has_multi {
        return Some(Sep::Literal(" ".to_owned()));
    }

    None
}

// ---------------------------------------------------------------------------
// Letter format conversion
// ---------------------------------------------------------------------------

fn convert_letter(content: &str, name_prefix: &str, col_major: bool) {
    let mut puzzle_num = 1usize;
    let mut pending: Vec<&str> = Vec::new();

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() {
            flush_letter(&pending, name_prefix, puzzle_num, &mut puzzle_num, col_major);
            pending.clear();
        } else {
            pending.push(line);
        }
    }
    flush_letter(&pending, name_prefix, puzzle_num, &mut puzzle_num, col_major);
}

fn flush_letter(
    lines: &[&str],
    name_prefix: &str,
    current_num: usize,
    next_num: &mut usize,
    col_major: bool,
) {
    if lines.is_empty() { return; }
    if lines.len() != 2 {
        eprintln!(
            "Warning: block {current_num} has {} line(s), expected 2 — skipping.",
            lines.len()
        );
        return;
    }

    let (first, second) = if col_major { (lines[1], lines[0]) } else { (lines[0], lines[1]) };

    let row_clues = match decode_letter_line(first) {
        Ok(c) => c,
        Err(e) => { eprintln!("Warning: block {current_num} row error: {e} — skipping."); return; }
    };
    let col_clues = match decode_letter_line(second) {
        Ok(c) => c,
        Err(e) => { eprintln!("Warning: block {current_num} col error: {e} — skipping."); return; }
    };

    emit(name_prefix, current_num, &col_clues, &row_clues);
    *next_num += 1;
}

// ---------------------------------------------------------------------------
// Columns-style format (semicolon / plain)
// ---------------------------------------------------------------------------

fn parse_semicolon_line(line: &str) -> Result<Vec<u32>, String> {
    line.split(';')
        .map(|s| s.trim().parse::<u32>().map_err(|_| format!("invalid number '{}'", s.trim())))
        .collect::<Result<Vec<_>, _>>()
        .map(normalize_clue)
}

fn parse_plain_line(line: &str) -> Result<Vec<u32>, String> {
    line.split_whitespace()
        .map(|s| s.parse::<u32>().map_err(|_| format!("invalid number '{s}'")))
        .collect::<Result<Vec<_>, _>>()
        .map(normalize_clue)
}

fn convert_columns_style(
    content: &str,
    name_prefix: &str,
    parse_clue_line: fn(&str) -> Result<Vec<u32>, String>,
    col_major: bool,
) {
    let mut puzzle_num = 1usize;
    let mut block: Vec<&str> = Vec::new();

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() {
            flush_columns(&block, name_prefix, puzzle_num, &mut puzzle_num, parse_clue_line, col_major);
            block.clear();
        } else {
            block.push(line);
        }
    }
    flush_columns(&block, name_prefix, puzzle_num, &mut puzzle_num, parse_clue_line, col_major);
}

fn flush_columns(
    lines: &[&str],
    name_prefix: &str,
    current_num: usize,
    next_num: &mut usize,
    parse_clue_line: fn(&str) -> Result<Vec<u32>, String>,
    col_major: bool,
) {
    if lines.is_empty() { return; }

    let Some(sep_idx) = lines.iter().position(|l| l.eq_ignore_ascii_case("COLUMNS")) else {
        eprintln!("Warning: block {current_num} has no COLUMNS separator — skipping.");
        return;
    };

    let first_section = &lines[..sep_idx];
    let second_section = &lines[sep_idx + 1..];
    let (row_lines, col_lines) = if col_major {
        (second_section, first_section)
    } else {
        (first_section, second_section)
    };

    let mut row_clues: Vec<Vec<u32>> = Vec::new();
    for (i, &line) in row_lines.iter().enumerate() {
        match parse_clue_line(line) {
            Ok(c) => row_clues.push(c),
            Err(e) => { eprintln!("Warning: block {current_num} row {i}: {e} — skipping."); return; }
        }
    }

    let mut col_clues: Vec<Vec<u32>> = Vec::new();
    for (i, &line) in col_lines.iter().enumerate() {
        match parse_clue_line(line) {
            Ok(c) => col_clues.push(c),
            Err(e) => { eprintln!("Warning: block {current_num} col {i}: {e} — skipping."); return; }
        }
    }

    emit(name_prefix, current_num, &col_clues, &row_clues);
    *next_num += 1;
}

fn convert_semicolon(content: &str, name_prefix: &str, col_major: bool) {
    convert_columns_style(content, name_prefix, parse_semicolon_line, col_major);
}

fn convert_plain(content: &str, name_prefix: &str, col_major: bool) {
    convert_columns_style(content, name_prefix, parse_plain_line, col_major);
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let cli = Cli::parse();

    let raw = match fs::read_to_string(&cli.input) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: could not read '{}': {e}", cli.input);
            process::exit(1);
        }
    };

    if let Err(e) = check_line_endings(&raw) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
    let content = normalize_line_endings(&raw);
    let col_major = cli.col;

    if cli.detect {
        let filename = Path::new(&cli.input)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&cli.input);
        detect_format(&content, filename);
        return;
    }

    if let Some(sep_arg) = &cli.sep {
        let config = match parse_sep_arg(sep_arg) {
            Ok(c) => c,
            Err(e) => { eprintln!("Error: {e}"); process::exit(1); }
        };
        convert_generic(&content, &cli.name, &config, col_major);
        return;
    }

    match cli.format {
        InputFormat::Letter => convert_letter(&content, &cli.name, col_major),
        InputFormat::Semicolon => convert_semicolon(&content, &cli.name, col_major),
        InputFormat::Plain => convert_plain(&content, &cli.name, col_major),
    }
}
