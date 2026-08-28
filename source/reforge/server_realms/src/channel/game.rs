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

use game_core::ecs::{CombatIntent, Intent};

use crate::channel::session::Session;
use crate::channel::{chat, combat, dragon_soul, events, horse, items, land, locale, movement, party, pvp, quest, quickslot, safebox, script, shop, skills, trade};

/// Loop de juego de la conexión: corre SOLO con la sesión llena (las fases
/// 1-7 las hizo `entry::run`). `Err` = cierre con razón (fatal o protocolario
/// — speedhack vía `Outcome::Close`).
pub async fn run(session: &mut Session) -> Result<(), String> {
    // Guard de sesión LLENA (fix 2026-08-14): el entry retorna Ok(()) para
    // los cierres limpios del protocolo (login fallido — GC_LOGIN_FAILURE,
    // guild mark, slot vacío) — sin fila NO hay game loop (el cliente ya
    // recibió su respuesta o nada; los handlers usarían row()/store() con
    // expect y el cierre por EOF panickearía en el save).
    if session.row.is_none() {
        return Ok(());
    }
    // Save al CIERRE de conexión (fix 2026-08-13): la posición del jugador
    // se persiste ANTES de soltar la sesión — el LeaveGuard (Drop, sync y
    // sin la sesión) no puede. En TODOS los caminos (Ok y Err — cierre por
    // timeout/speedhack/EOF): el save es fire-and-forget → Batcher del canal
    // (100 ms, WAL) — el batch se aplica aunque la conexión ya se cerró.
    let result = game_loop(session).await;
    // Safebox al CIERRE de conexión (parity `CHARACTER::Destroy` →
    // `CloseSafebox`, char.cpp:1352): persiste el oro (los items ya se
    // persistieron en cada mutación) y suelta el estado. Errores de PG/
    // socket → log interno, no fatal (el cierre no debe colgarse).
    let _ = safebox::close(session).await;
    session.save();
    result
}

/// El loop de juego puro (extraído para el save al cierre en `run`).
async fn game_loop(session: &mut Session) -> Result<(), String> {
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
                    // F5.3+: TARGET de mob — CG_TARGET (61, 5 B: header + vid
                    // — Packet.h:671-675). Parity `Target` (input_main.cpp:
                    // 1918-1935 → SetTarget, char.cpp:5048-5094): el mundo
                    // responde GC_TARGET (63) con el HP% del mob apuntado —
                    // la barra de vida del objetivo (fix bug 5, 2026-08-15;
                    // antes: header sin arm en el dispatch — sin barra).
                    header::CG_TARGET => {
                        let vid = u32::from_le_bytes([pkt[1], pkt[2], pkt[3], pkt[4]]);
                        // Lote 3 (GM): el `/kill` usa el último target
                        // (parity m_dwTargetVID del CHARACTER).
                        session.target_vid = Some(vid);
                        session.intent(Intent::Combat(CombatIntent::Target {
                            player_vid: session.player_vid(),
                            target_vid: vid,
                        }))?;
                    }
                    header::CG_USE_SKILL => skills::handle(session, &pkt).await?.into_result()?,
                    // FASE 1 caballo jugable: CG_HORSE (63, aditivo reforge —
                    // 2 B: header + bRide) monta/desmonta + persiste.
                    header::CG_HORSE => horse::handle(session, &pkt).await?.into_result()?,
                    // F1 pull del locale (ADR-0009 — hot reload): el cliente
                    // re-pide la lengua EN GAME; stateless (parity auth).
                    header::CG_LOCALE_REQUEST => locale::handle(session, &pkt).await?.into_result()?,
                    // F5.3: chat — echo GC_CHAT (4) al jugador (parity
                    // `Chat()` input_main.cpp:641-685 → `ChatPacket` →
                    // char.cpp — sin interpret_command por ahora, YAGNI).
                    header::CG_CHAT => chat::handle(session, &pkt).await?.into_result()?,
                    // F6 social (gap-lane-C): whisper — CG_WHISPER (19,
                    // variable: header + wSize + szNameTo[25] + msg — el
                    // framer ya lo entrega completo; parity `Whisper()`
                    // input_main.cpp:273-487 — destino por nombre, confirmación
                    // al emisor.
                    header::CG_WHISPER => {
                        chat::handle_whisper(session, &pkt).await?.into_result()?;
                    }
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
                    // Lane R: REFINE — CG_ITEM_USE_TO_ITEM (60, 7 B:
                    // header + TItemPos Cell + TItemPos TargetCell —
                    // Packet.h:549-554). Parity `ItemToItem` → `UseItemEx`
                    // → `RefineItem` (char_item.cpp:1316): el scroll
                    // (USE_TUNING) sobre el item destino abre la ventana de
                    // refine (GC_REFINE_INFORMATION 119) y guarda el modo.
                    header::CG_ITEM_USE_TO_ITEM => {
                        items::handle_use_to_item(session, &pkt).await?.into_result()?;
                    }
                    // Lane R: CONFIRMAR el refine — CG_REFINE (96, 3 B:
                    // header + pos BYTE + type BYTE — Packet.h:976-982).
                    // Parity `CInputMain::Refine` (input_main.cpp:2831):
                    // NORMAL (0) → DoRefine (tabla refine_proto — fee ×5,
                    // materiales, prob; FAIL destruye); SCROLL (2) →
                    // DoRefineWithScroll (consume el scroll del modo, FAIL
                    // baja de nivel); 255 → cancelar. Respuesta:
                    // GC_ITEM_DEL+GC_ITEM_SET / GC_ITEM_DEL + GC_POINTS.
                    header::CG_REFINE => {
                        items::handle_refine(session, &pkt).await?.into_result()?;
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
                    // Lane D (gap loop 2026-08-15): SOLTAR item/oro del
                    // inventario al suelo — CG_ITEM_DROP (12, 8 B: header +
                    // TItemPos + gold) y CG_ITEM_DROP2 (20, 9 B: + count).
                    // Parity `ItemDrop`/`ItemDrop2` (input_main.cpp:855-890):
                    // gold > 0 → DropGold (item vnum 1 en el suelo + resta
                    // del monedero); si no → DropItem (quita del inventario +
                    // SyncQuickslot de la celda). El mundo responde el
                    // GC_ITEM_GROUND_ADD (26) + GC_ITEM_OWNERSHIP (31) vía
                    // el `DropResult` (events.rs — el vid lo asigna el mundo).
                    header::CG_ITEM_DROP => {
                        items::handle_drop(session, &pkt).await?.into_result()?;
                    }
                    header::CG_ITEM_DROP2 => {
                        items::handle_drop2(session, &pkt).await?.into_result()?;
                    }
                    // Lane D: BARRA RÁPIDA — CG_QUICKSLOT_ADD (16, 4 B),
                    // CG_QUICKSLOT_DEL (17, 2 B) y CG_QUICKSLOT_SWAP (18,
                    // 3 B). Parity `QuickslotAdd/Delete/Swap`
                    // (input_main.cpp:908-934): validan, mutan el bytea
                    // `player.quickslot` (36 × TQuickslot — 72 B), lo
                    // PERSISTEN y responden GC_QUICKSLOT_ADD (28) / DEL
                    // (29) / SWAP (30) — el cliente pinta la barra.
                    header::CG_QUICKSLOT_ADD => {
                        quickslot::handle_add(session, &pkt).await?.into_result()?;
                    }
                    header::CG_QUICKSLOT_DEL => {
                        quickslot::handle_del(session, &pkt).await?.into_result()?;
                    }
                    header::CG_QUICKSLOT_SWAP => {
                        quickslot::handle_swap(session, &pkt).await?.into_result()?;
                    }
                    // Lane D: SENTARSE/PARARSE — CG_CHARACTER_POSITION
                    // (28, 2 B: header + position). Parity `Position`
                    // (input_main.cpp:1276-1295): GENERAL → Standup,
                    // SITTING_CHAIR/GROUND → Sitdown (el wire de vuelta es
                    // SIEMPRE SITTING_GROUND); el estado vive en la sesión y
                    // se responde GC_CHARACTER_POSITION (28, 6 B) al propio
                    // jugador (el broadcast de zona es F5).
                    header::CG_CHARACTER_POSITION => {
                        movement::handle_position(session, &pkt).await?.into_result()?;
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
                    // Lane D: QUEST — CG_SCRIPT_BUTTON (66, 5 B: header +
                    // idx), CG_QUEST_INPUT_STRING (30, 66 B: header + char
                    // [65]) y CG_QUEST_CONFIRM (31, 6 B: header + answer +
                    // requestPID). Parity `ScriptButton`/`QuestInputString`/
                    // `QuestConfirm` (input_main.cpp:1850-1917): se re-envían
                    // al mundo (`QuestIntent::Button/Input/Confirm` — el
                    // engine aún no implementa botones/input/confirm
                    // cross-player: log + no-op, GAP documentado en
                    // game_core::quest).
                    header::CG_SCRIPT_BUTTON => {
                        quest::handle_button(session, &pkt).await?.into_result()?;
                    }
                    header::CG_QUEST_INPUT_STRING => {
                        quest::handle_input_string(session, &pkt).await?.into_result()?;
                    }
                    header::CG_QUEST_CONFIRM => {
                        quest::handle_confirm(session, &pkt).await?.into_result()?;
                    }
                    // Lane D: flag PvP — CG_PVP (41). El cliente de esta
                    // variante define el header pero nunca lo envía (sin
                    // sistema de duelo) y el C++ no lo despacha; el handler
                    // es DEFENSIVO (parsea + flag de sesión + log, sin eco
                    // GC_PVP). Detalle en channel/pvp.rs.
                    header::CG_PVP => pvp::handle(session, &pkt).await?.into_result()?,
                    // F6 social: click en NPC (26, 5 B: header + vid — el
                    // mundo resuelve el shop del NPC) + CG_SHOP (50) +
                    // CG_EXCHANGE (27). CG_SHOP es variable (2-4 B según
                    // subheader) — el framer lo resuelve (fix bug 3,
                    // 2026-08-15; antes UnknownHeader -> cierre en el primer
                    // BUY/SELL/END de la tienda).
                    header::CG_ON_CLICK => shop::click(session, &pkt).await?.into_result()?,
                    50 /* CG_SHOP — Packet.h:62 */ => {
                        shop::handle(session, &pkt).await?.into_result()?;
                    }
                    header::CG_EXCHANGE => trade::handle(session, &pkt).await?.into_result()?,
                    // MESSENGER (channel/messenger.rs — bloque 2026-08-21):
                    // CG_MESSENGER (67, variable por subheader — el framer lo
                    // resuelve). Parity `Messenger` input_main.cpp:927-1037:
                    // ADD_BY_VID/ADD_BY_NAME (prompt messenger_auth al
                    // destino) y REMOVE (borrado en ambas direcciones + sync).
                    header::CG_MESSENGER => {
                        crate::channel::messenger::handle(session, &pkt).await?.into_result()?;
                    }
                    // PARTY (lane 2026-08-16 — `channel/party.rs`): invitación
                    // (72), respuesta (73), expulsión/salida (74), estado/rol
                    // (75), skill de party (76) y modo de reparto de exp (78).
                    header::CG_PARTY_INVITE => {
                        party::handle_invite(session, &pkt).await?.into_result()?;
                    }
                    header::CG_PARTY_INVITE_ANSWER => {
                        party::handle_invite_answer(session, &pkt).await?.into_result()?;
                    }
                    header::CG_PARTY_REMOVE => {
                        party::handle_remove(session, &pkt).await?.into_result()?;
                    }
                    header::CG_PARTY_SET_STATE => {
                        party::handle_set_state(session, &pkt).await?.into_result()?;
                    }
                    header::CG_PARTY_USE_SKILL => {
                        party::handle_use_skill(session, &pkt).await?.into_result()?;
                    }
                    header::CG_PARTY_PARAMETER => {
                        party::handle_parameter(session, &pkt).await?.into_result()?;
                    }
                    // GUILD (slice 2026-08-27 — channel/guild.rs): CG_GUILD
                    // (80, variable por subheader — el framer lo resuelve):
                    // CREATE (sub 1) → GC_GUILD INFO + GC_CHAT; INVITE
                    // (sub 0) / respuesta (sub 11) → pendiente en la guild
                    // + GC_GUILD INVITE (2026-08-28). Memoria del proceso
                    // (player.guild PG = slice futuro, GAP documentado).
                    header::CG_GUILD => {
                        crate::channel::guild::handle(session, &pkt).await?.into_result()?;
                    }
                    // SAFEBOX (channel/safebox.rs — parity SafeboxCheckin/
                    // SafeboxCheckout/SafeboxItemMove, input_main.cpp:1940-
                    // 2117 + safebox.cpp:170-231): meter (70) / sacar (71) /
                    // mover dentro (77, mismo shape que CG_ITEM_MOVE) de la
                    // caja. El 79 (CG_SAFEBOX_MONEY — oro de la caja) es
                    // DEFENSIVO: el C++ congelado no lo registra y el
                    // cliente de la variante no lo envía.
                    header::CG_SAFEBOX_CHECKIN => {
                        safebox::handle_checkin(session, &pkt).await?.into_result()?;
                    }
                    header::CG_SAFEBOX_CHECKOUT => {
                        safebox::handle_checkout(session, &pkt).await?.into_result()?;
                    }
                    header::CG_SAFEBOX_ITEM_MOVE => {
                        safebox::handle_item_move(session, &pkt).await?.into_result()?;
                    }
                    header::CG_SAFEBOX_MONEY => {
                        safebox::handle_money(session, &pkt).await?.into_result()?;
                    }
                    // LAND (phase 1 — channel/land.rs): CG_LAND_BUY (56) /
                    // CG_LAND_TRANSFER (57) — aditivos del reforge (el
                    // cliente v24 no los envía; el id lo asigna la sequence
                    // PG, ver channel/land.rs).
                    header::CG_LAND_BUY => land::handle_buy(session, &pkt).await?.into_result()?,
                    header::CG_LAND_TRANSFER => {
                        land::handle_transfer(session, &pkt).await?.into_result()?;
                    }
                    // DRAGON SOUL (phase 1 — channel/dragon_soul.rs):
                    // CG_DRAGON_SOUL_REFINE (205, 47 B — Packet.h:2715-2722).
                    // Parity `CInputMain` input_main.cpp:3197-3222: registra
                    // el refine en el ledger `player.dragon_soul` (id por la
                    // IDENTITY de PG) y responde el FAIL determinista — la
                    // ventana no se cuelga; el refine real es fase 2.
                    header::CG_DRAGON_SOUL_REFINE => {
                        dragon_soul::handle_refine(session, &pkt).await?.into_result()?;
                    }
                    // TODO(F5 npcs): game_core::npc::... para los NPCs/mobs
                    //
                    // SUBIR STATS (lane D — documentado, sin paquete): el
                    // cliente de esta variante NO tiene header de stat-up
                    // (Packet.h:32-37 — el 21 es `//HEADER_BLANK21`, sin
                    // sender; el C++ de esta variante tampoco lo despacha:
                    // input_main.cpp no tiene AddPoint). No hay paquete
                    // identificable → NO hay handler que añadir: si un
                    // cliente externo mandara el 21 caería como
                    // UnknownHeader del framer (cierre documentado). Los
                    // stat_point se cargan y se mandan en GC_POINTS
                    // (POINT_STAT) — el día que exista el wire, el handler
                    // validará stat_point > 0, sumará al ST/DX/IQ/HT
                    // pedido (parity ComputePoints) y reenviará GC_POINTS.
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
            // F6 social (gap-lane-C): bytes de CHAT de OTRAS sesiones (el
            // broadcast GC_CHAT y el whisper GC_WHISPER entregan aquí — el
            // outbox `chat_rx` de la sesión; ver chat.rs). El cierre del
            // canal es inalcanzable mientras la sesión vive (la sesión
            // conserva su propio `chat_tx`); defensivo.
            bytes = session.chat_rx.recv() => {
                let Some(bytes) = bytes else {
                    return Err("canal de chat de la sesión cerrado".into());
                };
                session
                    .send(&bytes)
                    .await
                    .map_err(|e| format!("enviando chat de otra sesión: {e}"))?;
            }
            // PARTY (lane 2026-08-16): mensajes de OTROS jugadores (GC_PARTY_*,
            // exp compartida, Joined/LeftParty) — el outbox `party_rx`; los
            // aplica `party::handle_msg` (bytes al socket / exp al row /
            // party_id).
            msg = session.party_rx.recv() => {
                let Some(msg) = msg else {
                    return Err("canal de party de la sesión cerrado".into());
                };
                party::handle_msg(session, msg).await?;
            }
        }
    }
}
