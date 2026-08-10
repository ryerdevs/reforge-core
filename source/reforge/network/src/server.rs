//! `serve` — accept loop con una tarea por conexión (F1.2).
//!
//! Sustituye a `fdwatch` + `socket_accept` de libthecore. Errores de `accept`
//! transitorios se registran y se reintentan con backoff de 100 ms (sin
//! busy-loop); el bucle termina cuando la tarea se aborta.

use std::future::Future;
use std::time::Duration;

use tokio::net::{TcpListener, TcpStream};

use crate::connection::Connection;

/// Acepta conexiones en `listener` y lanza una tarea por conexión.
///
/// `handler` recibe la [`Connection`] ya envuelta. Cuando el handler retorna,
/// la conexión se cierra (drop del stream) — paridad con `PHASE_CLOSE`
/// (`input.cpp:83`). El bucle corre hasta que la tarea se aborta.
pub async fn serve<F, Fut>(listener: TcpListener, handler: F)
where
    F: Fn(Connection<TcpStream>) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    loop {
        match listener.accept().await {
            Ok((stream, _peer)) => {
                let conn = Connection::new(stream);
                // el handler se clona por conexión (F: Clone); el propio
                // handler decide si quiere estado compartido (Arc, canal, ...)
                let handler = handler.clone();
                tokio::spawn(async move { handler(conn).await });
            }
            Err(e) => {
                // Los errores de accept son transitorios en general; log +
                // backoff para no girar en un bucle apretado.
                eprintln!("net: accept error: {e}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framer::{ConnectionRole, Framer};
    use protocol::TPacketCGLogin3;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::mpsc;

    /// F1.2: cliente TCP raw conecta, envía bytes, recibe respuesta y cierre
    /// limpio — sin panic, sin floods (el handler responde una sola vez).
    #[tokio::test]
    async fn accept_loop_echo_and_clean_close() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (tx, mut rx) = mpsc::channel::<usize>(4);
        let server = tokio::spawn(async move {
            serve(listener, move |mut conn: Connection<TcpStream>| {
                let tx = tx.clone();
                async move {
                    let mut framer = Framer::new(ConnectionRole::Channel);
                    match framer.next_packet(&mut conn).await {
                        Ok(pkt) => {
                            // respuesta: eco del paquete (una sola vez)
                            let _ = conn.send(&pkt).await;
                            let _ = tx.send(pkt.len()).await;
                        }
                        Err(e) => {
                            let _ = tx.send(0).await;
                            eprintln!("net: handler error: {e}");
                        }
                    }
                    // al retornar, conn se dropea → cierre limpio del socket
                }
            })
            .await;
        });

        // cliente raw
        let mut client = TcpStream::connect(addr).await.unwrap();
        let payload =
            TPacketCGLogin3::new_channel("test", "1234", [1, 2, 3, 4]).to_bytes_channel().to_vec();
        client.write_all(&payload).await.unwrap();

        // respuesta completa (eco)
        let mut echo = vec![0u8; payload.len()];
        client.read_exact(&mut echo).await.unwrap();
        assert_eq!(echo, payload);

        // cierre limpio: el handler retornó → el servidor cerró el socket → EOF
        let mut b = [0u8; 1];
        assert_eq!(client.read(&mut b).await.unwrap(), 0);

        // el handler vio exactamente un paquete (sin duplicados ni floods)
        assert_eq!(rx.recv().await, Some(payload.len()));
        assert!(rx.try_recv().is_err());

        server.abort();
    }

    /// F1.3 end-to-end: header desconocido → el handler ve el error y la
    /// conexión se cierra limpiamente (paridad `input.cpp:77-84`).
    #[tokio::test]
    async fn unknown_header_closes_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (tx, mut rx) = mpsc::channel::<bool>(4);
        let server = tokio::spawn(async move {
            serve(listener, move |mut conn: Connection<TcpStream>| {
                let tx = tx.clone();
                async move {
                    let mut framer = Framer::new(ConnectionRole::Channel);
                    let res = framer.next_packet(&mut conn).await;
                    let _ = tx.send(res.is_err()).await;
                }
            })
            .await;
        });

        // cliente raw enviando garbage
        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(&[0x99, 0x01, 0x02]).await.unwrap();

        // el handler recibió el error de header desconocido
        assert!(rx.recv().await.unwrap());
        // y el servidor cerró la conexión → el cliente ve EOF (cierre limpio)
        let mut b = [0u8; 1];
        assert_eq!(client.read(&mut b).await.unwrap(), 0);

        server.abort();
    }
}
