//! `Connection` — encapsula el par read/write de un socket (F1.2).
//!
//! Contrato de escritura verificado en la línea base C++ (fixes #1/#2/#6 de
//! AGENTS.md): `socket_write` devuelve `result > 0` (bytes escritos, éxito
//! parcial), `0` (EAGAIN, backpressure) o `-1` (error). La contraparte en
//! tokio es `AsyncWriteExt::write_all`: consume TODO el buffer en bucle y
//! duerme la tarea mientras el socket no acepte más bytes (backpressure sin
//! busy-loop — el `0`/EAGAIN del legacy, y sin el flood de WRITE del fdwatch,
//! fix #10).
//!
//! `Connection` implementa `AsyncRead`/`AsyncWrite` por delegación: el
//! [`Framer`](crate::framer) y cualquier otro consumidor usan las traits
//! estándar de tokio sin conocer el wrapper.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};

/// Par read/write de una conexión con la semántica del contrato legacy.
pub struct Connection<S> {
    stream: S,
}

impl<S> Connection<S> {
    /// Envuelve un stream ya conectado.
    pub fn new(stream: S) -> Self {
        Self { stream }
    }

    /// Recupera el stream interno.
    pub fn into_inner(self) -> S {
        self.stream
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> Connection<S> {
    /// Envía `data` COMPLETO (bucle hasta consumir todo el buffer).
    ///
    /// Equivalencia con la semántica del contrato legacy (`socket.c` /
    /// `desc.cpp` `ProcessOutput` / `PeerBase.cpp` `Send`):
    ///
    /// | legacy `socket_write` | tokio |
    /// |---|---|
    /// | `result > 0` — se escribieron `result` bytes; el caller repetía con el resto | `write` parcial → el bucle interno de `write_all` repite hasta consumir todo el buffer |
    /// | `0` = EAGAIN — backpressure (buffer de salida del kernel lleno) | `write` devuelve `Pending` y la tarea se duerme hasta que el socket vuelva a aceptar bytes — mismo efecto, sin busy-loop |
    /// | `-1` — error de socket | `Err` → el caller cierra la conexión |
    pub async fn send(&mut self, data: &[u8]) -> io::Result<()> {
        self.write_all(data).await
    }

    /// Lee en `buf`; `Ok(0)` = el peer cerró la conexión (EOF).
    pub async fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.read(buf).await
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for Connection<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Connection<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
