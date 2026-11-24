# Hard bounds

Invariants that must never break. The correctness reviewer checks every one of
these against each part's diff; a violation is a blocker, not a note.

1. **Single hardened exe.** `cargo build --release` produces one exe and nothing
   else. "Hardened" means the release build settings (fat LTO, stripped, no PDB, abort on
   panic). The two `.onnx` models and one runtime,
   `onnxruntime-directml.dll`, are embedded, never shipped as a file beside the exe.
   The exe must not rely on an ONNX Runtime installed on the PC. The only Windows part
   it uses is `DirectML.dll` (Windows 10 1903+), and without it the model runs on the
   CPU. A fix that "puts a file next to the exe" is wrong by definition. (CLAUDE.md
   "Ships as one hardened exe"; memory: pproni-single-file-requirement.)

2. **No veepee ties.** No veepee / vptech references, identities, data, or deps
   enter the code, tests, assets, or docs. (memory: no-veepee-ties.)

3. **EIX no-flip bound.** Never flip a freestanding object on a plain backdrop
   from `EIX0000`. A missed frame is acceptable; a wrongly-peeled margin is not.
   (*Flip* = the edge verdict changes from `0000` to anything else. *Peel* = the framing step that
   crops away a frame round the picture, `Decision::Peel` in `core::framing`; a wrongly-peeled margin
   is a peel on a picture that has no frame.)
   45 (the frame case) is out of scope for the background-fill work and
   stays `EIX0000`.

4. **Permitted backgrounds.** A background is one of three types, and the output never
   has a transparent area: preprocessing flattens any alpha on white and the output is a JPEG:
   - **(a) Flat artificial** — most often 100% flat solid white.
   - **(b) Natural** — any colour, little texture, so easily stretchable.
   - **(c) Mixed / lifestyle / ambiance** — at least one major area that is *not* easily
     stretchable, so it needs seam-carving / inpainting. (Handled in R1.)

5. **No unrequested behaviour.** No fallbacks, guards, retries, or new code paths
   beyond what the plan states, without the user agreeing first. (CLAUDE.md
   working rules.)

6. **Judgment values are the user's.** Thresholds, warp/blend, seam-carve and the
   like are set only with the user's agreement, never silently.

7. **Diagram stays in sync.** A new/renamed/removed module, struct, enum, or
   public fn is patched into `JBA2B.drawio.svg` in the same commit.
