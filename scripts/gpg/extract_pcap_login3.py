#!/usr/bin/env python3
"""F0 harness — extractor del LOGIN3 real desde un pcap (tcpdump).

Lee un pcap clásico (formato por defecto de `tcpdump -w`, sin pcapng),
reensambla el stream TCP cliente→servidor (por secuencia) y localiza el
LOGIN3 de 88 B (0x6f + login[31] + passwd[17] + keys[16] + lang[3] +
version[4] + hwid[16]) validando cada campo contra los valores esperados
del peer de captura (f16_peer, ver `capture_auth.sh`).

Sin dependencias pesadas: solo `struct` de la stdlib (ponytail: el pcap
clásico es 24 B de cabecera global + registros de longitud simple).

Uso:
    python3 extract_pcap_login3.py <pcap> <fixture.bin>

Salida: resumen del tráfico (dirección, longitud, primer byte de cada
payload TCP), hexdump del stream cliente→servidor y del LOGIN3 extraído,
y el fixture de 88 B escrito en <fixture.bin>.
"""

import hashlib
import struct
import sys

# Valores esperados del peer de captura (f16_peer --login3 --version 40999
# --hwid aabbccddeeff00112233445566778899, test/1234, lang es, keys [0;4]).
EXPECT_LOGIN = b"test"
EXPECT_PASSWD = b"1234"
EXPECT_LANG = b"es\x00"
EXPECT_VERSION = 40999
EXPECT_HWID = bytes.fromhex("aabbccddeeff00112233445566778899")
AUTH_PORT = 30001

# Linktypes soportados (campo `network` de la cabecera global pcap).
LINK_ETHERNET = 1      # DLT_EN10MB (tcpdump -i lo / eth)
LINK_NULL = 0          # DLT_NULL / loopback BSD
LINK_RAW = 101         # DLT_RAW (sin cabecera de enlace)
LINK_SLL = 113         # DLT_LINUX_SLL (tcpdump -i any, libpcap 1.x)
LINK_SLL2 = 276        # DLT_LINUX_SLL2 (libpcap >= 1.10, alternativa)

# Magic leído como LE uint32: el ORDEN de los bytes indica el endianness del
# archivo. d4 c3 b2 a1 (→ 0xA1B2C3D4) = little-endian; a1 b2 c3 d4
# (→ 0xD4C3B2A1) = big-endian. Variantes de nanosegundos: b2 a1 / a1 b2.
MAGICS = {
    0xA1B2C3D4: "<",   # little-endian, microsegundos
    0xD4C3B2A1: ">",   # big-endian, microsegundos
    0xA1B23C4D: "<",   # little-endian, nanosegundos
    0x4D3CB2A1: ">",   # big-endian, nanosegundos
}


def parse_pcap(path):
    """Itera los registros del pcap: (ts_sec, data)."""
    with open(path, "rb") as f:
        gh = f.read(24)
        if len(gh) < 24:
            raise SystemExit(f"pcap demasiado corto: {len(gh)} B (cabecera global = 24 B)")
        magic = struct.unpack_from("<I", gh, 0)[0]
        endian = MAGICS.get(magic)
        if endian is None:
            raise SystemExit(f"magic pcap no reconocido: 0x{magic:08x} (¿pcapng? usar tcpdump -w, formato clásico)")
        (network,) = struct.unpack_from(endian + "I", gh, 20)
        while True:
            rh = f.read(16)
            if len(rh) == 0:
                break
            if len(rh) < 16:
                # Registro final parcial: tcpdump escribió cabecera y datos en
                # fwrites separados y el SIGINT de cierre puede dejar el último
                # registro a medias — los registros completos ya están leídos.
                print(f"    [aviso: registro final parcial ({len(rh)}/16 B de cabecera) — captura cortada por cierre]")
                break
            ts_sec, _ts_frac, incl_len, _orig_len = struct.unpack(endian + "IIII", rh)
            data = f.read(incl_len)
            if len(data) < incl_len:
                print(f"    [aviso: payload parcial ({len(data)}/{incl_len} B) — captura cortada por cierre]")
                break
            yield ts_sec, network, data


def ipv4_payload(frame, linktype):
    """Payload TCP de un frame IPv4, o None si no es IPv4/TCP."""
    if linktype == LINK_ETHERNET:
        if len(frame) < 14:
            return None
        ethertype = struct.unpack_from(">H", frame, 12)[0]
        if ethertype != 0x0800:
            return None
        off = 14
    elif linktype == LINK_SLL:  # linux cooked v1: 16 B, proto @14
        if len(frame) < 16:
            return None
        if struct.unpack_from(">H", frame, 14)[0] != 0x0800:
            return None
        off = 16
    elif linktype == LINK_SLL2:  # linux cooked v2: 20 B, proto @0
        if len(frame) < 20:
            return None
        if struct.unpack_from(">H", frame, 0)[0] != 0x0800:
            return None
        off = 20
    elif linktype == LINK_NULL:
        off = 4
    elif linktype == LINK_RAW:
        off = 0
    else:
        raise SystemExit(f"linktype no soportado: {linktype}")
    if len(frame) < off + 20:
        return None
    ihl = (frame[off] & 0x0F) * 4
    if frame[off + 9] != 6:  # TCP
        return None
    src = frame[off + 12:off + 16]
    dst = frame[off + 16:off + 20]
    tcp = off + ihl
    if len(frame) < tcp + 20:
        return None
    src_port, dst_port = struct.unpack_from(">HH", frame, tcp)
    seq = struct.unpack_from(">I", frame, tcp + 4)[0]
    flags = frame[tcp + 13]
    doff = (frame[tcp + 12] >> 4) * 4
    payload = frame[tcp + doff:]
    return (src, src_port, dst, dst_port, seq, flags, payload)


def reassemble(segments):
    """Reensambla un flujo TCP por secuencia (maneja solapamientos/retransmisiones)."""
    if not segments:
        return b""
    segments = sorted(segments, key=lambda s: s[0])
    # isn: SYN → seq+1; si no hay SYN, la base es el seq del primer segmento.
    base = None
    for seq, flags, payload in segments:
        if flags & 0x02:  # SYN
            base = seq + 1
            break
    if base is None:
        base = segments[0][0]
    out = bytearray()
    next_seq = base
    for seq, _flags, payload in segments:
        if not payload:
            continue
        if seq == next_seq:
            out.extend(payload)
            next_seq += len(payload)
        elif seq < next_seq:  # solapamiento/retransmisión
            skip = next_seq - seq
            if skip < len(payload):
                out.extend(payload[skip:])
                next_seq += len(payload) - skip
        else:  # hueco (no esperado en loopback; se loguea)
            print(f"    [gap en stream TCP: seq {seq} != esperado {next_seq}]")
            out.extend(payload)
            next_seq = seq + len(payload)
    return bytes(out)


def find_login3(stream):
    """Localiza el LOGIN3 de 88 B dentro del stream cliente→servidor."""
    for pos in range(0, len(stream) - 88 + 1):
        b = stream[pos:pos + 88]
        if (b[0] != 0x6F
                or b[1:5] != EXPECT_LOGIN
                or b[32:36] != EXPECT_PASSWD
                or b[49:65] != b"\x00" * 16
                or b[65:68] != EXPECT_LANG
                or struct.unpack_from("<I", b, 68)[0] != EXPECT_VERSION
                or b[72:88] != EXPECT_HWID):
            continue
        return pos, b
    return None, None


def hexdump(data, base=0):
    lines = []
    for i in range(0, len(data), 16):
        chunk = data[i:i + 16]
        hexpart = " ".join(f"{c:02x}" for c in chunk)
        asc = "".join(chr(c) if 32 <= c < 127 else "." for c in chunk)
        lines.append(f"    {base + i:04x}  {hexpart:<47}  {asc}")
    return "\n".join(lines)


def main():
    if len(sys.argv) != 3:
        raise SystemExit("uso: extract_pcap_login3.py <pcap> <fixture.bin>")
    pcap_path, out_path = sys.argv[1], sys.argv[2]

    flows = {}   # (src, sport, dst, dport) -> [(seq, flags, payload)]
    server_flow = None
    n_pkts = 0
    for ts, linktype, data in parse_pcap(pcap_path):
        tcp = ipv4_payload(data, linktype)
        if tcp is None:
            continue
        src, sport, dst, dport, seq, flags, payload = tcp
        n_pkts += 1
        key = (src, sport, dst, dport)
        flows.setdefault(key, []).append((seq, flags, payload))
        if dport == AUTH_PORT and server_flow is None:
            server_flow = key

    if server_flow is None:
        raise SystemExit(f"no se encontró tráfico hacia el puerto {AUTH_PORT} en {pcap_path}")

    print(f"pcap: {pcap_path} ({n_pkts} paquetes TCP)")
    print("flujos TCP detectados:")
    for (src, sport, dst, dport), segs in sorted(flows.items()):
        tot = sum(len(p) for _, _, p in segs)
        print(f"    {'.'.join(map(str, src))}:{sport} -> {'.'.join(map(str, dst))}:{dport}  ({len(segs)} seg, {tot} B payload)")

    c2s = reassemble(flows[server_flow])
    print(f"\nstream cliente->servidor (puerto {AUTH_PORT}): {len(c2s)} B")
    print(hexdump(c2s))

    pos, login3 = find_login3(c2s)
    if login3 is None:
        # Diagnóstico: primer byte de cada payload por si el LOGIN3 no valida.
        print("no se encontró un LOGIN3 válido (88 B) en el stream cliente->servidor")
        for i, (seq, flags, payload) in enumerate(sorted(flows[server_flow], key=lambda s: s[0])):
            if payload:
                print(f"    seg {i}: seq={seq} len={len(payload)} head={payload[:16].hex()}")
        raise SystemExit(1)

    with open(out_path, "wb") as f:
        f.write(login3)
    md5 = hashlib.md5(login3).hexdigest()
    print(f"\nLOGIN3 extraído: offset {pos} en el stream, {len(login3)} B -> {out_path}")
    print(f"md5: {md5}")
    print(hexdump(login3))

    # Resumen servidor->cliente (evidencia de la respuesta del auth).
    server_side = []
    for key, segs in flows.items():
        if key == server_flow:
            continue
        if key[1] == AUTH_PORT:  # flujo con sport = auth: respuestas del servidor
            server_side.extend(segs)
    if server_side:
        s2c = reassemble(server_side)
        print(f"\nstream servidor->cliente: {len(s2c)} B (primeros 64 B):")
        print(hexdump(s2c[:64]))


if __name__ == "__main__":
    main()
