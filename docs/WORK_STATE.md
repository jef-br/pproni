# Pproni work state

Read this first. Keep it short. Update it at the end of each work session.
Last updated: 2026-10-16.

## Decided
* The 512 model (`birefnet_lite_512.onnx`) is canonical — proven to work, and what `build.rs`
  bakes in (`MODEL_FILE`). The 384 int8 export is a parked, untested speed experiment, kept only as
  the commented-out alternative in `build.rs`; 448 is out of scope.
* The preprocessor ends with one function, `preprocessor::optimizer::optimize`. It does two steps, and
  its output is the only image BiRefNet sees (`Prepared::model_input`, cut from `working` at `Prepared::crop`):
  * 1: colour balance (Levels, once per image).
  * 2: trim, spec'd by the user on 2026-10-13: 16 border samples (4 per edge at 0/25/50/75%);
    outliers dropped by MAD (`TRIM_OUTLIER_FLOOR`/`_K`, always keeping the `TRIM_MIN_KEEP` closest to
    the median); the kept samples' min/max per channel is the backdrop colour range; gaps closed with
    a 2x2 window (`TRIM_CLOSE_K`);
    box around the product; squared to its largest side; margin = larger of 10 px and 10% of that
    side; the square is centred on the product and slid inside the frame (margin squeezed if the frame
    is too narrow; if the product is longer than the frame's short side the margin drops to 10 px and
    the crop is cut at the frame, not square); crop.
  * Nothing but backdrop left: `optimize` returns `Err`; the file stays in INPUT and counts as failed.
  * The model's mask is pasted back into a full working-size canvas by `CroppedModel`
    (`shot_classifier::segmentation`), so the gate, refinement and routes are unchanged.
* `working` and `original` stay true colour. BGC/FGC are sampled from `working`.
* The 5% rule (`MASK_MIN_COVERAGE`) is the user's call. Never change it unasked. It is the only 5% rule.
* No retries, no guards, no fallbacks anywhere in the trim. Add none without asking.
* Goal now: improve results by refining the preprocessor, not by fine-tuning the model.

## Rejected
* Raw-colour retry (adds another path).
* Blind zoom-to-65% retry.
* Recalibrating the int8 model (384 fp32 is no better).

## Parked: the first trim (code in commit `cc525bd`, not in the tree)
Replaced on 2026-10-13 by the trim above. Its extra rules (widen on a cut edge, keep step 1's box,
whole-frame guard, whole-frame retry, tolerance numbers) were not asked for and are not in the code.

## Open

### R1 stretch/inpaint follow-ups (2026-10-16)

* **Lifestyle / ambiance detection** — 2026-09-21-0 (snowboarder on mountains) still outputs
  white where the gated edges land, because the local-BGC fallback correctly refuses the
  textured strip. The user's hint was that preprocess's 16 border samples (`optimizer::border_samples`)
  plus the `prep.crop` fill-ratio can flag the image as lifestyle *before* R1 runs, after which
  the correct output is a per-edge content-aware stretch outside the segmask rather than white.
  **The current plan's write-up of these characteristics is likely a misinterpretation of what
  the user meant.** A dedicated planning conversation is needed — walk through the user's
  definition of lifestyle/ambiance (what the image looks like, what the segmask + border
  samples will show, what the desired output is) and agree on the routing *before* any code.
* **Telea / fast-marching inpaint** — kept on standby. The current classical fill is
  edge-aware lateral copy (`center_and_stretch::inpaint::fill_segment`). If visible striping
  shows up at corners where only one neighbour context exists, promote to narrow-band Telea
  before anything else.
* **ONNX LaMa-style inpaint kicker** — for truly textured scenes (lifestyle), the long-term
  option is embedded like BiRefNet (`shot_classifier::birefnet` + `cipher` + `build.rs`).
* **BGC confidence** — `colors::sample` returns a binary pass/fail. A dominant-cluster fraction
  would let R1 treat weakly-dominant backdrops more gracefully. `TODO(bgc-confidence)` left in
  `compose::local_edge_bgc`'s neighbourhood.
* **Pass-2 subtlety** — pass-2 classification reads `mid` (pass-1 output). A column pass-1
  inpainted with a lerp between two shoe pixels could land near BGC and re-stretch at pass 2.
  Rare in practice; known second-order effect.

### Older open items

* **Stale (pre-512, 384/int8-era) — kept only as a record.** Eight images where the 384 int8 model
  found nothing (847_det0, 579_det4, 579_det5, 648_det5, 653_det4, 759_det0, 759_det1, 812_det5) and
  the 579_det5 int8-vs-fp32 decoder-split findings (decoder split at D3→D1; int8 20-30% off fp32 in
  the encoder; fp32 found it on the raw trimmed crop, int8 never). No longer relevant under the 512
  model.
* First run of the new trim on the 9 images in `JB core img batch` (old vs new exe, scratch): 8 give the
  same edge tags and colours. IMG_2619_indigo loses its mask (25.6% -> 0%): the crop (327x919 of
  672x1000) is found empty by the model, so BGC/FGC drop out; EIX stays 0000. On 99218793_det1 a soft
  gradient backdrop falls outside the sampled colour range, so the crop stays wide (836x965 of 1000x1000).
  **Note:** the trim may be under active refactor in another session (plan
  `~/.claude/plans/read-these-plans-in-elegant-teacup.md`) — confirm there before acting on this.
* Idea, **still relevant**: BiRefNet applies ImageNet normalisation itself, so the image gets
  normalised twice (colour balance, then ImageNet) — still implemented under the 512 model. Possible
  last-resort counter-move. Not started.
* **Stale (pre-512, int8).** The "change one input at a time on 102454579_det5 and read int8's best
  score" experiment — int8-specific, no longer relevant under 512.

## State of the repo
* On `main`: `refined-single-384-model` was merged (commit `2f3dcbb`). The trim is committed —
  `preprocessor::optimizer` and the `TRIM_*` consts are in `HEAD`, including a later MAD-based
  sampler refinement (commit `0dfd66c`). Uncommitted working-tree changes remain (the R1
  stretch/inpaint work: `center_and_stretch/inpaint.rs` is new/untracked, `compose.rs` and others
  modified).
* `models/birefnet_lite_384.onnx` (fp32, gitignored) is a leftover test file.
