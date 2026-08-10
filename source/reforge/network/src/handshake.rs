//! F1.5 — Handshake de conexión (`GC_PHASE` + `GC_HANDSHAKE` / eco `CG_HANDSHAKE`).
//!
//! # Por qué el handshake NO se elimina
//!
//! (Criterio del plan F1.5: *"no se elimina: pasa una vez en login, beneficio
//! nulo, riesgo alto"*.)
//!
//! - El cliente legacy lo **EXIGE**: `CAccountConnector` espera
//!   `GC_PHASE(PHASE_HANDSHAKE)` + `GC_HANDSHAKE` antes de enviar `CG_LOGIN3`
//!   (`AccountConnector.cpp:77-97`); eliminarlo rompe el login de todos los
//!   clientes desplegados.
//! - Pasa **una vez por conexión** y cuesta ~un RTT: el beneficio de quitarlo
//!   es nulo.
//! - Además ajusta el reloj del cliente (`ELTimer_SetServerMSec`) y es la base
//!   del time-sync posterior (F1.4): el riesgo de tocarlo es alto.
//!
//! # Semántica (parity `desc.cpp:664-740` / `input.cpp:175-203` / `input_auth.cpp`)
//!
//! Por intento (hasta [`HANDSHAKE_RETRY_LIMIT`]):
//!
//! 1. S→C: `GC_PHASE` (0xfd, `phase::HANDSHAKE`) + `GC_HANDSHAKE` (0xff, 13 B)
//!    con el nonce propio y el reloj del servidor (`now_ms`, `l_delta = 0`).
//! 2. C→S: el cliente responde el eco `CG_HANDSHAKE` (13 B, mismo layout) con
//!    su reloj (el cliente legacy alinea primero su reloj con el `dwTime` del
//!    servidor: `ELTimer_SetServerMSec`).
//! 3. El servidor valida: (a) el nonce del eco == nonce enviado; (b) `l_delta`
//!    del eco >= 0 (parity `desc.cpp:693-697`, "value error"); (c) el bias de
//!    reloj `now - (dwTime + lDelta)` dentro de tolerancia. Si todo vale →
//!    [`Handshake`] con el bias como `delta`.
//!
//! Mientras espera el eco se **filtran** los keepalives `CG_TIME_SYNC` (0xfc)
//! y `CG_PONG` (0xfe) (F1.4), y se **descartan** los paquetes conocidos fuera
//! de orden (parity `input.cpp:625-626`: el C++ loguea sys_err y sigue; el
//! flujo sigue acotado por el timeout por intento).
//!
//! Si el eco no llega (timeout), el nonce no coincide o el bias cae fuera de
//! tolerancia → retry con [`HANDSHAKE_RETRY_DELAY`] de respiro. Tras
//! [`HANDSHAKE_RETRY_LIMIT`] intentos → [`HandshakeError::RetriesExhausted`].
//!
//! # Elección de constantes
//!
//! - [`HANDSHAKE_RETRY_LIMIT`] = **32** — `desc.h:17` (`HANDSHAKE_RETRY_LIMIT`).
//! - [`CLOCK_BIAS_TOLERANCE_MS`] = **80** — el legacy exige bias ∈ [0, 50]
//!   **unilateral** (`desc.cpp:701`); aquí se acepta |bias| ≤ 80 **simétrico**:
//!   el rango documentado del legacy es "~40-80ms" (AGENTS.md) y la
//!   unilateralidad depende de la dirección en que se mida. En la práctica el
//!   cliente legacy alinea su reloj con el `dwTime` del servidor ANTES del eco,
//!   así que el bias real es ~la latencia (< 80 ms en redes normales).
//! - [`HANDSHAKE_ATTEMPT_TIMEOUT`] = **500 ms** — el legacy NO tiene timeout
//!   por intento (fdwatch sin idle timeout, F1.3): un cliente mudo bloquea la
//!   conexión para siempre. Aquí cada intento se aborta a los 500 ms y se
//!   reintenta; 500 ms es amplio para un RTT de login y el límite de 32 acota
//!   el total (≤ ~17.6 s con el respiro).
//! - [`HANDSHAKE_RETRY_DELAY`] = **50 ms** — pequeño respiro entre intentos
//!   para no martillear a un peer momentáneamente lento.
//!
//! # Divergencias deliberadas vs el legacy (documentadas)
//!
//! - **Nonce u32, no u64:** el campo wire es `DWORD`
//!   (`TPacketGCHandshake.dw_handshake`, spec §3); el legacy usa CRC32 (u32).
//!   Se genera sin RNG externo (zero-deps): mezcla de nanos de reloj + contador
//!   atómico; nunca 0 (parity `desc_manager.cpp:141-142`). No es una medida de
//!   seguridad (sesión plaintext): es el identificador del eco.
//! - **Nonce incorrecto → retry (el legacy cierra al instante,**
//!   `input.cpp:179-183`): un eco duplicado/atrasado de un intento previo es
//!   más probable que un peer malicioso, y el límite de intentos acota el
//!   coste. (Decisión del plan F1.5.)
//! - **`GC_PHASE` se reenvía en cada intento:** el legacy solo lo envía una vez
//!   (`SetPhase`); reenviarlo es idempotente en el cliente
//!   (`AccountConnector.cpp:146-149`) y mantiene el intento autocontenido.
//! - **Sin `GC_PHASE(CLOSE)` al fallar:** el legacy lo envía vía
//!   `SetPhase(PHASE_CLOSE)`. Aquí el caller (F2) es dueño del teardown: al
//!   recibir `Err` debe cerrar la conexión (enviando `GC_PHASE(CLOSE)` si se
//!   quiere la paridad observable completa).
//! - **El legacy NO timeout por intento** (ver constantes): es una mejora de
//!   robustez, el efecto observable con clientes legítimos es idéntico.
//!
//! # Seguridad de la cancelación por timeout
//!
//! `tokio::time::timeout` cancela la espera; la cancelación solo descarta
//! reads **Pending**, que no consumen bytes del socket — los reads completos
//! ya terminaron y sus bytes están en `framer.buf`. No se pierde ni se
//! desalinea nada entre intentos.
//!
//! # `now_ms`
//!
//! Reloj del servidor en **ms desde boot** (parity `get_dword_time()`,
//! `utils.c:445`). El wire lleva `DWORD` (wrap a 2^32 ≈ 49.7 días, el mismo
//! wrap del legacy); `Handshake::server_time` devuelve el `now_ms` del caller.

use std::fmt;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use protocol::header;
use protocol::phase;
use protocol::{TPacketCGHandshake, TPacketGCHandshake, TPacketGCPhase};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::connection::Connection;
use crate::framer::Framer;
use crate::FramingError;

/// Límite de reintentos del handshake (parity `desc.h:17`, `HANDSHAKE_RETRY_LIMIT` = 32).
pub const HANDSHAKE_RETRY_LIMIT: u32 = 32;

/// Tolerancia de bias de reloj: |delta| ≤ 80 ms. El legacy exige bias ∈ [0, 50]
/// unilateral (`desc.cpp:701`); el rango documentado es "~40-80ms" — se toma 80
/// simétrico (ver doc del módulo).
pub const CLOCK_BIAS_TOLERANCE_MS: i64 = 80;

/// Timeout por intento: espera del eco `CG_HANDSHAKE` (el legacy no tiene
/// timeout por intento — ver doc del módulo).
pub const HANDSHAKE_ATTEMPT_TIMEOUT: Duration = Duration::from_millis(500);

/// Respiro entre intentos fallidos (el legacy reintenta sin pausa).
pub const HANDSHAKE_RETRY_DELAY: Duration = Duration::from_millis(50);

/// Resultado de un handshake completado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handshake {
    /// Reloj del servidor usado (el `now_ms` del caller, sin truncar).
    pub server_time: u64,
    /// `dw_time` crudo del eco del cliente (reloj del cliente, sin ajustar).
    pub client_time: u64,
    /// Bias de reloj `server_time - client_time` (i32 firmado con wrap 2^32,
    /// parity `desc.cpp:699`): ~la latencia en redes normales.
    pub delta: i64,
}

/// Error del handshake. Todos los errores son terminales salvo
/// [`HandshakeError::RetriesExhausted`] (que agota los reintentos): el caller
/// DEBE cerrar la conexión (el teardown es del caller, ver doc del módulo).
#[derive(Debug)]
pub enum HandshakeError {
    /// El eco no se validó en [`HANDSHAKE_RETRY_LIMIT`] intentos
    /// (timeout, nonce incorrecto, `l_delta` negativo o bias fuera de
    /// tolerancia en todos ellos).
    RetriesExhausted { attempts: u32 },
    /// Error de I/O al enviar `GC_PHASE`/`GC_HANDSHAKE`.
    Io(io::Error),
    /// Error de framing al leer el eco (header desconocido, EOF, ...).
    Framing(FramingError),
}

impl fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HandshakeError::RetriesExhausted { attempts } => write!(
                f,
                "handshake failed after {attempts} attempts (GC_PHASE + GC_HANDSHAKE echo \
                 never validated; parity HANDSHAKE_RETRY_LIMIT desc.h:17)"
            ),
            HandshakeError::Io(e) => write!(f, "handshake io error: {e}"),
            HandshakeError::Framing(e) => write!(f, "handshake framing error: {e}"),
        }
    }
}

impl std::error::Error for HandshakeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            HandshakeError::Io(e) => Some(e),
            HandshakeError::Framing(e) => Some(e),
            HandshakeError::RetriesExhausted { .. } => None,
        }
    }
}

impl From<FramingError> for HandshakeError {
    fn from(e: FramingError) -> Self {
        HandshakeError::Framing(e)
    }
}

/// Configuración del handshake. Los valores por defecto son las constantes del
/// módulo; se inyecta en tests para reducir límites/timeouts y mantenerlos
/// deterministas y rápidos.
#[derive(Debug, Clone, Copy)]
pub struct HandshakeConfig {
    /// Número máximo de intentos (default [`HANDSHAKE_RETRY_LIMIT`]).
    pub retry_limit: u32,
    /// Timeout de espera del eco por intento (default [`HANDSHAKE_ATTEMPT_TIMEOUT`]).
    pub attempt_timeout: Duration,
    /// Respiro entre intentos (default [`HANDSHAKE_RETRY_DELAY`]).
    pub retry_delay: Duration,
    /// Tolerancia de bias (default [`CLOCK_BIAS_TOLERANCE_MS`]).
    pub bias_tolerance_ms: i64,
}

impl Default for HandshakeConfig {
    fn default() -> Self {
        Self {
            retry_limit: HANDSHAKE_RETRY_LIMIT,
            attempt_timeout: HANDSHAKE_ATTEMPT_TIMEOUT,
            retry_delay: HANDSHAKE_RETRY_DELAY,
            bias_tolerance_ms: CLOCK_BIAS_TOLERANCE_MS,
        }
    }
}

/// Ejecuta el handshake completo con la configuración por defecto.
///
/// `now_ms` es el reloj del servidor en ms (ver doc del módulo). El `framer`
/// debe estar construido con el rol de la conexión (auth o canal — el
/// handshake es idéntico en ambos).
pub async fn perform<S: AsyncRead + AsyncWrite + Unpin>(
    conn: &mut Connection<S>,
    framer: &mut Framer,
    now_ms: u64,
) -> Result<Handshake, HandshakeError> {
    perform_with(conn, framer, now_ms, &HandshakeConfig::default()).await
}

/// [`perform`] con configuración inyectada (tests, tuning).
pub async fn perform_with<S: AsyncRead + AsyncWrite + Unpin>(
    conn: &mut Connection<S>,
    framer: &mut Framer,
    now_ms: u64,
    cfg: &HandshakeConfig,
) -> Result<Handshake, HandshakeError> {
    let nonce = generate_nonce();
    // Wire: DWORD con wrap a 2^32 (parity get_dword_time, utils.c:445).
    let now32 = now_ms as u32;
    let phase = TPacketGCPhase::new(phase::HANDSHAKE).to_bytes();

    let mut attempt: u32 = 0;
    loop {
        attempt += 1;

        // 1. S→C: GC_PHASE(HANDSHAKE) + GC_HANDSHAKE(nonce, now32, 0).
        //    Reenviar GC_PHASE por intento es idempotente en el cliente
        //    (AccountConnector.cpp:146-149) y mantiene el intento autocontenido.
        conn.send(&phase).await.map_err(HandshakeError::Io)?;
        conn.send(&TPacketGCHandshake::new(nonce, now32, 0).to_bytes())
            .await
            .map_err(HandshakeError::Io)?;

        // 2. Esperar el eco (filtrando keepalives), con timeout por intento.
        //    La cancelación por timeout no pierde bytes: solo descarta reads
        //    Pending (ver doc del módulo).
        let echo =
            match tokio::time::timeout(cfg.attempt_timeout, wait_for_echo(conn, framer)).await {
                Err(_) => None,      // eco no llegó a tiempo → retry
                Ok(Err(e)) => return Err(e), // terminal: io / framing
                Ok(Ok(echo)) => Some(echo),
            };

        // 3. Validar el eco.
        match echo {
            None => {} // timeout → retry
            Some(echo) => {
                if echo.dw_handshake != nonce {
                    // nonce del eco != nonce enviado → retry (divergencia
                    // deliberada: el legacy cierra, input.cpp:179-183)
                } else if echo.l_delta < 0 {
                    // parity desc.cpp:693-697 ("value error") → retry
                } else {
                    // bias = now - (dwTime + lDelta), aritmética DWORD con wrap
                    // y cast a i32 (desc.cpp:699). Con un cliente legacy el
                    // eco ya viene alineado al reloj del servidor → bias ≈ RTT.
                    let client_total = echo.dw_time.wrapping_add(echo.l_delta as u32);
                    let bias = now32.wrapping_sub(client_total) as i32;
                    if bias.unsigned_abs() <= cfg.bias_tolerance_ms as u32 {
                        return Ok(Handshake {
                            server_time: now_ms,
                            client_time: echo.dw_time as u64,
                            delta: bias as i64,
                        });
                    }
                    // bias fuera de tolerancia → retry
                }
            }
        }

        if attempt >= cfg.retry_limit {
            return Err(HandshakeError::RetriesExhausted { attempts: attempt });
        }
        tokio::time::sleep(cfg.retry_delay).await;
    }
}

/// Espera el eco `CG_HANDSHAKE` filtrando keepalives (0xfc/0xfe, F1.4) y
/// descartando paquetes conocidos fuera de orden (parity `input.cpp:625-626`:
/// el C++ loguea sys_err y sigue esperando). El flujo sigue acotado por el
/// timeout por intento del caller (un flood de paquetes no lo alarga).
async fn wait_for_echo<S: AsyncRead + Unpin>(
    conn: &mut Connection<S>,
    framer: &mut Framer,
) -> Result<TPacketCGHandshake, HandshakeError> {
    loop {
        let pkt = framer.next_packet(conn).await?; // FramingError → terminal
        match pkt[0] {
            header::CG_TIME_SYNC | header::CG_PONG => continue, // keepalives
            header::CG_HANDSHAKE => {
                return Ok(match TPacketCGHandshake::from_bytes(&pkt) {
                    Ok(echo) => echo,
                    // El framer solo entrega paquetes de exactamente
                    // TPacketCGHandshake::SIZE para 0xff (framer.rs) →
                    // BadLength es imposible aquí.
                    Err(_) => unreachable!(
                        "framer guarantees {} bytes for CG_HANDSHAKE (0xff)",
                        TPacketCGHandshake::SIZE
                    ),
                });
            }
            // Paquete conocido fuera de orden durante el handshake → se
            // descarta y se sigue esperando (parity input.cpp:625-626).
            _ => continue,
        }
    }
}

/// Genera el nonce del handshake (u32 — el campo wire es `DWORD`).
///
/// Zero-deps (filosofía del workspace): mezcla de nanos de reloj + contador
/// atómico por proceso. Nunca devuelve 0 (parity `desc_manager.cpp:141-142`).
/// No es una medida de seguridad (sesión plaintext): es el identificador del
/// eco.
fn generate_nonce() -> u32 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    // splitmix64 multiplier: baraja el contador para que nonces consecutivos
    // no compartan bits bajos.
    let mixed = nanos ^ counter.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let nonce = mixed as u32;
    if nonce == 0 { 1 } else { nonce }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::TPacketCGLogin3;
    use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt, DuplexStream};

    use crate::ConnectionRole;

    const NOW: u64 = 1_000_000;

    /// Config pequeña para tests: límites y timeouts cortos → deterministas y
    /// rápidos (el retry rápido no necesita esperar 500 ms por intento).
    fn test_cfg() -> HandshakeConfig {
        HandshakeConfig {
            retry_limit: 2,
            attempt_timeout: Duration::from_millis(100),
            retry_delay: Duration::from_millis(10),
            bias_tolerance_ms: CLOCK_BIAS_TOLERANCE_MS,
        }
    }

    /// Lado servidor del handshake en una tarea: `Connection` + `Framer` con
    /// rol auth (el handshake es idéntico en ambos roles).
    fn spawn_server(
        cfg: HandshakeConfig,
        now_ms: u64,
        stream: DuplexStream,
    ) -> tokio::task::JoinHandle<Result<Handshake, HandshakeError>> {
        tokio::spawn(async move {
            let mut conn = Connection::new(stream);
            let mut framer = Framer::new(ConnectionRole::Auth);
            perform_with(&mut conn, &mut framer, now_ms, &cfg).await
        })
    }

    /// Lado cliente: lee `GC_PHASE(HANDSHAKE)` + `GC_HANDSHAKE` y devuelve el
    /// paquete recibido (valida el phase).
    async fn recv_handshake(stream: &mut DuplexStream) -> TPacketGCHandshake {
        let mut phase = [0u8; TPacketGCPhase::SIZE];
        stream.read_exact(&mut phase).await.unwrap();
        assert_eq!(phase, TPacketGCPhase::new(phase::HANDSHAKE).to_bytes());
        let mut hs = [0u8; TPacketGCHandshake::SIZE];
        stream.read_exact(&mut hs).await.unwrap();
        TPacketGCHandshake::from_bytes(&hs).unwrap()
    }

    /// (a) Eco correcto → Ok con delta computado.
    #[tokio::test]
    async fn correct_echo_completes_with_delta() {
        let (server_side, mut client_side) = duplex(1024);
        let server = spawn_server(HandshakeConfig::default(), NOW, server_side);

        let hs = recv_handshake(&mut client_side).await;
        assert_ne!(hs.dw_handshake, 0); // el nonce enviado nunca es 0
        assert_eq!(hs.dw_time, NOW as u32);
        assert_eq!(hs.l_delta, 0);

        // el cliente está 20 ms por detrás del servidor → delta = +20 (≤ 80)
        let client_time = hs.dw_time - 20;
        client_side
            .write_all(&TPacketCGHandshake::new(hs.dw_handshake, client_time, 0).to_bytes())
            .await
            .unwrap();

        let h = server.await.unwrap().unwrap();
        assert_eq!(h.server_time, NOW);
        assert_eq!(h.client_time, client_time as u64);
        assert_eq!(h.delta, 20);
    }

    /// (b) Eco con nonce incorrecto → retries y error tras el límite.
    #[tokio::test]
    async fn wrong_nonce_retries_then_exhausts() {
        let (server_side, mut client_side) = duplex(1024);
        let server = spawn_server(test_cfg(), NOW, server_side);

        for _ in 0..test_cfg().retry_limit {
            let hs = recv_handshake(&mut client_side).await;
            client_side
                .write_all(
                    &TPacketCGHandshake::new(hs.dw_handshake ^ 0xFF, hs.dw_time, 0).to_bytes(),
                )
                .await
                .unwrap();
        }

        let err = server.await.unwrap().unwrap_err();
        assert!(matches!(err, HandshakeError::RetriesExhausted { attempts: 2 }));
    }

    /// (c) Cliente silencioso (timeout por intento) → error tras el límite.
    #[tokio::test]
    async fn silent_client_times_out_then_exhausts() {
        let (server_side, mut client_side) = duplex(1024);
        let server = spawn_server(test_cfg(), NOW, server_side);

        // nunca se escribe nada; solo se drena lo que el servidor envía
        for _ in 0..test_cfg().retry_limit {
            recv_handshake(&mut client_side).await;
        }

        let err = server.await.unwrap().unwrap_err();
        assert!(matches!(err, HandshakeError::RetriesExhausted { attempts: 2 }));
    }

    /// (d) Keepalives (0xfc time sync, 0xfe pong) intercalados antes del eco →
    /// se filtran (F1.4) y el handshake completa.
    #[tokio::test]
    async fn keepalives_are_filtered_while_waiting_for_echo() {
        let (server_side, mut client_side) = duplex(1024);
        let server = spawn_server(HandshakeConfig::default(), NOW, server_side);

        let hs = recv_handshake(&mut client_side).await;

        let mut timesync = [0u8; TPacketCGHandshake::SIZE];
        timesync[0] = header::CG_TIME_SYNC;
        client_side.write_all(&timesync).await.unwrap();
        client_side.write_all(&[header::CG_PONG]).await.unwrap();

        // eco correcto (5 ms por delante del servidor → delta = -5)
        let client_time = hs.dw_time + 5;
        client_side
            .write_all(&TPacketCGHandshake::new(hs.dw_handshake, client_time, 0).to_bytes())
            .await
            .unwrap();

        let h = server.await.unwrap().unwrap();
        assert_eq!(h.delta, -5);
    }

    /// (e) Bias fuera de tolerancia en todos los intentos → retries y error
    /// tras el límite.
    #[tokio::test]
    async fn bias_out_of_tolerance_retries_then_exhausts() {
        let (server_side, mut client_side) = duplex(1024);
        let server = spawn_server(test_cfg(), NOW, server_side);

        for _ in 0..test_cfg().retry_limit {
            let hs = recv_handshake(&mut client_side).await;
            // 1 s de bias (|delta| = 1000 > 80) — el reloj del cliente "corre"
            // 1 s por delante del servidor
            let client_time = hs.dw_time.wrapping_add(1_000);
            client_side
                .write_all(&TPacketCGHandshake::new(hs.dw_handshake, client_time, 0).to_bytes())
                .await
                .unwrap();
        }

        let err = server.await.unwrap().unwrap_err();
        assert!(matches!(err, HandshakeError::RetriesExhausted { attempts: 2 }));
    }

    /// (f) Un intento con bias malo no es fatal: el retry con eco correcto
    /// completa el handshake.
    #[tokio::test]
    async fn retry_recovers_after_bad_bias() {
        let (server_side, mut client_side) = duplex(1024);
        let server = spawn_server(test_cfg(), NOW, server_side);

        // intento 1: bias malo (1 s fuera)
        let hs = recv_handshake(&mut client_side).await;
        client_side
            .write_all(
                &TPacketCGHandshake::new(hs.dw_handshake, hs.dw_time.wrapping_add(1_000), 0)
                    .to_bytes(),
            )
            .await
            .unwrap();

        // intento 2: eco correcto (20 ms por detrás)
        let hs = recv_handshake(&mut client_side).await;
        let client_time = hs.dw_time - 20;
        client_side
            .write_all(&TPacketCGHandshake::new(hs.dw_handshake, client_time, 0).to_bytes())
            .await
            .unwrap();

        let h = server.await.unwrap().unwrap();
        assert_eq!(h.delta, 20);
    }

    /// (g) `l_delta` negativo en el eco → retry (parity `desc.cpp:693-697`,
    /// "value error"); un eco correcto después completa el handshake.
    #[tokio::test]
    async fn negative_l_delta_retries_then_recovers() {
        let (server_side, mut client_side) = duplex(1024);
        let server = spawn_server(test_cfg(), NOW, server_side);

        // intento 1: eco con l_delta negativo → inválido
        let hs = recv_handshake(&mut client_side).await;
        client_side
            .write_all(&TPacketCGHandshake::new(hs.dw_handshake, hs.dw_time, -5).to_bytes())
            .await
            .unwrap();

        // intento 2: eco correcto
        let hs = recv_handshake(&mut client_side).await;
        let client_time = hs.dw_time - 30;
        client_side
            .write_all(&TPacketCGHandshake::new(hs.dw_handshake, client_time, 0).to_bytes())
            .await
            .unwrap();

        let h = server.await.unwrap().unwrap();
        assert_eq!(h.delta, 30);
    }

    /// (h) El peer se va (EOF) durante el handshake → error terminal de
    /// framing, no retries.
    #[tokio::test]
    async fn peer_eof_during_handshake_is_terminal() {
        let (server_side, mut client_side) = duplex(1024);
        let server = spawn_server(HandshakeConfig::default(), NOW, server_side);

        // se drena el intento del servidor y luego el peer cierra sin responder
        // (nota: con duplex, si el peer se cae ANTES del primer envío, el write
        // del servidor falla con BrokenPipe → HandshakeError::Io; aquí se
        // ejercita el EOF en la lectura del eco)
        recv_handshake(&mut client_side).await;
        drop(client_side);

        let err = server.await.unwrap().unwrap_err();
        assert!(matches!(err, HandshakeError::Framing(FramingError::Eof)));
    }

    /// (i) Paquete conocido fuera de orden durante el handshake → se descarta
    /// (parity `input.cpp:625-626`) y el eco posterior completa el handshake.
    #[tokio::test]
    async fn unexpected_packet_is_dropped_parity_legacy() {
        let (server_side, mut client_side) = duplex(1024);
        let server = spawn_server(HandshakeConfig::default(), NOW, server_side);

        let hs = recv_handshake(&mut client_side).await;

        // un LOGIN3 antes de tiempo (rol auth → 68 B, paquete completo)
        let login3 = TPacketCGLogin3::new_auth("test", "1234", [1, 2, 3, 4], "es").to_bytes_auth();
        client_side.write_all(&login3).await.unwrap();

        // el eco correcto después sí se procesa
        let client_time = hs.dw_time - 10;
        client_side
            .write_all(&TPacketCGHandshake::new(hs.dw_handshake, client_time, 0).to_bytes())
            .await
            .unwrap();

        let h = server.await.unwrap().unwrap();
        assert_eq!(h.delta, 10);
    }

    // ------------------------------------------------------------------
    // Nonce
    // ------------------------------------------------------------------

    #[test]
    fn nonce_is_never_zero() {
        assert_ne!(generate_nonce(), 0);
    }

    #[test]
    fn nonces_differ_across_calls() {
        assert_ne!(generate_nonce(), generate_nonce());
    }
}
