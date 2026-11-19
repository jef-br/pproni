//! R1: the lines of an edge the stretch must not use because the product grazes
//! that edge there. A line is *blocked* when the mask has a pixel within `GATE_MARGIN_PX` of the
//! border on it (the pixels that made the edge gate flag the edge), plus `GATE_MARGIN_PX` either side
//! along the edge. [`super::compose`] leaves a blocked line empty and fills it afterwards from the
//! nearest unblocked lines either side. See docs/work-state/2026-11-13-flat-edge-gradients.md.

use image::{Rgb, RgbImage};

use super::super::super::super::config;
use crate::core::shot_classifier::{Edge, Mask};

/// Per edge, one flag per line of the *original* image (rows for left and right, columns for top and
/// bottom). An empty list means the edge blocks nothing.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Blocked {
    pub top: Vec<bool>,
    pub bottom: Vec<bool>,
    pub left: Vec<bool>,
    pub right: Vec<bool>,
    /// Per edge: `true` when its blocked lines go to the patch fill ([`super::patch`]), `false` when a
    /// plain colour or ramp fills them. Set by [`Blocked::choose_fill`], the second test.
    pub patch: Edges,
}

/// One flag per edge.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Edges {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

impl Blocked {
    /// Does any edge have a blocked line?
    pub fn any(&self) -> bool {
        [&self.top, &self.bottom, &self.left, &self.right].iter().any(|v| v.iter().any(|&b| b))
    }

    /// The second test, per edge that has blocked lines. The edge is flat, and its lines get a plain
    /// colour or ramp, when a background colour (`bgc`) was found, or when the outermost pixel line of `src`
    /// (its safe lines only) spans at most `R1_GRAZE_FLAT_TOLERANCE` levels. Any other edge, with variance,
    /// gets the patch fill. See docs/work-state/2026-11-13-flat-edge-gradients.md.
    pub fn choose_fill(&mut self, src: &RgbImage, bgc: Option<Rgb<u8>>) {
        let decide = |blocked: &[bool], edge: Edge| -> bool {
            blocked.iter().any(|&b| b) && bgc.is_none() && !edge_is_flat(src, edge, blocked)
        };
        self.patch = Edges {
            top: decide(&self.top, Edge::Top),
            bottom: decide(&self.bottom, Edge::Bottom),
            left: decide(&self.left, Edge::Left),
            right: decide(&self.right, Edge::Right),
        };
    }
}

/// Does the outermost pixel line of `src` along `edge`, over the lines not blocked, vary by at most the
/// tolerance? The line is smoothed over 5 pixels first so JPEG noise does not count, then the span from
/// its 5th to its 95th percentile is read per channel. Too few safe lines to tell: not flat.
fn edge_is_flat(src: &RgbImage, edge: Edge, blocked: &[bool]) -> bool {
    let (w, h) = src.dimensions();
    let line: Vec<[u8; 3]> = match edge {
        Edge::Top => (0..w).map(|x| src.get_pixel(x, 0).0).collect(),
        Edge::Bottom => (0..w).map(|x| src.get_pixel(x, h - 1).0).collect(),
        Edge::Left => (0..h).map(|y| src.get_pixel(0, y).0).collect(),
        Edge::Right => (0..h).map(|y| src.get_pixel(w - 1, y).0).collect(),
    };
    let safe: Vec<[u8; 3]> =
        line.iter().enumerate().filter(|(i, _)| !blocked.get(*i).copied().unwrap_or(false)).map(|(_, p)| *p).collect();
    if safe.len() < 10 {
        return false;
    }
    (0..3).all(|c| {
        let line: Vec<u8> = safe.iter().map(|p| p[c]).collect();
        let mut smooth: Vec<u8> = (0..line.len())
            .map(|i| {
                let mut win: Vec<u8> = line[i.saturating_sub(2)..(i + 3).min(line.len())].to_vec();
                win.sort_unstable();
                win[win.len() / 2]
            })
            .collect();
        smooth.sort_unstable();
        let (lo, hi) = (smooth[smooth.len() * 5 / 100], smooth[(smooth.len() * 95 / 100).min(smooth.len() - 1)]);
        (hi - lo) as u32 <= config::R1_GRAZE_FLAT_TOLERANCE
    })
}

/// The blocked lines of every edge in `flagged` (the edges pass 1 of the classifier flagged).
/// `orig` is the original image's size; the mask is read at its own resolution.
pub fn blocked_lines(mask: &Mask, flagged: &[Edge], orig: (u32, u32)) -> Blocked {
    let mut out = Blocked::default();
    for &edge in flagged {
        let (lines, orig_len) = match edge {
            Edge::Top | Edge::Bottom => (culprits(mask, edge), orig.0),
            Edge::Left | Edge::Right => (culprits(mask, edge), orig.1),
        };
        let lines = widen(&lines, config::GATE_MARGIN_PX as usize);
        let on_original = to_original(&lines, orig_len as usize);
        match edge {
            Edge::Top => out.top = on_original,
            Edge::Bottom => out.bottom = on_original,
            Edge::Left => out.left = on_original,
            Edge::Right => out.right = on_original,
        }
    }
    out
}

/// Per line of the mask along `edge`: does the mask have a pixel within `GATE_MARGIN_PX` of the border?
fn culprits(mask: &Mask, edge: Edge) -> Vec<bool> {
    let (w, h) = (mask.width as usize, mask.height as usize);
    let band = config::GATE_MARGIN_PX as usize;
    let on = |x: usize, y: usize| mask.data[y * w + x] != 0;
    match edge {
        Edge::Top => (0..w).map(|x| (0..band.min(h)).any(|y| on(x, y))).collect(),
        Edge::Bottom => (0..w).map(|x| (h.saturating_sub(band)..h).any(|y| on(x, y))).collect(),
        Edge::Left => (0..h).map(|y| (0..band.min(w)).any(|x| on(x, y))).collect(),
        Edge::Right => (0..h).map(|y| (w.saturating_sub(band)..w).any(|x| on(x, y))).collect(),
    }
}

/// Every flag spreads `by` lines either way.
fn widen(lines: &[bool], by: usize) -> Vec<bool> {
    let n = lines.len();
    let mut out = vec![false; n];
    for (i, _) in lines.iter().enumerate().filter(|(_, &v)| v) {
        for o in out.iter_mut().take((i + by + 1).min(n)).skip(i.saturating_sub(by)) {
            *o = true;
        }
    }
    out
}

/// The same flags on the original's lines: each original line reads the mask line it covers, the way
/// `Silhouette` does.
fn to_original(lines: &[bool], orig_len: usize) -> Vec<bool> {
    if lines.is_empty() || orig_len == 0 {
        return vec![false; orig_len];
    }
    let f = lines.len() as f32 / orig_len as f32;
    (0..orig_len).map(|o| lines[(((o as f32 + 0.5) * f) as usize).min(lines.len() - 1)]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask_with(w: u32, h: u32, rect: (u32, u32, u32, u32)) -> Mask {
        let mut m = Mask { width: w, height: h, data: vec![0u8; (w * h) as usize] };
        for y in rect.1..rect.1 + rect.3 {
            for x in rect.0..rect.0 + rect.2 {
                m.data[(y * w + x) as usize] = 255;
            }
        }
        m
    }

    #[test]
    fn a_product_two_pixels_from_the_top_blocks_its_columns_and_the_margin_round_them() {
        let m = mask_with(200, 200, (60, 2, 80, 150));
        let b = blocked_lines(&m, &[Edge::Top], (200, 200));
        let margin = config::GATE_MARGIN_PX as usize;
        for x in 0..200usize {
            let near = x + margin >= 60 && x < 140 + margin;
            assert_eq!(b.top[x], near, "column {x}");
        }
        assert!(b.bottom.is_empty() && b.left.is_empty() && b.right.is_empty(), "other edges block nothing");
    }

    #[test]
    fn a_product_far_from_the_border_blocks_nothing() {
        let m = mask_with(200, 200, (60, 60, 80, 80));
        let b = blocked_lines(&m, &[Edge::Top, Edge::Left], (200, 200));
        assert!(b.top.iter().chain(b.left.iter()).all(|&v| !v));
    }

    #[test]
    fn a_right_graze_blocks_rows_and_maps_to_a_bigger_original() {
        // Rows 50..100 reach the right band; the original is twice as tall as the mask.
        let m = mask_with(100, 200, (50, 50, 49, 50));
        let b = blocked_lines(&m, &[Edge::Right], (100, 400));
        let margin = config::GATE_MARGIN_PX as usize;
        assert_eq!(b.right.len(), 400);
        assert!(b.right[2 * 50] && b.right[2 * 99 + 1], "the culprit rows are blocked");
        assert!(b.right[2 * (50 - margin)] && !b.right[2 * (50 - margin - 1)], "margin above");
        assert!(b.right[2 * (99 + margin) + 1] && !b.right[2 * (100 + margin) + 1], "margin below");
    }

    #[test]
    fn a_flat_edge_gets_the_plain_fill_and_a_varying_one_gets_the_patch_fill() {
        // Top edge blocked on columns 40..60. Row 0 is flat on the left half of the safe lines and
        // wrinkled (a 20-level swing) on the right half for the second picture.
        let mut blocked = Blocked { top: (0..100).map(|x| (40..60).contains(&x)).collect(), ..Default::default() };
        let flat = RgbImage::from_pixel(100, 100, Rgb([200, 200, 200]));
        blocked.choose_fill(&flat, None);
        assert!(!blocked.patch.top, "no variance: plain fill");

        let mut wrinkled = flat.clone();
        for x in 0..100u32 {
            let v = 200 + 20 * ((x / 7) % 2) as u8;
            wrinkled.put_pixel(x, 0, Rgb([v, v, v]));
        }
        blocked.choose_fill(&wrinkled, None);
        assert!(blocked.patch.top, "variance: patch fill");
        blocked.choose_fill(&wrinkled, Some(Rgb([200, 200, 200])));
        assert!(!blocked.patch.top, "a background colour was found: plain fill");
        assert!(!blocked.patch.bottom && !blocked.patch.left && !blocked.patch.right, "edges without blocked lines never patch");
    }
}
