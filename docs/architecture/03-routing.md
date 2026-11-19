# 3. Routing

[Up: table of contents](TOC.md)

Three routes, keyed on the shot classifier's verdict:

| Route | Verdict | Strategy |
|---|---|---|
| **R1** `CenterAndStretch` | a mask that describes a subject, with room to grow | square around the subject; bled edges pinned, background stretched to fill |
| **R2** `CropSquare` | `mask_too_small`, or left+right bled and nothing else, or all four bled | the largest square already in the frame, centred on the frame |
| **R3** `Fallback` | no verdict at all | fit the whole image into a square canvas, centred, white fill |

**Three routes, six behaviours.** [Spec §5](../specs/05-routing.md)'s table is not a competing route list — it is
the *inside* of these routes. Four of its bleed rows are one rule seen from four angles, and
`center_and_stretch` applies that rule: a bled edge is pinned, an axis with a free side takes the
margin, and the slack goes to the free sides. Choosing among them is a behaviour, not a routing
decision, so it never reaches `Route::select`. Only the full bleed leaves R1, because with all four
edges blocked there is no background anywhere to stretch from.

R3 has no spec row. It is the answer to "we don't know": nothing cropped, nothing scaled, so a wrong
guess costs framing rather than pixels.

**Status:** all three routes are implemented. Spec behaviours are carried in their own module docstring.
