#!/usr/bin/env bash
# Check del loop "Base jugable" — /loop check script
# Exit 0 = criterio cumplido. SCORE: <n> = progreso (tests pasados).
# Uso: bash scripts/loop_check.sh   (desde la raiz C:\projects\Metin2)
set -u
cd "$(dirname "$0")/../source/reforge" || { echo "SCORE: 0"; exit 1; }

# 1. El workspace debe compilar
cargo build --workspace >/dev/null 2>&1 || { echo "SCORE: 0"; echo "BUILD FAILED"; exit 1; }

# 2. Tests del workspace
out=$(cargo test --workspace 2>&1)
passed=$(echo "$out" | grep -oE '[0-9]+ passed' | awk '{s+=$1} END {print s+0}')
failed=$(echo "$out" | grep -oE '[0-9]+ failed' | awk '{s+=$1} END {print s+0}')

echo "SCORE: $passed"
echo "passed=$passed failed=$failed"

# 3. Criterio de "hecho": 0 tests fallidos y al menos los 564 de antes
[ "$failed" -eq 0 ] && [ "$passed" -ge 564 ] && exit 0 || exit 1
