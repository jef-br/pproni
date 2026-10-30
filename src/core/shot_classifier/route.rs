//! Routing: the shot classifier's last step. It reads the verdict, picks one of three routes, and sends
//! the image to it. Every decision about *which* route and *which plan* is made here; the routes in
//! `processor::routes` only change the picture.
//!
//! | Route | Verdict | Strategy |
//! |---|---|---|
//! | R1 [`Route::CenterAndStretch`] | a usable mask with room to grow | square around the subject, bled edges pinned, background stretched to fill |
//! | R2 [`Route::CropSquare`] | no usable mask, or no room to grow | take the largest square already in the frame |
//! | R3 [`Route::Fallback`] | no verdict, or no square can be planned | fit the whole image into a square canvas |
//!
//! **The bleed table lives inside R1.** `docs/pproni-spec.md` §5 lists five behaviours for a subject
//! that reaches an edge. Four of them are one rule seen from four angles: a bled edge is blocked,
//! an axis with a free side takes the margin, and the slack goes to the free sides. R1 applies that
//! rule, so those four never reach [`decide`] as a routing choice at all.
//!
//! **What does reach it is whether framing is possible.** R2 is the answer to "there is nothing here
//! to frame around, or no room to do it in". R3 is "we cannot say where the subject is" or "no square
//! respects the bled edges": nothing is cropped and nothing is guessed.
//!
//! The route a [`Decision`] names is the route that runs: [`send`] does exactly what it says and no
//! route hands the image on to another one. The only check left inside a route is R1's seam-carve
//! quality gate on a textured scene, which needs the pixels it has just made.

use super::geometry::{Edge, Rect};
use super::plan::{Pins, SquarePlan};
use super::{Mask, ShotClassification};
use crate::core::config;
use crate::core::preprocessor::{Framing, Prepared};
use crate::core::processor::routes::{center_and_stretch, crop_square, fallback};
use image::RgbImage;

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
    /// Dominant background colour, when the exporter's colour sampler found one. R1 holds every
    /// pixel it invents to this. `None` means no single colour dominated the background region.
    pub bgc: Option<image::Rgb<u8>>,
}

impl<'a> Shot<'a> {
    /// Tight bounding box of the subject, in the *working* image's coordinates.
    ///
    /// `None` for an empty mask — a classification can come back with nothing marked, and a route
    /// asking "where is the subject" deserves an answer that says "nowhere" rather than a box
    /// covering the whole frame.
    pub fn subject_bbox(&self) -> Option<Rect> {
        self.mask.bounds()
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

/// What R1 is told to do. The classifier works this out; R1 only carries it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum R1Job {
    /// Cut this square around the subject and stretch the background out to fill it.
    Stretch(SquarePlan),
    /// A textured scene with flat bars: reframe the trimmed `keep` box (crop, seam-carve or white-pad)
    /// instead of stretching. `subject` is relative to `keep`.
    Reframe { keep: Rect, subject: Rect },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// R1 — frame around the subject.
    CenterAndStretch(R1Job),
    /// R2 — crop a square out of the frame.
    CropSquare,
    /// R3 — fit the whole image into a square canvas.
    Fallback,
}

impl Route {
    /// The label the stage-90 log shows. It comes from the route that runs, never from a guess made
    /// before it.
    pub fn label(&self) -> &'static str {
        match self {
            Route::CenterAndStretch(_) => "[R1 - CenterAndStretch]",
            Route::CropSquare => "[R2 - CropSquare]",
            Route::Fallback => "[R3 - Fallback]",
        }
    }
}

/// One routing decision: the route, and what the stage-90 picture and log need to show with it.
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub route: Route,
    /// The subject's box in `prep.original` coordinates; `None` with no verdict or an empty mask.
    pub subject: Option<Rect>,
    /// The square R1 would cut, when one can be planned — shown on stage 90 whatever the route.
    pub plan: Option<SquarePlan>,
    /// One log line when the route is not the plain reading of the verdict.
    pub note: Option<String>,
}

/// Pick the route for one image.
///
/// R1 needs two things: a mask that describes a subject, and room to put a square around it. Each
/// of R2's verdicts is one of those two missing. `None` for `shot` means the classifier could not
/// produce a verdict at all — a failed inference, an unusable model output. That is deliberately
/// distinct from any verdict it *did* produce, so a model failure degrades one image instead of
/// being silently processed as a clean shot.
pub fn decide(prep: &Prepared, shot: Option<Shot<'_>>) -> Decision {
    let Some(shot) = shot else {
        return Decision { route: Route::Fallback, subject: None, plan: None, note: None };
    };
    let class = shot.class;
    let subject = shot.subject_bbox_in(prep);
    let plan = subject.and_then(|s| plan_square(prep, s, &class.touches_edges));
    let decided = |route: Route, note: Option<String>| Decision { route, subject, plan, note };

    // Nothing to frame around: the mask does not describe a subject, so cropping to it would
    // magnify whichever speck the segmenter latched onto — and the size envelope would then
    // blow that speck up to fill an 800px square.
    if class.mask_too_small {
        return decided(Route::CropSquare, None);
    }
    // No room to frame in. Left and right both bled and nothing else: the subject spans the
    // full width, so the square can be at most that wide, and it is taller than that. Growing
    // the width means painting background over a subject that runs off-frame.
    let horizontally_boxed_in = class.touches_edges.len() == 2
        && class.touches_edges.contains(&Edge::Left)
        && class.touches_edges.contains(&Edge::Right);
    if horizontally_boxed_in || class.touches_edges.len() == 4 {
        return decided(Route::CropSquare, None);
    }

    // R1 from here, unless nothing in the picture lets it work.
    // An empty mask means the classifier marked nothing: there is no subject to centre on, so the
    // safe framing of R3 is a better answer than a square around an arbitrary point.
    let Some(subject_box) = subject else {
        return decided(
            Route::Fallback,
            Some("R1: the mask is empty, so there is no subject to centre on: using R3 framing".into()),
        );
    };
    // A textured scene with flat letterbox/pillarbox bars must be re-framed, not stretched: R1's
    // stretch invents a smeared band across a textured overhang (measured on 268/46). The
    // preprocessor's detector (`prep.letterbox`) fires only on genuine bars-plus-textured-interior —
    // a freestanding product on a plain backdrop has flat edges on both axes, so it never fires and
    // `EIX0000` cannot flip. Runs after the subject guard so the mask gives the real subject box.
    if let Some(keep) = prep.letterbox {
        let job = R1Job::Reframe { keep, subject: subject_into(subject_box, keep) };
        return decided(
            Route::CenterAndStretch(job),
            Some(format!("R1 → Part C: textured scene, reframing on the trimmed {keep:?} rather than stretching")),
        );
    }
    // `None` means no square can respect the bled edges — R3 frames the image safely rather than R1
    // guessing.
    match plan {
        Some(plan) => decided(Route::CenterAndStretch(R1Job::Stretch(plan)), None),
        None => decided(
            Route::Fallback,
            Some("R1: no square can respect the bled edges: using R3 framing".into()),
        ),
    }
}

/// Send the image to the route that [`decide`] named, and return what it makes.
pub fn send(prep: &Prepared, shot: Option<Shot<'_>>, route: &Route) -> RgbImage {
    match (route, shot) {
        (Route::CenterAndStretch(job), Some(shot)) => center_and_stretch::apply(prep, shot, *job),
        (Route::CropSquare, Some(shot)) => crop_square::apply(prep, shot),
        // R3, and nothing else can reach here: `decide` names a non-fallback route only for `Some`,
        // so a missing shot always lands on the fallback. R1 and R2 therefore take a plain `Shot`
        // rather than an `Option`, and cannot be handed a state they can't serve.
        _ => fallback::apply(prep),
    }
}

/// Where R1's square goes.
///
/// Normally it is planned from the subject: the edges the subject bleeds off are the ones the square
/// may not move ([`SquarePlan::new`]). When `core::framing` found white bars on one axis
/// ([`Framing::CentreContent`]) it is cut from the content box instead: as wide as the box's longer
/// side, no margin, centred on the box — equal padding either side of the shorter axis, aligned on
/// nothing. Whatever of the square falls outside the picture is filled as always.
pub fn plan_square(prep: &Prepared, subject: Rect, touches: &[Edge]) -> Option<SquarePlan> {
    if let Framing::CentreContent(content) = prep.framing {
        let (fx, fy) = (
            prep.original.width() as f32 / prep.working.width().max(1) as f32,
            prep.original.height() as f32 / prep.working.height().max(1) as f32,
        );
        let (x0, y0) = ((content.x as f32 * fx).round() as i64, (content.y as f32 * fy).round() as i64);
        let (w, h) = ((content.w as f32 * fx).round() as i64, (content.h as f32 * fy).round() as i64);
        let side = w.max(h).max(1);
        return Some(SquarePlan {
            side: side as u32,
            x: x0 + (w - side).div_euclid(2),
            y: y0 + (h - side).div_euclid(2),
        });
    }
    SquarePlan::new(subject, config::R1_MARGIN, Pins::from_edges(touches))
}

/// Re-express `subject` (original coordinates) relative to the trimmed `keep` sub-rect, clamped to
/// it. The trimmed bars are background, so the subject normally lies wholly inside `keep`; the
/// clamp only guards a subject box that grazes a trimmed band.
pub(crate) fn subject_into(subject: Rect, keep: Rect) -> Rect {
    let x0 = subject.x.max(keep.x);
    let y0 = subject.y.max(keep.y);
    let x1 = (subject.x + subject.w).min(keep.x + keep.w);
    let y1 = (subject.y + subject.h).min(keep.y + keep.h);
    Rect { x: x0 - keep.x, y: y0 - keep.y, w: x1.saturating_sub(x0), h: y1.saturating_sub(y0) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::preprocessor::Trims;
    use image::Rgb;

    fn prepared(w: u32, h: u32) -> Prepared {
        let img = RgbImage::from_pixel(w, h, Rgb([255, 255, 255]));
        Prepared {
            original: img.clone(),
            working: img.clone(),
            model_input: img.clone(),
            crop: Rect { x: 0, y: 0, w, h },
            trims: Trims::NONE,
            framing: Framing::Free,
            letterbox: None,
        }
    }

    fn verdict(touches: &[Edge]) -> ShotClassification {
        ShotClassification { touches_edges: touches.to_vec(), mask_too_small: false, refinements: Vec::new() }
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

    /// A 400x400 frame with a 100x100 square subject in the middle: the edges the verdict names are
    /// the only thing that differs between the cases below.
    fn route_for(class: &ShotClassification) -> Decision {
        let prep = prepared(400, 400);
        let mask = mask_with(400, 400, Rect { x: 150, y: 150, w: 100, h: 100 });
        decide(&prep, Some(Shot { class, mask: &mask, bgc: None }))
    }

    fn is_r1(route: &Route) -> bool {
        matches!(route, Route::CenterAndStretch(_))
    }

    #[test]
    fn no_verdict_falls_back() {
        // The distinction R3 exists for: "touches nothing" (bits 0000 below, which R1 takes) and
        // "no verdict" must not collapse.
        let d = decide(&prepared(100, 100), None);
        assert_eq!(d.route, Route::Fallback);
        assert_eq!((d.subject, d.plan, d.note), (None, None, None));
    }

    #[test]
    fn every_edge_combination_routes() {
        // All 16 subsets of the four edges, for a square subject. Three things can happen:
        //  * R2: left+right bled and nothing else (the subject spans the width and is taller than
        //    it), or all four edges (nothing to grow from).
        //  * R3: an axis bled at both ends and the other axis wanting a bigger square. There is no
        //    legal place for the slack, so no square can be planned. For a square subject that is
        //    every subset with top+bottom or left+right, bar the two R2 cases above: 30, 43 and 44.
        //  * R1: everything else.
        for bits in 0u8..16 {
            let touches: Vec<Edge> = Edge::ALL
                .iter()
                .enumerate()
                .filter(|(i, _)| bits & (1 << i) != 0)
                .map(|(_, &e)| e)
                .collect();
            let has = |e: Edge| touches.contains(&e);
            let both_ends = (has(Edge::Top) && has(Edge::Bottom)) || (has(Edge::Left) && has(Edge::Right));
            let boxed_in = touches.len() == 2 && has(Edge::Left) && has(Edge::Right);

            let got = route_for(&verdict(&touches)).route;
            if boxed_in || bits == 0b1111 {
                assert_eq!(got, Route::CropSquare, "bits {bits:04b} ({touches:?})");
            } else if both_ends {
                assert_eq!(got, Route::Fallback, "bits {bits:04b} ({touches:?})");
            } else {
                assert!(is_r1(&got), "bits {bits:04b} ({touches:?}) routed to {got:?}");
            }
        }
    }

    #[test]
    fn a_mask_worth_nothing_crops_whatever_its_edges_say() {
        // Images 6 and 30: the edge verdict may be perfectly ordinary, but there is no subject
        // behind it, so no route that frames around the mask can produce a sane result.
        for touches in [vec![], vec![Edge::Top], Edge::ALL.to_vec()] {
            let class = ShotClassification { mask_too_small: true, ..verdict(&touches) };
            assert_eq!(route_for(&class).route, Route::CropSquare, "{touches:?}");
        }
    }

    #[test]
    fn a_square_that_cannot_respect_the_bled_edges_is_r3_and_says_why() {
        // 30 in miniature: top and bottom bled, so the height cannot grow, and the width wants to.
        let d = route_for(&verdict(&[Edge::Top, Edge::Bottom]));
        assert_eq!(d.route, Route::Fallback);
        assert_eq!(d.plan, None);
        assert_eq!(d.note.as_deref(), Some("R1: no square can respect the bled edges: using R3 framing"));
    }

    #[test]
    fn an_empty_mask_falls_back_and_says_why() {
        let prep = prepared(100, 100);
        let mask = Mask { width: 100, height: 100, data: vec![0u8; 100 * 100] };
        let class = verdict(&[]);
        let d = decide(&prep, Some(Shot { class: &class, mask: &mask, bgc: None }));
        assert_eq!(d.route, Route::Fallback);
        assert!(d.note.unwrap().contains("the mask is empty"));
    }

    #[test]
    fn a_textured_scene_with_bars_reframes_on_the_trimmed_box() {
        let mut prep = prepared(60, 60);
        let keep = Rect { x: 10, y: 0, w: 40, h: 60 };
        prep.letterbox = Some(keep);
        let subject = Rect { x: 20, y: 3, w: 12, h: 54 };
        let mask = mask_with(60, 60, subject);
        let class = verdict(&[]);
        let d = decide(&prep, Some(Shot { class: &class, mask: &mask, bgc: None }));
        assert_eq!(d.route, Route::CenterAndStretch(R1Job::Reframe { keep, subject: subject_into(subject, keep) }));
        assert!(d.note.unwrap().starts_with("R1 → Part C: textured scene"));
    }

    #[test]
    fn route_labels_are_the_three_the_log_shows() {
        let plan = SquarePlan { side: 1, x: 0, y: 0 };
        assert_eq!(Route::CenterAndStretch(R1Job::Stretch(plan)).label(), "[R1 - CenterAndStretch]");
        assert_eq!(Route::CropSquare.label(), "[R2 - CropSquare]");
        assert_eq!(Route::Fallback.label(), "[R3 - Fallback]");
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
        let mut prep = prepared(200, 200);
        prep.working = RgbImage::new(100, 100);

        let got = Shot { class: &class, mask: &mask, bgc: None }
            .subject_bbox_in(&prep)
            .expect("mask is not empty");

        // 2x on both axes. Routes compose from `original`, so handing them a working-resolution
        // box would misplace every crop by the downscale factor.
        assert_eq!((got.x, got.y, got.w, got.h), (40, 60, 80, 40));
    }

    #[test]
    fn white_bars_are_centred_with_equal_padding_and_not_aligned_on_the_subject() {
        // A 300x400 picture with 100 px bars each side of a 100 px wide content box, full height.
        let img = RgbImage::from_pixel(300, 400, Rgb([255, 255, 255]));
        let mut prep = Prepared {
            original: img.clone(),
            working: img.clone(),
            model_input: img.clone(),
            crop: Rect { x: 0, y: 0, w: 300, h: 400 },
            trims: Trims::NONE,
            framing: Framing::CentreContent(Rect { x: 100, y: 0, w: 100, h: 400 }),
            letterbox: None,
        };
        // The subject hugs the right of the content: a subject-aligned plan would not be centred.
        let subject = Rect { x: 150, y: 100, w: 50, h: 300 };
        let plan = plan_square(&prep, subject, &[Edge::Bottom]).unwrap();
        assert_eq!((plan.side, plan.x, plan.y), (400, -50, 0), "400 square, 150 px of padding split 75/75 about the box");
        // Scaled the same way when the working copy is smaller than the original.
        prep.working = RgbImage::from_pixel(150, 200, Rgb([255, 255, 255]));
        prep.framing = Framing::CentreContent(Rect { x: 50, y: 0, w: 50, h: 200 });
        let plan = plan_square(&prep, subject, &[Edge::Bottom]).unwrap();
        assert_eq!((plan.side, plan.x, plan.y), (400, -50, 0));
        // Without the framing the subject decides, as before.
        prep.framing = Framing::Free;
        let plan = plan_square(&prep, subject, &[Edge::Bottom]).unwrap();
        assert_ne!((plan.side, plan.x), (400, -50));
    }
}
