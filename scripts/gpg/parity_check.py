#!/usr/bin/env python3
"""G-PG parity checker: row count + md5 per phase-1 table, MariaDB vs PostgreSQL.

Spec: docs/plans/server-rewrite.md §8.2.1d ("scripts/gpg/parity_check.py — per table,
row count + md5 over the streamed sorted rows from both engines (bytea normalized to
hex); non-zero exit on mismatch").

Method (ponytail, documented):
- Python3 + pymysql (MariaDB 127.0.0.1:3306, mt2/mt2) + psycopg2 (PG 127.0.0.1:5432,
  db metin2, mt2/mt2). Table names are qualified as <schema>.<table> on both sides
  (MariaDB database == PG schema for the phase-1 set); no search_path juggling.
- Per table: (1) column names+order compared (cursor.description); (2) row counts;
  (3) all rows fetched in batches (fetchmany, streamed) and serialized to a canonical
  byte string: bytes/bytea -> lowercase hex, None -> NULL, everything else -> str();
  rows are SORTED on the canonical bytes in Python (engine collations differ, so the
  sort must happen on the normalized form to be byte-comparable); md5 is fed
  incrementally. Phase-1 tables are small (max item_proto 11k rows) so the sorted
  batch lives in memory; the harness is per-table so memory stays bounded.
- Exit: 0 all equal, 1 any mismatch, 2 connectivity/setup error.

Modes (decision post-cutover 2026-08-11 — PG is OPERATIVE, MariaDB FROZEN):
- default (no mode flag): MariaDB LIVE vs PG live. MariaDB is the frozen migration
  source, so a diff here is PG having advanced operationally (the user plays on PG).
- --make-snapshot <file.json>: captures the REFERENCE state from MariaDB (the frozen
  cutover state) into a json file: {"<schema.table>": {"rows": n, "md5": m}}.
  The md5 is computed over the SAME canonical rows as the live check (volatile
  columns excluded, see VOLATILE_COLUMNS).
- --snapshot <file.json>: compares PG against the snapshot (same OK/DIFF output and
  exit semantics as the default mode). This is the post-cutover mode: the harness no
  longer depends on MariaDB staying reachable/frozen; the snapshot IS the oracle.
- Known OPERATIONAL diffs (PG advanced, MariaDB frozen — expected, documented):
  * account.account: lang ('es' vs frozen 'en' — the client overwrites account.lang
    on every login, Language System) and last_play/hwid (excluded as volatile).
  * player.player: +3 rows (in-game "hol" + two e2e_rust_* throwaways from the F2b
    auth E2E), player_index.pid3=5 (the "hol" slot), player.item: +1 row (items of
    the in-game character), messenger_list: +4 rows (E2E leftovers).
  Anything ELSE is an UNEXPECTED diff (exit 1) and must be investigated.

Usage:
  parity_check.py [--table <schema.table>]... [--verbose]
  parity_check.py --make-snapshot <file.json> [--table <schema.table>]...
  parity_check.py --snapshot <file.json> [--table <schema.table>]...
"""
import argparse
import hashlib
import json
import sys

import pymysql
import psycopg2

MYSQL = dict(host="127.0.0.1", port=3306, user="mt2", password="mt2",
             charset="utf8mb4", connect_timeout=10)
PG = dict(host="127.0.0.1", port=5432, user="mt2", password="mt2",
          dbname="metin2", connect_timeout=10)

# Phase-1 login/boot subset (spec §8.2.1b): the 17 boot/login tables + the 4
# character-load tables (item/quest/affect/safebox) migrated 2026-08-10.
# NOTE: the 26 `log.*` tables are deliberately NOT compared: they are DDL-only
# in PG (empty) by spec — MariaDB holds runtime rows (bootlog, log, levellog...)
# that are NOT migrated; a parity diff there would be a false negative.
TABLES = [
    "account.account",
    "player.player",
    "player.player_index",
    "player.mob_proto",
    "player.item_proto",
    "player.shop",
    "player.shop_item",
    "player.skill_proto",
    "player.refine_proto",
    "player.item_attr",
    "player.item_attr_rare",
    "player.banword",
    "player.land",
    "player.object_proto",
    "player.object",
    "player.monarch",
    "common.locale",
    "common.priv_settings",
    "player.item",
    "player.quest",
    "player.affect",
    "player.safebox",
    # boot gaps found by the gate (2026-08-10): guild/marriage/war queries at db boot,
    # exp_table/spam_db/gmlist/gmhost at game boot (spec §8.2.1b common list)
    "player.guild",
    "player.guild_war_reservation",
    "player.marriage",
    "common.exp_table",
    "common.spam_db",
    "common.gmlist",
    "common.gmhost",
    # item awards (character select / ItemAwardManager, 2026-08-10 gate)
    "player.item_award",
    # messenger (world entry, 2026-08-10 — MessengerManager load)
    "player.messenger_list",
]

# Volatile columns that legitimately diverge (2026-08-10 gate): account.last_play is
# written by the LIVE runtime on every login. PostgreSQL is the operative store now;
# MariaDB is frozen at the cutover (migration source) - the live write lands only on
# PG. Excluded from the row comparison (column-order check too).
# account.hwid: F2b auth-Rust column (2026-08-11) - exists only on PG (the operative
# store); MariaDB is frozen without it. A bare extra column would break the
# column-order check, hence the exclusion.
VOLATILE_COLUMNS = {"account.account": {"last_play", "hwid"}}

def canon(value):
    """Canonical serialization of a cell from either driver."""
    if value is None:
        return "NULL"
    if isinstance(value, (bytes, bytearray, memoryview)):
        return bytes(value).hex()
    if isinstance(value, str):
        return value
    return str(value)

def load_table(cur, table, exclude=(), batch=1000):
    """Yield canonical rows sorted (deterministic byte order); drop excluded columns."""
    cur.execute(f"SELECT * FROM {table}")
    cols = [d[0] for d in cur.description]
    skip = {i for i, c in enumerate(cols) if c.lower() in exclude}
    rows = []
    while True:
        chunk = cur.fetchmany(batch)
        if not chunk:
            break
        for row in chunk:
            rows.append("\x1f".join(canon(v) for i, v in enumerate(row) if i not in skip))
    rows.sort()
    return cols, rows

def md5_of(rows):
    h = hashlib.md5()
    for r in rows:
        h.update(r.encode("utf-8", "surrogateescape"))
        h.update(b"\x1e")
    return h.hexdigest()

def check_table(name, verbose):
    schema, tbl = name.split(".", 1)
    qual = f"{schema}.{tbl}"
    problems = []

    my = pymysql.connect(**MYSQL)
    pg = psycopg2.connect(**PG)
    try:
        with my.cursor() as mc, pg.cursor() as pc:
            mc.execute(f"SELECT COUNT(*) FROM {qual}")
            my_count = mc.fetchone()[0]
            pc.execute(f"SELECT COUNT(*) FROM {qual}")
            pg_count = pc.fetchone()[0]

            exclude = VOLATILE_COLUMNS.get(name, set())
            my_cols, my_rows = load_table(mc, qual, exclude)
            pg_cols, pg_rows = load_table(pc, qual, exclude)

            # Column identifiers: MySQL is case-insensitive and PG folds unquoted
            # identifiers to lowercase (dwVnum -> dwvnum). Compare case-insensitively
            # (volatile columns excluded): same logical column set in the same
            # physical order == parity.
            my_cols_lc = [c.lower() for c in my_cols if c.lower() not in exclude]
            pg_cols_lc = [c.lower() for c in pg_cols if c.lower() not in exclude]
            if my_cols_lc != pg_cols_lc:
                problems.append(
                    f"column order differs: MariaDB={my_cols} PG={pg_cols}")
            if my_count != pg_count:
                problems.append(f"row count differs: MariaDB={my_count} PG={pg_count}")
            if len(my_rows) != len(pg_rows):
                problems.append(
                    f"fetched rows differ: MariaDB={len(my_rows)} PG={len(pg_rows)}")
            if not problems:
                my_md5 = md5_of(my_rows)
                pg_md5 = md5_of(pg_rows)
                if my_md5 != pg_md5:
                    problems.append(f"md5 differs: MariaDB={my_md5} PG={pg_md5}")
    finally:
        my.close()
        pg.close()

    if problems:
        for p in problems:
            print(f"DIFF {name}: {p}")
        return 1
    print(f"OK   {name}: rows={my_count} md5={my_md5}")
    if verbose:
        print(f"     {qual}: {len(my_cols)} columns in order: {', '.join(my_cols)}")
    return 0

def snapshot_table(cur, name):
    """(count, md5) de una tabla sobre UN cursor (modo snapshot): mismas
    filas canónicas que el modo live (volatile columns excluidas) — así el
    md5 del snapshot es directamente comparable con el de PG."""
    exclude = VOLATILE_COLUMNS.get(name, set())
    cur.execute(f"SELECT COUNT(*) FROM {name}")
    count = cur.fetchone()[0]
    _, rows = load_table(cur, name, exclude)
    return count, md5_of(rows)

def make_snapshot(path, tables):
    """Captura el estado de referencia desde MariaDB (la fuente congelada =
    el estado del cutover): {"<schema.table>": {"rows": n, "md5": m}}."""
    my = pymysql.connect(**MYSQL)
    snap = {}
    try:
        with my.cursor() as mc:
            for t in tables:
                try:
                    count, md5 = snapshot_table(mc, t)
                except Exception as e:  # noqa: BLE001
                    print(f"ERROR {t}: {type(e).__name__}: {e}", file=sys.stderr)
                    return 2
                snap[t] = {"rows": count, "md5": md5}
    finally:
        my.close()
    with open(path, "w", encoding="utf-8") as f:
        json.dump(snap, f, indent=2, sort_keys=True)
        f.write("\n")
    print(f"SNAPSHOT {path}: {len(snap)} tables (origen: MariaDB congelada = cutover)")
    for t, v in snap.items():
        print(f"  {t}: rows={v['rows']} md5={v['md5']}")
    return 0

def check_snapshot(path, tables):
    """Compara PG contra el snapshot (mismo formato OK/DIFF y exit que el
    modo live). Los diffs operacionales conocidos están documentados en el
    docstring del módulo; cualquier otro diff es inesperado (exit 1)."""
    try:
        with open(path, "r", encoding="utf-8") as f:
            snap = json.load(f)
    except OSError as e:
        print(f"ERROR snapshot {path}: {e}", file=sys.stderr)
        return 2
    pg = psycopg2.connect(**PG)
    rc = 0
    try:
        with pg.cursor() as pc:
            for t in tables:
                if t not in snap:
                    print(f"ERROR {t}: no está en el snapshot", file=sys.stderr)
                    rc = 2
                    continue
                try:
                    count, md5 = snapshot_table(pc, t)
                except Exception as e:  # noqa: BLE001
                    print(f"ERROR {t}: {type(e).__name__}: {e}", file=sys.stderr)
                    rc = 2
                    continue
                exp = snap[t]
                problems = []
                if exp["rows"] != count:
                    problems.append(f"row count differs: snapshot={exp['rows']} PG={count}")
                if exp["md5"] != md5:
                    problems.append(f"md5 differs: snapshot={exp['md5']} PG={md5}")
                if problems:
                    for p in problems:
                        print(f"DIFF {t}: {p}")
                    rc = 1
                else:
                    print(f"OK   {t}: rows={count} md5={md5}")
    finally:
        pg.close()
    return rc

def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--table", action="append", metavar="schema.table",
                    help="check only this table (repeatable)")
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--make-snapshot", metavar="file.json",
                    help="capture the MariaDB (frozen cutover) state to a json snapshot")
    ap.add_argument("--snapshot", metavar="file.json",
                    help="compare PG against a snapshot file (post-cutover mode)")
    args = ap.parse_args()

    if args.make_snapshot and args.snapshot:
        ap.error("--make-snapshot y --snapshot son mutuamente excluyentes")

    tables = args.table or TABLES
    if args.make_snapshot:
        sys.exit(make_snapshot(args.make_snapshot, tables))
    if args.snapshot:
        sys.exit(check_snapshot(args.snapshot, tables))

    rc = 0
    for t in tables:
        try:
            rc |= check_table(t, args.verbose)
        except Exception as e:  # noqa: BLE001 - report and fail
            print(f"ERROR {t}: {type(e).__name__}: {e}", file=sys.stderr)
            rc = 2
    sys.exit(rc)

if __name__ == "__main__":
    main()
