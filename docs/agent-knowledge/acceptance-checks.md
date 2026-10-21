# Acceptance checks

Each verification target as a measurable check. The runner reports pass/fail with
the actual measured number — never "looks fine". Labels resolve via
[stem-map.md](stem-map.md).

## How a pixel check is measured
- Pixel checks (wrong-colour band, % filled, transparency, streak-vs-background) run
  through a **throwaway Python/Pillow script the runner writes into the
  scratchpad each run** — nothing committed to the repo. It loads the output
  image and reports: the any-colour-band result (check 1, method below),
  transparent-pixel count, % filled, and the streak-vs-background metric below.
- **Any-colour-band check (check 1):** a wrongly-introduced fill band of *any* colour
  (not only black `(0,0,0)`) fails. Measured by idempotency: trim the image → `output1`,
  trim `output1` again → `output2`, then compare `diff(input, output1)` against
  `diff(output1, output2)`. A well-behaved trim is a near-no-op the second time, so
  `diff2 ≈ empty`; a spurious fill band makes `diff2` non-empty and fails the check.
- **Streak-vs-background metric (decided in /pair 2026-10-20):**
  - Reference = median colour of a **clean local background patch adjacent to
    the former streak** (not pure white, not the global BGC).
  - Target = median colour of the former-streak region.
  - Both are medians/trimmed means over a patch, never single pixels (robust to
    JPEG ringing — echoes the codebase's `drop_outliers` habit).
  - Compare the two with **CIEDE2000 (ΔE2000)**. PASS when `ΔE2000 ≤ threshold`.
  - **Optional extra confidence gate:** proximity of the streak median to the
    recorded `--BGC=<hex>` — informational, not the primary pass/fail.
- EIX checks (5, 6) need no pixel scan — the verdict is in the output filename
  (`--EIX=<trbl>`); the runner reads it there.
- Remaining item: only the numeric `ΔE2000` (CIEDE2000) threshold, being tuned in another session
  against real 10/11 output — not set here.

## Checks
| # | Target | Check | Pass when |
|---|---|---|---|
| 1 | det3, det4 (48, 49) | no wrong-colour band in fill | no band of **any** colour wrongly introduced (method below) |
| 2 | det3, det4 (48, 49) | coverage | 100% filled, zero transparent |
| 3 | 10, 11 | former grey streak | ΔE2000(streak median, local bg median) ≤ threshold (number in /pair) |
| 4 | 268 (46), mountain (41) | textured scene | no flat-white band present |
| 5 | large square + thin white margin | no wrongful peel | EIX stays `0000` |
| 6 | 246 (45) | frame case | stays `EIX0000` (246 frame case, out of scope) |

## Also required
- A synthetic unit test per fill path.
- A real image that forces a non-empty, unstretchable gated edge so inpaint
  actually runs (not just the easy path).
  - Rule: forced by whatever image needs it. Current expectation: 10, 11, 48, 49
    force the inpaint path; possibly also 41 and 46. The runner reports which
    images actually hit inpaint, so this list stays honest as the code evolves.
