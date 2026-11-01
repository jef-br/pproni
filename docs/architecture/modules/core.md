# core

[Up: architecture](../TOC.md)

Detail moved out of the comments of the top-level `src/core/` files. The module table is
[2. Modules: `core`](../02-modules-core.md); the stages are [1. Pipeline](../01-pipeline.md).

## options.rs

### How `Options` travels
The frontend builds one `Options` before the batch starts; `core::process_file` then hands the same
`&Options` to every stage as a plain argument. Nothing changes it after that, and no stage reads it
from anywhere else.
