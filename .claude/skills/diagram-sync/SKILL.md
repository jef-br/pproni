---
name: diagram-sync
description: >
  Keep Pproni's docs/diagrams/*.drawio.svg diagrams (currently JBA2B, the
  end-to-end App/Core flowchart) in sync with the code they document,
  and always read/edit them through their embedded mxGraph XML rather than
  as plain images. Use this whenever you add, rename, or remove a module,
  struct, enum, or public function/method in Pproni's Rust source — check
  whether it has a matching diagram node and patch it in the same commit.
  Also use this before opening, reading, or editing any .drawio.svg file
  directly, and whenever the user asks to check, sync, or reconcile the
  diagrams with the code (e.g. "check diagram sync", "do the diagrams still
  match the code", "update the diagrams for this change").
---

# Diagram sync

Pproni documents its architecture with draw.io diagrams under `docs/diagrams/`, referenced
from `docs/architecture/06-diagram.md`. They are real SVGs — GitHub renders them as images — but
each one also carries its full draw.io source embedded inside itself, so opening the
same file in an editor drops you back into an editable diagram, not a dead picture.
That dual nature is the thing to respect: treat the file as a diagram with a picture
attached, not a picture with metadata attached.

**Current inventory — read this before assuming a diagram exists.** One file:
`JBA2B.drawio.svg`, an end-to-end flowchart across four containers (App; Core, holding
Preprocessor → Shot Classifier → Processor → Exporter; and a proposed Pproni-Analyzer).
The four topic diagrams this skill used to describe — `class-map`, `pipeline-flow`,
`detection-internals`, `geometry-routing` — were deleted along with the classical
detector they documented. Do not look for them, and do not treat their absence as
something to repair by recreating them under the old names; the pipeline they described
no longer exists.

`JBA2B.drawio.svg` was rebuilt against the current code on 2026-09-09 and is in sync;
the drift that used to be listed here — a Shot Classifier and Processor still describing
the retired detector — is gone. `docs/architecture/06-diagram.md` says what each container
covers. It now has six containers, not four: the proposed Pproni-Analyzer lane was replaced
by a dev-harness lane for `examples/run_dir.rs`.

Two situations bring you here: **code changed** (does a diagram need a matching edit?)
and **a diagram itself needs reading or editing** (are you touching the right layer?).

## 1. Code changed — does a diagram need to follow?

The maintenance rule (source of truth: `docs/architecture/06-diagram.md`; restated in `CLAUDE.md`): one
node per module, one node per `struct`/`enum`. If you added, renamed, or removed a
module, a `struct`/`enum`, or a public function/method, check whether it's represented:

1. Work out which container of `JBA2B.drawio.svg` covers it:
   - **App** — folder discovery, the batch loop, the close prompt.
   - **Preprocessor** — decode, EXIF orientation, alpha flatten, sRGB, working copy.
   - **Shot Classifier** — segmentation and the two-pass edge verdict.
   - **Processor** — `Route::select` and the three routes.
   - **Exporter** — save, then delete the source on success.
2. Decode the file's XML (see §2) and search it for the module/type name to confirm
   whether a node already exists.
3. If it exists and your change affects what the node says (renamed, resigned,
   added/removed a public method), edit the node. If it's new public surface with no
   node yet, add one. If you removed the code, remove the node — a node with no
   matching file is exactly the drift `docs/architecture/06-diagram.md` warns about.
4. Make the diagram edit through the actual draw.io editor (see §2.2), and commit it
   alongside the code change — not as a follow-up.

A change entirely internal to a function body, with no change to its signature or to
the module/type structure, doesn't need a diagram edit — the diagrams model shape, not
implementation detail.

## 2. Reading or editing a .drawio.svg file

**Never rely on the rendered picture as your source of truth for structure.** The SVG
drawing is a rendering, generated from the diagram's real source, which is a separate
mxGraph XML document embedded inside the same file. Looking at the picture (or
screenshotting it) can tell you rough layout, but it can't tell you accurately which
node is nested inside which container, exact labels, or edge endpoints — for that you
need the XML.

### 2.1 Reading: decode the embedded XML

The XML lives in the root `<svg>` tag's `content="..."` attribute, HTML-entity-escaped,
wrapping `<mxfile><diagram>...</diagram></mxfile>`. The `<diagram>` body itself is
usually base64 + raw-deflate + URL-encoded (draw.io's default compression), though some
files store it as plain nested XML instead — both forms exist in this repo.

Don't hand-decode this. Run the bundled script:

```bash
python3 .claude/skills/diagram-sync/scripts/decode_drawio.py docs/diagrams/JBA2B.drawio.svg
```

This prints the `<mxGraphModel>` XML: every `<mxCell>` node/edge, its `value` (label),
`style`, and — critically — its `parent="<id>"`, which is how draw.io containers
express grouping. A shape's `parent` pointing at a container's `id` is a real,
structural grouping, not just something that happens to look nearby; don't infer
grouping from coordinates alone, read `parent`.

From there, grep/read the printed XML like any other text — search for a module name,
list all `parent="app"`-style children, etc.

Rendering the file to PNG (e.g. via a headless browser or image tool) is a fine
*supplement* when you need to sanity-check the visual layout — spacing, overlap,
whether a container reads as a container — but it's never a substitute for reading the
XML when the question is about structure (which nodes, which edges, which containers).

### 2.2 Editing: use the draw.io editor, not the raw blob

The decode script is read-only on purpose. If you hand-edit the compressed XML blob and
write it back into the SVG, the file's `content` attribute and its visible `<path>`/`<g>`
drawing elements fall out of sync — the source of truth changes but the picture GitHub
renders doesn't, silently reintroducing the exact problem this convention exists to
prevent.

Edit diagrams the way `docs/diagrams/drawio-files.md` already describes: open the
`.drawio.svg` in the **Draw.io Integration** VS Code extension
(`hediet.vscode-drawio`), which understands the embedded format and regenerates both
the XML and the rendered SVG together on save. Use containers (a real container shape,
not just placing boxes near each other) when adding something that conceptually belongs
inside an existing group — that's what makes the `parent=` relationship in the XML.

If no interactive editor is available, don't stop at describing the change — use the
headless renderer in §2.3, which rewrites both layers together the same way.

### 2.3 Editing without the editor: render both layers headlessly

The extension bundles the whole drawio webapp, and headless Chrome can drive it, so a
session with no GUI can still produce a real drawio export rather than a hand-patched
blob. `scripts/render_drawio.py` does exactly that: it loads drawio's own `Graph`,
decodes the model into it, calls `graph.getSvg()`, and writes the `content=` attribute
from the same model. Both layers come out of one source, which is the whole point.

```bash
python3 .claude/skills/diagram-sync/scripts/decode_drawio.py docs/diagrams/JBA2B.drawio.svg > model.xml
# edit model.xml — plain <mxGraphModel> XML, the same thing decode prints
python3 .claude/skills/diagram-sync/scripts/render_drawio.py model.xml docs/diagrams/JBA2B.drawio.svg
```

Decode → render with no edit in between reproduces the same model and the same drawing,
so the round-trip is safe to lean on. Two things it does not do for you: it can't tell
you a box is too small for its text, and it can't tell you an edge label landed on top
of a shape. Screenshot the result (Chrome `--screenshot` on the SVG) and look at it
before committing — labels colliding with box text is the usual failure.

A caution when writing labels: drawio renders them as HTML, so text containing `<stem>`
or `<input>` is parsed as a tag and disappears. Escape it for HTML *and* for the XML
attribute it lives in — two rounds, not one.

## 3. On explicit request: check sync

When asked to check/sync the diagrams (without a specific code change prompting it),
walk every file in `docs/diagrams/` — today that is `JBA2B.drawio.svg` alone:

1. Decode it with the script.
2. List the module/struct/enum nodes it contains (their `value` labels), per container.
3. Compare against the actual current code for that container's territory (grep the
   relevant `src/` modules, or ask ripgrep for `pub struct`/`pub enum`/`pub fn`/`mod`
   declarations in that container's scope).
4. Report drift plainly: nodes with no matching code (removed/renamed and diagram not
   updated), and public code with no matching node (added and diagram not updated).

Check the outstanding list in `docs/architecture/06-diagram.md` first — known drift is already
written up there node by node, so start from it rather than rediscovering it.

This is a manual, judgment-based comparison — not an automated AST-to-XML diff. Don't
build tooling to fully automate it; the diagrams are small enough that reading their
decoded XML alongside the module list is fast and more reliable than a brittle parser.
