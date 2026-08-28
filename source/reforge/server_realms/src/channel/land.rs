//! LAND (phase 1 — land jugable): CG_LAND_BUY (56) / CG_LAND_TRANSFER (57).
//! Aditivos del reforge (ni el cliente v24 ni el C++ congelado los tienen —
//! `//HEADER_BLANK56/57` Packet.h): el id sale de la sequence PG
//! (`player.land_id_seq`), nunca de un contador de proceso. GAP documentado:
//! el gate de guild del jugador (el dueño legacy es una GUILD) entra cuando
//! la guild viva en la sesión — hoy el wire es verificable por el harness.
use super::session::{Outcome, Session};
use database::land::LandRepo;

fn rd32(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// CG_LAND_BUY (56, 29 B: header + map_index/x/y/width/height i32 en células
/// + price i64). Compra: insert con id de la sequence; el terreno nace SIN
/// dueño (guild_id 0 — la propiedad entra por CG_LAND_TRANSFER).
pub async fn handle_buy(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let b: &[u8; 29] = pkt.try_into().map_err(|_| "CG_LAND_BUY: 29 B")?;
    let (map_index, x, y, width, height) = (rd32(b, 1), rd32(b, 5), rd32(b, 9), rd32(b, 13), rd32(b, 17));
    let price = i64::from_le_bytes(b[21..29].try_into().expect("29 B"));
    if !(price > 0 && width > 0 && height > 0) {
        eprintln!(
            "server_realms: channel conn {}: CG_LAND_BUY rechazado (price/width/height <= 0)",
            session.conn_id
        );
        return Ok(Outcome::Continue);
    }
    let id = LandRepo::new(session.pool.clone())
        .buy(map_index as i64, x as i64, y as i64, width as i64, height as i64, price)
        .await?;
    eprintln!(
        "server_realms: channel conn {}: land comprado id {id} (mapa {map_index} {width}×{height}, price {price})",
        session.conn_id
    );
    Ok(Outcome::Continue)
}

/// CG_LAND_TRANSFER (57, 9 B: header + land_id u32 + new_owner u32).
/// Parity `CLand::SetOwner` (building.cpp:603-610): solo cambia el dueño.
pub async fn handle_transfer(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let b: &[u8; 9] = pkt.try_into().map_err(|_| "CG_LAND_TRANSFER: 9 B")?;
    let land_id = u32::from_le_bytes(b[1..5].try_into().expect("9 B"));
    let new_owner = u32::from_le_bytes(b[5..9].try_into().expect("9 B"));
    let n = LandRepo::new(session.pool.clone())
        .transfer(land_id as i64, new_owner as i64)
        .await?;
    eprintln!(
        "server_realms: channel conn {}: land {land_id} → dueño {new_owner} ({n} filas)",
        session.conn_id
    );
    Ok(Outcome::Continue)
}