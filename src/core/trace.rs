//! Debug trace: save every intermediate image of a run, so a bad result can be followed back to the
//! stage that caused it.
//!
//! **Turning it on.** The same marker the exporter looks for: a file named `PPRONI_DEBUG_TAGS` next
//! to the running executable. One check per process, cached. With no marker, every function here
//! is inert. Debug and release both honour it — same scope as the exporter's filename tags and
//! `<stem>_segmask.jpg`.
//!
//! **What gets written**, per image, into the output image's folder (next to the exporter's
//! `<stem>_segmask.jpg`):
//! * `<stem>  -  <NN>.jpg` — one file per stage; `NN` comes from [`stage`]. Numbers are fixed per
//!   stage, not counted per run, so `10` is the model input on every image and a skipped stage
//!   leaves a gap rather than renumbering its neighbours.
//! * `<stem>  -  log.txt` — the stages that were written, in order, plus the numbers behind each
//!   decision (mask coverage, which edges the gate flagged, the graze ratio, the route and why).
//!
//! **How to add a stage.** Pick the next free number in [`stage`], then call [`image`] where the
//! picture exists, passing a closure that builds it. The closure only runs while tracing, so
//! building an overlay costs nothing in a normal run.

use super::shot_classifier::geometry::Rect;
use super::shot_classifier::{Edge, Mask};
use image::{Rgb, RgbImage};
use std::path::Path;
use std::sync::OnceLock;

/// Stage numbers. A group takes a block of ten so a stage can be added without renumbering.
pub mod stage {
    /// Decoded, EXIF-rotated, alpha flattened onto white. Full resolution.
    pub const ORIGINAL: u8 = 1;
    /// The working-resolution copy the model runs on. Kept for `examples/run_dir.rs`; the
    /// pipeline no longer traces it.
    pub const WORKING: u8 = 3;
    /// The primary BiRefNet model: four stages — the model input (`PRIMARY + 0`), the ImageNet-
    /// normalised input (`+1`), the raw logits (`+2`), and the mask at working resolution (`+5`).
    /// Numbers kept so earlier offsets stay recognisable in the log.
    pub const PRIMARY: u8 = 10;

    /// Plan view on the original: subject box (cyan), R1's planned square (yellow), grey =
    /// invented area, plus semi-transparent edge bars for the gate/verdict (red = touches,
    /// orange = gated but cleared). Emitted for every image; both the subject and the square
    /// are optional, so a missing verdict or an impossible plan still produces a useful picture.
    pub const R1_PLAN: u8 = 90;
}

/// Labels for the stages of one BiRefNet run, as offsets from [`stage::PRIMARY`]. Indices 3, 4
/// and 6 are no longer traced; the empty strings keep the array indexable by the raw offsets.
pub const MODEL_STEPS: [&str; 7] = [
    "",
    "Input: Color-balanced and trimmed OG",
    "Output: Raw logits",
    "",
    "",
    "Mask resampled to:",
    "",
];

/// The share of the frame a mask covers, as a percentage.
pub fn coverage_pct(mask: &Mask) -> f64 {
    let fg = mask.data.iter().filter(|&&v| v > 0).count();
    100.0 * fg as f64 / (mask.data.len().max(1)) as f64
}

// ---- the switch -------------------------------------------------------------------------------

/// Held for the length of one image's run. Dropping it writes that image's `log.txt`.
pub struct Session {
    owns: bool,
}

mod live {
    use std::cell::RefCell;
    use std::path::PathBuf;

    pub struct State {
        pub dir: PathBuf,
        pub stem: String,
        pub log: Vec<String>,
    }

    // One image runs start to finish on one thread (rayon fans out across images, not inside one),
    // so a thread-local is the right scope: parallel images never mix their logs.
    thread_local! {
        pub static CURRENT: RefCell<Option<State>> = const { RefCell::new(None) };
    }
}

/// `true` when a `PPRONI_DEBUG_TAGS` file sits next to the running executable. One lookup per
/// process, cached. The exporter's filename tags and the trace both read this.
pub fn debug_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("PPRONI_DEBUG_TAGS")))
            .is_some_and(|p| p.is_file())
    })
}

/// Start tracing one image. Inert unless the `PPRONI_DEBUG_TAGS` marker is next to the exe. Stage
/// images and `log.txt` are written into `output.parent()`, using `output`'s file stem — the same
/// folder and stem the exporter uses for `<stem>_segmask.jpg`.
pub fn begin(output: &Path) -> Session {
    if !debug_enabled() {
        return Session { owns: false };
    }
    let already = live::CURRENT.with(|c| c.borrow().is_some());
    if already {
        return Session { owns: false };
    }
    let dir = output.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| std::path::PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    let stem = output.file_stem().and_then(|s| s.to_str()).unwrap_or("image").to_owned();
    live::CURRENT.with(|c| *c.borrow_mut() = Some(live::State { dir, stem, log: Vec::new() }));
    Session { owns: true }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.owns {
            if let Some(state) = live::CURRENT.with(|c| c.borrow_mut().take()) {
                let path = state.dir.join(format!("{}  -  log.txt", state.stem));
                let _ = std::fs::write(path, state.log.join("\n") + "\n");
            }
        }
    }
}

/// Is a trace running on this thread? For callers that would otherwise build something expensive
/// just to hand it to [`image`] or [`note`] — normally the closures make this unnecessary.
#[allow(dead_code)]
pub fn active() -> bool {
    live::CURRENT.with(|c| c.borrow().is_some())
}

/// Save one stage as `<stem>  -  <index>.jpg`. `make` runs only while tracing.
pub fn image(index: u8, label: &str, make: impl FnOnce() -> RgbImage) {
    live::CURRENT.with(|c| {
        // Take the state out while `make` runs: it may itself call back into this module.
        let Some(mut state) = c.borrow_mut().take() else { return };
        let img = make();
        let path = state.dir.join(format!("{}  -  {index:02}.jpg", state.stem));
        let saved = super::exporter::save::save_jpeg_srgb(&img, &path, 90);
        state.log.push(format!(
            "{index:02}  {}{label}  ({}x{}){}",
            group(index),
            img.width(),
            img.height(),
            if saved.is_err() { "  [SAVE FAILED]" } else { "" }
        ));
        *c.borrow_mut() = Some(state);
    });
}

/// Add a line to this image's `log.txt`. `make` runs only while tracing.
pub fn note(make: impl FnOnce() -> String) {
    live::CURRENT.with(|c| {
        let text = {
            if c.borrow().is_none() {
                return;
            }
            make()
        };
        if let Some(state) = c.borrow_mut().as_mut() {
            state.log.push(format!("      {text}"));
        }
    });
}

/// Which block a stage number belongs to, as a log prefix.
fn group(index: u8) -> String {
    match index {
        10..=19 => "[primary model] ".into(),
        _ => String::new(),
    }
}

// ---- picture builders -------------------------------------------------------------------------
//
// Plain helpers that turn the pipeline's own data into something viewable. They are only ever
// called from inside the closures above, so they cost nothing in a normal run.

/// A mask as a black-and-white picture.
pub fn mask_image(mask: &Mask) -> RgbImage {
    let mut img = RgbImage::new(mask.width.max(1), mask.height.max(1));
    for (px, &v) in img.pixels_mut().zip(&mask.data) {
        *px = Rgb([v, v, v]);
    }
    img
}

/// Greyscale from raw values, stretched so the smallest becomes black and the largest white.
/// (Logits have no fixed range, so a fixed scale would leave most images unreadable.)
pub fn stretched_gray(values: &[f32], w: u32, h: u32) -> RgbImage {
    let (lo, hi) = values.iter().fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
    let span = (hi - lo).max(f32::EPSILON);
    let mut img = RgbImage::new(w, h);
    for (px, &v) in img.pixels_mut().zip(values) {
        let g = (((v - lo) / span) * 255.0).round() as u8;
        *px = Rgb([g, g, g]);
    }
    img
}

/// Greyscale from values already in 0..=1.
pub fn unit_gray(values: &[f32], w: u32, h: u32) -> RgbImage {
    let mut img = RgbImage::new(w, h);
    for (px, &v) in img.pixels_mut().zip(values) {
        let g = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        *px = Rgb([g, g, g]);
    }
    img
}

/// A three-channel planar (CHW) tensor as a colour picture, stretched over its whole range.
pub fn stretched_chw(chw: &[f32], w: u32, h: u32) -> RgbImage {
    let plane = (w * h) as usize;
    let (lo, hi) = chw.iter().fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
    let span = (hi - lo).max(f32::EPSILON);
    let to_u8 = |v: f32| (((v - lo) / span) * 255.0).round() as u8;
    let mut img = RgbImage::new(w, h);
    for (i, px) in img.pixels_mut().enumerate() {
        *px = Rgb([to_u8(chw[i]), to_u8(chw[plane + i]), to_u8(chw[2 * plane + i])]);
    }
    img
}

/// `base` with everything outside the mask dimmed and tinted blue, so the silhouette can be judged
/// against the picture. The subject keeps its own colours — tinting it instead hid red products.
pub fn tint(base: &RgbImage, mask: &Mask) -> RgbImage {
    let mut out = base.clone();
    for y in 0..out.height() {
        for x in 0..out.width() {
            if !mask.get(x * mask.width / out.width().max(1), y * mask.height / out.height().max(1)) {
                let Rgb([r, g, b]) = *out.get_pixel(x, y);
                out.put_pixel(x, y, Rgb([r / 3, g / 3, b / 3 + 110]));
            }
        }
    }
    out
}

/// A hollow rectangle, clipped to the picture. `(x, y)` may be negative.
pub fn outline(img: &mut RgbImage, x: i64, y: i64, w: u32, h: u32, colour: [u8; 3], thick: u32) {
    let (iw, ih) = (img.width() as i64, img.height() as i64);
    let mut paint = |px: i64, py: i64| {
        if px >= 0 && py >= 0 && px < iw && py < ih {
            img.put_pixel(px as u32, py as u32, Rgb(colour));
        }
    };
    let (w, h, t) = (w as i64, h as i64, thick as i64);
    for d in 0..t {
        for i in 0..w {
            paint(x + i, y + d);
            paint(x + i, y + h - 1 - d);
        }
        for j in 0..h {
            paint(x + d, y + j);
            paint(x + w - 1 - d, y + j);
        }
    }
}

/// The pass-1 gate: the mask tinted, and each edge's band outlined — red if the gate found
/// foreground in it (so pass 2 runs), green if not.
pub fn gate_view(working: &RgbImage, mask: &Mask, margin_px: u32, near: &[Edge]) -> RgbImage {
    let mut img = tint(working, mask);
    let (w, h) = (img.width(), img.height());
    let m = margin_px.min(w).min(h);
    for edge in Edge::ALL {
        let colour = if near.contains(&edge) { [255, 0, 0] } else { [0, 200, 0] };
        let (x, y, bw, bh) = match edge {
            Edge::Top => (0, 0, w, m),
            Edge::Bottom => (0, h - m, w, m),
            Edge::Left => (0, 0, m, h),
            Edge::Right => (w - m, 0, m, h),
        };
        outline(&mut img, x as i64, y as i64, bw, bh, colour, 2);
    }
    img
}

/// The edge verdict: a thick bar along each edge — red where the subject touches it, orange where
/// the gate flagged it but refinement said no, nothing where the gate never looked.
pub fn verdict_view(working: &RgbImage, mask: &Mask, near: &[Edge], touching: &[Edge]) -> RgbImage {
    let mut img = tint(working, mask);
    let (w, h) = (img.width(), img.height());
    let bar = (w.max(h) / 100).max(4);
    for edge in near {
        let colour = if touching.contains(edge) { [255, 0, 0] } else { [255, 165, 0] };
        let (x, y, bw, bh) = match edge {
            Edge::Top => (0, 0, w, bar),
            Edge::Bottom => (0, h.saturating_sub(bar), w, bar),
            Edge::Left => (0, 0, bar, h),
            Edge::Right => (w.saturating_sub(bar), 0, bar, h),
        };
        for py in y..(y + bh).min(h) {
            for px in x..(x + bw).min(w) {
                img.put_pixel(px, py, Rgb(colour));
            }
        }
    }
    img
}

/// Debug view on the original: subject box (cyan) and R1's planned square (yellow), on a grey
/// canvas padded wherever the square reaches past the picture — the grey is exactly what R1 has
/// to invent by stretching. Both are optional: a missing subject skips the cyan outline, a
/// missing plan skips the yellow square and the grey padding. Semi-transparent red/orange bars
/// along each gated edge sit under the outlines (red = subject touches, orange = gate flagged
/// but refinement cleared). `strips` adds a one-pixel green/magenta strip along each gated edge
/// showing R1's per-column stretch/inpaint call — green where the column will resample, magenta
/// where it will fall into the inpainter.
pub fn plan_view(
    original: &RgbImage,
    subject: Option<Rect>,
    plan: Option<(i64, i64, u32)>,
    gated: &[Edge],
    touching: &[Edge],
    strips: &[(Edge, Vec<bool>)],
) -> RgbImage {
    let (w, h) = (original.width() as i64, original.height() as i64);
    let (pad_l, pad_t, pad_r, pad_b) = match plan {
        Some((sq_x, sq_y, side)) => (
            (-sq_x).max(0),
            (-sq_y).max(0),
            (sq_x + side as i64 - w).max(0),
            (sq_y + side as i64 - h).max(0),
        ),
        None => (0, 0, 0, 0),
    };

    let mut canvas = RgbImage::from_pixel(
        (w + pad_l + pad_r) as u32,
        (h + pad_t + pad_b) as u32,
        Rgb([128, 128, 128]),
    );
    image::imageops::replace(&mut canvas, original, pad_l, pad_t);

    let thick = ((w.max(h) / 400).max(2)) as u32;
    // Semi-transparent edge bars drawn first so the subject/square outlines sit above them.
    overlay_edge_bars(&mut canvas, original.width(), original.height(), pad_l as u32, pad_t as u32, gated, touching);
    overlay_stretch_strips(&mut canvas, original.width(), original.height(), pad_l as u32, pad_t as u32, strips);
    if let Some(s) = subject {
        outline(&mut canvas, pad_l + s.x as i64, pad_t + s.y as i64, s.w, s.h, [0, 255, 255], thick);
    }
    if let Some((sq_x, sq_y, side)) = plan {
        outline(&mut canvas, pad_l + sq_x, pad_t + sq_y, side, side, [255, 255, 0], thick);
    }
    canvas
}

/// One-pixel strip per gated edge: green where R1's classifier judges the column stretchable,
/// magenta where it does not. Drawn one pixel inward from the edge so the orange bar underneath
/// stays visible — the strip is the per-column call *inside* that band.
fn overlay_stretch_strips(
    canvas: &mut RgbImage,
    w: u32,
    h: u32,
    pad_x: u32,
    pad_y: u32,
    strips: &[(Edge, Vec<bool>)],
) {
    const GREEN: [u8; 3] = [0, 220, 0];
    const MAGENTA: [u8; 3] = [255, 0, 255];
    for (edge, calls) in strips {
        for (i, &stretchable) in calls.iter().enumerate() {
            let colour = if stretchable { GREEN } else { MAGENTA };
            let (x, y) = match edge {
                Edge::Top => (i as u32, 1u32),
                Edge::Bottom => (i as u32, h.saturating_sub(2)),
                Edge::Left => (1u32, i as u32),
                Edge::Right => (w.saturating_sub(2), i as u32),
            };
            if x < w && y < h {
                canvas.put_pixel(pad_x + x, pad_y + y, Rgb(colour));
            }
        }
    }
}

/// Blends a bar colour at 50% alpha along each gated edge of the original-image region on
/// `canvas` (the area at `(pad_x, pad_y) .. +(w, h)`). Red for an edge the subject touches,
/// orange for one the gate flagged but refinement cleared.
fn overlay_edge_bars(
    canvas: &mut RgbImage,
    w: u32,
    h: u32,
    pad_x: u32,
    pad_y: u32,
    gated: &[Edge],
    touching: &[Edge],
) {
    let bar = (w.max(h) / 100).max(4);
    for edge in gated {
        let colour = if touching.contains(edge) { [255u8, 0, 0] } else { [255, 165, 0] };
        let (x0, y0, bw, bh) = match edge {
            Edge::Top => (0u32, 0u32, w, bar),
            Edge::Bottom => (0, h.saturating_sub(bar), w, bar),
            Edge::Left => (0, 0, bar, h),
            Edge::Right => (w.saturating_sub(bar), 0, bar, h),
        };
        for py in y0..(y0 + bh).min(h) {
            for px in x0..(x0 + bw).min(w) {
                let dst = canvas.get_pixel_mut(pad_x + px, pad_y + py);
                dst[0] = ((dst[0] as u16 + colour[0] as u16) / 2) as u8;
                dst[1] = ((dst[1] as u16 + colour[1] as u16) / 2) as u8;
                dst[2] = ((dst[2] as u16 + colour[2] as u16) / 2) as u8;
            }
        }
    }
}
