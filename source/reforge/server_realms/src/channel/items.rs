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
    TPacketCGItemDrop, TPacketCGItemDrop2, TPacketCGItemUse, TPacketGCItemDelDeprecated,
    TPacketGCItemSet, TItemPos,
};
use game_core::ecs::{CombatIntent, Intent, ItemIntent};
use game_core::packets;

use crate::channel::session::{Outcome, Session};

/// `ITEM_TYPE_USE = 3` (ItemData.h:77 — el tipo consumible del wire).
const ITEM_TYPE_USE: i16 = 3;
/// `ITEM_TYPE_AUTOUSE = 4` (ItemData.h:78 — auto-poción, también consumible).
const ITEM_TYPE_AUTOUSE: i16 = 4;

/// El gate de consumibles (parity `UseItemEx`, char_item.cpp:1616+): SOLO
/// los items ITEM_TYPE_USE/AUTOUSE se aplican y consumen con CG_ITEM_USE.
fn is_consumable(b_type: i16) -> bool {
    b_type == ITEM_TYPE_USE || b_type == ITEM_TYPE_AUTOUSE
}
use crate::channel::{
    equipped_armor, quickslot, ITEM_COUNT_LIMIT, INVENTORY_MAX_NUM, WEAR_MAX_NUM,
};

/// `ITEM_GOLD_VNUM = 1` — el oro del suelo es el item vnum 1 (parity
/// `DropGold` char_item.cpp:5518-5540: `CreateItem(1, gold)`).
const ITEM_GOLD_VNUM: u32 = 1;

/// Clamp del count del DropItem (parity char_item.cpp:5424-5430: `bCount == 0
/// || bCount > item->GetCount()` → count del item).
fn drop_want(count: u8, stack: i64) -> i64 {
    if count == 0 || i64::from(count) > stack {
        stack
    } else {
        i64::from(count)
    }
}

/// CG_ITEM_DROP (12, 8 B: header + TItemPos + gold DWORD — Packet.h:566-570):
/// soltar un item del inventario o oro al suelo (lane D). Parity `ItemDrop`
/// (input_main.cpp:855-871): `gold > 0` → `DropGold` (item vnum 1 con
/// count = gold + `PointChange(POINT_GOLD, -gold)`); si no → `DropItem`.
pub async fn handle_drop(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let d = match TPacketCGItemDrop::from_bytes(pkt) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: CG_ITEM_DROP malformado: {e}",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    };
    drop_cell_or_gold(session, d.cell, d.gold, 0).await
}

/// CG_ITEM_DROP2 (20, 9 B: header + TItemPos + gold DWORD + count BYTE —
/// Packet.h:566-575): soltar con CANTIDAD. Parity `ItemDrop2`
/// (input_main.cpp:875-890): `gold > 0` → `DropGold`; si no →
/// `DropItem(Cell, count)`.
pub async fn handle_drop2(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let d = match TPacketCGItemDrop2::from_bytes(pkt) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: CG_ITEM_DROP2 malformado: {e}",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    };
    drop_cell_or_gold(session, d.cell, d.gold, d.count).await
}

/// Router del drop (parity ItemDrop/ItemDrop2): gold > 0 → DropGold;
/// si no → DropItem(Cell, count).
async fn drop_cell_or_gold(
    session: &mut Session,
    cell: TItemPos,
    gold: u32,
    count: u8,
) -> Result<Outcome, String> {
    if gold > 0 {
        return drop_gold(session, gold).await;
    }
    drop_item(session, cell, count).await
}

/// `DropGold(gold)` (parity char_item.cpp:5518-5540): crea el item vnum 1
/// (oro) con count = gold en la posición del jugador (el mundo asigna el vid
/// y el `DropResult` manda el GC_ITEM_GROUND_ADD + ownership) y descuenta
/// el oro (GC_POINTS + save). Rechazos: `gold <= 0` o `gold > GetGold()`
/// (el C++ devuelve false en silencio). El intent se envía ANTES de mutar
/// el oro (si el mundo está muerto, el send falla y no se pierde nada).
async fn drop_gold(session: &mut Session, gold: u32) -> Result<Outcome, String> {
    if i64::from(gold) > i64::from(session.row().gold) {
        eprintln!(
            "server_realms: channel conn {}: {} — drop de {gold} oro \
             con {} en el monedero — rechazado (parity DropGold)",
            session.conn_id,
            session.row().name,
            session.row().gold
        );
        return Ok(Outcome::Continue);
    }
    let (x, y) = (session.motion().x, session.motion().y);
    session.intent(Intent::Item(ItemIntent::DropItem {
        player_vid: session.player_vid(),
        vnum: ITEM_GOLD_VNUM,
        count: gold,
        x,
        y,
        z: 0,
    }))?;
    {
        let row = session.row_mut();
        // El gate de arriba garantiza `gold <= row.gold` (i32) — cast seguro.
        row.gold = row.gold.saturating_sub(gold as i32);
    }
    session
        .send(&packets::points_packet(session.row(), session.next_exp, &session.battle).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_POINTS (drop oro): {e}"))?;
    session.save();
    eprintln!(
        "server_realms: channel conn {}: {} soltó {gold} oro en el suelo \
         (vnum {ITEM_GOLD_VNUM}, {x},{y})",
        session.conn_id, session.row().name
    );
    Ok(Outcome::Continue)
}

/// `DropItem(Cell, count)` (parity char_item.cpp:5424-5516): valida
/// muerto/celda/item, clamp del count al count del stack, SyncQuickslot de
/// la celda (la barra rápida deja de referenciar el item soltado), quita del
/// inventario (GC_ITEM_UPDATE/DEL + upsert/delete) y crea el item en el
/// suelo con el intent `DropItem` del mundo (el `DropResult` →
/// GC_ITEM_GROUND_ADD + ownership — events.rs). Subset documentado: sin
/// gate de antiflag (ITEM_ANTIFLAG_DROP — el C++ chequea; el cliente ya
/// bloquea items protegidos en su UI), sin cheque/ENABLE_CHEQUE_SYSTEM.
async fn drop_item(
    session: &mut Session,
    cell: TItemPos,
    count: u8,
) -> Result<Outcome, String> {
    if session.row().hp <= 0 {
        eprintln!(
            "server_realms: channel conn {}: {} — drop con hp 0 \
             (muerto) — rechazado (parity IsDead)",
            session.conn_id, session.row().name
        );
        return Ok(Outcome::Continue);
    }
    if cell.window != TItemPos::WINDOW_INVENTORY || cell.cell >= INVENTORY_MAX_NUM {
        eprintln!(
            "server_realms: channel conn {}: drop de celda inválida \
             (window {} cell {}) — rechazado",
            session.conn_id, cell.window, cell.cell
        );
        return Ok(Outcome::Continue);
    }
    let Some(idx) = session
        .inventory
        .iter()
        .position(|i| i.window == "INVENTORY" && i.pos as u16 == cell.cell)
    else {
        eprintln!(
            "server_realms: channel conn {}: drop de celda {} sin item",
            session.conn_id, cell.cell
        );
        return Ok(Outcome::Continue);
    };
    // Clamp del count (parity: `bCount == 0 || bCount > item->GetCount()` →
    // count del item).
    let want = drop_want(count, session.inventory[idx].count);
    // SyncQuickslot del cell (parity char_item.cpp:5432): la barra rápida
    // deja de referenciar el item soltado (GC_QUICKSLOT_DEL por slot).
    let mut qblob = quickslot::blob(session.row());
    let cleared = quickslot::clear_item_refs(&mut qblob, cell.cell);
    if !cleared.is_empty() {
        for pos in &cleared {
            session
                .send(&protocol::world::TPacketGCQuickSlotDel::new(*pos).to_bytes())
                .await
                .map_err(|e| format!("enviando GC_QUICKSLOT_DEL: {e}"))?;
        }
        session.row_mut().quickslot = Some(qblob);
    }
    // El INTENT primero (el mundo asigna el vid; el send falla → no se
    // muta el inventario).
    let (x, y) = (session.motion().x, session.motion().y);
    let vnum = session.inventory[idx].vnum;
    session.intent(Intent::Item(ItemIntent::DropItem {
        player_vid: session.player_vid(),
        vnum: vnum as u32,
        count: want as u32,
        x,
        y,
        z: 0,
    }))?;
    // Quitar del inventario (parity RemoveFromCharacter + SetCount).
    session.inventory[idx].count -= want;
    if session.inventory[idx].count <= 0 {
        let id = session.inventory[idx].id;
        session
            .send(&TPacketGCItemDelDeprecated::new(
                TItemPos {
                    window: TItemPos::WINDOW_INVENTORY,
                    cell: cell.cell,
                },
                0,
                0,
            )
            .to_bytes())
            .await
            .map_err(|e| format!("enviando GC_ITEM_DEL: {e}"))?;
        ItemRepo::new(session.pool.clone()).delete(id).await?;
        session.inventory.remove(idx);
    } else {
        let up = protocol::world::TPacketGCItemUpdate {
            header: protocol::world::TPacketGCItemUpdate::HEADER,
            cell: TItemPos {
                window: TItemPos::WINDOW_INVENTORY,
                cell: cell.cell,
            },
            count: session.inventory[idx].count as u8,
            sockets: session.inventory[idx].sockets,
            attrs: session.inventory[idx].attrs,
        };
        session
            .send(&up.to_bytes())
            .await
            .map_err(|e| format!("enviando GC_ITEM_UPDATE: {e}"))?;
        ItemRepo::new(session.pool.clone())
            .upsert(&session.inventory[idx], session.row().id)
            .await?;
    }
    session.save();
    eprintln!(
        "server_realms: channel conn {}: {} soltó item vnum {vnum} \
         (×{want}, celda {}) en {x},{y}",
        session.conn_id, session.row().name, cell.cell
    );
    Ok(Outcome::Continue)
}

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
    // Buscar el item en INVENTORY o EQUIPMENT por (window, cell). El
    // doble-click en un item equipado manda el cell del equip (180+wear con
    // window INVENTORY o EQUIPMENT — parity `GetItem(Cell)` acepta ambos,
    // char_item.cpp:5246) y es un TOGGLE: si está equipado → desequipa
    // (fix 2026-08-15 — antes "uso de celda 182 sin item").
    let equip_window = is_equip_position(item_use.pos);
    let Some(idx) = session.inventory.iter().position(|i| {
        let matches_win = if equip_window {
            // El cell 180+wear del cliente puede venir con window INVENTORY
            // o EQUIPMENT — aceptar ambos (parity IsEquipPosition).
            i.window == "EQUIPMENT" || i.window == "INVENTORY"
        } else {
            i.window == "INVENTORY"
        };
        matches_win && i.pos as u16 == item_use.pos.cell
    }) else {
        eprintln!(
            "server_realms: channel conn {}: uso de celda {} sin item",
            session.conn_id, item_use.pos.cell
        );
        return Ok(Outcome::Continue);
    };
    let Some(proto) = ItemRepo::new(session.pool.clone())
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
    // TOGGLE del doble-click (parity UseItemEx char_item.cpp:1874-1938: si
    // el item está EQUIPADO → UnequipItem, si está en INVENTORY →
    // EquipItem). Fix 2026-08-15: antes el doble-click en equipado daba
    // "uso de celda 182 sin item".
    if equip_window && session.inventory[idx].window == "EQUIPMENT" {
        // Desequipar: EQUIPMENT → INVENTORY con el cell del cliente
        // (180+wear). Se reutiliza el path del CG_ITEM_MOVE (desequipar —
        // validación + wire + parts + persistencia).
        let synthesized = protocol::world::TPacketCGItemMove {
            header: protocol::world::TPacketCGItemMove::HEADER,
            pos: TItemPos {
                window: TItemPos::WINDOW_EQUIPMENT,
                cell: item_use.pos.cell,
            },
            change_pos: TItemPos {
                window: TItemPos::WINDOW_INVENTORY,
                cell: (0..INVENTORY_MAX_NUM)
                    .find(|c| {
                        !session.inventory.iter().any(|i| {
                            i.window == "INVENTORY" && i.pos as u16 == *c
                        })
                    })
                    .unwrap_or(0),
            },
            num: 0,
        };
        eprintln!(
            "server_realms: channel conn {}: doble-click item vnum {} → \
             DESEQUIPAR (toggle, celda {})",
            session.conn_id, session.inventory[idx].vnum, item_use.pos.cell
        );
        return handle_move(session, &synthesized.to_bytes()).await;
    }
    // EQUIPAR por doble-click (FIX 2026-08-14 — "no puedo usar las dagas ni
    // las botas"): parity `UseItemEx` → `EquipItem` (char_item.cpp:1874-1938 —
    // el switch por tipo EQUIPA armas/armaduras/costume; el consumo es SOLO
    // para consumibles USE/AUTOUSE). El gate del wave 7 rechazaba los no
    // consumibles y rompió el equip por doble-click. Se reutiliza el path del
    // CG_ITEM_MOVE (equip INVENTORY→EQUIPMENT, validación + wire + parts +
    // persistencia) sintetizando el paquete con el slot COMPUTADO
    // (`FindEquipCell` — item.cpp:509-623). GAP documentado: el slot ocupado
    // se RECHAZA (el C++ desequipa el actual y hace swap — pendiente).
    if !is_consumable(proto.b_type) {
        let Some(slot) = packets::find_equip_cell(&proto) else {
            eprintln!(
                "server_realms: channel conn {}: item vnum {} type {} no equipable \
                 (wearflag 0 o fuera del subset) — uso ignorado",
                session.conn_id, session.inventory[idx].vnum, proto.b_type
            );
            return Ok(Outcome::Continue);
        };
        let synthesized = protocol::world::TPacketCGItemMove {
            header: protocol::world::TPacketCGItemMove::HEADER,
            pos: TItemPos {
                window: TItemPos::WINDOW_INVENTORY,
                cell: item_use.pos.cell,
            },
            change_pos: TItemPos {
                window: TItemPos::WINDOW_EQUIPMENT,
                cell: INVENTORY_MAX_NUM + slot,
            },
            num: 0,
        };
        eprintln!(
            "server_realms: channel conn {}: doble-click item vnum {} type {} → \
             equip (wear {slot})",
            session.conn_id, session.inventory[idx].vnum, proto.b_type
        );
        return handle_move(session, &synthesized.to_bytes()).await;
    }
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
        .send(&packets::points_packet(session.row(), session.next_exp, &session.battle).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_POINTS: {e}"))?;
    session.save();
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
            .send(&TPacketGCItemDelDeprecated::new(cell, 0, 0).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_ITEM_DEL: {e}"))?;
        ItemRepo::new(session.pool.clone()).delete(id).await?;
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
        ItemRepo::new(session.pool.clone())
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
/// Parity `SItemPos::IsEquipPosition` (length.h:825-830): una posición es
/// de equip si window ∈ {INVENTORY, EQUIPMENT} Y cell ∈ [INVENTORY_MAX_NUM,
/// INVENTORY_MAX_NUM + WEAR_MAX_NUM). El drag-equip del cliente llega como
/// INVENTORY→INVENTORY con cell destino = 180+wear (no EQUIPMENT); el
/// doble-click como INVENTORY→EQUIPMENT. Ambos deben equipar (bug 2026-08-15).
fn is_equip_position(p: TItemPos) -> bool {
    (p.window == TItemPos::WINDOW_INVENTORY || p.window == TItemPos::WINDOW_EQUIPMENT)
        && p.cell >= INVENTORY_MAX_NUM
        && p.cell < INVENTORY_MAX_NUM + WEAR_MAX_NUM
}

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
    // Parity `SItemPos::IsEquipPosition` (length.h:825-830) — ver
    // `is_equip_position` abajo (bug 2026-08-15: el drag-equip manda
    // INVENTORY→INVENTORY con cell = 180+wear; el doble-click EQUIPMENT).
    // Subset de windows: INVENTORY→INVENTORY (mover/stack/split),
    // INVENTORY→equip (equipar — parity `EquipItem` char_item.cpp:6128;
    // wire: el cell del EQUIPMENT = INVENTORY_MAX_NUM + wear, length.h:827)
    // y equip→INVENTORY (desequipar). Belt/DS fuera.
    let equipping = is_equip_position(mv.change_pos);
    let unequipping = is_equip_position(mv.pos);
    let inv_to_inv = !is_equip_position(mv.pos)
        && !is_equip_position(mv.change_pos)
        && mv.pos.window == TItemPos::WINDOW_INVENTORY
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
        let Some(proto) = ItemRepo::new(session.pool.clone())
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
            .send(&TPacketGCItemDelDeprecated::new(cell, 0, 0).to_bytes())
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
        ItemRepo::new(session.pool.clone())
            .upsert(&session.inventory[src], session.row().id)
            .await?;
        // El CHARACTER_UPDATE (header 19) con los parts COMPUTADOS (parity
        // `UpdatePacket` — char.cpp:1017-1052; el C++ lo manda en el EquipTo,
        // item.cpp:1004-1005): el cliente recalcula el daño del arma
        // (ATT_MIN/ATT_MAX — `__SetWeaponPower` lee value3/value4 del item
        // por el part del arma) y refresca la ventana
        // (`__RecvCharacterUpdatePacket` → `__SetWeaponPower` +
        // `__RefreshStatus`). El ADDITIONAL_INFO (136) NO vale aquí: es el
        // paquete de la secuencia de ENTRADA (el cliente lo aplica solo si el
        // VID coincide con el `s_kNetActorData` pendiente —
        // PythonNetworkStreamPhaseGameActor.cpp:153,165).
        let parts = packets::equipped_parts(session.row(), &session.inventory);
        // dw_arrow = VNUM del item en WEAR_ARROW (parity UpdatePacket
        // char.cpp:1046: `GetWear(WEAR_ARROW)->GetOriginalVnum()`; el cliente
        // SetArrow lo resuelve como vnum — InstanceBase.cpp:3354). Fix
        // 2026-08-15 (verifier): antes mandaba el count -> carcaj nunca visible.
        let arrows = super::equipped_arrow_index(&session.inventory)
            .map(|i| session.inventory[i].vnum as u32)
            .unwrap_or(0);
        // C27 (velocidad de botas): re-computar la velocidad con el equipo
        // NUEVO (el apply APPLY_MOV_SPEED de la bota — parity ModifyPoints
        // item.cpp:718-735) y mandarla en el UPDATE (b_moving_speed).
        let boots = super::equipped_boots_proto(&session.pool, &session.inventory).await?;
        session.mov_speed = packets::mov_speed_for_boots(boots.as_ref());
        session
            .send(&packets::character_update_with_parts(
                session.row(),
                &parts,
                arrows,
                session.mov_speed,
            )
            .to_bytes())
            .await
            .map_err(|e| format!("enviando GC_CHARACTER_UPDATE (equip): {e}"))?;
        // El iArmor del mundo COMPARTIDO (el ataque del mob usa
        // `player_def_grade` con la armadura del equipo — solo cambia al
        // equipar/desequipar).
        let armor = equipped_armor(&session.inventory, &session.pool).await?;
        session.intent(Intent::Combat(CombatIntent::SetArmor {
            player_vid: session.player_vid(),
            armor,
        }))?;
        // Battle points RE-COMPUTADOS con el equipo nuevo (parity: el C++
        // tras EquipItem hace ComputePoints/ComputeBattlePoints → el
        // PointsPacket — char_item.cpp:6309+): la ventana del cliente muestra
        // el ataque (daño del arma) y la defensa (level+HT+armor) nuevos.
        let weapon_proto = super::equipped_weapon_proto(&session.pool, &session.inventory).await?;
        session.battle =
            packets::compute_battle_points(session.row(), weapon_proto.as_ref(), armor);
        session
            .send(&packets::points_packet(session.row(), session.next_exp, &session.battle).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_POINTS (equip): {e}"))?;
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
            .send(&TPacketGCItemDelDeprecated::new(cell, 0, 0).to_bytes())
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
        ItemRepo::new(session.pool.clone())
            .upsert(&session.inventory[src], session.row().id)
            .await?;
        // CHARACTER_UPDATE con los parts COMPUTADOS (el arma/armadura ya no
        // está — el part se quita; parity del C++: `CItem::Unequip` →
        // ComputeBattlePoints + UpdatePacket, item.cpp).
        let parts = packets::equipped_parts(session.row(), &session.inventory);
        // dw_arrow = VNUM (parity char.cpp:1046/937 — GetOriginalVnum).
        let arrows = super::equipped_arrow_index(&session.inventory)
            .map(|i| session.inventory[i].vnum as u32)
            .unwrap_or(0);
        // C27: si se desequipó la BOTA, la velocidad vuelve a 100 (parity
        // `CItem::Unequip` → `ModifyPoints(false)` — el apply se quita).
        let boots = super::equipped_boots_proto(&session.pool, &session.inventory).await?;
        session.mov_speed = packets::mov_speed_for_boots(boots.as_ref());
        session
            .send(&packets::character_update_with_parts(
                session.row(),
                &parts,
                arrows,
                session.mov_speed,
            )
            .to_bytes())
            .await
            .map_err(|e| format!("enviando GC_CHARACTER_UPDATE (desequip): {e}"))?;
        // El iArmor del mundo COMPARTIDO baja con el item quitado.
        let armor = equipped_armor(&session.inventory, &session.pool).await?;
        session.intent(Intent::Combat(CombatIntent::SetArmor {
            player_vid: session.player_vid(),
            armor,
        }))?;
        // Battle points RE-COMPUTADOS (el arma/armadura ya no está — la
        // ventana del cliente baja el ataque/defensa; parity del C++ tras
        // UnequipItem → ComputeBattlePoints → PointsPacket).
        let weapon_proto = super::equipped_weapon_proto(&session.pool, &session.inventory).await?;
        session.battle =
            packets::compute_battle_points(session.row(), weapon_proto.as_ref(), armor);
        session
            .send(&packets::points_packet(session.row(), session.next_exp, &session.battle).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_POINTS (desequip): {e}"))?;
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
            ItemRepo::new(session.pool.clone())
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
                let _vnum = session.inventory[src].vnum;
                let id = session.inventory[src].id;
                session
                    .send(&TPacketGCItemDelDeprecated::new(cell, 0, 0).to_bytes())
                    .await
                    .map_err(|e| format!("enviando GC_ITEM_DEL: {e}"))?;
                ItemRepo::new(session.pool.clone()).delete(id).await?;
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
                ItemRepo::new(session.pool.clone())
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
        ItemRepo::new(session.pool.clone())
            .upsert(&session.inventory[src], session.row().id)
            .await?;
        let id = ItemRepo::new(session.pool.clone())
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
        ItemRepo::new(session.pool.clone())
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
        let _vnum = session.inventory[src].vnum;
        session
            .send(&TPacketGCItemDelDeprecated::new(cell, 0, 0).to_bytes())
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
        ItemRepo::new(session.pool.clone())
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
#[cfg(test)]
mod tests {
    use super::*;

    /// El gate del uso (fix 2026-08-14): SOLO consumibles (USE=3/AUTOUSE=4)
    /// pasan — armas (1)/armaduras (2) se rechazan (el doble-click se las
    /// comía: la daga 1007 se borró del slot; las botas 15005 "sin efecto").
    #[test]
    fn use_gate_only_consumables() {
        assert!(is_consumable(3), "ITEM_TYPE_USE (poción)");
        assert!(is_consumable(4), "ITEM_TYPE_AUTOUSE");
        assert!(!is_consumable(1), "arma — NO se consume");
        assert!(!is_consumable(2), "armadura — NO se consume");
        assert!(!is_consumable(0), "NONE — NO se consume");
    }

    /// Fix bug 2026-08-15: el drag-equip del cliente llega como
    /// INVENTORY→INVENTORY con cell destino = 180+wear; el doble-click como
    /// INVENTORY→EQUIPMENT. Ambos deben reconocerse como posición de equip
    /// (parity `SItemPos::IsEquipPosition`, length.h:825-830).
    #[test]
    fn equip_position_accepts_drag_and_double_click() {
        let p = |w: u8, cell: u16| TItemPos { window: w, cell };
        let inv = TItemPos::WINDOW_INVENTORY;
        let eqp = TItemPos::WINDOW_EQUIPMENT;
        // Drag-equip: INVENTORY con cell = 180 + wear (ej. wear 4 → 184).
        assert!(is_equip_position(p(inv, INVENTORY_MAX_NUM + 4)), "drag (INV 184)");
        // Doble-click: EQUIPMENT con el mismo cell.
        assert!(is_equip_position(p(eqp, INVENTORY_MAX_NUM + 4)), "doble-click (EQP 184)");
        // Último slot de wear válido (212-1).
        assert!(is_equip_position(p(inv, INVENTORY_MAX_NUM + WEAR_MAX_NUM - 1)), "wear 31");
        // Fuera de rango: celda de inventario normal, y celda > 180+32.
        assert!(!is_equip_position(p(inv, 7)), "inv normal 7");
        assert!(!is_equip_position(p(inv, INVENTORY_MAX_NUM + WEAR_MAX_NUM)), "cell 212");
        // Otras ventanas (p. ej. SAFEBOX) nunca son equip.
        assert!(!is_equip_position(p(5, INVENTORY_MAX_NUM + 4)), "safebox");
    }

    /// Clamp del count del DropItem (parity char_item.cpp:5424-5430):
    /// count 0 o > stack → todo el stack; si no, el count pedido.
    #[test]
    fn drop_want_clamps_to_stack() {
        assert_eq!(drop_want(0, 5), 5, "count 0 → todo");
        assert_eq!(drop_want(3, 5), 3, "count válido");
        assert_eq!(drop_want(9, 5), 5, "count > stack → todo");
        assert_eq!(drop_want(0, 1), 1);
        assert_eq!(drop_want(200, 0), 0, "stack vacío → 0 (el gate de arriba ya rechazó)");
    }

    /// El oro del suelo es el item vnum 1 (parity `DropGold`
    /// char_item.cpp:5534 — `CreateItem(1, gold)`).
    #[test]
    fn gold_drop_uses_vnum_1() {
        assert_eq!(ITEM_GOLD_VNUM, 1);
    }
}