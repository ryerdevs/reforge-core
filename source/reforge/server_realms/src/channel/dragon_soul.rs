//! channel/dragon_soul.rs — DS refine phase2: fee+mat reales vía refine_proto.
use super::session::{Outcome, Session};
use database::dragon_soul::DragonSoulRepo;
use database::item::ItemRepo;
use game_core::dragon_soul::{ds_fee, ds_is_success};
const NOT_ENOUGH_MAT: u8 = 10;
const NOT_ENOUGH_MONEY: u8 = 8;
const SUCCEED: u8 = 11;
fn parse(b: &[u8]) -> Option<(u8, [[u8; 3]; 15])> {
    let a: &[u8; 47] = b.try_into().ok()?;
    let mut g = [[0u8; 3]; 15];
    for i in 0..15 {
        g[i] = [a[2 + i * 3], a[3 + i * 3], a[4 + i * 3]];
    }
    Some((a[1], g))
}
fn roll() -> i32 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    ((n ^ C
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_mul(0x9E3779B97F4A7C15))
        % 100) as i32
        + 1
}
pub async fn handle_refine(session: &mut Session, pkt: &[u8]) -> Result<Outcome, String> {
    let Some((sub, grid)) = parse(pkt) else {
        return Err("CG_DRAGON_SOUL_REFINE: 47 B".into());
    };
    if !(1..=4).contains(&sub) {
        return Ok(Outcome::Continue);
    }
    let mut vnum = 0i64;
    for c in grid {
        if c == [0, 0, 0] {
            continue;
        }
        let cell = u16::from(c[1]) | (u16::from(c[2]) << 8);
        if let Some(it) = session.inventory.iter().find(|r| r.pos as u16 == cell) {
            vnum = it.vnum;
            break;
        }
    }
    let mut result = NOT_ENOUGH_MAT;
    if vnum != 0 {
        let repo = ItemRepo::new(session.pool.clone());
        if let Some((set, refined)) = repo.load_refine_proto(vnum).await?
            && refined != 0
            && let Some(recipe) = repo.load_refine_recipe(set).await?
        {
            let fee = ds_fee(&recipe);
            if i64::from(session.row().gold) < fee {
                result = NOT_ENOUGH_MONEY;
            } else {
                let mut ok = true;
                for &(mv, mc) in &recipe.materials {
                    if mv == 0 || mc <= 0 {
                        continue;
                    }
                    let have: i64 = session
                        .inventory
                        .iter()
                        .filter(|r| r.vnum == mv)
                        .map(|r| r.count)
                        .sum();
                    if have < i64::from(mc) {
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    result = NOT_ENOUGH_MAT;
                } else {
                    session.row_mut().gold = session.row().gold.saturating_sub(fee as i32);
                    session.save();
                    for &(mv, mc) in &recipe.materials {
                        if mv == 0 || mc <= 0 {
                            continue;
                        }
                        let mut need = i64::from(mc);
                        let ids: Vec<i64> = session
                            .inventory
                            .iter()
                            .filter(|r| r.vnum == mv)
                            .map(|r| r.id)
                            .collect();
                        for id in ids {
                            if need <= 0 {
                                break;
                            }
                            if let Some(idx) = session.inventory.iter().position(|r| r.id == id) {
                                let take = need.min(session.inventory[idx].count);
                                session.inventory[idx].count -= take;
                                need -= take;
                                if session.inventory[idx].count <= 0 {
                                    let did = session.inventory[idx].id;
                                    repo.delete(did).await?;
                                    session.inventory.remove(idx);
                                } else {
                                    let row = session.inventory[idx].clone();
                                    repo.upsert(&row, session.row().id).await?;
                                }
                            }
                        }
                    }
                    result = if ds_is_success(&recipe, roll()) {
                        SUCCEED
                    } else {
                        5
                    };
                }
            }
        }
    }
    let id = DragonSoulRepo::new(session.pool.clone())
        .record(session.row().id, i16::from(sub))
        .await?;
    session
        .send(&[protocol::header::GC_DRAGON_SOUL_REFINE, result, 0, 0, 0])
        .await
        .map_err(|e| format!("GC_DS_REFINE:{e}"))?;
    eprintln!(
        "server_realms: conn {} ds refine {sub}->{result} ledger {id}",
        session.conn_id
    );
    Ok(Outcome::Continue)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_layout_is_byte_exact() {
        let mut pkt = [0u8; 47];
        pkt[0] = 205;
        pkt[1] = 3;
        assert_eq!(parse(&pkt).unwrap().0, 3);
        assert!(parse(&[205; 46]).is_none());
        assert!(parse(&[205; 48]).is_none());
        let mut o = pkt;
        o[1] = 0;
        assert_eq!(parse(&o).unwrap().0, 0);
    }
    #[test]
    fn verifier_uses_refine_proto_fee_and_prob() {
        let r = database::item::RefineRecipe {
            cost: 9999,
            prob: 42,
            materials: [(0, 0); 5],
        };
        assert_eq!(ds_fee(&r), 9999);
        assert!(ds_is_success(&r, 42));
        assert!(!ds_is_success(&r, 43));
    }
}
