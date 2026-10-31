# Working with draw.io diagrams

[Up: table of contents](TOC.md)

WYSIWYG diagrams for Pproni, edited as draw.io (diagrams.net) files — free, no account,
no server.

## One-time setup

VS Code extension recommendation lives in `.vscode/extensions.json` (gitignored like the
rest of `.vscode/`, so it's local-only — open this folder in VS Code and accept the
"install recommended extensions" prompt, or install manually: search **Draw.io
Integration** (`hediet.vscode-drawio`) in the Extensions view.

## Creating a diagram

`Ctrl+Shift+P` → **Draw.io: New Diagram** → save it here as `<name>.drawio.svg` (not
plain `.drawio`). That extension matters:

- It's a real SVG, so GitHub renders it as an image in the repo view and PR diffs —
  nothing extra needed on GitHub's side.
- The full diagram source is embedded in the SVG's own metadata, so reopening the same
  file in VS Code drops you back into the drag-and-drop editor. One file, no export step.

## Editing

Open the `.drawio.svg` file in VS Code — it opens straight into the draw.io canvas.
Drag shapes, draw connectors, resize, recolor, same as the web app.

## Grouping — use containers, not just proximity

For things that belong together conceptually, use an actual **container** shape (draw a
rectangle, tick "Container" in the format panel) rather than just placing shapes near
each other. Dropping a shape into a container re-parents it in the XML
(`parent="<container id>"`), so the grouping is explicit in the file, not just implied
by pixel position. Give the container a title and it reads as a labeled group visually
too.

## Naming

kebab-case, one diagram per concern, e.g. `pipeline-overview.drawio.svg`.

## Asking Claude to look at a diagram

Point Claude at the file path (or attach it). It reads the embedded XML directly for
the exact node/edge/container structure, and can render the SVG to a PNG when it needs
to check the visual layout too — same file, no separate format needed for either side.
