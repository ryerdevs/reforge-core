//! # `game_core::quest` — el runtime de las quests DSL (F5.x)
//!
//! Conecta el catálogo tipado de `quest_dsl` (parser + expansión de familias,
//! FINAL) con el mundo: el engine evalúa los triggers/condiciones/acciones
//! del DSL para UN jugador y produce efectos accionables que la conexión
//! traduce a paquetes GC y a persistencia.
//!
//! Semántica portada del runtime legacy (`questmanager.cpp`/`questpc.cpp`):
//! - El estado actual de la quest = el flag `{quest}.__status` (índice 1-based
//!   del state en el orden del archivo — parity `PC::SetCurrentQuestStateName`,
//!   questpc.cpp:115-118). 0 = quest no empezada → los eventos del estado
//!   `start` (índice 1) pueden ARRANCARLA (parity `FuncMissHandleEvent`).
//! - Los flags `pc.setqf` = filas `player.quest` (quest, flag, valor; valor 0
//!   = DELETE — parity `QUERY_QUEST_SAVE`). El repo ya está migrado
//!   (`database::quest::QuestRepo`); la persistencia es save-by-event desde la
//!   conexión (patrón ADR-0008, igual que items).
//! - `wait()`/`select()` SUSPENDEN el evento: el diálogo (GC_SCRIPT 45) se
//!   envía con `[NEXT]`/`[QUESTION …]` y la reanudación llega por
//!   `CG_SCRIPT_ANSWER` (parity `GotoSelectState`/`GotoNextState`,
//!   questlua.cpp:901-937). Mientras hay una quest suspendida, ningún otro
//!   evento corre (parity `pc.IsRunning()`).
//! - `get_time()` = segundos del reloj del server (el patrón del corpus:
//!   `set_qf(duration, get_time()+60*60*22)` + `on login with get_time() >=
//!   get_qf(duration)`).
//!
//! ## Cobertura del catálogo (actualizada 2026-08-30 — G3.1c)
//!
//! | Acción DSL | Estado |
//! |---|---|
//! | say / say_title / wait / select / set_state / set_qf / give_item2 / remove_item / warp / notice / return / say_reward / send_letter / set_quest_state / target_vid / target_delete / affect_add / affect_remove | **Implementadas** (con verifiers en engine.rs) |
//! | clear_letter / say_item_vnum / notice_multiline | mapeadas-pero-pendientes (se loguean, no fallan) |
//! | input_number | **Pendiente** — G2.5 en el Gap Registry |
//!
//! Condiciones: todo el catálogo del spec §4 (pc.level, count_item — con el
//! snapshot del inventario que pasa la conexión, get_qf, number, get_time,
//! get_map_index, get_gm_level=0, pet.is_summon=0, is_test_server=0,
//! comparaciones/aritmética/between).
//!
//! ## Wire (verificado contra Packet.h)
//!
//! `GC_SCRIPT` (45): header + size(WORD, = 6 + src) + skin + src_size(WORD) y
//! markup — parity `packet_script` (packet.h:1250-1259): el `TPacketGCScript`
//! del cliente es de 6 B (Packet.h:1874-1879; el server desplegado no define
//! ENABLE_QUEST_CATEGORY). Markup del event-set del cliente: `texto[ENTER]`,
//! `[NEXT]`, `[QUESTION 1;key|2;key]`, `[DONE]` (PythonEventManager.cpp:
//! 466-504). La respuesta: `CG_SCRIPT_ANSWER` (29, 2 B: header + answer —
//! Packet.h:679). El notice usa `GC_CHAT` (4, CHAT_TYPE_NOTICE —
//! questlua_global.cpp:133-139); el warp, `GC_WARP` (65, 15 B —
//! protocol::world::TPacketGCWarp).

pub mod engine;

pub use engine::{
    DirtyFlag, EvalCtx, PersistedFlag, QuestEffect, QuestEngine, QuestOutcome, QuestRuntime,
    QuestTrigger, Suspension,
};

use bevy_ecs::prelude::Resource;

/// Recurso del mundo ECS: las quests cargadas (`QuestIntent::Load` — el canal
/// las envía al arrancar). El engine se init-ALIZA aquí (lazy) desde
/// `ecs/systems/quest.rs` — la fachada del mundo no cambia.
#[derive(Resource, Default)]
pub struct QuestTable {
    pub engine: Option<QuestEngine>,
}

/// Recurso del mundo ECS: el runtime de quests por jugador
/// (`QuestIntent::Init/Event/Answer` — la conexión carga las filas
/// persistidas en el entry).
#[derive(Resource, Default)]
pub struct QuestRuntimeStore(pub std::collections::HashMap<u32, QuestRuntime>);
