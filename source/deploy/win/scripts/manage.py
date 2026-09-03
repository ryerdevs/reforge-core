#!/usr/bin/env python3
"""
manage.py — Universal cross-platform management CLI for reforge-core.

Zero external dependencies (Python 3 standard library only).
Works identically on Windows and Linux.

Commands:
  start     Start PostgreSQL (if needed), auth, and channel in background
  stop      Stop running server_realms processes
  restart   Stop and start the server stack
  status    Show connectivity, listening ports, and running process status
  backup    Dump the PostgreSQL database into the backups/ directory
  doctor    Run diagnostic checks on configs, executables, and services
"""

from __future__ import annotations

import argparse
import datetime
import os
import socket
import subprocess
import sys
import time
from pathlib import Path


def get_repo_root() -> Path:
    if os.environ.get("REFORGE_ROOT"):
        return Path(os.environ["REFORGE_ROOT"]).resolve()
    # scripts/manage.py -> parents[1] is repo root
    return Path(__file__).resolve().parent.parent


def get_deploy_dir(repo_root: Path) -> Path:
    if os.environ.get("REFORGE_DEPLOY_WIN"):
        return Path(os.environ["REFORGE_DEPLOY_WIN"]).resolve()
    # If script is located directly inside deploy bundle
    direct_bundle = Path(__file__).resolve().parent.parent
    if (direct_bundle / "config").is_dir() and (
        (direct_bundle / "server_realms.exe").is_file()
        or (direct_bundle / "server_realms").is_file()
    ):
        return direct_bundle
    return (repo_root / "source" / "deploy" / "win").resolve()


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
        print("[*] PostgreSQL is stopped on 127.0.0.1:5432. Attempting to start service...")

    if sys.platform == "win32":
        # Try Windows service names
        for svc in ["postgresql-metin2", "postgresql"]:
            try:
                subprocess.run(["net", "start", svc], capture_output=True, check=False)
            except OSError:
                pass
            if is_port_open("127.0.0.1", 5432):
                break
    else:
        # Try Linux systemd / init
        for cmd in [["systemctl", "start", "postgresql"], ["service", "postgresql", "start"]]:
            try:
                subprocess.run(cmd, capture_output=True, check=False)
            except OSError:
                pass
            if is_port_open("127.0.0.1", 5432):
                break

    # Wait up to 3.5 seconds
    for _ in range(15):
        time.sleep(0.2)
        if is_port_open("127.0.0.1", 5432):
            if verbose:
                print("[+] PostgreSQL service started successfully.")
            return True

    if verbose:
        print("[-] ERROR: PostgreSQL is not responding on 127.0.0.1:5432.", file=sys.stderr)
        print("    Please start your PostgreSQL service or docker container first.", file=sys.stderr)
    return False


def find_server_realms(deploy_dir: Path, repo_root: Path) -> Path | None:
    exe_name = "server_realms.exe" if sys.platform == "win32" else "server_realms"

    # 1. In deploy_dir
    candidate = deploy_dir / exe_name
    if candidate.is_file():
        return candidate

    # 2. In target/release or target/debug
    for profile in ["release", "debug"]:
        candidate = repo_root / "source" / "reforge" / "target" / profile / exe_name
        if candidate.is_file():
            return candidate

    return None


def find_config(deploy_dir: Path, role: str) -> Path | None:
    file_name = f"{role}.toml"
    # 1. deploy_dir/config/<role>.toml
    candidate = deploy_dir / "config" / file_name
    if candidate.is_file():
        return candidate

    # 2. deploy_dir/<role>.toml
    candidate = deploy_dir / file_name
    if candidate.is_file():
        return candidate

    # 3. deploy_dir/config/examples/<role>.example.toml
    candidate = deploy_dir / "config" / "examples" / f"{role}.example.toml"
    if candidate.is_file():
        return candidate

    return None


def cmd_start(args: argparse.Namespace) -> int:
    repo_root = get_repo_root()
    deploy_dir = get_deploy_dir(repo_root)

    print(f"[*] Deploy directory: {deploy_dir}")

    # 1. Verify / start PostgreSQL
    if not ensure_postgres(verbose=True):
        return 1

    # 2. Locate server_realms binary
    exe_path = find_server_realms(deploy_dir, repo_root)
    if not exe_path:
        print(f"[-] ERROR: server_realms binary not found in {deploy_dir}", file=sys.stderr)
        return 1

    # 3. Locate configs
    auth_cfg = find_config(deploy_dir, "auth")
    channel_cfg = find_config(deploy_dir, "channel")

    if not auth_cfg:
        print(f"[-] ERROR: auth.toml not found in {deploy_dir}/config", file=sys.stderr)
        return 1
    if not channel_cfg:
        print(f"[-] ERROR: channel.toml not found in {deploy_dir}/config", file=sys.stderr)
        return 1

    # 4. Stop existing server_realms
    cmd_stop(args, silent=True)
    time.sleep(0.5)

    # 5. Prepare logs
    logs_dir = deploy_dir / "logs"
    logs_dir.mkdir(parents=True, exist_ok=True)
    ts = datetime.datetime.now().strftime("%H%M%S")

    auth_out = open(logs_dir / f"auth.{ts}.out.log", "w", encoding="utf-8")
    auth_err = open(logs_dir / f"auth.{ts}.err.log", "w", encoding="utf-8")
    channel_out = open(logs_dir / f"channel.{ts}.out.log", "w", encoding="utf-8")
    channel_err = open(logs_dir / f"channel.{ts}.err.log", "w", encoding="utf-8")

    spawn_kwargs = {
        "cwd": str(deploy_dir),
        "stdin": subprocess.DEVNULL,
    }
    if sys.platform == "win32":
        # CREATE_NEW_PROCESS_GROUP (0x200) | DETACHED_PROCESS (0x8)
        spawn_kwargs["creationflags"] = 0x00000208
    else:
        spawn_kwargs["start_new_session"] = True

    try:
        # Spawn Auth
        subprocess.Popen(
            [str(exe_path), "--role", "auth", "--config", str(auth_cfg)],
            stdout=auth_out,
            stderr=auth_err,
            **spawn_kwargs,
        )

        # Spawn Channel
        subprocess.Popen(
            [str(exe_path), "--role", "channel", "--config", str(channel_cfg)],
            stdout=channel_out,
            stderr=channel_err,
            **spawn_kwargs,
        )

        print(f"[+] Auth and Channel launched successfully.")
        print(f"    Logs: logs/auth.{ts}.* and logs/channel.{ts}.* in {logs_dir}")
        print("[*] Checking listener endpoints...")
        time.sleep(1.0)
        cmd_status(args)
        return 0
    except OSError as e:
        print(f"[-] Failed to launch server_realms: {e}", file=sys.stderr)
        return 1


def cmd_stop(args: argparse.Namespace, silent: bool = False) -> int:
    if not silent:
        print("[*] Stopping server_realms processes...")

    if sys.platform == "win32":
        cmd = ["taskkill", "/F", "/IM", "server_realms.exe"]
    else:
        cmd = ["pkill", "-9", "server_realms"]

    res = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if not silent:
        if res.returncode == 0:
            print("[+] server_realms processes stopped.")
        else:
            print("[*] No running server_realms processes found.")
    return 0


def cmd_restart(args: argparse.Namespace) -> int:
    cmd_stop(args)
    time.sleep(1.0)
    return cmd_start(args)


def cmd_status(args: argparse.Namespace) -> int:
    pg_ok = is_port_open("127.0.0.1", 5432)
    auth_ok = is_port_open("127.0.0.1", 30001)
    channel_ok = is_port_open("127.0.0.1", 30003)

    print("\n--- Reforge Server Stack Status ---")
    print(f"  PostgreSQL (:5432) : {'RUNNING' if pg_ok else 'STOPPED'}")
    print(f"  Auth Role  (:30001): {'LISTENING' if auth_ok else 'STOPPED'}")
    print(f"  Channel    (:30003): {'LISTENING' if channel_ok else 'STOPPED'}")

    # Inspect running PIDs on Windows
    if sys.platform == "win32":
        res = subprocess.run(
            ["tasklist", "/FI", "IMAGENAME eq server_realms.exe", "/NH"],
            capture_output=True,
            text=True,
            check=False,
        )
        pids = []
        for line in res.stdout.splitlines():
            parts = line.split()
            if len(parts) >= 2 and parts[0].lower() == "server_realms.exe":
                try:
                    pids.append(int(parts[1]))
                except ValueError:
                    pass
        if pids:
            print(f"  PIDs (server_realms): {pids}")
    print("-----------------------------------\n")
    return 0


def cmd_backup(args: argparse.Namespace) -> int:
    repo_root = get_repo_root()
    deploy_dir = get_deploy_dir(repo_root)
    backup_dir = Path(os.environ.get("REFORGE_BACKUP_DIR", deploy_dir / "backups"))
    backup_dir.mkdir(parents=True, exist_ok=True)

    ts = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    outfile = backup_dir / f"metin2_backup_{ts}.dump"

    pg_dump = os.environ.get("PGDUMP_PATH", "pg_dump")
    print(f"[*] Starting backup to {outfile}...")

    cmd = [
        pg_dump,
        "-h", "127.0.0.1",
        "-p", "5432",
        "-U", "mt2",
        "-Fc",
        "-f", str(outfile),
        "metin2",
    ]
    env = os.environ.copy()
    if "PGPASSWORD" not in env:
        env["PGPASSWORD"] = "mt2"

    try:
        res = subprocess.run(cmd, env=env, capture_output=True, text=True, check=False)
        if res.returncode == 0 and outfile.is_file():
            size_kb = outfile.stat().st_size / 1024
            print(f"[+] Backup completed successfully: {outfile} ({size_kb:.1f} KB)")
            return 0
        print(f"[-] pg_dump failed: {res.stderr.strip()}", file=sys.stderr)
        return 1
    except FileNotFoundError:
        print(f"[-] ERROR: '{pg_dump}' not found in PATH or PGDUMP_PATH.", file=sys.stderr)
        return 1


def cmd_doctor(args: argparse.Namespace) -> int:
    repo_root = get_repo_root()
    deploy_dir = get_deploy_dir(repo_root)

    print("=== Reforge Environment Doctor ===")
    print(f"Python Version : {sys.version.split()[0]} ({sys.platform})")
    print(f"Repo Root      : {repo_root}")
    print(f"Deploy Bundle  : {deploy_dir}")

    # Check postgres
    pg = is_port_open("127.0.0.1", 5432)
    print(f"PostgreSQL 5432: {'[OK] Responding' if pg else '[WARN] Not responding'}")

    # Check server_realms
    exe = find_server_realms(deploy_dir, repo_root)
    if exe:
        print(f"server_realms  : [OK] Found at {exe}")
    else:
        print(f"server_realms  : [FAIL] Not found in deploy_dir or target/")

    # Check configs
    auth = find_config(deploy_dir, "auth")
    channel = find_config(deploy_dir, "channel")
    print(f"Config auth    : {'[OK] ' + str(auth) if auth else '[FAIL] Missing auth.toml'}")
    print(f"Config channel : {'[OK] ' + str(channel) if channel else '[FAIL] Missing channel.toml'}")

    # Check logs writeable
    logs_dir = deploy_dir / "logs"
    try:
        logs_dir.mkdir(parents=True, exist_ok=True)
        test_file = logs_dir / ".probe_write"
        test_file.write_text("test")
        test_file.unlink()
        print(f"Logs Directory : [OK] Writeable ({logs_dir})")
    except OSError as e:
        print(f"Logs Directory : [FAIL] Cannot write ({e})")

    # Check database schema & seed if postgres is running
    if pg:
        try:
            import bootstrap_db
            code, stdout, _ = bootstrap_db.run_psql(
                sql_cmd="SELECT count(*) FROM information_schema.tables WHERE table_schema IN ('account','common','player','log','world');",
                db="metin2",
            )
            tbl_count = stdout.strip() if code == 0 else "0"
            code, stdout, _ = bootstrap_db.run_psql(
                sql_cmd="SELECT count(*) FROM account.account;",
                db="metin2",
            )
            acc_count = stdout.strip() if code == 0 else "0"
            db_status = f"[OK] {tbl_count} tables, {acc_count} accounts" if int(tbl_count or 0) >= 30 else "[WARN] Incomplete (run: manage.py db init)"
            print(f"Database Schema: {db_status}")
        except Exception:
            pass

    print("==================================")
    return 0


def cmd_db(args: argparse.Namespace) -> int:
    import bootstrap_db

    handlers = {
        "init": bootstrap_db.cmd_init,
        "seed": bootstrap_db.cmd_seed,
        "reset": bootstrap_db.cmd_reset,
        "check": bootstrap_db.cmd_check,
        "restore": bootstrap_db.cmd_restore,
    }
    return handlers[args.db_command](args)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Cross-platform management CLI for reforge-core server stack."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    subparsers.add_parser("start", help="Start PostgreSQL (if needed), auth, and channel")
    subparsers.add_parser("stop", help="Stop running server_realms processes")
    subparsers.add_parser("restart", help="Restart server_realms")
    subparsers.add_parser("status", help="Show port status and running processes")
    subparsers.add_parser("backup", help="Run pg_dump backup into backups/")
    subparsers.add_parser("doctor", help="Perform health diagnosis of environment")

    # db subcommand group
    db_parser = subparsers.add_parser("db", help="Manage PostgreSQL database schema, bootstrap, and seed")
    db_sub = db_parser.add_subparsers(dest="db_command", required=True)
    db_sub.add_parser("init", help="Create database 'metin2' and apply versioned schema DDL")
    db_sub.add_parser("seed", help="Load minimal lawful synthetic development seed records")
    db_reset = db_sub.add_parser("reset", help="Drop and recreate 'metin2' database from schema + seed")
    db_reset.add_argument("-f", "--force", action="store_true", help="Skip confirmation prompt")
    db_sub.add_parser("check", help="Verify database connectivity, schemas, and table counts")
    db_restore = db_sub.add_parser("restore", help="Restore a custom database dump (.dump or .sql)")
    db_restore.add_argument("file", help="Path to .dump or .sql file")

    args = parser.parse_args(argv)

    handlers = {
        "start": cmd_start,
        "stop": cmd_stop,
        "restart": cmd_restart,
        "status": cmd_status,
        "backup": cmd_backup,
        "doctor": cmd_doctor,
        "db": cmd_db,
    }
    return handlers[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
