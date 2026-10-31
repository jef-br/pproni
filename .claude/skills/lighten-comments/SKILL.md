---
name: lighten-comments
description: >
  Shorten Pproni's Rust comments so they point to docs/ instead of repeating them, without
  losing information or changing code, following docs/agent-knowledge/code-comments.md. Use when
  asked to lighten, trim or thin out code comments, to move comment detail into the docs, or to
  check comment edits against the comment rule.
---

# Lighten comments

The rule is `docs/agent-knowledge/code-comments.md`. Read it first; this skill is only the
process that applies it and proves it was applied. You are the orchestrator: you plan, start the
agents, run the checks and decide when to stop. Agents do the edits and the audits.

## 0. Before starting
* Target files: the ones named, or every `.rs` file. Skip none silently: list any you leave out.
* Snapshot: copy every target file to `<scratchpad>/lighten-<date>/` (same relative paths). This
  is the restore point; `git` alone is not, because the files may already have uncommitted edits.
* Base for the checks: `BASE=$(git rev-parse HEAD)`. The check compares code tokens, so earlier
  comment-only edits in the working tree are fine.
* `python tools/check_docs.py` passes (or only lists known, explained items).

## 1. Batches
Split the targets into batches of files that share a topic (one module folder), about 150-350
comment lines per batch (`awk '/^[[:space:]]*\/\//' file | wc -l` counts them). Each batch
owns its `.rs` files and its module pages `docs/architecture/modules/<path-with-dashes>.md`, and
nothing else. Two agents never own the same file, so they can run in parallel.

## 2. Trim
Start one `comment-trimmer` agent per batch, in parallel. Give it: its file list, its module page
names, `BASE`, and the rule file. Shared docs (anything in `docs/` outside its module pages) it
only reads; detail with no home goes to its module page.

## 3. Check (deterministic)
For each batch, once its trimmer reports:
```
python tools/check_comments.py --base $BASE <batch files>
python tools/check_docs.py
```
Check 1 (code unchanged) or 2 (pointer resolves) failing is a blocker. The "found nowhere else"
and size lists go to the auditor.

## 4. Audit (independent)
Start one `comment-auditor` agent per batch (fresh context, read-only). Give it the batch files,
`BASE` and the check output. It returns findings, each with `file:line`, what is wrong, and the fix.

## 5. Fix loop — at most 2 rounds
If step 3 failed or step 4 found anything: send the findings to that batch's trimmer
(SendMessage, so it keeps its context) and repeat steps 3-4 for that batch.
**Stop after 2 fix rounds**, pass or not. Never start round 3. Then:
* A file still failing check 1 (code changed): restore it from the snapshot.
* Any other finding still open: leave the file as it is and report the finding to the user.
* A finding the trimmer disputes with a reason: do not loop on it; report both sides.

## 6. Finish
1. List every new module page in `docs/architecture/TOC.md` (a "Module pages" table).
2. `cargo check --all-targets` passes (a stray `///` with nothing after it fails to compile).
3. `python tools/check_comments.py --base $BASE` over all targets, and `python tools/check_docs.py`.
4. Report per file: comment lines before -> after, pointers added, module page written, open
   findings. Report files restored or skipped, and why.
