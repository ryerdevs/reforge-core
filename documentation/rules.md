# Rules — Lo que no se repite

## 1. C++ congelado
**Regla:** `source/server` nunca se recompila (ADR-0012). Es oráculo.
**Por qué:** evita divergencia y pérdida de paridad. WSL solo on-demand.
**Si la rompes:** `git diff -- source/server` debe quedar vacío.

## 2. Encoding CP949
**Regla:** `locale/*.lua` con coreano usa CP949 (2 bytes), no UTF-8.
**Por qué:** `liblua/5.0/llex.c` lee 2 bytes si `b & 0x80`.
**Si la rompes:** `unfinished string` y `monster_chat` muerto.

## 3. Item_proto original
**Regla:** `item_proto.name` queda CP949 original.
**Por qué:** `etc_drop_item.txt` referencia por nombre CP949, el boot falla si lo traduces.
**Si la rompes:** `No such item` en boot.

## 4. WAL antes que PG
**Regla:** `WAL -> Batcher -> PG`, nunca SQL inline en world task.
**Por qué:** evita pérdida en crash y doble escritura.
**Si la rompes:** pérdida de oro/items.

## 5. Puertos y prefijos desechables
**Regla:** tests PG usan `m2_audit_*` y `%TEMP%\metin2-*`, nunca `5432/metín2`.
**Por qué:** protege prod.
**Si la rompes:** DDL en prod.

## 6. Docs vivos
**Regla:** `planning/progress.md` se actualiza al cierre de cada slice; `docs/CURRENT.md` archivado.
**Por qué:** handoff sin adivinar.
