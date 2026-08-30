//! F0 golden test — LOGIN3 REAL capturado del wire (hito F0 del ROADMAP:
//! "un LOGIN3 real capturado se parsea y re-serializa byte-por-byte idéntico").
//!
//! Fixture: `tests/golden/auth_login3_40999.bin` — los 88 B exactos del
//! LOGIN3 auth extendido (header 0x6f + login[31] + passwd[17] + keys[16] +
//! lang[3] + version[4] + hwid[16]) reconstruidos del pcap real.
//!
//! Metadata de la captura (2026-08-11, WSL Debian-M2):
//! - peer: `network/examples/f16_peer.rs` (cargo build --release --example
//!   f16_peer; ELF) contra `127.0.0.1:30001` (auth Rust `server_realms`),
//!   args: `--login3 --version 40999 --hwid aabbccddeeff00112233445566778899`.
//! - captura: `scripts/gpg/capture_auth.sh` (tcpdump -i any 'port 30001' →
//!   /tmp/gpg/capture_auth.pcap; 18 paquetes; SLL2).
//! - extracción: `scripts/gpg/extract_pcap_login3.py` — reensamblado TCP del
//!   stream cliente→servidor por secuencia; LOGIN3 en offset 13 del stream
//!   (tras el eco CG_HANDSHAKE de 13 B); validado campo a campo.
//! - respuesta del auth en el mismo pcap: GC_PHASE(1) + GC_HANDSHAKE +
//!   GC_PHASE(10) + GC_AUTH_SUCCESS (key=0x6fb40f61, result=1) — login OK.
//! - md5 del fixture: 6a93aa8f102d6b11c4b2ea842dceef87.

use protocol::TPacketCGLogin3;

/// Los 88 B capturados (auth + version + hwid).
const FIXTURE: &[u8] = include_bytes!("golden/auth_login3_40999.bin");

/// HWID del peer de captura: `aabbccddeeff00112233445566778899`.
const HWID: [u8; 16] = [
    0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, //
    0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99,
];

#[test]
fn golden_fixture_is_88_bytes() {
    assert_eq!(
        FIXTURE.len(),
        TPacketCGLogin3::SIZE_AUTH_FULL,
        "88 B auth + version + hwid"
    );
}

#[test]
fn golden_login3_40999_parses() {
    let p = TPacketCGLogin3::from_bytes(FIXTURE).expect("el LOGIN3 real debe parsear");
    assert_eq!(p.header, protocol::header::CG_LOGIN3, "header 0x6f");
    assert_eq!(p.login, protocol::from_cstr::<31>("test"), "login test");
    assert_eq!(p.passwd, protocol::from_cstr::<17>("1234"), "password 1234");
    assert_eq!(
        p.adw_client_key, [0; 4],
        "keys del peer (f16_peer envía [0;4])"
    );
    assert_eq!(p.sz_language, *b"es\0", "lang es");
    assert_eq!(p.version, Some(40999), "version del cliente");
    assert_eq!(p.hwid, Some(HWID), "hwid aabbccddeeff00112233445566778899");
}

#[test]
fn golden_login3_40999_reserializes_byte_identical() {
    // El hito F0: parse → serialize → byte-por-byte idéntico al wire real.
    let p = TPacketCGLogin3::from_bytes(FIXTURE).expect("el LOGIN3 real debe parsear");
    let re = p.to_bytes_auth_with(p.version, p.hwid);
    assert_eq!(re.len(), TPacketCGLogin3::SIZE_AUTH_FULL);
    assert_eq!(
        re, FIXTURE,
        "re-serialización byte-por-byte idéntica al wire capturado"
    );
}
