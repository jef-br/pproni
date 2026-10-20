//! R1 step 2 — build the square.
//!
//! Two passes, horizontal then vertical. The first widens every real row to the square's width;
//! the second stretches the result to the square's height. Splitting it this way is what fills the
//! *corners*: a corner sits outside the image on both axes at once, so no single pass can source
//! it from real pixels — but after the horizontal pass, the top rows are already the full square
//! width, and the vertical pass reads them like any other row.
//!
//! Both passes are the same function over one line — a row for the first, a column for the second.
//! Crop and stretch are not separate branches in it: a side whose square edge lands inside the
//! image resamples a source segment onto a target of exactly the same length, which is a copy.
//!
//! ## Stretch vs inpaint, inside a gated band
//!
//! The planner handles *touching* edges by pinning (the square flush against them, no stretch
//! required). What this module adds is per-line handling of *gated* edges — the ones the pass-1
//! gate flagged but refinement cleared. For those, blind stretching smears whatever the gate was
//! worried about (a shadow sliver, a shoe fragment) across the overhang.
//!
//! Instead, each gated side is classified per line: if the few pixels nearest the edge sit close
//! to the background colour, the line stretches as before; otherwise [`super::inpaint`] fills it,
//! so the fragment is not resampled. The axis mapping is natural — pass 1 looks at Left/Right,
//! pass 2 at Top/Bottom. Pass 2 reads `mid` rather than `src`, because that's what pass 2
//! actually stretches from.

use image::{Rgb, RgbImage};

use super::super::super::super::config;
use super::inpaint;
use super::plan::SquarePlan;
use crate::core::shot_classifier::geometry::{Edge, Rect};

/// Compose the square from `src`, stretching background outward wherever the square reaches past a
/// border. `subject` is the subject's box in `src`'s coordinates; `safety` is how far outside it
/// the background is considered trustworthy. `gated` lists the edges the pass-1 gate flagged (red
/// and orange on stage 90) — those sides go through the classify/inpaint path instead of plain
/// resampling. `bgc` is the one dominant background colour the exporter sampled, when present;
/// without one, each gated side tries a local per-edge sample before giving up on stretching.
/// `probe_px` is how many pixels inward from the edge the classifier reads to make the call.
pub fn compose(
    src: &RgbImage,
    plan: &SquarePlan,
    subject: Rect,
    silhouette: &super::Silhouette,
    safety: u32,
    clearance: u32,
    gated: &[Edge],
    bgc: Option<Rgb<u8>>,
    probe_px: u32,
) -> RgbImage {
    let (w, h) = (src.width(), src.height());
    let Some(real) = plan.real_region(w, h) else {
        return RgbImage::new(plan.side, plan.side);
    };
    let side = plan.side as usize;
    let safety = safety as i64;
    let clearance = clearance as usize;
    let probe = probe_px.max(1) as usize;
    let tol = config::R1_STRETCH_BGC_TOLERANCE;

    // Pre-pass: resolve a BGC per gated side (global if given, else local-sample fallback).
    // A side that isn't gated keeps the "no decision needed" path (SideState::Clean).
    let left_state = resolve_side(src, &real, Edge::Left, gated, bgc, probe);
    let right_state = resolve_side(src, &real, Edge::Right, gated, bgc, probe);

    // Pass 1: every real row becomes a full-width square row.
    let mut mid = RgbImage::new(plan.side, real.h);
    let mut line: Vec<Rgb<u8>> = vec![Rgb([0, 0, 0]); side];
    let mut fills: Vec<LineFill> = Vec::with_capacity(real.h as usize);
    for ry in 0..real.h {
        let y = real.y + ry;
        let row: Vec<Rgb<u8>> = (0..w).map(|x| *src.get_pixel(x, y)).collect();
        // Product band from the silhouette's reach on this row (plan A.1). A row with no subject,
        // or a rectangular mask, falls back to the bbox — reproducing the previous behaviour
        // exactly. `max_x` is inclusive, so the band's high edge is `max_x + 1`.
        let (inner_lo, inner_hi) = match silhouette.row_x.get(y as usize).copied().flatten() {
            Some((lo, hi)) => (lo as i64 - safety, hi as i64 + 1 + safety),
            None => (subject.x as i64 - safety, (subject.x + subject.w) as i64 + safety),
        };
        let fill = fill_line(
            &row,
            &mut line,
            plan.x,
            inner_lo,
            inner_hi,
            FillSides { outer_lo: left_state, outer_hi: right_state },
            clearance,
            probe,
            tol,
        );
        fills.push(fill);
        for (x, px) in line.iter().enumerate() {
            mid.put_pixel(x as u32, ry, *px);
        }
    }

    // Plan A.5 neighbour-row borrow: replace each fully-bled segment's flat BGC/white fill with the
    // nearest clean neighbour row's background at the same columns, so a run of bled rows reads as
    // the surrounding background rather than a flat bar. Pixels with no clean neighbour in their
    // column keep the BGC/white fill (the rest of the fallback chain).
    borrow_neighbour_rows(&mut mid, &fills);

    // After pass 1, resolve BGC for Top/Bottom against `mid` — pass 2 reads mid, not src, so a
    // column pass 1 inpainted appears in its context here.
    let mid_real = Rect { x: 0, y: 0, w: plan.side, h: real.h };
    let top_state = resolve_side(&mid, &mid_real, Edge::Top, gated, bgc, probe);
    let bottom_state = resolve_side(&mid, &mid_real, Edge::Bottom, gated, bgc, probe);

    // Pass 2: those rows become the square's full height. `mid` row 0 is original row `real.y`, so
    // the square's origin sits at `plan.y - real.y` in this pass's coordinates.
    let mut out = RgbImage::new(plan.side, plan.side);
    for x in 0..plan.side {
        let col: Vec<Rgb<u8>> = (0..real.h).map(|y| *mid.get_pixel(x, y)).collect();
        fill_line(
            &col,
            &mut line,
            plan.y - real.y as i64,
            subject.y as i64 - safety - real.y as i64,
            (subject.y + subject.h) as i64 + safety - real.y as i64,
            FillSides { outer_lo: top_state, outer_hi: bottom_state },
            clearance,
            probe,
            tol,
        );
        for (y, px) in line.iter().enumerate() {
            out.put_pixel(x, y as u32, *px);
        }
    }
    out
}

/// The three states a side can be in on one pass.
///
/// * `Clean` — not gated. The outer segment is always resampled, exactly as the original
///   implementation did; the gated-vs-unstretchable question doesn't apply.
/// * `GatedWith(bgc)` — gated, and we have a reference BGC (global or per-edge median). The
///   classifier decides per line whether the inward strip stays close to it.
/// * `GatedBlind` — gated, no reference BGC. Every line on this side inpaints.
#[derive(Clone, Copy, Debug)]
enum SideState {
    Clean,
    GatedWith(Rgb<u8>),
    GatedBlind,
}

#[derive(Clone, Copy, Debug)]
struct FillSides {
    outer_lo: SideState,
    outer_hi: SideState,
}

/// Pick the BGC to use on one side, or declare the side clean/blind.
///
/// `region` is the band of real pixels in the source we care about (pass 1 gives src's real region,
/// pass 2 gives mid's whole area). A gated side first tries the global `bgc`; without one it
/// samples [`config::R1_LOCAL_BGC_SAMPLES`] colours along a thin inward strip, drops outliers via
/// an MAD rule mirroring the preprocessor's trim, and accepts the per-channel median when the
/// kept set fits within [`config::R1_LOCAL_BGC_SPAN_MAX`] on every channel.
fn resolve_side(
    img: &RgbImage,
    region: &Rect,
    edge: Edge,
    gated: &[Edge],
    global_bgc: Option<Rgb<u8>>,
    probe: usize,
) -> SideState {
    if !gated.contains(&edge) {
        return SideState::Clean;
    }
    if let Some(c) = global_bgc {
        return SideState::GatedWith(c);
    }
    match local_edge_bgc(img, region, edge, probe) {
        Some(c) => SideState::GatedWith(c),
        None => SideState::GatedBlind,
    }
}

/// Sample a thin strip just inside `edge` of `region`, filter outliers, and return the per-channel
/// median when the kept samples are tight enough per [`config::R1_LOCAL_BGC_SPAN_MAX`].
fn local_edge_bgc(img: &RgbImage, region: &Rect, edge: Edge, probe: usize) -> Option<Rgb<u8>> {
    let n = config::R1_LOCAL_BGC_SAMPLES as usize;
    if n == 0 || probe == 0 || region.w == 0 || region.h == 0 {
        return None;
    }
    // Walk the edge direction, picking `n` evenly spaced positions; at each one, average a few
    // inward pixels so a single speckle doesn't poison the sample.
    let (edge_len, inward_len) = match edge {
        Edge::Top | Edge::Bottom => (region.w as usize, region.h as usize),
        Edge::Left | Edge::Right => (region.h as usize, region.w as usize),
    };
    let depth = probe.min(inward_len);
    if depth == 0 {
        return None;
    }
    let mut samples: Vec<[u16; 3]> = Vec::with_capacity(n);
    for i in 0..n {
        let along = (i * edge_len.saturating_sub(1)) / n.max(1).max(1);
        let mut acc = [0u32; 3];
        for d in 0..depth {
            let (x, y) = inward_pixel(region, edge, along as u32, d as u32);
            let p = img.get_pixel(x, y).0;
            acc[0] += p[0] as u32;
            acc[1] += p[1] as u32;
            acc[2] += p[2] as u32;
        }
        let m = depth as u32;
        samples.push([(acc[0] / m) as u16, (acc[1] / m) as u16, (acc[2] / m) as u16]);
    }
    // Per-channel median, then MAD-filter on L1 distance to it (same shape as the preprocessor's
    // trim rule), then recompute the median across the kept set.
    let med0 = per_channel_median(&samples);
    let dists: Vec<u32> = samples.iter().map(|s| l1(s, &med0)).collect();
    let mad = median_u32(&dists);
    let threshold = config::TRIM_OUTLIER_FLOOR + (config::TRIM_OUTLIER_K * mad as f64).round() as u32;
    let kept: Vec<[u16; 3]> = samples
        .iter()
        .copied()
        .zip(dists.iter())
        .filter(|(_, &d)| d <= threshold)
        .map(|(s, _)| s)
        .collect();
    // A clean per-edge BGC needs most of the samples to agree, not just the MAD-filter's
    // residual. A half-clean / half-textured strip (the mountain-landscape case) agrees on only
    // the clean half; accepting it would read back as "the whole edge is stretchable" and we'd
    // smear the textured half across the overhang. 75% is the floor.
    let min_kept = ((n * 3) / 4).max(config::TRIM_MIN_KEEP);
    if kept.len() < min_kept {
        return None;
    }
    // Reject the whole side when any channel's span across the kept set is too wide — the edge
    // isn't flat enough to trust as a backdrop.
    for c in 0..3 {
        let (lo, hi) = channel_range(&kept, c);
        if hi.saturating_sub(lo) > config::R1_LOCAL_BGC_SPAN_MAX as u16 {
            return None;
        }
    }
    let median = per_channel_median(&kept);
    Some(Rgb([median[0] as u8, median[1] as u8, median[2] as u8]))
}

/// Map `(along_edge, inward_depth)` to a pixel inside `region`.
fn inward_pixel(region: &Rect, edge: Edge, along: u32, depth: u32) -> (u32, u32) {
    match edge {
        Edge::Top => (region.x + along.min(region.w - 1), region.y + depth.min(region.h - 1)),
        Edge::Bottom => (
            region.x + along.min(region.w - 1),
            region.y + (region.h - 1 - depth.min(region.h - 1)),
        ),
        Edge::Left => (region.x + depth.min(region.w - 1), region.y + along.min(region.h - 1)),
        Edge::Right => (
            region.x + (region.w - 1 - depth.min(region.w - 1)),
            region.y + along.min(region.h - 1),
        ),
    }
}

fn per_channel_median(samples: &[[u16; 3]]) -> [u16; 3] {
    let mut channels: [Vec<u16>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for s in samples {
        channels[0].push(s[0]);
        channels[1].push(s[1]);
        channels[2].push(s[2]);
    }
    let mut out = [0u16; 3];
    for (c, mut v) in channels.into_iter().enumerate() {
        v.sort_unstable();
        out[c] = v[v.len() / 2];
    }
    out
}

fn median_u32(values: &[u32]) -> u32 {
    if values.is_empty() {
        return 0;
    }
    let mut v: Vec<u32> = values.to_vec();
    v.sort_unstable();
    v[v.len() / 2]
}

fn l1(a: &[u16; 3], b: &[u16; 3]) -> u32 {
    (a[0] as i32 - b[0] as i32).unsigned_abs()
        + (a[1] as i32 - b[1] as i32).unsigned_abs()
        + (a[2] as i32 - b[2] as i32).unsigned_abs()
}

fn channel_range(samples: &[[u16; 3]], c: usize) -> (u16, u16) {
    let mut lo = u16::MAX;
    let mut hi = 0u16;
    for s in samples {
        lo = lo.min(s[c]);
        hi = hi.max(s[c]);
    }
    (lo, hi)
}

/// Classify one outer-segment slice: is the strip of pixels nearest the gated edge close enough
/// to this side's background colour for a plain resample, or does it carry something — a shadow,
/// a shoe fragment — that would smear across the overhang?
///
/// Reads only `probe` pixels. The slice itself may be longer; we care about the ones against the
/// edge, which is where the gate was looking in the first place.
fn is_stretchable(outer_slice: &[Rgb<u8>], state: SideState, tol: u8, probe: usize, edge_at_lo: bool) -> bool {
    match state {
        SideState::Clean => true,
        SideState::GatedBlind => false,
        SideState::GatedWith(bgc) => {
            if outer_slice.is_empty() {
                return true;
            }
            let n = probe.min(outer_slice.len());
            let probe_slice = if edge_at_lo {
                &outer_slice[..n]
            } else {
                &outer_slice[outer_slice.len() - n..]
            };
            probe_slice.iter().all(|px| within_tol(*px, bgc, tol))
        }
    }
}

// <jbpair goal="R1 stretch-source clean-run clip: BGC tolerance threshold and the clean-run-from-edge rule that keeps a dark shadow run deeper in the source out of the stretch (the 10/11 fix); best-judged + verified on real stems, pending user /pair">
/// The clean (within-tol) run of `slice` adjacent to the gated edge, for a side with a known BGC.
///
/// `is_stretchable` only asks whether the *edge-nearest* `probe` pixels are background; it cannot
/// see a dark region sitting deeper in the source (the 10/11 right-side streak, 5–40 px inside the
/// edge). Stretching the whole source then smears that region across the overhang. Sampling only
/// this run keeps the stretch to the genuine background band against the edge.
///
/// `edge_at_lo` = the gated edge is at index 0; otherwise it is at the slice's end. For a non-gated
/// (`Clean`) or blind side the full slice is returned unchanged, so crops and ungated stretches are
/// byte-identical.
fn clean_source(slice: &[Rgb<u8>], state: SideState, tol: u8, edge_at_lo: bool) -> &[Rgb<u8>] {
    let SideState::GatedWith(bgc) = state else {
        return slice;
    };
    let run = if edge_at_lo {
        slice.iter().take_while(|p| within_tol(**p, bgc, tol)).count()
    } else {
        slice.iter().rev().take_while(|p| within_tol(**p, bgc, tol)).count()
    };
    if edge_at_lo {
        &slice[..run]
    } else {
        &slice[slice.len() - run..]
    }
}
// </jbpair>

fn within_tol(px: Rgb<u8>, bgc: Rgb<u8>, tol: u8) -> bool {
    px.0.iter().zip(bgc.0.iter()).all(|(a, b)| a.abs_diff(*b) <= tol)
}

// <jbpair goal="R1 fill decisions: product-band vs stretch vs inpaint, the inpaint-context→BGC rule (never flat-fill a product/shadow pixel), the mask clearance, and the A.5 neighbour-row borrow algorithm; best-judged + real-image verified (det3/det4, 10/11), pending user /pair">
/// Write one output line of `out.len()` samples, taken from `src`.
///
/// `start` is the source index the output's first sample maps to — negative when the square
/// begins outside the image. `inner_lo`/`inner_hi` bound the subject plus its safety margin: the
/// segment between them is copied untouched, because that is where the product lives and the
/// product is never resampled. Everything outside them is background, and background is what
/// absorbs the difference between what the source has and what the square needs.
///
/// `sides` tells the function which outer segments are gated and with what BGC; a `Clean` side
/// goes down the same path the original compose had, so no gated image is affected by this.
///
/// Returns [`LineFill`] so the caller can run the plan A.5 neighbour-row borrow: a *fully-bled*
/// segment (no background left to stretch from, so it fell to a flat BGC/white fill) is a candidate
/// to be replaced by the nearest clean neighbour row's background at the same columns.
fn fill_line(
    src: &[Rgb<u8>],
    out: &mut [Rgb<u8>],
    start: i64,
    inner_lo: i64,
    inner_hi: i64,
    sides: FillSides,
    clearance: usize,
    probe: usize,
    tol: u8,
) -> LineFill {
    let len = src.len() as i64;
    let side = out.len() as i64;
    if len == 0 || side == 0 {
        return LineFill { lo_bled: false, hi_bled: false, a: 0, b: 0 };
    }

    // The source range the square actually covers, and the product segment inside it.
    let lo_src = start.max(0);
    let hi_src = (start + side).min(len);
    let inner_lo = inner_lo.clamp(lo_src, hi_src);
    let inner_hi = inner_hi.clamp(inner_lo, hi_src);

    // The same two boundaries in output coordinates.
    let a = (inner_lo - start).clamp(0, side) as usize;
    let b = (inner_hi - start).clamp(0, side) as usize;

    // Mask-dilated stretch source (plan A.2). On a *gated* side, pull the inner end of the source
    // back by `clearance` so the strip hugging the silhouette — a cuff, a shadow halo, a pixel that
    // merely sits within BGC tolerance (the 10/11 grey streak) — is never sampled into the stretch.
    // A `Clean` side keeps the full source, so crops and ungated stretches are byte-identical.
    let lo_clear = if matches!(sides.outer_lo, SideState::Clean) { 0 } else { clearance as i64 };
    let hi_clear = if matches!(sides.outer_hi, SideState::Clean) { 0 } else { clearance as i64 };
    let lo_src_end = (inner_lo - lo_clear).max(lo_src);
    let hi_src_start = (inner_hi + hi_clear).min(hi_src);

    let outer_lo_src = &src[lo_src as usize..lo_src_end as usize];
    let outer_hi_src = &src[hi_src_start as usize..hi_src as usize];

    // A segment is "fully bled" when the square overhangs that side but there is no background left
    // to stretch or copy — it fell to the flat BGC/white fill. The caller may then borrow a clean
    // neighbour row (plan A.5).
    let lo_bled = a > 0 && outer_lo_src.is_empty();
    let hi_bled = b < out.len() && outer_hi_src.is_empty();

    // Decide once per line, so the two segments and the product band can be written in order and
    // the inpainter can read the product-band's edge pixel as a neighbour context.
    let lo_stretch = is_stretchable(outer_lo_src, sides.outer_lo, tol, probe, true);
    let hi_stretch = is_stretchable(outer_hi_src, sides.outer_hi, tol, probe, false);

    // Outer-lo segment first.
    if a > 0 {
        if outer_lo_src.is_empty() {
            // A fully-bled row: the subject runs to the border and there is no background left on
            // this side to stretch or copy. `resample` would early-return and leave the overhang
            // black; the product band must not be read as a colour source either. Fall flat to the
            // side BGC, else white — the hard "fill is never black" bound.
            inpaint::fill_segment(&mut out[..a], None, None, side_bgc(sides.outer_lo));
        } else if lo_stretch {
            resample(clean_source(outer_lo_src, sides.outer_lo, tol, true), &mut out[..a]);
        } else {
            // Right-side context is the first product-band pixel (which we're about to write just
            // after this). Read it directly from `src` so we don't depend on write order. It is a
            // usable fill colour only when it is itself clean background: a dark safety-margin
            // pixel (a cast shadow just outside the silhouette — the 10/11 hem shadow) must not be
            // flat-filled across the overhang, so fall to the BGC instead (plan A.5: never the
            // product colour).
            let bgc = side_bgc(sides.outer_lo);
            let right_ctx = (inner_lo < hi_src)
                .then(|| src[inner_lo as usize])
                .filter(|&px| bgc.map_or(true, |b| within_tol(px, b, tol)));
            inpaint::fill_segment(&mut out[..a], None, right_ctx, bgc);
        }
    }

    // Product band: untouched copy from the source.
    for (k, i) in (inner_lo..inner_hi).enumerate() {
        out[a + k] = src[i as usize];
    }

    // Outer-hi segment.
    if b < out.len() {
        if outer_hi_src.is_empty() {
            // Fully-bled row on this side: same as outer-lo above — flat BGC-or-white, never the
            // black that an empty `resample` leaves, never the product band.
            inpaint::fill_segment(&mut out[b..], None, None, side_bgc(sides.outer_hi));
        } else if hi_stretch {
            resample(clean_source(outer_hi_src, sides.outer_hi, tol, false), &mut out[b..]);
        } else {
            // Left-side context is the last product-band pixel (what we just wrote at position
            // b-1 if the product band was non-empty, otherwise None). Same rule as the lo side: a
            // dark safety-margin pixel is not clean background, so fall to the BGC rather than
            // flat-fill a shadow across the overhang (plan A.5).
            let bgc = side_bgc(sides.outer_hi);
            let left_ctx = (b > a)
                .then(|| out[b - 1])
                .filter(|&px| bgc.map_or(true, |b2| within_tol(px, b2, tol)));
            inpaint::fill_segment(&mut out[b..], left_ctx, None, bgc);
        }
    }

    LineFill { lo_bled, hi_bled, a, b }
}

/// What [`fill_line`] did on one line, for the plan A.5 neighbour-row borrow. `a`/`b` are the
/// output-column boundaries of the product band; a bled lo segment occupies `0..a`, a bled hi
/// segment `b..side`.
#[derive(Clone, Copy)]
struct LineFill {
    lo_bled: bool,
    hi_bled: bool,
    a: usize,
    b: usize,
}

/// Plan A.5 neighbour-row borrow. For each output column that any fully-bled row touched, replace
/// those bled pixels with the nearest clean neighbour row's value at the same column. A column that
/// is bled on every row (the subject runs the full height against that border) has no clean
/// neighbour and keeps its BGC/white fill. Only bled pixels are ever written, so every stretched or
/// copied pixel is byte-for-byte unchanged.
fn borrow_neighbour_rows(mid: &mut RgbImage, fills: &[LineFill]) {
    let h = mid.height() as usize;
    let w = mid.width();
    if h == 0 {
        return;
    }
    let is_bled = |ry: usize, x: u32| {
        let f = fills[ry];
        (f.lo_bled && (x as usize) < f.a) || (f.hi_bled && (x as usize) >= f.b)
    };
    for x in 0..w {
        if !(0..h).any(|ry| is_bled(ry, x)) {
            continue;
        }
        // Nearest clean (non-bled) row at or above / at or below each row, by two linear sweeps.
        let mut above = vec![None; h];
        let mut last = None;
        for ry in 0..h {
            if !is_bled(ry, x) {
                last = Some(ry);
            }
            above[ry] = last;
        }
        let mut below = vec![None; h];
        let mut next = None;
        for ry in (0..h).rev() {
            if !is_bled(ry, x) {
                next = Some(ry);
            }
            below[ry] = next;
        }
        for ry in 0..h {
            if !is_bled(ry, x) {
                continue;
            }
            let src_row = match (above[ry], below[ry]) {
                (Some(a), Some(b)) => if ry - a <= b - ry { a } else { b },
                (Some(a), None) => a,
                (None, Some(b)) => b,
                (None, None) => continue, // whole column bled — keep the BGC/white fill
            };
            let px = *mid.get_pixel(x, src_row as u32);
            mid.put_pixel(x, ry as u32, px);
        }
    }
}
// </jbpair>

fn side_bgc(state: SideState) -> Option<Rgb<u8>> {
    match state {
        SideState::GatedWith(c) => Some(c),
        _ => None,
    }
}

/// Classify every position along a gated edge of `src` as stretchable or not, from the same inputs
/// the compose loop uses. Length is `src.width()` for Top/Bottom, `src.height()` for Left/Right.
///
/// Used by stage 90 to show the user *which* columns inside the orange band R1 is going to
/// stretch versus inpaint. Reads only `src`, so it is a pre-pass snapshot — pass 2 actually reads
/// `mid`, but the per-edge decision for Top/Bottom usually agrees because pass 1 only alters
/// pixels in gated-and-unstretchable sub-ranges.
pub fn classify_edge(src: &RgbImage, edge: Edge, bgc: Option<Rgb<u8>>, probe_px: u32) -> Vec<bool> {
    let region = Rect { x: 0, y: 0, w: src.width(), h: src.height() };
    let probe = probe_px.max(1) as usize;
    let state = resolve_side(src, &region, edge, &[edge], bgc, probe);
    let along = match edge {
        Edge::Top | Edge::Bottom => src.width(),
        Edge::Left | Edge::Right => src.height(),
    };
    let depth = probe.min(match edge {
        Edge::Top | Edge::Bottom => src.height() as usize,
        Edge::Left | Edge::Right => src.width() as usize,
    });
    let tol = config::R1_STRETCH_BGC_TOLERANCE;
    let bgc = match state {
        SideState::Clean => return vec![true; along as usize],
        SideState::GatedBlind => return vec![false; along as usize],
        SideState::GatedWith(c) => c,
    };
    (0..along)
        .map(|i| {
            (0..depth).all(|d| {
                let (x, y) = inward_pixel(&region, edge, i, d as u32);
                within_tol(*src.get_pixel(x, y), bgc, tol)
            })
        })
        .collect()
}

/// Linear resample of `src` across `out`, with both ends pinned: `out[0]` is `src[0]` and the last
/// sample of each is the other's. Pinning matters at the outer end — that sample lands on the
/// square's own border, so anything else would shift the background by a fraction of a pixel and
/// leave a seam against the untouched product band.
///
/// An empty source with a non-empty target cannot happen from `fill_line`'s clamping unless the
/// subject runs to the border with no background left at all; that line is left black rather than
/// invented, so the case is visible instead of silently plausible.
pub(super) fn resample(src: &[Rgb<u8>], out: &mut [Rgb<u8>]) {
    if out.is_empty() || src.is_empty() {
        return;
    }
    let n = src.len();
    let m = out.len();
    if n == m {
        out.copy_from_slice(src);
        return;
    }
    let span = (n - 1) as f32;
    let steps = (m - 1).max(1) as f32;
    for (k, dst) in out.iter_mut().enumerate() {
        let pos = k as f32 * span / steps;
        let i = pos.floor() as usize;
        let t = pos - i as f32;
        let a = src[i.min(n - 1)];
        let b = src[(i + 1).min(n - 1)];
        *dst = Rgb([
            (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t).round() as u8,
            (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t).round() as u8,
            (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t).round() as u8,
        ]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_side_is_always_stretchable() {
        let slice = vec![Rgb([0, 0, 0]); 10];
        assert!(is_stretchable(&slice, SideState::Clean, 18, 5, true));
    }

    #[test]
    fn gated_blind_is_never_stretchable() {
        let slice = vec![Rgb([255, 255, 255]); 10];
        assert!(!is_stretchable(&slice, SideState::GatedBlind, 18, 5, true));
    }

    #[test]
    fn gated_with_bgc_passes_a_clean_strip() {
        // Flat white strip, BGC white: stretchable.
        let slice = vec![Rgb([255, 255, 255]); 10];
        assert!(is_stretchable(&slice, SideState::GatedWith(Rgb([255, 255, 255])), 18, 5, true));
    }

    #[test]
    fn gated_with_bgc_rejects_a_shadow_sliver() {
        // Strip is mostly white but has a dark pixel near the edge: unstretchable.
        let mut slice = vec![Rgb([255, 255, 255]); 10];
        slice[9] = Rgb([120, 120, 120]); // edge-adjacent (hi-end), far from white.
        assert!(!is_stretchable(&slice, SideState::GatedWith(Rgb([255, 255, 255])), 18, 3, false));
    }

    #[test]
    fn a_fully_bled_row_fills_bgc_never_black() {
        // det3/det4: the subject runs to the border, so the gated side has no background to
        // stretch from — the outer segment is empty. The old code left that overhang black.
        // Source is 10 product pixels; the square (20 wide) overhangs 5 each side.
        let src = vec![Rgb([200, 20, 20]); 10];
        let mut out = vec![Rgb([0, 0, 0]); 20];
        let white = Rgb([255, 255, 255]);
        fill_line(
            &src,
            &mut out,
            -5, // square starts 5 before the source
            0,  // inner_lo (subject + safety) clamps to the border
            10, // inner_hi clamps to the far border
            FillSides { outer_lo: SideState::GatedWith(white), outer_hi: SideState::GatedWith(white) },
            0,
            4,
            18,
        );
        // Outer bands: near-white (BGC + tiny noise), and above all never pure black.
        for &x in &[0usize, 1, 2, 3, 4, 15, 16, 17, 18, 19] {
            let p = out[x];
            assert!(p.0 != [0, 0, 0], "overhang left black at index {x}");
            assert!(p.0.iter().all(|&c| c >= 240), "overhang not BGC-white at {x}: {p:?}");
        }
        // Product band is an untouched copy.
        for x in 5..15 {
            assert_eq!(out[x], Rgb([200, 20, 20]), "product band altered at {x}");
        }
    }

    #[test]
    fn clearance_keeps_the_silhouette_adjacent_strip_out_of_the_stretch() {
        // Gated-lo row: far background A, then a 3px strip B that is *within* BGC tolerance of A
        // (so is_stretchable still passes), then the product. With clearance = 3 the B strip is
        // pulled out of the source, so the stretched overhang is pure A — the 10/11 streak fix.
        let a_col = Rgb([250, 250, 250]);
        let b_col = Rgb([235, 235, 235]); // |Δ| = 15 ≤ tol 18: tolerated, but silhouette-adjacent.
        let p_col = Rgb([200, 20, 20]);
        let mut src = vec![a_col; 5];
        src.extend(vec![b_col; 3]);
        src.extend(vec![p_col; 2]);
        let mut out = vec![Rgb([0, 0, 0]); 12];
        fill_line(
            &src,
            &mut out,
            -2, // overhang 2px on the low side
            8,  // product starts at source index 8
            10, // product runs to the source end
            FillSides { outer_lo: SideState::GatedWith(a_col), outer_hi: SideState::Clean },
            3, // clearance pulls the 3px B strip out of the source
            4,
            18,
        );
        for x in 0..10 {
            assert_eq!(out[x], a_col, "stretched overhang picked up the B strip at {x}: {:?}", out[x]);
        }
        assert_eq!(out[10], p_col);
        assert_eq!(out[11], p_col);
    }

    #[test]
    fn a_dark_region_deeper_than_the_probe_does_not_streak_across_the_overhang() {
        // The 10/11 shape on the right (hi) side: the source is clean (≈white) for the 2 columns
        // against the edge, then a dark ~210 region deeper in. The edge probe (2 px) passes, so the
        // row stretches — but only the clean edge run must be sampled, or the dark region streaks
        // across the whole right overhang.
        let white = Rgb([255, 255, 255]);
        let edge = Rgb([252, 252, 252]); // within tol of white
        let dark = Rgb([210, 210, 210]); // |Δ| = 45 > tol: a real streak colour
        let prod = Rgb([200, 20, 20]);
        // src[0..2] product, src[2..8] dark, src[8..10] clean edge (index 9 = the border).
        let mut src = vec![prod; 2];
        src.extend(vec![dark; 6]);
        src.extend(vec![edge; 2]);
        let mut out = vec![Rgb([0, 0, 0]); 12];
        fill_line(
            &src,
            &mut out,
            0, // no overhang on the low side; product sits at the start
            0,
            2, // product is src[0..2]
            FillSides { outer_lo: SideState::Clean, outer_hi: SideState::GatedWith(white) },
            0, // clearance off, to isolate the clean-run effect
            2, // probe reaches only the 2 clean edge pixels
            18,
        );
        // Right overhang (output 2..12) must be clean background, never the dark streak.
        for x in 2..12 {
            assert!(
                out[x].0.iter().all(|&c| c >= 235),
                "dark region streaked into the overhang at {x}: {:?}",
                out[x]
            );
        }
    }

    #[test]
    fn inpaint_falls_to_bgc_when_the_product_edge_is_a_dark_shadow() {
        // The true 10/11 cause: the gated right side routes to inpaint (dark pixels in the probe),
        // and the product-band edge pixel is a dark hem shadow (~210). That must NOT be flat-filled
        // across the overhang — the fill falls to the BGC (white) instead.
        let white = Rgb([255, 255, 255]);
        // src[0..4] product band (last pixel is the 210 shadow); src[4..10] dark, so the hi side
        // is unstretchable and takes the inpaint branch.
        let src = vec![
            Rgb([255, 255, 255]),
            Rgb([255, 255, 255]),
            Rgb([255, 255, 255]),
            Rgb([210, 210, 210]),
            Rgb([210, 210, 210]),
            Rgb([210, 210, 210]),
            Rgb([210, 210, 210]),
            Rgb([210, 210, 210]),
            Rgb([210, 210, 210]),
            Rgb([210, 210, 210]),
        ];
        let mut out = vec![Rgb([0, 0, 0]); 12];
        fill_line(
            &src,
            &mut out,
            0,
            0,
            4, // product band is src[0..4]
            FillSides { outer_lo: SideState::Clean, outer_hi: SideState::GatedWith(white) },
            0,
            3, // probe sees the dark tail → unstretchable → inpaint
            18,
        );
        for x in 4..12 {
            assert!(
                out[x].0.iter().all(|&c| c >= 240),
                "overhang flat-filled the dark shadow at {x}: {:?}",
                out[x]
            );
        }
    }

    #[test]
    fn neighbour_row_borrow_fills_a_bled_row_from_its_clean_neighbour() {
        // 4-wide, 3-row mid. Row 1's lo segment (cols 0..2) is bled (flat white); rows 0 and 2 hold
        // clean background (100) there. After the borrow, row 1 cols 0..2 take the neighbour's 100.
        let mut mid = RgbImage::from_pixel(4, 3, Rgb([100, 100, 100]));
        mid.put_pixel(0, 1, Rgb([255, 255, 255]));
        mid.put_pixel(1, 1, Rgb([255, 255, 255]));
        let fills = vec![
            LineFill { lo_bled: false, hi_bled: false, a: 2, b: 4 },
            LineFill { lo_bled: true, hi_bled: false, a: 2, b: 4 },
            LineFill { lo_bled: false, hi_bled: false, a: 2, b: 4 },
        ];
        borrow_neighbour_rows(&mut mid, &fills);
        assert_eq!(*mid.get_pixel(0, 1), Rgb([100, 100, 100]), "bled col 0 borrowed");
        assert_eq!(*mid.get_pixel(1, 1), Rgb([100, 100, 100]), "bled col 1 borrowed");
        assert_eq!(*mid.get_pixel(3, 1), Rgb([100, 100, 100]), "non-bled col untouched");
    }

    #[test]
    fn neighbour_row_borrow_keeps_the_fill_when_the_whole_column_is_bled() {
        // Column 0 is bled on every row: no clean neighbour exists, so the BGC/white fill stays.
        let mut mid = RgbImage::new(2, 3);
        for y in 0..3 {
            mid.put_pixel(0, y, Rgb([255, 255, 255]));
            mid.put_pixel(1, y, Rgb([100, 100, 100]));
        }
        let fills = vec![LineFill { lo_bled: true, hi_bled: false, a: 1, b: 2 }; 3];
        borrow_neighbour_rows(&mut mid, &fills);
        for y in 0..3 {
            assert_eq!(*mid.get_pixel(0, y), Rgb([255, 255, 255]), "no neighbour → keep fill");
        }
    }

    #[test]
    fn a_fully_bled_row_with_no_bgc_fills_white_never_black() {
        // Same geometry, but a blind gated side (no reference BGC): the fallback tail is white,
        // still never black and never the product colour.
        let src = vec![Rgb([10, 90, 10]); 10];
        let mut out = vec![Rgb([0, 0, 0]); 20];
        fill_line(
            &src,
            &mut out,
            -5,
            0,
            10,
            FillSides { outer_lo: SideState::GatedBlind, outer_hi: SideState::GatedBlind },
            0,
            4,
            18,
        );
        for &x in &[0usize, 2, 4, 15, 17, 19] {
            let p = out[x];
            assert!(p.0 != [0, 0, 0], "overhang left black at index {x}");
            assert!(p.0.iter().all(|&c| c >= 240), "blind overhang not white at {x}: {p:?}");
        }
    }

    #[test]
    fn local_edge_bgc_recovers_a_flat_strip_when_global_is_missing() {
        // A 20x10 image with its top strip (first 2 rows) flat white: local sampler should
        // recover white as the per-edge BGC.
        let mut img = RgbImage::from_pixel(20, 10, Rgb([30, 30, 30]));
        for y in 0..2 {
            for x in 0..20 {
                img.put_pixel(x, y, Rgb([255, 255, 255]));
            }
        }
        let region = Rect { x: 0, y: 0, w: 20, h: 10 };
        let bgc = local_edge_bgc(&img, &region, Edge::Top, 2);
        assert_eq!(bgc, Some(Rgb([255, 255, 255])));
    }

    #[test]
    fn local_edge_bgc_rejects_a_textured_strip() {
        // Half white, half dark along the top edge: span exceeds R1_LOCAL_BGC_SPAN_MAX, no local BGC.
        let mut img = RgbImage::new(20, 10);
        for y in 0..10 {
            for x in 0..20 {
                let v = if x < 10 { 255 } else { 10 };
                img.put_pixel(x, y, Rgb([v, v, v]));
            }
        }
        let region = Rect { x: 0, y: 0, w: 20, h: 10 };
        assert!(local_edge_bgc(&img, &region, Edge::Top, 2).is_none());
    }
}
