//! R1 step 1 — where the square goes. Pure geometry, no pixels.
//!
//! Per axis: each free side takes half the margin (4.2% of the subject's extent), an axis blocked on
//! both sides needs exactly `extent`, and the square's side is the larger of the two needs. The slack
//! goes to the free sides (split when both are free); a blocked side never moves. A 0-edge subject is
//! the degenerate case, not a separate one. See docs/specs/05-routing.md §5.1.
//!
//! Deriving the side from the subject rather than from the image is what makes the square able to
//! reach past a border — the case R1 exists for, not a failure mode. A side inside the image crops
//! there; a side past it leaves an [`Overhang`] for the fill to cover.

use crate::core::shot_classifier::{geometry::Rect, Edge};

/// Which sides the subject runs off the frame at, and therefore may not be moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pins {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

impl Pins {
    /// Nothing blocked — the free-standing subject R1 was first written for.
    pub const NONE: Pins = Pins { top: false, bottom: false, left: false, right: false };

    pub fn from_edges(edges: &[Edge]) -> Pins {
        Pins {
            top: edges.contains(&Edge::Top),
            bottom: edges.contains(&Edge::Bottom),
            left: edges.contains(&Edge::Left),
            right: edges.contains(&Edge::Right),
        }
    }
}

/// The output square, in the *original* image's coordinate space.
///
/// `x`/`y` are signed because the square legitimately starts outside the image — a subject framed
/// tightly against the left border puts the square's left column at a negative x.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SquarePlan {
    pub side: u32,
    pub x: i64,
    pub y: i64,
}

/// How far the square reaches past each image border, in pixels. Zero on a side that lands on real
/// pixels — that side crops instead of stretching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Overhang {
    pub top: u32,
    pub bottom: u32,
    pub left: u32,
    pub right: u32,
}

impl SquarePlan {
    /// Size the square and place it, given which sides are blocked.
    ///
    /// `None` when an axis is blocked at both ends and still comes up short — the *other* axis
    /// demanded a bigger square, and there is nowhere legal to put the difference. Growing a
    /// blocked axis means inventing background on top of a subject that continues off-frame, and
    /// splitting the difference anyway would stretch the product. R1 has no answer, so it says so
    /// and the caller frames the image safely instead.
    pub fn new(subject: Rect, margin: f32, pins: Pins) -> Option<SquarePlan> {
        // `margin` is the share for a subject with both sides of an axis free; each free side takes
        // half of it, so one free side gets 4.2% and two get 8.4%.
        let need = |extent: u32, free_sides: u32| {
            extent as f64 * (1.0 + margin as f64 * free_sides as f64 / 2.0)
        };
        let free = |lo: bool, hi: bool| (!lo) as u32 + (!hi) as u32;
        let side = need(subject.h, free(pins.top, pins.bottom))
            .max(need(subject.w, free(pins.left, pins.right)))
            .round()
            .max(1.0) as u32;

        let y = origin(subject.y, subject.h, side, pins.top, pins.bottom)?;
        let x = origin(subject.x, subject.w, side, pins.left, pins.right)?;
        Some(SquarePlan { side, x, y })
    }

    /// The part of the square that lands on real pixels, in original-image coordinates.
    ///
    /// `None` when the square misses the image entirely — impossible for a box derived from a mask
    /// inside that image, but the fill would read out of bounds if it ever happened.
    pub fn real_region(&self, img_w: u32, img_h: u32) -> Option<Rect> {
        let x0 = self.x.max(0);
        let y0 = self.y.max(0);
        let x1 = (self.x + self.side as i64).min(img_w as i64);
        let y1 = (self.y + self.side as i64).min(img_h as i64);
        (x1 > x0 && y1 > y0).then(|| Rect {
            x: x0 as u32,
            y: y0 as u32,
            w: (x1 - x0) as u32,
            h: (y1 - y0) as u32,
        })
    }

    pub fn overhang(&self, img_w: u32, img_h: u32) -> Overhang {
        let past = |v: i64| v.max(0) as u32;
        Overhang {
            left: past(-self.x),
            top: past(-self.y),
            right: past(self.x + self.side as i64 - img_w as i64),
            bottom: past(self.y + self.side as i64 - img_h as i64),
        }
    }
}

/// Where the square starts on one axis: the subject's own start, less whatever share of the slack
/// the low side is entitled to.
///
/// Rounding on an even split matches what the original centred-only code did, so a free-standing
/// subject lands on exactly the pixel it always has.
fn origin(start: u32, extent: u32, side: u32, pin_lo: bool, pin_hi: bool) -> Option<i64> {
    let slack = side as i64 - extent as i64;
    let gap_lo = match (pin_lo, pin_hi) {
        (true, _) => 0,
        (false, true) => slack,
        (false, false) => slack.div_euclid(2) + slack.rem_euclid(2),
    };
    // Both ends blocked and the square still wants to grow: no legal home for the slack.
    (!(pin_lo && pin_hi) || slack == 0).then(|| start as i64 - gap_lo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::R1_MARGIN;

    fn rect(x: u32, y: u32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
    }

    fn plan(subject: Rect, pins: Pins) -> SquarePlan {
        SquarePlan::new(subject, R1_MARGIN, pins).expect("a legal square")
    }

    #[test]
    fn side_is_the_longest_bbox_edge_plus_margin() {
        // Portrait subject: height decides, width follows it into a square.
        assert_eq!(plan(rect(100, 30, 1300, 1670), Pins::NONE).side, 1810); // 1670 * 1.084
    }

    #[test]
    fn the_square_is_centred_on_the_subject() {
        // Subject centre is (750, 865); a 1810 square centred there starts at -155, -40.
        let p = plan(rect(100, 30, 1300, 1670), Pins::NONE);
        assert_eq!((p.x, p.y), (-155, -40));
    }

    #[test]
    fn overhang_is_per_side_and_zero_where_real_pixels_reach() {
        // The worked case: 1500x2000 source. Left and right reach 155px past the frame, the top
        // 40px, and the bottom lands 230px inside it — so the bottom crops rather than stretches.
        let p = plan(rect(100, 30, 1300, 1670), Pins::NONE);
        assert_eq!(p.overhang(1500, 2000), Overhang { top: 40, bottom: 0, left: 155, right: 155 });
    }

    #[test]
    fn a_square_inside_the_image_has_no_overhang_and_crops() {
        let p = plan(rect(400, 400, 200, 200), Pins::NONE);
        assert_eq!(p.side, 217);
        assert_eq!(p.overhang(1000, 1000), Overhang::default());
        assert_eq!(
            p.real_region(1000, 1000),
            Some(rect(391, 391, 217, 217)),
            "the whole square is real pixels, so the region is the square itself"
        );
    }

    #[test]
    fn a_subject_filling_the_frame_overhangs_on_all_four_sides() {
        // Free-standing but tight: no cap on the stretch, by decision.
        let p = plan(rect(0, 0, 1000, 1000), Pins::NONE);
        assert_eq!(p.side, 1084);
        assert_eq!(p.overhang(1000, 1000), Overhang { top: 42, bottom: 42, left: 42, right: 42 });
    }

    #[test]
    fn one_blocked_side_gives_the_free_side_its_own_margin_of_4_2_percent() {
        // 28.jpg: 667x1000 frame, mask 474x987 flush against the top.
        let p = plan(rect(106, 0, 474, 987), Pins { top: true, ..Pins::NONE });
        assert_eq!(p.side, 1028, "987 * 1.042 — a single free side gets 4.2%, not the two-sided 8.4%");
        assert_eq!(p.y, 0, "the blocked top does not move");

        // All 41px of slack lands under the feet. 13 of them are real floor already in frame;
        // the other 28 are new canvas the fill has to cover.
        let over = p.overhang(667, 1000);
        assert_eq!(over.top, 0, "nothing is ever invented above a subject that continues off-frame");
        assert_eq!(over.bottom, 28);
    }

    #[test]
    fn an_axis_blocked_at_both_ends_gets_no_margin() {
        // EIX 1010 on a 640x840 frame: the subject runs off the top and the bottom, so the
        // vertical axis needs 840 flat rather than 840 * 1.042 or 840 * 1.084, and the square is 840.
        let p = plan(rect(60, 0, 520, 840), Pins { top: true, bottom: true, ..Pins::NONE });
        assert_eq!(p.side, 840);
        assert_eq!(p.y, 0);
        assert_eq!(p.overhang(640, 840), Overhang { top: 0, bottom: 0, left: 100, right: 100 });
    }

    #[test]
    fn three_blocked_sides_push_all_the_slack_onto_the_one_that_is_free() {
        // Top, bottom and left blocked: the square fills the vertical axis, and the width it is
        // short lands entirely on the right.
        let p = plan(rect(0, 0, 300, 500), Pins { top: true, bottom: true, left: true, right: false });
        assert_eq!(p.side, 500);
        assert_eq!((p.x, p.y), (0, 0), "both blocked corners hold");
        assert_eq!(p.overhang(400, 500).right, 100);
    }

    #[test]
    fn a_blocked_axis_that_still_comes_up_short_has_no_answer() {
        // Wide subject bleeding off the top and bottom of a shallow frame: the width demands a
        // square taller than the image, and neither vertical side may move. Inventing background
        // there would paint over a subject that continues off-frame.
        assert!(
            SquarePlan::new(rect(0, 0, 900, 400), R1_MARGIN, Pins { top: true, bottom: true, ..Pins::NONE }).is_none()
        );
    }

    #[test]
    fn real_region_clips_to_the_image_on_every_side() {
        let p = SquarePlan { side: 100, x: -30, y: 950 };
        assert_eq!(p.real_region(200, 1000), Some(rect(0, 950, 70, 50)));
    }

    #[test]
    fn a_square_that_misses_the_image_has_no_real_region() {
        assert!(SquarePlan { side: 10, x: -50, y: 0 }.real_region(200, 200).is_none());
    }
}
