//! `channel/game.rs` — el LOOP DE JUEGO de la conexión (R-s3 del refactor):
//! el `tokio::select!` (paquetes del cliente / idle / ping / eventos del
//! mundo) + el dispatch por header — cada CG_* delega en su módulo con una
//! línea (C2: los lanes futuros aterrizan sus arms sin tocar nada más).
//!
//! El HEARTBEAT es del SERVIDOR (parity `ping_event`, desc.cpp:179-214): el
//! cliente en reposo no manda nada; el canal envía GC_PING (44, 1 B) cada
//! `ping_interval_ms` y el cliente responde CG_PONG (0xfe — ya en la tabla
//! del framer), que resetea el timeout de inactividad. El ping es
//! INDEPENDIENTE del tráfico entrante (tokio::select! — se envía incluso si
//! llegan MOVE). El cierre para headers desconocidos/variables lo hace el
//! FRAMER (parity input.cpp:77-84) — este loop no lo relaja.
//!
//! El TICK del mundo (500 ms) corre en la TAREA DEL CANAL (run) — la
//! conexión solo drena los eventos de su cola (`event_rx` → `events.rs`).

use protocol::header;

use crate::channel::session::Session;
use crate::channel::{chat, combat, events, items, movement, script, skills};

/// Loop de juego de la conexión: corre SOLO con la sesión llena (las fases
/// 1-7 las hizo `entry::run`). `Err` = cierre con razón (fatal o protocolario
/// — speedhack vía `Outcome::Close`).
pub async fn run(session: &mut Session) -> Result<(), String> {
    // 8. Loop de juego: la conexión se mantiene viva.
    loop {
        let idle_deadline = session.last_packet + session.config.timeout;
        let idle = tokio::time::sleep_until(idle_deadline);
        tokio::pin!(idle);
        tokio::select! {
            pkt = session.framer.next_packet(&mut session.conn) => {
                let pkt = pkt.map_err(|e| format!("framer (game): {e}"))?;
                // Hook del harness F5: captura golden del wire recibido.
                crate::bench_capture::capture_conn(
                    session.conn_id,
                    crate::bench_capture::Direction::Inbound,
                    &pkt,
                );
                session.last_packet = tokio::time::Instant::now();
                match pkt[0] {
                    header::CG_TIME_SYNC | header::CG_PONG | header::CG_MARK_LOGIN => continue,
                    header::CG_CLIENT_VERSION2 => {
                        // El cliente puede re-mandar la versión en la fase game
                        // (parity input.cpp:205-213 — sin validación, sin respuesta).
                        let name_end = pkt[1..34].iter().position(|&b| b == 0).unwrap_or(33);
                        eprintln!(
                            "server_realms: channel conn {}: VERSION {} (game) — ignorado",
                            session.conn_id,
                            String::from_utf8_lossy(&pkt[1..1 + name_end])
                        );
                        continue;
                    }
                    header::CG_MOVE => movement::handle(session, &pkt).await?.into_result()?,
                    header::CG_ATTACK => combat::handle(session, &pkt).await?.into_result()?,
                    header::CG_USE_SKILL => skills::handle(session, &pkt).await?.into_result()?,
                    // F5.3: chat — echo GC_CHAT (4) al jugador (parity
                    // `Chat()` input_main.cpp:641-685 → `ChatPacket` →
                    // char.cpp — sin interpret_command por ahora, YAGNI).
                    header::CG_CHAT => chat::handle(session, &pkt).await?.into_result()?,
                    header::CG_ITEM_PICKUP => {
                        items::handle_pickup(session, &pkt).await?.into_result()?;
                    }
                    // F5.3: USO de items consumibles (pociones) — CG_ITEM_USE
                    // (11, 4 B: header + TItemPos — Packet.h:559-563). Parity
                    // `UseItemEx` → `UseItem` (char_item.cpp:1616+):
                    // value0 = HP flat, value1 = SP flat, value3 = HP % del
                    // máximo, value4 = SP % del máximo (del item_proto);
                    // NO consume si no hay efecto aplicable (HP/MP llenos).
                    // Al consumir: GC_POINTS (hp/mp) + count-1 → GC_ITEM_UPDATE
                    // (38 B) si queda, GC_ITEM_DEL deprecated (42 B) si agota.
                    header::CG_ITEM_USE => {
                        items::handle_use(session, &pkt).await?.into_result()?;
                    }
                    // F5.3: MOVER items del inventario — CG_ITEM_MOVE (13,
                    // 8 B: header + TItemPos origen + TItemPos destino +
                    // BYTE num — Packet.h:593-599). Parity `MoveItem`
                    // (char_item.cpp:5609-5767): stack si el destino tiene el
                    // mismo vnum + sockets iguales + count < 200; split si
                    // `0 < num < count`; si no, mover todo. Solo
                    // INVENTORY→INVENTORY (equipar/belt/DS = pendiente).
                    header::CG_ITEM_MOVE => {
                        items::handle_move(session, &pkt).await?.into_result()?;
                    }
                    // F5.3: REVIVE del jugador — CG_SCRIPT_ANSWER (29, 2 B:
                    // header + answer BYTE — Packet.h:679). El diálogo de
                    // muerte del cliente manda la respuesta; el C++ revive
                    // con `RestartAtSamePos` (cmd_general.cpp:534 — el mismo
                    // punto) o warpea a la ciudad (cmd_general.cpp:552-554 →
                    // WarpSet EMPIRE_START).
                    header::CG_SCRIPT_ANSWER => {
                        script::handle(session, &pkt).await?.into_result()?;
                    }
                    // TODO(F5 npcs): game_core::npc::... para los NPCs/mobs
                    other => {
                        eprintln!(
                            "server_realms: channel conn {}: paquete de juego 0x{other:02x} ignorado \
                             (el procesamiento — movimiento/combate — es F5)",
                            session.conn_id
                        );
                    }
                }
            }
            _ = &mut idle => {
                return Err(format!(
                    "timeout de inactividad de {} ms — sin paquetes del cliente, conexión cerrada",
                    session.config.timeout.as_millis()
                ));
            }
            _ = session.ping_timer.tick() => {
                // Heartbeat del server (parity desc.cpp:205-208): GC_PING cada
                // ping_interval_ms; el cliente responde CG_PONG (que resetea
                // `last_packet` al llegar por el brazo del recv).
                session
                    .send(&[header::GC_PING])
                    .await
                    .map_err(|e| format!("enviando GC_PING: {e}"))?;
            }
            ev = session.event_rx.recv() => {
                let Some(ev) = ev else {
                    return Err("canal de eventos del mundo cerrado".into());
                };
                // R-s4: la traducción NpcEvent → GC vive en `events.rs`.
                events::handle(session, ev).await?;
            }
        }
    }
}
