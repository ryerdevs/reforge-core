---
Type: Snapshot
Status: Current
Audience: Contributors
Last verified: 2026-08-29
---

# Progress — Metin2 Reforge

## Goal

Complete the remaining work needed to make the server better than the original, without a 1:1 rewrite, with less code, and without 2000-era decisions. Preset: OmO (`openai/gpt-5.6-luna`, variant `max`).

## Current

- Date: 2026-08-29
- HEAD: `5354e6f` (`fix(items): enforce wire-safe stack counts`) — verified with `git rev-parse HEAD`. The working tree is clean.
- Tests: **891 workspace tests listed** by `cargo test --workspace -- --list` at the historical measurement point. This is not a fresh recount and is not a claim that the suite has passed.
- Build: the previous `gm.rs` match-arm build blocker was resolved by `2c8d31a`; it is not a current blocker.
- G0.1a audit: the effective item-stack cap remains **200**. The current item set/update/move/drop wire fields serialize counts as `BYTE` (`u8`), so 2000 is blocked pending a coordinated protocol/client migration. GM counts clamp to 200, channel consumers share the same cap, and entry serialization rejects counts above it instead of truncating them. `ItemRow.count` remains bigint-backed storage; no database schema change is needed for this audit.
- Closed gaps: safebox checkout to Dragon Soul (`dae52e5`), quest target/affect (`dae52e5`), and quest timers (`80b33bf`) are closed. Do not list them as pending.
- Tracker: [Gap Registry](plans/gap-registry.md) is the live per-gap tracker; historical gap analyses remain read-only.
- Gate 2: G0, G1, G2, and G3 remain **pending execution**. The registry's closed rows record completed prerequisite work; they do not close these Gate 2 blocks.
- Deploy: `source\deploy\win` still runs the pre-Phase-1 binary; redeploy remains pending and is no longer blocked by the historical `gm.rs` compile failure (see G1.5 in the tracker).

## Verified capabilities (not a Gate 2 closure)

- [x] Five stat points per level without a 90 cap (ADR-0014)
- [x] Item attributes (roll_attrs plus sockets/magic — `database::attr`, `apply_index` verifier)
- [x] Experience level delta (parity table plus low-mob kills never grant full experience — combat)
- [x] Guild basics, wire, grade/comment/ranking/PG persistence, and war declaration/score handling; lifecycle, finish, and scoreboard remain open in G2.3a–G2.3c
- [x] PvP `can_attack`, PK modes, and penalty (guild/party/safe-zone/protect gates; exp/drop penalty and war-PK exception are covered by the recorded verifiers)
- [x] Skill splash/horse/skill_power, PARTY family, and grand master (`kMasterBonusPoly` in `ecs/systems/skill.rs:329-334`; `ATT_SPEED` applied in `combat.rs:554`); numeric `CASTING_SPEED` remains G2.4
- [x] Berserk/coward/godspeed/stoneskin AIFLAG handling (ECS combat)
- [x] Weight gate is fail-open by design because the classic sources contain no weight column; zero weight never rejects pickup (`weight.rs:64-73`), while an external import remains G2.9
- [x] Locale push and pull (`GC_LOCALE` chunks on connect plus `CG_LOCALE_REQUEST` → `GC_LOCALE`, `287e414`)
- [x] Phase 1 dungeon/event/land/belt/Dragon Soul/refine wire and persistence; deferred content and dungeon instances remain in the live registry
- [x] Full refine probability, scroll destroy/degrade, fee, and window behavior; see `items.rs:1664` and `game_core::refine`
- [x] Party core wire/actions — `set_state`/`use_skill` plus LINK/UNLINK emission (`91b389c`); leader bonus, leadership thresholds, and periodic updates remain G2.1a–G2.1d
- [x] Safebox grid/password plus checkout to belt and Dragon Soul; the remaining storage gaps are tracked in the registry rather than here
- [x] Event scheduling/lifecycle and dungeon WAIT → START → END wire; raid/OX/three-way-war/arena/wedding/monarch content remains G2.8a–G2.8f
- [x] Messenger `OTHER_SEX_ONLY` and its current INFO behavior (`d39c5e8`); marriage, block mode, observer mode, and locale-backed INFO remain G2.2a–G2.2d
- [x] GM parsing and permission checks for mob/kill/purge/goto/polymorph/setskill/transfer/ipurge; transfer/ipurge dispatch and the remaining commands remain G2.6a–G2.6k
- [x] Dragon Soul refine request wire and partial fee/probability/material handling; reward-item creation and 15-cell grid validation remain G2.7a–G2.7b
- [x] Event persistence by database-generated id (`database/event.rs`, `cb5e9a5`)

## Gate 2 — pending execution

All four work groups remain open in the [Gap Registry](plans/gap-registry.md):

- [ ] **G0 — Architecture and storage:** cap decisions and disk-storage work remain to be executed and verified; G0.1a is safely enforced at 200 but blocked at the requested 2000 by the current BYTE item-count wire.
- [ ] **G1 — Gates, documentation, and deployment:** verification, formatting, changelog, deployment, and documentation rows remain to be executed and verified.
- [ ] **G2 — Gameplay and content:** the remaining gameplay, social, quest, GM, data-channel, and deferred-content rows remain open in the registry.
- [ ] **G3 — Hygiene and test debt:** stale comments and ignored-test policy remain to be executed and verified.

The registry's `C1`–`C12` rows are closed prerequisite fixes; they do not mark G0–G3 or Gate 2 as closed.

## Handoff

- 2026-08-29 | **G0.1a item-stack cap audit (coder):** baseline `96606e3`; committed as `5354e6f`. `ITEM_COUNT_LIMIT` remains 200 and is shared by the channel and GM paths. The client/server item count fields are BYTE-sized (`protocol/src/world.rs` item set/update/move/drop2), so the requested 2000 target is not enabled. `game_core::packets::item_set_packets` rejects counts above 200 rather than silently wrapping 2000 to 208; the GM parser/handler clamps to 200. Mutation verifiers cover the cap, shared channel value, explicit 2000 clamp, and entry-wire rejection. `ItemRow.count` stays bigint-backed; events, quests, safebox, belt, trade, economy, movement/spawn, and bench lanes were not modified. G0.1a remains **BLOCKED** until a coordinated u16 protocol/client migration and real-client stack test above 200.
- 2026-08-29 | Gate 2 handoff sync (librarian): superseded by the committed documentation reconciliation at `96606e3` and G0.1a at `5354e6f`; current HEAD and working-tree state are recorded in Current above.

> Entries below this line are historical snapshots. Their counts and states are preserved for their dates; use the Current section and the live Gap Registry for present status.

- 2026-08-28 | **Historical snapshot (superseded)** — the previous handoff recorded HEAD `674296c`, the temporary `gm.rs` build break, and the then-open safebox/quest items. Subsequent commits `2c8d31a`, `dae52e5`, and `80b33bf` resolved those blockers and gaps; see the current handoff above.

- 2026-08-28 | slice docs sync (librarian): progress.md sincronizado con HEAD `d3479bc` verificado (`git rev-parse HEAD`; ojo: el HEAD pedido `43063fb` es 6 commits anterior — 43063fb→9b8c318→4352cb8→55851a6→81d5f85→d3479bc). Checklist: [x] party 75/76, safebox grid full, weight fail-open (provenance re-verificada), locale pull, events/dungeon schedule+lifecycle, skills PARTY/PK (292a267); quedan 10 pendientes reales (messenger, quests, GM, guild war, pvp penalty, skills GM, dragon_soul FASE 2, residuales party/safebox/events). Tests **857** (--list 2026-08-28, verificado). Redeploy del binario Phase 1 al stack `source\deploy\win` sigue pendiente.

- 2026-08-28 | slice dragon_soul jugable FASE 1 (coder): wire CG_DRAGON_SOUL_REFINE (205, 47 B — el SIZE ya lo anclaba el framer) + tabla **`player.dragon_soul`** (ledger ADITIVO del reforge — el legacy no la tiene: el estado del alma vive en vnum+sockets, DragonSoul.cpp:593; patrón append-only de money_log, F3 tail ACID; id **GENERATED ALWAYS AS IDENTITY** → `player.dragon_soul_id_seq`, lección land: nunca un contador de proceso). `database/src/dragon_soul.rs` NUEVO (DragonSoulRepo::record — INSERT sin id + RETURNING) + `channel/dragon_soul.rs` NUEVO (parse bSubType [1]/47 B; handle_refine parity input_main.cpp:3197-3222: solo 1..4 se despachan, OPEN/resto silencio; registra en el ledger + responde GC_DRAGON_SOUL_REFINE 209 (5 B) FAIL_NOT_ENOUGH_MATERIAL con Pos NPOS — parity SendRefineResultPacket DragonSoul.cpp:970-987, la ventana del cliente no se cuelga) + dispatch en game.rs + const GC 209 en protocol. DDL aplicada al PG vivo (idempotente, `scripts/gpg/dragon_soul_table.sql`) + smoke live: INSERT con id=1 de la identity + DELETE 0 residuos. VERIFIERs **mutaciones PROBADAS rojas**: (1) parse con offset 0 en vez de 1 → parse_layout_is_byte_exact RED; (2) RECORD_SQL con `(id, player_id, refine_type)` explícito → record_identity_comes_from_pg RED. VE: workspace 0 failed (~823 tests: protocol 101, server_realms 130+1, database 92+1), clippy sin hits en los archivos nuevos (solo pre-existentes), cargo check 8-10 s. GAPs/notas: (1) el refine REAL (materiales/fee/prob, consumir/crear items — DragonSoul.cpp:488+) es fase 2 — hoy el cliente recibe FAIL determinista; (2) el grid de 15 TItemPos (stride 3 desde [2]) lo valida fase 2 (hoy parse solo lee bSubType); (3) sin test de sesión live para handle_refine (patrón land: verifier de contrato + wire); (4) redeploy del binario pendiente (el stack corre el binario viejo); (5) lane horse tocó en paralelo horse.rs/packets.rs/framer.rs/entry.rs — NO tocado (disjunto). HEAD trabajando en paralelo: lane land/belt cerrado.

- 2026-08-28 | slice belt jugable FASE 1 (coder): wire + persistencia del CINTURÓN (parity belt_inventory_helper.h): `channel/belt.rs` NUEVO (consts wire 242..258 — 180+32 wear+6×2 DS+6×3 reserva — is_belt_cell/wire_cell/place_at/is_available_cell [regla grade EXACTA belt_inventory_helper.h:39-44]/can_move_into_belt [USE 3 + POTION 0/POTION_NODELAY 11/ABILITY_UP 7]/equipped_belt [EQUIPMENT 203]) + CG_ITEM_MOVE → belt en items.rs handle_move (gates: tipo + belt equipado + grade; stack sin grade — parity MoveItem 5645/5709/5730; compat INVENTORY↔belt↔belt; wire_cell en handle_use/consume_one_use — usar poción desde la cinta) + safebox CHECKOUT → belt (safebox.rs, parity @fixme119 input_main.cpp:2096-2100 + rama belt IsEmptyItemGrid 547-567) + entry wire FIX en game_core packets.rs item_set_packet (BELT_INVENTORY PG → window INVENTORY + cell 242+pos; ANTES mandaba window 6 — el cliente lee INVENTORY 242+i, uiinventory.py:247-249) + find_equip_cell rama ITEM_BELT→WEAR_BELT (item.cpp:558-559, equip del cinturón). Persistencia = upsert directo (window 'BELT_INVENTORY' pos 0..15, formato del C++ SaveItem 414-421). VERIFIER live-PG belt_move_wired_and_grade_gated (vnum proto throwaway por pid + self-heal DELETE/ON CONFLICT + cleanup): move inv→belt (wire DEL 20/42 B + SET 21/51 B cell 242 + memoria/persistencia BELT_INVENTORY pos 0), rechazo grade (243 con grade 1 → silencio), belt→inv, checkout→belt; **mutaciones PROBADAS rojas** (to_belt=false en items.rs → rojo en window PG; to_belt=false en safebox.rs → rojo "la caja se vacía"). VE: server_realms 130+4+5 passed / 0 failed, game_core 235+3 passed / 0 failed, clippy sin nuevos (solo pre-existentes); PG sin residuos (0 filas throwaway). GAPs/notas: (1) item_proto de la PG NO tiene belts (type 32 = 0 filas — NO HAY cinturón equipable en los datos: verifier crea proto throwaway); (2) quickslot de celdas belt = GAP (mover desde la barra rápida); (3) gate de equip: WEAR_BELT=23 (<32 — sin colisión con belt 242+).

- 2026-08-28 | slice weight import real (coder): buscada fuente clásica de pesos en TODO el ecosistema — NO EXISTE ninguna con datos (re-verificado de primera mano): (1) PG item_proto 11 002 filas weight=0; (2) dump MariaDB re-parseado = 11 002 filas en 2 bloques, weight=0; (3) item_proto del pack del cliente ×3 idiomas decodificado con la pipeline REAL (MIPX 20B + MCOZ 20B + TEA-ECB-32 key DumpProto {173217,72619434,408587239,27973291} + LZO1X, probe temporal Rust) → bWeight (TItemTable_r156 pack(1) offset 60, struct 156) = 0 en las 11 002; (4) item_proto.txt del C++ 0 B; (5) DumpProto sin columna weight en su CSV (no puede transportarla); (6) upstream old-metin2.com sin columna (doc previa). Por qué fail-open: la variante nunca tuvo peso (C++ sin GetWeight/GetMaxWeight, cliente sin barra ni consumidor de bWeight) y ninguna fuente clásica conserva la columna → el gate (events.rs pickup) queda fail-open por diseño: peso 0 → weight_for_item 0 → can_carry siempre true. Importación futura = UPDATE item_proto.weight desde una fuente externa (p. ej. Metin2 upstream) — la escala correcta queda PINNED: verifier ESPADA 190 → 19 u ÷10 (weight.rs:35-62) + fail-open verificado al límite (weight.rs:64-73). Cambios: weight.rs doc + verifier fail-open (80 líneas), progress.md. VE: game_core 233 passed / 0 failed (3 weight), clippy sin warnings nuevos (solo pre-existentes protocol). Probe temporal eliminado (bin/ borrado).

- 2026-08-27 | slice safebox full (coder, lane paralela del party): grid de `size` celdas (strip vertical paso 5, parity `IsEmpty(pos,1,size)` safebox.cpp:130-136) en checkin/checkout/item_move con item_proto.size real; gates EXPAND (71009) + antiflag SAFEBOX (1<<17) + STACKABLE/ANTIFLAG_STACK del stack; `/safebox_change_password` completo (gm parse + SafeboxRepo::change_password, strcasecmp del C++); verifiers: checkin_gate_rejects_antiflag_safebox (mutation PROBADA: roja al quitar el gate), _2x2_over_strip_cell, _2x2_out_of_grid, checkout_strip_respects_inventory_page_and_occupancy, stack_requires_stackable_dst_without_antiflag, old_password_matches_is_case_insensitive; smoke live PG del batch ANY($1) (9510: size 2 + antiflag) — workspace 784 passed / 0 failed; clippy sin nuevos.

- 2026-08-27 | slice party (coder): CG_PARTY_SET_STATE (75) + CG_PARTY_USE_SKILL (76) cableados en game.rs (antes caían a `other`) — handle_set_state parity input_main.cpp:2184-2239 (gates líder/miembro/rol, SetRole cupo 1, broadcast GC_PARTY_UPDATE) + handle_use_skill parity 2388-2415 (HealParty real con cooltime 60 min + SummonToLeader con anillo SUMMON_RING vía GC_WARP); protocol: TPacketCGPartySetState/UseSkill/GCLink/GCUnlink + consts 91/92; verifier party_role_heal_summon_wired. VE: protocol 100/100, server_realms 117/117. GAPs que quedan: GC_PARTY_LINK/UNLINK sin emitir, +30% item líder (sin equipo). OJO: lane safebox paralela tocó safebox.rs/gm.rs/database — su test checkout_strip... FAILED (safebox.rs:1082, suyo, no mío).
- 2026-08-27 16:55 | HEAD 4245dc6 | M source/reforge/game_core/src/gm.rs; M source/reforge/server_realms/src/channel/gm.rs; M source/reforge/server_realms/src/channel/session.rs; M source/reforge/server_realms/src/config.rs; ?? documentation/adr/0014-infinite-stats-five-per-level.md
- ADR-0014 + stats 5/nivel infinito implementado (gm.rs/session.rs/config.rs) — verifier 2 passed
- 2026-08-27 | slice weight (coder): game_core/src/weight.rs (weight_for_item/can_carry/max_weight (30+level*3+ST*2)*10) + gate pickup events.rs con GC_CHAT INFO + verifier weight_limit_rejects_pickup; game_core 214/214, server_realms check OK. DB item_proto.weight = 0 (11 002 filas): el gate es fail-open hasta importar pesos (pendiente: columna weight). VE: fórmula clásica GetMaxWeight (el C++ de la variante no tiene peso).
- 2026-08-27 | Goal started: corre todo lo que falta
