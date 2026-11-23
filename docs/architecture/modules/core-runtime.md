# core::runtime

[Up: module pages](TOC-module-architecture.md)

Detail moved out of the `src/core/runtime.rs` comments. Why the ONNX Runtime is carried inside
the exe and unpacked rather than linked, and how it could be linked in later:
[5. Single hardened exe](../05-single-exe.md).

## `DirectML.dll` is not embedded
The runtime asks Windows for `DirectML.dll` by name, and Windows 10 1903 and newer carry it in
System32. Pproni does not ship its own copy (decided 2026-11-23; it saved 17.7 MB). If a PC's copy
is missing or too old, the graphics-card route fails to start and the model runs on the CPU.
