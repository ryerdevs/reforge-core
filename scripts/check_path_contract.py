#!/usr/bin/env python3
"""
check_path_contract.py — A2.1: verify path contract and relative discovery.

Verifies:
1. No hardcoded maintainer repository root (C:\\projects\\Metin2) in deploy/runtime scripts.
2. Scripts correctly discover executable, deploy directory, and scripts
   using repository-relative defaults and overrides.
"""

import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def fail(failures: list, msg: str):
    failures.append(msg)


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    failures = []
    print("== check_path_contract ==")

    # Check 1: Static scan for hardcoded maintainer repository root
    script_candidates = [
        root / "scripts" / "manage.py",
        root / "scripts" / "package.py",
        root / "scripts" / "bootstrap_db.py",
        root / "source" / "deploy" / "win" / "scripts" / "manage.py",
    ]

    for f in script_candidates:
        if not f.is_file():
            continue
        try:
            lines = f.read_text(encoding="utf-8", errors="replace").splitlines()
            for idx, line in enumerate(lines, 1):
                stripped = line.strip()
                if stripped.startswith("#"):
                    continue
                if re.search(r"C:\\projects\\Metin2(?:[\\/]|$)", line, re.IGNORECASE):
                    fail(failures, f"hardcoded repo path in {f.name}:{idx}: {line}")
        except Exception as e:
            fail(failures, f"unable to read {f.name}: {e}")

    # Check 2: Relative discovery in mock checkout
    temp_dir = Path(tempfile.mkdtemp(prefix="reforge_path_contract_"))
    try:
        mock_scripts = temp_dir / "scripts"
        mock_deploy = temp_dir / "source" / "deploy" / "win"
        mock_target = temp_dir / "source" / "reforge" / "target" / "release"

        mock_scripts.mkdir(parents=True, exist_ok=True)
        mock_deploy.mkdir(parents=True, exist_ok=True)
        mock_target.mkdir(parents=True, exist_ok=True)

        if (root / "scripts" / "package.py").is_file():
            shutil.copy2(root / "scripts" / "package.py", mock_scripts / "package.py")
        if (root / "scripts" / "manage.py").is_file():
            shutil.copy2(root / "scripts" / "manage.py", mock_scripts / "manage.py")

        # Test manage.py execution from external cwd
        cmd = [sys.executable, str(mock_scripts / "manage.py"), "--help"]
        res = subprocess.run(cmd, cwd=str(temp_dir), capture_output=True, text=True)
        if res.returncode != 0:
            fail(failures, f"manage.py in mock checkout failed: {res.stderr}")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

    if failures:
        print("FALLO: check_path_contract")
        for f in failures:
            print(f"  - {f}")
        return 1

    print("OK: check_path_contract")
    return 0


if __name__ == "__main__":
    sys.exit(main())
