"""Check comment edits in Rust files against docs/agent-knowledge/code-comments.md.

usage: python tools/check_comments.py --base <git-ref> [file.rs ...]
       (no files: every .rs file that differs from <git-ref>)

1. code unchanged   FAIL  the file with comments stripped must equal <ref>'s, token for token
2. pointers resolve FAIL  every docs/....md path in a comment must exist
3. kept information LIST  words of removed comments found neither in the file's comments nor in docs/
4. size             LIST  comment blocks over the size guide

Exit 1 when check 1 or 2 fails. Run from the repo root.
"""
import argparse, re, subprocess, sys
from collections import Counter
from pathlib import Path

LIMITS = {"//!": 12, "///": 8, "//": 4}
POINTER = re.compile(r"docs/[\w./\-]+\.md")
# words worth tracking when a comment is removed: `code`, dates, numbers with a unit or a dot,
# CONSTANTS, paths::like::this and snake_case_names
KEY = re.compile(r"`[^`]+`|\b\d{4}-\d{2}-\d{2}\b|\b\d+(?:\.\d+)?\s?(?:%|px|ms|s|MB|KB)\b|\b\d+\.\d+\b"
                 r"|\b[A-Z][A-Z0-9_]{3,}\b|\b\w+(?:::\w+)+\b|\b[a-z]+_[a-z0-9_]+\b")


def lex(src):
    """Split Rust source into (code tokens, comments). Strings stay whole tokens; a comment acts
    as whitespace. Comments come back as (marker, line number, text)."""
    toks, comments, buf = [], [], []
    i, n, line = 0, len(src), 1

    def flush():
        if buf:
            toks.append("".join(buf))
            buf.clear()

    while i < n:
        c = src[i]
        if src.startswith("//", i):
            flush()
            j = src.find("\n", i)
            j = n if j < 0 else j
            text = src[i:j]
            marker = "//!" if text.startswith("//!") else "///" if text.startswith("///") and not text.startswith("////") else "//"
            comments.append((marker, line, text[len(marker):].strip()))
            i = j
            continue
        if src.startswith("/*", i):
            flush()
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif src.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            text = src[i:j]
            comments.append(("/*", line, text))
            line += text.count("\n")
            i = j
            continue
        m = re.match(r'(b?r)(#*)"', src[i:i + 300]) if c in "br" and (not buf or not re.match(r"\w", buf[-1])) else None
        if m:
            flush()
            end = '"' + m.group(2)
            j = src.find(end, i + m.end())
            j = n if j < 0 else j + len(end)
            toks.append(src[i:j]); line += src[i:j].count("\n"); i = j
            continue
        if c == '"' or (c == "b" and src.startswith('b"', i) and (not buf or not re.match(r"\w", buf[-1]))):
            flush()
            j = i + (2 if c == "b" else 1)
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            j += 1
            toks.append(src[i:j]); line += src[i:j].count("\n"); i = j
            continue
        if c == "'":
            # char literal ('a', '\n', '\u{1F600}') or a lifetime ('a)
            m = re.match(r"'(\\u\{[0-9a-fA-F]+\}|\\x[0-9a-fA-F]{2}|\\.|[^\\'])'", src[i:i + 12])
            if m:
                flush()
                toks.append(m.group(0)); i += m.end()
                continue
        if c.isspace():
            flush()
            line += c == "\n"
            i += 1
            continue
        buf.append(c)
        i += 1
    flush()
    return toks, comments


def blocks(comments):
    """Consecutive comment lines with the same marker -> (marker, first line, line count)."""
    out = []
    for marker, ln, _ in comments:
        if marker == "/*":
            continue
        if out and out[-1][0] == marker and out[-1][1] + out[-1][2] == ln:
            out[-1][2] += 1
        else:
            out.append([marker, ln, 1])
    return out


def git_show(ref, path):
    r = subprocess.run(["git", "show", f"{ref}:{path}"], capture_output=True, text=True, encoding="utf-8")
    return r.stdout if r.returncode == 0 else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True)
    ap.add_argument("files", nargs="*")
    a = ap.parse_args()
    files = a.files or subprocess.run(["git", "diff", "--name-only", a.base, "--", "*.rs"],
                                      capture_output=True, text=True, check=True).stdout.split()
    docs_text = "\n".join(p.read_text(encoding="utf-8") for p in Path("docs").rglob("*.md"))
    failed = False
    for f in files:
        f = f.replace("\\", "/")
        new = Path(f).read_text(encoding="utf-8").replace("\r\n", "\n")
        old = git_show(a.base, f)
        new_toks, new_com = lex(new)
        print(f"== {f}")
        if old is not None:
            old_toks, old_com = lex(old.replace("\r\n", "\n"))
            if old_toks != new_toks:
                failed = True
                k = next((k for k, (x, y) in enumerate(zip(old_toks, new_toks)) if x != y), min(len(old_toks), len(new_toks)))
                print(f"  FAIL code changed near token {k}: {old_toks[k-3:k+3]} -> {new_toks[k-3:k+3]}")
            else:
                print(f"  ok   code unchanged ({len(new_toks)} tokens); comment lines {len(old_com)} -> {len(new_com)}")
            kept = "\n".join(t for _, _, t in new_com)
            removed = Counter(t for _, _, t in old_com) - Counter(t for _, _, t in new_com)
            lost = sorted({k.strip("`") for t in removed for k in KEY.findall(t)
                           if k.strip("`") not in kept and k.strip("`") not in docs_text and not POINTER.fullmatch(k.strip("`"))})
            if lost:
                print(f"  list words from removed comments found nowhere else ({len(lost)}): " + ", ".join(lost))
        for _, ln, t in new_com:
            for p in POINTER.findall(t):
                if not Path(p).exists():
                    failed = True
                    print(f"  FAIL line {ln}: pointer {p} does not exist")
        for marker, ln, cnt in blocks(new_com):
            if cnt > LIMITS[marker]:
                print(f"  list line {ln}: {marker} block of {cnt} lines (guide {LIMITS[marker]})")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
