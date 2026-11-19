# core::runtime

[Up: architecture](../TOC.md)

Detail moved out of the `src/core/runtime.rs` comments. Why the ONNX Runtime is carried inside
the exe and unpacked rather than linked, and how it could be linked in later:
[5. Single hardened exe](../05-single-exe.md).

## Preloading `DirectML.dll`
`preload_directml` loads our `DirectML.dll` by full path before the runtime asks for it by name.
Windows hands a later request for the bare name `DirectML.dll` the copy already loaded, so the
runtime gets the version we shipped, not whatever older one sits in System32. A failure is not fatal
and not reported: the graphics-card route then fails to start and the model runs on the CPU.
