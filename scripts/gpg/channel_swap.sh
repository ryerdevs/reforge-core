#!/bin/bash
# F4 slice 2 verification: swap the C++ core1 for the Rust channel on 30003.
# Usage: channel_swap.sh            -> Rust channel up on 30003
#        channel_swap.sh --restore  -> C++ core1 back
export LANG=C
SV=/home/m2/source/metin2_svfiles/main/srv1
if [ "$1" = "--restore" ]; then
  echo "=== restore: matar canal Rust + arrancar core C++ ==="
  pkill -f 'server_realms --role channel' 2>/dev/null
  sleep 2
  cd "$SV/chan/ch1/core1" && setsid nohup ./srv1-ch1-core1 > stdout 2>&1 &
  for i in $(seq 1 15); do ss -tln 2>/dev/null | grep -q ':30003' && { echo "core1 C++ UP (30003)"; break; }; sleep 2; done
  exit 0
fi
echo "=== swap: matar core1 C++ ==="
pkill -f 'srv1-ch1-core1' 2>/dev/null
sleep 3
echo "=== arrancar canal Rust (30003) ==="
cd "$SV" && setsid nohup ./share/bin/server_realms --role channel --config server_realms_channel.toml >> /tmp/gpg/channel.log 2>&1 &
for i in $(seq 1 15); do ss -tln 2>/dev/null | grep -q ':30003' && { echo "canal Rust UP (30003)"; break; }; sleep 2; done
echo "=== estado ==="
ss -tln 2>/dev/null | grep -E ':(30001|30003)' 
pgrep -fl 'server_realms|srv1-ch1' | head -4
tail -3 /tmp/gpg/channel.log 2>/dev/null
echo "=== listo para el login del cliente (test/1234 -> select por el canal Rust) ==="
