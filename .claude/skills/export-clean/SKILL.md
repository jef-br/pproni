---
name: export-clean
description: >
  Publish Pproni to the shared repo as bare source: no comments, no docs, no built files.
  Strips a copy of HEAD into a separate clone of the shared repo, checks it still builds
  and passes tests, then commits and pushes only after the user approves. Manual only.
disable-model-invocation: true
argument-hint: "[commit message]"
---

# Export clean

This repo is the private master copy: comments, docs, diagrams all stay here. The shared
repo agrees on "no comments, no docs, no built files", so it gets a stripped copy made by
`scripts/export_clean.py`. Changes flow one way only, private → shared.

## What the script does

1. Reads **HEAD** of this repo, never the working tree. Uncommitted work is not exported.
2. Keeps only the files `config.json` allowlists (`include`, plus `python_tools` when
   `include_python_tools` is true), minus `exclude`.
3. Strips comments with a tokenizer, so `//` inside strings survives:
   - Rust: `//`, `///`, `//!`, nested `/* */`
   - Python: `#` comments and docstrings
   - TOML: `#` comments
   - `.gitignore` / `.gitattributes`: `#` lines
   - `verbatim` files are copied untouched (`Cargo.lock` is generated, images are binary).
   - Any other file type stops the export, so nothing leaks with its comments.
4. Refuses output containing any `forbidden_terms`.
5. Wipes the clean clone's tracked files, writes the new set, copies the gitignored build
   inputs (`models/*.onnx`, `onnxruntime-directml.dll`) in, then runs `verify` (`cargo fmt`,
   `cargo test`).
6. Stages everything with `git add -A`. It never commits or pushes.

Exit codes: `0` staged · `1` a check failed · `2` config needs a value · `3` the shared repo
has commits the clone does not.

## Steps

1. Run from the repo root, with a long timeout (the test build is slow):
   `python .claude/skills/export-clean/scripts/export_clean.py`

2. **Exit 2 — config needs a value.** Ask the user with AskUserQuestion, write the answer
   into `config.json`, and rerun. Never pick a value yourself.
   - `clean_repo`: path to a local clone of the shared repo, outside this repo. If they
     have none, offer to `git clone <url>` it next to this repo (e.g. `../pproni-clean`).
   - `include_python_tools`: whether the model-pipeline scripts in `tools/` count as source.

3. **Exit 3 — the shared repo moved on.** Someone else pushed. Show the listed commits and
   stop. Do not pull, merge, or port them. Those changes must be brought into this private
   repo by hand first. Ask the user how they want to handle it.

4. **Exit 1 — a check failed.** Show the error.
   - Build or test failure: most likely a stripper bug. Fix it in `export_clean.py`, test
     with a dry run (below), and rerun. Never hand-edit files in the clean clone; the next
     export wipes them.
   - Forbidden term: stop and tell the user which file. It is in code, not a comment.
   - Unknown file type: ask the user whether it belongs in `verbatim` or out of `include`.

5. **Exit 0 — staged.** Show the user the file list and diffstat the script printed. Before
   committing, spot-check one or two stripped `.rs` files in the clean clone for leftover
   comments.

6. **Commit** in the clean clone. Use the message passed as the skill argument if there is
   one. Otherwise draft one from `git log` in this repo since the last export, show it,
   and let the user edit it. The message must describe code changes only: no quotes from
   comments or docs, no private commit hashes.

7. **Push** only after the user says yes. Never force-push. If the push is rejected,
   treat it as exit 3.

## Dry run

`python .claude/skills/export-clean/scripts/export_clean.py --out <scratch dir>` writes the
stripped tree to a scratch directory with no git steps. Add `--no-verify` to skip the
cargo build. Use it to test changes to the stripper or the allowlist.
