#!/usr/bin/env python3
"""Render an mxGraphModel XML file into a .drawio.svg, both layers written together.

The counterpart to `decode_drawio.py`. That one is read-only because hand-editing the
compressed blob leaves the visible SVG stale; this one avoids that by regenerating the
drawing and the `content=` attribute from the same model, the way the editor does.

It does it by driving draw.io's own renderer: the `hediet.vscode-drawio` VS Code
extension ships the full drawio webapp, and headless Chrome loads it, decodes the model
into a `Graph`, and calls `graph.getSvg()`. So the output is drawio's output — not an
approximation of it — and reopening the file in the extension shows the same diagram.

Usage:
    python3 render_drawio.py model.xml out.drawio.svg [--name JBA2B] [--id page1]

`model.xml` is a bare `<mxGraphModel>…</mxGraphModel>` document — exactly what
`decode_drawio.py` prints, so decode → edit the XML → render round-trips.
"""
import argparse
import base64
import glob
import html
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.parse
import zlib

for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        _stream.reconfigure(encoding="utf-8")

# drawio stamps this on its own exports; keep it so the file looks like the editor's.
HOST = "65bd71144e"

CHROME_CANDIDATES = [
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
]

PAGE = """<!DOCTYPE html>
<html><head><meta charset="utf-8">
<style>body{margin:0}#c{position:absolute;visibility:hidden;width:1px;height:1px;overflow:hidden}</style>
<script>
  window.RENDER_DONE = null; window.RENDER_ERR = null;
  urlParams = {'offline':'1','sync':'manual','math':'0'};
  mxBasePath = 'mxgraph'; mxLoadStylesheets = false; mxLanguage = 'en'; mxLoadResources = false;
</script>
<script src="js/viewer-static.min.js"></script>
</head><body><div id="c"></div>
<script>
var P = __PAYLOAD__;
window.addEventListener('load', function () {
  var out;
  try {
    var graph = new Graph(document.getElementById('c'));
    graph.setEnabled(false);
    graph.foldingEnabled = false;
    var doc = mxUtils.parseXml(P.model);
    new mxCodec(doc).decode(doc.documentElement, graph.getModel());
    graph.refresh();
    var svg = graph.getSvg(null, 1, 0, false, null, true, true, null, null, false, false, null, null);
    svg.setAttribute('content', P.mxfile);
    out = new XMLSerializer().serializeToString(svg);
  } catch (e) {
    out = 'ERROR:' + String((e && e.stack) || e);
  }
  var pre = document.createElement('pre');
  pre.textContent = '<<<BEGIN>>>' + out + '<<<END>>>';
  document.body.innerHTML = '';
  document.body.appendChild(pre);
});
</script></body></html>
"""


def find_webapp():
    """The drawio webapp bundled inside the installed VS Code extension."""
    roots = [
        os.path.expanduser("~/.vscode/extensions"),
        os.path.expanduser("~/.vscode-server/extensions"),
        os.path.expanduser("~/.vscode-insiders/extensions"),
    ]
    for root in roots:
        for ext in sorted(glob.glob(os.path.join(root, "hediet.vscode-drawio-*")), reverse=True):
            app = os.path.join(ext, "drawio", "src", "main", "webapp")
            if os.path.isfile(os.path.join(app, "js", "viewer-static.min.js")):
                return app
    raise SystemExit(
        "cannot find the drawio webapp — install the VS Code extension hediet.vscode-drawio")


def find_chrome():
    for c in CHROME_CANDIDATES:
        if os.path.isfile(c):
            return c
    raise SystemExit("cannot find Chrome or Edge to render with")


def build_mxfile(model_xml, name, page_id):
    """Wrap the model the way drawio stores it: base64 + raw deflate + URL-encoded."""
    quoted = urllib.parse.quote(model_xml, safe="~()*!.'")
    co = zlib.compressobj(9, zlib.DEFLATED, -15)
    body = base64.b64encode(co.compress(quoted.encode("utf-8")) + co.flush()).decode("ascii")
    return '<mxfile><diagram name="%s" id="%s">%s</diagram></mxfile>' % (name, page_id, body)


def render(model_xml, mxfile_xml):
    webapp = find_webapp()
    page = PAGE.replace("__PAYLOAD__", json.dumps({"model": model_xml, "mxfile": mxfile_xml}))
    # The page must sit inside the webapp so its relative script paths resolve.
    fd, run = tempfile.mkstemp(prefix="drawio-render-", suffix=".html", dir=webapp)
    os.close(fd)
    try:
        with open(run, "w", encoding="utf-8") as f:
            f.write(page)
        proc = subprocess.run(
            [find_chrome(), "--headless=new", "--disable-gpu", "--no-sandbox",
             "--allow-file-access-from-files", "--virtual-time-budget=15000",
             "--user-data-dir=" + tempfile.mkdtemp(prefix="drawio-render-profile-"),
             "--dump-dom", "file:///" + run.replace("\\", "/")],
            capture_output=True, text=True, encoding="utf-8", timeout=300)
    finally:
        os.remove(run)

    dom = proc.stdout or ""
    m = re.search(r"&lt;&lt;&lt;BEGIN&gt;&gt;&gt;(.*?)&lt;&lt;&lt;END&gt;&gt;&gt;", dom, re.S)
    result = html.unescape(m.group(1)) if m else None
    if result is None:
        m = re.search(r"<<<BEGIN>>>(.*?)<<<END>>>", dom, re.S)
        result = m.group(1) if m else None
    if result is None:
        sys.stderr.write((proc.stderr or "")[-4000:])
        raise SystemExit("the renderer produced no output")
    if result.startswith("ERROR:"):
        raise SystemExit(result)
    return result


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("model", help="a bare <mxGraphModel> XML file")
    ap.add_argument("out", help="the .drawio.svg to write")
    ap.add_argument("--name", default="JBA2B", help="diagram page name (default JBA2B)")
    ap.add_argument("--id", dest="page_id", default="page1", help="diagram page id (default page1)")
    args = ap.parse_args()

    model = open(args.model, encoding="utf-8").read()
    svg = render(model, build_mxfile(model, args.name, args.page_id))
    if svg.startswith("<svg "):
        svg = '<svg host="%s" ' % HOST + svg[len("<svg "):]
    with open(args.out, "w", encoding="utf-8", newline="") as f:
        f.write("\ufeff" + svg)
    print("wrote %s (%d bytes)" % (args.out, len(svg)))


if __name__ == "__main__":
    main()
