//! The three routes. Each one turns the input picture into the output picture, and nothing else:
//! which route runs, and with what plan, is decided before it by `shot_classifier::route`, which
//! also sends the image here.
//!
//! | Route | Does |
//! |---|---|
//! | R1 [`center_and_stretch`] | cuts the square it is told to around the subject, bled edges pinned, background stretched to fill |
//! | R2 [`crop_square`] | takes the largest square already in the frame |
//! | R3 [`fallback`] | fits the whole image into a square canvas |
//!
//! A route never hands the image on to another one. The single check that remains inside a route is
//! R1's seam-carve quality gate on a textured scene, which can only be made on the pixels it has just
//! produced.

pub mod center_and_stretch;
pub mod crop_square;
pub mod fallback;
