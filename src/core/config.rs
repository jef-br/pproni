//! Compile-time tuning constants.

/// JPEG quality for the saved output.
pub const JPEG_QUALITY: u8 = 95;

/// Gates the headcut question. If false, Pproni runs without headcuts. Set to true while developing
/// to enable headcuts.
pub const PERMIT_HEADCUTS:bool = false;

// ---- Shot classification ----------------------------------------------------------------------
//
// `GATE_MARGIN_PX` and `REFINE_NARROW_BAND_PX` are in working-resolution pixels, so they are coupled
// to `WORKING_SIZE` — change one and the others shift meaning.
// See docs/architecture/01-why-two-resolutions.md.

/// Longest side of the working-resolution copy the segmentation model runs on.
pub const WORKING_SIZE: u32 = 1024;

/// Pass 1 and 2 band width: an edge is worth refining when foreground appears within this many
/// pixels of it, and it is how far the refinement looks.
pub const GATE_MARGIN_PX: u32 = 20;

/// Pass-2 graze test, narrow band.
pub const REFINE_NARROW_BAND_PX: u32 = 5;

/// Pass-2 graze test threshold.
/// The graze test compares 2 bands:
/// A/B = <band sized to 'GATE_MARGIN_PX'> / <band sized to 'REFINE_NARROW_BAND_PX'>
/// A/B > GRAZE_RATIO_MAX → R1 → bands have the same productwidth so the direction is deemed to intersect the image edge, touching.
/// A/B <= GRAZE_RATIO_MAX → R2 → indicates a changing direction, is interpreted as the product grazing the image edge, not touching.
pub const GRAZE_RATIO_MAX: f32 = 0.80;

/// Below this fraction of the frame, a mask is not a subject — unless its longest side reaches
/// `MASK_MIN_LONGEST_SIDE_PX`. See `shotcode::mask_too_small`. JB-calibrated.
pub const MASK_MIN_COVERAGE: f64 = 0.05;

/// A mask under `MASK_MIN_COVERAGE` is still a subject when its longest side, in original pixels,
/// is at least this long. `MIN_SIZE` less R1's margin: 800 − 8.4% = 732.8, rounded to 733; derived
/// so it follows `MIN_SIZE` and `R1_MARGIN`. The user's rule.
/// See docs/work-state/2026-10-29-routing-in-shot-classifier.md.
pub const MASK_MIN_LONGEST_SIDE_PX: u32 = (MIN_SIZE as f64 * (1.0 - R1_MARGIN as f64) + 0.5) as u32;

// ---- Optimiser trim ---------------------------------------------------------------------------
//
// The optimiser's backdrop trim (`preprocessor::optimizer`), so the 512px model sees the product
// larger. Working-resolution pixels or shares. See docs/architecture/01-why-two-resolutions.md.

/// Backdrop samples per edge of the frame, evenly spaced from the edge's start (4: 0, 25, 50 and
/// 75%; 100% is the next edge's 0%).
pub const TRIM_SAMPLES_PER_EDGE: u32 = 4;

/// Outlier drop on the border samples: kept when the L1 colour distance to the per-channel median
/// (`|ΔR|+|ΔG|+|ΔB|`, 0 to 765) is at most `TRIM_OUTLIER_FLOOR + TRIM_OUTLIER_K * MAD` (MAD: the
/// median of those distances). The floor keeps a near-uniform edge from being shredded; the MAD term
/// widens the band on noisy edges. The `TRIM_MIN_KEEP` closest samples always stay, so a
/// mostly-product edge cannot collapse the backdrop range.
pub const TRIM_OUTLIER_FLOOR: u32 = 120;
pub const TRIM_OUTLIER_K: f64 = 1.5;
pub const TRIM_MIN_KEEP: usize = 4;

/// A pixel outside the backdrop range only counts as product when it sits inside a block of this
/// many pixels per side that is entirely outside it. Smaller specks and thinner lines are treated as
/// backdrop (a morphological closing of the backdrop with a square window of this size).
pub const TRIM_CLOSE_K: u32 = 2;

/// Margin grown on every side of the squared product box: the larger of `TRIM_MARGIN_MIN_PX` and
/// `TRIM_MARGIN_SHARE` of its side (`optimizer::square_crop`).
pub const TRIM_MARGIN_MIN_PX: u32 = 10;
pub const TRIM_MARGIN_SHARE: f64 = 0.10;

/// Trim 2, the control (`optimizer::recheck_box`): the trim is run a second time on trim 1's tight
/// box. If any side of the second box differs from the first by MORE than this many pixels, the box
/// was not stable — a mono-colour frame (a passe-partout) sat round the real picture — and the
/// second box replaces the first. Starting value, to be calibrated.
pub const TRIM_RECHECK_TOLERANCE_PX: u32 = 2;

/// How far, in working pixels, the segmask's box may sit from a trim box and still count as "equal"
/// to it (`core::framing`): the mask's own edge is only good to a few pixels. Starting value.
pub const TRIM_SEGMASK_TOLERANCE_PX: u32 = 8;

/// `core::framing`: trim 1 only counts as padding on ONE axis (white bars) when the other axis loses
/// no more than this share of its length, summed over both its sides. Starting value.
pub const FRAMING_OTHER_AXIS_SLACK: f32 = 0.02;

/// `core::framing`: bars are padding only when the thinner cut side is at least this share of the
/// thicker (padding is added evenly; backdrop round an off-centre product is not). Starting value.
/// See docs/work-state/2026-10-21-trims-and-framing.md.
pub const FRAMING_BARS_SYMMETRY: f32 = 0.3;

/// `core::framing`: trim 1's box counts as square when its sides differ by no more than this share
/// of the longer one. Starting value.
pub const FRAMING_SQUARE_TOLERANCE: f32 = 0.02;

/// Pass-2 safety margin excluded around the mask when sampling background colour.
pub const REFINE_SAFETY_PX: u32 = 10;

/// Pass-2 context grown around the band-intersection region before cropping, so the colour
/// samplers have enough pixels to work with.
pub const REFINE_CONTEXT_PX: u32 = 30;

// ---- Output envelope --------------------------------------------------------------------------
//
// Applied by the exporter, which every route ends at, so no route can bypass it. The scale is
// uniform, so a square in stays a square out.

/// Minimum output square side.
pub const MIN_SIZE: u32 = 800;
/// Maximum output square side.
pub const MAX_SIZE: u32 = 2000;

// ---- R1 · Center & Stretch --------------------------------------------------------------------

/// Margin R1 leaves around the subject, as a fraction of the square's side: an axis with both sides
/// free needs `extent * (1 + R1_MARGIN)`, 4.2% a side. The square is the larger of the two axes'
/// needs (`shot_classifier::plan::SquarePlan::new`). See docs/architecture/02-modules-core.md.
pub const R1_MARGIN: f32 = 2.0*0.042;

/// Standard deviation of the per-channel noise added to a flat BGC/white fill, so a line with no
/// background left to stretch does not read as a dead-flat seam against its JPEG-noisy neighbours.
pub const R1_INPAINT_NOISE_SIGMA: f32 = 2.0;

// ---- R1 · mask-aware fill (plan Part A) -------------------------------------------------------
//
// Drive the segmask-aware stretch (`center_and_stretch::compose`). Working values: the user allows
// tuning when an image shows a problem. See docs/work-state/2026-10-21-trims-and-framing.md.

/// Plan name `FILL_SOURCE_CLEARANCE_PX`; the one distance between the product and the background
/// the fill may use, in *working*-resolution pixels (scaled at use). The silhouette grown by it is
/// the product band, copied untouched and never a stretch source, so the stretch never samples a
/// cuff, edge or tiny shadow hugging the product (grey streak, images 10 and 11).
/// See docs/work-state/2026-10-21-r1-stretch-rework.md.
pub const R1_FILL_SOURCE_CLEARANCE_WORKING_PX: u32 = GATE_MARGIN_PX;

/// How fast the product band's edge may move from one line to the next, in pixels per line (1 is a
/// 45° slope). The band is only widened, so it still covers the product, until no edge moves faster;
/// this stops a visible step where a second object starts a few lines after the first.
/// See docs/work-state/2026-10-21-fixes-after-review.md.
pub const R1_BAND_SLOPE_PX_PER_LINE: u32 = 1;

// ---- Per-edge trim + texture detector (plan Part C; shared with deferred D) --------------------
//
// A preprocessor-stage primitive, upstream of the mask. Working values, tuned only when an image shows a problem.

/// Plan name `T_edge_flat`. Max per-channel span (0..255) across one edge's kept border samples
/// for that edge to count as `flat`.
pub const T_EDGE_FLAT: u16 = 12;

/// Plan name `T_texture_highpass`. Mean high-pass response (0..255) outside the subject proxy above
/// which the trimmed background reads as *textured* (a Part C scene) rather than *double-flat* (a
/// frame — the deferred Part D signal).
pub const T_TEXTURE_HIGHPASS: f32 = 12.5;

/// Seam-carve perceptual quality gate (plan Part C, priority 2). The max mean per-pixel energy
/// (forward-difference luma gradient `|dL/dx| + |dL/dy|`, 0..~510) of the seams a round inserts.
/// Above it there is no low-energy corridor to grow through, so the carve would visibly warp and
/// `extend::seam_carve_to_square` rejects — the chain falls through to white-pad.
pub const R1_SEAM_CARVE_MAX_SEAM_ENERGY: f32 = 24.0;

/// Headcut on a bars image: the room below the cut may fall short of the mask's width by at most this
/// share, or no headcut is done and the bars rule centres the whole picture. The user's gut value;
/// wants calibrating on more examples. See docs/work-state/2026-10-30-headcut-on-bars-image.md.
pub const HEADCUT_BARS_MAX_SHORTFALL: f32 = 0.25;
