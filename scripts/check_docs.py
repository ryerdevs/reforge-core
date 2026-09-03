#!/usr/bin/env python3
"""
check_docs.py — Documentation metadata and fragment link gate.

Validates that:
1. Live state files exist (documentation/progress.md, documentation/plans/gap-registry.md).
2. Every live document in documentation/ (excluding history/) carries the mandatory metadata
   header (Type, Status, Audience, Last verified).
3. documentation/roadmap.md is Type: Reference.
4. Internal markdown fragment links resolve to real headings in active documentation.
"""

import os
import re
import sys
import urllib.parse
from pathlib import Path

REQUIRED_METADATA = ["Type:", "Status:", "Audience:", "Last verified:"]
LIVE_STATE_FILES = [
    "documentation/progress.md",
    "documentation/plans/gap-registry.md",
]


def to_github_fragment(heading: str) -> str:
    val = heading.strip()
    val = re.sub(r"\s+#+\s*$", "", val)
    val = re.sub(r"<[^>]+>", "", val)
    val = re.sub(r"!\[([^\]]*)\]\([^)]*\)", r"\1", val)
    val = re.sub(r"\[([^\]]+)\]\([^)]*\)", r"\1", val)

    slug = []
    for ch in val.lower():
        if ch.isspace():
            slug.append("-")
        elif ch.isalnum() or ch in ("-", "_"):
            slug.append(ch)
    res = "".join(slug).strip("-")
    return res


def get_markdown_fragments(path: Path) -> set:
    fragments = set()
    counts = {}
    in_fence = False

    try:
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    except Exception:
        return fragments

    for line in lines:
        if re.match(r"^\s{0,3}(`{3,}|~{3,})", line):
            in_fence = not in_fence
            continue
        if in_fence:
            continue

        m = re.match(r"^\s{0,3}#{1,6}(?:\s+|$)(.*)$", line)
        if m:
            base = to_github_fragment(m.group(1))
            if not base:
                continue
            if base not in counts:
                counts[base] = 0
                fragments.add(base)
            else:
                counts[base] += 1
                fragments.add(f"{base}-{counts[base]}")
    return fragments


def remove_markdown_fences(text: str) -> str:
    lines = []
    in_fence = False
    for line in text.splitlines():
        if re.match(r"^\s{0,3}(`{3,}|~{3,})", line):
            in_fence = not in_fence
            lines.append("")
        elif in_fence:
            lines.append("")
        else:
            lines.append(line)
    return "\n".join(lines)


def is_external_link(target: str) -> bool:
    return bool(re.match(r"^(?:[a-zA-Z][a-zA-Z0-9+.-]*:|//)", target))


def is_generated_artifact(path: Path) -> bool:
    parts = [p.lower() for p in path.parts]
    return "target" in parts or "logs" in parts or "graphify-out" in parts


def test_markdown_fragments(source: Path, root: Path, heading_cache: dict) -> list:
    link_pattern = re.compile(r"(?<!\!)\[(?P<text>[^\]]+)\]\((?P<target>[^)\r\n]+)\)")
    try:
        content = source.read_text(encoding="utf-8", errors="replace")
    except Exception as e:
        return [f"UNREADABLE: {source.relative_to(root)} -> {e}"]

    doc = remove_markdown_fences(content)
    failures = []

    for match in link_pattern.finditer(doc):
        raw_target = match.group("target").strip()
        if raw_target.startswith("<") and ">" in raw_target:
            raw_target = raw_target[1 : raw_target.index(">")]
        else:
            raw_target = raw_target.split()[0]

        if is_external_link(raw_target):
            continue

        if "#" not in raw_target:
            continue

        rel_target, fragment = raw_target.split("#", 1)
        if not fragment:
            continue

        try:
            fragment = urllib.parse.unquote(fragment).lower()
        except Exception:
            continue

        # Resolve path
        if not rel_target:
            target_path = source
        else:
            try:
                decoded = urllib.parse.unquote(rel_target)
                target_path = (source.parent / decoded).resolve()
            except Exception:
                continue

        # Must be inside repo
        try:
            target_path.relative_to(root)
        except ValueError:
            continue

        if is_generated_artifact(target_path):
            continue
        if target_path.suffix.lower() != ".md":
            continue
        if "history" in [p.lower() for p in target_path.parts]:
            continue

        source_rel = source.relative_to(root).as_posix()
        link_text = re.sub(r"\s+", " ", match.group("text")).strip()

        if not target_path.is_file():
            failures.append(
                f"MISSING MARKDOWN TARGET FOR FRAGMENT '#{fragment}': {source_rel} [{link_text}] -> {raw_target}"
            )
            continue

        if target_path not in heading_cache:
            heading_cache[target_path] = get_markdown_fragments(target_path)

        if fragment not in heading_cache[target_path]:
            failures.append(
                f"MISSING MARKDOWN FRAGMENT '#{fragment}': {source_rel} [{link_text}] -> {raw_target}"
            )

    return failures


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    docs = root / "documentation"
    failures = []

    # 1. Live state files must exist
    for rel_f in LIVE_STATE_FILES:
        fpath = root / rel_f
        if not fpath.is_file():
            failures.append(f"MISSING live state file: {rel_f}")

    # 2. Metadata block on every live markdown document
    if docs.is_dir():
        for p in docs.rglob("*.md"):
            if "history" in [part.lower() for part in p.parts]:
                continue
            try:
                lines = p.read_text(encoding="utf-8", errors="replace").splitlines()[:15]
                head_text = "\n".join(lines)
                rel = p.relative_to(root).as_posix()
                for field in REQUIRED_METADATA:
                    if field not in head_text:
                        failures.append(f"NO METADATA '{field}': {rel}")
            except Exception as e:
                failures.append(f"ERROR reading {p}: {e}")

    # 3. Roadmap metadata check
    roadmap = docs / "roadmap.md"
    if roadmap.is_file():
        head = "\n".join(roadmap.read_text(encoding="utf-8", errors="replace").splitlines()[:10])
        if not re.search(r"^\s*Type:\s*Reference", head, re.MULTILINE):
            failures.append("documentation/roadmap.md must be Type: Reference (phase map), see documentation/README.md")

    # 4. Check active fragments
    active_sources = []
    for entry in ["README.md", "CHANGELOG.md", "AGENTS.md"]:
        ep = root / entry
        if ep.is_file():
            active_sources.append(ep)

    if docs.is_dir():
        for p in docs.rglob("*.md"):
            if "history" not in [part.lower() for part in p.parts]:
                active_sources.append(p)

    heading_cache = {}
    for source in active_sources:
        failures.extend(test_markdown_fragments(source, root, heading_cache))

    if failures:
        print("FALLO: check_docs")
        for f in failures:
            print(f"  - {f}")
        return 1

    print("OK: check_docs (metadata + live state files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
