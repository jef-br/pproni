//! The three routes: R1 [`center_and_stretch`], R2 [`crop_square`], R3 [`fallback`].
//! See docs/architecture/03-routing.md.
//!
//! Each one turns the input picture into the output picture, and nothing else: which route runs,
//! and with what plan, is decided before it by `shot_classifier::route`, which also sends the image
//! here. A route never hands the image on to another one; the one check left inside a route is R1's
//! seam-carve quality gate on a textured scene, which needs the pixels it has just produced.
//! See docs/work-state/2026-10-29-routing-in-shot-classifier.md.

pub mod center_and_stretch;
pub mod crop_square;
pub mod fallback;
