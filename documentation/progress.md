---
Type: Snapshot
Status: Current
Audience: Contributors
Last verified: 2026-09-02
---

# Progress — reforge-core server

## Goal

Complete the remaining work needed to make the server simpler, more
testable, and easier to verify, without claiming parity with any other
implementation. Preset: OmO (`openai/gpt-5.6-luna`, variant `max`).

## Current

- Baseline snapshot (2026-09-02, before the Task 1 documentation commit): captured HEAD is `42783e9c335ce56562258a18744418f60c24dcc3`. The captured worktree had four modified tracked Rust files (`spawn.rs`, `map.rs`, `channel/entry.rs`, and `channel/session.rs`) and untracked `.superpowers/` and `docs/` directories; Task 1 leaves them untouched.
- Task 2 disposition (captured at HEAD `1c59cb1650499da09cb005b091fece6d0550fbec`): `map.rs`, `channel/entry.rs`, and `channel/session.rs` are `continue in named slice` for the provisional `fix(world): normalize client-unsafe persisted positions` continuation; `ecs/systems/spawn.rs` is `continue in named slice` for the separate provisional `perf(world): limit initial spawn materialization` continuation. None is staged or committed by A0 Task 2; each still needs focused tests, applicable mutation/negative coverage, current-document confirmation, and review before its named commit.
- The captured local agent files are `ignore as local tool state`: they remain on disk and are excluded by the root `/.superpowers/` and `/docs/superpowers/` rules. No public `docs/` tree is created; the exact path-level table is in the [A0.2 worktree disposition](plans/gap-registry.md).
- Standard gate: `scripts/verify.ps1` **FAILED** in normal `cargo test --workspace` (exit 101). Both `database/tests/wal_pg.rs` integration tests failed at `wal_pg.rs:27` because PostgreSQL at `127.0.0.1:5432` refused the connection. The optional ignored PG/WSL leg was not reached and is not counted as passed; `scripts/verify.ps1:24-55` therefore produced no final `OK: verificacion completa`.
- Deploy snapshot: `source/deploy/win/server_realms.exe` exists with SHA-256 `4277E5A8C74E69B2D9643967B260C7E345641EBDB991848421435EC83375056B`; listeners 5432, 30001, and 30003 were all closed in the capture. The executable hash was observed independently; no deployed-HEAD equivalence is claimed.
- Task 4R2 documentation-gate hardening (2026-09-02): the dependency-free
  fragment check now allocates every heading anchor against the full used set
  and escapes decoded control characters in diagnostics. The duplicate-anchor
  mutation passes; the `%0A::warning` mutation fails with `\u000A` and no raw
  `::warning` line. The post-fix missing-fragment marker is
  `task-4r-final-mutation-fragment-does-not-exist`; the earlier
  `task-4r-mutation-fragment-does-not-exist` remains only as Task 4R's expected
  pre-fix false-success evidence. The four Rust worktree changes remain outside
  this documentation slice.
- Next action: make PostgreSQL available at `127.0.0.1:5432` and rerun `scripts/verify.ps1`.

## Verified capabilities (not a Gate 2 closure)

- [x] Five stat points per level without a 90 cap (ADR-0014)
- [x] Item attributes (roll_attrs plus sockets/magic — `database::attr`, `apply_index` verifier)
- [x] Experience level delta (parity table plus low-mob kills never grant full experience — combat)
- [x] Guild basics, wire, grade/comment/ranking/PG persistence, and war declaration/score handling; lifecycle, finish, and scoreboard remain open in G2.3a–G2.3c
- [x] PvP `can_attack`, PK modes, and penalty (guild/party/safe-zone/protect gates; exp/drop penalty and war-PK exception are covered by the recorded verifiers)
- [x] Skill splash/horse/skill_power, PARTY family, and grand master (`kMasterBonusPoly` in `ecs/systems/skill.rs:329-334`; `ATT_SPEED` applied in `combat.rs:554`); numeric `CASTING_SPEED` remains G2.4
- [x] Berserk/coward/godspeed/stoneskin AIFLAG handling (ECS combat)
- [x] Weight gate is fail-open by design because the classic sources contain no weight column; zero weight never rejects pickup (`weight.rs:64-73`), while an external import remains G2.9
- [x] Auth/loading locale bootstrap (`GC_LOCALE` chunks from auth plus `CG_LOCALE_REQUEST` → `GC_LOCALE`, `287e414`); the external compatible client rejects header 140 in `Game`, so the channel must neither push nor respond with it. In-game hot reload remains deferred until a compatible external client Game-phase parser/cache and version-gated rollout exist outside this public server repository.
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

- [ ] **G0 — Architecture and storage:** G0.1b–G0.1e are implementation-complete for their focused checks and ready for Oracle Gate, but remain open; G0.1a is safely enforced at 200 but blocked at the requested 2000 by the current BYTE item-count wire. [G0.2 disk storage is closed](plans/gap-registry.md) with the registry's dated 2026-08-30 evidence.
- [ ] **G1 — Gates, documentation, and deployment:** normal/ignored verification, formatting, documentation CI, and redeployment remain open; G1.14b's immutable-history decision is closed, and changelog freshness/current archive navigation are reconciled.
- [ ] **G2 — Gameplay and content:** the remaining gameplay, social, quest, GM, data-channel, and deferred-content rows remain open in the registry.
- [ ] **G3 — Hygiene and test debt:** stale comments and ignored-test policy remain to be executed and verified.

The registry's `C1`–`C12` rows are closed prerequisite fixes; they do not mark G0–G3 or Gate 2 as closed.

## Handoff

- 2026-09-02 | **A0 Task 4R2 documentation-gate hardening:**
  `scripts/check_docs.ps1` now allocates every heading anchor against the full
  used set and renders decoded fragment controls as escapes. The duplicate
  `echo`, `echo`, `echo 1` mutation passes; the `%0A::warning` mutation fails
  with `\u000A`, without a line beginning `::warning`; both sources restore
  byte-for-byte. The post-fix missing-fragment mutation uses
  `task-4r-final-mutation-fragment-does-not-exist`; the earlier
  `task-4r-mutation-fragment-does-not-exist` was the expected pre-fix
  false-success marker. The unmutated checker passes; the four Rust worktree
  changes remain unstaged and untouched.

- 2026-09-02 | **A0 Task 4R documentation-contract repair:** active Markdown
  fragments now use their actual GitHub-compatible heading slugs (or no anchor
  where a line number was cited). The pre-fix
  `task-4r-mutation-fragment-does-not-exist` mutation was the expected false
  success; the post-fix rejection is recorded with the unique
  `task-4r-final-mutation-fragment-does-not-exist` marker and its source path and
  link text. The mutation source was restored byte-for-byte. The checker
  intentionally excludes external URLs, `mailto:`, generated artifacts, and
  `documentation/history/`. Next action: orchestrator validation; the four Rust
  worktree changes remain unstaged and untouched.

- 2026-09-02 | **A0 Task 2 artifact isolation:** commit `7affb25`
  (`chore(repo): isolate local agent artifacts`) records the disposition; local
  `.superpowers/` and `docs/superpowers/` files remain on disk and are ignored,
  not deleted. The two Rust slices remain uncommitted:
  `fix(world): normalize client-unsafe persisted positions` (map/entry/session)
  and `perf(world): limit initial spawn materialization` (spawn). **Next A0
  task:** Task 3, publish the [tooling and deploy-boundary inventory](plans/alpha-a0-truthful-baseline.md#task-3-publish-the-tooling-and-deploy-boundary-inventory).

- 2026-09-02 | **Collaborative alpha foundation (user-approved plan):** alpha is a contributor preview, not a claim of total gameplay parity. Public scope is the complete authored Rust server, clean tools, documentation, scripts, and lawful synthetic/versioned development data; client source/packs/binaries, decompiled/proprietary material, and the frozen C++ oracle remain external per ADR-0015. Apache-2.0 is the chosen public license. The canonical [A0–A5 plan](plans/alpha-collaborative-readiness.md) begins with a truthful baseline and ends only after clean-clone reproducibility, contribution governance, and enforceable CI gates are evidenced. `ARQ-E` tracks the work; implementation has not yet begun.

- 2026-09-02 | **OpenCode 2 subagent correction (orchestrator):** the current OpenCode service/API was healthy, but a completed child could remain `unconfirmed` in task tracking; a continuation was therefore rejected and no duplicate was created. `AGENTS.md` and `documentation/rules.md` now require lifecycle-based handling: a bounded child is never resumed, re-prompted, or duplicated under live/unconfirmed state; background cancellation uses `POST /api/session/<id>/interrupt` followed by API verification. The cancelled crash-diagnostics research lane is not evidence and must be re-scoped only after its workflow is reliable.

- 2026-09-01 | **Auth-only locale bootstrap fix (orchestrator):** the external compatible client rejects `GC_LOCALE` (header 140) in `Game` (`Unprocessed packet header 140, state Game`), so the channel locale module, Game request handler, and post-entry push were removed; auth remains the sole bootstrap responder. The deterministic loopback verifier rejects header 140 after `GC_PHASE(GAME)` and fails under the `140 → break` mutation; focused channel-entry (1/1), auth-locale (2/2), fmt, clippy, and diff checks passed. Release `4277E5A8…75056B` was deployed to `source/deploy/win` with matching hash and ports 30001/30003 listening. Real client evidence: entered map 41 with 89 spawns, emitted sustained `MOVE`, NPC, item, combat, death/revive, and drop traffic, without a reset or new `syserr.txt` entry. In-game hot reload remains deferred until a compatible client Game-phase parser/cache and version-gated rollout exist outside this public server repository. `scripts/verify.ps1` completed after the OpenCode2 restart with `OK: verificacion completa` (the informational ignored PG/WSL leg was unavailable).

- 2026-08-30 | **Plan general, higiene F0 y gobernanza documental F1 (orchestrator):** el usuario aprobó el plan de diagnóstico general (fases 0–4, en `~/.commandcode/plans/`) con foco en gameplay + simplificación documental y criterio de gate MIXTO POR RIESGO. Ejecutado en este slice: (0.1) commits atómicos del árbol sucio — `270b8a7` (client/pack+LICENSE), `74f62a7`/`4aaff76`/`d924aaa`/`96e969f` (lanes G0.1b–e), `2cf099f` (docs). (0.2) G0.1e CERRADA (mixto por riesgo) y G0.1b/c/d estrechadas al check de cliente real. (0.3) G1.5 lado servidor: binario desplegado 09:19:55 == build release de HEAD (SHA `65C1EBFC…`), puertos LISTEN, smoke wire 1/1 ok (el harness falsamente reportaba desync por el push GC_LOCALE — splitter corregido `1b595e0`). (0.4) G1.1a: `verify.ps1` corre la suite normal antes que la ignorada (`9056cc3`); G1.2: pase fmt+clippy 1.97 completo (`6d8fdeb`); G1.1b/G3.2: flakes des-flakeados y re-habilitados (`fa41755`, `0b4d757`, `70246a9`). Primeras corridas del gate destaparon deuda REAL: C13 skill_power filas anchas (`5946364`, paridad config.cpp:532-613), C14 mob_proto smallint (`81c934b`), flake remove_dir_all (Windows 145). (0.5) `restore_drill.ps1` ejecutado y PASADO contra `metin2_2026-08-29.dump` + runbook `documentation/reference/backup-restore.md`; G0.2: target/ 7,36 GB, presupuesto <=5 GB documentado (clean pendiente). (F1) `document-authority.md` (precedencia única), roadmap.md → mapa de fases, README rutea al estado vivo, banner histórico en ROADMAP.md, disposition en trackers .omo/.slim, ADR-0016 (quest engine) y ADR-0017 (regional channels deferida), enmiendas ADR-0006/0010/0011, gate de metadata en CI (`5937186`), tabla de cobertura quest corregida (G3.1c). Pendiente del operador: sesión de cliente real (login test/1234 + andar + montar + tienda NPC) para cerrar G0.1b/c/d y G1.5; copia off-host de dumps (no hay segundo volumen).
- 2026-08-29 | **G0.1a item-stack cap audit (coder):** baseline `96606e3`; committed as `5354e6f`. `ITEM_COUNT_LIMIT` remains 200 and is shared by the channel and GM paths. The client/server item count fields are BYTE-sized (`protocol/src/world.rs` item set/update/move/drop2), so the requested 2000 target is not enabled. `game_core::packets::item_set_packets` rejects counts above 200 rather than silently wrapping 2000 to 208; the GM parser/handler clamps to 200. Mutation verifiers cover the cap, shared channel value, explicit 2000 clamp, and entry-wire rejection. `ItemRow.count` stays bigint-backed; events, quests, safebox, belt, trade, economy, movement/spawn, and bench lanes were not modified. G0.1a remains **BLOCKED** until a coordinated u16 protocol/client migration and real-client stack test above 200.
- 2026-08-29 | **G0.1b–G0.1e cap lanes (coder):** against baseline `5a0ac99`, the four code lanes remain uncommitted. **G0.1b:** only `game_core/src/movement.rs`; inclusive 6000 mounted/unmounted limit preserved, `i128` distance arithmetic added, 12 focused tests passed, and no envelope/default-speed/anti-cheat-tolerance change. **G0.1c:** only `game_core/src/ecs/systems/spawn.rs`; production 300000/310000 constants and predicates unchanged, boundary/hysteresis mutation coverage passed, and 13 focused tests passed. **G0.1d:** only `game_core/src/packets.rs`; cap 200 remains before BYTE ADD/UPDATE serialization, wide/saturating accumulation is covered, 22 focused tests passed, mutation baseline failed as expected, and no real-client check was run. **G0.1e:** economy/shop/channel gold consumers retain `GOLD_MAX = 2_000_000_000`, checked delta/sub bounds, and property coverage; the detached focused run finished **147 passed, 0 failed, 4 ignored** with warnings only (existing/unused). `git diff --check` was clean for the lanes. All four rows are **IN PROGRESS — ready for Oracle Gate**, not `CLOSED`.
- 2026-08-29 | Prior Gate 2 handoff sync (librarian): superseded by the G0.1b–e handoff above; the earlier documentation reconciliation was committed at `96606e3` and G0.1a at `5354e6f`.
- 2026-08-30 | **Public repository boundary and documentation reconciliation (librarian):** ADR-0015 accepted; the authored Rust server remains the public implementation scope, client and pack sources remain outside the repository, and real-client verification uses an external compatible client. ADR-0013 is superseded and F7 is deferred outside this repository. The current documentation policy and live hubs were restored/reconciled; the staged removals and documentation changes are intentionally uncommitted; no Rust source was changed by this documentation slice.
- 2026-08-30 | **Archive navigation and metadata follow-up (librarian):** added the current `documentation/history-index.md` successor without editing `documentation/history/**`; reconciled the hub and registry freshness rows; added metadata to the historical `ASSUMPTIONS.md` snapshot and refreshed `source/reforge/README.md` as a current workspace hub. G1.14b was then resolved by the explicit decision to preserve `history/README.md` byte-for-byte, including its pre-migration metadata; G1.18 remains open for CI implementation. No Rust source was changed.
- 2026-08-30 | **G1.14b history-policy decision (librarian):** the user chose immutable preservation of `documentation/history/README.md`, including its original metadata. The current [archive index](history-index.md) records this narrow exception; the historical file remains untouched, and all new or edited documents continue to follow [DOCUMENTATION.md](DOCUMENTATION.md).

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
