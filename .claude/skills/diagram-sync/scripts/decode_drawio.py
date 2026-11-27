#!/usr/bin/env python3
"""Extract the mxGraphModel XML embedded in a .drawio.svg file.

A .drawio.svg is a real SVG (so GitHub renders it) with the full diagram
source embedded in the root <svg>'s `content` attribute as HTML-entity-encoded
`<mxfile><diagram>...</diagram></mxfile>`. The <diagram> body is itself
base64 + raw-deflate + URL-encoded. This script reverses that so the actual
node/edge/container XML can be read (or grepped) directly.

This is read-only by design: editing the compressed blob by hand would leave
the visible SVG shapes stale, since they're a separate rendering baked into
the same file. Edit diagrams through the draw.io editor (VS Code extension or
draw.io desktop/web) so both the XML and the rendered SVG update together.

Usage:
    python3 decode_drawio.py path/to/diagram.drawio.svg
    python3 decode_drawio.py path/to/diagram.drawio.svg --diagram-index 1
"""
import argparse
import base64
import html
import re
import sys
import urllib.parse
import zlib

for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        _stream.reconfigure(encoding="utf-8")


def extract_xml(svg_path: str, diagram_index: int = 0) -> str:
    svg = open(svg_path, encoding="utf-8").read()
    m = re.search(r'\bcontent="([^"]*)"', svg)
    if not m:
        raise ValueError(f"no content= attribute found in {svg_path} — is this a real .drawio.svg?")
    mxfile = html.unescape(m.group(1))

    # DOTALL: an uncompressed <diagram> holds nested XML tags, not just base64 text.
    diagrams = re.findall(r'<diagram\b([^>]*)>(.*?)</diagram>', mxfile, re.S)
    if not diagrams:
        raise ValueError(f"no <diagram> blocks found inside content= in {svg_path}")
    if diagram_index >= len(diagrams):
        raise IndexError(f"{svg_path} only has {len(diagrams)} diagram page(s); index {diagram_index} out of range")

    attrs, data = diagrams[diagram_index]
    data = data.strip()
    if not data:
        raise ValueError("diagram data is empty")

    if data.startswith("<"):
        # Some files store the model uncompressed, as literal XML inside <diagram>.
        return data

    raw = base64.b64decode(data)
    inflated = zlib.decompress(raw, -15)  # raw deflate, no zlib/gzip header
    return urllib.parse.unquote(inflated.decode("utf-8"))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("svg", help="path to a .drawio.svg file")
    ap.add_argument("--diagram-index", type=int, default=0, help="page index if the file has multiple <diagram> pages (default 0)")
    args = ap.parse_args()

    try:
        xml = extract_xml(args.svg, args.diagram_index)
    except Exception as e:
        print(f"error: {e}", file=sys.stderr)
        sys.exit(1)
    print(xml)


if __name__ == "__main__":
    main()
