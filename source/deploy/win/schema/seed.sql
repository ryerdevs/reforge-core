-- Minimal Lawful Synthetic Development Seed for Reforge-Core (PostgreSQL 18)
--
-- Provides baseline records for accounts, character, inventory items,
-- protos, map definitions, and experience tables without proprietary data.

-- 1. Accounts
INSERT INTO account.account (id, login, password, social_id, email, status, lang, availDt, create_time)
VALUES
    (1, 'test', '*A4B6157319038724E3560894F7F932C8886EBFCF', '1234567', 'test@example.com', 'OK', 'en', NOW(), NOW()),
    (2, 'admin', '*4ACFE3202A5FF5CF467898FC58AAB1D615029441', '7654321', 'admin@example.com', 'OK', 'en', NOW(), NOW())
ON CONFLICT (id) DO UPDATE SET
    login = EXCLUDED.login,
    password = EXCLUDED.password,
    status = EXCLUDED.status;

-- 2. Player Index
INSERT INTO player.player_index (id, pid1, pid2, pid3, pid4, pid5, empire)
VALUES
    (1, 1, 0, 0, 0, 0, 1),
    (2, 2, 0, 0, 0, 0, 1)
ON CONFLICT (id) DO UPDATE SET
    pid1 = EXCLUDED.pid1,
    empire = EXCLUDED.empire;

-- 3. Characters
INSERT INTO player.player (
    id, account_id, name, job, voice, dir, x, y, z, map_index,
    exit_x, exit_y, exit_map_index, hp, mp, stamina, random_hp, random_sp,
    playtime, level, level_step, st, ht, dx, iq, exp, gold,
    stat_point, skill_point, sub_skill_point, stat_reset_count,
    part_main, part_base, part_hair, part_acce, skill_level, quickslot
)
VALUES
    (
        1, 1, 'Ryer', 0, 0, 0, 969600, 278400, 0, 1,
        969600, 278400, 1, 1000, 500, 1000, 0, 0,
        0, 1, 0, 4, 3, 3, 3, 0, 10000,
        0, 0, 0, 0,
        0, 0, 0, 0, '\x00'::bytea, '\x00'::bytea
    ),
    (
        2, 2, 'Admin', 0, 0, 0, 969600, 278400, 0, 1,
        969600, 278400, 1, 5000, 2000, 1000, 0, 0,
        0, 75, 0, 90, 90, 90, 90, 0, 5000000,
        0, 0, 0, 0,
        0, 0, 0, 0, '\x00'::bytea, '\x00'::bytea
    )
ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    level = EXCLUDED.level,
    x = EXCLUDED.x,
    y = EXCLUDED.y,
    gold = EXCLUDED.gold;

-- 4. Game Master Authorization
INSERT INTO common.gmlist (mID, mAccount, mName, mContactIP, mServerIP, mAuthority)
VALUES
    (1, 'admin', 'Admin', '', 'ALL', 'IMPLEMENTOR')
ON CONFLICT (mID) DO UPDATE SET
    mAuthority = EXCLUDED.mAuthority;

-- 5. Experience Table
INSERT INTO common.exp_table (level, exp)
SELECT
    lvl,
    (lvl * lvl * 150 + lvl * 500)::bigint
FROM generate_series(1, 120) AS lvl
ON CONFLICT (level) DO UPDATE SET
    exp = EXCLUDED.exp;

-- 6. Starter Items
INSERT INTO player.item (id, owner_id, "window", pos, count, vnum, socket0, socket1, socket2)
VALUES
    (1, 1, 'INVENTORY', 0, 1, 10, 0, 0, 0),
    (2, 1, 'INVENTORY', 1, 200, 27001, 0, 0, 0),
    (3, 1, 'INVENTORY', 2, 200, 27004, 0, 0, 0)
ON CONFLICT (id) DO UPDATE SET
    count = EXCLUDED.count,
    vnum = EXCLUDED.vnum;

-- 7. Minimal Item Protos
INSERT INTO player.item_proto (
    vnum, name, locale_name, type, subtype, weight, size, antiflag, flag,
    wearflag, immuneflag, gold, shop_buy_price, refined_vnum, refine_set, refine_set2,
    magic_pct, limittype0, limitvalue0, limittype1, limitvalue1,
    applytype0, applyvalue0, applytype1, applyvalue1, applytype2, applyvalue2,
    value0, value1, value2, value3, value4, value5, specular, socket_pct, addon_type
)
VALUES
    (10, 'Sword+0', 'Sword+0', 1, 0, 0, 2, 56, 1, 16, '', 0, 0, 11, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 15, 19, 0, 0, 0, 0),
    (27001, 'Potion Red (S)', 'Potion Red (S)', 3, 0, 0, 1, 0, 4, 0, '', 100, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 300, 0, 0, 0, 0, 0, 0, 0, 0),
    (27004, 'Potion Blue (S)', 'Potion Blue (S)', 3, 0, 0, 1, 0, 4, 0, '', 200, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 100, 0, 0, 0, 0, 0, 0, 0, 0)
ON CONFLICT (vnum) DO UPDATE SET
    locale_name = EXCLUDED.locale_name;

-- 8. Minimal Mob Protos
INSERT INTO player.mob_proto (
    vnum, name, locale_name, rank, type, battle_type, level, size, ai_flag,
    mount_capacity, setraceflag, setimmuneflag, empire, folder, on_click,
    st, dx, ht, iq, damage_min, damage_max, max_hp, regen_cycle, regen_percent,
    gold_min, gold_max, exp
)
VALUES
    (101, 'Wild Dog', convert_to('Wild Dog', 'UTF8'), 0, 0, 0, 1, 'SMALL', '', 0, '', '', 0, 'wild_dog', 0, 5, 5, 5, 5, 4, 8, 100, 3, 5, 10, 50, 35),
    (11000, 'Town Guard', convert_to('Town Guard', 'UTF8'), 5, 1, 0, 100, 'SMALL', '', 0, '', '', 1, 'guard', 2, 100, 100, 100, 100, 500, 800, 50000, 3, 10, 0, 0, 0)
ON CONFLICT (vnum) DO UPDATE SET
    locale_name = EXCLUDED.locale_name;

-- 9. World Maps
INSERT INTO world.maps (map_id, name, base_x, base_y, spawn_x, spawn_y)
VALUES
    (1, 'metin2_map_a1', 400000, 600000, 969600, 278400)
ON CONFLICT (map_id) DO UPDATE SET
    name = EXCLUDED.name;

-- 10. Multi-language Locale Strings
INSERT INTO common.item_names (vnum, lang, name)
VALUES
    (10, 'en', 'Sword+0'),
    (10, 'es', 'Espada+0'),
    (27001, 'en', 'Red Potion (S)'),
    (27001, 'es', 'Poción Roja (P)'),
    (27004, 'en', 'Blue Potion (S)'),
    (27004, 'es', 'Poción Azul (P)')
ON CONFLICT (vnum, lang) DO UPDATE SET
    name = EXCLUDED.name;

INSERT INTO common.mob_names (vnum, lang, name)
VALUES
    (101, 'en', 'Wild Dog'),
    (101, 'es', 'Perro Salvaje'),
    (11000, 'en', 'Town Guard'),
    (11000, 'es', 'Guarda de la Aldea')
ON CONFLICT (vnum, lang) DO UPDATE SET
    name = EXCLUDED.name;

INSERT INTO common.map_names (map_id, lang, name)
VALUES
    (1, 'en', 'Village 1 (Shinsoo)'),
    (1, 'es', 'Aldea 1 (Shinsoo)')
ON CONFLICT (map_id, lang) DO UPDATE SET
    name = EXCLUDED.name;
