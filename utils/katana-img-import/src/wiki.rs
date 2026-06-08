use serde_json::Value;

#[derive(Debug)]
pub struct PuzzleMeta {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub wiki_filename: String,
    pub category: String,
    pub img_url: String,
}

pub fn fetch_puzzles(page_title: &str) -> Vec<PuzzleMeta> {
    let wikitext = fetch_wikitext(page_title);
    let mut metas = parse_wikitext(&wikitext);
    fetch_image_urls(&mut metas);
    metas.retain(|m| m.width > 0 && m.height > 0);
    metas
}

fn fetch_wikitext(page_title: &str) -> String {
    let encoded = url_encode(&page_title.replace(' ', "_"));
    let url = format!(
        "https://nonograms-katana.fandom.com/api.php?action=query&titles={encoded}&prop=revisions&rvprop=content&format=json"
    );
    let json: Value = ureq::get(&url).call()
        .expect("wiki API request failed")
        .into_json()
        .expect("wiki API JSON parse failed");
    json["query"]["pages"]
        .as_object()
        .and_then(|p| p.values().next())
        .and_then(|p| p["revisions"][0]["*"].as_str())
        .unwrap_or("")
        .to_string()
}

fn parse_wikitext(text: &str) -> Vec<PuzzleMeta> {
    let mut result = Vec::new();
    let mut section_dims: Option<(u32, u32)> = None;
    let mut category = String::new();

    for line in text.lines() {
        let line = line.trim();

        if line.starts_with("==") && line.ends_with("==") {
            let sec = line.trim_matches('=').trim();
            section_dims = parse_nxm(sec);
            category = sec.replace(" - ", "-").to_string();
            continue;
        }

        if !line.contains(".png|") {
            continue;
        }

        let Some((filename, name, label_dims)) = parse_gallery_line(line) else {
            continue;
        };

        let (width, height) = label_dims.or(section_dims).unwrap_or((0, 0));

        result.push(PuzzleMeta {
            name,
            width,
            height,
            wiki_filename: filename,
            category: category.clone(),
            img_url: String::new(),
        });
    }
    result
}

fn parse_gallery_line(line: &str) -> Option<(String, String, Option<(u32, u32)>)> {
    let pipe = line.find(".png|")?;
    let filename = line[..pipe + 4].to_string();
    let label = &line[pipe + 5..];

    // Try to extract (WxH) or (WxH, method) suffix
    let (label_main, dims) = match label.rfind(" (") {
        Some(pos) => {
            let inner = label[pos + 2..].trim_end_matches(')');
            let dim_str = inner.split(',').next().unwrap_or("").trim();
            match parse_nxm(dim_str) {
                Some(d) => (&label[..pos], Some(d)),
                None => (label, None),
            }
        }
        None => (label, None),
    };

    let name = label_main.split(',').last()?.trim().to_string();
    Some((filename, name, dims))
}

fn parse_nxm(s: &str) -> Option<(u32, u32)> {
    let x = s.find('x')?;
    let w = s[..x].trim().parse().ok()?;
    let h = s[x + 1..].trim().parse().ok()?;
    Some((w, h))
}

fn fetch_image_urls(metas: &mut Vec<PuzzleMeta>) {
    for chunk in metas.chunks_mut(50) {
        let titles: String = chunk.iter()
            .map(|m| format!("File:{}", m.wiki_filename))
            .collect::<Vec<_>>()
            .join("|");

        let url = format!(
            "https://nonograms-katana.fandom.com/api.php?action=query&titles={}&prop=imageinfo&iiprop=url&format=json",
            url_encode(&titles)
        );

        let json: Value = match ureq::get(&url).call().and_then(|r| r.into_json().map_err(|e| e.into())) {
            Ok(j) => j,
            Err(e) => { eprintln!("imageinfo request failed: {e}"); continue; }
        };

        let Some(pages) = json["query"]["pages"].as_object() else { continue };

        for page in pages.values() {
            let Some(url) = page["imageinfo"][0]["url"].as_str() else { continue };
            let title = page["title"].as_str().unwrap_or("").trim_start_matches("File:");
            let title_norm = title.replace(' ', "_");
            if let Some(meta) = chunk.iter_mut().find(|m| m.wiki_filename.replace(' ', "_") == title_norm) {
                meta.img_url = url.to_string();
            }
        }
    }
}

pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
