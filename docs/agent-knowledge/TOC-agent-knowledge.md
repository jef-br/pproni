# Agent knowledge base

[Up: docs home](../TOC-PProni.md)

Should contain agreed, unambiguous facts the project's agents run on. One file per domain.
If anything feels ambiguous, surface it as a question. Otherwise, treat them as ground truth.
Shared by the **verification runner** and the **correctness reviewer** unless a line says otherwise.

| File | Domain | Who reads it |
|---|---|---|
| [paths-and-inputs.md](paths-and-inputs.md) | Where images live, what PPRONI-INPUT is, the build/run commands | runner (reviewer for context) |
| [core-image-roles.md](core-image-roles.md) | What each core image (1-51) is for | runner, reviewer |
| [acceptance-checks.md](acceptance-checks.md) | Each verification check as a number + how it's measured | runner, reviewer |
| [hard-bounds.md](hard-bounds.md) | Invariants that must never break | reviewer (runner for EIX/coverage) |
| [code-comments.md](code-comments.md) | What a Rust comment keeps, what moves to `docs/`, the pointer form, the checks | comment-trimmer, comment-auditor, reviewer |
