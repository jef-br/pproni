# build

[Up: module pages](TOC-module-architecture.md)

`build.rs`, the build-time step that shapes the final exe. Model embedding (done with `include_bytes!`
in the model files, not in `build.rs`; the paths are in `core::config`): [5. Single hardened exe](../05-single-exe.md). This page
holds only what that one does not.

## The app icon
`build.rs` stamps the Windows app icon onto the binary; Explorer, the taskbar and the console read
it straight from the exe. `Pepperoni.ico` is committed at the repo root: unlike the models and the
runtime it is small, so it lives in the repo rather than being a build-time prerequisite.
