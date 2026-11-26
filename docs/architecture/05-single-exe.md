# 5. Single hardened exe, no install

[Up: table of contents](TOC-architecture.md)

**Pproni ships as one hardened executable with no installation step. This is a hard requirement.**
It runs on Windows only; Linux is never an option.

"Hardened" means the release build settings in `Cargo.toml`'s `[profile.release]`: fat LTO, one
codegen unit, symbols and debuginfo stripped, no PDB, abort on panic. They make the exe smaller and
harder to reverse-engineer. Both embedded models are off-the-shelf (BiRefNet with its `DeformConv`
ops rewritten, and YuNet from the OpenCV Zoo); their licences allow shipping them inside the exe
(confirmed by the owner, 2026-11-24).

**Met.** `cargo build --release` produces one exe file (~211MB) and nothing else. Three things are
compiled in:

- **The segmentation model.** `shot_classifier::birefnet` embeds one BiRefNet export with
  `include_bytes!` (its path is `config::birefnet_model_path!()`) and loads it straight from those
  bytes through `Session::commit_from_memory`. Nothing is written to disk. The shipped file is `birefnet_512_nodeform_op19.onnx` (~188MB); how it differs from the
  stock export: [models/README.md](../../models/README.md).
- **The philtrum model.** `preprocessor::maria_antonieta` embeds a second ONNX export (YuNet,
  OpenCV Zoo, BSD-3-Clause) the same way: its path is `config::yunet_model_path!()`. Used
  by `cut_above_philtrum` for the headcut feature; built once at startup by `core::preflight` so a
  corrupt export aborts the batch loudly.
- **The ONNX Runtime.** `core::runtime` embeds `onnxruntime-directml.dll` (~20MB, ORT 1.24, the
  DirectML build) the same way. It is the only runtime Pproni carries, and it serves
  both routes: the graphics card (DirectML) and the CPU. `ort`'s `load-dynamic` needs a real path to
  `LoadLibrary`, so at first use the bytes are written once to
  `<base>/pproni-<tag>/pproni.dll` and that path is handed to `ort::init_from`. `<base>` is the first writable of `%LOCALAPPDATA%`, the system temp folder and the exe's own folder. `<tag>` is a fingerprint of the
  embedded runtime: its size in hex, a dash, then a hash of three 4 KB samples of it (start,
  middle, end), for example `pproni-1422420-fd138fc75a784c81`. Each runtime build gets its own tag,
  so a rebuilt runtime lands in a fresh directory, repeat runs reuse the file already there, and the
  write is atomic (temp name then rename) so concurrent Pproni processes don't corrupt it. The exe never uses an ONNX Runtime that happens to be installed on the PC.

**`DirectML.dll` is not embedded.** The DirectML runtime calls Windows' own `DirectML.dll`, which
ships with Windows 10 version 1903 and newer (the docs name no edition limits; checked on Windows 11
Pro). If a PC lacks it, or has one too old, the graphics-card route fails to start and the model
runs on the CPU (`BiRefNetModel::build`). Decided 2026-11-23; it saved 17.7MB.

Both source files are gitignored — the models in `models/`, `onnxruntime-directml.dll` at the repo
root — and a missing one is a *build* failure naming the file, not a runtime surprise. `models/` is
the single place any `.onnx` lives; it holds every variant being tried, and only the paths in
`core::config` (`birefnet_model_path!`, `yunet_model_path!`) are baked in. A new model gets its path there too. The only files Pproni still touches beside the exe are optional
markers (`PPRONI_DEBUG_TAGS`). `ORT_DYLIB_PATH` can point at a hand-placed runtime instead of the
embedded one (a dev override, honoured by `core::runtime` for the exe and the `examples/` alike).

## Why the runtime is unpacked rather than linked

Static linking was the first choice and it doesn't work here. The only static ONNX Runtime pyke
publishes for `x86_64-pc-windows-msvc` (`ortrs_static`, listed in `ort-sys`' `dist.txt`) is **ORT
1.20.0** and CPU-only: it has no DirectML support, so it cannot use the graphics card. Newer `ort`
does not rescue it: from `ort-sys` rc.13 the Windows distribution is Microsoft's own build
(`ms@1.28.0`), a DLL — pyke stopped shipping static Windows libraries.

`DeformConv` is a second, separate fact. BiRefNet's stock export needs the native `DeformConv` op,
which only arrives in ORT 1.22 (the static 1.20 build fails at load with "Could not find an
implementation for DeformConv(19)"), and DirectML cannot run it at all. Re-exporting with an older
opset is no way out (it decomposes to `GatherND`, which allocates enormous intermediates: see the
docstring of `tools/2. export_onnx.py`). So the shipped model has those 20 nodes rewritten as
`GridSample` + `Conv`, and it no longer needs 1.22. The runtime is still a DLL, because of
DirectML — it is just carried *inside* the exe and unpacked on demand rather than shipped next
to it.

## Linking it in later, if a static ORT with DirectML appears

Build ONNX Runtime from source with CMake as a static library with the DirectML provider, point
`ort-sys` at it with `ORT_LIB_LOCATION`, and swap `load-dynamic` for the default linking. `ort`
rc.9 requests API version 20 and the ORT C API is forward-compatible, so a newer library serves it.
`birefnet`'s `init_runtime` and the whole of `core::runtime` then delete — nothing else in the
codebase knows the runtime is dynamic. This would drop the runtime unpack (a one-time ~20MB write
on first run) and shrink the exe slightly, but it is no longer on the critical path for the
single-file requirement.

## Note: the runtime DLL still needs the VC++ runtime

`onnxruntime-directml.dll` imports `msvcp140.dll` / `vcruntime140*.dll`. Those are present on any
up-to-date Windows 11 and were already a requirement of the old sidecar layout, so this is not a
regression — but a truly bare target still needs the Visual C++ Redistributable.
