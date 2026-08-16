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

use database::affect::{AffectRepo, AffectRow};
use database::item::ItemRepo;
use protocol::world::{
    RefineMaterial, TPacketAffectElement, TPacketCGItemDrop, TPacketCGItemDrop2,
    TPacketCGItemUse, TPacketCGItemUseToItem, TPacketCGRefine,
    TPacketGCAffectAdd, TPacketGCItemDelDeprecated, TPacketGCItemSet,
    TPacketGCRefineInformation, TItemPos,
};
use game_core::ecs::{CombatIntent, Intent, ItemIntent};
use game_core::packets;

use crate::channel::session::{Outcome, Session};

/// `ITEM_TYPE_USE = 3` (ItemData.h:77 — el tipo consumible del wire).
const ITEM_TYPE_USE: i16 = 3;
/// `ITEM_TYPE_AUTOUSE = 4` (ItemData.h:78 — auto-poción, también consumible).
const ITEM_TYPE_AUTOUSE: i16 = 4;

// ---------------------------------------------------------------------------
// Subtipos USE_* del lane (parity `EUseSubTypes`/`EAutoUseSubTypes`,
// item_length.h:250-300): el switch del C++ `UseItem` por `GetSubType()`
// (char_item.cpp:4172+). El número del wire es el ORDEN del enum legacy.
// ---------------------------------------------------------------------------

/// `USE_TREASURE_BOX = 4` (item_length.h:255) — los cofres.
const USE_TREASURE_BOX: i16 = 4;
/// `USE_ABILITY_UP = 7` (item_length.h:258) — las pociones de buff
/// (char_item.cpp:4332-4388).
const USE_ABILITY_UP: i16 = 7;
/// `AUTOUSE_GOLD = 3` (item_length.h:298) — las bolsas de oro.
const AUTOUSE_GOLD: i16 = 3;

// ---------------------------------------------------------------------------
// USE_ABILITY_UP: value0 = índice EApplyTypes (length.h:354-405 — el MISMO
// catálogo de `database::attr::APPLY_NAMES`), value1 = duración (segundos),
// value2 = cantidad (char_item.cpp:4332-4388). Solo los applies del switch
// del C++ tienen case.
// ---------------------------------------------------------------------------

/// `APPLY_CON = 3` .. `APPLY_DEF_GRADE_BONUS = 54` (length.h:354-405).
const APPLY_CON: i32 = 3;
const APPLY_INT: i32 = 4;
const APPLY_STR: i32 = 5;
const APPLY_DEX: i32 = 6;
const APPLY_ATT_SPEED: i32 = 7;
const APPLY_MOV_SPEED: i32 = 8;
const APPLY_CAST_SPEED: i32 = 9;
const APPLY_ATT_GRADE_BONUS: i32 = 53;
const APPLY_DEF_GRADE_BONUS: i32 = 54;

/// `AFFECT_MOV_SPEED = 200` .. `AFFECT_DEF_GRADE = 226` (affect.h:22-54 —
/// `EAffectTypes`): el dwType del affect (el icono del cliente).
const AFFECT_MOV_SPEED: u32 = 200;
const AFFECT_ATT_SPEED: u32 = 201;
const AFFECT_ATT_GRADE: u32 = 202;
const AFFECT_STR: u32 = 204;
const AFFECT_DEX: u32 = 205;
const AFFECT_CON: u32 = 206;
const AFFECT_INT: u32 = 207;
const AFFECT_CAST_SPEED: u32 = 217;
const AFFECT_DEF_GRADE: u32 = 226;

/// `AFF_MOV_SPEED_POTION = 12` / `AFF_ATT_SPEED_POTION = 13` (affect.h:137-138
/// — `EAffectBits`, valores del enum legacy) — el dwFlag del affect (solo
/// los buffs de velocidad llevan flag; el resto 0, parity literal del C++).
const AFF_MOV_SPEED_POTION: u32 = 12;
const AFF_ATT_SPEED_POTION: u32 = 13;

/// `POINT_ST = 12` .. `POINT_IQ = 15` (char.h:148-151) — los POINT_* de las
/// stats que `game_core::skill::point` no cubre (los demás POINT_* de este
/// lane viven ahí: ATT_SPEED 17 / MOV_SPEED 19 / CASTING_SPEED 21 /
/// ATT_GRADE_BONUS 95 / DEF_GRADE_BONUS 96).
const POINT_ST: u8 = 12;
const POINT_HT: u8 = 13;
const POINT_DX: u8 = 14;
const POINT_IQ: u8 = 15;

/// `GOLD_MAX = 2000000000` (length.h:80) — el cap del oro del PointChange.
const GOLD_MAX: i64 = 2_000_000_000;

/// Mapeo del switch USE_ABILITY_UP (parity char_item.cpp:4332-4388):
/// value0 (APPLY_*) → (AFFECT_*, POINT_*, AFF_*). `None` = apply sin case
/// en el C++ → sin buff y SIN consumo.
fn ability_up_apply(apply: i32) -> Option<(u32, u8, u32)> {
    match apply {
        APPLY_MOV_SPEED => Some((
            AFFECT_MOV_SPEED,
            game_core::skill::point::MOV_SPEED,
            AFF_MOV_SPEED_POTION,
        )),
        APPLY_ATT_SPEED => Some((
            AFFECT_ATT_SPEED,
            game_core::skill::point::ATT_SPEED,
            AFF_ATT_SPEED_POTION,
        )),
        APPLY_STR => Some((AFFECT_STR, POINT_ST, 0)),
        APPLY_DEX => Some((AFFECT_DEX, POINT_DX, 0)),
        APPLY_CON => Some((AFFECT_CON, POINT_HT, 0)),
        APPLY_INT => Some((AFFECT_INT, POINT_IQ, 0)),
        APPLY_CAST_SPEED => Some((AFFECT_CAST_SPEED, game_core::skill::point::CASTING_SPEED, 0)),
        APPLY_ATT_GRADE_BONUS => Some((AFFECT_ATT_GRADE, game_core::skill::point::ATT_GRADE_BONUS, 0)),
        APPLY_DEF_GRADE_BONUS => Some((AFFECT_DEF_GRADE, game_core::skill::point::DEF_GRADE_BONUS, 0)),
        _ => None,
    }
}

/// Cap del oro de la bolsa AUTOUSE_GOLD (parity `PointChange(POINT_GOLD)` —
/// el C++ clamp a GOLD_MAX; el row.gold es i32 y no puede excederlo).
fn gold_after_add(current: i32, add: i32) -> i32 {
    (i64::from(current) + i64::from(add)).min(GOLD_MAX) as i32
}

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
        sockets: [0; 3],
        attrs: [(0, 0); 7],
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
        // El item del suelo conserva attrs/sockets del row (parity: el CItem
        // soltado los mantiene — el pickup los devuelve al inventario).
        sockets: session.inventory[idx].sockets,
        attrs: session.inventory[idx].attrs,
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
    // Dispatch por SUBTIPO (parity `UseItem` — switch(item->GetSubType()),
    // char_item.cpp:4172+): los subtipos con semántica PROPIA se manejan
    // ANTES del heal genérico (el value0 de un buff es un APPLY_* — no HP
    // flat; el de la bolsa de oro es la cantidad — tampoco).
    match proto.b_sub_type {
        s if s == USE_ABILITY_UP && proto.b_type == ITEM_TYPE_USE => {
            return use_ability_up(session, idx, &proto).await;
        }
        s if s == USE_TREASURE_BOX && proto.b_type == ITEM_TYPE_USE => {
            // Parity LITERAL: el USE_TREASURE_BOX por doble-click es NO-OP
            // (char_item.cpp:4971-4973 — `case USE_MOVE: case
            // USE_TREASURE_BOX: case USE_MONEYBAG: break;` — SIN consumo).
            // El cofre se abre con la LLAVE (ITEM_TREASURE_KEY, UseItemEx
            // char_item.cpp:1968-2051) que tira del grupo especial
            // (`special_item_group.txt` — loader item_manager_read_tables
            // .cpp:306; sin tablas PG). GAP documentado: el sistema de
            // grupos no existe en el rewrite — gap parcial del lane.
            eprintln!(
                "server_realms: channel conn {}: item vnum {} — cofre \
                 USE_TREASURE_BOX sin abrir (parity: no-op sin consumo; la \
                 apertura es llave+cofre vía el grupo especial — gap)",
                session.conn_id, session.inventory[idx].vnum
            );
            return Ok(Outcome::Continue);
        }
        s if s == AUTOUSE_GOLD && proto.b_type == ITEM_TYPE_AUTOUSE => {
            return use_autouse_gold(session, idx, &proto).await;
        }
        _ => {}
    }
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
    consume_one_use(session, idx).await?;
    Ok(Outcome::Continue)
}

/// Consume 1 del stack de un consumible usado (parity `item->SetCount(
/// count-1)`, char_item.cpp): count-1 → GC_ITEM_UPDATE (38 B) + upsert si
/// queda; GC_ITEM_DEL deprecated (42 B) + delete si se agota.
async fn consume_one_use(session: &mut Session, idx: usize) -> Result<(), String> {
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
    Ok(())
}

/// USE_ABILITY_UP (7 — item_length.h:258): las pociones de buff (parity
/// char_item.cpp:4332-4388). value0 = APPLY_* → (AFFECT_*, POINT_*, AFF_*),
/// value1 = duración en segundos, value2 = cantidad. El buff se aplica con
/// el sistema de affects EXISTENTE: override del mismo (type, applyOn) con
/// GC_AFFECT_REMOVE + GC_AFFECT_ADD (parity `AddAffect` bOverride=true,
/// char_affect.cpp:518-590), se guarda en session.affects + PG (parity
/// TPacketGDAddAffect), y los numéricos entran al componente `Affects` del
/// MUNDO (el combate lee ATT_SPEED/ATT_GRADE_BONUS/DEF_GRADE_BONUS/CRIT de
/// ahí; el `affects_system` los expira → AffectRemoved). MOV_SPEED además
/// recalcula la velocidad del motion (mismo cálculo que el buff de skill en
/// events.rs). Apply sin case en el C++ → sin buff y SIN consumo.
async fn use_ability_up(
    session: &mut Session,
    idx: usize,
    proto: &database::item::ProtoItem,
) -> Result<Outcome, String> {
    let Some((dw_type, point, flag)) = ability_up_apply(proto.values[0]) else {
        eprintln!(
            "server_realms: channel conn {}: item vnum {} — pocion de buff \
             con apply {} fuera del switch USE_ABILITY_UP — sin efecto, no \
             consume (parity)",
            session.conn_id, session.inventory[idx].vnum, proto.values[0]
        );
        return Ok(Outcome::Continue);
    };
    // value1 = duración (segundos); 0 → 1 (parity AddAffect :529-532 — el
    // C++ clamp a 1 en vez de rechazar).
    let duration = proto.values[1].max(1);
    let amount = proto.values[2];
    // Override del mismo (dwType, bApplyOn) — parity `FindAffect(dwType,
    // bApplyOn)` + SendAffectRemovePacket (char_affect.cpp:541-547).
    let mut overridden = false;
    session.affects.retain(|a| {
        if a.b_type == dw_type as i32 && a.b_apply_on == point as i16 {
            overridden = true;
            false
        } else {
            true
        }
    });
    if overridden {
        // GC_AFFECT_REMOVE (127, 6 B: header + dwType + bApplyOn — mismo
        // patrón crudo que events.rs).
        let mut out = Vec::with_capacity(6);
        out.push(127);
        out.extend_from_slice(&dw_type.to_le_bytes());
        out.push(point);
        session
            .send(&out)
            .await
            .map_err(|e| format!("enviando GC_AFFECT_REMOVE (override): {e}"))?;
    }
    // GC_AFFECT_ADD (126, 22 B) — el icono del buff en el cliente.
    session
        .send(
            &TPacketGCAffectAdd::new(TPacketAffectElement {
                dw_type,
                b_apply_on: point,
                l_apply_value: amount,
                dw_flag: flag,
                l_duration: duration,
                l_sp_cost: 0,
            })
            .to_bytes(),
        )
        .await
        .map_err(|e| format!("enviando GC_AFFECT_ADD: {e}"))?;
    // Mirror de la sesión + persistencia (parity TPacketGDAddAffect →
    // QUERY_ADD_AFFECT — ClientManagerPlayer.cpp:1150-1160).
    let row = AffectRow {
        dw_pid: session.row().id,
        b_type: dw_type as i32,
        b_apply_on: point as i16,
        l_apply_value: amount,
        dw_flag: i64::from(flag),
        l_duration: duration,
        l_sp_cost: 0,
    };
    session.affects.push(row.clone());
    AffectRepo::new(session.pool.clone()).save(&row).await?;
    // MOV_SPEED: el buff SUMA al factor POINT_MOV_SPEED — recalcular la
    // velocidad real del motion (parity GetMoveSpeed; mismo bloque que el
    // buff de skill en events.rs).
    if point == game_core::skill::point::MOV_SPEED {
        let total: i32 = 100
            + session
                .affects
                .iter()
                .filter(|a| a.b_apply_on == game_core::skill::point::MOV_SPEED as i16)
                .map(|a| a.l_apply_value)
                .sum::<i32>();
        let dur = game_core::ai::calculate_duration(total, 10_000);
        session.motion_mut().speed =
            (300u32.saturating_mul(10_000) / dur.max(1) as u32).max(1);
    }
    // El buff entra al MUNDO (componente `Affects` del jugador — el combate
    // lee los numéricos de ahí; el affects_system lo expira).
    session.intent(Intent::Combat(CombatIntent::SetAffect {
        player_vid: session.player_vid(),
        dw_type,
        point,
        value: amount,
        flag,
        duration_secs: duration,
    }))?;
    consume_one_use(session, idx).await?;
    eprintln!(
        "server_realms: channel conn {}: {} se buffeó con item vnum {} \
         (apply {}, point {}, +{amount} durante {duration}s, flag {flag})",
        session.conn_id, session.row().name, session.inventory[idx].vnum, proto.values[0], point
    );
    Ok(Outcome::Continue)
}

/// AUTOUSE_GOLD (3 — item_length.h:298): las bolsas de oro. VERIFICADO en
/// el C++ congelado: el camino ITEM_AUTOUSE de `UseItem` es NO-OP
/// (char_item.cpp:5152-5155) y el oro real de item vive en ITEM_ELK_VNUM
/// (50026, USE_SPECIAL, socket0 — char_item.cpp:3788-3793, fuera del
/// subset). Este lane implementa el contrato del item: value0 = oro →
/// gold += value0 (cap GOLD_MAX — length.h:80) + GC_POINTS + consumir.
async fn use_autouse_gold(
    session: &mut Session,
    idx: usize,
    proto: &database::item::ProtoItem,
) -> Result<Outcome, String> {
    let amount = proto.values[0];
    if amount <= 0 {
        eprintln!(
            "server_realms: channel conn {}: bolsa de oro vnum {} con \
             value0 {amount} — sin oro, no consume",
            session.conn_id, session.inventory[idx].vnum
        );
        return Ok(Outcome::Continue);
    }
    session.row_mut().gold = gold_after_add(session.row().gold, amount);
    session
        .send(&packets::points_packet(session.row(), session.next_exp, &session.battle).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_POINTS (bolsa de oro): {e}"))?;
    session.save();
    consume_one_use(session, idx).await?;
    eprintln!(
        "server_realms: channel conn {}: {} abrió la bolsa de oro vnum {} \
         (+{amount} oro, total {})",
        session.conn_id,
        session.row().name,
        session.inventory[idx].vnum,
        session.row().gold
    );
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

// ---------------------------------------------------------------------------
// REFINE / UPGRADE (lane R — parity char_item.cpp:1218-1345 + input_main
// .cpp:2831-2900): el item sube de nivel por tabla (`refine_proto`).
// ---------------------------------------------------------------------------

/// `USE_TUNING = 2` (ItemData.h:253) — el subtipo de los SCROLLS de refine
/// (el único camino del CG_ITEM_USE_TO_ITEM que abre la ventana).
const USE_TUNING: i16 = 2;
/// `MUSIN_SCROLL`/`BDRAGON_SCROLL` value0 (char_item.cpp:1322-1334) — el
/// gate del BDRAGON exige refine_set 702; el resto va por el camino SCROLL
/// genérico (501 rechazado, char_item.cpp:1341-1343).
const BDRAGON_SCROLL: i32 = 6;
/// `REFINE_SET_501` — los items de esa línea se rechazan con scroll genérico
/// (char_item.cpp:1341).
const REFINE_SET_501: i64 = 501;
/// `REFINE_SET_702` — el set exigido por el BDRAGON_SCROLL
/// (char_item.cpp:1329-1331).
const REFINE_SET_702: i64 = 702;
/// `number(1, 100)` del C++ — el roll del refine (parity `number(1, 100)`
/// char_item.cpp:919). Determinista sin dependencias (patrón `rand32` de
/// mod.rs — nanos + contador).
fn roll_1_100() -> i32 {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let c = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    ((nanos ^ c.wrapping_mul(0x9E37_79B9_7F4A_7C15)) % 100) as i32 + 1
}

/// Load del (refine_set, refined_vnum) + receta — el gate común del refine
/// (parity `GetRefineSet`/`GetRefinedVnum` item.h:137,157 + `GetRefineRecipe`
/// refine.h:30). `None` = el vnum no refina (sin fila, sin receta o sin
/// siguiente nivel).
async fn load_refine(
    session: &Session,
    vnum: i64,
) -> Result<Option<(database::item::RefineRecipe, i64)>, String> {
    let repo = ItemRepo::new(session.pool.clone());
    let Some((refine_set, refined_vnum)) = repo.load_refine_proto(vnum).await? else {
        return Ok(None);
    };
    if refined_vnum == 0 {
        return Ok(None);
    }
    let Some(recipe) = repo.load_refine_recipe(refine_set).await? else {
        return Ok(None);
    };
    Ok(Some((recipe, refined_vnum)))
}

/// Conteo de un material por vnum en TODO el inventario (parity
/// `CountSpecifyItem(char_item.cpp:926-936)` con el skipList del @fixme346 —
/// el item destino y el scroll NO se cuentan).
fn count_material(inventory: &[database::item::ItemRow], vnum: i64, skip: &[usize]) -> i64 {
    inventory
        .iter()
        .enumerate()
        .filter(|(i, r)| !skip.contains(i) && r.vnum == vnum)
        .map(|(_, r)| r.count)
        .sum()
}

/// Cobra la fee del refine (parity `PayRefineFee` char.cpp:6616 sin guild →
/// `PointChange(POINT_GOLD, -fee)` — iRemain = fee entero): descuenta el oro,
/// manda GC_POINTS y persiste. `fee` ya incluye el ×5 del `ComputeRefineFee`
/// cuando corresponde (el NORMAL cobra cost×5; el SCROLL cobra cost —
/// parity literal del C++).
async fn pay_refine_fee(session: &mut Session, fee: i64) -> Result<(), String> {
    {
        let row = session.row_mut();
        row.gold = row.gold.saturating_sub(fee as i32);
    }
    session
        .send(&packets::points_packet(session.row(), session.next_exp, &session.battle).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_POINTS (refine fee): {e}"))?;
    session.save();
    Ok(())
}

/// Remueve materiales del inventario (parity `RemoveSpecifyItem`
/// char_item.cpp:945-963 — el conteo ya fue verificado): descuenta por
/// stacks (GC_ITEM_UPDATE + upsert) y borra los que llegan a 0
/// (GC_ITEM_DEL + delete). `skip` = índices intocables (el item destino y
/// el scroll — @fixme346).
async fn remove_materials(
    session: &mut Session,
    recipe: &database::item::RefineRecipe,
    skip: &[usize],
) -> Result<(), String> {
    let repo = ItemRepo::new(session.pool.clone());
    for &(mat_vnum, mat_count) in &recipe.materials {
        if mat_vnum == 0 || mat_count <= 0 {
            continue;
        }
        let mut need = i64::from(mat_count);
        // El skip es por índice y remove() mueve los índices — recolectar
        // los ids ANTES y re-buscar por id tras cada borrado (robusto).
        let ids: Vec<i64> = session
            .inventory
            .iter()
            .enumerate()
            .filter(|(i, r)| !skip.contains(i) && r.vnum == mat_vnum)
            .map(|(_, r)| r.id)
            .collect();
        for id in ids {
            if need <= 0 {
                break;
            }
            let Some(idx) = session.inventory.iter().position(|r| r.id == id) else {
                continue;
            };
            let take = need.min(session.inventory[idx].count);
            session.inventory[idx].count -= take;
            need -= take;
            if session.inventory[idx].count <= 0 {
                let cell = TItemPos {
                    window: TItemPos::WINDOW_INVENTORY,
                    cell: session.inventory[idx].pos as u16,
                };
                session
                    .send(&TPacketGCItemDelDeprecated::new(cell, 0, 0).to_bytes())
                    .await
                    .map_err(|e| format!("enviando GC_ITEM_DEL (material): {e}"))?;
                repo.delete(session.inventory[idx].id).await?;
                session.inventory.remove(idx);
            } else {
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
                    .map_err(|e| format!("enviando GC_ITEM_UPDATE (material): {e}"))?;
                repo.upsert(&session.inventory[idx], session.row().id).await?;
            }
        }
    }
    Ok(())
}

/// Consume 1 del scroll (parity `pkItemScroll->SetCount(count-1)`
/// char_item.cpp:1152 — SIEMPRE, antes del roll): GC_ITEM_UPDATE si queda,
/// GC_ITEM_DEL + delete si se agota.
async fn consume_scroll(session: &mut Session, scroll_idx: usize) -> Result<(), String> {
    let repo = ItemRepo::new(session.pool.clone());
    session.inventory[scroll_idx].count -= 1;
    if session.inventory[scroll_idx].count <= 0 {
        let cell = TItemPos {
            window: TItemPos::WINDOW_INVENTORY,
            cell: session.inventory[scroll_idx].pos as u16,
        };
        let id = session.inventory[scroll_idx].id;
        session
            .send(&TPacketGCItemDelDeprecated::new(cell, 0, 0).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_ITEM_DEL (scroll): {e}"))?;
        repo.delete(id).await?;
        session.inventory.remove(scroll_idx);
    } else {
        let up = protocol::world::TPacketGCItemUpdate {
            header: protocol::world::TPacketGCItemUpdate::HEADER,
            cell: TItemPos {
                window: TItemPos::WINDOW_INVENTORY,
                cell: session.inventory[scroll_idx].pos as u16,
            },
            count: session.inventory[scroll_idx].count as u8,
            sockets: session.inventory[scroll_idx].sockets,
            attrs: session.inventory[scroll_idx].attrs,
        };
        session
            .send(&up.to_bytes())
            .await
            .map_err(|e| format!("enviando GC_ITEM_UPDATE (scroll): {e}"))?;
        repo.upsert(&session.inventory[scroll_idx], session.row().id).await?;
    }
    Ok(())
}

/// Reemplaza el item del slot por el vnum refinado (parity
/// `ITEM_MANAGER::RemoveItem` + `CreateItem(result_vnum)` +
/// `AddToCharacter` — char_item.cpp:963-990): GC_ITEM_DEL (42 B legacy) +
/// GC_ITEM_SET (51 B) en la MISMA celda; sockets/attrs se conservan
/// (`CopyAllAttrTo` item.cpp). El id del row se mantiene (upsert — el wire
/// no lleva id; el C++ crea uno nuevo, diferencia solo de logging).
async fn replace_item(session: &mut Session, idx: usize, new_vnum: i64) -> Result<(), String> {
    let repo = ItemRepo::new(session.pool.clone());
    let cell = TItemPos {
        window: TItemPos::WINDOW_INVENTORY,
        cell: session.inventory[idx].pos as u16,
    };
    let old = session.inventory[idx].clone();
    // GC_ITEM_DEL (layout legacy 42 B — ver TPacketGCItemDelDeprecated).
    session
        .send(&TPacketGCItemDelDeprecated::new(cell, old.vnum as u32, old.count as u8).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_ITEM_DEL (refine OK): {e}"))?;
    // Mutar el row in-place (misma celda/sockets/attrs) + GC_ITEM_SET.
    session.inventory[idx].vnum = new_vnum;
    session.inventory[idx].count = 1;
    let set = TPacketGCItemSet {
        header: TPacketGCItemSet::HEADER,
        cell,
        vnum: new_vnum as u32,
        count: 1,
        flags: 0,
        anti_flags: 0,
        highlight: 0,
        sockets: session.inventory[idx].sockets,
        attrs: session.inventory[idx].attrs,
    };
    session
        .send(&set.to_bytes())
        .await
        .map_err(|e| format!("enviando GC_ITEM_SET (refine OK): {e}"))?;
    repo.upsert(&session.inventory[idx], session.row().id).await?;
    Ok(())
}

/// Destruye el item (parity `RemoveItem(item, "REMOVE (REFINE FAIL)")`
/// char_item.cpp:991-997 — el FAIL del refine NORMAL): GC_ITEM_DEL + delete.
async fn destroy_item(session: &mut Session, idx: usize) -> Result<(), String> {
    let repo = ItemRepo::new(session.pool.clone());
    let cell = TItemPos {
        window: TItemPos::WINDOW_INVENTORY,
        cell: session.inventory[idx].pos as u16,
    };
    let id = session.inventory[idx].id;
    let vnum = session.inventory[idx].vnum;
    session
        .send(&TPacketGCItemDelDeprecated::new(cell, vnum as u32, 0).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_ITEM_DEL (refine FAIL): {e}"))?;
    repo.delete(id).await?;
    session.inventory.remove(idx);
    Ok(())
}

/// El roll + resultado del refine (parity `DoRefine` char_item.cpp:821-1011
/// y `DoRefineWithScroll` :975-1217 — el núcleo compartido): verifica el
/// oro (fee distinta por camino), remueve materiales, tira `number(1,100)`
/// contra `prob`, y aplica el resultado. `scroll_idx` = Some → camino
/// SCROLL (el scroll se consume SIEMPRE antes del roll; el FAIL BAJA el
/// item al vnum anterior si existe — `GetRefineFromVnum` — en vez de
/// destruirlo). Devuelve true si el refine se ejecutó.
async fn refine_execute(
    session: &mut Session,
    target_idx: usize,
    scroll_idx: Option<usize>,
) -> Result<bool, String> {
    let repo = ItemRepo::new(session.pool.clone());
    let target_vnum = session.inventory[target_idx].vnum;
    let Some((recipe, refined_vnum)) = load_refine(session, target_vnum).await? else {
        eprintln!(
            "server_realms: channel conn {}: refine de vnum {target_vnum} \
             sin receta/siguiente nivel — rechazado",
            session.conn_id
        );
        return Ok(false);
    };
    // @fixme346: el skip list del C++ — el item destino y el scroll no se
    // tocan como material ni se cuentan.
    let mut skip = vec![target_idx];
    if let Some(si) = scroll_idx {
        skip.push(si);
    }
    // Fee: NORMAL → ComputeRefineFee(cost) = cost×5 (char.cpp:6598, sin
    // guild); SCROLL → prt->cost sin multiplicar (char_item.cpp:1093).
    let fee = if scroll_idx.is_some() {
        i64::from(recipe.cost)
    } else {
        i64::from(recipe.cost) * 5
    };
    if i64::from(session.row().gold) < fee {
        eprintln!(
            "server_realms: channel conn {}: refine de vnum {target_vnum} — \
             oro insuficiente ({}/{} fee)",
            session.conn_id, session.row().gold, fee
        );
        return Ok(false);
    }
    // Materiales: primero conteo (nada se toca si falta), luego remoción
    // (parity DoRefine:950-963).
    for &(mat_vnum, mat_count) in &recipe.materials {
        if mat_vnum == 0 || mat_count <= 0 {
            continue;
        }
        if count_material(&session.inventory, mat_vnum, &skip) < i64::from(mat_count) {
            eprintln!(
                "server_realms: channel conn {}: refine de vnum {target_vnum} — \
                 falta material {mat_vnum} (×{mat_count})",
                session.conn_id
            );
            return Ok(false);
        }
    }
    // El SCROLL se consume SIEMPRE (antes del roll — parity :1152); el
    // índice se re-busca por id tras las remociones (los índices se
    // mueven).
    let scroll_id = scroll_idx.map(|si| session.inventory[si].id);
    remove_materials(session, &recipe, &skip).await?;
    let scroll_idx = if let Some(sid) = scroll_id {
        session.inventory.iter().position(|r| r.id == sid)
    } else {
        None
    };
    if let Some(si) = scroll_idx {
        consume_scroll(session, si).await?;
    }
    let ok = roll_1_100() <= recipe.prob;
    if ok {
        // Éxito: el item se reemplaza por el vnum +1 (CopyAllAttrTo).
        let cell = session.inventory[target_idx].pos;
        replace_item(session, target_idx, refined_vnum).await?;
        pay_refine_fee(session, fee).await?;
        eprintln!(
            "server_realms: channel conn {}: {} refine OK vnum {target_vnum} \
             → {refined_vnum} (celda {})",
            session.conn_id, session.row().name, cell
        );
    } else if let Some(_si) = scroll_idx {
        // FAIL con scroll: baja al vnum ANTERIOR si existe (parity
        // char_item.cpp:1103-1130 — `result_fail_vnum = GetRefineFromVnum`;
        // sin vnum anterior el item se queda como está).
        let cur_vnum = session.inventory[target_idx].vnum;
        match repo.load_refine_from_vnum(cur_vnum).await? {
            Some(fail_vnum) => {
                replace_item(session, target_idx, fail_vnum).await?;
                pay_refine_fee(session, fee).await?;
                eprintln!(
                    "server_realms: channel conn {}: {} refine FAIL (scroll) vnum \
                     {cur_vnum} → {fail_vnum}",
                    session.conn_id, session.row().name
                );
            }
            None => {
                pay_refine_fee(session, fee).await?;
                eprintln!(
                    "server_realms: channel conn {}: {} refine FAIL (scroll) vnum \
                     {cur_vnum} sin nivel anterior — sin cambios",
                    session.conn_id, session.row().name
                );
            }
        }
    } else {
        // FAIL del refine NORMAL: el item se DESTRUYE (parity :991-997).
        let cell = session.inventory[target_idx].pos;
        destroy_item(session, target_idx).await?;
        pay_refine_fee(session, fee).await?;
        eprintln!(
            "server_realms: channel conn {}: {} refine FAIL vnum {target_vnum} \
             — item destruido (celda {})",
            session.conn_id, session.row().name, cell
        );
    }
    Ok(true)
}

/// CG_ITEM_USE_TO_ITEM (60, 7 B: header + TItemPos Cell + TItemPos
/// TargetCell — Packet.h:549-554). Parity `ItemToItem` (input_main.cpp) →
/// `UseItem` → `UseItemEx` (char_item.cpp:4468-4480) → `RefineItem`
/// (char_item.cpp:1316): el SCROLL (ITEM_USE + USE_TUNING) sobre el item
/// destino valida y abre la ventana de refine — GC_REFINE_INFORMATION
/// (119, 56 B) con prob/materiales/coste/resultado + `SetRefineMode` (el
/// scroll queda en `session.refine_scroll` para el CG_REFINE). GAP
/// documentado: solo el camino SCROLL genérico (gates 501/702); los
/// especiales MUSIN/HYUNIRON/YONGSIN/YAGONG/MEMO/BDRAGON (prob override) y
/// el USE_DETACHMENT (desprender metin) no — subset del lane.
pub async fn handle_use_to_item(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let d = match TPacketCGItemUseToItem::from_bytes(pkt) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: CG_ITEM_USE_TO_ITEM malformado: {e}",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    };
    let Some(scroll_idx) = session.inventory.iter().position(|i| {
        i.window == "INVENTORY" && i.pos as u16 == d.cell.cell && d.cell.window == TItemPos::WINDOW_INVENTORY
    }) else {
        eprintln!(
            "server_realms: channel conn {}: use-to-item — celda {} sin item",
            session.conn_id, d.cell.cell
        );
        return Ok(Outcome::Continue);
    };
    let Some(target_idx) = session.inventory.iter().position(|i| {
        i.window == "INVENTORY"
            && i.pos as u16 == d.target_cell.cell
            && d.target_cell.window == TItemPos::WINDOW_INVENTORY
    }) else {
        eprintln!(
            "server_realms: channel conn {}: use-to-item — destino {} sin item",
            session.conn_id, d.target_cell.cell
        );
        return Ok(Outcome::Continue);
    };
    if scroll_idx == target_idx {
        eprintln!(
            "server_realms: channel conn {}: use-to-item — scroll == destino",
            session.conn_id
        );
        return Ok(Outcome::Continue);
    }
    // El scroll debe ser USE_TUNING (parity RefineItem:1316-1345 — los
    // demás subtipos devuelven false sin ventana).
    let Some(proto) = ItemRepo::new(session.pool.clone())
        .load_proto_use_values(session.inventory[scroll_idx].vnum)
        .await?
    else {
        return Ok(Outcome::Continue);
    };
    if proto.b_type != ITEM_TYPE_USE || proto.b_sub_type != USE_TUNING {
        eprintln!(
            "server_realms: channel conn {}: use-to-item — item vnum {} no es \
             scroll de refine (type {} sub {})",
            session.conn_id,
            session.inventory[scroll_idx].vnum,
            proto.b_type,
            proto.b_sub_type
        );
        return Ok(Outcome::Continue);
    }
    let target_vnum = session.inventory[target_idx].vnum;
    let Some((recipe, refined_vnum)) = load_refine(session, target_vnum).await? else {
        eprintln!(
            "server_realms: channel conn {}: use-to-item — destino vnum \
             {target_vnum} no refinable",
            session.conn_id
        );
        return Ok(Outcome::Continue);
    };
    // Gates del scroll (RefineItem:1322-1343): BDRAGON exige el set 702;
    // el scroll genérico rechaza el set 501.
    if proto.values[0] == BDRAGON_SCROLL {
        let Some((refine_set, _)) = ItemRepo::new(session.pool.clone())
            .load_refine_proto(target_vnum)
            .await?
        else {
            return Ok(Outcome::Continue);
        };
        if refine_set != REFINE_SET_702 {
            eprintln!(
                "server_realms: channel conn {}: use-to-item — BDRAGON sobre \
                 set {refine_set} (exige 702)",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    } else if let Some((refine_set, _)) = ItemRepo::new(session.pool.clone())
        .load_refine_proto(target_vnum)
        .await?
        && refine_set == REFINE_SET_501
    {
        eprintln!(
            "server_realms: channel conn {}: use-to-item — scroll genérico \
             sobre set 501 rechazado",
            session.conn_id
        );
        return Ok(Outcome::Continue);
    }
    // GC_REFINE_INFORMATION: la ventana del cliente (parity
    // RefineInformation char_item.cpp:1236-1309 — `p.cost =
    // ComputeRefineFee(prt->cost)` = cost×5; `p.prob = prt->prob`;
    // materials del prt; material_count = slots no vacíos).
    let mut materials = [RefineMaterial { vnum: 0, count: 0 }; 5];
    let mut material_count = 0u8;
    for (i, &(v, c)) in recipe.materials.iter().enumerate() {
        if v != 0 && c > 0 {
            materials[i] = RefineMaterial { vnum: v as u32, count: c };
            material_count = i as u8 + 1;
        }
    }
    let info = TPacketGCRefineInformation {
        header: TPacketGCRefineInformation::HEADER,
        r#type: TPacketCGRefine::TYPE_SCROLL,
        pos: session.inventory[target_idx].pos as u8,
        src_vnum: target_vnum as u32,
        result_vnum: refined_vnum as u32,
        material_count,
        cost: recipe.cost * 5,
        prob: recipe.prob,
        materials,
    };
    session
        .send(&info.to_bytes())
        .await
        .map_err(|e| format!("enviando GC_REFINE_INFORMATION: {e}"))?;
    // SetRefineMode (char_item.cpp:1309 — el scroll para el CG_REFINE).
    session.refine_scroll = Some(scroll_idx);
    eprintln!(
        "server_realms: channel conn {}: {} abrió refine de vnum {target_vnum} \
         (→ {refined_vnum}, prob {}, scroll celda {})",
        session.conn_id,
        session.row().name,
        recipe.prob,
        d.cell.cell
    );
    Ok(Outcome::Continue)
}

/// CG_REFINE (96, 3 B: header + pos BYTE + type BYTE — Packet.h:976-982).
/// Parity `CInputMain::Refine` (input_main.cpp:2831-2900): type 255 =
/// cancelar (`ClearRefineMode`); NORMAL (0) → `DoRefine` (herrero — tabla,
/// fee ×5, FAIL destruye); SCROLL (2) → `DoRefineWithScroll` (consume el
/// scroll de `refine_scroll`, fee sin multiplicar, FAIL baja de nivel si
/// hay vnum anterior). Resultado por wire: GC_ITEM_DEL + GC_ITEM_SET
/// (éxito), GC_ITEM_DEL (fail NORMAL), GC_ITEM_UPDATE/DEL (materiales y
/// scroll) + GC_POINTS (fee). GAP: sin los types HYUNIRON/MUSIN/BDRAGON/
/// MONEY_ONLY (specials fuera del subset) ni las gates de exchange/safebox/
/// shop del C++ (estados no expuestos en el rewrite).
pub async fn handle_refine(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let d = match TPacketCGRefine::from_bytes(pkt) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: CG_REFINE malformado: {e}",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    };
    if d.r#type == TPacketCGRefine::TYPE_CANCEL {
        session.refine_scroll = None;
        return Ok(Outcome::Continue);
    }
    if d.pos as u16 >= INVENTORY_MAX_NUM {
        eprintln!(
            "server_realms: channel conn {}: CG_REFINE pos {} fuera de rango",
            session.conn_id, d.pos
        );
        session.refine_scroll = None;
        return Ok(Outcome::Continue);
    }
    let Some(target_idx) = session
        .inventory
        .iter()
        .position(|i| i.window == "INVENTORY" && i.pos as u16 == u16::from(d.pos))
    else {
        eprintln!(
            "server_realms: channel conn {}: CG_REFINE celda {} sin item",
            session.conn_id, d.pos
        );
        session.refine_scroll = None;
        return Ok(Outcome::Continue);
    };
    let scroll_idx = match d.r#type {
        TPacketCGRefine::TYPE_NORMAL => None,
        TPacketCGRefine::TYPE_SCROLL => {
            let Some(si) = session.refine_scroll.take() else {
                eprintln!(
                    "server_realms: channel conn {}: CG_REFINE SCROLL sin modo \
                     de refine (usa primero el scroll sobre el item)",
                    session.conn_id
                );
                return Ok(Outcome::Continue);
            };
            if si >= session.inventory.len()
                || session.inventory[si].window != "INVENTORY"
            {
                eprintln!(
                    "server_realms: channel conn {}: CG_REFINE SCROLL — scroll \
                     inválido (slot cambiado)",
                    session.conn_id
                );
                return Ok(Outcome::Continue);
            }
            Some(si)
        }
        other => {
            eprintln!(
                "server_realms: channel conn {}: CG_REFINE type {other} fuera \
                 del subset (NORMAL/SCROLL/cancelar) — modo limpio",
                session.conn_id
            );
            session.refine_scroll = None;
            return Ok(Outcome::Continue);
        }
    };
    if refine_execute(session, target_idx, scroll_idx).await? {
        session.refine_scroll = None;
    }
    Ok(Outcome::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::item::ItemRow;

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

    /// Lane R: el roll del refine es `number(1, 100)` (parity
    /// char_item.cpp:919) — siempre en rango (el C++ compara `prob <=
    /// prt->prob` con prob 1..100).
    #[test]
    fn refine_roll_is_1_to_100() {
        for _ in 0..200 {
            let r = roll_1_100();
            assert!((1..=100).contains(&r), "roll {r} fuera de 1..=100");
        }
    }

    /// Lane R: el conteo de materiales excluye el skipList (@fixme346 — el
    /// item destino y el scroll no se cuentan como material) y suma los
    /// stacks del mismo vnum.
    #[test]
    fn count_material_excludes_skip_and_sums_stacks() {
        let inv = vec![
            ItemRow {
                id: 1,
                window: "INVENTORY".into(),
                pos: 0,
                count: 2,
                vnum: 30053,
                sockets: [0; 3],
                attrs: [(0, 0); 7],
            },
            ItemRow {
                id: 2,
                window: "INVENTORY".into(),
                pos: 1,
                count: 3,
                vnum: 30053,
                sockets: [0; 3],
                attrs: [(0, 0); 7],
            },
            ItemRow {
                id: 3,
                window: "INVENTORY".into(),
                pos: 2,
                count: 5,
                vnum: 30053,
                sockets: [0; 3],
                attrs: [(0, 0); 7],
            },
            ItemRow {
                id: 4,
                window: "INVENTORY".into(),
                pos: 3,
                count: 9,
                vnum: 999,
                sockets: [0; 3],
                attrs: [(0, 0); 7],
            },
        ];
        assert_eq!(count_material(&inv, 30053, &[]), 10, "2+3+5");
        assert_eq!(count_material(&inv, 30053, &[1]), 7, "skip idx 1 → 2+5");
        assert_eq!(count_material(&inv, 30053, &[0, 1, 2]), 0, "todo skip");
        assert_eq!(count_material(&inv, 999, &[]), 9, "otro vnum");
        assert_eq!(count_material(&inv, 111, &[]), 0, "vnum sin items");
    }

    /// Lane USE_*: el mapeo del switch USE_ABILITY_UP (parity
    /// char_item.cpp:4332-4388 — cada case del C++ → (AFFECT_*, POINT_*,
    /// AFF_*)). Los numéricos (MOV_SPEED 19 / ATT_SPEED 17 / ATT_GRADE_BONUS
    /// 95 / DEF_GRADE_BONUS 96 / CASTING_SPEED 21) son los POINT_* que el
    /// combate del mundo ya lee del componente `Affects`; ST/HT/DX/IQ son
    /// los 12-15 de char.h:148-151. Apply sin case → None (sin buff, sin
    /// consumo — parity literal).
    #[test]
    fn ability_up_apply_matches_cpp_switch() {
        use game_core::skill::point;
        // (apply, affect_type, point, flag) — el orden de char_item.cpp:4332.
        let cases = [
            (APPLY_MOV_SPEED, AFFECT_MOV_SPEED, point::MOV_SPEED, AFF_MOV_SPEED_POTION),
            (APPLY_ATT_SPEED, AFFECT_ATT_SPEED, point::ATT_SPEED, AFF_ATT_SPEED_POTION),
            (APPLY_STR, AFFECT_STR, POINT_ST, 0),
            (APPLY_DEX, AFFECT_DEX, POINT_DX, 0),
            (APPLY_CON, AFFECT_CON, POINT_HT, 0),
            (APPLY_INT, AFFECT_INT, POINT_IQ, 0),
            (APPLY_CAST_SPEED, AFFECT_CAST_SPEED, point::CASTING_SPEED, 0),
            (APPLY_ATT_GRADE_BONUS, AFFECT_ATT_GRADE, point::ATT_GRADE_BONUS, 0),
            (APPLY_DEF_GRADE_BONUS, AFFECT_DEF_GRADE, point::DEF_GRADE_BONUS, 0),
        ];
        for (apply, dw_type, point, flag) in cases {
            assert_eq!(
                ability_up_apply(apply),
                Some((dw_type, point, flag)),
                "APPLY {apply}"
            );
        }
        // Fuera del switch del C++ (p. ej. APPLY_MAX_HP = 1, APPLY_HP_REGEN =
        // 10, APPLY_CRITICAL_PCT = 14) → None → sin buff ni consumo.
        assert_eq!(ability_up_apply(1), None, "APPLY_MAX_HP sin case");
        assert_eq!(ability_up_apply(10), None, "APPLY_HP_REGEN sin case");
        assert_eq!(ability_up_apply(14), None, "APPLY_CRITICAL_PCT sin case");
        assert_eq!(ability_up_apply(0), None);
        assert_eq!(ability_up_apply(-1), None);
    }

    /// Lane USE_*: los subtipos del wire son el ORDEN del enum legacy
    /// (item_length.h:250-300) — USE_TREASURE_BOX 4, USE_ABILITY_UP 7
    /// (EUseSubTypes) y AUTOUSE_GOLD 3 (EAutoUseSubTypes). El USE_TREASURE_BOX
    /// por doble-click es NO-OP en el C++ congelado (char_item.cpp:4971-4973
    /// — sin consumo; la apertura es llave+cofre vía `special_item_group`,
    /// gap documentado del lane).
    #[test]
    fn use_subtype_constants_match_cpp_enums() {
        assert_eq!(USE_TREASURE_BOX, 4, "item_length.h:255");
        assert_eq!(USE_ABILITY_UP, 7, "item_length.h:258");
        assert_eq!(AUTOUSE_GOLD, 3, "item_length.h:298");
        assert_eq!(USE_TUNING, 2, "item_length.h:252 (refine — ya existente)");
        // EAffectTypes/EAffectBits spot-checks (affect.h:22-54, 137-138).
        assert_eq!(AFFECT_MOV_SPEED, 200);
        assert_eq!(AFFECT_ATT_SPEED, 201);
        assert_eq!(AFFECT_CAST_SPEED, 217);
        assert_eq!(AFFECT_DEF_GRADE, 226);
        assert_eq!(AFF_MOV_SPEED_POTION, 12);
        assert_eq!(AFF_ATT_SPEED_POTION, 13);
    }

    /// Lane USE_*: la bolsa AUTOUSE_GOLD suma value0 al oro con el cap
    /// GOLD_MAX = 2e9 (parity `PointChange(POINT_GOLD)` + length.h:80).
    #[test]
    fn autouse_gold_caps_at_gold_max() {
        assert_eq!(gold_after_add(100, 50), 150, "suma normal");
        assert_eq!(gold_after_add(0, 5000), 5000);
        assert_eq!(gold_after_add(1_999_999_900, 500), 2_000_000_000, "cap");
        assert_eq!(gold_after_add(2_000_000_000, 5), 2_000_000_000, "ya en el cap");
        // El gate del handler rechaza amount <= 0 antes (no consume) — el
        // helper puro se comporta bien igualmente.
        assert_eq!(gold_after_add(100, 0), 100);
        assert_eq!(gold_after_add(100, -50), 50);
    }
}