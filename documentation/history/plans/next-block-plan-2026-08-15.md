# PLAN — Próximo bloque de trabajo (2026-08-15, noche)

> Plan consolidado: bugs nuevos de la sesión real + plan original pendiente.
> Fuentes: bug-registry-2026-08-15.md (inventario vivo), ROADMAP.md, gap-analysis-2026-08-15.md.
> Método: fixers paralelos (scopes disjuntos) + verifier adversarial + oracle director (probado en el Gap loop).

---

## FASE 1 — Bugs de la sesión real (PRIORIDAD MÁXIMA, el usuario los sintió)

### 1.1 [REVIVE] Revivir en la ciudad NO teletransporta — aparece donde moriste

- **Causa (C26)**: el path answer==1 manda GC_WARP al village (969600,278400) pero NO actualiza `row.x/y` antes del `save()`. La reconexión (DirectEnter) hace `player_load` con la posición guardada = la del death.
- **Fix**: en `revive()` answer==1, setear `row.x = 969600, row.y = 278400` + save ANTES del GC_WARP.
- **Archivo**: server_realms/src/channel/script.rs.

### 1.2 [REVIVE] "Revivir aquí" deja los botones / no revive limpio

- **Causa**: el `CloseRestartWindow` (type 5) se manda, pero el path answer==0 hace Remove+Insert del personaje — el usuario reporta que NO cierra la ventana. Verificar que el paquete llega bien (id=0, payload sin "Name : ") y que el orden (CloseRestartWindow PRIMERO) se cumple. También: quizá el cliente manda answer por CG_SCRIPT_ANSWER y NO por el chat — el path del chat manda `/restart_here` que SÍ revivió (log "REVIVIÓ answer 0"). CONFIRMAR qué botón manda qué.
- **Fix**: verificar wire + si el diálogo usa CG_SCRIPT_ANSWER, asegurar el mismo tratamiento.

### 1.3 [EQUIPO] Las botas no agregan velocidad de movimiento

- **Causa (C27)**: el C++ da velocidad por `AddAffect(POINT_MOV_SPEED, item->GetValue(2))` al equipar botas (char_item.cpp:4337 y las variantes). El Rust hardcodea `b_moving_speed = 100` (packets.rs:282) sin procesar los affects de items equipados.
- **Fix**: calcular `b_moving_speed` desde los affects de items equipados (botas: value2), o al menos sumar el valor de las botas al 100 base. Actualizar `compute_battle_points`/packets para que el UpdatePacket y el ADD lleven la velocidad real.
- **Archivo**: game_core/src/packets.rs + server_realms (equipar).

### 1.4 [MOBS] Comportamiento raro: movimientos rápidos + se amontonan

- **Causa A (C23)**: mobs muertos no respawnean por tiempo (solo por distancia). El farming no funciona.
- **Causa B (C28)**: sin separación/collisión entre mobs — todos persiguen al jugador en línea recta y se superponen (el C++ tiene distancia mínima entre mobs).
- **Causa C**: la velocidad percibida "muy rápida" — revisar `move_duration_ms` y el tick (250ms) — el paso por tick puede ser demasiado grande o la duración del GC_MOVE no coincide con el avance real.
- **Fix**: (a) timer de respawn por entrada (el regen.txt ya tiene el intervalo); (b) separación mínima entre mobs (no perseguir si hay otro mob en el radio de ~50-100 u); (c) revisar el envelope de movimiento del mob (duración = dist/speed real).

### 1.5 [MOVIMIENTO] Envelope rechaza MOVEs sin ack (jugador clavado) — C19

- Confirmado en vivo: 74 rechazos. El C++ hace Show+Stop (sincroniza); el Rust solo loguea.
- Fix: al rechazar, enviar la posición corregida al cliente.

---

## FASE 2 — Plan original pendiente (prioridad media)

### 2.1 [CABLEAR] Safebox + Messenger (repos dormidos, C2)

- `database/src/safebox.rs` (4 fns) + `messenger.rs` (3 fns) — CERO callers. Añadir handlers CG_SAFEBOX_*/CG_MESSENGER + framer + dispatch.

### 2.2 [QUESTS] Acciones pendientes (C1)

- say_reward, send_letter, set_quest_state, target_vid, affect_*, input_number.

### 2.3 [SKILLS] GAPs (C3)

- SPLASH/PARTY/HORSE families, skill_power.txt, buffs numéricos aplicados (MOV_SPEED/ATT_SPEED/CRITICAL), quest-granted skills.

### 2.4 [GAME] PvP + difusión de movimiento (C24/C25)

- Atacar a otro jugador (process_attack solo usa npc_view). Movimiento del player a peers (multijugador).

### 2.5 [GAME] Oro del suelo (C22)

- Pickup de vnum 1 → row.gold + GC_POINTS (hoy va al inventario).

### 2.6 [GAME] GM mob spawn (/mob)

- Intent nuevo del mundo.

### 2.7 [GAME] Level-up cap 99 (C21)

- char.cpp:2976 bloquea >99; el Rust usa saturating_add sin tope.

---

## FASE 3 — Infraestructura (F5 tail / F6)

### 3.1 ADRs pendientes

- ADR quest engine, ADR regional channels (ROADMAP:83), ADR server→client data (85).

### 3.2 Data layer completo (ROADMAP F3)

- Repos por dominio (account/world/social/economy/log) con schemas + permisos.
- Port por QID: items/social pendientes.
- SQL routing (SQL_ACCOUNT/SQL_PLAYER).
- PROTO_FROM_DB mantenido; RLS post-WAL; Patroni failover.

### 3.3 F5 tail

- Data channel 162/163 activo, locale wire (ADR-0009), channel list desde auth, config via manifest, hot reload, benchmark ladder 250/500/1000, side-by-side automatizado.

### 3.4 F6

- Eliminar mysql_proxy + protocol::legacy; srv1 100% Rust.

---

## ORDEN SUGERIDO DE EJECUCIÓN (bloque nocturno)

1. **Fase 1 completa** (los bugs que el usuario siente) — 5 fixers paralelos con verifier.
2. **2.1 Safebox + Messenger** + **2.5 Oro** + **2.6 GM mob** (rápidos, alto valor).
3. **2.2 Quest actions** + **2.3 Skills GAPs** (completan sistemas existentes).
4. Fase 3 según avance (ADRs primero, luego data layer).
