# core::shot_classifier

[Up: architecture](../TOC.md)

Detail moved out of the small files in `src/core/shot_classifier/`: `mod.rs`, `gate.rs`,
`cipher.rs` and `gpu_adapters.rs`. The two passes are [spec §3.1](../../specs/03-shot-classifier.md);
the module table is [2. Modules: `core::shot_classifier`](../02-modules-shot-classifier.md).

## mod.rs: how the classifier is wired
* `segmentation::SegmentationModel` lets gate and refine run against a stand-in model.
* `core::classify` (`super::classify`) wires the real BiRefNet up and feeds `classify_instance` the
  preprocessor's two resolutions.
* Last, `route` reads the verdict, picks the route and sends the image to it.
* `plan` is the square R1 is told to cut, which the choice of route depends on.
* Pass 1 (`gate`) keeps pass 2 off the common case: a subject nowhere near an edge.

## gate.rs: pass 1
What pass 1 does and why a hit is not a verdict: [spec §3.1](../../specs/03-shot-classifier.md).
Scanning only the border band is what keeps it fast across a batch.

## cipher.rs: what the keystream is for
A ChaCha20 keystream (RFC 8439), used only to obfuscate the embedded model at rest; why that is not
secrecy: [5. Single hardened exe](../05-single-exe.md). It is XOR-symmetric, so the same routine
encrypts (in `build.rs`) and decrypts (at load); the build side is in [build](build.md).
There is no MAC: integrity is not a goal, and a corrupted model simply fails the session build in
`core::preflight`.

## gpu_adapters.rs: which graphics card is best
DirectML addresses a card by its position in DXGI's adapter list, so an `Adapter`'s `device_id` is
simply that position. "Best" means most dedicated video memory: a real card (NVIDIA, AMD) carries
gigabytes of its own, a shared-memory chip (Intel Iris) a token 128MB and borrows system RAM for the
rest. The software adapter ("Microsoft Basic Render Driver") is left out: it is the CPU in disguise
and slower than the CPU path.
