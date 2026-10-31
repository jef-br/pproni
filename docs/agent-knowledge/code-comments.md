# Code comments

[Up: agent knowledge](TOC.md)

How much a comment in the Rust source says, and when it points to the docs instead. Applies to
every `.rs` file. Checked by `python tools/check_comments.py` (see [Checks](#checks)); the
process that applies it to existing code is the `lighten-comments` skill.

## The rule
A comment says what the code next to it does, and why, in the fewest words that let the next
person change that code safely. Background, history and design write-ups live in `docs/`, once;
the comment points there.

## Stays in the code
* **Contracts.** What a public item takes, returns and promises; when it returns `None`/`Err`.
* **Units and meaning** of a constant or threshold, and who owns its value, as a short tag:
  "user's call, never change unasked", "starting value", "JB-calibrated".
* **Traps at the line.** Anything the next editor must see before touching that line: "must stay
  sorted", "working-resolution pixels", "keeps an exact tie on the side the rule says".
* **Short why.** One or two sentences on a choice that is not obvious from the code.
* `<jbpair>` / `<jbcomplete>` tags ([tag protocol](tag-protocol.md)), `SAFETY:` notes, `TODO`s.

## Moves to the docs (and leaves a pointer)
* **History:** "used to", "replaced", "tried and reverted", "was X before 2026-…".
* **Decisions and their story:** who decided what, on which date, which images it changed, which
  alternatives lost. These belong in `docs/work-state/`.
* **Design write-ups** longer than a short paragraph: the algorithm in prose, worked examples,
  tables, measurements. These belong in `docs/architecture/` or `docs/specs/`.
* **Anything already written in a doc.** Point to it; do not keep a second copy.

## The pointer
* Form: `See docs/<folder>/<file>.md.` — the repo-relative path, written out, ending in `.md`.
  An optional `§N` may follow. No `#anchors`: split a doc into topic files instead.
* It must resolve (`tools/check_docs.py` and `tools/check_comments.py` both check).
* Point to the one file that holds the detail. If several do, the detail is duplicated: fix the
  docs first (one source of truth), then point.
* Never delete information. If it is not in a doc yet, add it to the right doc first, then
  shorten the comment. When no existing doc fits, use the module's own page
  `docs/architecture/modules/<path-with-dashes>.md` (for `src/core/framing.rs`:
  `docs/architecture/modules/core-framing.md`) and list it in [architecture TOC](../architecture/TOC.md).
  A folder of small files may share one page named after the folder (`src/app/` →
  `docs/architecture/modules/app.md`), with one `##` section per file.

## Size guide
Soft limits; going over is allowed but the check lists it for a second look.

| Comment | Lines |
|---|---|
| Module doc (`//!`) | 12 |
| Item doc (`///`) | 8 |
| Inline (`//`) block | 4 |

## Never
* Change code while lightening comments. Comment-only edits; the check proves it.
* Remove a comment's information without it existing elsewhere (doc or remaining comment).
* Point to a doc that does not say what the removed comment said.

## Checks
`python tools/check_comments.py --base <git-ref> [files…]` compares the working tree with `<ref>`:

1. **Code unchanged** (fails): each file with comments stripped must be token-for-token equal.
2. **Pointers resolve** (fails): every `docs/….md` path in a comment exists.
3. **Kept information** (lists): words from removed comments — `identifiers`, numbers with units,
   dates, image numbers — that appear neither in the file's remaining comments nor in `docs/`.
   Each one is either fine (the meaning survives in other words) or must be put back.
4. **Size** (lists): comment blocks over the size guide.

`cargo check --all-targets` must pass afterwards (a stray `///` with no item after it is a
compile error).
