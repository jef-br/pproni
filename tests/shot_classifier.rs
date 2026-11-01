use pproni::core::shot_classifier::segmentation::{Instance, Mask};
use pproni::core::shot_classifier::{classify_instance, geometry::Rect, refine::RefineParams, refine::RefinementInput, Edge};
use image::{Rgb, RgbImage};

/// A mid-grey image with the `subject` rectangle painted white, and its matching mask. Used as both
/// working and original image (scale 1), to keep the case simple and deterministic.
fn build_case(width: u32, height: u32, subject: Rect) -> (RgbImage, Mask) {
    let mut image = RgbImage::from_pixel(width, height, Rgb([120, 120, 120]));
    let mut mask = Mask { width, height, data: vec![0u8; (width * height) as usize] };
    for y in subject.y..(subject.y + subject.h).min(height) {
        for x in subject.x..(subject.x + subject.w).min(width) {
            image.put_pixel(x, y, Rgb([230, 230, 230]));
            mask.data[(y * width + x) as usize] = 255;
        }
    }
    (image, mask)
}

fn instance(mask: Mask, bbox: Rect) -> Instance {
    Instance { mask, bbox, class_id: 0, confidence: 0.9 }
}

#[test]
fn subject_touching_left_edge_is_detected() {
    let (image, mask) = build_case(200, 200, Rect { x: 0, y: 60, w: 80, h: 80 });
    let inst = instance(mask, Rect { x: 0, y: 60, w: 80, h: 80 });
    let input = RefinementInput { working_image: &image, original_image: &image, instance: &inst };

    let result = classify_instance(&input, 20, RefineParams::default());

    assert!(result.touches_edges.contains(&Edge::Left), "expected Left edge to be flagged as touching");
    assert!(!result.touches_edges.contains(&Edge::Right));
    assert!(!result.touches_edges.contains(&Edge::Top));
    assert!(!result.touches_edges.contains(&Edge::Bottom));
}

#[test]
fn subject_away_from_all_edges_is_not_touching() {
    let (image, mask) = build_case(200, 200, Rect { x: 60, y: 60, w: 80, h: 80 });
    let inst = instance(mask, Rect { x: 60, y: 60, w: 80, h: 80 });
    let input = RefinementInput { working_image: &image, original_image: &image, instance: &inst };

    let result = classify_instance(&input, 20, RefineParams::default());

    assert!(result.touches_edges.is_empty(), "expected no edges flagged as touching, got {:?}", result.touches_edges);
}

#[test]
fn subject_touching_two_adjacent_edges() {
    let (image, mask) = build_case(200, 200, Rect { x: 0, y: 0, w: 80, h: 80 });
    let inst = instance(mask, Rect { x: 0, y: 0, w: 80, h: 80 });
    let input = RefinementInput { working_image: &image, original_image: &image, instance: &inst };

    let result = classify_instance(&input, 20, RefineParams::default());

    assert!(result.touches_edges.contains(&Edge::Top));
    assert!(result.touches_edges.contains(&Edge::Left));
    assert!(!result.touches_edges.contains(&Edge::Bottom));
    assert!(!result.touches_edges.contains(&Edge::Right));
}

/// Mask and *image* disagree at the border: the mask stops 10 px (`mask_bottom`) above the bottom
/// edge. Pass 1 flags the bottom (the mask is within the gate margin); those last rows sit in the
/// unknown ring, so only matting decides them. `subject_to_border`: does the subject colour
/// continue into them?
fn border_disagreement_case(subject_to_border: bool) -> (RgbImage, Mask) {
    let (w, h) = (200u32, 200u32);
    let (x0, x1) = (80u32, 120u32);
    let mask_bottom = 190u32; // mask stops here
    let image_bottom = if subject_to_border { h } else { mask_bottom };

    let mut image = RgbImage::from_pixel(w, h, Rgb([120, 120, 120]));
    let mut mask = Mask { width: w, height: h, data: vec![0u8; (w * h) as usize] };
    for y in 60..image_bottom {
        for x in x0..x1 {
            image.put_pixel(x, y, Rgb([230, 230, 230]));
        }
    }
    for y in 60..mask_bottom {
        for x in x0..x1 {
            mask.data[(y * w + x) as usize] = 255;
        }
    }
    (image, mask)
}

#[test]
fn refinement_extends_the_mask_to_the_border_when_the_colour_continues() {
    let (image, mask) = border_disagreement_case(true);
    let inst = instance(mask, Rect { x: 80, y: 60, w: 40, h: 130 });
    let input = RefinementInput { working_image: &image, original_image: &image, instance: &inst };

    let result = classify_instance(&input, 20, RefineParams::default());

    // The raw mask stops 10px short of the bottom, so this verdict can only come from the matting
    // step reclassifying the unknown ring. If pass 2 were inert, this would be `false`.
    assert!(
        result.touches_edges.contains(&Edge::Bottom),
        "matting should have carried the subject to the border, got {:?}",
        result.touches_edges
    );
}

#[test]
fn refinement_does_not_extend_the_mask_when_the_colour_stops() {
    let (image, mask) = border_disagreement_case(false);
    let inst = instance(mask, Rect { x: 80, y: 60, w: 40, h: 130 });
    let input = RefinementInput { working_image: &image, original_image: &image, instance: &inst };

    let result = classify_instance(&input, 20, RefineParams::default());

    // Same geometry, but the last rows are background-coloured: matting must leave them background.
    assert!(
        !result.touches_edges.contains(&Edge::Bottom),
        "matting should not have invented a border touch, got {:?}",
        result.touches_edges
    );
}

/// A subject meeting the bottom border. `flare == 0`: a straight column, the silhouette a frame
/// truncates. `flare > 0`: widens by `flare` px per row away from the border, a hem grazing the
/// edge. Both reach the border row, so only the graze test tells them apart. See
/// docs/architecture/modules/core-shot-classifier-refine.md.
fn bottom_contact_case(base_w: u32, flare: u32) -> (RgbImage, Mask) {
    let (w, h) = (200u32, 200u32);
    let mut image = RgbImage::from_pixel(w, h, Rgb([120, 120, 120]));
    let mut mask = Mask { width: w, height: h, data: vec![0u8; (w * h) as usize] };
    for depth in 0..40u32 {
        let y = h - 1 - depth;
        let half = (base_w + flare * depth) / 2;
        for x in (w / 2 - half)..(w / 2 + half) {
            image.put_pixel(x, y, Rgb([230, 230, 230]));
            mask.data[(y * w + x) as usize] = 255;
        }
    }
    (image, mask)
}

#[test]
fn a_subject_the_frame_truncates_touches_the_border() {
    let (image, mask) = bottom_contact_case(60, 0);
    let inst = instance(mask, Rect { x: 70, y: 160, w: 60, h: 40 });
    let input = RefinementInput { working_image: &image, original_image: &image, instance: &inst };

    let result = classify_instance(&input, 20, RefineParams::default());

    // Coverage is the same at 5px and 20px deep, so the ratio is 1.0 — a real bleed-off.
    assert!(
        result.touches_edges.contains(&Edge::Bottom),
        "a perpendicular contact must count as touching, got {:?}",
        result.touches_edges
    );
}

#[test]
fn a_subject_that_only_grazes_the_border_does_not_touch_it() {
    let (image, mask) = bottom_contact_case(20, 4);

    // Guard the point of the test: the mask really does reach the border, so a negative verdict can
    // only come from the graze test and not from an absence of contact.
    assert!(
        (0..200).any(|x| mask.data[(199 * 200 + x) as usize] != 0),
        "fixture must actually touch the bottom border"
    );

    let inst = instance(mask, Rect { x: 12, y: 160, w: 176, h: 40 });
    let input = RefinementInput { working_image: &image, original_image: &image, instance: &inst };

    let result = classify_instance(&input, 20, RefineParams::default());

    // 20px wide at the border but 96px by 20 deep: mean coverage over the narrow band is well under
    // the wide band's, so the silhouette is running along the edge rather than off it.
    assert!(
        !result.touches_edges.contains(&Edge::Bottom),
        "a grazing contact must not count as touching, got {:?}",
        result.touches_edges
    );
}
