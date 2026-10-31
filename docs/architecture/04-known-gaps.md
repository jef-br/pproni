# 4. Known gaps

[Up: table of contents](TOC.md)

- BiRefNet inference is always built in; `core::preflight` aborts at startup, before any file is
  touched, if the ONNX session can't be built (e.g. the embedded runtime can't be unpacked to any
  writable directory) — a batch never silently falls back to R3 for every image.
