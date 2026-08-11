#!/bin/bash
# G-PG part 2/3: start cluster, configure auth, role mt2, database metin2 + schemas per domain
set -e
export PGDATA=/var/lib/postgresql/18/main

echo "== start cluster =="
pg_ctlcluster 18 main start
pg_lsclusters

echo "== pg_hba.conf current host lines =="
grep -n "^host" /etc/postgresql/18/main/pg_hba.conf

echo "== ensure 127.0.0.1 scram-sha-256 lines =="
if ! grep -qE "^host\s+all\s+all\s+127\.0\.0\.1/32\s+scram-sha-256" /etc/postgresql/18/main/pg_hba.conf; then
  echo "host all all 127.0.0.1/32 scram-sha-256" >> /etc/postgresql/18/main/pg_hba.conf
  pg_ctlcluster 18 main reload
  echo "added host line"
fi
if ! grep -qE "^host\s+all\s+all\s+::1/128\s+scram-sha-256" /etc/postgresql/18/main/pg_hba.conf; then
  echo "host all all ::1/128 scram-sha-256" >> /etc/postgresql/18/main/pg_hba.conf
  pg_ctlcluster 18 main reload
  echo "added ::1 line"
fi

echo "== create role mt2 + database metin2 + schemas =="
cat > /tmp/gpg/pg-init.sql <<'SQL'
DO $$
BEGIN
   IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'mt2') THEN
      CREATE ROLE mt2 LOGIN PASSWORD 'mt2' CREATEDB;
   END IF;
END
$$;
SQL
runuser -u postgres -- psql -v ON_ERROR_STOP=1 -f /tmp/gpg/pg-init.sql
runuser -u postgres -- psql -tAc "SELECT 1 FROM pg_database WHERE datname='metin2'" | grep -q 1 || \
  runuser -u postgres -- createdb -O mt2 -E UTF8 -T template0 --locale=C.UTF-8 metin2

cat > /tmp/gpg/pg-schemas.sql <<'SQL'
CREATE SCHEMA IF NOT EXISTS account AUTHORIZATION mt2;
CREATE SCHEMA IF NOT EXISTS common  AUTHORIZATION mt2;
CREATE SCHEMA IF NOT EXISTS player  AUTHORIZATION mt2;
CREATE SCHEMA IF NOT EXISTS log     AUTHORIZATION mt2;
GRANT ALL ON SCHEMA account, common, player, log TO mt2;
ALTER ROLE mt2 SET search_path = account, common, player, log;
SQL
runuser -u postgres -- psql -d metin2 -v ON_ERROR_STOP=1 -f /tmp/gpg/pg-schemas.sql

echo "== verify connection as mt2 =="
PGPASSWORD=mt2 psql -h 127.0.0.1 -U mt2 -d metin2 -tAc "SELECT current_user, current_database(), version();"
PGPASSWORD=mt2 psql -h 127.0.0.1 -U mt2 -d metin2 -tAc "SELECT nspname FROM pg_namespace WHERE nspname IN ('account','common','player','log') ORDER BY 1;"
sync
echo "== DONE =="
