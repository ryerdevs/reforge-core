#!/bin/bash
# G-PG env probe (part 2/3): MariaDB alive, systemd, memory, disk, tools
set -x
mariadb -h127.0.0.1 -umt2 -pmt2 -N -e "SELECT @@version; SHOW DATABASES;"
echo ===
systemctl is-system-running 2>&1 | head -1
echo ===
free -m | head -2
echo ===
df -h / | tail -1
echo ===
which psql pg_ctlcluster gpg python3 mysqldump 2>&1
echo ===
ls /usr/lib/postgresql/ 2>/dev/null
echo ===
whoami; id -u
