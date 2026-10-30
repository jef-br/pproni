//! Compile-time tuning constants.
//!
//! Detection and geometry constants retired with the classical detector; the routes that replace
//! them will add their own here as they grow past stubs.

/// JPEG quality for the saved output.
pub const JPEG_QUALITY: u8 = 95;

// ---- Shot classification ----------------------------------------------------------------------
//
// The classifier runs on a downscaled *working* copy, and its pass-2 refinement then works against
// full-resolution pixels. The three params below are expressed in working-resolution pixels, so
// they are coupled to WORKING_SIZE — change one and the others shift meaning.

/// Longest side of the working-resolution copy the segmentation model runs on.
pub const WORKING_SIZE: u32 = 1024;

/// Pass 1 and 2 band with gate: an edge is worth refining when foreground appears within this many pixels of it and is the distance for how far the refinement looks.
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
/// is at least this long. `MIN_SIZE` less R1's margin: 800 − 8.4% = 732.8, rounded to 733. Set by the
/// user on 2026-10-29; derived here so it follows `MIN_SIZE` and `R1_MARGIN`.
pub const MASK_MIN_LONGEST_SIDE_PX: u32 = (MIN_SIZE as f64 * (1.0 - R1_MARGIN as f64) + 0.5) as u32;

// ---- Optimiser trim ---------------------------------------------------------------------------
//
// After the colour balance, the optimiser crops the backdrop away so the 512px model sees the product
// larger (`preprocessor::optimizer`). All four numbers are in working-resolution pixels or shares.

/// Backdrop samples taken along each edge of the frame, evenly spaced from the edge's start
/// (4 gives 0%, 25%, 50% and 75%; 100% is the next edge's 0%). The lowest and highest value of every
/// colour channel over all the kept samples is the colour range that counts as backdrop.
pub const TRIM_SAMPLES_PER_EDGE: u32 = 4;

/// Outlier drop on the border samples. A sample is kept when its L1 colour distance to the
/// per-channel median of the 16 samples (`|ΔR|+|ΔG|+|ΔB|`, so 0 to 765) stays at or below
/// `TRIM_OUTLIER_FLOOR + TRIM_OUTLIER_K * MAD`, where MAD is the median of those distances. The
/// floor is a fixed safety band around the median so a near-uniform edge is not shredded; the
/// MAD-scaled term grows the band on noisy edges. `TRIM_MIN_KEEP` samples are always kept — the
/// ones closest to the median — so a mostly-product edge cannot collapse the backdrop range.
pub const TRIM_OUTLIER_FLOOR: u32 = 120;
pub const TRIM_OUTLIER_K: f64 = 1.5;
pub const TRIM_MIN_KEEP: usize = 4;

/// A pixel outside the backdrop range only counts as product when it sits inside a block of this
/// many pixels per side that is entirely outside it. Smaller specks and thinner lines are treated as
/// backdrop (a morphological closing of the backdrop with a square window of this size).
pub const TRIM_CLOSE_K: u32 = 2;

/// The box around the product is first squared to its largest side, then grown on every side by the
/// larger of `TRIM_MARGIN_MIN_PX` and `TRIM_MARGIN_SHARE` of that side. The resulting square is slid to
/// stay inside the frame; it is never cut unless no square fits, and then the margin is the minimum
/// (see `optimizer::square_crop`).
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

/// `core::framing`: bars are padding only when the two cut sides are of similar width: the thinner
/// is at least this share of the thicker. Padding is added evenly (measured: 0.5-1.0); backdrop round a
/// product that sits off-centre is not (image 19: 0.05). Starting value.
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

/// Margin R1 leaves around the subject, as a fraction of the square's side.
///
/// The margin for an axis with both sides free: `extent * (1 + R1_MARGIN)`, shared between the two
/// sides, 4.2% on each. An axis with a single free side (the other is pinned to the frame) gets 4.2%,
/// not the whole 8.4%; an axis pinned on both sides gets none. The square is the larger of the two
/// axes' needs (`shot_classifier::plan::SquarePlan::new`).
pub const R1_MARGIN: f32 = 2.0*0.042;

/// Standard deviation of the per-channel noise added to a flat BGC/white fill, so a line with no
/// background left to stretch does not read as a dead-flat seam against its JPEG-noisy neighbours.
pub const R1_INPAINT_NOISE_SIGMA: f32 = 2.0;

// ---- R1 · mask-aware fill (plan Part A) -------------------------------------------------------
//
// These drive the segmask-aware stretch (`center_and_stretch::compose`). They are working values: the
// user allowed them to be tuned (2026-10-21) when an image shows a problem, and none does now.

/// Plan name `FILL_SOURCE_CLEARANCE_PX`; the one distance between the product and the background
/// the fill may use. The silhouette is grown outward by this many *working*-resolution pixels, in
/// every direction. Inside the grown shape is the product band, copied untouched; the stretch
/// source is only what lies outside it, so the stretch never samples a cuff, edge or tiny shadow
/// hugging the product (the 10/11 grey-streak case). Scaled to full resolution at use.
/// It replaces the old `R1_SAFETY_*` and this constant, which did the same job twice.
pub const R1_FILL_SOURCE_CLEARANCE_WORKING_PX: u32 = GATE_MARGIN_PX;

/// How fast the product band's edge may move from one line to the next, in pixels per line. The band
/// of a line is the silhouette's span on that line, so where a second object starts a few lines after
/// the first (two shoes side by side) the edge would jump by hundreds of pixels between two lines and
/// the stretch beside it would change completely in one line: a visible horizontal step. The band is
/// widened only (never narrowed, so it always covers the product) until no edge moves faster than
/// this. 1 is a 45° slope.
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

/// Headcut on a bars image (white bars that trim 1 cut off on one axis): the bars are padding, so the
/// cut picture is the content box from the philtrum down. The room below the cut (its height) may fall
/// short of the mask's width by at most this share, or the headcut is not done and the bars rule centres
/// the whole picture instead. The user's gut value (2026-10-30; 50% was too loose); only image 46 has
/// tested it, so it wants calibrating on more examples.
pub const HEADCUT_BARS_MAX_SHORTFALL: f32 = 0.25;
