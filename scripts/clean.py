#!/usr/bin/env python3
"""
clean.py — Preview and safely clean regenerable local artifacts (target, logs, temp).
"""

import argparse
import os
import shutil
import sys
from pathlib import Path


def get_dir_size(path: Path) -> int:
    total = 0
    try:
        for entry in path.rglob("*"):
            if entry.is_file():
                try:
                    total += entry.stat().st_size
                except OSError:
                    pass
    except OSError:
        pass
    return total


def format_bytes(size: int) -> str:
    for unit in ["B", "KB", "MB", "GB", "TB"]:
        if size < 1024.0:
            return f"{size:3.1f} {unit}"
        size /= 1024.0
    return f"{size:.1f} PB"


def main() -> int:
    parser = argparse.ArgumentParser(description="Clean regenerable local artifacts")
    parser.add_argument("--all", action="store_true", help="Clean all regenerable artifacts")
    parser.add_argument("--target", action="store_true", help="Clean source/reforge/target")
    parser.add_argument("--logs", action="store_true", help="Clean runtime logs")
    parser.add_argument("--yes", "-y", action="store_true", help="Confirm deletion without prompting")
    parser.add_argument("--what-if", action="store_true", help="Preview only, do not delete")
    args = parser.parse_args()

    root = Path(__file__).resolve().parent.parent
    clean_all = args.all or (not args.target and not args.logs and not args.what_if)

    targets_to_clean = []
    if clean_all or args.target:
        t = root / "source" / "reforge" / "target"
        if t.is_dir():
            targets_to_clean.append(t)

    if clean_all or args.logs:
        logs_dir = root / "source" / "deploy" / "win" / "logs"
        if logs_dir.is_dir():
            targets_to_clean.append(logs_dir)

    if not targets_to_clean:
        print("No cleanable artifacts found.")
        return 0

    print("Deletion plan:")
    total_bytes = 0
    for p in targets_to_clean:
        sz = get_dir_size(p)
        total_bytes += sz
        print(f"  - {p.relative_to(root)} ({format_bytes(sz)})")
    print(f"Total space to reclaim: {format_bytes(total_bytes)}")

    if args.what_if:
        print("What-if preview complete. Nothing was deleted.")
        return 0

    if not args.yes:
        resp = input("Type 'YES' to proceed with deletion: ").strip()
        if resp != "YES":
            print("Aborted by user.")
            return 0

    for p in targets_to_clean:
        print(f"Removing {p}...")
        try:
            shutil.rmtree(p, ignore_errors=True)
        except Exception as e:
            print(f"Error removing {p}: {e}", file=sys.stderr)

    print("Clean complete.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
