-- ============================================================================
-- alter_gold_check.sql — G-PG follow-up (F3 tail lane, 2026-08-13)
--
-- Gap cerrado: "CHECK (gold >= 0) NO existe en PG" (verificado en pg_catalog
-- 2026-08-13; solo existian checks de window/attr/land/skill). El plan exige
-- "the DB guarantees" (principio 4): el guard Rust
-- `EconomyRepo::checked_gold_mutation` (database/src/economy.rs:97) se queda
-- como defense-in-depth, pero la constraint pertenece a PG.
--
-- Tablas con estado de ORO del jugador (wallet) — se les anade el CHECK:
--   1. player.player.gold   (monedero del personaje)
--   2. player.safebox.gold  (oro de la caja de seguridad)
--   3. player.guild.gold    (banco de la guild)
--
-- Verificacion previa (2026-08-13, psql): 0 filas negativas en las tres
-- (player.player 6 filas, player.safebox 0, player.guild 0) — el CHECK no
-- rompe ninguna fila existente.
--
-- EXCLUIDAS a proposito (columnas "gold" que NO son wallet):
--   - log.money_log.gold — log de AUDIT con delta FIRMADO: el legacy registra
--     gastos como negativos a proposito (char.cpp:7804 y char_item.cpp:927/952
--     `-dwPrice` refine, shop.cpp:395 `-dwPrice`, guild.cpp:1628 `-gold`).
--     Un CHECK (gold >= 0) aqui romperia la paridad del log. La validacion
--     real de ese log es de TIPO (`validate_money_log_type`, economy.rs:81).
--   - player.item_proto.gold / mob_proto.gold_min / gold_max — catalogos
--     estaticos de precios/drops, no estado del jugador (0 negativos hoy).
--
-- Re-runnable: PG no tiene `ADD CONSTRAINT IF NOT EXISTS`; se usa
-- DO $$ ... IF NOT EXISTS (pg_constraint) $$. Idempotente.
-- ============================================================================

-- 1. player.player.gold
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'player_gold_check'
          AND conrelid = 'player.player'::regclass
    ) THEN
        ALTER TABLE player.player
            ADD CONSTRAINT player_gold_check CHECK (gold >= 0);
    END IF;
END $$;

-- 2. player.safebox.gold
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'safebox_gold_check'
          AND conrelid = 'player.safebox'::regclass
    ) THEN
        ALTER TABLE player.safebox
            ADD CONSTRAINT safebox_gold_check CHECK (gold >= 0);
    END IF;
END $$;

-- 3. player.guild.gold
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'guild_gold_check'
          AND conrelid = 'player.guild'::regclass
    ) THEN
        ALTER TABLE player.guild
            ADD CONSTRAINT guild_gold_check CHECK (gold >= 0);
    END IF;
END $$;

-- Verificacion (psql):
--   SELECT conname, pg_get_constraintdef(oid)
--   FROM pg_constraint
--   WHERE conname IN ('player_gold_check','safebox_gold_check','guild_gold_check')
--   ORDER BY 1;
-- Rechazo (transaccion que debe fallar y hacer rollback):
--   BEGIN;
--   INSERT INTO player.safebox (account_id, size, password, gold)
--   VALUES (999999, 0, '', -5);
--   ROLLBACK;
--   BEGIN;
--   UPDATE player.player SET gold = -1 WHERE id = 1;
--   ROLLBACK;
