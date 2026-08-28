-- Phase dragon_soul (2026-08-28): ledger de refinamientos de Dragon Soul.
-- Tabla ADITIVA del reforge (el legacy NO la tiene: el estado del alma vive
-- en el vnum del item + sockets; el ledger es el registro durable de cada
-- CG_DRAGON_SOUL_REFINE, patron append-only de log.money_log, F3 tail
-- ACID). El id lo asigna la IDENTITY de PG (player.dragon_soul_id_seq —
-- leccion land: nunca un contador de proceso). Idempotente.
CREATE TABLE IF NOT EXISTS player.dragon_soul (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    player_id   bigint NOT NULL,
    refine_type smallint NOT NULL,
    applied_at  timestamptz NOT NULL DEFAULT now()
);