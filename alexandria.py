#!/usr/bin/env python3
"""Alexandria graphify bootstrap — installs `graphify` if missing, then kicks off
a structural extraction (alexandria/graphify-out/graph.json) in the background.
If the graph already exists, keeps it fresh instead of leaving it stale: any
git tree drift (new commit, uncommitted changes) since the last run re-runs
the same extract in the background — graphify's own extraction is already
incremental (it caches per-file AST hashes), so a rerun on an unchanged repo
does effectively no work.

`graphify update <path>` was tried here first and rejected: it treats <path>
as both the scan root and the graph location, so it can't express Alexandria's
"scan the repo root, write to alexandria/graphify-out" split the way `extract
--out` can — verified it either writes a second, wrong graphify-out at the
repo root or silently rescans an empty alexandria/ directory instead.

Self-service path for a repo that has no Alexandria graph yet — or one that
does but an operator isn't around to refresh — and no operator around to run
alexandria.sh's own Graphify stage. Non-blocking by design: extraction can
take minutes, so this starts it detached and returns immediately.
"""
from __future__ import annotations
import argparse, hashlib, os, shutil, subprocess, sys
from pathlib import Path


def which(name: str) -> str | None:
    home = Path.home()
    path = f"{home / '.local/bin'}:{os.environ.get('PATH', '')}"
    return shutil.which(name, path=path)


def ensure_uv() -> str:
    exe = which("uv")
    if exe:
        return exe
    print("[INFO] uv not found — installing.")
    subprocess.run(["sh", "-c", "curl -LsSf https://astral.sh/uv/install.sh | sh"], check=True)
    exe = which("uv")
    if not exe:
        raise SystemExit("[ERROR] uv installation finished but executable was not found")
    return exe


def ensure_graphify() -> str:
    exe = which("graphify")
    if exe:
        return exe

    print("[INFO] graphify not found — installing via uv.")
    uv = ensure_uv()
    subprocess.run([uv, "tool", "install", "graphifyy"], check=True)

    exe = which("graphify")
    if exe:
        return exe

    cp = subprocess.run([uv, "tool", "dir", "--bin"], text=True, capture_output=True, check=True)
    candidate = Path(cp.stdout.strip()) / "graphify"
    if candidate.exists():
        return str(candidate)

    raise SystemExit("[ERROR] graphifyy installed but the graphify CLI was not found on PATH")


def git_signature(repo: Path) -> str | None:
    """HEAD commit + a hash of the working-tree diff status. alexandria/ and graphify-out/ are
    excluded from the status check — otherwise the tool's own output would itself look like
    drift on the very next run, in a repo where it isn't gitignored yet: graph.json/logs/this
    state file under alexandria/graphify-out/, plus a graphify-out/cache/stat-index.json that
    `graphify extract` writes at the repo root regardless of --out (verified: no CLI flag
    redirects it). None if repo isn't a git repo (or git isn't available) — caller treats that
    as "can't tell, leave it alone"."""
    try:
        head = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"],
                                        text=True, stderr=subprocess.DEVNULL).strip()
        status = subprocess.check_output(
            ["git", "-C", str(repo), "status", "--porcelain", "--",
             ".", ":!alexandria", ":!graphify-out"],
            text=True, stderr=subprocess.DEVNULL)
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None
    return f"{head}:{hashlib.sha256(status.encode()).hexdigest()}"


def run_in_background(cmd, repo: Path, log_file: Path, signature: str | None, state_file: Path):
    print(f"[INFO] Starting in the background: {' '.join(cmd)}")
    state_file.parent.mkdir(parents=True, exist_ok=True)
    with open(log_file, "ab") as fh:
        proc = subprocess.Popen(cmd, cwd=repo, stdout=fh, stderr=subprocess.STDOUT,
                                 start_new_session=True)

    # Written at kickoff, not completion — a rare edit landing mid-build won't
    # be picked up until the *next* invocation, same tradeoff as any
    # fire-and-forget background job.
    if signature is not None:
        state_file.write_text(signature, encoding="utf-8")

    print(f"[OK] Started (pid {proc.pid}). Log: {log_file}")
    print("     This does not block — check back later, or `tail -f` the log.")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=".", help="repository root (default: cwd)")
    ap.add_argument("--force", action="store_true", help="rebuild even if graph.json already exists")
    args = ap.parse_args()

    repo = Path(args.repo).resolve()
    if not repo.is_dir():
        raise SystemExit(f"[ERROR] Repository not found: {repo}")

    alexandria_dir = repo / "alexandria"
    graphify_out = alexandria_dir / "graphify-out"
    graph_file = graphify_out / "graph.json"
    state_file = graphify_out / ".git-sync-state"
    log_file = alexandria_dir / "graphify-build.log"

    signature = git_signature(repo)

    if graph_file.exists() and not args.force:
        stored = state_file.read_text(encoding="utf-8").strip() if state_file.exists() else None

        if signature is None:
            print(f"[OK] Graph already present — nothing to do (not a git repo, can't check for drift): {graph_file}")
            return

        if signature == stored:
            print(f"[OK] Graph already up to date with the working tree: {graph_file}")
            return

        print("[INFO] Git tree has moved since the last build — refreshing (incrementally, graphify caches unchanged files).")

    graphify = ensure_graphify()
    alexandria_dir.mkdir(parents=True, exist_ok=True)

    cmd = [
        graphify, "extract", ".",
        "--code-only",
        "--out", str(alexandria_dir),
        "--max-workers", str(os.cpu_count() or 4),
        "--timing",
    ]
    run_in_background(cmd, repo, log_file, signature, state_file)
    print(f"     Graph will land at: {graph_file}")


if __name__ == "__main__":
    main()
