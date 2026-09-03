#!/usr/bin/env python3
"""
bootstrap_db.py — Automated PostgreSQL schema bootstrap and synthetic seed loader.

Cross-platform (Windows / Linux), standard library only.
Works with local native PostgreSQL, Docker, or WSL instances.

Commands:
  init     Create database 'metin2' and apply versioned schema DDL
  seed     Load minimal lawful synthetic development seed records
  reset    Drop and recreate 'metin2' database from schema + seed
  check    Verify database connectivity, schemas, and table counts
  restore  Restore a custom database dump (.dump or .sql) into 'metin2'
"""

from __future__ import annotations

import argparse
import os
import shutil
import socket
import subprocess
import sys
import time
from pathlib import Path


def get_repo_root() -> Path:
    if os.environ.get("REFORGE_ROOT"):
        return Path(os.environ["REFORGE_ROOT"]).resolve()
    return Path(__file__).resolve().parent.parent


def get_schema_dir(repo_root: Path) -> Path:
    return repo_root / "source" / "deploy" / "win" / "schema"


def is_port_open(host: str = "127.0.0.1", port: int = 5432, timeout: float = 0.4) -> bool:
    try:
        with socket.create_connection((host, port), timeout=timeout):
            return True
    except (socket.timeout, ConnectionRefusedError, OSError):
        return False


def ensure_postgres(verbose: bool = True) -> bool:
    if is_port_open("127.0.0.1", 5432):
        return True

    if verbose:
        print("[*] PostgreSQL is stopped on 127.0.0.1:5432. Starting service...")

    if sys.platform == "win32":
        for svc in ["postgresql-metin2", "postgresql"]:
            try:
                subprocess.run(["net", "start", svc], capture_output=True, check=False)
            except OSError:
                pass
            if is_port_open("127.0.0.1", 5432):
                break
    else:
        for cmd in [["systemctl", "start", "postgresql"], ["service", "postgresql", "start"]]:
            try:
                subprocess.run(cmd, capture_output=True, check=False)
            except OSError:
                pass
            if is_port_open("127.0.0.1", 5432):
                break

    for _ in range(15):
        time.sleep(0.2)
        if is_port_open("127.0.0.1", 5432):
            if verbose:
                print("[+] PostgreSQL service started.")
            return True

    if verbose:
        print("[-] ERROR: PostgreSQL is not responding on 127.0.0.1:5432.", file=sys.stderr)
    return False


def find_pg_tool(name: str) -> Path | None:
    exe_name = f"{name}.exe" if sys.platform == "win32" else name

    # 1. Direct env vars
    env_var = f"{name.upper()}_PATH"
    if os.environ.get(env_var):
        cand = Path(os.environ[env_var])
        if cand.is_file():
            return cand
    if os.environ.get("PGBIN_PATH"):
        cand = Path(os.environ["PGBIN_PATH"]) / exe_name
        if cand.is_file():
            return cand

    # 2. PATH
    which = shutil.which(name)
    if which:
        return Path(which)

    # 3. Known Windows locations
    if sys.platform == "win32":
        known = [
            Path(r"C:\projects\metin2-extra\pg18\pgsql\bin") / exe_name,
            Path(r"C:\projects\metin2-extra\pg18\bin") / exe_name,
        ]
        for p in known:
            if p.is_file():
                return p

        for pf in [Path(r"C:\Program Files\PostgreSQL"), Path(r"C:\Program Files (x86)\PostgreSQL")]:
            if pf.is_dir():
                for sub in sorted(pf.iterdir(), reverse=True):
                    cand = sub / "bin" / exe_name
                    if cand.is_file():
                        return cand

    return None


def run_psql(
    sql_file: Path | None = None,
    sql_cmd: str | None = None,
    db: str = "metin2",
    host: str = "127.0.0.1",
    port: int = 5432,
    user: str = "mt2",
    password: str = "mt2",
) -> tuple[int, str, str]:
    psql = find_pg_tool("psql")
    if not psql:
        return 1, "", "psql executable not found in PATH or standard directories"

    cmd = [
        str(psql),
        "-h", host,
        "-p", str(port),
        "-U", user,
        "-d", db,
    ]
    if sql_file:
        cmd.extend(["-v", "ON_ERROR_STOP=1", "-f", str(sql_file)])
    elif sql_cmd:
        cmd.extend(["-tAc", sql_cmd])

    env = os.environ.copy()
    env["PGPASSWORD"] = password

    res = subprocess.run(cmd, env=env, capture_output=True, text=True, check=False)
    return res.returncode, res.stdout, res.stderr


def cmd_init(args: argparse.Namespace) -> int:
    if not ensure_postgres():
        return 1

    repo_root = get_repo_root()
    schema_dir = get_schema_dir(repo_root)
    schema_file = schema_dir / "schema.sql"

    if not schema_file.is_file():
        print(f"[-] Schema file not found: {schema_file}", file=sys.stderr)
        return 1

    print("[*] Checking database 'metin2' existence on 127.0.0.1:5432...")
    code, stdout, _ = run_psql(sql_cmd="SELECT 1 FROM pg_database WHERE datname = 'metin2';", db="postgres")
    if "1" not in stdout:
        print("[*] Database 'metin2' does not exist. Creating database...")
        code, _, stderr = run_psql(sql_cmd="CREATE DATABASE metin2 OWNER mt2 ENCODING 'UTF8';", db="postgres")
        if code != 0:
            print(f"[-] Failed to create database: {stderr.strip()}", file=sys.stderr)
            return 1
        print("[+] Database 'metin2' created successfully.")
    else:
        print("[+] Database 'metin2' exists.")

    print(f"[*] Applying schema DDL from {schema_file}...")
    code, stdout, stderr = run_psql(sql_file=schema_file, db="metin2")
    if code != 0:
        print(f"[-] Schema application failed: {stderr.strip()}", file=sys.stderr)
        return 1

    print("[+] Schema applied successfully: 5 domain schemas created (account, common, player, log, world).")
    return 0


def cmd_seed(args: argparse.Namespace) -> int:
    if not ensure_postgres():
        return 1

    repo_root = get_repo_root()
    schema_dir = get_schema_dir(repo_root)
    seed_file = schema_dir / "seed.sql"

    if not seed_file.is_file():
        print(f"[-] Seed file not found: {seed_file}", file=sys.stderr)
        return 1

    print(f"[*] Loading synthetic development seed from {seed_file}...")
    code, stdout, stderr = run_psql(sql_file=seed_file, db="metin2")
    if code != 0:
        print(f"[-] Seed application failed: {stderr.strip()}", file=sys.stderr)
        return 1

    print("[+] Synthetic seed loaded successfully:")
    print("    - Test Account : test / 1234 (Warrior Lv.1 'Ryer', Shinsoo)")
    print("    - Admin Account: admin / admin (Implementor GM Lv.75)")
    print("    - Basic Items  : Sword+0, Potions, Village 1 Map, EXP Table (1..120)")
    return 0


def cmd_reset(args: argparse.Namespace) -> int:
    if not ensure_postgres():
        return 1

    print("[!] WARNING: This will drop database 'metin2' and recreate it from scratch.")
    if not getattr(args, "force", False):
        confirm = input("Are you sure you want to proceed? [y/N]: ").strip().lower()
        if confirm not in ("y", "yes"):
            print("Aborted.")
            return 0

    print("[*] Terminating active connections to 'metin2'...")
    run_psql(
        sql_cmd="SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = 'metin2' AND pid <> pg_backend_pid();",
        db="postgres",
    )

    print("[*] Dropping database 'metin2'...")
    code, _, stderr = run_psql(sql_cmd="DROP DATABASE IF EXISTS metin2;", db="postgres")
    if code != 0:
        print(f"[-] Failed to drop database: {stderr.strip()}", file=sys.stderr)
        return 1

    print("[*] Creating fresh database 'metin2'...")
    code, _, stderr = run_psql(sql_cmd="CREATE DATABASE metin2 OWNER mt2 ENCODING 'UTF8';", db="postgres")
    if code != 0:
        print(f"[-] Failed to create database: {stderr.strip()}", file=sys.stderr)
        return 1

    ret = cmd_init(args)
    if ret != 0:
        return ret

    return cmd_seed(args)


def cmd_check(args: argparse.Namespace) -> int:
    if not ensure_postgres():
        return 1

    print("=== Reforge Database Check ===")
    psql = find_pg_tool("psql")
    print(f"psql Tool      : {psql if psql else '[WARN] Not found in PATH'}")

    # Check schemas
    code, stdout, _ = run_psql(
        sql_cmd="SELECT nspname FROM pg_namespace WHERE nspname IN ('account','common','player','log','world') ORDER BY 1;",
        db="metin2",
    )
    schemas = [s.strip() for s in stdout.splitlines() if s.strip()]
    expected_schemas = ["account", "common", "log", "player", "world"]
    all_schemas = all(s in schemas for s in expected_schemas)
    print(f"Schemas        : {'[OK] ' + ', '.join(schemas) if all_schemas else '[FAIL] Missing schemas: ' + str(schemas)}")

    # Check table count
    code, stdout, _ = run_psql(
        sql_cmd="SELECT count(*) FROM information_schema.tables WHERE table_schema IN ('account','common','player','log','world');",
        db="metin2",
    )
    tbl_count = stdout.strip()
    print(f"Tables Total   : {tbl_count} tables")

    # Check seed accounts
    code, stdout, _ = run_psql(
        sql_cmd="SELECT count(*) FROM account.account;",
        db="metin2",
    )
    acc_count = stdout.strip() if code == 0 else "0"
    print(f"Accounts       : {acc_count} records")

    # Check seed players
    code, stdout, _ = run_psql(
        sql_cmd="SELECT count(*) FROM player.player;",
        db="metin2",
    )
    ply_count = stdout.strip() if code == 0 else "0"
    print(f"Characters     : {ply_count} records")

    # Check exp table
    code, stdout, _ = run_psql(
        sql_cmd="SELECT count(*) FROM common.exp_table;",
        db="metin2",
    )
    exp_count = stdout.strip() if code == 0 else "0"
    print(f"EXP Table      : {exp_count} levels")

    verdict = all_schemas and int(tbl_count or 0) >= 30 and int(acc_count or 0) >= 1
    print("------------------------------")
    print(f"Verdict        : {'[OK] Database fully initialized and ready' if verdict else '[WARN] Incomplete database (run init/seed)'}")
    print("==============================")
    return 0 if verdict else 1


def cmd_restore(args: argparse.Namespace) -> int:
    if not ensure_postgres():
        return 1

    dump_path = Path(args.file).resolve()
    if not dump_path.is_file():
        print(f"[-] Dump file not found: {dump_path}", file=sys.stderr)
        return 1

    print(f"[*] Restoring from {dump_path} into 'metin2'...")
    if dump_path.suffix.lower() == ".dump":
        pg_restore = find_pg_tool("pg_restore")
        if not pg_restore:
            print("[-] pg_restore not found.", file=sys.stderr)
            return 1
        cmd = [
            str(pg_restore),
            "-h", "127.0.0.1",
            "-p", "5432",
            "-U", "mt2",
            "-d", "metin2",
            "--clean",
            "--if-exists",
            "--no-owner",
            str(dump_path),
        ]
        env = os.environ.copy()
        env["PGPASSWORD"] = "mt2"
        res = subprocess.run(cmd, env=env, capture_output=True, text=True, check=False)
        if res.returncode == 0:
            print("[+] Custom dump restored successfully.")
            return 0
        else:
            print(f"[-] pg_restore finished: {res.stderr.strip()}")
            return 0
    else:
        # SQL file
        code, stdout, stderr = run_psql(sql_file=dump_path, db="metin2")
        if code == 0:
            print("[+] SQL dump restored successfully.")
            return 0
        print(f"[-] SQL restore failed: {stderr.strip()}", file=sys.stderr)
        return 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Reforge PostgreSQL database bootstrap and seed CLI."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    subparsers.add_parser("init", help="Create database 'metin2' and apply versioned schema DDL")
    subparsers.add_parser("seed", help="Load minimal lawful synthetic development seed records")

    reset_p = subparsers.add_parser("reset", help="Drop and recreate 'metin2' database from schema + seed")
    reset_p.add_argument("-f", "--force", action="store_true", help="Skip confirmation prompt")

    subparsers.add_parser("check", help="Verify database connectivity, schemas, and table counts")

    restore_p = subparsers.add_parser("restore", help="Restore a custom database dump (.dump or .sql)")
    restore_p.add_argument("file", help="Path to .dump or .sql file")

    args = parser.parse_args(argv)

    handlers = {
        "init": cmd_init,
        "seed": cmd_seed,
        "reset": cmd_reset,
        "check": cmd_check,
        "restore": cmd_restore,
    }
    return handlers[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
