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
            QuestIntent::Load { text, texts } => {
                self.world.init_resource::<QuestTable>();
                let mut table = self.world.resource_mut::<QuestTable>();
                match QuestEngine::load(&text).map(|e| e.with_texts(texts)) {
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
            QuestIntent::Event {
                player_vid,
                trigger,
                items,
            } => {
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
            // CLICK a un NPC (wiring 2026-08-13): el vnum del vid (NpcIndex →
            // Mob) dispara el trigger Chat(vnum) — las quests del NPC ofrecen
            // su diálogo. Sin vnum resoluble o sin quests → sin evento.
            QuestIntent::NpcClick {
                player_vid,
                npc_vid,
                items,
            } => {
                self.world
                    .init_resource::<crate::ecs::resources::NpcIndex>();
                let npc = self
                    .world
                    .resource::<crate::ecs::resources::NpcIndex>()
                    .0
                    .get(&npc_vid)
                    .copied();
                let Some(ent) = npc.and_then(|e| self.world.get_entity(e).ok()) else {
                    return Vec::new();
                };
                let Some(mob) = ent.get::<crate::ecs::components::Mob>() else {
                    return Vec::new();
                };
                let outcome = self.run_quest(
                    player_vid,
                    QuestTrigger::Chat(mob.vnum as u32),
                    &items,
                    now_ms,
                    false,
                    0,
                );
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
                let outcome = self.run_quest(
                    player_vid,
                    QuestTrigger::Login,
                    &HashMap::new(),
                    now_ms,
                    true,
                    i64::from(answer),
                );
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
            // CG_QUEST_INPUT_STRING (30): el texto del diálogo de input. El
            // engine aún no implementa la acción `input` del DSL (mod.rs
            // §Cobertura — mapeada-pendiente): se loguea y no-op. El intent
            // existe para que el wire NO caiga en `other` y el día que el
            // engine tenga `input` solo se toque este brazo.
            QuestIntent::Input { player_vid, text } => {
                eprintln!(
                    "game_core: quest input de pid {player_vid}: {:?} — \
                     engine sin `input` (GAP documentado), no-op",
                    text.chars().take(64).collect::<String>()
                );
                Vec::new()
            }
            // CG_QUEST_CONFIRM (31): confirmación cross-player (requestPID).
            // El engine no tiene Confirm — log + no-op (GAP documentado).
            QuestIntent::Confirm {
                player_vid,
                answer,
                request_pid,
            } => {
                eprintln!(
                    "game_core: quest confirm de pid {player_vid}: answer {answer} \
                     para el requestPID {request_pid} — engine sin Confirm \
                     (GAP documentado), no-op"
                );
                Vec::new()
            }
            // CG_SCRIPT_BUTTON (66): botón de diálogo/ventana de quest (parity `ScriptButton` input_main.cpp:1850-1868).
            QuestIntent::Button { player_vid, idx } => {
                let info = idx & 0x8000_0000 != 0;
                let trigger = if info {
                    QuestTrigger::Info
                } else {
                    QuestTrigger::Button
                };
                let outcome =
                    self.run_quest(player_vid, trigger, &HashMap::new(), now_ms, false, 0);
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
            engine.answer(
                rt,
                answer,
                level,
                map_index,
                (now_ms / 1000) as i64,
                items,
                &mut rng,
            )
        } else {
            engine.run(
                rt,
                trigger,
                level,
                map_index,
                (now_ms / 1000) as i64,
                items,
                &mut rng,
            )
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
        w.process_intent(
            QuestIntent::Load {
                text: QUEST.into(),
                texts: HashMap::new(),
            }
            .into(),
            1_000,
        );
        w.process_intent(
            QuestIntent::Init {
                player_vid: 2,
                rows: vec![],
            }
            .into(),
            1_000,
        );
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
        let QuestEvent::Run {
            script,
            effects,
            dirty,
            suspended,
            ..
        } = q;
        assert_eq!(script.as_deref(), Some("hello[ENTER]"), "diálogo say");
        assert!(effects.is_empty());
        assert!(
            dirty.iter().any(|d| d.flag.ends_with(".__status")),
            "{dirty:?}"
        );
        assert!(!suspended);
    }

    #[test]
    fn world_select_suspends_and_answer_resumes() {
        let mut w = world_with(42);
        join(&mut w);
        w.process_intent(
            QuestIntent::Load {
                text: QUEST.into(),
                texts: HashMap::new(),
            }
            .into(),
            1_000,
        );
        w.process_intent(
            QuestIntent::Init {
                player_vid: 2,
                rows: vec![],
            }
            .into(),
            1_000,
        );
        let events = w.process_intent(
            Intent::Quest(QuestIntent::Event {
                player_vid: 2,
                trigger: QuestTrigger::Chat(20011),
                items: HashMap::new(),
            }),
            1_000,
        );
        let q = quest_event(events).expect("evento quest");
        let QuestEvent::Run {
            script, suspended, ..
        } = q;
        assert!(
            script
                .as_deref()
                .unwrap_or("")
                .contains("[QUESTION 1;_20_say|2;_30_say]")
        );
        assert!(suspended);
        // La respuesta 1 → la rama warp.
        let events = w.process_intent(
            Intent::Quest(QuestIntent::Answer {
                player_vid: 2,
                answer: 1,
            }),
            1_000,
        );
        let q = quest_event(events).expect("reanudación");
        let QuestEvent::Run {
            effects, suspended, ..
        } = q;
        assert_eq!(
            effects,
            vec![QuestEffect::Warp {
                x: 896500,
                y: 24600
            }]
        );
        assert!(!suspended);
    }

    /// CLICK a un NPC (wiring 2026-08-13): el mundo resuelve el vnum del vid
    /// (NpcIndex -> Mob) y dispara el trigger Chat(vnum) — la quest del NPC
    /// con `when <vnum>.chat` ofrece su diálogo (select -> suspende).
    #[test]
    fn npc_click_offers_quest_dialog() {
        let mut w = world_with(42);
        join(&mut w);
        // El NPC 20011 materializado (vid 10000 — primer vid del allocador).
        let mut row = mob_row(20011);
        row.ai_flag = Some("NOMOVE".into());
        load(&mut w, vec![(entry(20011, 100, 0, 1), row)]);
        w.update(500); // materializa (dist 100 < SPAWN_VIEW)
        w.process_intent(
            QuestIntent::Load {
                text: QUEST.into(),
                texts: HashMap::new(),
            }
            .into(),
            1_000,
        );
        w.process_intent(
            QuestIntent::Init {
                player_vid: 2,
                rows: vec![],
            }
            .into(),
            1_000,
        );
        let events = w.process_intent(
            Intent::Quest(QuestIntent::NpcClick {
                player_vid: 2,
                npc_vid: 10_000,
                items: HashMap::new(),
            }),
            1_000,
        );
        let q = quest_event(events).expect("diálogo del NPC");
        let QuestEvent::Run {
            script, suspended, ..
        } = q;
        assert!(
            script
                .as_deref()
                .unwrap_or("")
                .contains("[QUESTION 1;_20_say|2;_30_say]"),
            "{script:?}"
        );
        assert!(
            suspended,
            "el select suspende — CG_SCRIPT_ANSWER lo reanuda"
        );
    }

    #[test]
    fn world_quest_requires_init_runtime() {
        let mut w = world_with(42);
        join(&mut w);
        w.process_intent(
            QuestIntent::Load {
                text: QUEST.into(),
                texts: HashMap::new(),
            }
            .into(),
            1_000,
        );
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

    /// Los intents del lane D (CG_QUEST_INPUT_STRING / CG_QUEST_CONFIRM /
    /// CG_SCRIPT_BUTTON) NO rompen el mundo: se procesan (loguean el GAP del
    /// engine — input/confirm/button pendientes) y no producen eventos ni
    /// panics. El día que el engine los implemente, el wire ya está cableado.
    #[test]
    fn lane_d_quest_intents_are_noop_but_handled() {
        let mut w = world_with(42);
        join(&mut w);
        w.process_intent(
            QuestIntent::Load {
                text: QUEST.into(),
                texts: HashMap::new(),
            }
            .into(),
            1_000,
        );
        w.process_intent(
            QuestIntent::Init {
                player_vid: 2,
                rows: vec![],
            }
            .into(),
            1_000,
        );
        // Input de texto.
        let events = w.process_intent(
            Intent::Quest(QuestIntent::Input {
                player_vid: 2,
                text: "respuesta".into(),
            }),
            1_000,
        );
        assert!(events.is_empty(), "input: no-op sin eventos");
        // Confirm cross-player.
        let events = w.process_intent(
            Intent::Quest(QuestIntent::Confirm {
                player_vid: 2,
                answer: 1,
                request_pid: 99,
            }),
            1_000,
        );
        assert!(events.is_empty(), "confirm: no-op sin eventos");
        // Button (idx normal y con el bit de QuestInfo).
        let events = w.process_intent(
            Intent::Quest(QuestIntent::Button {
                player_vid: 2,
                idx: 3,
            }),
            1_000,
        );
        assert!(events.is_empty(), "button: no-op sin eventos");
        let events = w.process_intent(
            Intent::Quest(QuestIntent::Button {
                player_vid: 2,
                idx: 0x8000_0001,
            }),
            1_000,
        );
        assert!(events.is_empty(), "button QuestInfo: no-op sin eventos");
    }
}
