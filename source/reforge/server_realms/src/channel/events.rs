//! `channel/events.rs` — los handlers de los EVENTOS S→C del mundo (R-s4):
//! cada `NpcEvent` (movimiento/combate/items/skills del mundo COMPARTIDO) se
//! traduce a sus paquetes GC y a las mutaciones del estado de la sesión
//! (hp/exp/inventario del row + persistencia). El routing por jugador ya
//! ocurrió en la tarea del canal (`route_events`) — aquí solo llegan los
//! eventos del jugador de ESTA conexión.
//!
//! Parity: los paquetes GC los construyen los módulos puros de `game_core` (el
//! mundo ya resolvió cooldown/rango/daño — server-authoritative); este
//! handler solo los reenvía y aplica el estado PG-bound (row/inventory).

use database::item::ItemRepo;
use protocol::world::{
    TPacketGCAffectAdd, TPacketGCItemGroundAdd, TPacketGCItemGroundDel, TPacketGCItemOwnership,
    TPacketGCItemSet, TItemPos,
};
use game_core::ecs::{
    CombatEvent, Intent, ItemEvent, ItemIntent, MoveEvent, NpcEvent, SkillEvent,
};
use game_core::packets;

use crate::channel::session::Session;
use crate::channel::{now32, INVENTORY_MAX_NUM, ITEM_COUNT_LIMIT};

/// Un evento S→C del mundo → paquetes GC + estado de la sesión. `Err` =
/// fatal (socket/PG); los rechazos internos (sin víctima, fuera de rango,
/// inventario lleno) son `Ok(())` con log — el loop sigue.
pub async fn handle(session: &mut Session, ev: NpcEvent) -> Result<(), String> {
    match ev {
        NpcEvent::Move(MoveEvent::Moved { vid, x, y, rot, duration_ms, .. }) => {
            // GC_MOVE(FUNC_MOVE): el cliente interpola el paso (parity del
            // tick previo del canal).
            let mv = protocol::movement::TPacketGCMove {
                header: protocol::movement::TPacketGCMove::HEADER,
                b_func: protocol::movement::TPacketGCMove::FUNC_MOVE,
                b_arg: 0,
                b_rot: rot,
                vid,
                x,
                y,
                dw_time: now32(),
                dw_duration: duration_ms,
            };
            session
                .send(&mv.to_bytes())
                .await
                .map_err(|e| format!("enviando GC_MOVE: {e}"))?;
        }
        NpcEvent::Combat(CombatEvent::MobAttack { vid, vnum, x, y, damage, .. }) => {
            // GC_MOVE(FUNC_ATTACK): x/y = posición actual del mob,
            // dwDuration 0 (parity char_state.cpp:386).
            let mv = protocol::movement::TPacketGCMove {
                header: protocol::movement::TPacketGCMove::HEADER,
                b_func: protocol::movement::TPacketGCMove::FUNC_ATTACK,
                b_arg: 0,
                b_rot: 0,
                vid,
                x,
                y,
                dw_time: now32(),
                dw_duration: 0,
            };
            session
                .send(&mv.to_bytes())
                .await
                .map_err(|e| format!("enviando GC_MOVE(FUNC_ATTACK): {e}"))?;
            // GC_DAMAGE_INFO (135) al jugador — el número de daño (parity
            // `SendDamagePacket`).
            session
                .send(&protocol::combat::GcDamageInfo::new(
                    session.player_vid(),
                    protocol::combat::damage_flag::NORMAL,
                    damage,
                )
                .to_bytes())
                .await
                .map_err(|e| format!("enviando GC_DAMAGE_INFO: {e}"))?;
            // Daño al jugador + GC_POINTS (la barra) + save.
            // La MUERTE del PC (hp <= 0): GC_DEAD + puntos a 0 (el cliente
            // muestra la pantalla de muerte); el revive lo dispara el
            // CG_SCRIPT_ANSWER del cliente (handler propio — RestartAtSamePos).
            let hp = session.row().hp.saturating_sub(damage);
            session.row_mut().hp = hp;
            if hp <= 0 {
                session.row_mut().hp = 0;
                session
                    .send(&protocol::world::TPacketGCDead::new(session.player_vid()).to_bytes())
                    .await
                    .map_err(|e| format!("enviando GC_DEAD: {e}"))?;
                eprintln!(
                    "server_realms: channel conn {}: {} MURIÓ (mob vnum {vnum} \
                     vid {vid}) — esperando revive (CG_SCRIPT_ANSWER)",
                    session.conn_id, session.row().name
                );
            } else {
                eprintln!(
                    "server_realms: channel conn {}: mob vnum {vnum} (vid {vid}) \
                     atacó a {} por {damage} (hp {})",
                    session.conn_id, session.row().name, hp
                );
            }
            session
                .send(&packets::points_packet(session.row(), session.next_exp, &session.battle).to_bytes())
                .await
                .map_err(|e| format!("enviando GC_POINTS: {e}"))?;
            session.save();
        }
        NpcEvent::Combat(CombatEvent::AggroOn { vid, vnum, .. }) => {
            eprintln!(
                "server_realms: channel conn {}: mob vnum {vnum} (vid {vid}) — \
                 detectó al jugador — AGGRO proactivo",
                session.conn_id
            );
        }
        NpcEvent::Combat(CombatEvent::AggroOff { vid, vnum, .. }) => {
            eprintln!(
                "server_realms: channel conn {}: mob vnum {vnum} (vid {vid}) — \
                 perdió el aggro (fuera de rango)",
                session.conn_id
            );
        }
        NpcEvent::Combat(CombatEvent::Spawned { packets, .. }) => {
            // Spawn dinámico: los ADD(+INFO) ya vienen construidos por el
            // mundo (parity game_core::npc::entry_spawns) — el cliente los pinta
            // al acercarse el jugador.
            for pkt in packets {
                session
                    .send(&pkt)
                    .await
                    .map_err(|e| format!("enviando spawn dinámico: {e}"))?;
            }
        }
        NpcEvent::Combat(CombatEvent::Despawned { vid, .. }) => {
            session
                .send(&protocol::world::TPacketGCCharacterDelete::new(vid).to_bytes())
                .await
                .map_err(|e| format!("enviando GC_CHARACTER_DEL: {e}"))?;
        }
        NpcEvent::Combat(CombatEvent::AttackResult {
            victim_vid,
            packets,
            damage,
            dead,
            victim,
            ..
        }) => {
            // Los paquetes del golpe (GcAttack/GcDamageInfo) los construyó
            // `handle_attack` (puro) en el mundo — la conexión los reenvía
            // tal cual.
            for pkt in packets {
                session
                    .send(&pkt)
                    .await
                    .map_err(|e| format!("enviando combate: {e}"))?;
            }
            if damage > 0 {
                let Some(v) = victim else {
                    eprintln!(
                        "server_realms: channel conn {}: AttackResult sin víctima — \
                         daño {damage} descartado",
                        session.conn_id
                    );
                    return Ok(());
                };
                if dead {
                    // Flujo de kill compartido (ataque normal y skills):
                    // GC_DEAD/DEL + recompensa + drop.
                    session.apply_kill(victim_vid, v).await?;
                } else {
                    // GC_TARGET (63) — la barra de vida del mob baja tras el
                    // golpe (fix bug 5: parity `BroadcastTargetPacket`,
                    // char.cpp:5115-5143 — el daño al mob se difunde a quien
                    // lo tiene apuntado).
                    let pct = if v.max_hp > 0 {
                        (v.hp.saturating_mul(100) / v.max_hp).clamp(0, 100) as u8
                    } else {
                        0
                    };
                    session
                        .send(&protocol::world::TPacketGCTarget::new(victim_vid, pct).to_bytes())
                        .await
                        .map_err(|e| format!("enviando GC_TARGET: {e}"))?;
                    eprintln!(
                        "server_realms: channel conn {}: {} golpeó mob vnum {} ({}/{})",
                        session.conn_id, session.row().name, v.vnum, v.hp, v.max_hp
                    );
                }
            }
        }
        NpcEvent::Combat(CombatEvent::TargetResult { vid, hp, max_hp, .. }) => {
            // GC_TARGET (63) — la barra de vida del objetivo al apuntarlo
            // (parity `SetTarget`, char.cpp:5048-5094: bHPPercent =
            // hp*100/max; 0 para PCs/mobs sin max — el subset solo apunta
            // mobs materializados).
            let pct = if max_hp > 0 { (hp.saturating_mul(100) / max_hp).clamp(0, 100) as u8 } else { 0 };
            session
                .send(&protocol::world::TPacketGCTarget::new(vid, pct).to_bytes())
                .await
                .map_err(|e| format!("enviando GC_TARGET: {e}"))?;
            eprintln!(
                "server_realms: channel conn {}: target a vid {vid} — barra {pct}%",
                session.conn_id
            );
        }
        NpcEvent::Item(ItemEvent::PickupResult { item_vid, item, .. }) => {
            session.pending_pickups.remove(&item_vid);
            let Some(gi) = item else {
                eprintln!(
                    "server_realms: channel conn {}: pickup de vid {item_vid} — \
                     no hay item en el suelo",
                    session.conn_id
                );
                return Ok(());
            };
            let dist = game_core::combat::distance_approx(
                session.motion().x - gi.x,
                session.motion().y - gi.y,
            );
            if dist > 600 {
                eprintln!(
                    "server_realms: channel conn {}: pickup de vid {item_vid} — \
                     fuera de rango ({dist} > 600)",
                    session.conn_id
                );
                return Ok(());
            }
            // F5.3 (STACKING — parity `AutoStackItemProto`,
            // char_item.cpp:6722-6755): si ya existe un item del MISMO vnum
            // en el inventario con count < 200 (`g_bItemCountLimit`,
            // config.cpp:39) y sockets vacíos (`FN_check_item_socket`), se
            // suma el count al stack y se manda `GC_ITEM_UPDATE` (38 B) en
            // vez de crear un slot nuevo. El flag `ITEM_FLAG_STACKABLE` del
            // item_proto no se consulta (subset documentado).
            let mut remaining = gi.count as i64;
            loop {
                let Some(idx) = session.inventory.iter().position(|i| {
                    i.window == "INVENTORY"
                        && i.vnum == gi.vnum as i64
                        && i.sockets == [0; 3]
                        && i.count < ITEM_COUNT_LIMIT
                }) else {
                    break; // sin stack con espacio → slot nuevo
                };
                let add = (ITEM_COUNT_LIMIT - session.inventory[idx].count).min(remaining);
                session.inventory[idx].count += add;
                remaining -= add;
                // GC_ITEM_UPDATE: el count del stack actualizado.
                let up = protocol::world::TPacketGCItemUpdate {
                    header: protocol::world::TPacketGCItemUpdate::HEADER,
                    cell: TItemPos {
                        window: TItemPos::WINDOW_INVENTORY,
                        cell: session.inventory[idx].pos as u16,
                    },
                    count: session.inventory[idx].count as u8,
                    sockets: session.inventory[idx].sockets,
                    attrs: session.inventory[idx].attrs,
                };
                session
                    .send(&up.to_bytes())
                    .await
                    .map_err(|e| format!("enviando GC_ITEM_UPDATE: {e}"))?;
                // Persistencia del stack actualizado.
                ItemRepo::new(session.pool.clone())
                    .upsert(&session.inventory[idx], session.row().id)
                    .await?;
                eprintln!(
                    "server_realms: channel conn {}: {} apiló item vnum {} \
                     (vid {}) → slot {} (count {})",
                    session.conn_id,
                    session.row().name,
                    gi.vnum,
                    item_vid,
                    session.inventory[idx].pos,
                    session.inventory[idx].count
                );
                if remaining <= 0 {
                    break;
                }
            }
            if remaining <= 0 {
                // TODO apilado: el item del suelo desaparece.
                session
                    .send(&TPacketGCItemGroundDel::new(item_vid).to_bytes())
                    .await
                    .map_err(|e| format!("enviando GC_ITEM_GROUND_DEL: {e}"))?;
                session.intent(Intent::Item(ItemIntent::RemoveItem { item_vid }))?;
                return Ok(());
            }
            // Primer cell libre del inventario (parity `GetEmptyInventory`,
            // char_item.cpp:709-711).
            let occupied: std::collections::HashSet<u16> = session
                .inventory
                .iter()
                .filter(|i| i.window == "INVENTORY")
                .map(|i| i.pos as u16)
                .collect();
            let Some(slot) = (0u16..INVENTORY_MAX_NUM).find(|c| !occupied.contains(c)) else {
                eprintln!(
                    "server_realms: channel conn {}: inventario lleno — \
                     el item {item_vid} queda en el suelo",
                    session.conn_id
                );
                return Ok(());
            };
            // Item nuevo del pickup: id del rango ITEM_ID_RANGE (100M-200M —
            // parity `ItemIDRangeManager.cpp:93,121`; el E2E Q8 sondea ese
            // rango).
            let id = ItemRepo::new(session.pool.clone())
                .max_id_in_range(100_000_000, 200_000_000)
                .await?
                .map(|m| m + 1)
                .unwrap_or(100_000_000);
            let new_item = database::item::ItemRow {
                id,
                window: "INVENTORY".to_string(),
                pos: slot as i32,
                count: remaining,
                vnum: gi.vnum as i64,
                sockets: [0; 3],
                attrs: [(0, 0); 7],
            };
            // GC_ITEM_SET (51 B — el slot pintado del cliente).
            let set = TPacketGCItemSet {
                header: TPacketGCItemSet::HEADER,
                cell: TItemPos { window: TItemPos::WINDOW_INVENTORY, cell: slot },
                vnum: gi.vnum,
                count: remaining as u8,
                flags: 0,
                anti_flags: 0,
                highlight: 0,
                sockets: [0; 3],
                attrs: [(0, 0); 7],
            };
            session
                .send(&set.to_bytes())
                .await
                .map_err(|e| format!("enviando GC_ITEM_SET: {e}"))?;
            session
                .send(&TPacketGCItemGroundDel::new(item_vid).to_bytes())
                .await
                .map_err(|e| format!("enviando GC_ITEM_GROUND_DEL: {e}"))?;
            // Persistencia durable + commit del mundo.
            ItemRepo::new(session.pool.clone())
                .upsert(&new_item, session.row().id)
                .await?;
            session.intent(Intent::Item(ItemIntent::RemoveItem { item_vid }))?;
            session.inventory.push(new_item);
            eprintln!(
                "server_realms: channel conn {}: {} recogió item vnum {} (vid {}) \
                 → slot {slot} del inventario (id {id})",
                session.conn_id, session.row().name, gi.vnum, item_vid
            );
        }
        NpcEvent::Item(ItemEvent::DropResult { item_vid, vnum, count, x, y, z, .. }) => {
            // El drop se creó en el mundo (vid asignado por el mundo —
            // VidAlloc global): el ADD + ownership salen con el vid correcto.
            session
                .send(&TPacketGCItemGroundAdd::new(item_vid, vnum, x, y, z, count).to_bytes())
                .await
                .map_err(|e| format!("enviando GC_ITEM_GROUND_ADD: {e}"))?;
            // Ownership (parity item.cpp:145-162 — el nombre del dueño sobre
            // el item).
            session
                .send(&TPacketGCItemOwnership::new(
                    item_vid,
                    session.row().name.as_bytes(),
                )
                .to_bytes())
                .await
                .map_err(|e| format!("enviando GC_ITEM_OWNERSHIP: {e}"))?;
            eprintln!(
                "server_realms: channel conn {}: {} — drop item vnum {vnum} \
                 (vid {item_vid}) en el suelo",
                session.conn_id, session.row().name
            );
        }
        NpcEvent::Skill(SkillEvent::SkillResult {
            skill_id,
            victim_vid,
            packets,
            damage,
            dead,
            victim,
            sp_cost,
            hp_cost,
            buff,
            ..
        }) => {
            // dw_arrow: el disparo de un skill de ARCO se RESOLVIÓ — la
            // flecha se consume (parity UseArrow, char_battle.cpp:2770-2789:
            // 1 flecha por uso; el item se queda con count 0 y el gate del
            // próximo disparo rechaza). El flag lo puso el gate de skills.rs
            // (reseteado en cada dispatch — el skill rechazado por el mundo
            // no llega aquí y no consume).
            if session.pending_arrow_shot {
                session.pending_arrow_shot = false;
                super::consume_arrow(session).await?;
            }
            // Los paquetes del daño del skill (GcDamageInfo — el flag del
            // attr) los construyó el mundo.
            for pkt in packets {
                session
                    .send(&pkt)
                    .await
                    .map_err(|e| format!("enviando combate (skill): {e}"))?;
            }
            // Coste SP/HP del skill (parity PointChange(SP/-HP) en UseSkill —
            // el mundo ya lo descontó de su componente; el row espejo se
            // actualiza aquí).
            if sp_cost > 0 || hp_cost > 0 {
                {
                    let row = session.row_mut();
                    row.mp = row.mp.saturating_sub(sp_cost);
                    row.hp = row.hp.saturating_sub(hp_cost);
                }
                session
                    .send(&packets::points_packet(session.row(), session.next_exp, &session.battle).to_bytes())
                    .await
                    .map_err(|e| format!("enviando GC_POINTS: {e}"))?;
                session.save();
            }
            // Buff aplicado: GC_AFFECT_ADD (126) — el icono del cliente (el
            // mundo ya lo tiene en `Affects`).
            if let Some(elem) = buff {
                session
                    .send(&TPacketGCAffectAdd::new(elem).to_bytes())
                    .await
                    .map_err(|e| format!("enviando GC_AFFECT_ADD: {e}"))?;
            }
            if damage > 0 {
                let Some(v) = victim else {
                    eprintln!(
                        "server_realms: channel conn {}: SkillResult sin víctima — \
                         daño {damage} descartado",
                        session.conn_id
                    );
                    return Ok(());
                };
                if dead {
                    session.apply_kill(victim_vid, v).await?;
                } else {
                    eprintln!(
                        "server_realms: channel conn {}: {} golpeó con el skill \
                         {skill_id} a vnum {} ({}/{})",
                        session.conn_id, session.row().name, v.vnum, v.hp, v.max_hp
                    );
                }
            } else if sp_cost > 0 || hp_cost > 0 {
                eprintln!(
                    "server_realms: channel conn {}: {} usó el skill {skill_id} \
                     (SP -{sp_cost}, HP -{hp_cost})",
                    session.conn_id, session.row().name
                );
            }
        }
        NpcEvent::Skill(SkillEvent::AffectRemoved { skill_id, point, .. }) => {
            // GC_AFFECT_REMOVE (127, 6 B: header + dwType + bApplyOn —
            // Packet.h:2536-2543). Se emite crudo (el protocol crate no
            // define el struct — mismo patrón que el GC_CHAT del canal).
            let mut out = Vec::with_capacity(6);
            out.push(127);
            out.extend_from_slice(&skill_id.to_le_bytes());
            out.push(point);
            session
                .send(&out)
                .await
                .map_err(|e| format!("enviando GC_AFFECT_REMOVE: {e}"))?;
        }
        // Lanes futuros (C3 + N1): los emisores viven en sus archivos —
        // `social::emit` y `quest::emit` son async (envían GC + aplican la
        // DB del lane).
        NpcEvent::Social(s) => {
            super::social::emit(session, s).await?;
        }
        NpcEvent::Quest(q) => super::quest::emit(session, q).await?,
    }
    Ok(())
}
