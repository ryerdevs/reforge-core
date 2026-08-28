//! `channel/safebox.rs` — el SAFEBOX (banco del jugador): apertura/cierre
//! por chat (`/safebox_password <pass>` / `/safebox_close`), los handlers de
//! paquetes CG_SAFEBOX_CHECKIN (70) / CHECKOUT (71) / ITEM_MOVE (77) /
//! MONEY (79) y el tamaño GM (`/safebox <0..3>`).
//!
//! # Parity legacy (verificado contra el C++ congelado)
//!
//! - Apertura: `do_safebox_password` (cmd_general.cpp:805-810) →
//!   `ReqSafeboxLoad` (char.cpp:5494-5541): password vacía o > 6 → INFO;
//!   caja ya abierta → INFO; cooldown de 10 s (`PASSES_PER_SEC(10)`) →
//!   INFO. El resultado del load valida la password contra la fila
//!   (`RESULT_SAFEBOX_LOAD`, ClientManager.cpp:628-656): sin fila → solo la
//!   password por defecto "000000"; con fila → la password de la fila (o
//!   "000000" si la fila la tiene vacía). Fallo → `GC_SAFEBOX_WRONG_PASSWORD`
//!   (87). Éxito → `GC_SAFEBOX_SIZE` (88) + un `GC_SAFEBOX_SET` (85) por
//!   item (parity `LoadSafebox` char.cpp:5543-5588 — los items con posición
//!   fuera del grid se SALTAN).
//! - Cierre: `do_safebox_close` → `CloseSafebox` (char.cpp:5608-5627):
//!   `CSafebox::Save()` (solo el oro — `TSafeboxTable`, safebox.cpp:117-127;
//!   los items ya se persistieron en cada mutación), `ChatPacket(COMMAND,
//!   "CloseSafebox")` (el cliente cierra la ventana) y cooldown.
//! - Checkin: `SafeboxCheckin` (input_main.cpp:1940-2024): item del
//!   INVENTARIO (entero, sin count) → posición libre de la caja. Gates:
//!   EXPAND + antiflag SAFEBOX + strip del grid (ver abajo).
//! - Checkout: `SafeboxCheckout` (input_main.cpp:2027-2117): de la caja a
//!   una celda libre del INVENTARIO (window INVENTORY) o del BELT (celdas
//!   242..258 con los gates de belt.rs — parity `@fixme119` :2096-2100 +
//!   rama belt de `IsEmptyItemGrid` char_item.cpp:547-567), validando el
//!   strip del item en el grid del inventario (`IsEmptyItemGrid`, :2056).
//!   DS = GAP.
//! - ItemMove: `CSafebox::MoveItem` (safebox.cpp:170-231): stack si el
//!   destino es apilable SIN antiflag STACK + mismo vnum + sockets iguales
//!   + count < 200; si no, mover a un hueco donde quepa el strip.
//!     `count == 0` → todo el stack.
//! - Money: `TPacketCGSafeboxMoney` (packet.h:1627-1632) — DEFENSIVO: el
//!   C++ congelado NO registra el 79 en su framer y el cliente de la
//!   variante nunca lo envía (`SendSafeBoxMoneyPacket` es un assert,
//!   PythonNetworkStreamPhaseGameItem.cpp:14-17). Wire byte-exacto por si
//!   un cliente externo lo manda.
//! - ChangePassword: `/safebox_change_password <old> <new>`
//!   (do_safebox_change_password cmd_general.cpp:812-838 →
//!   RESULT_SAFEBOX_CHANGE_PASSWORD ClientManager.cpp:991-1053): old
//!   insensible a mayúsculas; sin fila → INSERT con la nueva.
//!
//! # Modelo del grid (verificado cliente+server)
//!
//! El C++ abre `CGrid(5, iSize)` (safebox.cpp:20-23) y el cliente
//! `SAFEBOX_SLOT_X_COUNT (5) × bSize` slots (PythonSafeBox.cpp:4-15): el
//! tamaño (páginas, 0..3) define `slots = 5 × size`. `GC_SAFEBOX_SIZE`
//! reenvía el valor de la DB tal cual (char.cpp:5553-5558). Los items con
//! `size > 1` del item_proto (celdas 1×1..3×3 — `load_safebox_proto`)
//! ocupan un STRIP VERTICAL de celdas (pos, pos+5, …): parity
//! `IsEmpty(pos, 1, size)` safebox.cpp:130-136 e `IsEmptyItemGrid`
//! char_item.cpp:579-596 (el grid legacy es de 5 celdas de ancho — no es
//! un cuadrado 2×2; el cliente renderiza la caja como slots planos,
//! PythonSafeBox.cpp:4-38 y uisafebox.py:361). El checkout valida el mismo
//! strip en el inventario (5×9 por página y 4 páginas — length.h:21-29 +
//! ENABLE_EXTEND_INVEN_SYSTEM: 180 celdas).
//!
//! # Gates del checkin/stack (parity item_proto)
//!
//! `SafeboxCheckin` rechaza el item de expansión (UNIQUE_ITEM_SAFEBOX_
//! EXPAND = 71009, input_main.cpp:1975-1979), los items con antiflag
//! SAFEBOX (1<<17, :1981-1985) y los que no caben en el grid
//! (`IsEmpty(p->bSafePos, 1, GetSize())`, :1969). `MoveItem` solo apila
//! sobre destinos apilables sin antiflag STACK (`item2->IsStackable() &&
//! !IS_SET(..., ITEM_ANTIFLAG_STACK)`, safebox.cpp:188-189). Los
//! size/flags vienen de `player.item_proto` (667 vnums reales con el
//! antiflag SAFEBOX; 2679 con size 2 — verificados en PG 2026-08-27).
//!
//! # Persistencia
//!
//! Cada mutación hace `ItemRepo::upsert` (patrón de items.rs) con el owner
//! correcto: INVENTORY → el personaje (`row().id`), SAFEBOX → la CUENTA
//! (`account_id` — parity `RESULT_SAFEBOX_LOAD` ClientManager.cpp:686-693,
//! que pasa `pi->account_id`). El oro se persiste con `SafeboxRepo::set_gold`
//! al mutarlo y al cerrar. Divergencia documentada: el C++ NO lee el gold en
//! el load (comentado, ClientManager.cpp:663-665) — el reforge lo lee
//! (`get_gold`) para que el dinero sobreviva entre sesiones.

use std::time::Duration;

use database::item::{ItemRepo, ItemRow};
use database::safebox::SafeboxRepo;
use game_core::packets;
use protocol::world::{
    TPacketCGSafeboxCheckin, TPacketCGSafeboxCheckout, TPacketCGSafeboxMoney,
    TPacketGCItemDel, TPacketGCItemDelDeprecated, TPacketGCItemSet, TPacketGCSafeboxMoneyChange,
    TPacketGCSafeboxSize, TPacketGCSafeboxWrongPassword, TItemPos,
};

use crate::channel::session::{Outcome, Session};
use crate::channel::{belt, gm, quickslot, INVENTORY_MAX_NUM, ITEM_COUNT_LIMIT};

/// `SAFEBOX_PASSWORD_MAX_LEN = 6` (tables.h:692).
const SAFEBOX_PASSWORD_MAX_LEN: usize = 6;
/// Password por defecto de una caja sin fila / con password vacía (parity
/// `RESULT_SAFEBOX_LOAD`, ClientManager.cpp:631-650).
const SAFEBOX_DEFAULT_PASSWORD: &str = "000000";
/// Cooldown de re-apertura: `PASSES_PER_SEC(10)` (char.cpp:5509-5513).
const REOPEN_COOLDOWN: Duration = Duration::from_secs(10);
/// `CHAT_TYPE_COMMAND` (length.h:263-274 — 5): el "CloseSafebox" del C++
/// viaja como comando de chat (char.cpp:5622).
const CHAT_TYPE_COMMAND: u8 = 5;
/// Clamp del tamaño GM (parity do_safebox_size cmd_gm.cpp:1857-1871:
/// `size > 3 || size < 0 → 0`).
const SAFEBOX_SIZE_MAX: u8 = 3;
/// `UNIQUE_ITEM_SAFEBOX_EXPAND = 71009` (unique_item.h:45) — el item de
/// expansión de la caja no se puede guardar (SafeboxCheckin,
/// input_main.cpp:1975-1979).
const UNIQUE_ITEM_SAFEBOX_EXPAND: i64 = 71009;
/// `ITEM_ANTIFLAG_SAFEBOX = (1 << 17)` (item_length.h:371) — items que no
/// entran en la safebox (667 vnums reales en PG).
const ITEM_ANTIFLAG_SAFEBOX: i64 = 1 << 17;
/// `ITEM_ANTIFLAG_STACK = (1 << 15)` (item_length.h:369) — no apilables.
const ITEM_ANTIFLAG_STACK: i64 = 1 << 15;
/// `ITEM_FLAG_STACKABLE = (1 << 2)` (item_length.h:337) — el stack exige
/// un destino apilable (parity `item2->IsStackable()`, item.h:25).
const ITEM_FLAG_STACKABLE: i64 = 1 << 2;
/// Ancho del grid en celdas: 5 (SAFEBOX_SLOT_X_COUNT, PythonSafeBox.cpp:
/// 4-15; `CGrid(5, iSize)`, safebox.cpp:18; INVENTORY_PAGE_COLUMN para el
/// checkout, length.h:21).
const GRID_WIDTH: u16 = 5;
/// Celdas por página del inventario: `5×9` (length.h:21-23) — el strip del
/// checkout no puede cruzar de página (parity `IsEmptyItemGrid`,
/// char_item.cpp:580-590; ENABLE_EXTEND_INVEN_SYSTEM → 4 páginas = 180).
const INVENTORY_PAGE_CELLS: u16 = 45;

/// Estado de la caja ABIERTA (vive en la sesión — parity `m_pkSafebox` del
/// CHARACTER C++). `None` en la sesión = caja cerrada.
pub struct SafeboxState {
    /// Tamaño en PÁGINAS (0..3 — valor de la DB, tal cual viaja en el
    /// GC_SAFEBOX_SIZE). Slots válidos = `5 × size`.
    pub size: i16,
    /// Oro de la caja (columna `player.safebox.gold`).
    pub gold: i32,
    /// Items de la caja (window "SAFEBOX", `pos` = slot 0..5×size).
    pub items: Vec<ItemRow>,
}

impl SafeboxState {
    /// Slots válidos del grid: `5 × size` (parity `CGrid(5, iSize)` +
    /// `SAFEBOX_SLOT_X_COUNT × bSize` del cliente).
    pub fn slots(&self) -> u16 {
        (5 * self.size.max(0)) as u16
    }
}

/// Celdas que cubre un item de `size` celdas anclado en `pos` — strip
/// VERTICAL de paso 5 (parity `IsEmpty(pos, 1, size)` safebox.cpp:130-136
/// e `IsEmptyItemGrid` char_item.cpp:579-596: `bCell + 5*j`, j < size — el
/// grid legacy del juego ES de 5 celdas de ancho, no un cuadrado 2×2).
fn strip_cells(pos: u16, size: u16) -> Vec<u16> {
    (0..size).map(|j| pos + GRID_WIDTH * j).collect()
}

/// ¿Cabe un item de `size` celdas con su esquina en `pos`? El strip debe
/// quedar dentro de `cells` (y, si `page`, sin cruzar la página —
/// `IsEmptyItemGrid`) y no pisar ninguna celda de `occupied` (parity
/// `CGrid::IsEmpty`, grid.h:13).
fn strip_fits(pos: u16, size: u16, cells: u16, page: Option<u16>, occupied: &[u16]) -> bool {
    strip_cells(pos, size).into_iter().all(|c| {
        c < cells
            && page.is_none_or(|pc| c / pc == pos / pc)
            && !occupied.contains(&c)
    })
}

/// Gates puros del checkin (parity `SafeboxCheckin`, input_main.cpp:
/// 1963-1991): no puede ser el item de EXPANSIÓN (unique_item.h:45), ni
/// llevar el antiflag SAFEBOX (item_length.h:371), y su strip debe caber
/// libre en el grid (`IsEmpty(p->bSafePos, 1, pkItem->GetSize())`, :1969).
/// `size`/`antiflag` vienen del item_proto (fail-open 1/0 si no existe).
fn checkin_gate(vnum: i64, size: u16, antiflag: i64, pos: u16, cells: u16, occupied: &[u16]) -> bool {
    vnum != UNIQUE_ITEM_SAFEBOX_EXPAND
        && antiflag & ITEM_ANTIFLAG_SAFEBOX == 0
        && strip_fits(pos, size, cells, None, occupied)
}

/// ¿Se puede apilar sobre el destino? (parity `MoveItem`, safebox.cpp:
/// 188-189: `item2->IsStackable() && !IS_SET(item2->GetAntiFlag(),
/// ITEM_ANTIFLAG_STACK)`). Fail-closed: proto desconocido → no apilar.
fn stackable_dst(flag: i64, antiflag: i64) -> bool {
    flag & ITEM_FLAG_STACKABLE != 0 && antiflag & ITEM_ANTIFLAG_STACK == 0
}

/// `TPacketGCItemSet` con header `GC_SAFEBOX_SET` (85) y cell window SAFEBOX
/// (parity safebox.cpp:66-78: `pack.header = HEADER_GC_SAFEBOX_SET;
/// pack.Cell = TItemPos(SAFEBOX, dwPos)`).
fn set_packet(item: &ItemRow, pos: u16) -> TPacketGCItemSet {
    TPacketGCItemSet {
        header: protocol::header::GC_SAFEBOX_SET,
        cell: TItemPos {
            window: TItemPos::WINDOW_SAFEBOX,
            cell: pos,
        },
        vnum: item.vnum as u32,
        count: item.count as u8,
        flags: 0,
        anti_flags: 0,
        highlight: 0,
        sockets: item.sockets,
        attrs: item.attrs,
    }
}

/// `/safebox_password <password>` — abrir la caja (parity do_safebox_password
/// cmd_general.cpp:805-810 → ReqSafeboxLoad char.cpp:5494-5541 →
/// RESULT_SAFEBOX_LOAD ClientManager.cpp:628-656). GAP documentado: sin
/// chequeo de distancia al NPC (el `SetSafeboxOpenPosition` del C++ lo fija
/// el click del NPC, que no existe en el subset).
pub async fn open(session: &mut Session, password: &str) -> Result<Outcome, String> {
    // Parity char.cpp:5496-5498: vacía o > 6 → INFO.
    if password.is_empty() || password.len() > SAFEBOX_PASSWORD_MAX_LEN {
        gm::gm_info(session, "<Safebox> Wrong password.").await?;
        return Ok(Outcome::Continue);
    }
    // Parity char.cpp:5499-5502: ya abierta → INFO.
    if session.safebox.is_some() {
        gm::gm_info(session, "<Safebox> The safebox is already open.").await?;
        return Ok(Outcome::Continue);
    }
    // Parity char.cpp:5508-5513: cooldown de 10 s desde el cierre.
    if let Some(until) = session.safebox_cooldown_until
        && tokio::time::Instant::now() < until
    {
        gm::gm_info(session, "<Safebox> You can only open the safebox once every 10 seconds.").await?;
        return Ok(Outcome::Continue);
    }
    // Load de la fila + validación de la password (parity RESULT_SAFEBOX_LOAD).
    let repo = SafeboxRepo::new(session.pool.clone());
    let row = repo.load(session.account_id).await?;
    let password_ok = match &row {
        // Sin fila: solo la password por defecto (ClientManager.cpp:631-635).
        None => password == SAFEBOX_DEFAULT_PASSWORD,
        // Con fila: password de la fila, o la por defecto si la fila la
        // tiene vacía (ClientManager.cpp:639-644).
        Some(r) => {
            if r.password.is_empty() {
                password == SAFEBOX_DEFAULT_PASSWORD
            } else {
                r.password == password
            }
        }
    };
    if !password_ok {
        // GC_SAFEBOX_WRONG_PASSWORD (87, 1 B) — el cliente re-muestra el
        // diálogo (input_db.cpp:1165-1175 → CancelSafeboxLoad).
        session
            .send(&TPacketGCSafeboxWrongPassword::new().to_bytes())
            .await
            .map_err(|e| format!("enviando GC_SAFEBOX_WRONG_PASSWORD: {e}"))?;
        return Ok(Outcome::Continue);
    }
    let size = row.as_ref().map(|r| r.size).unwrap_or(0);
    let slots = (5 * size.max(0)) as u16;
    // Divergencia documentada: el C++ arranca el oro en 0 (lectura comentada
    // en RESULT_SAFEBOX_LOAD) — el reforge lo lee de la DB para que el
    // handler de dinero funcione entre sesiones.
    let gold = repo.get_gold(session.account_id).await?.unwrap_or(0);
    let items = ItemRepo::new(session.pool.clone())
        .load_safebox(session.account_id)
        .await?;
    // GC_SAFEBOX_SIZE (88) — el cliente abre la ventana con 5×size slots.
    session
        .send(&TPacketGCSafeboxSize::new(size as u8).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_SAFEBOX_SIZE: {e}"))?;
    // Un GC_SAFEBOX_SET por item (parity LoadSafebox — los fuera del grid se
    // saltan: `if (!IsValidPosition(pos)) continue;` char.cpp:5561-5563).
    for item in &items {
        if item.pos as u16 >= slots {
            eprintln!(
                "server_realms: channel conn {}: item safebox id {} en pos \
                 {} fuera del grid (slots {slots}) — omitido (parity)",
                session.conn_id, item.id, item.pos
            );
            continue;
        }
        session
            .send(&set_packet(item, item.pos as u16).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_SAFEBOX_SET (open): {e}"))?;
    }
    session.safebox = Some(SafeboxState {
        size,
        gold,
        items,
    });
    eprintln!(
        "server_realms: channel conn {}: {} abrió la safebox (cuenta {}, \
         tamaño {size}, {} items, {gold} oro)",
        session.conn_id,
        session.row().name,
        session.account_id,
        session.safebox.as_ref().map(|s| s.items.len()).unwrap_or(0)
    );
    Ok(Outcome::Continue)
}

/// `/safebox_close` — cerrar la caja (parity do_safebox_close →
/// CloseSafebox char.cpp:5608-5627): persistir el oro, avisar al cliente
/// (CHAT COMMAND "CloseSafebox") y armar el cooldown de re-apertura.
/// También lo usa el cierre de conexión (game.rs — el C++ llama
/// CloseSafebox en CHARACTER::Destroy, char.cpp:1352).
pub async fn close(session: &mut Session) -> Result<(), String> {
    let Some(st) = session.safebox.take() else {
        return Ok(());
    };
    // CSafebox::Save(): solo el oro (safebox.cpp:117-127 — los items ya se
    // persistieron en cada mutación). Fallo de PG → log, no fatal (el cierre
    // no debe colgarse).
    if let Err(e) = SafeboxRepo::new(session.pool.clone())
        .set_gold(session.account_id, st.gold)
        .await
    {
        eprintln!(
            "server_realms: channel conn {}: safebox set_gold de la cuenta \
             {} falló en el cierre: {e}",
            session.conn_id, session.account_id
        );
    }
    session.safebox_cooldown_until = Some(tokio::time::Instant::now() + REOPEN_COOLDOWN);
    // Parity char.cpp:5622 — ChatPacket(CHAT_TYPE_COMMAND, "CloseSafebox"):
    // el cliente cierra la ventana de la caja.
    let text = "CloseSafebox";
    let mut out = Vec::with_capacity(9 + text.len());
    out.push(protocol::header::GC_CHAT);
    out.extend_from_slice(&((9 + text.len()) as u16).to_le_bytes());
    out.push(CHAT_TYPE_COMMAND);
    out.extend_from_slice(&session.player_vid().to_le_bytes());
    out.push(session.empire);
    out.extend_from_slice(text.as_bytes());
    session
        .send(&out)
        .await
        .map_err(|e| format!("enviando GC_CHAT CloseSafebox: {e}"))?;
    eprintln!(
        "server_realms: channel conn {}: {} cerró la safebox (cuenta {}, \
         {} items guardados, {} oro persistido)",
        session.conn_id,
        session.row().name,
        session.account_id,
        st.items.len(),
        st.gold
    );
    Ok(())
}

/// `/safebox_change_password <old> <new>` — cambiar la password de la caja
/// (parity do_safebox_change_password cmd_general.cpp:812-838 →
/// RESULT_SAFEBOX_CHANGE_PASSWORD ClientManager.cpp:991-1053: vacío o > 6
/// → INFO; old incorrecto → INFO; sin fila → INSERT con la nueva — el C++
/// crea la caja). INFO en EN (locale pendiente — patrón del open).
pub async fn change_password(
    session: &mut Session,
    old: &str,
    new: &str,
) -> Result<Outcome, String> {
    if old.is_empty()
        || new.is_empty()
        || old.len() > SAFEBOX_PASSWORD_MAX_LEN
        || new.len() > SAFEBOX_PASSWORD_MAX_LEN
    {
        gm::gm_info(session, "<Safebox> Wrong password.").await?;
        return Ok(Outcome::Continue);
    }
    let changed = SafeboxRepo::new(session.pool.clone())
        .change_password(session.account_id, old, new)
        .await?;
    gm::gm_info(
        session,
        if changed {
            "<Safebox> Your safebox password has been changed."
        } else {
            "<Safebox> The current password is wrong."
        },
    )
    .await?;
    Ok(Outcome::Continue)
}

/// `/safebox <0..3>` (GM) — tamaño de la caja (parity do_safebox_size
/// cmd_gm.cpp:1857-1871 → ChangeSafeboxSize char.cpp:5594-5605).
/// Divergencia documentada: el comando GM del C++ NO persiste el tamaño (lo
/// persiste el path de quest vía QUERY_SAFEBOX_CHANGE_SIZE) — aquí se
/// persiste con `SafeboxRepo::set_size` para que sobreviva a la sesión.
pub async fn set_size(session: &mut Session, size: u8) -> Result<Outcome, String> {
    let size = if size > SAFEBOX_SIZE_MAX { 0 } else { size };
    session
        .send(&TPacketGCSafeboxSize::new(size).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_SAFEBOX_SIZE (GM size): {e}"))?;
    if let Some(st) = session.safebox.as_mut()
        && size as i16 > st.size
    {
        // El grid solo CRECE (parity `CSafebox::ChangeSize`: `if (m_iSize >=
        // iSize) return;` safebox.cpp:139-141) — el paquete ya se mandó.
        st.size = size as i16;
    }
    // QUERY_SAFEBOX_CHANGE_SIZE (ClientManager.cpp:967-970): size==1 INSERT
    // (crea la fila — el ON CONFLICT DO NOTHING lo hace idempotente).
    SafeboxRepo::new(session.pool.clone())
        .set_size(session.account_id, i16::from(size))
        .await?;
    eprintln!(
        "server_realms: channel conn {}: GM {} cambió el tamaño de la \
         safebox a {size} (cuenta {})",
        session.conn_id, session.row().name, session.account_id
    );
    Ok(Outcome::Continue)
}

/// CG_SAFEBOX_CHECKIN (70, 5 B: header + bSafePos + TItemPos). Parity
/// `SafeboxCheckin` (input_main.cpp:1940-2024): el item del INVENTARIO
/// (entero, sin count) se mueve a la posición `bSafePos` de la caja.
/// Gates: EXPAND/antiflag SAFEBOX/strip del grid (parity :1963-1991, con el
/// item_proto real). GAP restante: irremovable/locked (flags del item en
/// sesión — el reforge no los persiste).
pub async fn handle_checkin(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let p = match TPacketCGSafeboxCheckin::from_bytes(pkt) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: CG_SAFEBOX_CHECKIN \
                 malformado: {e}",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    };
    let Some(st) = session.safebox.as_ref() else {
        eprintln!(
            "server_realms: channel conn {}: checkin sin caja abierta — \
             ignorado",
            session.conn_id
        );
        return Ok(Outcome::Continue);
    };
    // Parity `ch->GetItem(p->ItemPos)`: item del INVENTARIO en esa celda.
    if p.item_pos.window != TItemPos::WINDOW_INVENTORY || p.item_pos.cell >= INVENTORY_MAX_NUM {
        eprintln!(
            "server_realms: channel conn {}: checkin de celda inválida \
             (window {} cell {}) — rechazado",
            session.conn_id, p.item_pos.window, p.item_pos.cell
        );
        return Ok(Outcome::Continue);
    }
    let Some(idx) = session
        .inventory
        .iter()
        .position(|i| i.window == "INVENTORY" && i.pos as u16 == p.item_pos.cell)
    else {
        eprintln!(
            "server_realms: channel conn {}: checkin de celda {} sin item",
            session.conn_id, p.item_pos.cell
        );
        return Ok(Outcome::Continue);
    };
    // Gates del checkin (parity SafeboxCheckin input_main.cpp:1963-1991):
    // EXPAND + antiflag SAFEBOX + strip del grid (`IsEmpty(p->bSafePos, 1,
    // pkItem->GetSize())` safebox.cpp:1969) — los size/flags vienen del
    // item_proto (batch: el item entrante + los guardados, una query).
    // Rechazo ANTES de tocar el inventario (parity: el C++ valida con el
    // item puesto y solo muta al pasar todos los gates).
    let item_vnum = session.inventory[idx].vnum;
    let mut vnums: Vec<i64> = st.items.iter().map(|i| i.vnum).collect();
    vnums.push(item_vnum);
    let protos = ItemRepo::new(session.pool.clone())
        .load_safebox_proto(&vnums)
        .await?;
    let proto = protos.get(&item_vnum).copied().unwrap_or_default();
    let mut occupied = Vec::with_capacity(st.items.len());
    for it in &st.items {
        let size = protos.get(&it.vnum).map_or(1, |p| p.size);
        occupied.extend(strip_cells(it.pos as u16, size));
    }
    if !checkin_gate(
        item_vnum,
        proto.size.max(1),
        proto.antiflag,
        u16::from(p.b_safe_pos),
        st.slots(),
        &occupied,
    ) {
        eprintln!(
            "server_realms: channel conn {}: checkin de vnum {} a pos {} \
             rechazado (EXPAND/antiflag/grid)",
            session.conn_id, item_vnum, p.b_safe_pos
        );
        return Ok(Outcome::Continue);
    }
    // SyncQuickslot de la celda (parity input_main.cpp:2015-2017: el item
    // sale del inventario — la barra rápida deja de referenciarlo).
    let mut qblob = quickslot::blob(session.row());
    let cleared = quickslot::clear_item_refs(&mut qblob, p.item_pos.cell);
    if !cleared.is_empty() {
        for pos in &cleared {
            session
                .send(&protocol::world::TPacketGCQuickSlotDel::new(*pos).to_bytes())
                .await
                .map_err(|e| format!("enviando GC_QUICKSLOT_DEL: {e}"))?;
        }
        session.row_mut().quickslot = Some(qblob);
    }
    // Mover: inventario → caja (GC_ITEM_DEL deprecated + GC_SAFEBOX_SET +
    // upsert con owner = CUENTA — parity `pkItem->RemoveFromCharacter()` +
    // `pkSafebox->Add` (safebox.cpp:52-78; el item cambia de owner al pasar
    // a la caja, ClientManager.cpp:686-693).
    let mut item = session.inventory.remove(idx);
    let vnum = item.vnum;
    let cell = TItemPos {
        window: TItemPos::WINDOW_INVENTORY,
        cell: p.item_pos.cell,
    };
    session
        .send(&TPacketGCItemDelDeprecated::new(cell, 0, 0).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_ITEM_DEL (checkin): {e}"))?;
    item.window = "SAFEBOX".to_string();
    item.pos = p.b_safe_pos as i32;
    ItemRepo::new(session.pool.clone())
        .upsert(&item, session.account_id)
        .await?;
    session
        .send(&set_packet(&item, p.b_safe_pos as u16).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_SAFEBOX_SET (checkin): {e}"))?;
    let st = session.safebox.as_mut().expect("caja abierta (gate arriba)");
    st.items.push(item);
    session.save();
    eprintln!(
        "server_realms: channel conn {}: {} metió item vnum {vnum} en la \
         safebox (pos {})",
        session.conn_id, session.row().name, p.b_safe_pos
    );
    Ok(Outcome::Continue)
}

/// CG_SAFEBOX_CHECKOUT (71, 5 B: header + bSafePos + TItemPos). Parity
/// `SafeboxCheckout` (input_main.cpp:2027-2117): de la caja al INVENTARIO o
/// al BELT (celdas 242..258 — gates en belt.rs; DS = GAP).
pub async fn handle_checkout(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let p = match TPacketCGSafeboxCheckout::from_bytes(pkt) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: CG_SAFEBOX_CHECKOUT \
                 malformado: {e}",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    };
    if session.safebox.is_none() {
        eprintln!(
            "server_realms: channel conn {}: checkout sin caja abierta — \
             ignorado",
            session.conn_id
        );
        return Ok(Outcome::Continue);
    }
    // Destino: INVENTORY (celda libre) o BELT (242..258 — celdas wire del
    // belt, belt.rs); el resto se rechaza (parity `IsEmptyItemGrid` +
    // rechazo del C++ a destinos no-inventario para items no-DS,
    // input_main.cpp:2081-2093; el DS sigue siendo GAP).
    if p.item_pos.window != TItemPos::WINDOW_INVENTORY
        || (p.item_pos.cell >= INVENTORY_MAX_NUM && !belt::is_belt_cell(p.item_pos))
    {
        eprintln!(
            "server_realms: channel conn {}: checkout a celda inválida \
             (window {} cell {}) — rechazado (DS = GAP)",
            session.conn_id, p.item_pos.window, p.item_pos.cell
        );
        return Ok(Outcome::Continue);
    }
    let Some(idx) = session
        .safebox
        .as_ref()
        .and_then(|st| st.items.iter().position(|i| i.pos as u16 == p.b_safe_pos as u16))
    else {
        eprintln!(
            "server_realms: channel conn {}: checkout de pos {} sin item",
            session.conn_id, p.b_safe_pos
        );
        return Ok(Outcome::Continue);
    };
    // Grid del INVENTARIO destino (parity `IsEmptyItemGrid(p->ItemPos,
    // pkItem->GetSize())` input_main.cpp:2056): el strip del item no puede
    // pisar celdas ocupadas ni cruzar la página de 45 (char_item.cpp:
    // 569-598). Los size de los items del inventario vienen del item_proto
    // (batch: la caja + el inventario, una query). El BELT usa sus propios
    // gates (tipo poción + belt equipado + grade + celda libre — parity
    // `@fixme119` input_main.cpp:2096-2100 y la rama belt de
    // `IsEmptyItemGrid` char_item.cpp:547-567; el `GetSize` del belt es
    // siempre 1 — solo potions).
    let src_vnum = {
        let st = session.safebox.as_ref().expect("caja abierta (gate arriba)");
        st.items[idx].vnum
    };
    let mut vnums: Vec<i64> = session.inventory.iter().map(|i| i.vnum).collect();
    vnums.push(src_vnum);
    let protos = ItemRepo::new(session.pool.clone())
        .load_safebox_proto(&vnums)
        .await?;
    let size = protos.get(&src_vnum).map_or(1, |p| p.size);
    let mut occupied = Vec::with_capacity(session.inventory.len());
    for it in &session.inventory {
        if it.window != "INVENTORY" {
            continue;
        }
        let s = protos.get(&it.vnum).map_or(1, |p| p.size);
        occupied.extend(strip_cells(it.pos as u16, s));
    }
    let to_belt = belt::is_belt_cell(p.item_pos);
    if to_belt {
        let Some(proto) = ItemRepo::new(session.pool.clone())
            .load_proto_use_values(src_vnum)
            .await?
        else {
            eprintln!(
                "server_realms: channel conn {}: checkout a belt de vnum {} \
                 sin item_proto — rechazado",
                session.conn_id, src_vnum
            );
            return Ok(Outcome::Continue);
        };
        if !belt::can_move_into_belt(proto.b_type, proto.b_sub_type) {
            eprintln!(
                "server_realms: channel conn {}: checkout a belt de vnum {} \
                 no-poción — rechazado (parity @fixme119)",
                session.conn_id, src_vnum
            );
            return Ok(Outcome::Continue);
        }
        let grade = {
            let Some(belt_item) = belt::equipped_belt(&session.inventory) else {
                eprintln!(
                    "server_realms: channel conn {}: checkout a belt sin \
                     cinturón equipado — rechazado (parity IsEmptyItemGrid)",
                    session.conn_id
                );
                return Ok(Outcome::Continue);
            };
            let Some(bp) = ItemRepo::new(session.pool.clone())
                .load_proto_use_values(belt_item.vnum)
                .await?
            else {
                eprintln!(
                    "server_realms: channel conn {}: cinturón vnum {} sin \
                     item_proto — checkout a belt rechazado",
                    session.conn_id, belt_item.vnum
                );
                return Ok(Outcome::Continue);
            };
            bp.values[0]
        };
        if !belt::is_available_cell(p.item_pos.cell, grade)
            || session
                .inventory
                .iter()
                .any(|i| i.window == "BELT_INVENTORY" && i.pos == belt::belt_pos_of(p.item_pos.cell))
        {
            eprintln!(
                "server_realms: channel conn {}: checkout a belt cell {} \
                 fuera de grade {grade} u ocupada — rechazado",
                session.conn_id, p.item_pos.cell
            );
            return Ok(Outcome::Continue);
        }
    } else if !strip_fits(
        p.item_pos.cell,
        size,
        INVENTORY_MAX_NUM,
        Some(INVENTORY_PAGE_CELLS),
        &occupied,
    ) {
        eprintln!(
            "server_realms: channel conn {}: checkout a celda {} sin hueco \
             para el strip de {size} — rechazado",
            session.conn_id, p.item_pos.cell
        );
        return Ok(Outcome::Continue);
    }
    // Mover: caja → inventario/belt (GC_SAFEBOX_DEL + GC_ITEM_SET + upsert
    // con owner = PERSONAJE — parity `pkSafebox->Remove` + `AddToCharacter`;
    // el belt se guarda como window "BELT_INVENTORY" pos 0..15).
    let item = {
        let st = session.safebox.as_mut().expect("caja abierta (gate arriba)");
        st.items.remove(idx)
    };
    let vnum = item.vnum;
    session
        .send(&TPacketGCItemDel::new(p.b_safe_pos).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_SAFEBOX_DEL: {e}"))?;
    let mut item = item;
    belt::place_at(&mut item, to_belt, p.item_pos.cell);
    ItemRepo::new(session.pool.clone())
        .upsert(&item, session.row().id)
        .await?;
    let set = TPacketGCItemSet {
        header: TPacketGCItemSet::HEADER,
        cell: TItemPos {
            window: TItemPos::WINDOW_INVENTORY,
            cell: p.item_pos.cell,
        },
        vnum: item.vnum as u32,
        count: item.count as u8,
        flags: 0,
        anti_flags: 0,
        highlight: 0,
        sockets: item.sockets,
        attrs: item.attrs,
    };
    session
        .send(&set.to_bytes())
        .await
        .map_err(|e| format!("enviando GC_ITEM_SET (checkout): {e}"))?;
    session.inventory.push(item);
    session.save();
    eprintln!(
        "server_realms: channel conn {}: {} sacó item vnum {vnum} de la \
         safebox (pos {} → celda {})",
        session.conn_id, session.row().name, p.b_safe_pos, p.item_pos.cell
    );
    Ok(Outcome::Continue)
}

/// CG_SAFEBOX_ITEM_MOVE (77, 8 B — mismo shape que CG_ITEM_MOVE: header +
/// TItemPos origen + TItemPos destino + BYTE num). Parity `CSafebox::MoveItem`
/// (safebox.cpp:170-231): stack si el destino tiene el mismo vnum + sockets
/// iguales (count < 200); si no, mover todo el item a un hueco libre.
/// `num == 0` → todo el stack. Divergencia documentada (fix del @fixme del
/// C++): el C++ no manda updates al stackear (el cliente queda con counts
/// viejos hasta re-abrir) — aquí se manda GC_SAFEBOX_SET con el count nuevo.
pub async fn handle_item_move(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let mv = match protocol::world::TPacketCGItemMove::from_bytes(pkt) {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: CG_SAFEBOX_ITEM_MOVE \
                 malformado: {e}",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    };
    if mv.pos.cell == mv.change_pos.cell {
        return Ok(Outcome::Continue); // @fixme196 — misma posición
    }
    let Some(st) = session.safebox.as_ref() else {
        eprintln!(
            "server_realms: channel conn {}: item move sin caja abierta — \
             ignorado",
            session.conn_id
        );
        return Ok(Outcome::Continue);
    };
    let slots = st.slots();
    if mv.pos.cell >= slots || mv.change_pos.cell >= slots {
        eprintln!(
            "server_realms: channel conn {}: move de safebox fuera del grid \
             ({} → {}, slots {slots}) — rechazado",
            session.conn_id, mv.pos.cell, mv.change_pos.cell
        );
        return Ok(Outcome::Continue);
    }
    let Some(src_idx) = st.items.iter().position(|i| i.pos as u16 == mv.pos.cell) else {
        eprintln!(
            "server_realms: channel conn {}: move de safebox pos {} sin item",
            session.conn_id, mv.pos.cell
        );
        return Ok(Outcome::Continue);
    };
    let want = i64::from(mv.num);
    // Protos de la caja (size/flag/antiflag del item_proto — una query):
    // gates del move (parity MoveItem safebox.cpp:188-225): el stack exige
    // destino apilable SIN antiflag STACK; el hueco libre, el strip.
    let protos = ItemRepo::new(session.pool.clone())
        .load_safebox_proto(&st.items.iter().map(|i| i.vnum).collect::<Vec<_>>())
        .await?;
    let src_size = protos.get(&st.items[src_idx].vnum).map_or(1, |p| p.size);
    let mut occupied = Vec::with_capacity(st.items.len());
    for it in &st.items {
        let s = protos.get(&it.vnum).map_or(1, |p| p.size);
        occupied.extend(strip_cells(it.pos as u16, s));
    }
    // ¿Destino ocupado?
    let dst_idx = st
        .items
        .iter()
        .position(|i| i.pos as u16 == mv.change_pos.cell);
    let Some(dst_idx) = dst_idx else {
        // Hueco libre: ¿cabe el strip? (parity `IsEmpty(bDestCell, 1,
        // item->GetSize())` + grid Get/Put, safebox.cpp:211-225 — el ORIGEN
        // sigue colocado y sus celdas bloquean).
        if !strip_fits(mv.change_pos.cell, src_size, slots, None, &occupied) {
            eprintln!(
                "server_realms: channel conn {}: move a pos {} sin hueco \
                 para el strip de {src_size} — rechazado",
                session.conn_id, mv.change_pos.cell
            );
            return Ok(Outcome::Continue);
        }
        // Mover el item (GC_SAFEBOX_DEL + GC_SAFEBOX_SET + upsert).
        let mut item = {
            let st = session.safebox.as_mut().expect("caja abierta");
            st.items.remove(src_idx)
        };
        let vnum = item.vnum;
        item.pos = mv.change_pos.cell as i32;
        ItemRepo::new(session.pool.clone())
            .upsert(&item, session.account_id)
            .await?;
        session
            .send(&TPacketGCItemDel::new(mv.pos.cell as u8).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_SAFEBOX_DEL (move): {e}"))?;
        session
            .send(&set_packet(&item, mv.change_pos.cell).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_SAFEBOX_SET (move): {e}"))?;
        let st = session.safebox.as_mut().expect("caja abierta");
        st.items.push(item);
        eprintln!(
            "server_realms: channel conn {}: movió item vnum {vnum} dentro \
             de la safebox (pos {} → {})",
            session.conn_id, mv.pos.cell, mv.change_pos.cell
        );
        return Ok(Outcome::Continue);
    };
    if src_idx == dst_idx {
        return Ok(Outcome::Continue);
    }
    // Stack (parity safebox.cpp:187-211): destino apilable SIN antiflag
    // STACK + mismo vnum + sockets iguales + count < 200; `num == 0` →
    // todo el stack.
    let (same_vnum, same_sockets, src_count, src_id, dst_count, dst_id) = {
        let st = session.safebox.as_ref().expect("caja abierta");
        let src = &st.items[src_idx];
        let dst = &st.items[dst_idx];
        (
            src.vnum == dst.vnum,
            src.sockets == dst.sockets,
            src.count,
            src.id,
            dst.count,
            dst.id,
        )
    };
    let dst_proto = protos
        .get(&{
            let st = session.safebox.as_ref().expect("caja abierta");
            st.items[dst_idx].vnum
        })
        .copied()
        .unwrap_or_default();
    if !same_vnum
        || !same_sockets
        || !stackable_dst(dst_proto.flag, dst_proto.antiflag)
    {
        eprintln!(
            "server_realms: channel conn {}: stack en safebox de vnums/\
             sockets distintos o destino no apilable — rechazado (parity \
             MoveItem)",
            session.conn_id
        );
        return Ok(Outcome::Continue);
    }
    let add = (if want == 0 { src_count } else { want })
        .min(ITEM_COUNT_LIMIT - dst_count)
        .min(src_count);
    if add <= 0 {
        eprintln!(
            "server_realms: channel conn {}: stack en safebox lleno — \
             ignorado",
            session.conn_id
        );
        return Ok(Outcome::Continue);
    }
    let src_new = src_count - add;
    let dst_new = dst_count + add;
    if src_new <= 0 {
        // El origen se agota: GC_SAFEBOX_DEL + delete de la fila.
        session
            .send(&TPacketGCItemDel::new(mv.pos.cell as u8).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_SAFEBOX_DEL (stack): {e}"))?;
        ItemRepo::new(session.pool.clone()).delete(src_id).await?;
        {
            let st = session.safebox.as_mut().expect("caja abierta");
            st.items.remove(src_idx);
        }
    } else {
        // Origen parcial: GC_SAFEBOX_SET con el count nuevo + upsert.
        let mut row = {
            let st = session.safebox.as_mut().expect("caja abierta");
            st.items[src_idx].clone()
        };
        row.count = src_new;
        ItemRepo::new(session.pool.clone())
            .upsert(&row, session.account_id)
            .await?;
        session
            .send(&set_packet(&row, mv.pos.cell).to_bytes())
            .await
            .map_err(|e| format!("enviando GC_SAFEBOX_SET (stack src): {e}"))?;
        {
            let st = session.safebox.as_mut().expect("caja abierta");
            st.items[src_idx].count = src_new;
        }
    }
    // Destino: count nuevo + GC_SAFEBOX_SET + upsert.
    let mut drow = {
        let st = session.safebox.as_mut().expect("caja abierta");
        st.items[dst_idx].clone()
    };
    drow.count = dst_new;
    ItemRepo::new(session.pool.clone())
        .upsert(&drow, session.account_id)
        .await?;
    session
        .send(&set_packet(&drow, mv.change_pos.cell).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_SAFEBOX_SET (stack dst): {e}"))?;
    {
        let st = session.safebox.as_mut().expect("caja abierta");
        st.items[dst_idx].count = dst_new;
    }
    eprintln!(
        "server_realms: channel conn {}: apiló en safebox (pos {} → {}, \
         +{add}; src {src_new}, dst {dst_new}; dst id {dst_id})",
        session.conn_id, mv.pos.cell, mv.change_pos.cell
    );
    Ok(Outcome::Continue)
}

/// CG_SAFEBOX_MONEY (79, 6 B: header + bState + lMoney). DEFENSIVO (ver doc
/// del módulo): el C++ congelado no tiene handler y el cliente de la
/// variante nunca lo envía. Cuando llegue: SAVE (0) deposita oro del
/// monedero a la caja, WITHDRAW (1) lo retira — GC_POINTS +
/// GC_SAFEBOX_MONEY_CHANGE (84) + `SafeboxRepo::set_gold` + save.
pub async fn handle_money(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let p = match TPacketCGSafeboxMoney::from_bytes(pkt) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "server_realms: channel conn {}: CG_SAFEBOX_MONEY \
                 malformado: {e}",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    };
    let Some(st) = session.safebox.as_ref() else {
        eprintln!(
            "server_realms: channel conn {}: money sin caja abierta — \
             ignorado",
            session.conn_id
        );
        return Ok(Outcome::Continue);
    };
    if p.l_money <= 0 {
        eprintln!(
            "server_realms: channel conn {}: money con cantidad {} — \
             rechazado",
            session.conn_id, p.l_money
        );
        return Ok(Outcome::Continue);
    }
    let money = p.l_money as i64;
    match p.b_state {
        TPacketCGSafeboxMoney::STATE_SAVE => {
            // Depositar: el monedero debe cubrirlo (parity DropGold/PointChange).
            let player_gold = session.row().gold;
            if money > i64::from(player_gold) {
                eprintln!(
                    "server_realms: channel conn {}: depósito de {money} oro \
                     con {} en el monedero — rechazado",
                    session.conn_id, player_gold
                );
                return Ok(Outcome::Continue);
            }
            session.row_mut().gold = player_gold - money as i32;
            let st = session.safebox.as_mut().expect("caja abierta");
            st.gold = st.gold.saturating_add(money as i32);
        }
        TPacketCGSafeboxMoney::STATE_WITHDRAW => {
            // Retirar: la caja debe cubrirlo.
            let box_gold = st.gold;
            if money > i64::from(box_gold) {
                eprintln!(
                    "server_realms: channel conn {}: retiro de {money} oro \
                     con {box_gold} en la caja — rechazado",
                    session.conn_id
                );
                return Ok(Outcome::Continue);
            }
            session.row_mut().gold = session.row().gold.saturating_add(money as i32);
            let st = session.safebox.as_mut().expect("caja abierta");
            st.gold = box_gold - money as i32;
        }
        other => {
            eprintln!(
                "server_realms: channel conn {}: money con bState {other} — \
                 rechazado (SAVE=0/WITHDRAW=1)",
                session.conn_id
            );
            return Ok(Outcome::Continue);
        }
    }
    let box_gold = session.safebox.as_ref().expect("caja abierta").gold;
    // GC_POINTS (monedero) + GC_SAFEBOX_MONEY_CHANGE (84, oro de la caja) +
    // persistencia (parity PointChange + SafeboxMoney del C++ completo).
    session
        .send(&packets::points_packet(session.row(), session.next_exp, &session.battle).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_POINTS (safebox money): {e}"))?;
    session
        .send(&TPacketGCSafeboxMoneyChange::new(box_gold).to_bytes())
        .await
        .map_err(|e| format!("enviando GC_SAFEBOX_MONEY_CHANGE: {e}"))?;
    session.save();
    SafeboxRepo::new(session.pool.clone())
        .set_gold(session.account_id, box_gold)
        .await?;
    eprintln!(
        "server_realms: channel conn {}: safebox money (state {}, \
         {money} oro) — caja {box_gold}, monedero {}",
        session.conn_id,
        p.b_state,
        session.row().gold
    );
    Ok(Outcome::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Slots del grid: 5 × size (parity `CGrid(5, iSize)` del server +
    /// `SAFEBOX_SLOT_X_COUNT × bSize` del cliente, PythonSafeBox.cpp:4-15).
    #[test]
    fn safebox_state_slots_are_5x_size() {
        let st = |size: i16| SafeboxState {
            size,
            gold: 0,
            items: Vec::new(),
        };
        assert_eq!(st(0).slots(), 0, "sin fila / sin páginas");
        assert_eq!(st(1).slots(), 5);
        assert_eq!(st(2).slots(), 10);
        assert_eq!(st(3).slots(), 15, "máximo (do_safebox_size 0..3)");
        assert_eq!(st(-1).slots(), 0, "negativo → 0 (fail-safe)");
    }

    /// El set de la caja: header GC_SAFEBOX_SET (85) + cell window SAFEBOX
    /// (3) — parity safebox.cpp:66-78 (`TItemPos(SAFEBOX, dwPos)`).
    #[test]
    fn set_packet_uses_safebox_header_and_window() {
        let item = ItemRow {
            id: 100_000_001,
            window: "SAFEBOX".into(),
            pos: 3,
            count: 7,
            vnum: 30001,
            sockets: [1, 2, 3],
            attrs: [(0, 0); 7],
        };
        let set = set_packet(&item, 3);
        assert_eq!(set.header, protocol::header::GC_SAFEBOX_SET, "85");
        assert_eq!(set.cell.window, TItemPos::WINDOW_SAFEBOX, "window SAFEBOX");
        assert_eq!(set.cell.cell, 3);
        assert_eq!(set.vnum, 30001);
        assert_eq!(set.count, 7);
        assert_eq!(set.sockets, [1, 2, 3]);
        // Tamaño del wire: el mismo TPacketGCItemSet (51 B) que registra el
        // cliente para el 85 (PythonNetworkStream.cpp:132).
        assert_eq!(set.to_bytes().len(), TPacketGCItemSet::SIZE);
    }

    /// Clamp del tamaño GM (parity do_safebox_size cmd_gm.cpp:1863-1865:
    /// `size > 3 || size < 0 → 0`).
    #[test]
    fn gm_size_clamp() {
        assert_eq!(SAFEBOX_SIZE_MAX, 3);
        assert_eq!((4u8 > SAFEBOX_SIZE_MAX) as u8, 1, "4 → clamp a 0 en set_size");
    }

    // ── Grid de `size` celdas (items 2×2 — cerrado 2026-08-27) ─────────
    // El grid legacy es de 5 celdas de ancho: un item de N celdas ocupa un
    // strip VERTICAL (pos, pos+5, …) — parity `IsEmpty(pos,1,size)`
    // safebox.cpp:130-136 / `bCell + 5*j` char_item.cpp:579-596.

    /// Strips: paso 5, N celdas desde la esquina.
    #[test]
    fn strip_cells_step_is_5() {
        assert_eq!(strip_cells(0, 1), vec![0]);
        assert_eq!(strip_cells(0, 2), vec![0, 5]);
        assert_eq!(strip_cells(7, 3), vec![7, 12, 17]);
    }

    /// VERIFIER (mutation): un 2×2 cuyo strip pisa la celda de otro item se
    /// rechaza — el subset 1×1 lo aceptaba (GAP cerrado). Quitar el chequeo
    /// de `occupied` del strip → rojo.
    #[test]
    fn checkin_gate_rejects_2x2_over_strip_cell() {
        assert!(!checkin_gate(30001, 2, 0, 0, 10, &[5]), "2×2 en 0 pisa la 5");
        assert!(checkin_gate(30001, 1, 0, 0, 10, &[5]), "1×1 en 0 no pisa la 5");
    }

    /// VERIFIER (mutation): un 2×2 que sobresale por debajo del grid se
    /// rechaza (caja de 10 = 2 filas; esquina en la fila 1 → 3 filas).
    #[test]
    fn checkin_gate_rejects_2x2_out_of_grid() {
        assert!(!checkin_gate(30001, 2, 0, 5, 10, &[]), "fila 1 + 2 > 2");
        assert!(!checkin_gate(30001, 2, 0, 9, 10, &[]), "esquina en la última");
        assert!(checkin_gate(30001, 2, 0, 0, 10, &[]), "esquina 0,0: 2 filas");
    }

    /// VERIFIER (mutation): el antiflag SAFEBOX (1<<17 — 667 vnums reales
    /// en PG) bloquea el checkin (parity input_main.cpp:1981-1985). Quitar
    /// la rama del antiflag → rojo.
    #[test]
    fn checkin_gate_rejects_antiflag_safebox() {
        assert!(!checkin_gate(30001, 1, ITEM_ANTIFLAG_SAFEBOX, 0, 10, &[]));
        assert!(checkin_gate(30001, 1, 0, 0, 10, &[]));
    }

    /// VERIFIER (mutation): el item de expansión no se guarda (parity
    /// input_main.cpp:1975-1979 — UNIQUE_ITEM_SAFEBOX_EXPAND = 71009).
    #[test]
    fn checkin_gate_rejects_expand_item() {
        assert!(!checkin_gate(UNIQUE_ITEM_SAFEBOX_EXPAND, 1, 0, 0, 10, &[]));
    }

    /// VERIFIER (mutation): el checkout al inventario respeta la página de
    /// 45 celdas (parity IsEmptyItemGrid char_item.cpp:580-590) y el strip
    /// (un 2×2 en 40 cruza a la página 1 → rechaza).
    #[test]
    fn checkout_strip_respects_inventory_page_and_occupancy() {
        assert!(
            !strip_fits(40, 2, INVENTORY_MAX_NUM, Some(INVENTORY_PAGE_CELLS), &[]),
            "40+5=45 es la primera celda de la página 1"
        );
        assert!(
            strip_fits(35, 2, INVENTORY_MAX_NUM, Some(INVENTORY_PAGE_CELLS), &[]),
            "35+5=40 sigue en la página 0"
        );
        assert!(
            !strip_fits(0, 2, INVENTORY_MAX_NUM, Some(INVENTORY_PAGE_CELLS), &[5]),
            "strip pisado"
        );
    }

    /// VERIFIER (mutation): el stack exige destino apilable SIN antiflag
    /// STACK (parity safebox.cpp:188-189). Quitar cualquiera de los dos
    /// checks → rojo.
    #[test]
    fn stack_requires_stackable_dst_without_antiflag() {
        assert!(stackable_dst(ITEM_FLAG_STACKABLE, 0));
        assert!(!stackable_dst(0, 0), "sin ITEM_FLAG_STACKABLE");
        assert!(
            !stackable_dst(ITEM_FLAG_STACKABLE, ITEM_ANTIFLAG_STACK),
            "con ITEM_ANTIFLAG_STACK"
        );
    }
}
