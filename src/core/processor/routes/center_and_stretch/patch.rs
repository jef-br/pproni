//! R1: the patch fill for the unsafe area of a grazed edge, a stand-in for a person's clone stamp, patch
//! tool and healing brush. [`fill`] rebuilds each gap from copies of real background: the safe overhang
//! and the photo's own background outside the product and its margin, never inside them. Then it heals the
//! colours so the gap meets every real background pixel next to it, margin zone included.
//! See docs/work-state/2026-11-13-flat-edge-gradients.md.

use image::{Rgb, RgbImage};
use rayon::prelude::*;

use super::super::super::super::config;
use crate::core::shot_classifier::Mask;

/// What the fill needs to know about the canvas besides its pixels.
pub struct Scene<'a> {
    /// The product mask, in its own (working) resolution.
    pub mask: &'a Mask,
    /// Where canvas pixel (0, 0) lies on the original picture.
    pub origin: (i64, i64),
    /// The original picture's size. Canvas pixels outside it are the overhang.
    pub orig: (u32, u32),
    /// No source patch may touch this many canvas pixels round the product.
    pub margin_px: u32,
}

/// Fill every gap of `hole` (one flag per canvas pixel, row by row).
pub fn fill(canvas: &mut RgbImage, hole: &[bool], scene: &Scene<'_>) {
    let (w, h) = (canvas.width() as usize, canvas.height() as usize);
    if hole.len() != w * h {
        return;
    }
    for gap in gaps(hole, w, h) {
        fill_gap(canvas, hole, &gap, scene);
    }
}

/// One connected gap: its pixels and its box `[x0, x1) x [y0, y1)`.
struct Gap {
    pixels: Vec<usize>,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

fn gaps(hole: &[bool], w: usize, h: usize) -> Vec<Gap> {
    let mut seen = vec![false; hole.len()];
    let mut out = Vec::new();
    for start in 0..hole.len() {
        if !hole[start] || seen[start] {
            continue;
        }
        let mut gap = Gap { pixels: Vec::new(), x0: usize::MAX, y0: usize::MAX, x1: 0, y1: 0 };
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            gap.pixels.push(i);
            gap.x0 = gap.x0.min(x);
            gap.x1 = gap.x1.max(x + 1);
            gap.y0 = gap.y0.min(y);
            gap.y1 = gap.y1.max(y + 1);
            let mut visit = |j: usize| {
                if hole[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
        }
        out.push(gap);
    }
    out
}

fn fill_gap(canvas: &mut RgbImage, hole: &[bool], gap: &Gap, scene: &Scene<'_>) {
    let (cw, ch) = (canvas.width() as usize, canvas.height() as usize);
    let halo = config::R1_PATCH_SEARCH_HALO_PX;
    // The window the fill works in: the gap and a halo round it.
    let (rx0, ry0) = (gap.x0.saturating_sub(halo), gap.y0.saturating_sub(halo));
    let (rx1, ry1) = ((gap.x1 + halo).min(cw), (gap.y1 + halo).min(ch));
    let (w, h) = (rx1 - rx0, ry1 - ry0);
    let p = config::R1_PATCH_PX | 1;
    if w < p || h < p {
        return;
    }
    let n = w * h;

    // Which window pixels lie on the original picture, and which are the product.
    let (mw, mh) = (scene.mask.width as usize, scene.mask.height as usize);
    let fx = mw as f32 / scene.orig.0.max(1) as f32;
    let fy = mh as f32 / scene.orig.1.max(1) as f32;
    let mut real = vec![false; n];
    let mut raw = vec![false; n];
    for y in 0..h {
        for x in 0..w {
            let ox = (rx0 + x) as i64 + scene.origin.0;
            let oy = (ry0 + y) as i64 + scene.origin.1;
            if ox < 0 || oy < 0 || ox >= scene.orig.0 as i64 || oy >= scene.orig.1 as i64 {
                continue;
            }
            real[y * w + x] = true;
            if mw > 0 && mh > 0 {
                let mx = (((ox as f32 + 0.5) * fx) as usize).min(mw - 1);
                let my = (((oy as f32 + 0.5) * fy) as usize).min(mh - 1);
                raw[y * w + x] = scene.mask.data[my * mw + mx] != 0;
            }
        }
    }
    let product = dilate(&raw, w, h, config::R1_PATCH_PRODUCT_EDGE_PX as usize);
    let margin = dilate(&raw, w, h, scene.margin_px as usize);

    let all_gaps: Vec<bool> = (0..n).map(|i| hole[(ry0 + i / w) * cw + rx0 + i % w]).collect();
    let mut this_gap = vec![false; n];
    for &g in &gap.pixels {
        this_gap[(g / cw - ry0) * w + (g % cw - rx0)] = true;
    }
    // Sources: the overhang, and the photo outside the product's margin. Context (what the patch is
    // matched against and healed to): every real pixel that is not the product itself.
    let source_ok: Vec<bool> = (0..n).map(|i| !all_gaps[i] && (!real[i] || !margin[i])).collect();
    let known: Vec<bool> = (0..n).map(|i| !product[i] && !all_gaps[i]).collect();

    let mut px: Vec<[u8; 3]> = Vec::with_capacity(n);
    for y in 0..h {
        for x in 0..w {
            px.push(canvas.get_pixel((rx0 + x) as u32, (ry0 + y) as u32).0);
        }
    }
    let before = px.clone();
    let bbox = (gap.x0 - rx0, gap.y0 - ry0, gap.x1 - rx0, gap.y1 - ry0);
    let seed = ((gap.x0 as u64) << 32) ^ ((gap.y0 as u64) << 16) ^ gap.pixels.len() as u64;
    copy_patches(&mut px, w, h, &this_gap, &source_ok, &known, bbox, seed);
    heal(&mut px, &before, w, h, &this_gap, &known, bbox);

    for &g in &gap.pixels {
        let v = px[(g / cw - ry0) * w + (g % cw - rx0)];
        canvas.put_pixel((g % cw) as u32, (g / cw) as u32, Rgb(v));
    }
}

/// Grow `m` by a disc of radius `r`, exactly, one run of set pixels at a time.
fn dilate(m: &[bool], w: usize, h: usize, r: usize) -> Vec<bool> {
    if r == 0 {
        return m.to_vec();
    }
    let reach: Vec<i64> = (0..=r).map(|d| ((r * r - d * d) as f64).sqrt().floor() as i64).collect();
    let stride = w + 1;
    let mut diff = vec![0i32; stride * h];
    for y in 0..h {
        let row = &m[y * w..(y + 1) * w];
        let mut x = 0;
        while x < w {
            if !row[x] {
                x += 1;
                continue;
            }
            let start = x;
            while x < w && row[x] {
                x += 1;
            }
            for dy in -(r as i64)..=(r as i64) {
                let yy = y as i64 + dy;
                if yy < 0 || yy >= h as i64 {
                    continue;
                }
                let e = reach[dy.unsigned_abs() as usize];
                let lo = (start as i64 - e).max(0) as usize;
                let hi = ((x as i64 + e).min(w as i64)) as usize;
                diff[yy as usize * stride + lo] += 1;
                diff[yy as usize * stride + hi] -= 1;
            }
        }
    }
    let mut out = vec![false; w * h];
    for y in 0..h {
        let mut run = 0;
        for x in 0..w {
            run += diff[y * stride + x];
            out[y * w + x] = run > 0;
        }
    }
    out
}

/// A small deterministic random source (xorshift): the same picture always gives the same fill.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// The clone stamp. Patch by patch, from where most of the surroundings are known: take the best match
/// among the allowed source patches and copy its pixels into the gap's still-empty ones.
#[allow(clippy::too_many_arguments)]
fn copy_patches(
    px: &mut [[u8; 3]],
    w: usize,
    h: usize,
    gap: &[bool],
    source_ok: &[bool],
    known0: &[bool],
    bbox: (usize, usize, usize, usize),
    seed: u64,
) {
    let p = config::R1_PATCH_PX | 1;
    let r = p / 2;
    let stride = w + 1;
    // Integral of the pixels a source patch must not contain.
    let mut bad = vec![0u32; stride * (h + 1)];
    for y in 0..h {
        let mut run = 0u32;
        for x in 0..w {
            run += (!source_ok[y * w + x]) as u32;
            bad[(y + 1) * stride + x + 1] = bad[y * stride + x + 1] + run;
        }
    }
    let valid_at = |sy: usize, sx: usize| -> bool {
        sy + p <= h
            && sx + p <= w
            && bad[(sy + p) * stride + sx + p] + bad[sy * stride + sx] == bad[sy * stride + sx + p] + bad[(sy + p) * stride + sx]
    };
    let mut valid: Vec<u32> = Vec::new();
    for sy in 0..=h - p {
        for sx in 0..=w - p {
            if valid_at(sy, sx) {
                valid.push((sy * w + sx) as u32);
            }
        }
    }
    if valid.is_empty() {
        return;
    }

    let mut unknown = gap.to_vec();
    let mut known = known0.to_vec();
    let mut rng = Rng(seed | 1);
    let mut recent: Vec<(i64, i64)> = Vec::new();
    let (bx0, by0, bx1, by1) = bbox;
    let corner = |y: usize, x: usize| (y.saturating_sub(r).min(h - p), x.saturating_sub(r).min(w - p));

    loop {
        // How much of every empty pixel's patch is already known.
        let (gx0, gy0) = (bx0.saturating_sub(p), by0.saturating_sub(p));
        let (gx1, gy1) = ((bx1 + p).min(w), (by1 + p).min(h));
        let (gw, gh) = (gx1 - gx0, gy1 - gy0);
        let gs = gw + 1;
        let mut integral = vec![0u32; gs * (gh + 1)];
        for y in 0..gh {
            let mut run = 0u32;
            for x in 0..gw {
                run += known[(gy0 + y) * w + gx0 + x] as u32;
                integral[(y + 1) * gs + x + 1] = integral[y * gs + x + 1] + run;
            }
        }
        let mut targets: Vec<(u32, usize, usize)> = Vec::new();
        for y in by0..by1 {
            for x in bx0..bx1 {
                if !unknown[y * w + x] {
                    continue;
                }
                let (ty, tx) = corner(y, x);
                let (a, b) = (ty - gy0, tx - gx0);
                let count = integral[(a + p) * gs + b + p] + integral[a * gs + b] - integral[a * gs + b + p] - integral[(a + p) * gs + b];
                if count > 0 {
                    targets.push((count, y, x));
                }
            }
        }
        if targets.is_empty() {
            break;
        }
        // Most known first, which paints in from the gap's borders.
        targets.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        for (_, y, x) in targets {
            if !unknown[y * w + x] {
                continue;
            }
            let (ty, tx) = corner(y, x);
            let mut template = vec![false; p * p];
            for dy in 0..p {
                for dx in 0..p {
                    template[dy * p + dx] = known[(ty + dy) * w + tx + dx];
                }
            }
            let cost = |sy: usize, sx: usize| -> u64 {
                let mut s = 0u64;
                for dy in 0..p {
                    let (t, c) = ((ty + dy) * w + tx, (sy + dy) * w + sx);
                    for dx in 0..p {
                        if template[dy * p + dx] {
                            let (a, b) = (px[t + dx], px[c + dx]);
                            for k in 0..3 {
                                let d = a[k] as i32 - b[k] as i32;
                                s += (d * d) as u64;
                            }
                        }
                    }
                }
                s
            };
            // Candidates: the last patches' offsets (what lined up next door probably lines up here),
            // and a spread of random ones.
            let mut cands: Vec<(usize, usize)> = Vec::with_capacity(config::R1_PATCH_CANDIDATES + recent.len());
            for &(dy, dx) in &recent {
                let (sy, sx) = (ty as i64 + dy, tx as i64 + dx);
                if sy >= 0 && sx >= 0 && valid_at(sy as usize, sx as usize) {
                    cands.push((sy as usize, sx as usize));
                }
            }
            for _ in 0..config::R1_PATCH_CANDIDATES {
                let v = valid[(rng.next() % valid.len() as u64) as usize] as usize;
                cands.push((v / w, v % w));
            }
            let Some((mut best_cost, mut by, mut bx)) = cands.par_iter().map(|&(sy, sx)| (cost(sy, sx), sy, sx)).min() else {
                continue;
            };
            // A few steps of local search round the best one.
            for radius in [8i64, 4, 2, 1] {
                for _ in 0..16 {
                    let sy = by as i64 + (rng.next() % (2 * radius as u64 + 1)) as i64 - radius;
                    let sx = bx as i64 + (rng.next() % (2 * radius as u64 + 1)) as i64 - radius;
                    if sy < 0 || sx < 0 || !valid_at(sy as usize, sx as usize) {
                        continue;
                    }
                    let c = cost(sy as usize, sx as usize);
                    if c < best_cost {
                        (best_cost, by, bx) = (c, sy as usize, sx as usize);
                    }
                }
            }
            for dy in 0..p {
                for dx in 0..p {
                    let t = (ty + dy) * w + tx + dx;
                    if unknown[t] {
                        px[t] = px[(by + dy) * w + bx + dx];
                        unknown[t] = false;
                        known[t] = true;
                    }
                }
            }
            recent.push((by as i64 - ty as i64, bx as i64 - tx as i64));
            if recent.len() > 48 {
                recent.remove(0);
            }
        }
    }
}

/// The healing brush. Inside the gap keep the copied texture (its gradients), but take the colours from
/// the real background round it: Poisson blending, solved by conjugate gradients. A side with no real
/// background (the product, the border) lets the colour float.
fn heal(px: &mut [[u8; 3]], before: &[[u8; 3]], w: usize, h: usize, gap: &[bool], known: &[bool], bbox: (usize, usize, usize, usize)) {
    let mut id = vec![-1i32; w * h];
    let mut cells: Vec<usize> = Vec::new();
    for y in bbox.1..bbox.3 {
        for x in bbox.0..bbox.2 {
            if gap[y * w + x] {
                id[y * w + x] = cells.len() as i32;
                cells.push(y * w + x);
            }
        }
    }
    let n = cells.len();
    if n == 0 {
        return;
    }
    let mut neighbours = vec![[-1i32; 4]; n];
    let mut diag = vec![0f32; n];
    let mut rhs = vec![[0f32; 3]; n];
    for (u, &cell) in cells.iter().enumerate() {
        let (x, y) = ((cell % w) as i64, (cell / w) as i64);
        for (k, (dy, dx)) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)].into_iter().enumerate() {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                continue;
            }
            let q = ny as usize * w + nx as usize;
            let inside = id[q] >= 0;
            if !inside && !known[q] {
                continue;
            }
            diag[u] += 1.0;
            for c in 0..3 {
                rhs[u][c] += px[cell][c] as f32 - px[q][c] as f32;
                if !inside {
                    rhs[u][c] += before[q][c] as f32;
                }
            }
            if inside {
                neighbours[u][k] = id[q];
            }
        }
    }
    for c in 0..3 {
        let apply = |x: &[f32], out: &mut [f32]| {
            for u in 0..n {
                let mut s = diag[u] * x[u];
                for &j in &neighbours[u] {
                    if j >= 0 {
                        s -= x[j as usize];
                    }
                }
                out[u] = s;
            }
        };
        let b: Vec<f32> = rhs.iter().map(|v| v[c]).collect();
        let mut x: Vec<f32> = cells.iter().map(|&cell| px[cell][c] as f32).collect();
        let mut ax = vec![0f32; n];
        apply(&x, &mut ax);
        let mut r: Vec<f32> = (0..n).map(|u| b[u] - ax[u]).collect();
        let mut d = r.clone();
        let mut rs: f32 = r.iter().map(|v| v * v).sum();
        let mut ad = vec![0f32; n];
        for _ in 0..3000 {
            if rs < 1e-3 * n as f32 {
                break;
            }
            apply(&d, &mut ad);
            let dad: f32 = d.iter().zip(&ad).map(|(a, b)| a * b).sum();
            if dad.abs() < 1e-12 {
                break;
            }
            let alpha = rs / dad;
            for u in 0..n {
                x[u] += alpha * d[u];
                r[u] -= alpha * ad[u];
            }
            let rs_new: f32 = r.iter().map(|v| v * v).sum();
            let beta = rs_new / rs;
            for u in 0..n {
                d[u] = r[u] + beta * d[u];
            }
            rs = rs_new;
        }
        for (u, &cell) in cells.iter().enumerate() {
            px[cell][c] = x[u].round().clamp(0.0, 255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_product(w: u32, h: u32) -> Mask {
        Mask { width: w, height: h, data: vec![0u8; (w * h) as usize] }
    }

    /// A canvas of vertical stripes, 3 px wide, light and dark; `gap` is cleared to black.
    fn stripes(side: u32, gap: (u32, u32, u32, u32)) -> (RgbImage, Vec<bool>) {
        let mut img = RgbImage::new(side, side);
        let mut hole = vec![false; (side * side) as usize];
        for y in 0..side {
            for x in 0..side {
                let v = if (x / 3) % 2 == 0 { 90 } else { 170 };
                img.put_pixel(x, y, Rgb([v, v, v]));
                if x >= gap.0 && x < gap.2 && y >= gap.1 && y < gap.3 {
                    img.put_pixel(x, y, Rgb([0, 0, 0]));
                    hole[(y * side + x) as usize] = true;
                }
            }
        }
        (img, hole)
    }

    #[test]
    fn a_gap_in_a_pattern_is_rebuilt_from_the_pattern() {
        let (mut img, hole) = stripes(80, (24, 20, 56, 36));
        let mask = no_product(80, 80);
        fill(&mut img, &hole, &Scene { mask: &mask, origin: (0, 0), orig: (80, 80), margin_px: 0 });
        for y in 20..36 {
            for x in 24..56 {
                let v = img.get_pixel(x, y)[0] as i32;
                assert!((70..=190).contains(&v), "({x}, {y}) is {v}: not background");
            }
        }
        // The pattern goes on: the gap is not one flat colour.
        let row: Vec<u8> = (24..56).map(|x| img.get_pixel(x, 28)[0]).collect();
        assert!(row.iter().max().unwrap() - row.iter().min().unwrap() > 40, "a flat gap: {row:?}");
    }

    #[test]
    fn nothing_is_copied_from_the_product_or_its_margin() {
        // Grey 120 everywhere, a red product block left of the gap, its margin 10 px.
        let side = 80u32;
        let mut img = RgbImage::from_pixel(side, side, Rgb([120, 120, 120]));
        let mut mask = no_product(side, side);
        for y in 10..70 {
            for x in 6..14 {
                img.put_pixel(x, y, Rgb([250, 20, 20]));
                mask.data[(y * side + x) as usize] = 255;
            }
        }
        let mut hole = vec![false; (side * side) as usize];
        for y in 30..40 {
            for x in 40..70 {
                img.put_pixel(x, y, Rgb([0, 0, 0]));
                hole[(y * side + x) as usize] = true;
            }
        }
        fill(&mut img, &hole, &Scene { mask: &mask, origin: (0, 0), orig: (side, side), margin_px: 10 });
        for y in 30..40 {
            for x in 40..70 {
                let p = img.get_pixel(x, y);
                assert!(p[0] < 160 && p[0] > 80, "({x}, {y}) is {p:?}: red or empty, not the grey background");
            }
        }
    }

    #[test]
    fn a_gap_with_nothing_known_beside_it_is_left_alone() {
        let side = 30u32;
        let mut img = RgbImage::from_pixel(side, side, Rgb([0, 0, 0]));
        let mask = no_product(side, side);
        let hole = vec![true; (side * side) as usize];
        fill(&mut img, &hole, &Scene { mask: &mask, origin: (0, 0), orig: (side, side), margin_px: 0 });
        assert!(img.pixels().all(|p| *p == Rgb([0, 0, 0])));
    }
}
