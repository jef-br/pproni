# tests

[Up: architecture](../TOC.md)

The integration tests in `tests/`.

## tests/pipeline.rs: history
The detection/geometry tests that used to live in `tests/pipeline.rs` were retired with the
classical detector. What survives is the export invariant, which is independent of routing, plus
the preprocessor checks (working-copy bounds, alpha flattening).
