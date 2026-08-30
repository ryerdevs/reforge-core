//! `network` — transporte tokio del servidor Rust (Fase 1: F1.1–F1.4).
//!
//! Sustituye a `libthecore` + fdwatch con paridad de comportamiento observable:
//!
//! - [`connection`] — par read/write de un socket con la semántica del contrato
//!   legacy (`result > 0` / `0` EAGAIN / `-1` error — fixes #1/#2/#6 de
//!   AGENTS.md). Equivalencia documentada con `socket_write` en `Connection::send`.
//! - [`server`] — `TcpListener` + accept loop + tarea por conexión (F1.2).
//! - [`framer`] — framing sin prefijo de longitud (spec §2): header BYTE +
//!   payload fijo por tabla. Tabla cliente→servidor configurable por rol
//!   auth/canal (F1.3); keepalives (0xfc time sync, 0xfe pong) emitidos como
//!   tal sin romper el parseo del flujo (F1.4, erratas spec §7).
//! - [`handshake`] — `GC_PHASE` + `GC_HANDSHAKE` / eco `CG_HANDSHAKE` con
//!   retries de bias de reloj (F1.5, parity `desc.cpp`).
//! - [`auth`] — lógica del modo auth (F2): esqueleto.

pub mod auth;
pub mod connection;
pub mod framer;
pub mod handshake;
pub mod server;

pub use connection::Connection;
pub use framer::{
    ConnectionRole, Framer, FramingError, packet_range, packet_size, read_exact_size,
};
pub use handshake::{
    CLOCK_BIAS_TOLERANCE_MS, HANDSHAKE_ATTEMPT_TIMEOUT, HANDSHAKE_RETRY_DELAY,
    HANDSHAKE_RETRY_LIMIT, Handshake, HandshakeConfig, HandshakeError, perform, perform_with,
};
pub use server::serve;
