//! Componentes del mundo ECS (el estado del mundo, SoA en el almacén de
//! bevy). Definidos aquí y usados por los sistemas (`ecs::systems/*`) y los
//! métodos del `WorldSim` (`ecs::world.rs`).

use std::collections::HashSet;

use bevy_ecs::prelude::*;

use crate::npc::SpawnEntry;
use database::npc::MobRow;

/// VID del wire de la entidad (parity AllocVID — NPCs 10 000+, items del
/// suelo 50 000+, PCs ids bajos). Los asigna el recurso `VidAlloc` (GLOBAL
/// del canal — los eventos lo llevan para que el canal construya los
/// paquetes GC).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vid {
    pub vid: u32,
}

/// Posición en UNITS (parity del canal — x/y del mob/player/item).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

/// HP runtime (mobs y jugador — los máximos son estáticos).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hp {
    pub hp: i32,
    pub max_hp: i32,
}

/// Hostilidad del mob: `Some(player_entity)` = persigue/ataca a ESE jugador
/// (multi-jugador — el detect elige al más cercano; parity `FindVictim`).
/// Se marca al recibir daño (parity `OnDamage`) y por el aggro proactivo.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Aggro {
    pub target: Option<Entity>,
}

/// C29: momento (ms del `WorldClock`) del ÚLTIMO ataque de este mob — el
/// cooldown del golpe (`CalculateDuration(attack_speed, 2000)` ms, parity
/// char_state.cpp:1005-1012) se computa contra ESTE instante. Empieza en 0
/// (el mob ataca en el primer tick en rango, como el C++ que no tiene
/// last-attack al spawn).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastAttack {
    pub at_ms: u64,
}

/// Stats ESTÁTICAS del mob (una copia por entidad, del `mob_proto` vía
/// `Mob::from_row`) — lo que los sistemas y el combate leen.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Mob {
    pub vnum: i64,
    /// Recompensas del kill (parity mob_proto: exp/gold del reward).
    pub exp: i64,
    pub gold_min: i32,
    pub gold_max: i32,
    /// Drop primario del kill (0 = sin drop) — `mob_proto.drop_item`.
    pub drop_item: i64,
    /// `move_speed` (UNITS/seg) — el paso del AI por tick.
    pub move_speed: i32,
    /// `attack_speed` (default 100) — el cooldown del golpe del mob
    /// (`CalculateDuration(attack_speed, 2000)` ms — C29).
    pub attack_speed: i32,
    /// `damage_min`/`damage_max` — el daño del ataque del mob.
    pub damage_min: i32,
    pub damage_max: i32,
    /// Posición del SPAWN (home) — el patrullaje clampa el destino al radio.
    pub home_x: i32,
    pub home_y: i32,
    /// `ai_flag` "NOMOVE" → el mob NO patrulla (parity AIFLAG_NOMOVE).
    pub nomove: bool,
    /// `aggressive_sight` (UNITS) — rango del aggro proactivo y del de-aggro.
    pub aggressive_sight: i32,
    /// `ai_flag` "AGGR" → aggro proactivo (parity AIFLAG_AGGRESSIVE).
    pub aggressive: bool,
    // ---- combate (la vista `NpcState` que `game_core::combat` consume) ----
    pub level: i32,
    /// `ht` del PG — la "dx" del mob en el NpcState (parity channel.rs:426).
    pub ht: i32,
    /// `def` del PG — la `wDef` del mob (`tables.h:463`).
    pub wdef: i32,
    /// `battle_type` (MELEE = 0 extiende el rango del atacante PC).
    pub battle_type: u8,
    /// `attack_range` (UNITS, p.ej. mob 101 = 175).
    pub attack_range: u32,
}

impl Mob {
    /// Construye el componente desde el entry del spawn + la fila del
    /// `mob_proto` (mismo mapeo que el `LiveNpc` del canal, channel.rs:417-451).
    pub fn from_row(entry: &SpawnEntry, row: &MobRow) -> Self {
        Self {
            vnum: row.vnum,
            exp: row.exp,
            gold_min: row.gold_min,
            gold_max: row.gold_max,
            drop_item: row.drop_item,
            move_speed: row.move_speed,
            attack_speed: row.attack_speed,
            damage_min: row.damage_min,
            damage_max: row.damage_max,
            home_x: entry.x,
            home_y: entry.y,
            nomove: row
                .ai_flag
                .as_deref()
                .is_some_and(|f| f.contains("NOMOVE")),
            aggressive_sight: row.aggressive_sight,
            aggressive: crate::ai::is_aggressive(row.ai_flag.as_deref()),
            level: row.level,
            ht: row.ht,
            wdef: row.def,
            battle_type: row.battle_type as u8,
            attack_range: row.attack_range as u32,
        }
    }
}

/// Item EN EL SUELO del mundo (el pickup lo consume; el cliente lo pinta con
/// el `GC_ITEM_GROUND_ADD`). La posición vive en `Position`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    pub vnum: u32,
    pub count: u32,
    pub z: i32,
}

/// Stats de combate del jugador que el AI del mob usa (parity
/// `player_def_grade`: level + ht/1.25 + iArmor) + las que `handle_attack`
/// necesita (job/st/dx/iq — estáticas por personaje en el subset actual).
/// `vid` = el id del player (routing de eventos). `armor` se sincroniza
/// SOLO al equipar/desequipar; `level` al subir de nivel.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Player {
    pub vid: u32,
    pub level: i32,
    pub ht: i32,
    pub armor: i32,
    pub job: u8,
    pub st: i32,
    pub dx: i32,
    pub iq: i32,
}

/// Cooldown del combate del jugador (parity `m_kAttackLog` — battle.cpp:
/// 784-794). Vive en la entidad del jugador (muere al salir del mundo).
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Combat(pub crate::combat::CombatState);

/// SP del jugador (el coste de las skills lo paga de aquí — parity
/// `PointChange(POINT_SP, -iNeededSP)`; se sincroniza con row.mp: pociones,
/// revive — `Intent::SetMp`).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mp {
    pub mp: i32,
    pub max_mp: i32,
}

/// El blob de niveles de skills del jugador (255 × TPlayerSkill de 6 B —
/// `player.skill_level`; el nivel de la skill N vive en `N*6+1`).
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillLevels(pub Vec<u8>);

/// Cooldowns de skills del jugador (skill_id → ms del server del próximo
/// uso — parity `TSkillUseInfo::dwNextSkillUsableTime`, char_skill.cpp:94-121).
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillCooldowns(pub std::collections::HashMap<u32, u64>);

/// Buffs activos (server-timed — parity `CAffect`/`ProcessAffect`,
/// char_affect.cpp:204-260): el sistema `affects_system` decrementa
/// `duration_ms` cada tick y revierte/emite `AffectRemoved` al expirar.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct Affects(pub Vec<Affect>);

/// Un buff activo (los campos del wire `TPacketAffectElement` + el reloj).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Affect {
    /// `dwType` del wire — el vnum del skill (el icono del cliente).
    pub skill_id: u32,
    /// `bApplyOn` del wire — el POINT_* que el buff modifica.
    pub point: u8,
    /// `lApplyValue` del wire — el valor del buff.
    pub value: i32,
    /// `dwFlag` del wire — el AFF_* (icono).
    pub flag: u32,
    /// Segundos restantes del buff (el wire `lDuration`).
    pub duration_ms: u64,
    /// `lSPCost` del wire (0 para los skills del subset).
    pub sp_cost: i32,
}

impl Affects {
    /// La suma de los buffs de DEF (parity `POINT_DEF_GRADE_BONUS` — el
    /// total DEF_GRADE = base + bonus; el ataque del mob la resta).
    pub fn def_grade_bonus(&self) -> i32 {
        self.0
            .iter()
            .filter(|a| a.point == crate::skill::point::DEF_GRADE_BONUS)
            .map(|a| a.value)
            .sum()
    }

    /// La suma de los buffs de ATT (parity `POINT_ATT_GRADE_BONUS` — el
    /// `PlayerState.att_grade_bonus` del ataque del jugador).
    pub fn att_grade_bonus(&self) -> i32 {
        self.0
            .iter()
            .filter(|a| a.point == crate::skill::point::ATT_GRADE_BONUS)
            .map(|a| a.value)
            .sum()
    }
}

/// Mapa de la entidad (players y mobs). Las tablas de spawn y los sistemas
/// (aggro/patrulla/despawn) son por mapa — parity de los sectrees del C++.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Map {
    pub map_index: u32,
}

/// Marca qué entrada de la `SpawnTable` materializó la entidad (mapa+índice)
/// — el `spawn_despawn_system` la usa para no re-materializar.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnRef {
    pub map: u32,
    pub index: usize,
}

/// Jugadores que YA recibieron el ADD de esta entidad (seen per-player del
/// spawn dinámico — REGRESIÓN bench 2026-08-13: el ADD es por vista de
/// jugador, no "una vez por mundo"). Al salir un jugador de la vista se
/// olvida — el re-ADD al volver es parity del re-entry al sectree del C++.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct SpawnSeen(pub HashSet<u32>);
