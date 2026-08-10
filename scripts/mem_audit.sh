#!/bin/bash
# Memory audit for the WSL VM (diagnose why 2GB cap fills up fast).
# Usage: wsl -d Debian-M2 -- bash /mnt/c/projects/Metin2/scripts/mem_audit.sh
echo "=== free ==="
free -m | head -3
echo "=== top RSS ==="
ps aux --sort=-rss | head -12 | awk '{printf "%s %s %sMB %s%%\n", $2, $11, int($6/1024), $4}'
echo "=== meminfo highlights ==="
grep -E '^(MemTotal|MemFree|MemAvailable|Slab|SReclaimable|SUnreclaim|PageTables|Shmem|SwapTotal|SwapFree)' /proc/meminfo
echo "=== mariadb innodb settings ==="
mysql -uroot -e "SHOW VARIABLES LIKE 'innodb_buffer_pool_size';" 2>/dev/null || grep -r "innodb_buffer_pool" /etc/mysql/ 2>/dev/null | grep -v '^#' || echo "default (128M)"
echo "=== DONE ==="
