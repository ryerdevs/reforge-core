#!/bin/bash
# G-PG part 2/3: install PostgreSQL 18 from PGDG on Debian 12 bookworm
set -e
export DEBIAN_FRONTEND=noninteractive
echo "== apt-get update (baseline) =="
apt-get update -qq
echo "== apt-get install gnupg curl =="
apt-get install -y -qq gnupg curl
echo "== add PGDG key =="
curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc | gpg --dearmor -o /usr/share/keyrings/pgdg.gpg
ls -l /usr/share/keyrings/pgdg.gpg
echo "== add PGDG repo =="
echo "deb [signed-by=/usr/share/keyrings/pgdg.gpg] http://apt.postgresql.org/pub/repos/apt bookworm-pgdg main" > /etc/apt/sources.list.d/pgdg.list
cat /etc/apt/sources.list.d/pgdg.list
echo "== apt-get update (with pgdg) =="
apt-get update -qq
echo "== apt-get install postgresql-18 =="
apt-get install -y -qq postgresql-18
echo "== result =="
ls /usr/lib/postgresql/
/usr/lib/postgresql/18/bin/postgres --version
pg_lsclusters
sync
echo "== DONE =="
