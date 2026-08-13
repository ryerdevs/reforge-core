//! `ecs/systems/quest.rs` — el impl block QUEST del `WorldSim` (N1 → este
//! slice): traduce los `QuestIntent` (Load/Init/Event/Answer) en llamadas al
//! engine puro (`game_core::quest::QuestEngine`) y devuelve los
//! `QuestEvent::Run` para el routing del canal.
//!
//! Los recursos `QuestTable` (quests cargadas) y `QuestRuntimeStore` (estado
//! por jugador) viven en `game_core::quest` y se INITIALIZAN AQUÍ (lazy —
//! `init_resource`) — la fachada (`world.rs`) no cambia.

use std::collections::HashMap;

use crate::ecs::components::{Map, Player};
use crate::ecs::events::{NpcEvent, QuestEvent, QuestIntent};
use crate::ecs::resources::Rand;
use crate::ecs::world::WorldSim;
use crate::quest::{
    QuestEngine, QuestOutcome, QuestRuntime, QuestRuntimeStore, QuestTable, QuestTrigger,
};

impl WorldSim {
    /// Delegado de `Intent::Quest` — ver el módulo. `now_ms` = el reloj del
    /// server (get_time() = now_ms/1000 — el patrón de cooldowns del corpus).
    pub(crate) fn handle_quest(&mut self, q: QuestIntent, now_ms: u64) -> Vec<NpcEvent> {
        match q {
            QuestIntent::Load { text } => {
                self.world.init_resource::<QuestTable>();
                let mut table = self.world.resource_mut::<QuestTable>();
                match QuestEngine::load(&text) {
                    Ok(engine) => {
                        eprintln!("game_core: quests cargadas ({})", engine.quests().len());
                        table.engine = Some(engine);
                    }
                    Err(e) => {
                        eprintln!("game_core: carga de quests FALLIDA: {e}");
                        table.engine = None;
                    }
                }
                Vec::new()
            }
            QuestIntent::Init { player_vid, rows } => {
                self.world.init_resource::<QuestRuntimeStore>();
                let mut store = self.world.resource_mut::<QuestRuntimeStore>();
                store.0.insert(player_vid, QuestRuntime::load(&rows));
                Vec::new()
            }
            QuestIntent::Event { player_vid, trigger, items } => {
                let outcome = self.run_quest(player_vid, trigger, &items, now_ms, false, 0);
                match outcome {
                    Some(out) => vec![NpcEvent::Quest(QuestEvent::Run {
                        player_vid,
                        script: out.script,
                        effects: out.effects,
                        dirty: out.dirty,
                        suspended: out.suspended,
                    })],
                    None => Vec::new(),
                }
            }
            QuestIntent::Answer { player_vid, answer } => {
                let outcome = self.run_quest(player_vid, QuestTrigger::Login, &HashMap::new(), now_ms, true, i64::from(answer));
                match outcome {
                    Some(out) => vec![NpcEvent::Quest(QuestEvent::Run {
                        player_vid,
                        script: out.script,
                        effects: out.effects,
                        dirty: out.dirty,
                        suspended: out.suspended,
                    })],
                    None => Vec::new(),
                }
            }
        }
    }

    /// Ejecuta el engine para el jugador (evento o reanudación). `None` si el
    /// jugador no está en el mundo, no tiene runtime o no hay quests cargadas.
    fn run_quest(
        &mut self,
        player_vid: u32,
        trigger: QuestTrigger,
        items: &HashMap<u32, i64>,
        now_ms: u64,
        is_answer: bool,
        answer: i64,
    ) -> Option<QuestOutcome> {
        // Lookups que NO tocan el QuestTable (sin conflictos de borrow con el
        // take de abajo).
        let e = self.players.get(&player_vid).copied()?;
        let ent = self.world.get_entity(e).ok()?;
        let level = ent.get::<Player>()?.level;
        let map_index = ent.get::<Map>()?.map_index;
        self.world.init_resource::<QuestTable>();
        self.world.init_resource::<QuestRuntimeStore>();
        // El engine se SACA del recurso (owned) para no retener el borrow del
        // mundo durante la evaluación; se devuelve al final (o en el early
        // return del runtime ausente).
        let engine = self.world.resource_mut::<QuestTable>().engine.take()?;
        let mut rand = *self.world.resource::<Rand>();
        let mut store = self.world.resource_mut::<QuestRuntimeStore>();
        let Some(rt) = store.0.get_mut(&player_vid) else {
            self.world.resource_mut::<QuestTable>().engine = Some(engine);
            return None;
        };
        let mut rng = |min: i64, max: i64| {
            let lo = i32::try_from(min).unwrap_or(i32::MIN);
            let hi = i32::try_from(max).unwrap_or(i32::MAX);
            i64::from(rand.roll(lo, hi))
        };
        let out = if is_answer {
            engine.answer(rt, answer, level, map_index, (now_ms / 1000) as i64, items, &mut rng)
        } else {
            engine.run(rt, trigger, level, map_index, (now_ms / 1000) as i64, items, &mut rng)
        };
        // El borrow del store (y de rt) termina aquí — los resource_mut de
        // abajo vuelven a tomar el mundo.
        *self.world.resource_mut::<Rand>() = rand;
        self.world.resource_mut::<QuestTable>().engine = Some(engine);
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::events::Intent;
    use crate::ecs::test_util::*;
    use crate::quest::QuestEffect;

    /// Una quest corta con login condicionado, diálogo y select.
    const QUEST: &str = "\
quest hello
  state start
    on login with pc.level >= 5
      -> say(@hello)
quest branch
  state start
    on 20011.chat
      -> select(@_20_say, @_30_say) as choice
      if choice == 1
        -> warp(896500, 24600)
      else
        -> return
";

    fn quest_event(events: Vec<NpcEvent>) -> Option<QuestEvent> {
        events.into_iter().find_map(|e| match e {
            NpcEvent::Quest(q) => Some(q),
            _ => None,
        })
    }

    #[test]
    fn world_login_event_produces_script() {
        let mut w = world_with(42);
        join(&mut w); // vid 2, nivel 5 (harness)
        // Carga de quests + runtime del jugador.
        w.process_intent(QuestIntent::Load { text: QUEST.into() }.into(), 1_000);
        w.process_intent(QuestIntent::Init { player_vid: 2, rows: vec![] }.into(), 1_000);
        // login con nivel 5 → el diálogo say(@hello).
        let events = w.process_intent(
            Intent::Quest(QuestIntent::Event {
                player_vid: 2,
                trigger: QuestTrigger::Login,
                items: HashMap::new(),
            }),
            1_000,
        );
        let q = quest_event(events).expect("evento quest");
        let QuestEvent::Run { script, effects, dirty, suspended, .. } = q;
        assert_eq!(script.as_deref(), Some("hello[ENTER]"), "diálogo say");
        assert!(effects.is_empty());
        assert!(dirty.iter().any(|d| d.flag.ends_with(".__status")), "{dirty:?}");
        assert!(!suspended);
    }

    #[test]
    fn world_select_suspends_and_answer_resumes() {
        let mut w = world_with(42);
        join(&mut w);
        w.process_intent(QuestIntent::Load { text: QUEST.into() }.into(), 1_000);
        w.process_intent(QuestIntent::Init { player_vid: 2, rows: vec![] }.into(), 1_000);
        let events = w.process_intent(
            Intent::Quest(QuestIntent::Event {
                player_vid: 2,
                trigger: QuestTrigger::Chat(20011),
                items: HashMap::new(),
            }),
            1_000,
        );
        let q = quest_event(events).expect("evento quest");
        let QuestEvent::Run { script, suspended, .. } = q;
        assert!(script.as_deref().unwrap_or("").contains("[QUESTION 1;_20_say|2;_30_say]"));
        assert!(suspended);
        // La respuesta 1 → la rama warp.
        let events = w.process_intent(
            Intent::Quest(QuestIntent::Answer { player_vid: 2, answer: 1 }),
            1_000,
        );
        let q = quest_event(events).expect("reanudación");
        let QuestEvent::Run { effects, suspended, .. } = q;
        assert_eq!(effects, vec![QuestEffect::Warp { x: 896500, y: 24600 }]);
        assert!(!suspended);
    }

    #[test]
    fn world_quest_requires_init_runtime() {
        let mut w = world_with(42);
        join(&mut w);
        w.process_intent(QuestIntent::Load { text: QUEST.into() }.into(), 1_000);
        // Sin Init: el evento no produce nada (sin runtime).
        let events = w.process_intent(
            Intent::Quest(QuestIntent::Event {
                player_vid: 2,
                trigger: QuestTrigger::Login,
                items: HashMap::new(),
            }),
            1_000,
        );
        assert!(events.is_empty());
    }
}
