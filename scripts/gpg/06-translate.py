#!/usr/bin/env python3
# G-PG part 2/3: translate mysqldump INSERT statements to PostgreSQL syntax.
# Input : mysqldump --no-create-info --skip-extended-insert --hex-blob --complete-insert
# Output: PG INSERTs with bytea hex literals, de-backticked identifiers, MySQL escapes unescaped.
import sys

def tokenize_values(s):
    """Split the VALUES (...) payload into tokens (strings kept raw w/ MySQL escapes, numbers, NULL, hexblobs)."""
    tokens = []
    i = 0
    n = len(s)
    while i < n:
        c = s[i]
        if c in " \t\n\r(),":
            i += 1
            continue
        if c == "'":
            j = i + 1
            buf = ["'"]
            while j < n:
                ch = s[j]
                if ch == "\\" and j + 1 < n:
                    buf.append(ch)
                    buf.append(s[j + 1])
                    j += 2
                    continue
                if ch == "'":
                    # check for doubled quote '' (MySQL also escapes ' as \')
                    if j + 1 < n and s[j + 1] == "'":
                        buf.append("''")
                        j += 2
                        continue
                    buf.append("'")
                    j += 1
                    break
                buf.append(ch)
                j += 1
            tokens.append("".join(buf))
            i = j
            continue
        # unquoted token: number, NULL, hex blob
        j = i
        while j < n and s[j] not in " \t\n\r(),":
            j += 1
        tok = s[i:j]
        if tok == "NULL":
            tokens.append("NULL")
        elif tok.startswith("0x") and len(tok) > 2 and all(ch in "0123456789abcdefABCDEF" for ch in tok[2:]):
            tokens.append("'" + tok.replace("0x", "\\x", 1) + "'")
        elif tok == "NULL" or tok.lstrip("+-").isdigit() or "." in tok:
            tokens.append(tok)
        else:
            raise ValueError(f"unexpected token: {tok!r}")
        i = j
    return tokens

MYSQL_ESC = {
    "n": "\n", "r": "\r", "t": "\t", "b": "\b", "0": "\0", "Z": "\x1a",
    "'": "'", '"': '"', "\\": "\\", "%": "%", "_": "_",
}

def pg_string(quoted_raw):
    """quoted_raw starts and ends with '. Unescape MySQL backslash escapes, return PG literal."""
    body = quoted_raw[1:-1]
    out = []
    i = 0
    n = len(body)
    while i < n:
        ch = body[i]
        if ch == "\\" and i + 1 < n:
            nxt = body[i + 1]
            if nxt in MYSQL_ESC:
                out.append(MYSQL_ESC[nxt])
                i += 2
                continue
            # unknown escape: keep both chars (MySQL keeps backslash + char)
            out.append(ch)
            i += 1
            continue
        if ch == "'":
            out.append("''")
            i += 1
            continue
        out.append(ch)
        i += 1
    s = "".join(out)
    if "\0" in s:
        raise ValueError("NUL byte in string value; cannot represent in PG text")
    return "'" + s + "'"

def translate_line(line):
    line = line.rstrip("\n")
    if not line.strip():
        return None
    if line.startswith("/*") or line.startswith("--") or line.startswith("LOCK TABLES") or line.startswith("UNLOCK TABLES"):
        return None
    if not line.startswith("INSERT INTO"):
        raise ValueError(f"unexpected line: {line[:80]!r}")
    m = line[len("INSERT INTO "):]
    # `db`.`tbl` (`c1`,`c2`,...) VALUES (...);
    import re
    mm = re.match(r"^`(?:([^`]+)`\.)?([^`]+)` \((.*?)\) VALUES \((.*)\);$", m, re.S)
    if not mm:
        raise ValueError(f"cannot parse INSERT: {line[:120]!r}")
    tbl = mm.group(2)
    cols = re.sub(r"`", "", mm.group(3))
    vals = mm.group(4)
    tokens = tokenize_values(vals)
    ncols = len(cols.split(","))
    if ncols != len(tokens):
        raise ValueError(f"column/value count mismatch: {ncols} vs {len(tokens)} in {tbl}")
    pg_vals = []
    for t in tokens:
        if t == "NULL":
            pg_vals.append("NULL")
        elif t.startswith("'") and t.endswith("'"):
            if t.startswith("'\\x"):
                pg_vals.append(t)  # already bytea hex literal
            else:
                pg_vals.append(pg_string(t))
        else:
            pg_vals.append(t)
    return f"INSERT INTO {tbl} ({cols}) VALUES ({', '.join(pg_vals)});"

def main():
    total = 0
    for line in sys.stdin:
        out = translate_line(line)
        if out is not None:
            print(out)
            total += 1
    sys.stderr.write(f"translated {total} INSERT statements\n")

if __name__ == "__main__":
    main()
