# Stem map — plan label → image

This table resolves the shorthand image labels (det3, 268, mountain…) used across
agent-knowledge to a file in `JB core img batch`. The runner and reviewer read it
as ground truth. Filled 2026-10-20 after the user moved the extra images in and we
renamed them to continue the 1–40 numbering.

| Plan label | Image | Was | Role in verification |
|---|---|---|---|
| det3 | `48.jpg` | `102454740_det3.jpg` | black-band bug; expect no `(0,0,0)`, 100% coverage |
| det4 | `49.jpg` | `102454741_det4.jpg` | black-band bug; same checks as det3 |
| 10 | `10.jpg` | (pre-existing) | grey-streak; former streak ≈ background |
| 11 | `11.jpg` | (pre-existing) | grey-streak; same as 10 |
| 268 | `46.jpg` | `2026-09-21-268.jpg` | textured scene; no flat-white band |
| 246 | `45.jpg` | `2026-09-21-246.jpg` | frame case — **out of scope** for the background-fill work; stays EIX0000 |
| mountain | `41.jpg` | `2026-09-21-0.jpg` | textured scene; no flat-white band |

Other renamed images, tied to no current verification check (roles set by the user 2026-10-20):

- `42` (was `-78`) — spare.
- `43` (was `-82`) — detector stress image, for the upcoming head-cutting feature (which will turn
  framings like 25/26/27 into ones like 34).
- `44` (was `-114`) — detector stress image; the head-cutter must **not** modify it.
- `47` (was `-392`) — a tricky-to-detect product.
