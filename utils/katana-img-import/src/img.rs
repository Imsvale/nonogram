use image::{Rgb, RgbImage};

/// Detect all four boundaries of the solution grid.
/// Returns (x0, y0, x1, y1) — the pixel rectangle of the grid (exclusive end).
pub fn detect_grid_bounds(img: &RgbImage) -> (u32, u32, u32, u32) {
    let (w, h) = img.dimensions();

    // y_start: scan down at several x positions across the right half of the
    // image and take the deepest tan row found. Multiple positions guard
    // against any single scan landing on a vertical cell divider.
    let y_start = (1..=7u32)
        .map(|k| w / 2 + (w / 2) * k / 8)
        .filter(|&scan_x| scan_x < w)
        .map(|scan_x| last_tan_row(img, scan_x, 0, h))
        .max()
        .unwrap_or(0);

    // x_start: for each of 3 bands in the grid height, sample 7 y positions
    // spaced 3px apart. Taking the maximum across all successful scans guards
    // against any single scan line landing on a (less-warm) grid divider.
    let grid_h_approx = h - y_start;
    let x_start = [
        y_start + grid_h_approx / 4,
        y_start + grid_h_approx / 2,
        y_start + grid_h_approx * 3 / 4,
    ]
    .iter()
    .flat_map(|&center_y| {
        (0..7u32).filter_map(move |k| {
            let scan_y = center_y.saturating_sub(9).saturating_add(k * 3);
            if scan_y >= h { return None; }
            let last = (0..w)
                .filter(|&x| is_tan(img.get_pixel(x, scan_y)))
                .last()?;
            Some(last + 1)
        })
    })
    .max()
    .unwrap_or(0);

    // y_end: sample 5 x positions across the row-clue area and take the
    // deepest tan row found. Scanning from y_start avoids the minimap region.
    let y_end = (1..=5u32)
        .filter_map(|k| {
            let scan_x = (x_start * k / 6).max(1);
            if scan_x >= w { return None; }
            let last_y = last_tan_row(img, scan_x, y_start, h);
            if last_y > y_start { Some(last_y + 1) } else { None }
        })
        .max()
        .unwrap_or(h);

    // x_end: scan the column-clue area (y = y_start/2) from right.
    let scan_y_clue = (y_start / 2).max(1);
    let x_end = (0..w)
        .filter(|&x| is_tan(img.get_pixel(x, scan_y_clue)))
        .last()
        .map_or(w, |x| x + 1);

    (x_start, y_start, x_end, y_end)
}

/// Sample each cell in the solution grid and return true for filled cells.
/// Returns an empty vec if the bounds are degenerate.
pub fn sample_grid(
    img: &RgbImage,
    x0: u32, y0: u32,
    x1: u32, y1: u32,
    cols: u32, rows: u32,
) -> Vec<Vec<bool>> {
    if x1 <= x0 || y1 <= y0 || cols == 0 || rows == 0 {
        return vec![];
    }
    // Use float arithmetic so rounding error doesn't accumulate across rows/cols.
    let cw = (x1 - x0) as f32 / cols as f32;
    let ch = (y1 - y0) as f32 / rows as f32;
    let (iw, ih) = img.dimensions();

    (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| {
                    let cx = (x0 as f32 + (c as f32 + 0.5) * cw).round() as u32;
                    let cy = (y0 as f32 + (r as f32 + 0.5) * ch).round() as u32;
                    let cx = cx.min(iw - 1);
                    let cy = cy.min(ih - 1);
                    brightness(img.get_pixel(cx, cy)) < 128
                })
                .collect()
        })
        .collect()
}

/// Return a copy of the image with the detected grid overlaid:
///   - green rectangle for the outer grid boundary
///   - orange lines for each cell boundary
///   - red dot at each filled cell center, cyan at each empty cell center
pub fn draw_overlay(
    img: &RgbImage,
    x0: u32, y0: u32,
    x1: u32, y1: u32,
    cols: u32, rows: u32,
    grid: &[Vec<bool>],
) -> RgbImage {
    let mut out = img.clone();
    let (iw, ih) = img.dimensions();
    let cw_f = (x1 - x0) as f32 / cols as f32;
    let ch_f = (y1 - y0) as f32 / rows as f32;

    let orange = Rgb([255u8, 140, 0]);
    let green  = Rgb([0u8, 220, 0]);
    let red    = Rgb([255u8, 0, 0]);
    let cyan   = Rgb([0u8, 220, 220]);

    // Cell dividers (orange)
    for c in 0..=cols {
        let x = (x0 as f32 + c as f32 * cw_f).round() as u32;
        if x < iw {
            for y in y0..y1.min(ih) {
                out.put_pixel(x, y, orange);
            }
        }
    }
    for r in 0..=rows {
        let y = (y0 as f32 + r as f32 * ch_f).round() as u32;
        if y < ih {
            for x in x0..x1.min(iw) {
                out.put_pixel(x, y, orange);
            }
        }
    }

    // Outer grid boundary (green, 2px thick)
    for t in 0..2u32 {
        let xl = x0.saturating_sub(t);
        let xr = (x1 + t).min(iw - 1);
        let yt = y0.saturating_sub(t);
        let yb = (y1 + t).min(ih - 1);
        for x in xl..=xr {
            out.put_pixel(x, yt, green);
            out.put_pixel(x, yb, green);
        }
        for y in yt..=yb {
            out.put_pixel(xl, y, green);
            out.put_pixel(xr, y, green);
        }
    }

    // Cell center dots (5×5 cross) — positions match sample_grid exactly.
    for r in 0..rows {
        for c in 0..cols {
            let cx = (x0 as f32 + (c as f32 + 0.5) * cw_f).round() as u32;
            let cy = (y0 as f32 + (r as f32 + 0.5) * ch_f).round() as u32;
            let cx = cx.min(iw - 1);
            let cy = cy.min(ih - 1);
            let color = if grid[r as usize][c as usize] { red } else { cyan };
            for d in 0..5u32 {
                let px = (cx as i32 - 2 + d as i32).clamp(0, iw as i32 - 1) as u32;
                let py = (cy as i32 - 2 + d as i32).clamp(0, ih as i32 - 1) as u32;
                out.put_pixel(px, cy, color);
                out.put_pixel(cx, py, color);
            }
        }
    }

    out
}

fn last_tan_row(img: &RgbImage, scan_x: u32, y_from: u32, y_to: u32) -> u32 {
    (y_from..y_to)
        .filter(|&y| is_tan(img.get_pixel(scan_x, y)))
        .last()
        .unwrap_or(y_from)
}

// Tan clue-area background: warm-tinted, not dark, more red than blue.
// R > 100: handles muted tan (R≈130, Golden Mean series) while excluding
//          dark content cells (R≈23) and dark borders.
// r-b > 20: handles light tan (r-b≈29, some 20x20 images) while excluding
//           pure white grid cells (r-b≈0-4).
fn is_tan(px: &Rgb<u8>) -> bool {
    let r = px[0] as i32;
    let b = px[2] as i32;
    r > 100 && (r - b) > 20
}

fn brightness(px: &Rgb<u8>) -> u32 {
    (px[0] as u32 + px[1] as u32 + px[2] as u32) / 3
}
