"""Find every reference to a file in the repo (and extra folders) and check it resolves.

Kinds of reference:
  mdlink   [text](target) or ![alt](target) — resolved from the file's folder; #anchor checked
  path     a repo-relative path written out (docs/..., src/..., .claude/...) — resolved from the repo root
  home     a ~/.claude/... path
  wiki     [[name]] — an Obsidian note (docs/) or a memory slug
  bare     a known doc file name written with no folder in front — must be unique in the repo
  url      http(s) link — listed; checked only with --urls

Usage (from the repo root): python tools/check_docs.py [--repo .] [--extra <dir> ...] [--urls] [--out refs.json]
--extra adds folders outside the repo (e.g. the Claude memory folder). Exit code 1 when any
reference is broken and not on the allow-list (--allow, default tools/check_docs_allow.tsv:
one 'kind<TAB>target<TAB>why' per line).
"""
import argparse, json, re, subprocess, sys, urllib.parse, urllib.request
from pathlib import Path

TEXT_EXT = {".md", ".rs", ".toml", ".sh", ".py", ".json", ".ps1", ".txt"}
SKIP_DIRS = {"target", ".git", "graphify-out", ".obsidian", "node_modules", ".serena", "MD", ".vscode"}
MDLINK = re.compile(r"!?\[(?:[^\]\[]|\[[^\]]*\])*\]\((<[^>]+>|[^)\s]+)(?:\s+\"[^\"]*\")?\)")
WIKI = re.compile(r"!?\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\\?\|[^\]]*)?\]\]")
PATH = re.compile(r"(?<![\w/.\-~])((?:docs|src|examples|tools|models|tests|\.claude)/[\w./\-*]*[\w*/])")
HOME = re.compile(r"(~/\.claude/[\w./\-*]*\w)")
BARE = re.compile(r"(?<![\w/.\-])((?:ARCHITECTURE|WORK_STATE|pproni-spec|README|TOC|toc|SKILL)\.md)\b")
URL = re.compile(r"https?://[^\s)>\]`'\"*]+")
TICK = re.compile(r"`((?:docs|src|examples|tools|models|tests|\.claude)/[^`]*)`")


def files_to_scan(repo: Path, extras):
    out = []
    ls = subprocess.run(["git", "ls-files", "-co", "--exclude-standard"], cwd=repo,
                        capture_output=True, text=True, check=True).stdout.splitlines()
    for rel in ls + [".gitignore"]:
        p = repo / rel
        if any(part in SKIP_DIRS for part in Path(rel).parts):
            continue
        if p.is_file() and (p.suffix in TEXT_EXT or p.name == ".gitignore"):
            out.append(p)
    for d in extras:
        for p in Path(d).rglob("*"):
            if p.is_file() and p.suffix in TEXT_EXT:
                out.append(p)
    return sorted(set(out))


def slug_github(h):
    h = h.strip().lower()
    h = re.sub(r"[^\w\- ]", "", h)
    return h.replace(" ", "-")


def headings(md: Path):
    hs = set()
    try:
        for line in md.read_text(encoding="utf-8").splitlines():
            m = re.match(r"#{1,6}\s+(.*)", line)
            if m:
                t = m.group(1).strip()
                hs.add(slug_github(t))
                hs.add(t.lower())
    except OSError:
        pass
    return hs


def resolve_glob(p: Path):
    if "*" in str(p):
        return any(p.parent.glob(p.name)) if "*" not in str(p.parent) else True
    return p.exists()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=".")
    ap.add_argument("--extra", action="append", default=[])
    ap.add_argument("--urls", action="store_true")
    ap.add_argument("--out")
    ap.add_argument("--allow", default="tools/check_docs_allow.tsv")
    a = ap.parse_args()
    repo = Path(a.repo).resolve()
    home = Path.home()
    allow = set()
    if a.allow and Path(a.allow).exists():
        for line in Path(a.allow).read_text(encoding="utf-8").splitlines():
            if line.strip() and not line.startswith("#"):
                allow.add(tuple(line.split("\t")[:2]))

    files = files_to_scan(repo, a.extra)
    by_name = {}
    for p in repo.rglob("*.md"):
        if not any(part in SKIP_DIRS for part in p.relative_to(repo).parts):
            by_name.setdefault(p.name.lower(), []).append(p)
    wiki_names = {p.stem.lower() for p in (repo / "docs").rglob("*")} if (repo / "docs").exists() else set()
    for d in a.extra:
        for p in Path(d).glob("*.md"):
            wiki_names.add(p.stem.lower())
            m = re.search(r"^name:\s*(\S+)", p.read_text(encoding="utf-8"), re.M)
            if m:
                wiki_names.add(m.group(1).lower())

    refs = []
    for f in files:
        try:
            text = f.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        shown = str(f.relative_to(repo)) if f.is_relative_to(repo) else str(f)
        shown = shown.replace("\\", "/")
        fenced = False
        for n, line in enumerate(text.splitlines(), 1):
            spans = []
            if f.suffix == ".md" and line.lstrip().startswith("```"):
                fenced = not fenced
                continue
            if f.suffix == ".md" and not fenced:
                # links inside `code` are examples, not links
                code_free = re.sub(r"`[^`]*`", lambda c: " " * len(c.group(0)), line)
                for m in MDLINK.finditer(code_free):
                    spans.append(m.span())
                    t = m.group(1).strip("<>")
                    if re.match(r"[a-z]+:", t):
                        if t.startswith("http"):
                            refs.append(dict(file=shown, line=n, kind="url", target=t, ok=None))
                        continue
                    path, _, anchor = t.partition("#")
                    path = urllib.parse.unquote(path)
                    tgt = (f.parent / path).resolve() if path else f
                    ok = tgt.exists()
                    note = ""
                    if ok and anchor and tgt.suffix == ".md":
                        anc = urllib.parse.unquote(anchor)
                        if slug_github(anc) not in headings(tgt) and anc.lower() not in headings(tgt):
                            ok, note = False, "anchor not found"
                    refs.append(dict(file=shown, line=n, kind="mdlink", target=t,
                                     resolved=str(tgt), ok=ok, note=note))
                for m in WIKI.finditer(code_free):
                    spans.append(m.span())
                    name = m.group(1).strip()
                    stem = Path(name).stem.lower() if "." in Path(name).name else Path(name).name.lower()
                    ok = stem in wiki_names or (repo / "docs" / name).exists()
                    refs.append(dict(file=shown, line=n, kind="wiki", target=name, ok=ok))
            for m in TICK.finditer(line):
                # a whole path in backticks may hold spaces ("tools/2. export_onnx.py")
                t = m.group(1).split(" §")[0].rstrip(".,")
                if " " in t and (repo / t).exists():
                    spans.append(m.span())
                    refs.append(dict(file=shown, line=n, kind="path", target=t, ok=True))
            for m in PATH.finditer(line):
                if any(s <= m.start() < e for s, e in spans):
                    continue
                t = m.group(1).rstrip(".")
                ok = resolve_glob(repo / t)
                if not ok:
                    # names with spaces ("tools/2. export_onnx.py"): try the longest prefix of the
                    # rest of the line that exists
                    rest = re.split(r"[`\"'()<>,;]", line[m.start():])[0].rstrip(". ")
                    words = rest.split(" ")
                    for k in range(len(words), 1, -1):
                        cand = " ".join(words[:k]).rstrip(".:")
                        if (repo / cand).exists():
                            t, ok = cand, True
                            break
                refs.append(dict(file=shown, line=n, kind="path", target=t, ok=ok))
            for m in HOME.finditer(line):
                t = m.group(1)
                ok = resolve_glob(home / t[2:])
                refs.append(dict(file=shown, line=n, kind="home", target=t, ok=ok))
            for m in BARE.finditer(line):
                if any(s <= m.start() < e for s, e in spans):
                    continue
                # part of a longer path already counted?
                if line[max(0, m.start() - 1)] == "/":
                    continue
                cands = by_name.get(m.group(1).lower(), [])
                local = (f.parent / m.group(1)).exists()
                ok = local or len(cands) == 1
                note = "" if ok else f"{len(cands)} candidates"
                refs.append(dict(file=shown, line=n, kind="bare", target=m.group(1), ok=ok, note=note))
            for m in URL.finditer(line):
                if any(s <= m.start() < e for s, e in spans):
                    continue
                if True:
                    refs.append(dict(file=shown, line=n, kind="url", target=m.group(0).rstrip(".,;"), ok=None))

    if a.urls:
        seen = {}
        for r in refs:
            if r["kind"] != "url":
                continue
            u = r["target"]
            if u not in seen:
                try:
                    req = urllib.request.Request(u, method="GET", headers={"User-Agent": "Mozilla/5.0"})
                    with urllib.request.urlopen(req, timeout=15) as resp:
                        seen[u] = (resp.status < 400, str(resp.status))
                except Exception as e:  # noqa: BLE001 — report any failure as the status
                    seen[u] = (False, str(e)[:80])
            r["ok"], r["note"] = seen[u]

    for r in refs:
        r["allowed"] = (r["kind"], r["target"]) in allow
    bad = [r for r in refs if r["ok"] is False and not r["allowed"]]
    if a.out:
        Path(a.out).write_text(json.dumps(refs, indent=1), encoding="utf-8")
    kinds = {}
    for r in refs:
        kinds.setdefault(r["kind"], [0, 0])
        kinds[r["kind"]][0] += 1
        kinds[r["kind"]][1] += r["ok"] is False and not r["allowed"]
    print("refs by kind (total, broken):", kinds)
    for r in bad:
        print(f"BROKEN {r['kind']:6} {r['file']}:{r['line']}  {r['target']}  {r.get('note', '')}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
