# Decided 2026-11-25: DirectML runs with metacommands off (user; code in `birefnet::append_directml`, ticket #29)

[Up: table of contents](TOC-work-state.md)

* Bug: in about 1 batch run in 3, the NVIDIA card faulted partway through. The image on the card got
  `887A0006` (GPU hung) and every image after it got `887A0005` (GPU removed). `core::classify` dropped
  the error, so those images came out `EIX=____` on R3 while the console said "51 ok, 0 failed".
  One model run hung for about 78 s, which is why a failed run took about 6 times longer. Windows logged
  `nvlddmkm` event 153 at the fault and again when Pproni exited.
* Cause: DirectML's metacommands, the fast kernels the graphics driver supplies itself (here the NVIDIA RTX
  A1000 Laptop GPU, driver 32.0.15.9706). With them off, DirectML uses its own kernels.
* Ruled out:
  * A GPU timeout: the Windows limit on this PC is 8 s, and images finished every 0.15 s right up to the fault.
  * The worker threads: it also failed with one thread (`RAYON_NUM_THREADS=1`).
  * One bad image: the fault came at the 8th to 10th model run, on a different image each time.
* Fix: the DirectML session is set up with `disable_metacommands=true`. The `ort` crate (rc.9) takes only a
  device id, so the setup goes through ONNX Runtime's generic call (`SessionOptionsAppendExecutionProvider`
  with `"DML"`). If a card cannot be set up, the next card is still tried, as before.
* Measured (core set of 51 images, a fresh copy for every run):

  | Build | Card | Runs | Runs with `EIX=____` |
  |---|---|---|---|
  | Metacommands on (old code) | NVIDIA | 18 | 5 |
  | Metacommands off | NVIDIA | 44 | 0 |
  | Metacommands off | Intel Iris Xe | 6 | 0 |

  * Speed: NVIDIA unchanged (12–15 s per run). Intel 30 s, against 28 s with metacommands on.
  * Output: on NVIDIA, byte-identical to the old code. On Intel, one mask file (`5_segmask.jpg`) has
    1 pixel out of 857,476 flipped at the mask threshold; the final image is the same.
  * No driver events in any run with metacommands off.
* Still open: `core::classify` still drops a model error silently (`found.ok()?`). Whether to report it is a
  separate decision, moved to ticket #31 (telemetry).
