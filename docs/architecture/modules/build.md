# build

[Up: architecture](../TOC.md)

`build.rs`, the build-time steps that shape the final exe. Model embedding and encryption as a
whole: [5. Single hardened exe](../05-single-exe.md). This page holds only what that one does not.

## The app icon
`build.rs` stamps the Windows app icon onto the binary; Explorer, the taskbar and the console read
it straight from the exe. `Pepperoni.ico` is committed at the repo root: unlike the models and the
runtime it is small, so it lives in the repo rather than being a build-time prerequisite.

## Model slots
* The encrypted output names (`<slot>.onnx.enc`, `<slot>_key.rs` in `$OUT_DIR`) are fixed, so
  swapping which model ships is one edit in `build.rs` (the slot's filename constant), not two.
* `src/core/shot_classifier/cipher.rs` is an XOR-symmetric ChaCha20 keystream: the same call
  encrypts here and decrypts at load, which is why `build.rs` pulls the file in by path.
