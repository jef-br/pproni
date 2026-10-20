# Tag protocol — jbpair / jbcomplete

A lightweight, in-code way to mark judgment-heavy regions so nothing gets decided
without the user. The scout writes the tags; the reviewer audits them.

## Form — always Rust comments (so the code still compiles)
Wrap the region above and below:

```rust
// <jbpair goal="feather width vs selective replacement at the warp seam">
let seam = feather(upper, lower, FILL_FEATHER_PX);
// </jbpair>
```

- `goal="..."` holds the one thing worth judging.
- On review/resolution, swap the pair for `jbcomplete`:

```rust
// <jbcomplete goal="feather width vs selective replacement at the warp seam">
let seam = feather(upper, lower, FILL_FEATHER_PX);
// </jbcomplete>
```

## Lifecycle
- **At rest, code carries no tags.** They exist only between embarking on a
  quest/task/plan and finishing it.
- `jbpair` = open, unresolved → the user has not yet judged this.
- `jbcomplete` = resolved → judged with the user (via `/pair`).
- **At quest end, both kinds are removed.** A finished, committed tree is clean.

## Enforcement
- **Build block:** the PreToolUse hook
  (`.claude/hooks/block-jbpair.sh`) fails the agent's `cargo build` while any
  `<jbpair` remains in `src/`. `jbcomplete` does not block. The user's own
  manual build is never blocked.
- **Quest-end gate:** the correctness reviewer fails the quest unless `src/` has
  **zero `<jbpair>` and zero `<jbcomplete>`**.
- **Confidence signal:** `jbcomplete` tags seen during a quest tell the reviewer
  the judgment points were surfaced and resolved, not slipped in.

## Scout tagging rule
- Tag whole functions/blocks, not stray lines.
- Trigger classes come from the `/pair` skill (`~/.claude/skills/pair/SKILL.md`)
  so the two never drift: algorithms, scoring/ranking/ordering, thresholds and
  tuning constants, geometry/numerical methods, matching, model/inference
  adaptation, concurrency, money/time arithmetic.
- Skip plumbing, renames, config wiring, DI, imports, test scaffolding.
