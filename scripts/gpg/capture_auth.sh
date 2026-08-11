#!/bin/bash
# ============================================================================
# F0 verification harness — captura REAL del LOGIN3 contra el auth (WSL).
#
# ROADMAP F0: "real packet capture (tcpdump) against the C++ server as golden
# tests". El hito: un LOGIN3 real capturado del wire se parsea y re-serializa
# byte-por-byte idéntico (protocol::TPacketCGLogin3, 88 B auth + version +
# hwid).
#
# Flujo:
#   (a) instala tcpdump si falta (apt-get);
#   (b) arranca tcpdump en background -> /tmp/gpg/capture_auth.pcap
#       (interfaz `any`: el auth acepta 172.25.104.175:30001 [eth0] y
#       127.0.0.1:30001 [lo]; el peer se conecta a 127.0.0.1 — el filtro
#       `port 30001` captura ambas rutas);
#   (c) build del peer f16_peer en WSL (ELF, toolchain /root/.cargo/bin) y
#       ejecución: handshake + LOGIN3 88 B (version 40999 + hwid) + respuesta
#       del auth (GC_AUTH_SUCCESS esperado);
#   (d) detiene tcpdump (SIGINT para que flushee el pcap);
#   (e) extrae el LOGIN3 del pcap (reensamblado TCP por secuencia) y guarda el
#       fixture golden en source/reforge/protocol/tests/golden/.
#
# El peer hace UN login (test/1234, lang es) — mismo efecto que un login
# normal del cliente (actualiza last_play/lang/hwid). No toca db ni core.
#
# Uso (dentro de WSL):
#   bash /mnt/c/projects/Metin2/scripts/gpg/capture_auth.sh
#
# Requiere: stack levantado (auth Rust en 0.0.0.0:30001), rustup en /root/.cargo.
# ============================================================================
set -u

ROOT=/mnt/c/projects/Metin2/source/reforge
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PCAP=/tmp/gpg/capture_auth.pcap
FIXTURE="$ROOT/protocol/tests/golden/auth_login3_40999.bin"
PEER_ARGS=(127.0.0.1 30001 --login3 --version 40999 --hwid aabbccddeeff00112233445566778899)

mkdir -p /tmp/gpg "$(dirname "$FIXTURE")"

echo "== (a) tcpdump =="
if ! command -v tcpdump >/dev/null 2>&1; then
    echo "tcpdump no instalado — apt-get install -y tcpdump"
    apt-get update
    apt-get install -y tcpdump
fi
tcpdump --version 2>&1 | head -1

echo "== (b) captura -> $PCAP =="
rm -f "$PCAP"
# `any`: cubre lo (127.0.0.1) y eth0 (172.25.104.175). -s 0 = snaplen completo.
tcpdump -i any -s 0 -w "$PCAP" 'port 30001' 2>/tmp/gpg/tcpdump.err &
TCPDUMP_PID=$!
sleep 2

echo "== (c) build + peer =="
# Toolchain ELF de WSL (rustup en /root/.cargo, activado solo en shells
# interactivos vía .bashrc — lo cargamos explícitamente).
. "$HOME/.cargo/env"
cargo build --release --example f16_peer --manifest-path "$ROOT/Cargo.toml"
if [ $? -ne 0 ]; then
    kill -INT "$TCPDUMP_PID" 2>/dev/null
    echo "BUILD FAILED"
    exit 1
fi

echo "--- peer: ${PEER_ARGS[*]} ---"
timeout 30 "$ROOT/target/release/examples/f16_peer" "${PEER_ARGS[@]}"
PEER_EXIT=$?
echo "--- peer exit: $PEER_EXIT ---"

echo "== (d) stop tcpdump =="
sleep 1
kill -INT "$TCPDUMP_PID" 2>/dev/null
wait "$TCPDUMP_PID" 2>/dev/null

echo "== (e) extracción del LOGIN3 -> fixture =="
python3 "$SCRIPT_DIR/extract_pcap_login3.py" "$PCAP" "$FIXTURE"
EXTRACT_EXIT=$?

echo "== resultado =="
if [ -f "$FIXTURE" ]; then
    ls -l "$FIXTURE"
    md5sum "$FIXTURE"
    xxd "$FIXTURE" | head -8
fi
sync
echo "== DONE (peer exit: $PEER_EXIT, extract exit: $EXTRACT_EXIT) =="
