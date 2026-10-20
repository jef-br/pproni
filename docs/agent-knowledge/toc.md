# Agent knowledge base

Should contain agreed, unambiguous facts the project's agents run on. One file per domain.
If anything feels ambiguous, surface it as a question. Otherwise, treat them as ground truth.
Shared by the **verification runner** and the **correctness reviewer** unless a line says otherwise.

| File | Domain | Who reads it |
|---|---|---|
| [paths-and-inputs.md](paths-and-inputs.md) | Where images live, what PPRONI-INPUT is, the build/run commands | runner (reviewer for context) |
| [stem-map.md](stem-map.md) | Plan label (det3, 268, mountain…) → actual image number | runner, reviewer |
| [acceptance-checks.md](acceptance-checks.md) | Each verification check as a number + how it's measured | runner, reviewer |
| [hard-bounds.md](hard-bounds.md) | Invariants that must never break | reviewer (runner for EIX/coverage) |
| [tag-protocol.md](tag-protocol.md) | The `<jbpair>`/`<jbcomplete>` comment convention + lifecycle | scout, reviewer |
