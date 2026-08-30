//! DUNGEON + RAID: parity `dungeon.cpp` (WAIT→START→END, ids monotónicos :477)
use std::sync::atomic::{AtomicU32, Ordering};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DungeonState {
    Wait,
    Start,
    End,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dungeon {
    pub id: u32,
    pub map_index: i32,
    pub party_id: u32,
    pub state: DungeonState,
    pub members: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Raid {
    pub id: u32,
    pub boss_vnum: u32,
}
pub fn create_dungeon(party_id: u32, map_index: i32) -> Dungeon {
    static NEXT_ID: AtomicU32 = AtomicU32::new(1);
    Dungeon {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        map_index,
        party_id,
        state: DungeonState::Wait,
        members: 0,
    }
}
pub fn enter(d: &mut Dungeon) -> bool {
    if d.state == DungeonState::End {
        return false;
    }
    d.state = DungeonState::Start;
    d.members += 1;
    true
}
pub fn exit(d: &mut Dungeon) {
    d.members = d.members.saturating_sub(1);
    if d.members == 0 && d.state == DungeonState::Start {
        d.state = DungeonState::End;
    }
}
pub fn is_in_dungeon(player_party: Option<u32>, dungeon: &Dungeon) -> bool {
    player_party.is_some_and(|p| p == dungeon.party_id)
}
pub fn spawn_raid(id: u32, boss_vnum: u32) -> Option<Raid> {
    if boss_vnum == 0 {
        None
    } else {
        Some(Raid { id, boss_vnum })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verifier_dungeon_lifecycle() {
        let a = create_dungeon(7, 41);
        let b = create_dungeon(7, 41);
        assert_ne!(a.id, b.id, "ids únicos");
        let mut d = create_dungeon(7, 41);
        assert_eq!(d.state, DungeonState::Wait);
        assert!(enter(&mut d));
        assert_eq!(d.state, DungeonState::Start);
        assert!(enter(&mut d));
        exit(&mut d);
        assert_eq!(d.state, DungeonState::Start);
        exit(&mut d);
        assert_eq!(d.state, DungeonState::End);
        assert!(!enter(&mut d));
        assert!(
            is_in_dungeon(Some(7), &d) && !is_in_dungeon(Some(8), &d) && !is_in_dungeon(None, &d)
        );
    }
    #[test]
    fn verifier_raid_spawn() {
        assert!(spawn_raid(1, 0).is_none(), "boss 0 inválido");
        let r = spawn_raid(1, 1093).unwrap();
        assert_eq!(r.id, 1);
        assert_eq!(r.boss_vnum, 1093);
        let r2 = spawn_raid(2, 1093).unwrap();
        assert_ne!(r.id, r2.id);
        assert_eq!(r2.boss_vnum, 1093);
        assert!(spawn_raid(99, 0).is_none() && spawn_raid(99, 1).is_some());
    }
}
