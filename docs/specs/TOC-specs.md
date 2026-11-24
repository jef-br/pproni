# Pproni — Design Spec & Source of Truth

[Up: docs home](../TOC-PProni.md)

*living document · maintained by Claude from your comments + chat · last edited 2026-11-24*

This is the single place the target Pproni behaviour is written down. It supersedes the flow
diagrams for "what it should do". Status tags mark reality vs intent. Comment on any line;
I fold changes back in here.

**Legend:**
* `BUILT` — done and in the code.
* `ONGOING` — partly built. Includes parked items that already have a partial build.
* `BROKEN` — built, but not working.
* `PLANNED` — decided, not built yet.
* `BACKLOG` — parked until Pproni is mature enough: items to confirm, "phase 2" ideas, parked items with no build yet.

| § | File |
|---|---|
| 1 | [Principles & hard invariants](01-principles.md) |
| 2 | [Pipeline overview](02-pipeline-overview.md) |
| 3 | [Shot Classifier (SC) — the brain](03-shot-classifier.md) |
| 4 | [Resolution & bbox policy](04-resolution-policy.md) |
| 5 | [Detection outcomes & routing](05-routing.md) |
| 6 | [Decisions locked](06-decisions-locked.md) |
| 7 | [Open questions](07-open-questions.md) |
| 8 | [Build status](08-build-status.md) |

*Pproni source of truth · comment to change any line · I maintain it from your comments + chat*

*Source artifact: https://claude.ai/code/artifact/b3d70cd2-f493-4d01-992b-ace7709b5bc3*
