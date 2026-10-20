# Hard bounds

Invariants that must never break. The correctness reviewer checks every one of
these against each part's diff; a violation is a blocker, not a note.

1. **Single hardened exe.** `cargo build --release` produces one exe and nothing
   else. The `.onnx` model and `onnxruntime.dll` are embedded (model encrypted),
   never shipped as a file beside the exe. A fix that "puts a file next to the
   exe" is wrong by definition. (CLAUDE.md "Ships as one hardened exe";
   memory: pproni-single-file-requirement.)

2. **No veepee ties.** No veepee / vptech references, identities, data, or deps
   enter the code, tests, assets, or docs. (memory: no-veepee-ties.)

3. **EIX no-flip bound.** Never flip a freestanding object on a plain backdrop
   from `EIX0000`. A missed frame is acceptable; a wrongly-peeled margin is not.
   246 (the 246 frame case) is out of scope for the background-fill work and
   stays `EIX0000`.

4. **Permitted backgrounds.** A background is one of three types, and the output never
   leaves transparency:
   - **(a) Flat artificial** — most often 100% flat solid white.
   - **(b) Natural** — any colour, little texture, so easily stretchable.
   - **(c) Mixed / lifestyle / ambiance** — at least one major area that is *not* easily
     stretchable, so it needs seam-carving / inpainting. (Owned by the lifestyle/R1 session.)

5. **No unrequested behaviour.** No fallbacks, guards, retries, or new code paths
   beyond what the plan states, without the user agreeing first. (CLAUDE.md
   working rules.)

6. **Judgment code is surfaced, not decided.** Thresholds, warp/blend, seam-carve
   and the like are the user's call via `/pair`; the scout tags them, the
   reviewer confirms none were set silently. (See [tag-protocol.md](tag-protocol.md).)

7. **Diagram stays in sync.** A new/renamed/removed module, struct, enum, or
   public fn is patched into `JBA2B.drawio.svg` in the same commit.
