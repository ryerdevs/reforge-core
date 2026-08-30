//! `--bench-capture <dir>` — captura cruda del wire por conexión (modo
//! golden-capture del benchmark F5: streams byte-exactos del canal para
//! comparar runs y congelar vectores).
//!
//! # CONTRACT para el lane del canal (channel.rs — OTRO lane; NO tocar)
//!
//! Este módulo está completo y testeado; lo único que falta es que el canal
//! llame los hooks en sus call sites. Firmas exactas:
//!
//! ```ignore
//! // 1. Al abrir la conexión (tras `let mut conn = Connection::new(stream);`):
//! server_realms::bench_capture::open_conn(conn_id);
//!
//! // 2. Por cada paquete RECIBIDO crudo del socket — inmediatamente después
//! //    de que `framer.next_packet(&mut conn)` devuelva `Ok(pkt)`:
//! server_realms::bench_capture::capture_conn(conn_id, Direction::Inbound, &pkt);
//!
//! // 3. Por cada envío crudo al socket — justo después de que
//! //    `conn.send(bytes)` devuelva `Ok`:
//! server_realms::bench_capture::capture_conn(conn_id, Direction::Outbound, bytes);
//!
//! // 4. Al cerrar la conexión (teardown del handler):
//! server_realms::bench_capture::close_conn(conn_id);
//! ```
//!
//! **TODO(channel lane):** cablear los 4 call sites de arriba. Hasta que
//! existan, `--bench-capture` solo crea el directorio; `capture_conn` es un
//! no-op (no escribe archivos sin `open_conn`).
//!
//! # Formato de archivo
//!
//! `conn_{id:06}_{in|out}.bin` por conexión y dirección: el STREAM crudo
//! (bytes concatenados, sin framing ni cabeceras) — exactamente lo que el
//! golden-capture comparará (el consumidor parsea con el crate `protocol`).
//!
//! ⚠️ I/O de archivos BLOQUEANTE dentro de tasks async: aceptable para un
//! harness (un write por paquete, archivos locales); el lane del canal debe
//! saberlo antes de cablearlo a alta frecuencia.
//!
//! # dead_code esperado
//!
//! `open_conn`/`capture_conn`/`close_conn`/`is_enabled` son el CONTRATO del
//! lane del canal — no hay call sites todavía (TODO arriba). Hasta que el
//! lane los cablee, el módulo emite dead_code: documentado, no suprimido
//! silenciosamente.
#![allow(dead_code)]

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Dirección del tráfico capturado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Inbound,
    Outbound,
}

impl Direction {
    fn tag(&self) -> &'static str {
        match self {
            Direction::Inbound => "in",
            Direction::Outbound => "out",
        }
    }
}

struct ConnFiles {
    inbound: File,
    outbound: File,
}

struct CaptureState {
    dir: PathBuf,
    conns: Mutex<HashMap<u32, ConnFiles>>,
}

/// `None` = captura desactivada (no-op total). `Some(state)` = activa.
/// `Mutex` (no `OnceLock`): re-`init` SWAPEA el estado (tests aislados y
/// reinicio limpio; en el binario `init` se llama una vez, antes del listener).
static STATE: Mutex<Option<CaptureState>> = Mutex::new(None);

/// Activa la captura: crea `<dir>` y queda lista para `open_conn`. Re-`init`
/// reemplaza el estado anterior (los archivos abiertos se descartan). Sin la
/// flag, el módulo es un no-op de costo cero.
pub fn init(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    *STATE.lock().unwrap() = Some(CaptureState {
        dir: dir.to_path_buf(),
        conns: Mutex::new(HashMap::new()),
    });
    Ok(())
}

pub fn is_enabled() -> bool {
    STATE.lock().unwrap().is_some()
}

fn conn_path(state: &CaptureState, conn_id: u32, dir: Direction) -> PathBuf {
    state
        .dir
        .join(format!("conn_{conn_id:06}_{}.bin", dir.tag()))
}

/// Registra una conexión: crea `conn_{id}_{in|out}.bin` (append). No-op si la
/// captura no está activa.
pub fn open_conn(conn_id: u32) {
    let guard = STATE.lock().unwrap();
    let Some(st) = guard.as_ref() else { return };
    let open =
        |p: &Path| -> Option<File> { OpenOptions::new().create(true).append(true).open(p).ok() };
    let Some(inbound) = open(&conn_path(st, conn_id, Direction::Inbound)) else {
        return;
    };
    let Some(outbound) = open(&conn_path(st, conn_id, Direction::Outbound)) else {
        return;
    };
    st.conns
        .lock()
        .unwrap()
        .insert(conn_id, ConnFiles { inbound, outbound });
}

/// Captura `data` crudo de la conexión `conn_id` en la dirección indicada
/// (append al archivo). No-op sin `open_conn` previa o si la captura está
/// desactivada.
pub fn capture_conn(conn_id: u32, direction: Direction, data: &[u8]) {
    if data.is_empty() {
        return;
    }
    let guard = STATE.lock().unwrap();
    let Some(st) = guard.as_ref() else { return };
    let mut conns = st.conns.lock().unwrap();
    let Some(files) = conns.get_mut(&conn_id) else {
        return;
    };
    let f = match direction {
        Direction::Inbound => &mut files.inbound,
        Direction::Outbound => &mut files.outbound,
    };
    let _ = f.write_all(data);
    let _ = f.flush();
}

/// Cierra la conexión: flush + libera los archivos (la entrada del mapa se
/// elimina; la conexión siguiente con el mismo id empieza limpia).
pub fn close_conn(conn_id: u32) {
    let guard = STATE.lock().unwrap();
    let Some(st) = guard.as_ref() else { return };
    let mut conns = st.conns.lock().unwrap();
    if let Some(mut files) = conns.remove(&conn_id) {
        let _ = files.inbound.flush();
        let _ = files.outbound.flush();
    }
}

/// Registra las métricas del MUNDO por tick (harness F5 — el canal la llama
/// tras cada `WorldSim::update`): una línea CSV por tick en `tick_ms.csv` del
/// dir de captura (`ticks,intents_processed,mobs_spawned,mobs_despawned,
/// events_emitted,tick_ms`) — el run de bench registra el tick_ms por tick
/// (timing de sistemas) con los contadores del mundo. No-op sin
/// `--bench-capture`.
pub fn record_metrics(m: game_core::ecs::WorldMetrics) {
    let guard = STATE.lock().unwrap();
    let Some(st) = guard.as_ref() else { return };
    let path = st.dir.join("tick_ms.csv");
    let header = !path.exists();
    let mut f = match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(f) => f,
        Err(_) => return, // harness: un fallo de archivo no toca el runtime
    };
    if header {
        let _ = writeln!(
            f,
            "ticks,intents_processed,mobs_spawned,mobs_despawned,events_emitted,tick_ms"
        );
    }
    let _ = writeln!(
        f,
        "{},{},{},{},{},{}",
        m.ticks,
        m.intents_processed,
        m.mobs_spawned,
        m.mobs_despawned,
        m.events_emitted,
        m.last_tick_ms
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "bench_capture_test_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// Lectura con reintentos: en Windows el flush del stream recién cerrado
    /// puede tardar unos ms (flake 2026-08-16 en runs del workspace en
    /// paralelo) — reintentar hasta ~2 s antes de fallar.
    fn read_retry(path: &Path) -> Vec<u8> {
        for _ in 0..40 {
            match std::fs::read(path) {
                Ok(bytes) => return bytes,
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            }
        }
        std::fs::read(path).unwrap_or_default()
    }

    /// Un único test serial del ciclo de vida completo: el estado es GLOBAL
    /// (static Mutex) — tests paralelos del mismo módulo se pisarían. Las
    /// lecturas post-close usan `read_retry` (flake 2026-08-16 resuelto).
    #[test]
    fn capture_lifecycle_disabled_init_write_reopen_noop() {
        // 1. Sin init: no-op total.
        assert!(!is_enabled());
        open_conn(1);
        capture_conn(1, Direction::Inbound, b"abc");
        close_conn(1);

        // 2. init + captura: streams crudos por conexión y dirección.
        let dir = tmp_dir("write");
        init(&dir).unwrap();
        assert!(is_enabled());

        open_conn(7);
        capture_conn(7, Direction::Inbound, b"\xfd\x01");
        capture_conn(7, Direction::Inbound, b"\xff\x01\x02");
        capture_conn(7, Direction::Outbound, b"\xff\x0a");
        close_conn(7);

        let in_path = dir.join("conn_000007_in.bin");
        let out_path = dir.join("conn_000007_out.bin");
        assert_eq!(
            read_retry(&in_path),
            b"\xfd\x01\xff\x01\x02",
            "stream crudo IN"
        );
        assert_eq!(
            read_retry(&out_path),
            b"\xff\x0a",
            "stream crudo OUT"
        );

        // 3. Un id sin open_conn no escribe nada.
        capture_conn(8, Direction::Inbound, b"x");
        assert!(!dir.join("conn_000008_in.bin").exists());

        // 4. Re-abrir la misma id → append, no truncado; captura tras close
        //    es no-op.
        capture_conn(3, Direction::Inbound, b"early"); // no-op sin open
        open_conn(3);
        capture_conn(3, Direction::Inbound, b"a");
        close_conn(3);
        capture_conn(3, Direction::Inbound, b"late"); // no-op tras close
        let p3 = dir.join("conn_000003_in.bin");
        assert_eq!(read_retry(&p3), b"a");
        open_conn(3);
        capture_conn(3, Direction::Inbound, b"b");
        close_conn(3);
        assert_eq!(
            read_retry(&p3),
            b"ab",
            "append sobre la misma conexión"
        );

        // 5. Re-init (swap de estado) apunta a un dir nuevo.
        let dir2 = tmp_dir("swap");
        init(&dir2).unwrap();
        assert!(
            dir.join("conn_000007_in.bin").exists(),
            "los archivos viejos quedan"
        );
        open_conn(9);
        capture_conn(9, Direction::Outbound, b"z");
        close_conn(9);
        assert_eq!(
            std::fs::read(dir2.join("conn_000009_out.bin")).unwrap(),
            b"z"
        );
        assert_eq!(
            std::fs::read(dir2.join("conn_000009_out.bin")).unwrap(),
            b"z"
        );

        // 6. record_metrics (harness F5): CSV con header + una linea por tick
        //    (las metricas del mundo, con el tick_ms del ultimo update). Vive
        //    AQUI (test serial): el STATE es global - un test paralelo lo
        //    pisaria.
        let m = game_core::ecs::WorldMetrics {
            ticks: 3,
            intents_processed: 7,
            mobs_spawned: 11,
            mobs_despawned: 2,
            events_emitted: 5,
            last_tick_ms: 42,
        };
        record_metrics(m);
        record_metrics(m);
        let csv = std::fs::read_to_string(dir2.join("tick_ms.csv")).unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 3, "header + 2 ticks");
        assert_eq!(
            lines[0],
            "ticks,intents_processed,mobs_spawned,mobs_despawned,events_emitted,tick_ms"
        );
        assert_eq!(
            lines[1], "3,7,11,2,5,42",
            "linea por tick (tick_ms del ultimo update)"
        );
        assert_eq!(lines[2], "3,7,11,2,5,42");

        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&dir2).unwrap();
    }
}
