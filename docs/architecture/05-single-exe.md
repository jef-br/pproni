# 5. Single hardened exe, no install

[Up: table of contents](TOC.md)

**Pproni ships as one hardened executable with no installation step. This is a hard requirement.**

Hardening is already in place in `Cargo.toml`'s release profile — fat LTO, one codegen unit,
symbols and debuginfo stripped, no PDB, abort on panic — and is anti-RE as much as size. The
embedded model is also **encrypted at rest** (see below), so it no longer carves out of the exe
with `binwalk`/`strings`. This is obfuscation, not secrecy: the key ships in the binary, so it
raises the effort bar without pretending a determined analyst can't recover the weights.

**Met.** `cargo build --release` produces one exe file (~195MB) and nothing else. Three things are
compiled in:

- **The segmentation model.** `shot_classifier::birefnet` embeds one BiRefNet export and loads it
  through `Session::commit_from_memory`. It is stored **encrypted**: `build.rs` reads the plaintext
  out of `models/` — `MODEL_DIR` plus `MODEL_FILE` there are the only things that pick which export
  ships — encrypts it under a per-build key (a ChaCha20 keystream in `shot_classifier::cipher`,
  shared with the build script by path), and `include_bytes!`'s the encrypted blob from `OUT_DIR`;
  `birefnet::load` decrypts into a heap buffer only for as long as ORT needs to copy the graph in.
  The plaintext model is never a static string in the image and never touches disk.
- **The philtrum model.** `preprocessor::maria_antonieta` embeds a second ONNX export (YuNet,
  OpenCV Zoo, BSD-3-Clause) the same way, via `build.rs`'s `encrypt_slot` helper. One slot per
  model: `PHILTRUM_MODEL_FILE` picks the plaintext in `models/`, `philtrum.onnx.enc` and
  `philtrum_key.rs` emit beside the BiRefNet ones. Used by `cut_above_philtrum` for the headcut feature;
  built once at startup by `core::preflight` so a corrupt or missing export aborts the batch
  loudly.
- **The ONNX Runtime.** `core::runtime` embeds `onnxruntime.dll` the same way. `ort`'s
  `load-dynamic` needs a real path to `LoadLibrary`, so at first use the bytes are written once to
  `<%LOCALAPPDATA%|temp>/pproni-<tag>/pproni.dll` and that path is handed to `ort::init_from`.
  The `<tag>` is the library's size plus a content hash: a rebuilt runtime lands in a fresh
  directory, repeat runs reuse the file already there, and the write is atomic (temp name then
  rename) so concurrent Pproni processes don't corrupt it.

Both source files are gitignored — the model in `models/`, `onnxruntime.dll` at the repo root — and
a missing one is a *build* failure naming the file, not a runtime surprise. `models/` is the single
place any `.onnx` lives; it holds every variant being tried, and only `MODEL_FILE` decides which one
is baked in. The only files Pproni still touches beside the exe are
optional markers (`PPRONI_DEBUG_TAGS`) and the `ORT_DYLIB_PATH` override the `examples/` dev harness
uses to point at a hand-placed runtime.

## Why the runtime is unpacked rather than linked

Static linking was the first choice and it doesn't work here. pyke publishes a prebuilt *static*
ONNX Runtime for `x86_64-pc-windows-msvc` (`ortrs_static`, listed in `ort-sys`' `dist.txt`), but it
is **ORT 1.20.0**, and BiRefNet's decoder needs the native `DeformConv` op that only arrives in ORT
1.22:

```
Could not find an implementation for DeformConv(19) node with name 'node_deform_conv2d'
```

The session builds and then fails at load, on every image. Newer `ort` does not rescue it: from
`ort-sys` rc.13 the Windows distribution is Microsoft's own build (`ms@1.28.0`), a DLL — pyke
stopped shipping static Windows libraries. Re-exporting the model to avoid `DeformConv` is not the
way out either: without the native op it decomposes to `GatherND`, which allocates enormous
intermediates (see the docstring of `tools/2. export_onnx.py`). So the runtime stays a DLL — it is just carried
*inside* the exe and unpacked on demand rather than shipped next to it.

## Linking it in later, if a static ORT >= 1.22 appears

Build ONNX Runtime >= 1.22 from source with CMake as a static library, point `ort-sys` at it with
`ORT_LIB_LOCATION`, and swap `load-dynamic` for the default linking. `ort` rc.9 requests API
version 20 and the ORT C API is forward-compatible, so a 1.22+ library serves it. `birefnet`'s
`init_runtime` and the whole of `core::runtime` then delete — nothing else in the codebase knows
the runtime is dynamic. This would drop the runtime unpack (a one-time ~14MB write on first run)
and shrink the exe slightly, but it is no longer on the critical path for the single-file
requirement.

## Note: the runtime DLL still needs the VC++ runtime

`onnxruntime.dll` imports `msvcp140.dll` / `vcruntime140*.dll`. Those are present on any
up-to-date Windows 11 and were already a requirement of the old sidecar layout, so this is not a
regression — but a truly bare target still needs the Visual C++ Redistributable.
