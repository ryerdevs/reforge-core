# Quest DSL — Especificación v0.1 (borrador para discusión)

> **Estado: DRAFT v0.1** — documento de discusión. No es el spec final.
> **Fecha:** 2026-08-09 · **Proyecto:** Reescritura del servidor de Metin2 en Rust
> **Contexto:** sustituye al runtime Lua (mlua 5.1 + shim) previamente considerado. Decisión: **sin lenguaje de scripting en el servidor Rust; quests como datos declarativos en un DSL propio.**

---

## 1. Contexto y objetivos

El servidor legacy ejecuta las quests en Lua 5.0 (lexer parcheado para EUC-KR) compiladas desde un DSL propio de Metin2 (`qc`). El contenido real son 194 archivos `.quest` (aprox. 2.500+ líneas duplicadas solo en la familia `collect_quest_lv30..lv96`).

**Decisión:** eliminar Lua por completo. Diseñar un DSL declarativo propio, tipado y verificado por un parser Rust, con composición (familias + bloques + imports) para eliminar la repetición.

**Objetivos del formato:**
1. Legible y elegante para diseñadores de quests (no programadores).
2. Corto: una quest larga se compone, no se repite.
3. Cero lógica arbitraria: solo composición de acciones tipadas conocidas.
4. Validación en load-time con errores con archivo:línea:columna.
5. Migración automática y verificable desde el contenido legacy.
6. El mismo parser sirve a runtime, validador CLI y editor (una sola fuente de verdad).

**Lo que NO es:** un lenguaje de scripting. No hay variables de flujo arbitrarias, no hay bucles libres, no hay funciones generales. Los casos que lo requieran (gestores de eventos como `oxevent`) se escriben en Rust como módulos del servidor.

---

## 2. Sintaxis básica

- Extensión sugerida: `.quest` (misma que el legacy — facilita el diff en la migración).
- **Indentación significativa** (2 espacios). Sin `begin/end`, sin `(`, sin comas.
- `#` para comentarios.
- Una quest = bloques `quest`, dentro estados `state`, dentro eventos `on`, dentro acciones `->`.

```quest
# quests/biology/collect_quest_lv30.quest
quest collect_quest_lv30
  state start
    on login, levelup with pc.level >= 30
      -> set_state(information)

  state information
    on letter
      -> send_letter(gameforge.collect_quest_lv30._10_sendLetter)

    on button, info
      -> say_title(gameforge.collect_quest_lv30._10_sendLetter)
      -> say(gameforge.collect_quest_lv30._20_say)

    on 20084.chat
      -> say_title(gameforge.collect_herb_lv10._50_sayTitle)
      -> say(gameforge.collect_quest_lv30._40_say)
      -> wait()
      -> set_qf(duration, 0)
      -> set_qf(collect_count, 0)
      -> set_state(go_to_disciple)

    on 601.kill with number(1, 100) <= 5
      -> give_item2(30006, 1)
```

### 2.1 Reglas de la gramática

| Construcción | Sintaxis | Notas |
|---|---|---|
| Quest | `quest <nombre>` | Único por archivo (opcional: `import` arriba) |
| Estado | `state <nombre>` | `start` obligatorio; `__complete`, `__giveup__` convención |
| Evento | `on <trigger>[ , <trigger>...]` | Multi-trigger con coma; `or` implícito |
| Condición | `with <expresión>` | Opcional; expresión tipada (ver §5) |
| Acción | `-> <acción>(<args>)` | Una por línea; `()` opcional si no hay args |
| Bloque | `block <nombre>[ (<param>: <tipo>)]` | Reutilizable (§7) |
| Uso de bloque | `use <nombre>[ (<args>)]` | Dentro de un evento |
| Import | `import <archivo>` | Sin extensión, relativo a `quests/` |
| Familia | `quest <nombre> = <base>(<param>: <valor>)` | Instancia parametrizada (§6) |
| Comentario | `# ...` | Línea completa |

**Regla de validación:** toda acción, trigger y condición es **conocida por el parser** (catálogo tipado). Un nombre desconocido = error de carga con archivo:línea. No hay escape hacia código libre.

---

## 3. Triggers (eventos) — inventario del corpus real

Extraído de los 194 `.quest` desplegados (germany). Este es el catálogo; el conversor lo completa y audita.

| Trigger | Sintaxis | Semántica (legacy) |
|---|---|---|
| Login | `login` | Entra al mundo |
| LevelUp | `levelup` | Sube de nivel |
| Carta | `letter` | Abre el diario de quests |
| Botón/Info | `button`, `info` | Pulsa el botón o info de la quest |
| Chat NPC | `<vnum>.chat` | Habla con el NPC |
| Matar | `<vnum>.kill` | Mata al mob |
| Usar item | `<vnum>.use` | Usa el item |
| Click target | `__TARGET__.target.click` | Hace click en el objetivo marcado |
| Entrar | `enter` | Entra en un mapa |
| Salir | `logout` | Cierra sesión |
| Temporizador | `timer` (a definir con `pc.setqf` cooldown, ver §4) | Legacy: patrón `get_time()` |
| Select | `select` (acción) | Menú de opciones (acción, no trigger) |

**Triggers de eventos especiales (diferidos o Rust):** `arena.*`, `oxevent.*`, `d.*` (dungeon), `wedding.*` — el corpus de events se audita en la fase de conversión; los que no quepan en el DSL → módulos Rust.

---

## 4. Condiciones (expresiones tipadas)

Mini-lenguaje de expresiones, parseado y tipado por el parser. Soporta: comparación (`==`, `!=`, `<`, `>`, `<=`, `>=`), aritmética básica (`+`, `-`, `*`, `/`), `and`, `or`, `not`, paréntesis, literales numéricos/strings.

| Función | Sintaxis | Legacy |
|---|---|---|
| Nivel | `pc.level >= 30` | `pc.level` |
| Contar item | `count_item(30006) > 0` | `pc.count_item(vnum)` |
| Flag de quest | `get_qf(duration) != 0` | `pc.getqf("duration")` |
| Probabilidad | `number(1, 100) <= 5` | `number(min, max)` (aleatorio entero) |
| Tiempo | `get_time() >= get_qf(duration)` | `get_time()` |
| Mapa | `get_map_index() == 113` | `pc.get_map_index()` |
| GM | `get_gm_level() == 5` | `pc.get_gm_level()` |
| Mascota | `pet.is_summon(34003)` | `pet.is_summon(vnum)` |
| Servidor test | `is_test_server()` | — |
| Rango de nivel | `pc.level between 15, 39` | — (nueva, sintaxis amigable) |

**Decisiones abiertas (§11):** ¿`between` nativo o solo comparaciones compuestas? ¿necesitamos `get_qf(...) between a, b`?

---

## 5. Acciones — inventario del corpus real

| Acción | Sintaxis | Legacy |
|---|---|---|
| Diálogo título | `say_title(clave)` | `say_title(...)` |
| Diálogo | `say(clave)` | `say(...)` |
| Recompensa mostrada | `say_reward(clave)` | `say_reward(...)` |
| Item mostrado | `say_item_vnum(30006)` | `say_item_vnum(vnum)` |
| Enviar carta | `send_letter(clave)` | `send_letter(...)` |
| Limpiar carta | `clear_letter()` | `clear_letter()` |
| Esperar | `wait()` | `wait()` (coroutine yield → evento) |
| Cambiar estado | `set_state(nombre)` | `set_state(...)` |
| Quest externa | `set_quest_state(quest, estado)` | `set_quest_state(...)` |
| Flag de quest | `set_qf(nombre, valor)` | `pc.setqf("k", v)` |
| Dar item | `give_item2(vnum[, count])` | `pc.give_item2(...)` |
| Quitar item | `remove_item(vnum, count)` | `pc.remove_item(...)` |
| Marcar target | `target_vid(nombre, npc_vnum, clave)` | `target.vid(...)` |
| Borrar target | `target_delete(nombre)` | `target.delete(...)` |
| Teletransporte | `warp(x, y)` | `pc.warp(...)` |
| Aviso global | `notice(clave)` | `notice(...)` |
| Aviso multilínea | `notice_multiline(clave, notice_all)` | — |
| Afecto/buff | `affect_add(apply.MOV_SPEED, 10, segundos)` | `affect.add_collect(...)` |
| Quitar afecto | `affect_remove(...)` | — |
| Menú | `select(clave1, clave2...)` | `select(...)` (devuelve índice → requiere ramas: ver §10) |
| Input | `input_number(clave)` | `input_number(...)` |

**Catálogo completo:** las 982 entradas de `quest_functions` (inventario del API legacy) se auditan en la fase de conversión; solo las usadas por el corpus real se portan al DSL. El resto muere o pasa a Rust.

---

## 6. Familias de quest con parámetros

Elimina la repetición de quests casi-idénticas (caso real: 11 archivos `collect_quest_lv30..lv96` = misma quest con números distintos).

```quest
# quests/biology/collect_quest.family.quest
quest collect_quest family (level, mob, herb, drug)
  state start
    on login, levelup with pc.level >= (level)
      -> set_state(information)

  state information
    on letter
      -> send_letter(@100_sendLetter)

    on (mob).kill with number(1, 100) <= 5
      -> give_item2((herb), 1)

    on (drug).use with get_qf(duration) == 0
      -> remove_item((drug), 1)
      -> set_qf(duration, get_time() + 60 * 60 * 22)

# instancias (quests reales)
quest collect_quest_lv30 = collect_quest(level: 30, mob: 601, herb: 30006, drug: 71035)
quest collect_quest_lv40 = collect_quest(level: 40, mob: 602, herb: 30007, drug: 71036)
quest collect_quest_lv50 = collect_quest(level: 50, mob: 603, herb: 30008, drug: 71037)
```

- Parámetro: `(nombre)` en condiciones/acciones; `(nombre)` sin espacios.
- Claves de texto con parámetro: `@100_sendLetter` → el prefijo `@` indica clave de locale; el conversor la genera por nivel (la familia lleva su propio índice de claves).
- **El conversor automático detecta quests diff-casi-idénticas y las agrupa en familias** (heurística de similitud + confirmación humana).

---

## 7. Bloques reutilizables e imports

```quest
# quests/common/helpers.quest
block npc_target(npc: vnum, clave: key)
  -> target_vid(__TARGET__, (npc), (clave))

block reward_sequence(title, text, next_state)
  -> say_title((title))
  -> say((text))
  -> wait()
  -> set_state((next_state))
```

```quest
# quests/biology/collect_quest_lv30.quest
import helpers

state information
  on letter
    use npc_target(20084, @150_sayTitle)
    -> send_letter(@10_sendLetter)

  on 20084.chat
    use reward_sequence(@50_sayTitle, @40_say, go_to_disciple)
    -> set_qf(duration, 0)
```

- `block` y `use` solo componen acciones/condiciones tipadas — sin lógica libre.
- `import` permite compartir bloques entre quests y servir de librería base (`helpers.quest`).
- Validación: el parser resuelve bloques e imports en load-time; ciclo de import = error.

---

## 8. Casos especiales → Rust (no DSL)

Gestores de eventos con lógica real de coordinación (GM, flags globales, secuencias) — el corpus lo confirma con `oxevent.quest`, `christmas_*`, `oxevent_manager`, `game.set_event_flag`:

**Decisión:** estos se reimplementan como **módulos Rust del servidor** (con el mismo API de triggers/acciones disponible vía bindings nativos). El DSL no crece hacia un lenguaje general para acomodarlos.

---

## 9. Conversión automática del legacy (qc → DSL)

1. Parsear los 194 `.quest` con el parser de qc (extraer AST real del DSL legacy).
2. Traducir AST → DSL v2: triggers/condiciones/acciones mapeadas por tablas de equivalencia (¡las mismas del §3–5!).
3. Detección de familias (diff de ASTs) + agrupación propuesta.
4. Extracción de bloques repetidos (análisis de subárboles comunes).
5. **Harness de paridad:** ejecutar la misma quest en el servidor legacy (oráculo) y en el motor Rust con los mismos inputs simulados → mismo estado final y misma salida de diálogos. Es el mismo harness que valida el motor en F5.
6. Salida: `quests/` en DSL + informe de discrepancias + lista de quests que requieren revisión manual (las que no encajen en el DSL → Rust).

**Regla:** ninguna quest migrada se da por convertida sin pasar el harness de paridad.

---

## 10. Ramas y flujo dentro de un evento

El legacy usa `if/else` y `select(...)` con ramas dentro del cuerpo. El DSL declara ramas por evento:

```quest
on 20011.chat
  -> select(@_20_say, @_30_say) as choice
  if choice == 1
    -> warp(896500, 24600)
  else
    -> return

on 20011.chat with get_gm_level() == 5
  -> input_number(@_160_say) as amount
  if amount > 200
    -> say(@_250_say)
```

- `as <nombre>` captura el resultado; `if/else` ramifica solo sobre resultados capturados y condiciones simples.
- Sin bucles. Sin variables mutables fuera del alcance del evento. (El legacy las usa para contadores → en el DSL son `set_qf`/`get_qf`, persistidas.)

**Decisión abierta (§11):** ¿`if` anidado permitido (1 nivel) o `elif`? Proponemos 1 nivel + `else` para mantener la legibilidad.

---

## 11. Decisiones abiertas (para revisores)

1. `between a, b` en condiciones: ¿sintaxis nativa o solo comparaciones?
2. `if` dentro de eventos: ¿1 nivel + else (propuesto) o ilimitado?
3. ¿`select` con captura `as` cubre todos los usos del corpus, o hay menús anidados que obligan a reestructurar la quest?
4. ¿Claves de locale: `@clave` con tabla de claves por familia, o el literal directo?
5. Naming: `.quest` (continuidad) vs `.qdsl` vs `.mq` (metin quest).
6. ¿El `wait()` y temporizadores requieren un trigger `timer` explícito, o basta `on login ... with get_time() >= get_qf(...)`?

---

## 12. Fuera de alcance de este spec

- Motor de ejecución Rust (máquina de estados + scheduler de `wait()`) — diseño en fase F5.
- Harness de paridad (ejecución dual) — diseño en fase F0/F5.
- Validador CLI `quest-validate` y schema de editor — diseño tras cerrar este spec.
- Editor visual GUI — **excluido por decisión** (YAGNI; el validador + schema cubren el 90% del beneficio).
