-- F1: server-side locale + maps/spawns schema (ADR-0009 + plan locale-redesign)
-- Applied 2026-08-11. Rows per language (new language = INSERTs, never ALTER).

-- Locale tables (schema common)
CREATE TABLE IF NOT EXISTS common.mob_names (
    vnum bigint NOT NULL,
    lang varchar(4) NOT NULL,
    name text NOT NULL,
    PRIMARY KEY (vnum, lang)
);

CREATE TABLE IF NOT EXISTS common.item_names (
    vnum bigint NOT NULL,
    lang varchar(4) NOT NULL,
    name text NOT NULL,
    PRIMARY KEY (vnum, lang)
);

CREATE TABLE IF NOT EXISTS common.item_descriptions (
    vnum bigint NOT NULL,
    lang varchar(4) NOT NULL,
    text text NOT NULL,
    PRIMARY KEY (vnum, lang)
);

CREATE TABLE IF NOT EXISTS common.skill_names (
    skill_id int NOT NULL,
    lang varchar(4) NOT NULL,
    name text NOT NULL,
    PRIMARY KEY (skill_id, lang)
);

CREATE TABLE IF NOT EXISTS common.map_names (
    map_id int NOT NULL,
    lang varchar(4) NOT NULL,
    name text NOT NULL,
    PRIMARY KEY (map_id, lang)
);

CREATE TABLE IF NOT EXISTS common.ui_texts (
    key text NOT NULL,
    lang varchar(4) NOT NULL,
    value text NOT NULL,
    PRIMARY KEY (key, lang)
);

-- Server-side only (chat/notices); never sent to the client.
CREATE TABLE IF NOT EXISTS common.message_texts (
    key text NOT NULL,
    lang varchar(4) NOT NULL,
    value text NOT NULL,
    PRIMARY KEY (key, lang)
);

-- Panel-web only: PNG 32x32, one row per group of 10 vnums; client never reads it.
CREATE TABLE IF NOT EXISTS common.item_icons (
    vnum bigint PRIMARY KEY,
    png bytea NOT NULL
);

-- Maps & spawns (schema world)
CREATE TABLE IF NOT EXISTS world.maps (
    map_id int PRIMARY KEY,
    name text NOT NULL,
    base_x int NOT NULL,
    base_y int NOT NULL,
    spawn_x int NOT NULL,
    spawn_y int NOT NULL
);

-- Expanded spawns (groups already resolved), coordinates in UNITS.
CREATE TABLE IF NOT EXISTS world.spawns (
    map_id int NOT NULL,
    vnum bigint NOT NULL,
    x int NOT NULL,
    y int NOT NULL,
    count int NOT NULL,
    kind text NOT NULL
);
CREATE INDEX IF NOT EXISTS spawns_map_idx ON world.spawns (map_id);
