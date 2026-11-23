# Acceptance checks

Each verification target as a measurable check. The runner reports pass/fail with
the actual measured number — never "looks fine". Targets are image numbers in `JB core img batch`.

## How a pixel check is measured
- Pixel checks (wrong-colour band, % filled) run
  through a **throwaway Python/Pillow script the runner writes into the
  scratchpad each run** — nothing committed to the repo. It loads the output
  image and reports: the any-colour-band result (check 1, method below) and % filled. There is
  no transparent-pixel count: an input may have transparency, but preprocessing flattens it on white
  and the output is a JPEG, so the output has no transparent pixels to count.
- **Any-colour-band check (check 1):** a wrongly-introduced fill band of *any* colour
  (not only black `(0,0,0)`) fails. Measured by idempotency: trim the image (run
  `optimizer::trim_letterbox`; no command line trims alone) → `output1`,
  trim `output1` again → `output2`, then compare `diff(input, output1)` against
  `diff(output1, output2)`. A well-behaved trim is a near-no-op the second time, so
  `diff2 ≈ empty`; a spurious fill band makes `diff2` non-empty and fails the check.
- The EIX check (5) needs no pixel scan — the verdict is in the output filename
  (`--EIX=<trbl>`); the runner reads it there.

## Checks
| # | Target | Check | Pass when |
|---|---|---|---|
| 1 | 48, 49 | no wrong-colour band in fill | no band of **any** colour wrongly introduced (method above) |
| 2 | 48, 49 | coverage | 100% filled |
| 3 | 10, 11 | former grey streak | RETIRED 2026-11-20 (see above) |
| 4 | 46, 41 | textured scene | white bands only (centred, equal), nothing invented |
| 5 | 45 (hedge-trimmer blade, thin black frame all round) | no wrongful peel; frame case, out of scope for the background-fill work | EIX stays `0000` (baseline: 45 came out `EIX=0000` in the 2026-10-30 run) |

## Also required
- A synthetic unit test per fill path.
- A real image that forces a non-empty, unstretchable gated edge so inpaint
  actually runs (not just the easy path).
  - Rule: forced by whatever image needs it. Current expectation, not proven (`fill_segment` now
    fills only lines with no background at all): 10, 11, 48, 49 force the inpaint path; possibly
    also 41 and 46. The runner reports which
    images actually hit inpaint, so this list stays honest as the code evolves.
