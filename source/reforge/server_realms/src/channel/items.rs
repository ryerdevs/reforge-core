//! `channel/items.rs` — los handlers de ITEMS del game loop (R-s3):
//! CG_ITEM_PICKUP (recoger del suelo), CG_ITEM_USE (consumibles) y
//! CG_ITEM_MOVE (mover/stack/split/equipar/desequipar).
//!
//! Parity del canal C++:
//! - Pickup: `ItemPickup` input_main.cpp:902-907 → `PickupItem`
//!   char_item.cpp:5888-5947 — el mundo responde `PickupResult` (la
//!   distancia + inventario los decide el EVENTO, no este handler).
//! - Use: `UseItemEx` → `UseItem` (char_item.cpp:1616+): value0 = HP flat,
//!   value1 = SP flat, value3 = HP %, value4 = SP %; NO consume sin efecto.
//! - Move: `MoveItem` (char_item.cpp:5609-5767): stack/split/mover todo +
//!   equipar (`EquipItem` :6128, `FindEquipCell` item.cpp:509-623) y
//!   desequipar.
//!
//! C6a (firma uniforme): malformado/rechazos → log + `Outcome::Continue`;
//! errores PG/socket → Err (fatal).

use database::item::ItemRepo;
use protocol::world::{
    TPacketCGItemUse, TPacketGCItemDelDeprecated, TPacketGCItemSet, TItemPos,
};
use game_core::ecs::{CombatIntent, Intent, ItemIntent};
use game_core::packets;

use crate::channel::session::{Outcome, Session};
use crate::channel::{equipped_armor, ITEM_COUNT_LIMIT, INVENTORY_MAX_NUM, WEAR_MAX_NUM};

/// CG_ITEM_PICKUP (11): manda el intent `PickupItem` al mundo (la respuesta
/// `PickupResult` llega por la cola y el EVENTO decide distancia/inventario).
/// `pending_pickups` evita duplicar el MISMO vid mientras el primer pickup
/// se resuelve (la respuesta es asíncrona — parity del flujo síncrono).
pub async fn handle_pickup(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    if pkt.len() < 5 {
        // C6a: malformado → Continue con log (antes cerraba la conexión).
        eprintln!(
            "server_realms: channel conn {}: CG_ITEM_PICKUP malformado ({})",
            session.conn_id,
            pkt.len()
        );
        return Ok(Outcome::Continue);
    }
    let vid = u32::from_le_bytes([pkt[1], pkt[2], pkt[3], pkt[4]]);
    if !session.pending_pickups.insert(vid) {
        eprintln!(
            "server_realms: channel conn {}: pickup de vid {vid} — \
             ya en curso, ignorado",
            session.conn_id
        );
        return Ok(Outcome::Continue);
    }
    session.intent(Intent::Item(ItemIntent::PickupItem {
        player_vid: session.player_vid(),
        item_vid: vid,
    }))?;
    Ok(Outcome::Continue)
}

/// CG_ITEM_USE (11, 4 B: header + TItemPos — Packet.h:559-563). Parity
/// `UseItemEx` → `UseItem` (char_item.cpp:1616+): value0 = HP flat, value1 =
/// SP flat, value3 = HP % del máximo, value4 = SP % del máximo (del
/// item_proto); NO consume si no hay efecto aplicable (HP/MP llenos).
/// Al consumir: GC_POINTS (hp/mp) + count-1 → GC_ITEM_UPDATE (38 B) si
/// queda, GC_ITEM_DEL deprecated (42 B) si agota.
pub async fn handle_use(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let item_use = match TPacketCGItemUse::from_bytes(pkt) {
        Ok(u) => u,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: CG_ITEM_USE malformado: {e}",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    };
    // Buscar el item en el inventario por (window, cell).
    let Some(idx) = session.inventory.iter().position(|i| {
        i.window == "INVENTORY" && i.pos as u16 == item_use.pos.cell
    }) else {
        eprintln!(
            "server_realms: channel conn {}: uso de celda {} sin item",
            session.conn_id, item_use.pos.cell
        );
        return Ok(Outcome::Continue);
    };
    let Some(proto) = ItemRepo::new(&session.config.pg_conn)
        .load_proto_use_values(session.inventory[idx].vnum)
        .await?
    else {
        eprintln!(
            "server_realms: channel conn {}: item vnum {} sin fila en \
             item_proto — uso ignorado",
            session.conn_id, session.inventory[idx].vnum
        );
        return Ok(Outcome::Continue);
    };
    let values = proto.values;
    let max = packets::compute_max_points(session.row()).unwrap_or([100, 100, 0]);
    let mut used = false;
    // value0: HP flat (char_item.cpp:4172-4180).
    if values[0] != 0 && session.row().hp < max[0] {
        let hp = (session.row().hp + values[0]).min(max[0]);
        session.row_mut().hp = hp;
        used = true;
    }
    // value1: SP flat (char_item.cpp:4182-4190).
    if values[1] != 0 && session.row().mp < max[1] {
        let mp = (session.row().mp + values[1]).min(max[1]);
        session.row_mut().mp = mp;
        used = true;
    }
    // value3: HP % del máximo (char_item.cpp:4192-4200).
    if values[3] != 0 && session.row().hp < max[0] {
        let hp = (session.row().hp + values[3] * max[0] / 100).min(max[0]);
        session.row_mut().hp = hp;
        used = true;
    }
    // value4: SP % del máximo (char_item.cpp:4202-4210).
    if values[4] != 0 && session.row().mp < max[1] {
        let mp = (session.row().mp + values[4] * max[1] / 100).min(max[1]);
        session.row_mut().mp = mp;
        used = true;
    }
    if !used {
        // Sin efecto aplicable (HP/MP llenos o sin values) — no consume
        // (parity: el C++ solo SetCount si `used`).
        eprintln!(
            "server_realms: channel conn {}: item vnum {} sin efecto \
             (HP/MP llenos)",
            session.conn_id, session.inventory[idx].vnum
        );
        return Ok(Outcome::Continue);
    }
    // El mundo COMPARTIDO refleja el HP/SP nuevos (el daño del AI y el coste
    // de las skills los gastan de ahí — sin esto, las pociones serían
    // cosméticas).
    session.intent(Intent::Combat(CombatIntent::SetHp {
        player_vid: session.player_vid(),
        hp: session.row().hp,
    }))?;
    session.intent(Intent::Combat(CombatIntent::SetMp {
        player_vid: session.player_vid(),
        mp: session.row().mp,
    }))?;
    // GC_POINTS (hp/mp actualizados) + persistencia.
    session
        .send(&packets::points_packet(session.row(), session.next_exp).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_POINTS: {e}"))?;
    session.store().save_character(session.row());
    // Consumir 1 del stack (parity `item->SetCount(count-1)`).
    session.inventory[idx].count -= 1;
    if session.inventory[idx].count <= 0 {
        // Se agotó: GC_ITEM_DEL deprecated (42 B) + delete.
        let cell = TItemPos {
            window: TItemPos::WINDOW_INVENTORY,
            cell: session.inventory[idx].pos as u16,
        };
        let vnum = session.inventory[idx].vnum;
        let id = session.inventory[idx].id;
        session
            .send(&TPacketGCItemDelDeprecated::new(cell, vnum as u32, 0).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_ITEM_DEL: {e}"))?;
        ItemRepo::new(&session.config.pg_conn).delete(id).await?;
        session.inventory.remove(idx);
        eprintln!(
            "server_realms: channel conn {}: {} usó item vnum {vnum} \
             (agotado — slot borrado)",
            session.conn_id, session.row().name
        );
    } else {
        // GC_ITEM_UPDATE (38 B) con el count nuevo + upsert.
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
        ItemRepo::new(&session.config.pg_conn)
            .upsert(&session.inventory[idx], session.row().id)
            .await?;
        eprintln!(
            "server_realms: channel conn {}: {} usó item vnum {} \
             (count {})",
            session.conn_id,
            session.row().name,
            session.inventory[idx].vnum,
            session.inventory[idx].count
        );
    }
    Ok(Outcome::Continue)
}

/// CG_ITEM_MOVE (13, 8 B: header + TItemPos origen + TItemPos destino +
/// BYTE num — Packet.h:593-599). Parity `MoveItem` (char_item.cpp:5609-5767):
/// stack si el destino tiene el mismo vnum + sockets iguales + count < 200;
/// split si `0 < num < count`; si no, mover todo. Subset: INVENTORY→
/// INVENTORY + equipar/desequipar (Belt/DS pendiente).
pub async fn handle_move(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let mv = match protocol::world::TPacketCGItemMove::from_bytes(pkt) {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: CG_ITEM_MOVE malformado: {e}",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    };
    // Subset de windows: INVENTORY→INVENTORY (mover/stack/split),
    // INVENTORY→EQUIPMENT (equipar — parity `EquipItem` char_item.cpp:6128;
    // wire: el cell del EQUIPMENT = INVENTORY_MAX_NUM + wear, length.h:827)
    // y EQUIPMENT→INVENTORY (desequipar). Belt/DS fuera.
    let equipping = mv.change_pos.window == TItemPos::WINDOW_EQUIPMENT;
    let unequipping = mv.pos.window == TItemPos::WINDOW_EQUIPMENT;
    let inv_to_inv = mv.pos.window == TItemPos::WINDOW_INVENTORY
        && mv.change_pos.window == TItemPos::WINDOW_INVENTORY;
    if !(inv_to_inv || (equipping && mv.pos.window == TItemPos::WINDOW_INVENTORY)
        || (unequipping && mv.change_pos.window == TItemPos::WINDOW_INVENTORY))
    {
        eprintln!(
            "server_realms: channel conn {}: CG_ITEM_MOVE fuera del \
             subset (windows {}→{}) — belt/DS pendiente",
            session.conn_id, mv.pos.window, mv.change_pos.window
        );
        return Ok(Outcome::Continue);
    }
    if mv.pos.cell == mv.change_pos.cell {
        return Ok(Outcome::Continue); // @fixme196 — misma posición
    }
    let src_win = if unequipping { "EQUIPMENT" } else { "INVENTORY" };
    let src = session.inventory.iter().position(|i| {
        i.window == src_win && i.pos as u16 == mv.pos.cell
    });
    let Some(src) = src else {
        eprintln!(
            "server_realms: channel conn {}: move de celda {} sin item",
            session.conn_id, mv.pos.cell
        );
        return Ok(Outcome::Continue);
    };
    let want = i64::from(mv.num);
    if want > session.inventory[src].count {
        return Ok(Outcome::Continue); // parity: item->GetCount() < count → false
    }
    // EQUIPAR (INVENTORY→EQUIPMENT): el cell destino = INVENTORY_MAX_NUM +
    // wear; slot vacío obligatorio (parity `GetItem(DestCell)` → false,
    // char_item.cpp:5675-5680); `num` debe ser 0 (todo el stack — el split
    // al equipar es pendiente).
    if equipping {
        let Some(wear) = mv.change_pos.cell.checked_sub(INVENTORY_MAX_NUM)
        else {
            eprintln!(
                "server_realms: channel conn {}: equip a cell {} \
                 fuera del rango (debe ser INVENTORY_MAX_NUM + wear)",
                session.conn_id, mv.change_pos.cell
            );
            return Ok(Outcome::Continue);
        };
        if wear >= WEAR_MAX_NUM {
            eprintln!(
                "server_realms: channel conn {}: equip a wear {} \
                 fuera del rango (WEAR_MAX_NUM {})",
                session.conn_id, wear, WEAR_MAX_NUM
            );
            return Ok(Outcome::Continue);
        }
        if mv.num != 0 {
            eprintln!(
                "server_realms: channel conn {}: split al equipar — \
                 pendiente (num {})",
                session.conn_id, mv.num
            );
            return Ok(Outcome::Continue);
        }
        if session
            .inventory
            .iter()
            .any(|i| i.window == "EQUIPMENT" && i.pos as u16 == mv.change_pos.cell)
        {
            eprintln!(
                "server_realms: channel conn {}: slot de equip {} \
                 ocupado — rechazado (parity EquipItem)",
                session.conn_id, mv.change_pos.cell
            );
            return Ok(Outcome::Continue);
        }
        // Validación de TIPO (parity `EquipItem` → `FindEquipCell(item,
        // iCandidateCell)`, char_item.cpp:6139 + item.cpp:509-623): el slot
        // candidato debe ser el slot del item según su `wearflag`
        // (WEARABLE_*). Un item sin wearflag o con slot equivocado → rechazo.
        let Some(proto) = ItemRepo::new(&session.config.pg_conn)
            .load_proto_use_values(session.inventory[src].vnum)
            .await?
        else {
            eprintln!(
                "server_realms: channel conn {}: item vnum {} sin \
                 item_proto — equip rechazado",
                session.conn_id, session.inventory[src].vnum
            );
            return Ok(Outcome::Continue);
        };
        match packets::find_equip_cell(&proto) {
            Some(slot) if slot == wear => {}
            Some(slot) => {
                eprintln!(
                    "server_realms: channel conn {}: item vnum {} es \
                     de wear {} pero el slot pedido es {wear} — rechazado \
                     (parity FindEquipCell)",
                    session.conn_id, session.inventory[src].vnum, slot
                );
                return Ok(Outcome::Continue);
            }
            None => {
                eprintln!(
                    "server_realms: channel conn {}: item vnum {} no \
                     equipable (wearflag 0 o fuera del subset) — rechazado",
                    session.conn_id, session.inventory[src].vnum
                );
                return Ok(Outcome::Continue);
            }
        }
        // Mover el item: window INVENTORY → EQUIPMENT.
        let vnum = session.inventory[src].vnum;
        let cell = TItemPos {
            window: TItemPos::WINDOW_INVENTORY,
            cell: mv.pos.cell,
        };
        session
            .send(&TPacketGCItemDelDeprecated::new(
                cell,
                vnum as u32,
                session.inventory[src].count as u8,
            )
            .to_bytes())
            .await
            .map_err(|e| format!("enviando GC_ITEM_DEL: {e}"))?;
        session.inventory[src].window = "EQUIPMENT".to_string();
        session.inventory[src].pos = mv.change_pos.cell as i32;
        let set = TPacketGCItemSet {
            header: TPacketGCItemSet::HEADER,
            cell: TItemPos {
                window: TItemPos::WINDOW_EQUIPMENT,
                cell: mv.change_pos.cell,
            },
            vnum: session.inventory[src].vnum as u32,
            count: session.inventory[src].count as u8,
            flags: 0,
            anti_flags: 0,
            highlight: 0,
            sockets: session.inventory[src].sockets,
            attrs: session.inventory[src].attrs,
        };
        session
            .send(&set.to_bytes())
            .await
            .map_err(|e| format!("enviando GC_ITEM_SET (equip): {e}"))?;
        ItemRepo::new(&session.config.pg_conn)
            .upsert(&session.inventory[src], session.row().id)
            .await?;
        // El ADDITIONAL_INFO (parts) se reenvía con los parts COMPUTADOS de
        // los items equipados (ComputeParts F5.3 — el personaje muestra el
        // arma/armadura; el part = vnum del item).
        let parts = packets::equipped_parts(session.row(), &session.inventory);
        let arrows = super::equipped_arrow_index(&session.inventory)
            .map(|i| session.inventory[i].count as u32)
            .unwrap_or(0);
        session
            .send(&packets::character_additional_info_with_parts(
                session.row(),
                session.empire,
                &parts,
                arrows,
            )
            .to_bytes())
            .await
            .map_err(|e| format!("enviando GC_CHARACTER_ADDITIONAL_INFO: {e}"))?;
        // El iArmor del mundo COMPARTIDO (el ataque del mob usa
        // `player_def_grade` con la armadura del equipo — solo cambia al
        // equipar/desequipar).
        let armor = equipped_armor(&session.inventory, &session.config.pg_conn).await?;
        session.intent(Intent::Combat(CombatIntent::SetArmor {
            player_vid: session.player_vid(),
            armor,
        }))?;
        eprintln!(
            "server_realms: channel conn {}: {} EQUIPÓ item vnum {vnum} \
             (wear {wear}, cell {})",
            session.conn_id, session.row().name, mv.change_pos.cell
        );
        return Ok(Outcome::Continue);
    }
    // DESEQUIPAR (EQUIPMENT→INVENTORY): el destino INVENTORY debe estar vacío.
    if unequipping {
        if session
            .inventory
            .iter()
            .any(|i| i.window == "INVENTORY" && i.pos as u16 == mv.change_pos.cell)
        {
            eprintln!(
                "server_realms: channel conn {}: celda {} ocupada — \
                 desequipar rechazado",
                session.conn_id, mv.change_pos.cell
            );
            return Ok(Outcome::Continue);
        }
        let vnum = session.inventory[src].vnum;
        let cell = TItemPos {
            window: TItemPos::WINDOW_EQUIPMENT,
            cell: mv.pos.cell,
        };
        session
            .send(&TPacketGCItemDelDeprecated::new(
                cell,
                vnum as u32,
                session.inventory[src].count as u8,
            )
            .to_bytes())
            .await
            .map_err(|e| format!("enviando GC_ITEM_DEL: {e}"))?;
        session.inventory[src].window = "INVENTORY".to_string();
        session.inventory[src].pos = mv.change_pos.cell as i32;
        let set = TPacketGCItemSet {
            header: TPacketGCItemSet::HEADER,
            cell: TItemPos {
                window: TItemPos::WINDOW_INVENTORY,
                cell: mv.change_pos.cell,
            },
            vnum: session.inventory[src].vnum as u32,
            count: session.inventory[src].count as u8,
            flags: 0,
            anti_flags: 0,
            highlight: 0,
            sockets: session.inventory[src].sockets,
            attrs: session.inventory[src].attrs,
        };
        session
            .send(&set.to_bytes())
            .await
            .map_err(|e| format!("enviando GC_ITEM_SET (desequip): {e}"))?;
        ItemRepo::new(&session.config.pg_conn)
            .upsert(&session.inventory[src], session.row().id)
            .await?;
        // ADDITIONAL_INFO con los parts COMPUTADOS (el arma/armadura ya no
        // está — el part se quita).
        let parts = packets::equipped_parts(session.row(), &session.inventory);
        let arrows = super::equipped_arrow_index(&session.inventory)
            .map(|i| session.inventory[i].count as u32)
            .unwrap_or(0);
        session
            .send(&packets::character_additional_info_with_parts(
                session.row(),
                session.empire,
                &parts,
                arrows,
            )
            .to_bytes())
            .await
            .map_err(|e| format!("enviando GC_CHARACTER_ADDITIONAL_INFO: {e}"))?;
        // El iArmor del mundo COMPARTIDO baja con el item quitado.
        let armor = equipped_armor(&session.inventory, &session.config.pg_conn).await?;
        session.intent(Intent::Combat(CombatIntent::SetArmor {
            player_vid: session.player_vid(),
            armor,
        }))?;
        eprintln!(
            "server_realms: channel conn {}: {} DESEQUIPÓ item vnum {vnum} \
             → celda {}",
            session.conn_id, session.row().name, mv.change_pos.cell
        );
        return Ok(Outcome::Continue);
    }
    // Destino ocupado: stack si mismo vnum + sockets iguales + count < límite
    // (char_item.cpp:5709-5727); si no, el C++ corta (slot ocupado no-stack).
    let dst = session.inventory.iter().position(|i| {
        i.window == "INVENTORY" && i.pos as u16 == mv.change_pos.cell
    });
    if let Some(dst) = dst {
        if session.inventory[src].vnum == session.inventory[dst].vnum
            && session.inventory[src].sockets == session.inventory[dst].sockets
        {
            let add = (ITEM_COUNT_LIMIT - session.inventory[dst].count)
                .min(if want == 0 { session.inventory[src].count } else { want });
            if add <= 0 {
                eprintln!(
                    "server_realms: channel conn {}: stack de celda {} \
                     lleno — move ignorado",
                    session.conn_id, mv.change_pos.cell
                );
                return Ok(Outcome::Continue);
            }
            session.inventory[src].count -= add;
            session.inventory[dst].count += add;
            // GC_ITEM_UPDATE en el destino (SetCount).
            let up = protocol::world::TPacketGCItemUpdate {
                header: protocol::world::TPacketGCItemUpdate::HEADER,
                cell: TItemPos {
                    window: TItemPos::WINDOW_INVENTORY,
                    cell: mv.change_pos.cell,
                },
                count: session.inventory[dst].count as u8,
                sockets: session.inventory[dst].sockets,
                attrs: session.inventory[dst].attrs,
            };
            session
                .send(&up.to_bytes())
                .await
                .map_err(|e| format!("enviando GC_ITEM_UPDATE: {e}"))?;
            ItemRepo::new(&session.config.pg_conn)
                .upsert(&session.inventory[dst], session.row().id)
                .await?;
            eprintln!(
                "server_realms: channel conn {}: {} apiló en move vnum {} \
                 (celda {} → {})",
                session.conn_id,
                session.row().name,
                session.inventory[src].vnum,
                mv.pos.cell,
                mv.change_pos.cell
            );
            if session.inventory[src].count <= 0 {
                // El origen se agotó → GC_ITEM_DEL + delete.
                let cell = TItemPos {
                    window: TItemPos::WINDOW_INVENTORY,
                    cell: mv.pos.cell,
                };
                let vnum = session.inventory[src].vnum;
                let id = session.inventory[src].id;
                session
                    .send(&TPacketGCItemDelDeprecated::new(cell, vnum as u32, 0).to_bytes())
                    .await
                    .map_err(|e| format!("enviando GC_ITEM_DEL: {e}"))?;
                ItemRepo::new(&session.config.pg_conn).delete(id).await?;
                session.inventory.remove(src);
            } else {
                // El origen queda → GC_ITEM_UPDATE + upsert.
                let up = protocol::world::TPacketGCItemUpdate {
                    header: protocol::world::TPacketGCItemUpdate::HEADER,
                    cell: TItemPos {
                        window: TItemPos::WINDOW_INVENTORY,
                        cell: mv.pos.cell,
                    },
                    count: session.inventory[src].count as u8,
                    sockets: session.inventory[src].sockets,
                    attrs: session.inventory[src].attrs,
                };
                session
                    .send(&up.to_bytes())
                    .await
                    .map_err(|e| format!("enviando GC_ITEM_UPDATE: {e}"))?;
                ItemRepo::new(&session.config.pg_conn)
                    .upsert(&session.inventory[src], session.row().id)
                    .await?;
            }
        } else {
            eprintln!(
                "server_realms: channel conn {}: celda {} ocupada con \
                 otro item — move ignorado (parity MoveItem)",
                session.conn_id, mv.change_pos.cell
            );
        }
        return Ok(Outcome::Continue);
    }
    // Destino VACÍO: split (0 < num < count) o mover todo.
    if want > 0 && want < session.inventory[src].count {
        // SPLIT (char_item.cpp:5747-5763): el origen baja (GC_ITEM_UPDATE) +
        // item nuevo en el destino (GC_ITEM_SET con id del rango
        // ITEM_ID_RANGE).
        session.inventory[src].count -= want;
        let up = protocol::world::TPacketGCItemUpdate {
            header: protocol::world::TPacketGCItemUpdate::HEADER,
            cell: TItemPos {
                window: TItemPos::WINDOW_INVENTORY,
                cell: mv.pos.cell,
            },
            count: session.inventory[src].count as u8,
            sockets: session.inventory[src].sockets,
            attrs: session.inventory[src].attrs,
        };
        session
            .send(&up.to_bytes())
            .await
            .map_err(|e| format!("enviando GC_ITEM_UPDATE: {e}"))?;
        ItemRepo::new(&session.config.pg_conn)
            .upsert(&session.inventory[src], session.row().id)
            .await?;
        let id = ItemRepo::new(&session.config.pg_conn)
            .max_id_in_range(100_000_000, 200_000_000)
            .await?
            .map(|m| m + 1)
            .unwrap_or(100_000_000);
        let new_item = database::item::ItemRow {
            id,
            window: "INVENTORY".to_string(),
            pos: mv.change_pos.cell as i32,
            count: want,
            vnum: session.inventory[src].vnum,
            sockets: session.inventory[src].sockets,
            attrs: session.inventory[src].attrs,
        };
        let set = TPacketGCItemSet {
            header: TPacketGCItemSet::HEADER,
            cell: TItemPos {
                window: TItemPos::WINDOW_INVENTORY,
                cell: mv.change_pos.cell,
            },
            vnum: new_item.vnum as u32,
            count: new_item.count as u8,
            flags: 0,
            anti_flags: 0,
            highlight: 0,
            sockets: new_item.sockets,
            attrs: new_item.attrs,
        };
        session
            .send(&set.to_bytes())
            .await
            .map_err(|e| format!("enviando GC_ITEM_SET: {e}"))?;
        ItemRepo::new(&session.config.pg_conn)
            .upsert(&new_item, session.row().id)
            .await?;
        session.inventory.push(new_item);
        eprintln!(
            "server_realms: channel conn {}: {} split de vnum {} \
             ({want} → celda {})",
            session.conn_id,
            session.row().name,
            session.inventory[src].vnum,
            mv.change_pos.cell
        );
    } else {
        // MOVER TODO (char_item.cpp:5733-5746): el item cambia de celda —
        // GC_ITEM_DEL (origen) + GC_ITEM_SET (destino) + upsert.
        let cell = TItemPos {
            window: TItemPos::WINDOW_INVENTORY,
            cell: mv.pos.cell,
        };
        let vnum = session.inventory[src].vnum;
        session
            .send(&TPacketGCItemDelDeprecated::new(
                cell,
                vnum as u32,
                session.inventory[src].count as u8,
            )
            .to_bytes())
            .await
            .map_err(|e| format!("enviando GC_ITEM_DEL: {e}"))?;
        session.inventory[src].pos = mv.change_pos.cell as i32;
        let set = TPacketGCItemSet {
            header: TPacketGCItemSet::HEADER,
            cell: TItemPos {
                window: TItemPos::WINDOW_INVENTORY,
                cell: mv.change_pos.cell,
            },
            vnum: session.inventory[src].vnum as u32,
            count: session.inventory[src].count as u8,
            flags: 0,
            anti_flags: 0,
            highlight: 0,
            sockets: session.inventory[src].sockets,
            attrs: session.inventory[src].attrs,
        };
        session
            .send(&set.to_bytes())
            .await
            .map_err(|e| format!("enviando GC_ITEM_SET: {e}"))?;
        ItemRepo::new(&session.config.pg_conn)
            .upsert(&session.inventory[src], session.row().id)
            .await?;
        eprintln!(
            "server_realms: channel conn {}: {} movió item vnum {} \
             (celda {} → {})",
            session.conn_id,
            session.row().name,
            session.inventory[src].vnum,
            mv.pos.cell,
            mv.change_pos.cell
        );
    }
    Ok(Outcome::Continue)
}
