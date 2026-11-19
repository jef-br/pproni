//! The three routes: R1 [`center_and_stretch`], R2 [`crop_square`], R3 [`fallback`].
//! See docs/architecture/03-routing.md.
//!
//! A route only changes the picture and never hands it on: which route runs, and with what plan, is
//! decided before it by `shot_classifier::route`.
//! See docs/architecture/modules/core-processor-routes.md.

pub mod center_and_stretch;
pub mod crop_square;
pub mod fallback;
