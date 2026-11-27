# 3. Routing

[Up: table of contents](TOC-architecture.md)

Three routes, keyed on the shot classifier's verdict:

| Route | Verdict | Strategy |
|---|---|---|
| **R1** `CenterAndStretch` | a mask that describes a subject, with room to grow | square around the subject; bled edges pinned, background stretched to fill (`R1Job::Stretch`). White bars (from `core::framing`): the square is cut from the content box instead. A textured scene with flat bars: the trimmed box is reframed instead of stretched (`R1Job::Reframe`) |
| **R2** `CropSquare` | `mask_too_small`, or left+right bled and nothing else, or all four bled | the largest square already in the frame, centred on the frame |
| **R3** `Fallback` | no square can be planned around the subject; today also a model error, hidden (#31) | fit the whole image into a square canvas, centred, white fill |

**Three routes, six behaviours.** [Spec §5](../specs/05-routing.md)'s table is not a competing route list — it is
the *inside* of these routes. Four of its bleed rows are one rule seen from four angles, and
`center_and_stretch` applies that rule: a bled edge is pinned, an axis with a free side takes the
margin, and the slack goes to the free sides. Choosing among them is a behaviour, not a routing
decision, so it never reaches `route::decide`. Only the full bleed and left+right leave R1 for R2:
with all four edges blocked there is no background to stretch from, 
and left+right caps the square below what the subject needs.
A too-small mask goes to R2 before any of this.

R3 has no row in that table. It takes the images whose square does not fit, and today also no
verdict (a model error, #31): nothing cropped, nothing scaled, so a wrong guess costs framing rather
than pixels.

**Status:** all three routes are implemented. Spec behaviours are carried in their own module docstring.
