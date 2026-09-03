#!/usr/bin/env python3
"""
check_boundary.py — A1: enforce the versioned Rust-server public boundary.

The tracked index is the public checkout. The status scan catches newly
added forbidden paths without traversing ignored operator-only trees.
"""

import json
import os
import re
import subprocess
import sys
from pathlib import Path

ALLOWED_BINARY_FIXTURES = {
    "source/reforge/protocol/tests/golden/auth_login3_40999.bin"
}

BINARY_LOOKING_EXT = re.compile(
    r"\.(?:exe|bin|pack|dll|so|pdb|dylib|lib|a|o|obj|ilk|pyc|epk|eix|pak|gr2|dds|dump|bak|pcap|pcapng|rar|zip|7z|tar|gz|bz2|xz|zst)$",
    re.IGNORECASE,
)

DECOMPILED_PATH = re.compile(
    r"(?:^|/)(?:decompiled?|disassembly|reverse-engineered|reverse_engineering|ida|idapro|ghidra|x64dbg|dnspy|ilspy)(?:[._/-]|$)",
    re.IGNORECASE,
)
DECOMPILED_EXT = re.compile(r"\.(?:i64|idb|gdt|dmp|ghidra)$", re.IGNORECASE)

FORBIDDEN_PATH_RULES = [
    (re.compile(r"^source/server(?:/|$)", re.IGNORECASE), "frozen C++ oracle"),
    (re.compile(r"^source/deploy/(?!win(?:/|$))[^/]+(?:/|$)", re.IGNORECASE), "only source/deploy/win is public"),
    (re.compile(r"^source/deploy/[^/]+/share(?:/|$)", re.IGNORECASE), "pack/config data"),
    (re.compile(r"^source/deploy/[^/]+/(?:logs|backups)(?:/|$)", re.IGNORECASE), "runtime logs or backups"),
    (re.compile(r"^source/(?:client|client_rust|tools/pack)(?:/|$)", re.IGNORECASE), "client or pack source"),
    (re.compile(r"^(?:client|client-om2)(?:/|$)", re.IGNORECASE), "external client material"),
    (re.compile(r"^source/tools/proto/(?:_out(?:/|$)|(?:[^/]+/)*(?:item_proto|mob_proto)$)", re.IGNORECASE), "generated client proto output"),
    (re.compile(r"(?:^|/)\.env(?:\.(?!example$)[^/]+)?$", re.IGNORECASE), "environment credentials"),
    (re.compile(r"(?:^|/)target(?:/|$)", re.IGNORECASE), "generated build output"),
]

TOML_PATTERN = re.compile(r"(?i)(?<![A-Za-z0-9_])(?P<key>password|token|secret|api_key)\s*=\s*(?P<value>[^#\r\n]*)")
MYSQL_CONFIG_PATTERN = re.compile(r"^\s*\S+\s+\S+\s+(?P<value>\S+)\s+\S+\s*$")
SCRIPT_PATTERN = re.compile(r"(?i)^\s*(?:[$](?:env:)?|export\s+)?(?P<key>PGPASSWORD|MYSQL_PASSWORD|password|token|secret|api_key)\s*=\s*(?P<value>[^#\r\n]*)")
WINDOWS_SCRIPT_PATTERN = re.compile(r"""(?i)^\s*set\s+"?(?P<key>PGPASSWORD|MYSQL_PASSWORD|password|token|secret|api_key)\s*=\s*(?P<value>[^"\r\n]*?)"?\s*$""")


def git_lines(root: Path, args: list) -> list:
    res = subprocess.run(["git", "-C", str(root)] + args, capture_output=True, text=True, check=True)
    return [line.strip() for line in res.stdout.splitlines() if line.strip()]


def to_repo_path(path: str) -> str:
    val = path.strip()
    if val.startswith('"') and val.endswith('"'):
        val = val[1:-1]
    while val.startswith("./"):
        val = val[2:]
    return val.replace("\\", "/")


def test_extensionless_binary_content(root: Path, path: str) -> bool:
    if path in ALLOWED_BINARY_FIXTURES or Path(path).suffix != "":
        return False

    full_path = root / path.replace("/", os.sep)
    if not full_path.is_file():
        return False

    try:
        data = full_path.read_bytes()
    except Exception:
        return False

    if not data:
        return False

    prefix = data[:8].hex().upper()
    for magic in ("4D5A", "7F454C46", "504B0304", "4D495058", "4D434F5A", "89504E47", "FFD8FF"):
        if prefix.startswith(magic):
            return True

    # Try decode UTF-8 or UTF-16
    encoding = "utf-8"
    offset = 0
    if len(data) >= 2 and data[:2] in (b"\xff\xfe", b"\xfe\xff"):
        encoding = "utf-16"
        offset = 2
    elif len(data) >= 3 and data[:3] == b"\xef\xbb\xbf":
        offset = 3

    try:
        text = data[offset:].decode(encoding)
    except Exception:
        return True

    for ch in text:
        if ord(ch) < 32 and ch not in "\t\n\r":
            return True
    return False


def test_secret_scan_path(path: str) -> bool:
    p = path.replace("\\", "/")
    if re.search(r"^source/deploy/win/.+\.toml$", p, re.IGNORECASE):
        return True
    if re.search(r"(?:^|/)\.env(?:\.example)?$", p, re.IGNORECASE):
        return True
    if re.search(r"^source/deploy/win/scripts/.+\.(?:ps1|sh|bash|cmd|bat)$", p, re.IGNORECASE):
        return True
    if re.search(r"^scripts/.+\.(?:ps1|sh|bash|cmd|bat)$", p, re.IGNORECASE):
        return True
    if re.search(r"^source/reforge/(?:Cargo\.toml|.+/Cargo\.toml)$", p, re.IGNORECASE):
        return True
    if re.search(r"\.json(?:\.sample)?$", p, re.IGNORECASE):
        return True
    if re.search(r"(?:^|/)mysql\.conf$", p, re.IGNORECASE):
        return True
    return False


def get_secret_value(raw: str) -> str:
    v = raw.strip()
    if v.startswith('"'):
        end = v.find('"', 1)
        if end > 0:
            return v[1:end]
    if v.startswith("'"):
        end = v.find("'", 1)
        if end > 0:
            return v[1:end]
    return v.split()[0] if v.split() else ""


def is_allowed_secret(val: str, line: str, key: str) -> bool:
    c = val.strip()
    if not c:
        return True
    if c.lower() == "mt2" and (key.upper() in ("PGPASSWORD", "MYSQL_PASSWORD") or re.search(r"^\s*pg_conn\s*=", line, re.IGNORECASE)):
        return True
    return bool(re.match(r"^(?:<[^>\r\n]+>|CHANGE_ME|\$\{[^}\r\n]+\}|YOUR_[A-Za-z0-9_]*)$", c, re.IGNORECASE))


def check_json_secrets(node, path: str, failures: list):
    if node is None or isinstance(node, (str, int, float, bool)):
        return
    if isinstance(node, list):
        for item in node:
            check_json_secrets(item, path, failures)
        return
    if isinstance(node, dict):
        for k, v in node.items():
            if re.match(r"^(?:password|token|secret|api_key)$", k, re.IGNORECASE):
                val_str = str(v) if v is not None else ""
                if not is_allowed_secret(val_str, "", k):
                    failures.append(f"{path} secret-like assignment ({k})")
            check_json_secrets(v, path, failures)


def check_secrets(root: Path, path: str, failures: list):
    full_path = root / path.replace("/", os.sep)
    if not full_path.is_file():
        return

    ext = Path(path).suffix.lower()
    is_toml = ext == ".toml"
    is_json = bool(re.search(r"\.json(?:\.sample)?$", path, re.IGNORECASE))
    is_mysql_config = bool(re.search(r"(?:^|/)mysql\.conf$", path, re.IGNORECASE))
    is_windows_script = ext in (".cmd", ".bat")

    if is_json:
        try:
            doc = json.loads(full_path.read_text(encoding="utf-8", errors="replace"))
            check_json_secrets(doc, path, failures)
        except Exception:
            failures.append(f"unable to parse secret-scan JSON: {path}")
        return

    try:
        lines = full_path.read_text(encoding="utf-8", errors="replace").splitlines()
    except Exception:
        failures.append(f"unable to read tracked secret-scan file: {path}")
        return

    for line_idx, line in enumerate(lines, 1):
        stripped = line.lstrip()
        if stripped.startswith("#") or stripped.startswith("//") or stripped.startswith(";"):
            continue

        matches = []
        if is_toml:
            matches = list(TOML_PATTERN.finditer(line))
        elif is_mysql_config:
            m = MYSQL_CONFIG_PATTERN.match(line)
            if m:
                matches = [m]
        elif is_windows_script:
            m1 = WINDOWS_SCRIPT_PATTERN.match(line)
            m2 = SCRIPT_PATTERN.match(line)
            matches = [m for m in (m1, m2) if m]
        else:
            m = SCRIPT_PATTERN.match(line)
            if m:
                matches = [m]

        for m in matches:
            val = get_secret_value(m.group("value"))
            key = "MYSQL_PASSWORD" if is_mysql_config else m.group("key")
            if not is_allowed_secret(val, line, key):
                failures.append(f"{path} secret-like assignment at line {line_idx} ({key})")


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    failures = []

    try:
        tracked_paths = [to_repo_path(p) for p in git_lines(root, ["ls-files", "--full-name"])]
    except Exception as e:
        print(f"ERROR reading git tracked files: {e}")
        return 1

    try:
        status_lines = git_lines(root, ["status", "--porcelain=v1", "--untracked-files=all"])
        status_paths = []
        for line in status_lines:
            if len(line) < 4:
                continue
            code = line[:2]
            p = line[3:]
            if " -> " in p:
                status_paths.append(to_repo_path(p.split(" -> ", 1)[1]))
            elif "D" not in code:
                status_paths.append(to_repo_path(p))
    except Exception as e:
        print(f"ERROR reading git status: {e}")
        return 1

    # Check tracked paths
    for p in tracked_paths:
        for pat, reason in FORBIDDEN_PATH_RULES:
            if pat.search(p):
                failures.append(f"tracked forbidden path: {p} ({reason})")

        if DECOMPILED_PATH.search(p) or DECOMPILED_EXT.search(p):
            failures.append(f"tracked decompiled artifact path: {p}")

        if BINARY_LOOKING_EXT.search(p) and p not in ALLOWED_BINARY_FIXTURES:
            failures.append(f"tracked binary-looking public path: {p}")

        if test_extensionless_binary_content(root, p):
            failures.append(f"tracked extensionless binary content: {p}")

        if test_secret_scan_path(p):
            check_secrets(root, p, failures)

    # Check status paths
    for p in status_paths:
        for pat, reason in FORBIDDEN_PATH_RULES:
            if pat.search(p):
                failures.append(f"untracked forbidden path: {p} ({reason})")

        if DECOMPILED_PATH.search(p) or DECOMPILED_EXT.search(p):
            failures.append(f"untracked decompiled artifact path: {p}")

        if BINARY_LOOKING_EXT.search(p) and p not in ALLOWED_BINARY_FIXTURES:
            failures.append(f"untracked binary-looking public path: {p}")

        if test_extensionless_binary_content(root, p):
            failures.append(f"untracked extensionless binary content: {p}")

        if test_secret_scan_path(p):
            check_secrets(root, p, failures)

    if failures:
        print("FALLO: check_boundary")
        for f in failures:
            print(f"  - {f}")
        return 1

    print(f"OK: check_boundary (tracked paths: {len(tracked_paths)}; status paths checked: {len(status_paths)})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
