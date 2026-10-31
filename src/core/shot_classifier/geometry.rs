//! Edge-relative coordinate handling. Every per-edge algorithm (border-band
//! scanning, background-color sampling, the trimap ring) is written once,
//! for the top edge, against [`Grid`]; [`EdgeView`] presents the other three
//! edges as the top one by transposing/flipping coordinates. Nothing
//! edge-specific lives outside this file.
//! See docs/architecture/02-modules-shot-classifier.md.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

impl Edge {
    pub const ALL: [Edge; 4] = [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left];
}

/// A read-only 2D grid of values addressed as `(x, y)`, `(0, 0)` at the
/// top-left. Implemented for [`super::segmentation::Mask`] directly and
/// for `image::RgbImage` via a thin adapter, so both can be run through
/// the same edge-relative algorithms.
pub trait Grid {
    type Item: Copy;
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn get(&self, x: u32, y: u32) -> Self::Item;
}

/// Presents `inner` rotated/flipped so that `edge` reads as the top edge.
pub struct EdgeView<'a, G: Grid> {
    inner: &'a G,
    edge: Edge,
}

impl<'a, G: Grid> EdgeView<'a, G> {
    pub fn new(inner: &'a G, edge: Edge) -> Self {
        Self { inner, edge }
    }

    pub fn edge(&self) -> Edge {
        self.edge
    }

    /// Maps a canonical (top-edge-relative) coordinate back to the
    /// underlying grid's own coordinate space.
    pub fn to_original(&self, cx: u32, cy: u32) -> (u32, u32) {
        let (w, h) = (self.inner.width(), self.inner.height());
        match self.edge {
            Edge::Top => (cx, cy),
            Edge::Bottom => (cx, h.saturating_sub(1).saturating_sub(cy)),
            Edge::Left => (cy, cx),
            Edge::Right => (w.saturating_sub(1).saturating_sub(cy), cx),
        }
    }
}

impl<'a, G: Grid> Grid for EdgeView<'a, G> {
    type Item = G::Item;

    fn width(&self) -> u32 {
        match self.edge {
            Edge::Top | Edge::Bottom => self.inner.width(),
            Edge::Left | Edge::Right => self.inner.height(),
        }
    }

    fn height(&self) -> u32 {
        match self.edge {
            Edge::Top | Edge::Bottom => self.inner.height(),
            Edge::Left | Edge::Right => self.inner.width(),
        }
    }

    fn get(&self, cx: u32, cy: u32) -> Self::Item {
        let (ox, oy) = self.to_original(cx, cy);
        self.inner.get(ox, oy)
    }
}

/// An axis-aligned pixel rectangle, `[x, x+w) x [y, y+h)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn from_bounds(x0: u32, y0: u32, x1: u32, y1: u32) -> Self {
        Rect {
            x: x0,
            y: y0,
            w: x1.saturating_sub(x0) + 1,
            h: y1.saturating_sub(y0) + 1,
        }
    }

    /// Grows the rect by `px` on every side, clamped to `[0, bound_w) x [0, bound_h)`.
    pub fn expanded(&self, px: u32, bound_w: u32, bound_h: u32) -> Rect {
        let x0 = self.x.saturating_sub(px);
        let y0 = self.y.saturating_sub(px);
        let x1 = (self.x + self.w + px).min(bound_w);
        let y1 = (self.y + self.h + px).min(bound_h);
        Rect {
            x: x0,
            y: y0,
            w: x1.saturating_sub(x0),
            h: y1.saturating_sub(y0),
        }
    }

    /// Rescales from one coordinate space to another (e.g. working
    /// resolution to full original resolution) by independent x/y factors.
    pub fn scaled(&self, fx: f32, fy: f32) -> Rect {
        Rect {
            x: (self.x as f32 * fx).floor() as u32,
            y: (self.y as f32 * fy).floor() as u32,
            w: ((self.w as f32 * fx).ceil() as u32).max(1),
            h: ((self.h as f32 * fy).ceil() as u32).max(1),
        }
    }
}

/// Prefix sums over a boolean grid, so "is any cell set inside this
/// rectangle?" costs four lookups instead of scanning the rectangle.
///
/// Exists for [`super::sampling`]'s per-pixel mask window. Building the
/// table is one pass over the rows the strip can actually reach; every
/// query after it is O(1). See
/// docs/architecture/modules/core-shot-classifier-sampling.md.
pub(crate) struct BoolSummedArea {
    width: u32,
    height: u32,
    /// `(width + 1) * (height + 1)`, row-major. `sums[(y+1)*stride + x+1]`
    /// counts the set cells in `[0, x] x [0, y]`; the zero row and column
    /// are the usual padding that makes the four-corner query branchless.
    sums: Vec<u32>,
}

impl BoolSummedArea {
    /// Builds the table over the top `rows` rows of `grid` — the caller
    /// knows how deep its queries can reach, and rows below that are
    /// never read.
    pub(crate) fn new<G: Grid<Item = bool>>(grid: &G, rows: u32) -> Self {
        let width = grid.width();
        let height = rows.min(grid.height());
        let stride = width as usize + 1;
        let mut sums = vec![0u32; stride * (height as usize + 1)];
        for y in 0..height as usize {
            let mut run = 0u32;
            for x in 0..width as usize {
                run += u32::from(grid.get(x as u32, y as u32));
                sums[(y + 1) * stride + x + 1] = sums[y * stride + x + 1] + run;
            }
        }
        BoolSummedArea { width, height, sums }
    }

    /// Is any cell set inside the inclusive rectangle `[x0, x1] x [y0, y1]`?
    ///
    /// The rectangle is clamped to the table, and one that falls entirely
    /// outside it reads as empty — same answer the scan it replaces gave
    /// for an empty coordinate range.
    pub(crate) fn any_in(&self, x0: u32, y0: u32, x1: u32, y1: u32) -> bool {
        if self.width == 0 || self.height == 0 {
            return false;
        }
        let x1 = x1.min(self.width - 1);
        let y1 = y1.min(self.height - 1);
        if x0 > x1 || y0 > y1 {
            return false;
        }
        let stride = self.width as usize + 1;
        let (x0, y0) = (x0 as usize, y0 as usize);
        let (x1, y1) = (x1 as usize + 1, y1 as usize + 1);
        let count = self.sums[y1 * stride + x1] + self.sums[y0 * stride + x0]
            - self.sums[y0 * stride + x1]
            - self.sums[y1 * stride + x0];
        count > 0
    }
}
