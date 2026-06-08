mod img;
mod puzzle;
mod wiki;

use std::{fs, io::{Read, Write, BufWriter}, path::Path};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("download") => {
            let page = args.get(2).expect("usage: download <page_title> <output_dir>");
            let out = args.get(3).expect("usage: download <page_title> <output_dir>");
            cmd_download(page, out);
        }
        Some("parse") => {
            let path = args.get(2).expect("usage: parse <image> <width> <height> [name] [debug_out]");
            let w: u32 = args.get(3).expect("missing width").parse().expect("width must be integer");
            let h: u32 = args.get(4).expect("missing height").parse().expect("height must be integer");
            let name = args.get(5).cloned().unwrap_or_else(|| "puzzle".to_string());
            let debug_out = args.get(6).cloned();
            cmd_parse(path, w, h, &name, debug_out.as_deref());
        }
        Some("parse-all") => {
            let manifest = args.get(2).expect("usage: parse-all <manifest_tsv> <images_dir> <output_file>");
            let images_dir = args.get(3).expect("usage: parse-all <manifest_tsv> <images_dir> <output_file>");
            let out_file = args.get(4).expect("usage: parse-all <manifest_tsv> <images_dir> <output_file>");
            cmd_parse_all(manifest, images_dir, out_file);
        }
_ => {
            eprintln!("usage:");
            eprintln!("  katana-img-import download <page_title> <output_dir>");
            eprintln!("  katana-img-import parse <image_path> <width> <height> [name] [debug_out.png]");
            eprintln!("  katana-img-import parse-all <manifest_tsv> <images_dir> <output_file>");
        }
    }
}

fn cmd_download(page_title: &str, out_dir: &str) {
    println!("Fetching puzzle list...");
    let metas = wiki::fetch_puzzles(page_title);
    println!("Found {} puzzles with known dimensions", metas.len());

    for meta in &metas {
        if meta.img_url.is_empty() {
            eprintln!("  skip {} (no image URL)", meta.name);
            continue;
        }

        let dir = Path::new(out_dir).join(sanitize(&meta.category));
        fs::create_dir_all(&dir).unwrap();

        let safe = sanitize(&meta.name);
        let dest = dir.join(format!("{safe}.png"));

        if dest.exists() {
            println!("  exists: {}", dest.display());
            continue;
        }

        print!("  {} ({}) {}x{}... ", meta.name, meta.category, meta.width, meta.height);
        match download(&meta.img_url, &dest) {
            Ok(_) => println!("ok"),
            Err(e) => eprintln!("FAILED: {e}"),
        }
    }

    let manifest: String = metas.iter()
        .filter(|m| !m.img_url.is_empty())
        .map(|m| format!("{}\t{}\t{}\t{}\t{}", sanitize(&m.category), sanitize(&m.name), m.width, m.height, m.name))
        .collect::<Vec<_>>()
        .join("\n");
    let manifest_path = Path::new(out_dir).join("manifest.tsv");
    fs::write(&manifest_path, manifest).unwrap();
    println!("Manifest: {}", manifest_path.display());
}

fn cmd_parse_all(manifest_path: &str, images_dir: &str, out_path: &str) {
    let manifest = fs::read_to_string(manifest_path)
        .unwrap_or_else(|e| panic!("cannot read manifest {manifest_path}: {e}"));

    let out_file = fs::File::create(out_path)
        .unwrap_or_else(|e| panic!("cannot create output {out_path}: {e}"));
    let mut out = BufWriter::new(out_file);

    let mut ok = 0u32;
    let mut fail = 0u32;

    for line in manifest.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        let (Some(&cat), Some(&filename), Some(w_str), Some(h_str)) =
            (parts.get(0), parts.get(1), parts.get(2), parts.get(3))
        else {
            eprintln!("skip malformed line: {line}");
            fail += 1;
            continue;
        };
        let (Ok(w), Ok(h)) = (w_str.parse::<u32>(), h_str.parse::<u32>()) else {
            eprintln!("skip bad dimensions in: {line}");
            fail += 1;
            continue;
        };
        // Column 5 (added later): original unsanitized name. Fall back to filename if absent.
        let name = parts.get(4).copied().unwrap_or(filename);

        let img_path = Path::new(images_dir).join(cat).join(format!("{filename}.png"));

        let img = match image::ImageReader::open(&img_path)
            .ok()
            .and_then(|r| r.with_guessed_format().ok())
            .and_then(|r| r.decode().ok())
        {
            Some(img) => img.to_rgb8(),
            None => {
                eprintln!("FAIL {filename}: cannot open/decode {}", img_path.display());
                fail += 1;
                continue;
            }
        };

        let (x0, y0, x1, y1) = img::detect_grid_bounds(&img);
        if x1 <= x0 || y1 <= y0 {
            eprintln!("FAIL {filename}: bad bounds ({x0},{y0})–({x1},{y1}) image {}×{}", img.width(), img.height());
            fail += 1;
            continue;
        }
        let grid = img::sample_grid(&img, x0, y0, x1, y1, w, h);
        let puzzle = puzzle::format_puzzle(name, &grid);
        writeln!(out, "{puzzle}").unwrap();
        ok += 1;
    }

    eprintln!("done: {ok} ok, {fail} failed → {out_path}");
}

fn cmd_parse(img_path: &str, w: u32, h: u32, name: &str, debug_out: Option<&str>) {
    let img = image::ImageReader::open(img_path)
        .expect("failed to open image")
        .with_guessed_format()
        .expect("failed to guess image format")
        .decode()
        .expect("failed to decode image")
        .to_rgb8();

    let (x0, y0, x1, y1) = img::detect_grid_bounds(&img);
    eprintln!(
        "grid bounds: ({x0},{y0})–({x1},{y1})  cell: {}×{}  image: {}×{}",
        (x1 - x0) / w,
        (y1 - y0) / h,
        img.width(),
        img.height()
    );

    let grid = img::sample_grid(&img, x0, y0, x1, y1, w, h);

    eprintln!("grid (# = filled, . = empty):");
    for row in &grid {
        let line: String = row.iter().map(|&f| if f { '#' } else { '.' }).collect();
        eprintln!("  {line}");
    }

    if let Some(out_path) = debug_out {
        let overlay = img::draw_overlay(&img, x0, y0, x1, y1, w, h, &grid);
        overlay.save(out_path).expect("failed to save debug image");
        eprintln!("debug overlay saved to {out_path}");
    }

    println!("{}", puzzle::format_puzzle(name, &grid));
}

fn download(url: &str, dest: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let resp = ureq::get(url).call()?;
    let mut data = Vec::new();
    resp.into_reader().read_to_end(&mut data)?;
    fs::write(dest, data)?;
    Ok(())
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect()
}
