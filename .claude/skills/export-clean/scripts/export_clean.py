#!/usr/bin/env python3
"""Export Pproni's committed source, comments stripped, into a clone of the shared repo.

Reads HEAD of the private repo (never the working tree), copies only the files the
config allowlists, strips comments, checks the result still formats, compiles and passes
tests, then stages it in the clean clone. It never commits or pushes: that is the
skill's job, after the user has looked.

Exit codes: 0 staged, 1 a check failed, 2 config needs a value from the user,
3 the shared repo has commits this clone does not.

    python export_clean.py                 # real export into config's clean_repo
    python export_clean.py --out DIR       # dry run into DIR, no git steps
    python export_clean.py --no-verify     # skip cargo fmt/test (debugging only)
"""
import argparse
import ast
import io
import json
import re
import shutil
import subprocess
import sys
import tarfile
import tokenize
from pathlib import Path, PurePosixPath

SKILL_DIR = Path(__file__).resolve().parent.parent
CONFIG_PATH = SKILL_DIR / "config.json"
EXIT_FAIL, EXIT_CONFIG, EXIT_UPSTREAM = 1, 2, 3
DRYRUN_MARKER = ".export-clean-dryrun"

# Placed wherever a comment was cut, so tidy() knows which lines lost something.
MARK = "\ue000"


class ExportError(Exception):
    def __init__(self, msg, code=EXIT_FAIL):
        super().__init__(msg)
        self.code = code


def git(args, cwd, check=True, binary=False):
    r = subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=not binary)
    if check and r.returncode != 0:
        err = r.stderr if not binary else r.stderr.decode(errors="replace")
        raise ExportError(f"git {' '.join(args)} failed in {cwd}:\n{err}")
    return r


# ---------------------------------------------------------------- strippers

def tidy(text):
    """Drop lines a cut left empty, and the blank-line runs those drops create."""
    out, dropped = [], False
    for line in text.split("\n"):
        if MARK in line:
            line = re.sub(r"[ \t]*" + MARK + r"[ \t]*$", "", line).replace(MARK, "")
            if not line.strip():
                dropped = True
                continue
        if not line.strip():
            if dropped and (not out or not out[-1].strip()):
                continue
        else:
            dropped = False
        out.append(line)
    while out and not out[-1].strip():
        out.pop()
    return "\n".join(out) + "\n" if out else ""


def scan_escaped(src, j, close):
    """Index just past `close`, starting at j, honouring backslash escapes."""
    n = len(src)
    while j < n:
        if src[j] == "\\":
            j += 2
        elif src.startswith(close, j):
            return j + len(close)
        else:
            j += 1
    raise ExportError("unterminated string literal")


IDENT_CHAR = re.compile(r"[A-Za-z0-9_]")
RUST_RAW = re.compile(r'[bc]?r(#*)"')
RUST_PREFIXED = re.compile(r'[bc]"')


def strip_rust(src):
    out, i, n = [], 0, len(src)
    while i < n:
        c, nxt = src[i], src[i + 1] if i + 1 < n else ""
        after_ident = i > 0 and IDENT_CHAR.match(src[i - 1])
        if c == "/" and nxt == "/":
            j = src.find("\n", i)
            i = n if j == -1 else j
            out.append(MARK)
        elif c == "/" and nxt == "*":
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif src.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            if depth:
                raise ExportError("unterminated block comment")
            out.append(" " + MARK)
            i = j
        elif c == '"':
            j = scan_escaped(src, i + 1, '"')
            out.append(src[i:j])
            i = j
        elif c in "brc" and not after_ident and (m := RUST_RAW.match(src, i)):
            close = '"' + m.group(1)
            j = src.find(close, m.end())
            if j == -1:
                raise ExportError("unterminated raw string")
            out.append(src[i:j + len(close)])
            i = j + len(close)
        elif c in "bc" and not after_ident and (m := RUST_PREFIXED.match(src, i)):
            j = scan_escaped(src, m.end(), '"')
            out.append(src[i:j])
            i = j
        elif c == "'":
            if nxt == "\\":
                j = src.find("'", i + 3) + 1
            elif i + 2 < n and src[i + 2] == "'":
                j = i + 3
            else:
                j = i + 1  # lifetime or label
            out.append(src[i:j])
            i = j
        else:
            out.append(c)
            i += 1
    return tidy("".join(out))


def strip_toml(src):
    out, i, n = [], 0, len(src)
    while i < n:
        c = src[i]
        if src.startswith('"""', i):
            j = scan_escaped(src, i + 3, '"""')
        elif src.startswith("'''", i):
            j = src.find("'''", i + 3) + 3
        elif c == '"':
            j = scan_escaped(src, i + 1, '"')
        elif c == "'":
            j = src.find("'", i + 1) + 1
        elif c == "#":
            j = src.find("\n", i)
            i = n if j == -1 else j
            out.append(MARK)
            continue
        else:
            j = i + 1
        if j <= i:
            raise ExportError("unterminated TOML string")
        out.append(src[i:j])
        i = j
    return tidy("".join(out))


def strip_hash_lines(src):
    """.gitignore / .gitattributes: only a line starting with # is a comment."""
    return tidy("\n".join(MARK if l.startswith("#") else l for l in src.split("\n")))


def strip_python(src):
    tree = ast.parse(src)
    lines = src.splitlines(keepends=True)
    starts = [0]
    for l in lines:
        starts.append(starts[-1] + len(l))

    def offset(row, col):
        return starts[row - 1] + col

    def char_col(row, byte_col):
        return len(lines[row - 1].encode()[:byte_col].decode())

    edits = []
    for node in ast.walk(tree):
        if not isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        first = node.body[0] if node.body else None
        if isinstance(first, ast.Expr) and isinstance(first.value, ast.Constant) \
                and isinstance(first.value.value, str):
            a = offset(first.lineno, char_col(first.lineno, first.col_offset))
            b = offset(first.end_lineno, char_col(first.end_lineno, first.end_col_offset))
            only_stmt = len(node.body) == 1 and not isinstance(node, ast.Module)
            edits.append((a, b, MARK + ("pass" if only_stmt else "")))
    for tok in tokenize.generate_tokens(io.StringIO(src).readline):
        if tok.type == tokenize.COMMENT:
            edits.append((offset(*tok.start), offset(*tok.end), MARK))
    for a, b, rep in sorted(edits, reverse=True):
        src = src[:a] + rep + src[b:]
    out = tidy(src)
    ast.parse(out)
    return out


def stripper_for(path):
    p = PurePosixPath(path)
    if p.suffix == ".rs":
        return strip_rust
    if p.suffix == ".py":
        return strip_python
    if p.suffix == ".toml":
        return strip_toml
    if p.name in (".gitignore", ".gitattributes"):
        return strip_hash_lines
    return None


# ---------------------------------------------------------------- export

def load_config(path):
    cfg = json.loads(path.read_text(encoding="utf-8"))
    missing = [k for k in ("clean_repo", "include_python_tools") if cfg.get(k) is None]
    if missing:
        raise ExportError(f"config.json needs a value for: {', '.join(missing)}", EXIT_CONFIG)
    return cfg


def matches(path, patterns):
    p = PurePosixPath(path)
    return any(p.full_match(pat) for pat in patterns)


def read_head(private):
    """{path: bytes} for every file committed at HEAD."""
    tar_bytes = git(["archive", "--format=tar", "HEAD"], private, binary=True).stdout
    files = {}
    with tarfile.open(fileobj=io.BytesIO(tar_bytes)) as tar:
        for m in tar.getmembers():
            f = tar.extractfile(m) if m.isfile() else None
            if f is not None:
                files[m.name] = f.read()
    return files


def check_clean_clone(clean):
    if not (clean / ".git").exists():
        raise ExportError(f"{clean} is not a git clone of the shared repo")
    if git(["status", "--porcelain"], clean).stdout.strip():
        raise ExportError(f"{clean} has uncommitted changes; commit, stash or discard them first")
    if git(["remote"], clean).stdout.strip():
        git(["fetch", "--quiet"], clean)
    upstream = git(["rev-parse", "--abbrev-ref", "@{u}"], clean, check=False)
    if upstream.returncode != 0:
        return
    has_head = git(["rev-parse", "--verify", "-q", "HEAD"], clean, check=False).returncode == 0
    rng = "HEAD..@{u}" if has_head else "@{u}"
    ahead = git(["log", "--oneline", rng], clean).stdout.strip()
    if ahead:
        raise ExportError(
            f"the shared repo has commits this clone does not ({upstream.stdout.strip()}):\n{ahead}",
            EXIT_UPSTREAM)


def wipe(clean, dry_run):
    if dry_run:
        keep = {".git", "target", "models", "onnxruntime-directml.dll", DRYRUN_MARKER}
        for child in clean.iterdir():
            if child.name not in keep:
                shutil.rmtree(child) if child.is_dir() else child.unlink()
        return
    for rel in git(["ls-files", "-z"], clean).stdout.split("\0"):
        if rel:
            (clean / rel).unlink(missing_ok=True)
    for d in sorted((p for p in clean.rglob("*") if p.is_dir()), key=lambda p: -len(p.parts)):
        if ".git" not in d.relative_to(clean).parts and not any(d.iterdir()):
            d.rmdir()


def copy_build_inputs(private, clean, cfg, dry_run):
    for pattern in cfg["build_inputs"]:
        for src in private.glob(pattern):
            rel = src.relative_to(private).as_posix()
            if not dry_run and git(["check-ignore", "-q", rel], clean, check=False).returncode != 0:
                raise ExportError(f"build input {rel} is not ignored in the clean clone; refusing to copy")
            dst = clean / rel
            if not dst.exists() or dst.stat().st_size != src.stat().st_size:
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src, dst)


def run_verify(clean, cfg):
    for cmd in cfg["verify"]:
        print(f"verify: {' '.join(cmd)}", flush=True)
        r = subprocess.run(cmd, cwd=clean, capture_output=True, text=True)
        if r.returncode != 0:
            tail = "\n".join((r.stdout + r.stderr).splitlines()[-60:])
            raise ExportError(f"`{' '.join(cmd)}` failed:\n{tail}")


def export(args):
    cfg = load_config(args.config)
    private = Path(git(["rev-parse", "--show-toplevel"], SKILL_DIR).stdout.strip())
    dry_run = args.out is not None
    clean = Path(args.out if dry_run else cfg["clean_repo"])
    if not clean.is_absolute():
        clean = (private / clean).resolve()
    if clean == private or private in clean.parents:
        raise ExportError(f"clean repo {clean} must live outside the private repo")

    if dry_run:
        clean.mkdir(parents=True, exist_ok=True)
        if any(clean.iterdir()) and not (clean / DRYRUN_MARKER).exists():
            raise ExportError(f"{clean} is not empty and was not made by a dry run")
        (clean / DRYRUN_MARKER).touch()
    else:
        check_clean_clone(clean)

    head = git(["rev-parse", "--short", "HEAD"], private).stdout.strip()
    dirty = git(["status", "--porcelain"], private).stdout.strip()
    if dirty:
        print(f"note: private repo has uncommitted changes; exporting HEAD {head} only")

    include = list(cfg["include"]) + (cfg["python_tools"] if cfg["include_python_tools"] else [])
    files = {p: b for p, b in read_head(private).items()
             if matches(p, include) and not matches(p, cfg["exclude"])}
    if not files:
        raise ExportError("allowlist matched no files")

    wipe(clean, dry_run)
    forbidden = [t.lower() for t in cfg["forbidden_terms"]]
    stats = {"stripped": 0, "verbatim": 0}
    for rel, data in sorted(files.items()):
        dst = clean / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        strip = None if matches(rel, cfg["verbatim"]) else stripper_for(rel)
        if strip is None and not matches(rel, cfg["verbatim"]):
            raise ExportError(f"{rel}: no comment stripper for this file type; add it to "
                              "`verbatim` in config.json or drop it from `include`")
        if strip is None:
            dst.write_bytes(data)
            stats["verbatim"] += 1
            continue
        src = data.decode("utf-8").replace("\r\n", "\n")
        if MARK in src:
            raise ExportError(f"{rel}: contains the private-use marker character")
        try:
            out = strip(src)
        except (ExportError, SyntaxError) as e:
            raise ExportError(f"{rel}: {e}")
        if strip(out) != out:
            raise ExportError(f"{rel}: stripping is not stable; stripper bug")
        hits = [t for t in forbidden if t in out.lower()]
        if hits:
            raise ExportError(f"{rel}: contains forbidden term(s) {hits}")
        dst.write_text(out, encoding="utf-8", newline="\n")
        stats["stripped"] += 1

    if not args.no_verify:
        copy_build_inputs(private, clean, cfg, dry_run)
        run_verify(clean, cfg)

    print(f"exported HEAD {head}: {stats['stripped']} files stripped, "
          f"{stats['verbatim']} copied verbatim -> {clean}")
    if not dry_run:
        git(["add", "-A"], clean)
        print(git(["status", "--short"], clean).stdout or "no changes since the last export")
        print(git(["diff", "--cached", "--shortstat"], clean).stdout.strip())


def main():
    ap = argparse.ArgumentParser(description="Export Pproni source, comments stripped.")
    ap.add_argument("--out", help="dry run into this directory instead of the clean clone")
    ap.add_argument("--config", default=CONFIG_PATH, type=Path, help="config file to use")
    ap.add_argument("--no-verify", action="store_true", help="skip the verify commands")
    try:
        export(ap.parse_args())
    except ExportError as e:
        print(f"export-clean: {e}", file=sys.stderr)
        sys.exit(e.code)


if __name__ == "__main__":
    main()
