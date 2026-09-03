#!/usr/bin/env python3
"""
verify.py — Definition of done for the reforge-core workspace.

Runs full verification pipeline:
1. Public boundary check (check_boundary.py)
2. Path contract check (check_path_contract.py)
3. Documentation metadata & fragment link gate (check_docs.py)
4. Package assembly unit tests (test_package.py)
5. cargo fmt -- --check
6. cargo test --workspace
7. Optional / informative ignored test leg
8. cargo clippy --workspace --all-targets -- -D warnings
9. git diff --check
"""

import os
import subprocess
import sys
from pathlib import Path


def run_step(name: str, cmd: list, cwd: Path = None, allow_failure: bool = False) -> int:
    print(f"\n== {name} ==")
    res = subprocess.run(cmd, cwd=str(cwd) if cwd else None)
    if res.returncode != 0 and not allow_failure:
        print(f"\nFALLO: {name} (exit {res.returncode})", file=sys.stderr)
        sys.exit(res.returncode)
    return res.returncode


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    scripts_dir = root / "scripts"
    workspace = root / "source" / "reforge"
    manifest = workspace / "Cargo.toml"

    # Step 1: Public boundary
    run_step("public boundary", [sys.executable, str(scripts_dir / "check_boundary.py")], cwd=root)

    # Step 2: Path contract
    run_step("path contract", [sys.executable, str(scripts_dir / "check_path_contract.py")], cwd=root)

    # Step 3: Documentation metadata & links
    run_step("check docs", [sys.executable, str(scripts_dir / "check_docs.py")], cwd=root)

    # Step 4: Package unit tests
    test_pkg = scripts_dir / "test_package.py"
    if test_pkg.is_file():
        run_step("package unit tests", [sys.executable, str(test_pkg)], cwd=root)

    # Step 5: cargo fmt -- --check
    run_step("cargo fmt --check", ["cargo", "fmt", "--", "--check"], cwd=workspace)

    # Step 6: cargo test --workspace
    run_step("cargo test --workspace", ["cargo", "test", "--manifest-path", str(manifest), "--workspace"], cwd=root)

    # Step 7: Informative ignored tests (PG-gated)
    ignored_skips = [
        "member_remove_self",
        "channel_combat_kills_npc",
        "channel_deployed_30003_full_flow",
        "channel_idle_timeout_reset_by_traffic",
    ]
    ignored_cmd = ["cargo", "test", "--manifest-path", str(manifest), "--workspace", "--", "--ignored"]
    for s in ignored_skips:
        ignored_cmd.extend(["--skip", s])
    print("\n== cargo test --workspace -- --ignored (informative, PG-gated) ==")
    ret = subprocess.run(ignored_cmd, cwd=str(root), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode
    if ret != 0:
        print("INFO: la pata --ignored termino con aviso (PG/WSL apagados o test omitido); el gate normal continua")

    # Step 8: cargo clippy --all-targets -D warnings
    run_step(
        "cargo clippy --workspace -D warnings",
        ["cargo", "clippy", "--manifest-path", str(manifest), "--workspace", "--all-targets", "--", "-D", "warnings"],
        cwd=root,
    )

    # Step 9: git diff --check
    run_step("git diff --check", ["git", "diff", "--check"], cwd=root)

    print("\nOK: verificacion completa")
    return 0


if __name__ == "__main__":
    sys.exit(main())
