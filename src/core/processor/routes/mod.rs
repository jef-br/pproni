//! Routing: turn the shot classifier's verdict into one of three processing routes.
//!
//! | Route | Verdict | Strategy |
//! |---|---|---|
//! | R1 [`Route::CenterAndStretch`] | a usable mask with room to grow | square around the subject, bled edges pinned, background stretched to fill |
//! | R2 [`Route::CropSquare`] | no usable mask, or no room to grow | take the largest square already in the frame |
//! | R3 [`Route::Fallback`] | no verdict at all | fit the whole image into a square canvas |
//!
//! **The bleed table lives inside R1.** `docs/pproni-spec.md` §5 lists five behaviours for a subject
//! that reaches an edge. Four of them are one rule seen from four angles: a bled edge is blocked,
//! an axis with a free side takes the margin, and the slack goes to the free sides.
//! [`center_and_stretch`] applies that rule, so those four never reach [`Route::select`] as a
//! routing decision at all.
//!
//! **What does reach it is whether framing is possible.** R2 is not "the other bleed case" — it is
//! the answer to "there is nothing here to frame around, or no room to do it in". Three verdicts
//! say that, and [`crop_square`] explains each.
//!
//! R3 is not a spec row. It exists because "the model produced no verdict" is a genuinely
//! different state from any verdict the model did produce — see [`Route::select`].

pub mod center_and_stretch;
pub mod crop_square;
pub mod fallback;

use super::super::preprocessor::Prepared;
use crate::core::shot_classifier::{geometry::Rect, Edge, Mask, ShotClassification};
use image::{Rgb, RgbImage};

/// What the classifier produced for one image: the edge verdict, and the mask it was read from.
///
/// The two travel together because they are produced together — there is no verdict without a
/// mask — so routes never have to handle "verdict but no mask". Absence of the whole thing is the
/// no-verdict case, and that is R3's business.
///
/// **Resolutions.** `mask` is aligned with [`Prepared::working`] (longest side ≤ `WORKING_SIZE`),
/// while routes compose their output from [`Prepared::original`]. Anything derived from the mask
/// has to cross that gap; [`Shot::subject_bbox_in`] is the crossing, so routes do not each
/// rediscover the scale factor.
#[derive(Clone, Copy)]
pub struct Shot<'a> {
    pub class: &'a ShotClassification,
    /// Working-resolution subject mask.
    pub mask: &'a Mask,
    /// Dominant background colour, when the exporter's colour sampler found one. R1 reads this to
    /// decide whether a column inside a gated edge is close enough to the backdrop to stretch
    /// through — see `center_and_stretch::compose::classify_segment`. `None` means no single
    /// colour dominated the background region, and R1 falls back to per-edge local sampling.
    pub bgc: Option<Rgb<u8>>,
}

impl<'a> Shot<'a> {
    /// Tight bounding box of the subject, in the *working* image's coordinates.
    ///
    /// `None` for an empty mask — a classification can come back with nothing marked, and a route
    /// asking "where is the subject" deserves an answer that says "nowhere" rather than a box
    /// covering the whole frame.
    pub fn subject_bbox(&self) -> Option<Rect> {
        let (mut x0, mut y0) = (u32::MAX, u32::MAX);
        let (mut x1, mut y1) = (0u32, 0u32);
        for y in 0..self.mask.height {
            for x in 0..self.mask.width {
                if self.mask.get(x, y) {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        (x0 != u32::MAX).then(|| Rect::from_bounds(x0, y0, x1, y1))
    }

    /// The same box in `prep.original`'s coordinates.
    ///
    /// The preprocessor only ever scales down and keeps the aspect ratio, so one factor per axis
    /// is the whole of the mapping.
    pub fn subject_bbox_in(&self, prep: &Prepared) -> Option<Rect> {
        let fx = prep.original.width() as f32 / prep.working.width().max(1) as f32;
        let fy = prep.original.height() as f32 / prep.working.height().max(1) as f32;
        self.subject_bbox().map(|b| b.scaled(fx, fy))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// R1 — frame around the subject.
    CenterAndStretch,
    /// R2 — crop a square out of the frame.
    CropSquare,
    /// R3 — no classification was produced.
    Fallback,
}

impl Route {
    /// Pick the route for a verdict.
    ///
    /// R1 needs two things: a mask that describes a subject, and room to put a square around it.
    /// Each of R2's three verdicts is one of those two missing.
    ///
    /// `None` means the classifier could not produce a verdict at all — a failed inference, an
    /// unusable model output. That is deliberately distinct from any verdict it *did* produce, so
    /// a model failure degrades one image instead of being silently processed as a clean shot.
    pub fn select(class: Option<&ShotClassification>) -> Route {
        let Some(c) = class else {
            return Route::Fallback;
        };
        // Nothing to frame around: the mask does not describe a subject, so cropping to it would
        // magnify whichever speck the segmenter latched onto — and the size envelope would then
        // blow that speck up to fill an 800px square.
        if c.mask_too_small {
            return Route::CropSquare;
        }
        // No room to frame in. Left and right both bled and nothing else: the subject spans the
        // full width, so the square can be at most that wide, and it is taller than that. Growing
        // the width means painting background over a subject that runs off-frame. Three bled
        // edges are excluded deliberately — the free edge gives R1 somewhere to put the slack,
        // which is why a 3-edge image frames correctly today.
        let horizontally_boxed_in = c.touches_edges.len() == 2
            && c.touches_edges.contains(&Edge::Left)
            && c.touches_edges.contains(&Edge::Right);
        if horizontally_boxed_in || c.touches_edges.len() == 4 {
            return Route::CropSquare;
        }
        Route::CenterAndStretch
    }
}

/// Run the route the verdict selects.
pub fn dispatch(prep: &Prepared, shot: Option<Shot<'_>>) -> RgbImage {
    let route = Route::select(shot.map(|s| s.class));
    let img = match (route, shot) {
        (Route::CenterAndStretch, Some(shot)) => center_and_stretch::apply(prep, shot),
        (Route::CropSquare, Some(shot)) => crop_square::apply(prep, shot),
        // R3, and nothing else can reach here: `select` returns a non-fallback route only for
        // `Some`, so a missing shot always lands on the fallback. R1 and R2 therefore take a
        // plain `Shot` rather than an `Option`, and cannot be handed a state they can't serve.
        _ => fallback::apply(prep),
    };
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::preprocessor::{Framing, Trims};

    fn verdict(touches: &[Edge]) -> ShotClassification {
        ShotClassification { touches_edges: touches.to_vec(), mask_too_small: false, refinements: Vec::new() }
    }

    #[test]
    fn no_verdict_falls_back() {
        // The distinction R3 exists for: "touches nothing" (bits 0000 below, which R1 takes) and
        // "no verdict" must not collapse.
        assert_eq!(Route::select(None), Route::Fallback);
    }

    #[test]
    fn every_edge_combination_routes() {
        // All 16 subsets of the four edges. R1 takes every one that leaves it a way to build the
        // square; the two that do not are left+right (no room to grow the boxed-in axis) and the
        // full bleed (nothing anywhere to grow from).
        //
        // The asymmetry is real and it is not about the axis. Left+right bled means the square
        // cannot be wider than the frame while the subject is taller than it, so something has to
        // be cropped. Top+bottom bled means the square is as tall as the frame and the width has
        // room to stretch into, which R1 does well. A third bled edge hands the image back to R1,
        // because the one remaining free edge is somewhere to put the slack (image 29 of the test
        // set, which frames correctly).
        for bits in 0u8..16 {
            let touches: Vec<Edge> = Edge::ALL
                .iter()
                .enumerate()
                .filter(|(i, _)| bits & (1 << i) != 0)
                .map(|(_, &e)| e)
                .collect();

            let boxed_in = touches.len() == 2
                && touches.contains(&Edge::Left)
                && touches.contains(&Edge::Right);
            let expected = if boxed_in || bits == 0b1111 {
                Route::CropSquare
            } else {
                Route::CenterAndStretch
            };
            let got = Route::select(Some(&verdict(&touches)));
            assert_eq!(got, expected, "bits {bits:04b} ({touches:?}) routed to {got:?}");
        }
    }

    #[test]
    fn a_mask_worth_nothing_crops_whatever_its_edges_say() {
        // Images 6 and 30: the edge verdict may be perfectly ordinary, but there is no subject
        // behind it, so no route that frames around the mask can produce a sane result.
        for touches in [vec![], vec![Edge::Top], Edge::ALL.to_vec()] {
            let class = ShotClassification { mask_too_small: true, ..verdict(&touches) };
            assert_eq!(Route::select(Some(&class)), Route::CropSquare, "{touches:?}");
        }
    }

    fn mask_with(w: u32, h: u32, subject: Rect) -> Mask {
        let mut m = Mask { width: w, height: h, data: vec![0u8; (w * h) as usize] };
        for y in subject.y..subject.y + subject.h {
            for x in subject.x..subject.x + subject.w {
                m.data[(y * w + x) as usize] = 255;
            }
        }
        m
    }

    #[test]
    fn subject_bbox_is_tight_around_the_mask_and_empty_for_an_empty_one() {
        let class = verdict(&[]);
        let mask = mask_with(100, 100, Rect { x: 20, y: 30, w: 40, h: 20 });
        let got = Shot { class: &class, mask: &mask, bgc: None }.subject_bbox().expect("mask is not empty");

        // Tight, not the frame: the whole point of deriving it from the mask rather than taking
        // `Instance::bbox`, which BiRefNet fills with the full image.
        assert_eq!((got.x, got.y, got.w, got.h), (20, 30, 40, 20));

        let empty = Mask { width: 50, height: 50, data: vec![0u8; 2500] };
        assert!(
            Shot { class: &class, mask: &empty, bgc: None }.subject_bbox().is_none(),
            "an empty mask must report no subject, not a box covering the frame"
        );
    }

    #[test]
    fn subject_bbox_scales_from_working_to_original() {
        let mask = mask_with(100, 100, Rect { x: 20, y: 30, w: 40, h: 20 });
        let class = verdict(&[]);
        let prep = Prepared {
            original: RgbImage::new(200, 200),
            working: RgbImage::new(100, 100),
            model_input: RgbImage::new(100, 100),
            crop: Rect { x: 0, y: 0, w: 100, h: 100 },
            trims: Trims::NONE, framing: Framing::Free,
            letterbox: None,
        };

        let got = Shot { class: &class, mask: &mask, bgc: None }
            .subject_bbox_in(&prep)
            .expect("mask is not empty");

        // 2x on both axes. Routes compose from `original`, so handing them a working-resolution
        // box would misplace every crop by the downscale factor.
        assert_eq!((got.x, got.y, got.w, got.h), (40, 60, 80, 40));
    }
}
