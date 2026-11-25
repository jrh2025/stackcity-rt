//! 純 Rust 四視圖視覺外殼雕刻器（Visual-Hull Carving）。
//!
//! 支援透明 PNG 與單色平坦背景之 1×4 或 2×2 四視圖設計稿（TOP, SIDE, FRONT, BACK），
//! 自動執行去背、視圖切割、去除 1px 暗邊、體素交集雕刻與選用圓柱車床修圓。

use super::{BakedStack, RgbaPx, VoxelCanvas};
use image::{imageops::FilterType, DynamicImage, GenericImageView, Rgba, RgbaImage};
use std::collections::VecDeque;

/// 去背處理：若原圖已有顯著透明區則保留，否則由四個邊角進行 Flood-Fill 去背
fn remove_background(img: &RgbaImage, tol: i16) -> RgbaImage {
    let (w, h) = img.dimensions();
    let total = (w * h).max(1) as f32;
    let trans_cnt = img.pixels().filter(|p| p.0[3] < 128).count() as f32;

    if trans_cnt / total > 0.05 {
        let mut out = img.clone();
        for p in out.pixels_mut() {
            p.0[3] = if p.0[3] >= 128 { 255 } else { 0 };
        }
        return out;
    }

    // 取四角中位數作為背景參考色
    let corners = [
        img.get_pixel(0, 0).0,
        img.get_pixel(w.saturating_sub(1), 0).0,
        img.get_pixel(0, h.saturating_sub(1)).0,
        img.get_pixel(w.saturating_sub(1), h.saturating_sub(1)).0,
    ];
    let mut bg = [0i16; 3];
    for ch in 0..3 {
        let mut vals = [
            corners[0][ch] as i16,
            corners[1][ch] as i16,
            corners[2][ch] as i16,
            corners[3][ch] as i16,
        ];
        vals.sort_unstable();
        bg[ch] = (vals[1] + vals[2]) / 2;
    }

    let is_close = |x: u32, y: u32| -> bool {
        let p = img.get_pixel(x, y).0;
        let dr = (p[0] as i16 - bg[0]).abs();
        let dg = (p[1] as i16 - bg[1]).abs();
        let db = (p[2] as i16 - bg[2]).abs();
        dr.max(dg).max(db) <= tol
    };

    let mut bg_mask = vec![false; (w * h) as usize];
    let mut q = VecDeque::new();
    for x in 0..w {
        for y in [0, h.saturating_sub(1)] {
            let idx = (y * w + x) as usize;
            if !bg_mask[idx] && is_close(x, y) {
                bg_mask[idx] = true;
                q.push_back((x, y));
            }
        }
    }
    for y in 1..h.saturating_sub(1) {
        for x in [0, w.saturating_sub(1)] {
            let idx = (y * w + x) as usize;
            if !bg_mask[idx] && is_close(x, y) {
                bg_mask[idx] = true;
                q.push_back((x, y));
            }
        }
    }

    while let Some((x, y)) = q.pop_front() {
        let neighbors = [
            (x + 1, y),
            (x.wrapping_sub(1), y),
            (x, y + 1),
            (x, y.wrapping_sub(1)),
        ];
        for (nx, ny) in neighbors {
            if nx < w && ny < h {
                let idx = (ny * w + nx) as usize;
                if !bg_mask[idx] && is_close(nx, ny) {
                    bg_mask[idx] = true;
                    q.push_back((nx, ny));
                }
            }
        }
    }

    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            out.get_pixel_mut(x, y).0[3] = if bg_mask[idx] { 0 } else { 255 };
        }
    }
    out
}

#[derive(Clone, Copy)]
struct BBox {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

/// 定位四個正交視圖區域（自動過濾微小雜點並依閱讀順序排序）
fn find_four_views(sheet: &RgbaImage, order_reading: bool) -> Result<[RgbaImage; 4], String> {
    let (w, h) = sheet.dimensions();
    let len = (w * h) as usize;
    let mut grown = vec![false; len];
    for y in 0..h {
        for x in 0..w {
            grown[(y * w + x) as usize] = sheet.get_pixel(x, y).0[3] >= 128;
        }
    }
    let steps = (w.min(h) as f32 / 260.0).round().clamp(2.0, 6.0) as usize;
    for _ in 0..steps {
        let prev = grown.clone();
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) as usize;
                if prev[idx] {
                    continue;
                }
                let hit = (y > 0 && prev[((y - 1) * w + x) as usize])
                    || (y + 1 < h && prev[((y + 1) * w + x) as usize])
                    || (x > 0 && prev[(y * w + x - 1) as usize])
                    || (x + 1 < w && prev[(y * w + x + 1) as usize]);
                if hit {
                    grown[idx] = true;
                }
            }
        }
    }

    let mut seen = vec![false; len];
    let mut boxes: Vec<BBox> = Vec::new();
    for y0 in 0..h {
        for x0 in 0..w {
            let idx0 = (y0 * w + x0) as usize;
            if !grown[idx0] || seen[idx0] {
                continue;
            }
            let mut stack = vec![(x0, y0)];
            seen[idx0] = true;
            let (mut min_x, mut max_x) = (x0, x0);
            let (mut min_y, mut max_y) = (y0, y0);
            let mut cnt = 0usize;
            while let Some((x, y)) = stack.pop() {
                cnt += 1;
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
                for (nx, ny) in [
                    (x + 1, y),
                    (x.wrapping_sub(1), y),
                    (x, y + 1),
                    (x, y.wrapping_sub(1)),
                ] {
                    if nx < w && ny < h {
                        let ni = (ny * w + nx) as usize;
                        if grown[ni] && !seen[ni] {
                            seen[ni] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
            }
            if cnt > 40 {
                boxes.push(BBox {
                    x0: min_x,
                    y0: min_y,
                    x1: max_x + 1,
                    y1: max_y + 1,
                });
            }
        }
    }

    if boxes.is_empty() {
        return Err("圖中未偵測到任何前景輪廓，請確認圖片包含去背或單色純背景。".to_string());
    }
    let big = boxes
        .iter()
        .map(|b| (b.x1 - b.x0) * (b.y1 - b.y0))
        .max()
        .unwrap_or(1) as f32;
    boxes.retain(|b| ((b.x1 - b.x0) * (b.y1 - b.y0)) as f32 >= 0.14 * big);

    let row_band = (h as f32 / 2.2).max(1.0);
    boxes.sort_by(|a, b| {
        let ra = ((a.y0 + a.y1) as f32 * 0.5 / row_band).round() as i32;
        let rb = ((b.y0 + b.y1) as f32 * 0.5 / row_band).round() as i32;
        ra.cmp(&rb).then(a.x0.cmp(&b.x0))
    });

    if boxes.len() != 4 {
        return Err(format!(
            "偵測到 {} 個視圖區塊，需要剛好 4 個正交視圖（TOP、SIDE、FRONT、BACK）。",
            boxes.len()
        ));
    }

    let mut views: Vec<RgbaImage> = Vec::with_capacity(4);
    for b in boxes {
        let sub = sheet.view(b.x0, b.y0, b.x1 - b.x0, b.y1 - b.y0).to_image();
        let (sw, sh) = sub.dimensions();
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (sw, 0, sh, 0);
        for y in 0..sh {
            for x in 0..sw {
                if sub.get_pixel(x, y).0[3] >= 128 {
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            }
        }
        let cw = max_x.saturating_sub(min_x) + 1;
        let ch = max_y.saturating_sub(min_y) + 1;
        let mut cropped = RgbaImage::new(cw, ch);
        for y in 0..ch {
            for x in 0..cw {
                let mut px = sub.get_pixel(min_x + x, min_y + y).0;
                px[3] = if px[3] >= 128 { 255 } else { 0 };
                cropped.put_pixel(x, y, Rgba(px));
            }
        }
        views.push(cropped);
    }

    if order_reading {
        return Ok([
            views[0].clone(),
            views[1].clone(),
            views[2].clone(),
            views[3].clone(),
        ]);
    }

    // 依長寬比誤差配對出最適 (TOP, SIDE)
    let err = |t: usize, s: usize| -> f32 {
        let tv = &views[t];
        let sv = &views[s];
        let mut e = (tv.height() as f32 / sv.width().max(1) as f32).ln().abs();
        let mut rest_sum = 0.0f32;
        for (i, fv) in views.iter().enumerate() {
            if i != t && i != s {
                rest_sum += (tv.width() as f32 / fv.width().max(1) as f32).ln().abs()
                    + (sv.height() as f32 / fv.height().max(1) as f32).ln().abs();
            }
        }
        e += rest_sum * 0.5;
        e
    };
    let mut best = f32::MAX;
    let mut best_pair = (0usize, 1usize);
    for t in 0..4 {
        for s in 0..4 {
            if t == s {
                continue;
            }
            let e = err(t, s);
            if e < best {
                best = e;
                best_pair = (t, s);
            }
        }
    }
    let (mut chosen_t, mut chosen_s) = best_pair;
    'outer: for t in 0..4 {
        for s in 0..4 {
            if t != s && err(t, s) <= best + 0.1 {
                chosen_t = t;
                chosen_s = s;
                break 'outer;
            }
        }
    }
    let mut rest = Vec::with_capacity(2);
    for i in 0..4 {
        if i != chosen_t && i != chosen_s {
            rest.push(views[i].clone());
        }
    }
    Ok([
        views[chosen_t].clone(),
        views[chosen_s].clone(),
        rest[0].clone(),
        rest[1].clone(),
    ])
}

/// 去除 1px 暗色外框線條，替換成相鄰內部顏色
fn strip_outline(v: &RgbaImage, dark_thresh: i32) -> RgbaImage {
    if dark_thresh <= 0 {
        return v.clone();
    }
    let (w, h) = v.dimensions();
    let solid = |x: i32, y: i32| -> bool {
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            return false;
        }
        v.get_pixel(x as u32, y as u32).0[3] > 0
    };
    let mut is_line = vec![false; (w * h) as usize];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if !solid(x, y) {
                continue;
            }
            let mut all_neighbors = true;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if (dx != 0 || dy != 0) && !solid(x + dx, y + dy) {
                        all_neighbors = false;
                    }
                }
            }
            if !all_neighbors {
                let p = v.get_pixel(x as u32, y as u32).0;
                let avg = (p[0] as i32 + p[1] as i32 + p[2] as i32) / 3;
                if avg < dark_thresh {
                    is_line[(y as u32 * w + x as u32) as usize] = true;
                }
            }
        }
    }
    let mut out = v.clone();
    let dirs = [
        (0, 1),
        (0, -1),
        (1, 0),
        (-1, 0),
        (1, 1),
        (-1, 1),
        (1, -1),
        (-1, -1),
    ];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if !is_line[(y as u32 * w + x as u32) as usize] {
                continue;
            }
            for (dx, dy) in dirs {
                let nx = x + dx;
                let ny = y + dy;
                if solid(nx, ny) && !is_line[(ny as u32 * w + nx as u32) as usize] {
                    out.put_pixel(x as u32, y as u32, *v.get_pixel(nx as u32, ny as u32));
                    break;
                }
            }
        }
    }
    out
}

fn flip_h(v: &RgbaImage) -> RgbaImage {
    DynamicImage::ImageRgba8(v.clone()).fliph().to_rgba8()
}

fn flip_v(v: &RgbaImage) -> RgbaImage {
    DynamicImage::ImageRgba8(v.clone()).flipv().to_rgba8()
}

fn resize_nearest(v: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    image::imageops::resize(v, w.max(1), h.max(1), FilterType::Nearest)
}

/// 推斷 TOP 視圖車頭／正面朝向是上還是下
fn match_top_orientation(top: &RgbaImage, side: &RgbaImage) -> &'static str {
    let th = top.height() as usize;
    let sw = side.width() as usize;
    let sh = side.height() as usize;
    let mut tall_side = vec![0.0f32; sw];
    for x in 0..sw {
        tall_side[x] = (0..sh).filter(|&y| side.get_pixel(x as u32, y as u32).0[3] > 0).count() as f32;
    }
    let max_t = tall_side.iter().copied().fold(0.0f32, f32::max).max(1.0);
    let mut score_down = 0.0f32;
    let mut score_up = 0.0f32;
    for r in 0..th {
        let sx = ((r as f32 / th.saturating_sub(1).max(1) as f32) * sw.saturating_sub(1) as f32)
            .round() as usize;
        let is_tall = if tall_side[sx.min(sw - 1)] >= 0.8 * max_t {
            1.0
        } else {
            0.0
        };
        let mut dark_cnt = 0usize;
        for x in 0..top.width() {
            let p = top.get_pixel(x, r as u32).0;
            if p[3] > 0 && (p[0] as u16 + p[1] as u16 + p[2] as u16) < 240 {
                dark_cnt += 1;
            }
        }
        let dark = dark_cnt as f32 / top.width().max(1) as f32;
        score_down += dark * is_tall;
        let sx_rev = (((th - 1 - r) as f32 / th.saturating_sub(1).max(1) as f32)
            * sw.saturating_sub(1) as f32)
            .round() as usize;
        let is_tall_rev = if tall_side[sx_rev.min(sw - 1)] >= 0.8 * max_t {
            1.0
        } else {
            0.0
        };
        score_up += dark * is_tall_rev;
    }
    if score_down >= score_up {
        "down"
    } else {
        "up"
    }
}

fn edge_row_rgb(view: &RgbaImage, row: u32, target_len: usize) -> Option<Vec<[u8; 3]>> {
    let w = view.width();
    let mut first = None;
    let mut last = None;
    for x in 0..w {
        if view.get_pixel(x, row).0[3] > 0 {
            if first.is_none() {
                first = Some(x);
            }
            last = Some(x);
        }
    }
    let (fx, lx) = (first?, last?);
    let mut out = Vec::with_capacity(target_len);
    for i in 0..target_len {
        let x = (i as u32).clamp(fx, lx);
        let p = view.get_pixel(x, row).0;
        out.push([p[0], p[1], p[2]]);
    }
    Some(out)
}

/// 執行 4 視圖 3D 雕刻，並直接產出可存入系統的 `BakedStack`
#[allow(clippy::too_many_arguments)]
pub fn carve_four_views(
    png_bytes: &[u8],
    title: impl Into<String>,
    category: impl Into<String>,
    slices: Option<usize>,
    round_lathe: bool,
    side_nose_right: bool,
    order_reading: bool,
    top_nose: &str,
    default_scale: f32,
) -> Result<BakedStack, String> {
    let raw = image::load_from_memory(png_bytes)
        .map_err(|e| format!("無法讀取圖片：{e}"))?
        .to_rgba8();
    let clean = remove_background(&raw, 28);
    let [mut top, side, mut front, back] = find_four_views(&clean, order_reading)?;

    let mut side_aligned = if side_nose_right {
        side.clone()
    } else {
        flip_h(&side)
    };

    let raw_h = side.height() as usize;
    let target_h = match slices {
        None => {
            if raw_h > 64 {
                36
            } else {
                raw_h
            }
        }
        Some(0) => raw_h,
        Some(n) => n.clamp(8, 80),
    };

    let (w, l, h) = if target_h != raw_h {
        let l = ((target_h as f32 * side.width() as f32 / side.height().max(1) as f32).round()
            as usize)
            .max(4);
        let w = ((l as f32 * top.width() as f32 / top.height().max(1) as f32).round() as usize)
            .max(4);
        top = resize_nearest(&top, w as u32, l as u32);
        side_aligned = resize_nearest(&side_aligned, l as u32, target_h as u32);
        front = resize_nearest(&front, w as u32, target_h as u32);
        (w, l, target_h)
    } else {
        let (w, l) = (top.width() as usize, top.height() as usize);
        let h = ((side.height() as f32 * l as f32 / side.width().max(1) as f32).round() as usize)
            .max(2);
        side_aligned = resize_nearest(&side_aligned, l as u32, h as u32);
        front = resize_nearest(&front, w as u32, h as u32);
        (w, l, h)
    };

    let back_resized = resize_nearest(&back, w as u32, h as u32);
    let mut back_aligned = flip_h(&back_resized);

    top = strip_outline(&top, 70);
    side_aligned = strip_outline(&side_aligned, 70);
    front = strip_outline(&front, 70);
    back_aligned = strip_outline(&back_aligned, 70);

    let resolved_top_nose = if top_nose == "auto" {
        match_top_orientation(&top, &side_aligned)
    } else {
        top_nose
    };
    if resolved_top_nose == "up" {
        top = flip_v(&top);
    }

    // Visual-Hull 體素交集雕刻
    let mut occ = vec![false; h * l * w];
    for z in 0..h {
        let r = (h - 1 - z) as u32;
        let along: Vec<bool> = (0..l)
            .map(|y| side_aligned.get_pixel(y as u32, r).0[3] > 0)
            .collect();
        let across: Vec<bool> = (0..w)
            .map(|x| {
                front.get_pixel(x as u32, r).0[3] > 0 || back_aligned.get_pixel(x as u32, r).0[3] > 0
            })
            .collect();

        for y in 0..l {
            if !along[y] {
                continue;
            }
            for x in 0..w {
                if across[x] && top.get_pixel(x as u32, y as u32).0[3] > 0 {
                    occ[(z * l + y) * w + x] = true;
                }
            }
        }

        if round_lathe {
            let ys: Vec<usize> = (0..l).filter(|&y| along[y]).collect();
            let xs: Vec<usize> = (0..w).filter(|&x| across[x]).collect();
            if let (Some(&y0), Some(&y1), Some(&x0), Some(&x1)) =
                (ys.first(), ys.last(), xs.first(), xs.last())
            {
                let solid_y = (y0..=y1).all(|y| along[y]);
                let solid_x = (x0..=x1).all(|x| across[x]);
                if solid_y && solid_x {
                    let cy = (y0 + y1) as f32 * 0.5;
                    let cx = (x0 + x1) as f32 * 0.5;
                    let ry = (y1 - y0) as f32 * 0.5 + 0.5;
                    let rx = (x1 - x0) as f32 * 0.5 + 0.5;
                    for y in 0..l {
                        for x in 0..w {
                            let idx = (z * l + y) * w + x;
                            if occ[idx] {
                                let d = ((y as f32 - cy) / ry).powi(2)
                                    + ((x as f32 - cx) / rx).powi(2);
                                if d >= 1.0 {
                                    occ[idx] = false;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 自上而下上色並填入 VoxelCanvas
    let mut canvas = VoxelCanvas::new(h, l, w);
    let mut covered = vec![false; l * w];
    let shade = 0.2f32;

    for z in (0..h).rev() {
        let r = (h - 1 - z) as u32;
        let above = covered.clone();
        for i in 0..(l * w) {
            if occ[z * l * w + i] {
                covered[i] = true;
            }
        }
        let side_row = edge_row_rgb(&side_aligned, r, l);
        let back_row = edge_row_rgb(&back_aligned, r, w);
        let front_row = edge_row_rgb(&front, r, w);

        for y in 0..l {
            for x in 0..w {
                let o_idx = (z * l + y) * w + x;
                if !occ[o_idx] {
                    continue;
                }
                let lw_idx = y * w + x;
                // 計算距離邊界的最小深度與朝向面
                let mut d_left = 0usize;
                while x > d_left && occ[(z * l + y) * w + (x - d_left - 1)] {
                    d_left += 1;
                }
                let mut d_right = 0usize;
                while x + d_right + 1 < w && occ[(z * l + y) * w + (x + d_right + 1)] {
                    d_right += 1;
                }
                let mut d_back = 0usize;
                while y > d_back && occ[(z * l + (y - d_back - 1)) * w + x] {
                    d_back += 1;
                }
                let mut d_front = 0usize;
                while y + d_front + 1 < l && occ[(z * l + (y + d_front + 1)) * w + x] {
                    d_front += 1;
                }
                let dists = [d_left, d_right, d_back, d_front];
                let (face, &depth) = dists
                    .iter()
                    .enumerate()
                    .min_by_key(|&(_, d)| d)
                    .unwrap_or((0, &0));

                let mut near_above = false;
                'na: for dy in -2isize..=2 {
                    for dx in -2isize..=2 {
                        let ny = y as isize + dy;
                        let nx = x as isize + dx;
                        if ny >= 0 && ny < l as isize && nx >= 0 && nx < w as isize {
                            if above[ny as usize * w + nx as usize] {
                                near_above = true;
                                break 'na;
                            }
                        }
                    }
                }
                let is_above = above[lw_idx];
                let is_steep_step = !is_above && depth <= 2 && near_above;
                let use_top = !is_above && !is_steep_step;
                let use_above = z + 1 < h
                    && is_above
                    && depth > 2
                    && canvas.vol[canvas.idx(x, y, z + 1)][3] > 0;

                let top_px = top.get_pixel(x as u32, y as u32).0;
                let mut rgb: [u8; 3] = if use_above {
                    let up = canvas.vol[canvas.idx(x, y, z + 1)];
                    [up[0], up[1], up[2]]
                } else if use_top {
                    [top_px[0], top_px[1], top_px[2]]
                } else {
                    match face {
                        0 | 1 => side_row
                            .as_ref()
                            .map(|row| row[y])
                            .unwrap_or([top_px[0], top_px[1], top_px[2]]),
                        2 => back_row
                            .as_ref()
                            .map(|row| row[x])
                            .unwrap_or([top_px[0], top_px[1], top_px[2]]),
                        _ => front_row
                            .as_ref()
                            .map(|row| row[x])
                            .unwrap_or([top_px[0], top_px[1], top_px[2]]),
                    }
                };

                if (is_above || is_steep_step) && depth <= 2 {
                    let factor =
                        1.0 - shade * (1.0 - z as f32 / (h.saturating_sub(1).max(1) as f32))
                            - shade * 0.4;
                    rgb = [
                        (rgb[0] as f32 * factor).round().clamp(0.0, 255.0) as u8,
                        (rgb[1] as f32 * factor).round().clamp(0.0, 255.0) as u8,
                        (rgb[2] as f32 * factor).round().clamp(0.0, 255.0) as u8,
                    ];
                }
                let ci = canvas.idx(x, y, z);
                canvas.vol[ci] = [rgb[0], rgb[1], rgb[2], 255] as RgbaPx;
            }
        }
    }

    if canvas.voxel_count() == 0 {
        return Err("四視圖交集雕刻結果為空，請檢查四個視圖是否位置對齊。".to_string());
    }

    canvas.bake(
        title,
        category,
        None,
        default_scale,
        "static",
        false,
        true,
    )
}
