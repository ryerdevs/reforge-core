#!/usr/bin/env python3
"""
build_admin_tui.py — Build admin_tui and copy binary to source/deploy/win/.
"""

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description="Build admin_tui and copy to deploy/win")
    parser.add_argument("--debug", action="store_true", help="Build debug instead of release")
    parser.add_argument("--no-copy", action="store_true", help="Do not copy binary to deploy/win")
    args = parser.parse_args()

    root = Path(__file__).resolve().parent.parent
    workspace = root / "source" / "reforge"
    deploy_win = root / "source" / "deploy" / "win"

    profile = "debug" if args.debug else "release"
    cargo_args = ["cargo", "build", "-p", "admin_tui"]
    if not args.debug:
        cargo_args.append("--release")

    print(f"==> {' '.join(cargo_args)} (in {workspace})")
    res = subprocess.run(cargo_args, cwd=str(workspace))
    if res.returncode != 0:
        print(f"ERROR: cargo build failed with exit {res.returncode}", file=sys.stderr)
        return res.returncode

    if not args.no_copy:
        bin_name = "admin_tui.exe" if os.name == "nt" else "admin_tui"
        src_bin = workspace / "target" / profile / bin_name
        dst_bin = deploy_win / bin_name

        deploy_win.mkdir(parents=True, exist_ok=True)
        if src_bin.is_file():
            shutil.copy2(src_bin, dst_bin)
            size = dst_bin.stat().st_size
            print(f"==> Copied {src_bin.name} to {dst_bin} ({size:,} bytes)")
        else:
            print(f"ERROR: built binary not found at {src_bin}", file=sys.stderr)
            return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
