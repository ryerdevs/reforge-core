# Registro de errores encontrados — Metin2 rewrite (2026-08-15)

> Bitácora viva de bugs confirmados y candidatos, para el siguiente bloque de trabajo.
> Estado: [ABIERTO] / [FIXED] / [VERIFICADO] / [PARCIAL] / [REVISANDO]
> Cada entrada: sintoma, causa raiz (con parity C++ citada cuando aplica), fix, estado.

---

## A. Bugs del usuario (sesion real 2026-08-15)

### A1. [FIXED-VERIFICADO] Revivir aqui no cierra la ventana de muerte

- **Sintoma**: los botones de revivir se quedan en pantalla al revivir "aqui".
- **Causa**: el C++ manda `ChatPacket(CHAT_TYPE_COMMAND, "CloseRestartWindow")` antes del revive (cmd_general.cpp:406,460); el Rust no lo enviaba. El pack lo usa para cerrar la ventana (game.py:1874).
- **Fix**: `revive()` envia el GC_CHAT tipo COMMAND "CloseRestartWindow" primero (ambos paths). CHAT_TYPE_COMMAND=5 (no 9), id=0.
- **Commit**: `c2c5ae8`. Verifier: PASS (parity byte-exacta).
- **Para probar**: morir -> revivir aqui -> la ventana debe cerrarse y el personaje reaparecer en el sitio.

### A2. [FIXED-VERIFICADO] Revivir en la ciudad warpea al sitio equivocado

- **Sintoma**: "revivir en la ciudad" teletransporta a 960640,263099 (punto de entrada) en vez del village.
- **Causa**: el Rust usaba `exit_x/exit_y`; el C++ usa EMPIRE_START_X/Y o GetRecallPositionByEmpire (cmd_general.cpp:475,489,554).
- **Fix**: warp siempre al village fijo del mapa 41 (969600,278400 UNITS).
- **Commit**: `c2c5ae8`. Verifier: PASS.

### A3. [FIXED-PEND-VERIFY] Subir de nivel no da puntos de stats

- **Sintoma**: al subir nivel, el contador de puntos de stats no aumenta.
- **Causa**: el level-up subia level/exp pero no sumaba stat_point.
- **Fix**: `level_up_step()` — +3 POINT_STAT por nivel (nivel pre-incremento <= 90). DEVIACION documentada: la spec decia +1 pero el ciclo completo del C++ da 3/nivel (char.cpp:3075-3136, ResetPoint char.cpp:5857).
- **Commit**: `875b4ae`. Verifier: en curso.
- **RIESGO ABIERTO**: si el orquestador prefiere +1 (spec), es un cambio de 1 linea.

### A4. [FIXED-PEND-VERIFY] Stats del equipo no aumentan los del personaje

- **Sintoma**: ponerse equipo no cambia el ataque/defensa visibles.
- **Causa raiz (doble)**:
  1. `weapon_min/max` leia values[0]/[1]; el dano real del arma es value3/value4 (battle.cpp:460-461, cliente __SetWeaponPower).
  2. El Rust enviaba GC_CHARACTER_ADDITIONAL_INFO (136) al equipar — paquete de ENTRADA que el cliente DESCARTA en runtime. El C++ envia UpdatePacket (GC_CHARACTER_UPDATE, 19) en EquipTo/Unequip (item.cpp:1004-1005) — el cliente calcula ATT_MIN/MAX localmente desde ese paquete (__RecvCharacterUpdatePacket).
- **Fix**: weapon de values[3]/[4] + nuevo TPacketGCCharacterUpdate (19, 51 B) enviado al equipar/desequipar.
- **Commit**: `875b4ae`. Verifier: en curso.

### A5. [FIXED] Item proto weight: int2 leido como i64 -> vuelta al login

- **Sintoma**: al entrar al mundo, el player se devuelve al login.
- **Causa**: el fix del peso (lane D) leia `weight` (smallint) con try_get::<i64> -> "error deserializing column 9".
- **Fix**: `let weight: i16 = r.try_get(9)` + i64::from (patron shop.rs:284-289).
- **Commit**: `6665941`. Desplegado.

---

## B. Hallazgos del Gap loop (verifiers adversariales, 2026-08-15)

### B1. [FIXED] Lane B: `/skillup 0` creaba skill fantasma + gastaba punto

- C++: CanUseSkill(char_skill.cpp:3572) `if (0 == dwSkillVnum) return false` (no-op). Rust mutaba el blob.
- Fix: `skillup_apply` devuelve None para vnum 0. Commit `c24fe70`.

### B2. [FIXED] Lane B: bMasterType nunca escrito (grado MASTER/GRAND/PERFECT)

- C++: SetSkillLevel (char_skill.cpp:207-217) setea bMasterType en cada subida (20/30/40 -> 1/2/3).
- Fix: `master_type_for_level` escribe blob[off]. Commit `c24fe70`.
- Layout del bytea confirmado: bMasterType@+0, bLevel@+1, tNextRead u32@+2 (tables.h:351-356) — el "vnum u32 en off" era incorrecto.

### B3. [FIXED] Lane A: entry.rs:457 no compilaba (points_packet firma 3 args)

- El commit e6ca209 cambio la firma pero dejo 1 call site con 2 args. Ya resuelto en 3bcaf26 (lane E). Sin commit nuevo (correcto).

### B4. [FIXED] Lane C: NUL de cola rompia los comandos '/' (CRITICO — "No such command" para todo)

- El cliente manda el texto con NUL; parse_command con split_whitespace no matcheaba "comando\0".
- Fix: `chat_text()` recorta en el primer NUL ANTES del hook. Commit `70379f1`.

### B5. [FIXED] Lane C: sin caps de longitud -> DoS del cliente legacy

- GC_CHAT/WHISPER sin cap -> el cliente legacy hacia Recv sobre buf[1025]/[513] sin bound-check (stack overflow).
- Fix: caps 485 (chat) / 512 (whisper), CHATBUF_MAX 512. Commit `70379f1`.

### B6. [FIXED] Lane C: rango TALKING 1000 no parity (C++: todo el mapa, ~5500 efectivo)

- Fix: cull server-side por VIEW_RANGE 5000+500=5500. Commit `70379f1`.

### B7. [FIXED] Lane C: SHOUT con alcance/formato equivocados

- C++: canal completo, id=0, payload "Name : msg", cooldown 15s. Fix: rama propia + last_shout. Commit `70379f1`.

### B8. [FIXED] Lane C: GC_CHAT sin prefijo "Name : "

- El cliente pinta el payload verbatim (AppendChat); sin nombre en la ventana.
- Fix: `chat_payload()` arma "Name : msg". Commit `70379f1`.

### B9. [FIXED] Lane C: whisper case-sensitive vs FindPC case-insensitive

- Fix: eq_ignore_ascii_case. Commit `70379f1`.

### B10. [FIXED] Lane D: GC_CHARACTER_POSITION con header 28 (debe ser 43)

- 28 es GC_QUICKSLOT_ADD (4 B estaticos) -> desync del stream del cliente.
- Fix: header 43 (packet.h:159). Commit `18dc92f`.

### B11. [FIXED] Lane D: posturas corridas (SITTING_CHAIR=3/4, no 1/2)

- Fix: length.h:288-296. Commit `18dc92f`.

### B12. [FIXED] Lane D: quickslot types corridos (ITEM=1/SKILL=2/COMMAND=3)

- Fix: length.h:238-245; clear_item_refs con type 1. Commit `18dc92f`.

### B13. [FIXED] Lane E: CG_EMPIRE(90)/CG_CHANGE_NAME(106) sin framer -> rename desconecta

- Fix: 90->2, 106->27 en packet_size. Commit `073ee93`.

### B14. [FIXED] Lane E: rollback del create inefectivo (fila huerfana + nombre quemado)

- Fix: `delete_row` sin gate del indice (parity ClientManagerPlayer.cpp:901-907). Commit `073ee93`.

### B15. [FIXED] Lane E: player.rs delete() usaba dw_pid pero el esquema es dwPID (sqlstate 42703)

- Detectado por el oracle director (realm_pg 5/6). Fix: DELETE quest/affect con dwPID. Commit `4a6130c`.

### B16. [FIXED] Oracle: realm_pg nombre de 25 chars > varchar(24)

- Fix: 'e2e_rok_' (18 chars). Commit `4a6130c`.

### B17. [FIXED] Lane F: vendedores del pueblo 20002/20006/20023 sin shop (regresion)

- El fix legacy dejo sin shop a los vendedores visibles. Fix: 20002->1002, 20006->1003, 20023->1004. Commit `83f2481`.

---

## C. Errores/candidatos ABIERTOS (proximo bloque)

### C1. [ABIERTO] Quest actions pendientes

- `say_reward`, `send_letter`, `set_quest_state`, `target_vid`, `affect_*`, `input_number` — el engine los tiene como TODO. Fuente: CURRENT.md.

### C2. [ABIERTO] Safebox + Messenger repos dormidos

- `database/src/safebox.rs` (4 fns) + `messenger.rs` (3 fns) con CERO callers — sin handlers/framer/dispatch. Fuente: gap-analysis §6.

### C3. [ABIERTO] Skills GAPs

- SPLASH/PARTY/HORSE families, `skill_power.txt`, buffs numericos (MOV_SPEED/ATT_SPEED/CRITICAL no aplicados de verdad), quest-granted skills. Fuente: CURRENT.md.

### C4. [ABIERTO] GM mob spawn (`/mob`)

- Necesita intent nuevo del mundo (crear entidad + Spawned). Fuente: gap-analysis §1.

### C5. [ABIERTO] Party/Guild gameplay

- Headers en framer pero handlers "not implemented" (lane B los reconoce pero no los implementa). Fuente: gap-analysis §1/§2.

### C6. [ABIERTO] Data channel 162/163 inactivo

- Wire definido (protocol::datachannel) pero sin activacion; PROTO_FROM_DB pendiente. Fuente: ROADMAP:156.

### C7. [ABIERTO] Locale wire (ADR-0009)

- Importer listo, wire GC_LOCALE no (server no envia texto por idioma todavia). Fuente: CURRENT.md.

### C8. [ABIERTO] Benchmark ladder incompleto

- Solo paso 1 (100 bots). Faltan 250/500/1000 + per-tick CPU. Fuente: CURRENT.md.

### C9. [ABIERTO] Auction / trade unificado

- Trade entre jugadores basico existe; auction no. Fuente: CURRENT.md.

### C10. [ABIERTO] Client UTF-8 name overrides: API hecha, wire pendiente

- Fuente: ROADMAP F4 tail.

### C11. [ABIERTO] Subir stats: sin paquete del cliente identificable

- El level-up ahora da stat_point (A3) pero no hay handler para asignarlos (ST/DX/IQ/HT). Fuente: gap-analysis §6.

### C12. [ABIERTO] Envelope follow-ups

- N-violations auto-ban, path-line sampling, buffs/mounts recompute speed, warp re-anchor. Fuente: CURRENT.md.

### C13. [ABIERTO] Manifest/hot-reload

- NOTIFY -> reload -> manifest bump -> delta. Fuente: ROADMAP:184.

### C14. [ABIERTO] Regional channels (ADR sin escribir)

- El plan §4.4 exige procesos por region con anti-double-login; el ADR no existe. Fuente: ROADMAP:83.

### C15. [ABIERTO] Converter quests (~88 restantes)

- Corpus 194/194 convertido, pero ~88 con acciones sin mapear (say_pc_name, give_exp2, change_money...). Fuente: CURRENT.md.

### C16. [CANDIDATO] pesos del inventario no se muestran / max_weight formula

- El peso se implemento con (30+level+2*ST)*10 — el oracle noto que el C++ de esta variante NO tiene GetMaxWeight (parity clasica, no de este repo). REVISAR si el cliente lo muestra o si la formula debe ser otra.

### C17. [CANDIDATO] shim pi.exe en pnpm/bin (infra)

- Necesario para que pi-agents funcione (spawn pi ENOENT en Windows). Documentar en caso de reinstalar pnpm.

### C18. [CANDIDATO] modo RPC de pi-agents roto en Windows

- pi --mode rpc se cuelga sin salida. Limita workflow_create. Usar pi-subagents (Agent tool) en su lugar.

### C19. [ABIERTO — NUEVO, alto impacto] Envelope rechaza movimientos SIN ack al cliente

- **Sintoma (log 120851, 56 veces)**: el jugador manda MOVE a 962247,275617 y el server responde "fuera del envelope (speed 500) — rechazado" 6 veces seguidas; el jugador queda CLAVADO (el cliente no recibe la correccion).
- **Causa**: el rechazo del envelope (movement.rs:199) solo hace log + Continue. El C++ en el caso de distancia hace Show()+Stop() (sincroniza la posicion del cliente, input_main.cpp:1463-1482); el Rust no envia ninguna correccion -> el cliente mantiene su posicion local y sigue reenviando el mismo MOVE.
- **Fix propuesto**: al rechazar por envelope/distancia, enviar una sincronizacion de posicion al cliente (TPacketGCMove de vuelta a la posicion actual, o el equivalente del Show+Stop del C++). REVISAR tambien la tolerancia (1.5x/250ms) que parece muy estricta para el patron real de MOVEs en rafagas.

### C20. [CANDIDATO — NUEVO] Quickslot clippy error (WIP de otro proceso)

- channel/quickslot.rs:139 clippy::absurd_extreme_comparisons — pre-existente, sin commitear de otro proceso. Confirmado por 3 verifiers que no es de los lanes.

---

## D. Diferido / no aplica (documentado, no bug)

- Peso: formula clasica no presente en este C++ — gate fail-open aditivo (B16 del verifier D).
- `f16_peer_smoke` inexistente en el arbol actual.
- Tests PG/WSL gated por diseno (map41_spawns, channel_pg).
