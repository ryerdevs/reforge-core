#!/usr/bin/env python3
"""
package.py — Whitelisted package assembly and manifest verification (A2.4).

Pure Python 3 standard library (no external dependencies).
Works identically on Windows and Linux.

Ensures that distributed deployment packages contain only explicitly allowlisted
configuration, schema, scripts, and optional release binaries, strictly excluding
runtime logs, private database dumps, dirty artifacts, and non-distributable files.

Commands:
  build    Assemble a clean package to an output directory and optional archive
  verify   Validate an assembled directory or zip archive against its manifest
  manifest Generate and display a manifest without packaging
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import sys
import zipfile
from pathlib import Path

# Explicit relative files allowlist
ALLOWLIST_EXACT_FILES = {
    "README.md",
    "config/auth.toml",
    "config/channel.toml",
    "config/examples/auth.example.toml",
    "config/examples/channel.example.toml",
    "schema/schema.sql",
    "schema/seed.sql",
    "scripts/manage.py",
    "scripts/bootstrap_db.py",
    "scripts/start.py",
    "scripts/stop.py",
    "scripts/status.py",
    "scripts/backup.py",
}

# Optional binaries allowlist (included if present and requested)
ALLOWLIST_BINARIES = {
    "server_realms",
    "server_realms.exe",
    "admin_tui",
    "admin_tui.exe",
}

# Prohibited patterns - strictly rejected if encountered
PROHIBITED_EXTENSIONS = {
    ".dump",
    ".bak",
    ".log",
    ".tmp",
    ".pdb",
    ".ilk",
    ".obj",
    ".o",
    ".a",
    ".lib",
    ".epk",
    ".eix",
    ".dds",
    ".gr2",
}

PROHIBITED_DIR_PATTERNS = [
    re.compile(r"(?:^|[/\\])logs(?:[/\\]|$)", re.IGNORECASE),
    re.compile(r"(?:^|[/\\])backups(?:[/\\]|$)", re.IGNORECASE),
    re.compile(r"(?:^|[/\\])target(?:[/\\]|$)", re.IGNORECASE),
    re.compile(r"(?:^|[/\\])\.git(?:[/\\]|$)", re.IGNORECASE),
]


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(65536):
            h.update(chunk)
    return h.hexdigest()


def normalize_rel_path(p: Path) -> str:
    return str(p).replace("\\", "/")


def is_prohibited(rel_path: str) -> tuple[bool, str]:
    lower = rel_path.lower()
    for ext in PROHIBITED_EXTENSIONS:
        if lower.endswith(ext):
            return True, f"prohibited extension '{ext}'"
    for pat in PROHIBITED_DIR_PATTERNS:
        if pat.search(rel_path):
            return True, f"prohibited directory pattern in '{rel_path}'"
    return False, ""


def get_repo_root() -> Path:
    if os.environ.get("REFORGE_ROOT"):
        return Path(os.environ["REFORGE_ROOT"]).resolve()
    return Path(__file__).resolve().parent.parent


def get_deploy_source_dir(repo_root: Path) -> Path:
    if os.environ.get("REFORGE_DEPLOY_WIN"):
        return Path(os.environ["REFORGE_DEPLOY_WIN"]).resolve()
    return repo_root / "source" / "deploy" / "win"


def collect_allowed_files(source_dir: Path, include_binaries: bool = True) -> list[tuple[str, Path]]:
    candidates = []
    # 1. Exact allowlist files
    for rel in sorted(ALLOWLIST_EXACT_FILES):
        p = source_dir / Path(rel)
        if p.is_file():
            candidates.append((rel, p))

    # 2. Binaries if requested
    if include_binaries:
        for bin_name in sorted(ALLOWLIST_BINARIES):
            p = source_dir / bin_name
            if p.is_file():
                candidates.append((bin_name, p))
            else:
                # Fallback: check target/release in repo root
                repo_root = get_repo_root()
                tgt = repo_root / "source" / "reforge" / "target" / "release" / bin_name
                if tgt.is_file():
                    candidates.append((bin_name, tgt))

    return candidates


def generate_manifest(files: list[tuple[str, Path]], target: str = "agnostic", version: str = "0.1.0") -> dict:
    file_entries = []
    total_bytes = 0

    for rel_path, abs_path in sorted(files, key=lambda x: x[0]):
        prohibited, reason = is_prohibited(rel_path)
        if prohibited:
            raise ValueError(f"Cannot add {rel_path} to manifest: {reason}")

        size = abs_path.stat().st_size
        checksum = sha256_file(abs_path)
        total_bytes += size
        file_entries.append({
            "path": rel_path,
            "size": size,
            "sha256": checksum,
        })

    return {
        "package": "reforge-runtime",
        "version": version,
        "target": target,
        "file_count": len(file_entries),
        "total_bytes": total_bytes,
        "manifest_version": "1.0",
        "files": file_entries,
    }


def assemble_package(
    source_dir: Path,
    out_dir: Path,
    include_binaries: bool = True,
    create_archive: bool = True,
    target: str = "auto",
) -> tuple[Path, Path | None, dict]:
    if target == "auto":
        target = "windows-x86_64" if sys.platform == "win32" else "linux-x86_64"

    files = collect_allowed_files(source_dir, include_binaries=include_binaries)
    if not files:
        raise RuntimeError(f"No allowlisted files found in {source_dir}")

    manifest = generate_manifest(files, target=target)

    # Clean destination directory
    if out_dir.exists():
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    # Copy files
    for rel_path, abs_path in files:
        dest = out_dir / Path(rel_path)
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(abs_path, dest)

    # Write manifest.json
    manifest_path = out_dir / "manifest.json"
    with open(manifest_path, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
        f.write("\n")

    archive_path = None
    if create_archive:
        archive_name = f"{manifest['package']}-{manifest['version']}-{target}"
        archive_file = out_dir.parent / f"{archive_name}.zip"
        with zipfile.ZipFile(archive_file, "w", zipfile.ZIP_DEFLATED) as zf:
            for root, _, dirfiles in os.walk(out_dir):
                for df in dirfiles:
                    full_p = Path(root) / df
                    rel_in_zip = full_p.relative_to(out_dir)
                    zf.write(full_p, arcname=str(rel_in_zip))
        archive_path = archive_file

    return out_dir, archive_path, manifest


def verify_package(package_target: Path) -> tuple[bool, list[str]]:
    errors = []

    if package_target.is_file() and package_target.suffix.lower() == ".zip":
        # Verify Zip Archive
        with zipfile.ZipFile(package_target, "r") as zf:
            names = set(zf.namelist())
            if "manifest.json" not in names:
                return False, ["Archive missing manifest.json"]

            manifest_data = json.loads(zf.read("manifest.json").decode("utf-8"))
            expected_files = {e["path"]: e for e in manifest_data.get("files", [])}

            # Check that every file in manifest exists with correct checksum
            for rel, meta in expected_files.items():
                if rel not in names:
                    errors.append(f"Missing file declared in manifest: {rel}")
                    continue
                content = zf.read(rel)
                if len(content) != meta["size"]:
                    errors.append(f"Size mismatch for {rel}: expected {meta['msize']}, got {len(content)}")
                actual_sha = hashlib.sha256(content).hexdigest()
                if actual_sha != meta["sha256"]:
                    errors.append(f"Checksum mismatch for {rel}")

            # Check for unlisted or prohibited files in archive
            for name in names:
                if name == "manifest.json":
                    continue
                prohibited, reason = is_prohibited(name)
                if prohibited:
                    errors.append(f"Prohibited file in archive: {name} ({reason})")
                if name not in expected_files:
                    errors.append(f"Unlisted file in archive: {name}")
    elif package_target.is_dir():
        # Verify Directory
        manifest_file = package_target / "manifest.json"
        if not manifest_file.is_file():
            return False, ["Directory missing manifest.json"]

        with open(manifest_file, "r", encoding="utf-8") as f:
            manifest_data = json.load(f)

        expected_files = {e["path"]: e for e in manifest_data.get("files", [])}

        for rel, meta in expected_files.items():
            abs_p = package_target / Path(rel)
            if not abs_p.is_file():
                errors.append(f"Missing file declared in manifest: {rel}")
                continue
            if abs_p.stat().st_size != meta["size"]:
                errors.append(f"Size mismatch for {rel}")
            if sha256_file(abs_p) != meta["sha256"]:
                errors.append(f"Checksum mismatch for {rel}")

        for root, _, dirfiles in os.walk(package_target):
            for df in dirfiles:
                full_p = Path(root) / df
                rel = normalize_rel_path(full_p.relative_to(package_target))
                if rel == "manifest.json":
                    continue
                prohibited, reason = is_prohibited(rel)
                if prohibited:
                    errors.append(f"Prohibited file in directory: {rel} ({reason})")
                if rel not in expected_files:
                    errors.append(f"Unlisted file in directory: {rel}")
    else:
        return False, [f"Invalid package target (must be dir or .zip): {package_target}"]


    return len(errors) == 0, errors


def cmd_build(args: argparse.Namespace) -> int:
    repo_root = get_repo_root()
    source_dir = Path(args.source_dir).resolve() if args.source_dir else get_deploy_source_dir(repo_root)
    out_dir = Path(args.out_dir).resolve() if args.out_dir else repo_root / "dist" / "reforge-runtime"

    print(f"[*] Assembling whitelisted package from: {source_dir}")
    print(f"[*] Output target directory: {out_dir}")

    try:
        pkg_dir, archive, manifest = assemble_package(
            source_dir=source_dir,
            out_dir=out_dir,
            include_binaries=not args.no_binaries,
            create_archive=not args.no_archive,
            target=args.target,
        )
        print(f"[+] Package successfully assembled: {manifest['file_count']} files ({manifest['total_bytes'] / 1024:.1f} KB)")
        print(f"[+] Manifest: {pkg_dir / 'manifest.json'}")
        if archive:
            print(f"[+] Archive : {archive} ({archive.stat().st_size / 1024:.1f} KB)")
        return 0
    except Exception as e:
        print(f"[-] Assembly failed: {e}", file=sys.stderr)
        return 1


def cmd_verify(args: argparse.Namespace) -> int:
    target = Path(args.target).resolve()
    print(f"[*] Verifying package against manifest: {target}")

    ok, errors = verify_package(target)
    if ok:
        print("[+] Package VERIFIED: all allowlisted files match manifest hashes, zero prohibited artifacts.")
        return 0
    else:
        print(f"[-] Package VERIFICATION FAILED with {len(errors)} error(s):", file=sys.stderr)
        for err in errors:
            print(f"    - {err}", file=sys.stderr)
        return 1


def cmd_manifest(args: argparse.Namespace) -> int:
    repo_root = get_repo_root()
    source_dir = Path(args.source_dir).resolve() if args.source_dir else get_deploy_source_dir(repo_root)
    files = collect_allowed_files(source_dir, include_binaries=not args.no_binaries)
    manifest = generate_manifest(files, target=args.target)
    print(json.dumps(manifest, indent=2))
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Whitelisted package assembly and manifest verification (A2.4).")
    subparsers = parser.add_subparsers(dest="command", required=True)

    build_p = subparsers.add_parser("build", help="Assemble clean package and optional archive")
    build_p.add_argument("--source-dir", help="Path to source deploy directory")
    build_p.add_argument("--out-dir", help="Path to output assembled package directory")
    build_p.add_argument("--no-binaries", action="store_true", help="Exclude compiled executables")
    build_p.add_argument("--no-archive", action="store_true", help="Do not create a zip archive")
    build_p.add_argument("--target", default="auto", help="Target platform identifier")

    verify_p = subparsers.add_parser("verify", help="Verify package directory or archive against manifest")
    verify_p.add_argument("target", help="Path to package directory or .zip archive")

    man_p = subparsers.add_parser("manifest", help="Preview manifest JSON without assembling")
    man_p.add_argument("--source-dir", help="Path to source deploy directory")
    man_p.add_argument("--no-binaries", action="store_true", help="Exclude compiled executables")
    man_p.add_argument("--target", default="auto", help="Target platform identifier")

    args = parser.parse_args(argv)
    handlers = {
        "build": cmd_build,
        "verify": cmd_verify,
        "manifest": cmd_manifest,
    }
    return handlers[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
